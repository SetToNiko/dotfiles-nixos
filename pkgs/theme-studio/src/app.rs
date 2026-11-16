//! Editor state and input handling.

use std::io::Write;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;

use crate::color::{self, Rgb};
use crate::palette::{self, GROUPS, Palette};
use crate::store::{Store, TerminalMap};

/// Which control the keyboard drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    List,
    Square,
    Hue,
    SliderH,
    SliderS,
    SliderL,
    Hex,
}

const FOCUS_ORDER: [Focus; 7] = [
    Focus::List, Focus::Square, Focus::Hue, Focus::SliderH, Focus::SliderS, Focus::SliderL, Focus::Hex,
];

/// Something that would throw away unsaved changes, waiting for confirmation.
#[derive(Clone, Debug)]
pub enum Pending {
    Quit,
    New,
    Open(String),
}

pub enum Overlay {
    None,
    Open { filter: String, sel: usize, entries: Vec<(String, Vec<Rgb>)> },
    SaveAs { input: String, overwrite: bool, then_apply: bool },
    Confirm(Pending),
    /// Picking the key whose colour gets copied into `target`.
    CopyFrom { target: &'static str },
    Help,
}

/// Screen areas from the last draw, for mouse hit-testing.
#[derive(Default)]
pub struct Areas {
    pub list: Rect,
    /// First list row shown, and which key (if any) each visible row is.
    pub list_rows: Vec<Option<usize>>,
    pub square: Rect,
    pub hue: Rect,
    pub slider_h: Rect,
    pub slider_s: Rect,
    pub slider_l: Rect,
    pub hex: Rect,
}

pub struct App {
    pub store: Store,
    pub name: String,
    /// Not in palettes.json yet.
    pub is_new: bool,
    pub palette: Palette,
    /// The palette as last loaded or saved, for "unsaved" and the old swatch.
    pub saved: Palette,
    undo: Vec<Palette>,
    redo: Vec<Palette>,
    /// Key being edited by consecutive picker changes (one undo step for all).
    last_edit: Option<&'static str>,
    /// Keys in display order (GROUPS flattened).
    pub keys: Vec<&'static str>,
    pub sel: usize,
    pub list_scroll: usize,
    pub focus: Focus,
    /// Picker state: hue (degrees), saturation and value (0..1). Kept as
    /// floats so the hue survives greys and small steps don't get rounded away.
    pub hsv: (f64, f64, f64),
    pub hex_input: String,
    pub active: Option<String>,
    pub terminal: Option<TerminalMap>,
    /// Status-line message and whether it's an error.
    pub message: Option<(String, bool)>,
    pub overlay: Overlay,
    pub areas: Areas,
    quit: bool,
}

pub enum Start {
    Active,
    Named(String),
    New,
}

impl App {
    pub fn new(store: Store, start: Start) -> Result<App> {
        let active = store.active_name();
        let (name, palette, is_new) = match start {
            Start::Named(n) => {
                let p = store.load(&n)?;
                (n, p, false)
            }
            Start::Active => match active.as_deref().map(|n| (n, store.load(n))) {
                Some((n, Ok(p))) => (n.to_string(), p, false),
                _ => ("untitled".to_string(), Palette::blank(), true),
            },
            Start::New => ("untitled".to_string(), Palette::blank(), true),
        };
        let terminal = store.terminal_map().ok();
        let mut app = App {
            store,
            name,
            is_new,
            saved: palette.clone(),
            palette,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            keys: GROUPS.iter().flat_map(|(_, ks)| ks.iter().copied()).collect(),
            sel: 0,
            list_scroll: 0,
            focus: Focus::List,
            hsv: (0.0, 0.0, 0.0),
            hex_input: String::new(),
            active,
            terminal,
            message: None,
            overlay: Overlay::None,
            areas: Areas::default(),
            quit: false,
        };
        app.load_picker();
        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.quit {
            terminal.draw(|f| crate::ui::draw(f, self))?;
            match event::read()? {
                Event::Key(k) if k.kind == KeyEventKind::Press => self.on_key(k),
                Event::Mouse(m) => self.on_mouse(m),
                Event::Paste(s) => self.on_paste(&s),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn key(&self) -> &'static str {
        self.keys[self.sel]
    }

    pub fn dirty(&self) -> bool {
        self.is_new || self.palette != self.saved
    }

    fn info(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), false));
    }

    fn error(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), true));
    }

    // ----- editing ---------------------------------------------------------

    /// Points the picker at the selected key's colour.
    fn load_picker(&mut self) {
        let c = self.palette.get(self.key());
        let (h, s, v) = color::rgb_to_hsv(c);
        self.hsv = (h.unwrap_or(self.hsv.0), s, v);
        self.hex_input = c.hex();
    }

    fn select(&mut self, i: usize) {
        if i < self.keys.len() && i != self.sel {
            self.sel = i;
            self.last_edit = None;
            self.load_picker();
        }
    }

    fn push_undo(&mut self) {
        self.undo.push(self.palette.clone());
        if self.undo.len() > 500 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Sets the selected key from the picker. Consecutive changes to one key
    /// are a single undo step.
    fn edit(&mut self, c: Rgb) {
        let key = self.key();
        if self.palette.get(key) == c {
            return;
        }
        if self.last_edit != Some(key) {
            self.push_undo();
            self.last_edit = Some(key);
        }
        self.palette.set(key, c);
        self.hex_input = c.hex();
    }

    /// Sets `key` to `c` as its own undo step (paste, copy from another key).
    fn set_key(&mut self, key: &'static str, c: Rgb) {
        self.push_undo();
        self.last_edit = None;
        self.palette.set(key, c);
        if key == self.key() {
            self.load_picker();
        }
    }

    fn set_hsv(&mut self, h: f64, s: f64, v: f64) {
        self.hsv = (h.clamp(0.0, 360.0), s.clamp(0.0, 1.0), v.clamp(0.0, 1.0));
        let (h, s, v) = self.hsv;
        self.edit(color::hsv_to_rgb(h, s, v));
    }

    /// Current saturation and lightness (HSL) of the selected colour.
    pub fn hsl(&self) -> (f64, f64) {
        let (_, s, l) = color::rgb_to_hsl(self.palette.get(self.key()));
        (s, l)
    }

    fn set_hsl(&mut self, s: f64, l: f64) {
        let c = color::hsl_to_rgb(self.hsv.0, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
        let (_, sv, v) = color::rgb_to_hsv(c);
        self.hsv.1 = sv;
        self.hsv.2 = v;
        self.edit(c);
    }

    fn toggle_pin(&mut self) {
        let key = self.key();
        if palette::is_base(key) {
            self.info(format!("{key} is a base colour; it's always set by hand"));
            return;
        }
        self.push_undo();
        self.last_edit = None;
        if self.palette.is_pinned(key) {
            self.palette.unpin(key);
            self.info(format!("{key} follows {} again", palette::sources(key)));
        } else {
            let c = self.palette.get(key);
            self.palette.set(key, c);
            self.info(format!("{key} pinned"));
        }
        self.load_picker();
    }

    fn undo(&mut self) {
        match self.undo.pop() {
            Some(p) => {
                self.redo.push(std::mem::replace(&mut self.palette, p));
                self.last_edit = None;
                self.load_picker();
            }
            None => self.info("Nothing to undo"),
        }
    }

    fn redo(&mut self) {
        match self.redo.pop() {
            Some(p) => {
                self.undo.push(std::mem::replace(&mut self.palette, p));
                self.last_edit = None;
                self.load_picker();
            }
            None => self.info("Nothing to redo"),
        }
    }

    /// Copies text to the system clipboard with OSC 52 (Ghostty supports it).
    fn copy(&mut self, text: &str) {
        let seq = format!("\x1b]52;c;{}\x07", base64(text.as_bytes()));
        let mut out = std::io::stdout();
        let _ = out.write_all(seq.as_bytes()).and_then(|_| out.flush());
        self.info(format!("Copied {text}"));
    }

    // ----- files -----------------------------------------------------------

    fn save(&mut self, then_apply: bool) {
        if self.is_new {
            self.overlay = Overlay::SaveAs { input: String::new(), overwrite: false, then_apply };
            return;
        }
        let name = self.name.clone();
        self.save_as(&name, then_apply);
    }

    fn save_as(&mut self, name: &str, then_apply: bool) {
        match self.store.save(name, &self.palette) {
            Ok(()) => {
                self.name = name.to_string();
                self.is_new = false;
                self.saved = self.palette.clone();
                if then_apply {
                    self.apply_saved();
                } else {
                    self.info(format!("Saved \"{name}\" to palettes.json"));
                }
            }
            Err(e) => self.error(format!("Save failed: {e:#}")),
        }
    }

    /// Saves if needed, then makes this palette the system theme.
    fn apply(&mut self) {
        if self.dirty() {
            self.save(true);
        } else {
            self.apply_saved();
        }
    }

    fn apply_saved(&mut self) {
        let name = self.name.clone();
        match self.store.apply(&name) {
            Ok(()) => {
                self.active = Some(name.clone());
                self.info(format!("Saved and applied \"{name}\""));
            }
            Err(e) => self.error(format!("Apply failed: {e:#}")),
        }
    }

    fn open(&mut self, name: &str) {
        match self.store.load(name) {
            Ok(p) => {
                self.name = name.to_string();
                self.is_new = false;
                self.saved = p.clone();
                self.palette = p;
                self.reset_history();
                self.info(format!("Opened \"{name}\""));
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    fn new_palette(&mut self) {
        self.name = "untitled".to_string();
        self.is_new = true;
        self.palette = Palette::blank();
        self.saved = self.palette.clone();
        self.reset_history();
        self.info("New palette: edit the 8 base colours first, the rest follow");
    }

    fn reset_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        self.sel = 0;
        self.list_scroll = 0;
        self.focus = Focus::List;
        self.load_picker();
    }

    /// Runs `p` now, or asks first when there are unsaved changes.
    fn guard(&mut self, p: Pending) {
        if self.dirty() {
            self.overlay = Overlay::Confirm(p);
        } else {
            self.run_pending(p);
        }
    }

    fn run_pending(&mut self, p: Pending) {
        self.overlay = Overlay::None;
        match p {
            Pending::Quit => self.quit = true,
            Pending::New => self.new_palette(),
            Pending::Open(n) => self.open(&n),
        }
    }

    fn open_overlay(&mut self) {
        match self.store.load_all() {
            Ok(all) => {
                let entries = all
                    .iter()
                    .map(|(name, v)| {
                        let sw = ["bg0", "fg0", "red", "orange", "yellow", "green", "aqua", "blue", "purple"]
                            .iter()
                            .filter_map(|k| v.get(*k).and_then(|x| x.as_str()).and_then(Rgb::parse))
                            .collect();
                        (name.clone(), sw)
                    })
                    .collect::<Vec<_>>();
                let sel = entries.iter().position(|(n, _)| *n == self.name).unwrap_or(0);
                self.overlay = Overlay::Open { filter: String::new(), sel, entries };
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    // ----- input -----------------------------------------------------------

    fn on_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.guard(Pending::Quit);
            return;
        }
        if !matches!(self.overlay, Overlay::None) {
            self.on_overlay_key(k);
            return;
        }
        self.message = None;
        if self.focus == Focus::Hex {
            self.on_hex_key(k);
            return;
        }

        // Keys that work everywhere
        match k.code {
            KeyCode::Char('q') => return self.guard(Pending::Quit),
            KeyCode::Char('s') => return self.save(false),
            KeyCode::Char('S') => {
                self.overlay = Overlay::SaveAs { input: self.name.clone(), overwrite: false, then_apply: false };
                return;
            }
            KeyCode::Char('a') => return self.apply(),
            KeyCode::Char('o') => return self.open_overlay(),
            KeyCode::Char('n') => return self.guard(Pending::New),
            KeyCode::Char('u') => return self.undo(),
            KeyCode::Char('r') if ctrl => return self.redo(),
            KeyCode::Char('p') => return self.toggle_pin(),
            KeyCode::Char('y') => {
                let hex = self.palette.get(self.key()).hex();
                return self.copy(&hex);
            }
            KeyCode::Char('c') => {
                self.overlay = Overlay::CopyFrom { target: self.key() };
                self.focus = Focus::List;
                return;
            }
            KeyCode::Char('?') => {
                self.overlay = Overlay::Help;
                return;
            }
            KeyCode::Char('#') => {
                self.focus = Focus::Hex;
                self.hex_input = "#".into();
                return;
            }
            KeyCode::Tab => return self.cycle_focus(1),
            KeyCode::BackTab => return self.cycle_focus(-1),
            KeyCode::Esc => {
                self.focus = Focus::List;
                return;
            }
            _ => {}
        }

        let big = k.modifiers.contains(KeyModifiers::SHIFT) || k.code.as_char().is_some_and(|c| c.is_ascii_uppercase());
        let dir = match k.code {
            KeyCode::Char('h' | 'H') | KeyCode::Left => Some((-1, 0)),
            KeyCode::Char('l' | 'L') | KeyCode::Right => Some((1, 0)),
            KeyCode::Char('k' | 'K') | KeyCode::Up => Some((0, -1)),
            KeyCode::Char('j' | 'J') | KeyCode::Down => Some((0, 1)),
            _ => None,
        };
        match self.focus {
            Focus::List => match k.code {
                KeyCode::Enter => self.focus = Focus::Square,
                KeyCode::Char('g') | KeyCode::Home => self.select(0),
                KeyCode::Char('G') | KeyCode::End => self.select(self.keys.len() - 1),
                KeyCode::PageDown => self.select((self.sel + 10).min(self.keys.len() - 1)),
                KeyCode::PageUp => self.select(self.sel.saturating_sub(10)),
                _ => match dir {
                    Some((0, dy)) => self.move_sel(dy),
                    Some((1, 0)) => self.focus = Focus::Square,
                    _ => {}
                },
            },
            Focus::Square => {
                if let Some((dx, dy)) = dir {
                    let step = if big { 0.1 } else { 0.02 };
                    let (h, s, v) = self.hsv;
                    self.set_hsv(h, s + dx as f64 * step, v - dy as f64 * step);
                }
            }
            Focus::Hue => {
                if let Some((dx, dy)) = dir {
                    let step = if big { 15.0 } else { 2.0 };
                    let (h, s, v) = self.hsv;
                    self.set_hsv(h + (dy + dx) as f64 * step, s, v);
                }
            }
            Focus::SliderH | Focus::SliderS | Focus::SliderL => match dir {
                Some((dx, 0)) => {
                    let n = dx as f64 * if big { 10.0 } else { 1.0 };
                    let (s, l) = self.hsl();
                    match self.focus {
                        Focus::SliderH => {
                            let (h, s, v) = self.hsv;
                            self.set_hsv(h + n, s, v);
                        }
                        Focus::SliderS => self.set_hsl(s + n / 100.0, l),
                        _ => self.set_hsl(s, l + n / 100.0),
                    }
                }
                Some((0, dy)) => self.cycle_focus(dy),
                _ => {}
            },
            Focus::Hex => {}
        }
    }

    fn on_hex_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Enter => match Rgb::parse(&self.hex_input) {
                Some(c) => {
                    self.last_edit = None;
                    self.edit(c);
                    self.load_picker();
                    self.focus = Focus::List;
                }
                None => self.error(format!("Not a colour: {} (use #rrggbb)", self.hex_input)),
            },
            KeyCode::Esc => {
                self.load_picker();
                self.focus = Focus::List;
            }
            KeyCode::Tab => self.cycle_focus(1),
            KeyCode::BackTab => self.cycle_focus(-1),
            KeyCode::Backspace => {
                self.hex_input.pop();
            }
            KeyCode::Char(c) if c.is_ascii_hexdigit() || c == '#' => {
                if c == '#' {
                    self.hex_input.clear();
                }
                if self.hex_input.len() < 7 {
                    if self.hex_input.is_empty() && c != '#' {
                        self.hex_input.push('#');
                    }
                    self.hex_input.push(c.to_ascii_lowercase());
                }
            }
            _ => {}
        }
    }

    fn on_overlay_key(&mut self, k: KeyEvent) {
        let overlay = std::mem::replace(&mut self.overlay, Overlay::None);
        self.overlay = match overlay {
            Overlay::Help => Overlay::None,
            Overlay::Confirm(p) => match k.code {
                KeyCode::Char('y') => {
                    self.run_pending(p);
                    Overlay::None
                }
                KeyCode::Char('s') if !self.is_new => {
                    let name = self.name.clone();
                    self.save_as(&name, false);
                    self.run_pending(p);
                    Overlay::None
                }
                KeyCode::Esc | KeyCode::Char('n') => Overlay::None,
                _ => Overlay::Confirm(p),
            },
            Overlay::CopyFrom { target } => match k.code {
                KeyCode::Enter => {
                    let from = self.key();
                    let c = self.palette.get(from);
                    let to = self.keys.iter().position(|k| *k == target).unwrap_or(0);
                    self.sel = to;
                    self.set_key(target, c);
                    self.info(format!("{target} = {from} ({})", c.hex()));
                    Overlay::None
                }
                KeyCode::Esc => Overlay::None,
                KeyCode::Char('j') | KeyCode::Down => {
                    self.move_sel(1);
                    Overlay::CopyFrom { target }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.move_sel(-1);
                    Overlay::CopyFrom { target }
                }
                _ => Overlay::CopyFrom { target },
            },
            Overlay::SaveAs { mut input, overwrite, then_apply } => match k.code {
                KeyCode::Esc => Overlay::None,
                KeyCode::Enter => {
                    let name = input.trim().to_string();
                    let exists = self.store.load_all().map(|a| a.contains_key(&name)).unwrap_or(false);
                    if name.is_empty() {
                        Overlay::SaveAs { input, overwrite, then_apply }
                    } else if exists && name != self.name && !overwrite {
                        self.error(format!("\"{name}\" exists. Enter again to overwrite it"));
                        Overlay::SaveAs { input, overwrite: true, then_apply }
                    } else {
                        self.save_as(&name, then_apply);
                        Overlay::None
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    Overlay::SaveAs { input, overwrite: false, then_apply }
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    Overlay::SaveAs { input, overwrite: false, then_apply }
                }
                _ => Overlay::SaveAs { input, overwrite, then_apply },
            },
            Overlay::Open { mut filter, mut sel, entries } => {
                let n = open_matches(&entries, &filter).len();
                match k.code {
                    KeyCode::Esc => Overlay::None,
                    KeyCode::Enter => match open_matches(&entries, &filter).get(sel) {
                        Some(&i) => {
                            let name = entries[i].0.clone();
                            self.guard(Pending::Open(name));
                            // guard() may have opened a confirmation instead
                            std::mem::replace(&mut self.overlay, Overlay::None)
                        }
                        None => Overlay::Open { filter, sel, entries },
                    },
                    KeyCode::Down => {
                        sel = (sel + 1).min(n.saturating_sub(1));
                        Overlay::Open { filter, sel, entries }
                    }
                    KeyCode::Up => {
                        sel = sel.saturating_sub(1);
                        Overlay::Open { filter, sel, entries }
                    }
                    KeyCode::Char('n') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        sel = (sel + 1).min(n.saturating_sub(1));
                        Overlay::Open { filter, sel, entries }
                    }
                    KeyCode::Char('p') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        sel = sel.saturating_sub(1);
                        Overlay::Open { filter, sel, entries }
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                        Overlay::Open { filter, sel: 0, entries }
                    }
                    KeyCode::Char(c) => {
                        filter.push(c);
                        Overlay::Open { filter, sel: 0, entries }
                    }
                    _ => Overlay::Open { filter, sel, entries },
                }
            }
            Overlay::None => Overlay::None,
        };
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        if !matches!(self.overlay, Overlay::None | Overlay::CopyFrom { .. }) {
            return;
        }
        let (x, y) = (m.column, m.row);
        let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        let frac = |r: Rect, v: u16, len: u16| {
            if len <= 1 { 0.0 } else { (v.saturating_sub(r.x.min(v)) as f64 / (len - 1) as f64).clamp(0.0, 1.0) }
        };
        match m.kind {
            MouseEventKind::ScrollDown if inside(self.areas.list) => self.move_sel(1),
            MouseEventKind::ScrollUp if inside(self.areas.list) => self.move_sel(-1),
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                let a = &self.areas;
                let (sq, hue, sh, ss, sl, hex, list) = (a.square, a.hue, a.slider_h, a.slider_s, a.slider_l, a.hex, a.list);
                if inside(list) {
                    if let Some(Some(i)) = self.areas.list_rows.get((y - list.y) as usize).copied() {
                        self.select(i);
                    }
                    if matches!(self.overlay, Overlay::None) {
                        self.focus = Focus::List;
                    }
                } else if inside(sq) {
                    self.focus = Focus::Square;
                    let s = frac(sq, x, sq.width);
                    let v = 1.0 - ((y - sq.y) as f64 / (sq.height.max(2) - 1) as f64).clamp(0.0, 1.0);
                    self.set_hsv(self.hsv.0, s, v);
                } else if inside(hue) {
                    self.focus = Focus::Hue;
                    let t = ((y - hue.y) as f64 / (hue.height.max(2) - 1) as f64).clamp(0.0, 1.0);
                    let (_, s, v) = self.hsv;
                    self.set_hsv(t * 360.0, s, v);
                } else if inside(sh) {
                    self.focus = Focus::SliderH;
                    let (_, s, v) = self.hsv;
                    self.set_hsv(frac(sh, x, sh.width) * 360.0, s, v);
                } else if inside(ss) {
                    self.focus = Focus::SliderS;
                    let (_, l) = self.hsl();
                    self.set_hsl(frac(ss, x, ss.width), l);
                } else if inside(sl) {
                    self.focus = Focus::SliderL;
                    let (s, _) = self.hsl();
                    self.set_hsl(s, frac(sl, x, sl.width));
                } else if inside(hex) {
                    self.focus = Focus::Hex;
                    self.hex_input = "#".into();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => self.last_edit = None,
            _ => {}
        }
    }

    /// Bracketed paste (Ctrl+Shift+V): a hex colour sets the selected key.
    fn on_paste(&mut self, s: &str) {
        match (&mut self.overlay, Rgb::parse(s)) {
            (Overlay::SaveAs { input, .. }, _) => input.push_str(s.trim()),
            (Overlay::Open { filter, sel, .. }, _) => {
                filter.push_str(s.trim());
                *sel = 0;
            }
            (Overlay::None, Some(c)) => {
                let key = self.key();
                self.set_key(key, c);
                self.info(format!("{key} = {}", c.hex()));
            }
            (Overlay::None, None) => self.error(format!("Pasted text isn't a colour: {}", s.trim())),
            _ => {}
        }
    }

    fn move_sel(&mut self, d: i32) {
        let i = (self.sel as i32 + d).clamp(0, self.keys.len() as i32 - 1) as usize;
        self.select(i);
    }

    fn cycle_focus(&mut self, d: i32) {
        let i = FOCUS_ORDER.iter().position(|f| *f == self.focus).unwrap_or(0) as i32;
        let n = FOCUS_ORDER.len() as i32;
        self.focus = FOCUS_ORDER[(i + d).rem_euclid(n) as usize];
        if self.focus == Focus::Hex {
            self.hex_input = "#".into(); // typing replaces the colour; Esc restores it
        }
    }
}

/// Indices of entries whose name contains `filter` (case-insensitive).
pub fn open_matches(entries: &[(String, Vec<Rgb>)], filter: &str) -> Vec<usize> {
    let f = filter.to_lowercase();
    entries
        .iter()
        .enumerate()
        .filter(|(_, (n, _))| n.to_lowercase().contains(&f))
        .map(|(i, _)| i)
        .collect()
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::store::to_jq_string;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use serde_json::{Map, Value};

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b"#9f6434"), "IzlmNjQzNA==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
    }

    /// A store in a temp folder with palettes "One" and "Two".
    fn temp_store(tag: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("theme-studio-app-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut two = Palette::blank();
        two.set("red", Rgb(200, 0, 0));
        let mut m = Map::new();
        m.insert("One".into(), Value::Object(Palette::blank().to_json()));
        m.insert("Two".into(), Value::Object(two.to_json()));
        std::fs::write(dir.join("palettes.json"), to_jq_string(&Value::Object(m))).unwrap();
        Store { dir }
    }

    fn press(app: &mut App, keys: &str) {
        for c in keys.chars() {
            let code = match c {
                '\n' => KeyCode::Enter,
                '\t' => KeyCode::Tab,
                '\x1b' => KeyCode::Esc,
                c => KeyCode::Char(c),
            };
            app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| crate::ui::draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol().to_string()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn picker_edits_follow_through_derived_keys_and_undo() {
        let mut app = App::new(temp_store("edit"), Start::Named("One".into())).unwrap();
        assert_eq!(app.key(), "bg0");
        let grey_before = app.palette.get("grey1");
        // bg0: into the square, raise value a lot
        press(&mut app, "\nKKK");
        let bg = app.palette.get("bg0");
        assert_ne!(bg, Palette::blank().get("bg0"));
        assert_ne!(app.palette.get("grey1"), grey_before, "grey1 follows bg0");
        assert!(app.dirty());
        // one undo step for the whole run of picker moves
        press(&mut app, "u");
        assert_eq!(app.palette, Palette::blank());
        press(&mut app, "\x18"); // not a binding: ignored
        app.on_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert_eq!(app.palette.get("bg0"), bg);
    }

    #[test]
    fn pinning_hex_entry_paste_and_copy_from() {
        let mut app = App::new(temp_store("pin"), Start::Named("One".into())).unwrap();
        // select grey1 (in the Greys group) and type a hex colour
        let i = app.keys.iter().position(|k| *k == "grey1").unwrap();
        app.select(i);
        press(&mut app, "#123456\n");
        assert_eq!(app.palette.get("grey1"), Rgb(0x12, 0x34, 0x56));
        assert!(app.palette.is_pinned("grey1"));
        // a pinned key ignores base changes; p lets it follow again
        app.select(0);
        app.on_paste("#000000");
        assert_eq!(app.palette.get("bg0"), Rgb(0, 0, 0));
        assert_eq!(app.palette.get("grey1"), Rgb(0x12, 0x34, 0x56));
        app.select(i);
        press(&mut app, "p");
        assert!(!app.palette.is_pinned("grey1"));
        assert_ne!(app.palette.get("grey1"), Rgb(0x12, 0x34, 0x56));
        // copy red into grey1: c, move to red, Enter
        press(&mut app, "c");
        let red = app.keys.iter().position(|k| *k == "red").unwrap();
        while app.sel > red {
            press(&mut app, "k");
        }
        press(&mut app, "\n");
        assert_eq!(app.key(), "grey1");
        assert_eq!(app.palette.get("grey1"), app.palette.get("red"));
    }

    #[test]
    fn save_open_and_save_as() {
        let store = temp_store("save");
        let dir = store.dir.clone();
        let mut app = App::new(store, Start::Named("One".into())).unwrap();
        press(&mut app, "#abcdef\n");
        press(&mut app, "s");
        assert!(!app.dirty());
        assert_eq!(Store { dir: dir.clone() }.load("One").unwrap().get("bg0"), Rgb(0xab, 0xcd, 0xef));
        // open "Two" through the search box
        press(&mut app, "oTwo\n");
        assert_eq!(app.name, "Two");
        assert_eq!(app.palette.get("red"), Rgb(200, 0, 0));
        // unsaved changes ask before opening another palette
        press(&mut app, "#111111\n");
        press(&mut app, "oOne\n");
        assert!(matches!(app.overlay, Overlay::Confirm(Pending::Open(_))));
        press(&mut app, "\x1b");
        assert_eq!(app.name, "Two");
        // save as a new name; it lands at the end of the file
        press(&mut app, "S");
        for _ in 0..3 {
            app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        press(&mut app, "Three\n");
        let names: Vec<String> = Store { dir }.load_all().unwrap().keys().cloned().collect();
        assert_eq!(names, ["One", "Two", "Three"]);
        assert_eq!(app.name, "Three");
    }

    #[test]
    fn renders_at_full_size_and_warns_when_small() {
        let mut app = App::new(temp_store("draw"), Start::Named("One".into())).unwrap();
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("Palette: One"));
        assert!(screen.contains("Backgrounds"));
        // the list scrolls to keep the selection visible
        press(&mut app, "G");
        assert!(render(&mut app, 160, 45).contains("bg_diff_blue"));
        press(&mut app, "g");
        assert!(screen.contains("hex #1e1e22"));
        // contrast column fits: e.g. "fg0 ... 11.8:1"
        let fg_row = screen.lines().find(|l| l.contains(" fg0 ")).unwrap();
        assert!(fg_row.contains(":1"), "contrast cut off: {fg_row}");
        press(&mut app, "o?");
        let small = render(&mut app, 80, 20);
        assert!(small.contains("needs at least"));
        // every pop-up draws without panicking
        for keys in ["\x1b?", "\x1bS", "\x1b\x1bc", "\x1b"] {
            press(&mut app, keys);
            render(&mut app, 160, 45);
        }
    }
}
