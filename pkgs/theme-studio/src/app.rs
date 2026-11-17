//! Editor state and input handling.

use std::io::Write;
use std::time::Duration;

use anyhow::Result;
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::Rect;

use crate::color::{self, Rgb};
use crate::fonts::FontsTab;
use crate::keymap::{Action, Context, Key, Keymap, Lookup};
use crate::palette::{self, GROUPS, Palette};
use crate::preview::{Mode as PreviewMode, PaneId, Preview};
use crate::store::{Store, TerminalMap};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::StatefulProtocol;

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
    Focus::List,
    Focus::Square,
    Focus::Hue,
    Focus::SliderH,
    Focus::SliderS,
    Focus::SliderL,
    Focus::Hex,
];

/// Something that would throw away unsaved changes, waiting for confirmation.
#[derive(Clone, Debug)]
pub enum Pending {
    Quit,
    New,
    Open(String),
    NewFonts,
    OpenFonts(String),
}

impl Pending {
    fn is_fonts(&self) -> bool {
        matches!(self, Pending::NewFonts | Pending::OpenFonts(_))
    }
}

/// Which column has the keyboard (both are always on screen).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Colours,
    Fonts,
}

/// F1 / F2 / F3.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Main,
    Downloads,
    Settings,
}

/// A row of the colour list.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ColourRow {
    Group { index: usize, expanded: bool },
    Key(usize),
}

pub enum Overlay {
    None,
    Open {
        filter: String,
        sel: usize,
        entries: Vec<(String, Vec<Rgb>)>,
    },
    SaveAs {
        input: String,
        overwrite: bool,
        then_apply: bool,
    },
    Confirm(Pending),
    /// Picking the key whose colour gets copied into `target`.
    CopyFrom {
        target: &'static str,
    },
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
    /// The fonts column and its sample pane (mouse routing).
    pub fonts_column: Rect,
    pub sample: Rect,
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
    pub preview: Preview,
    pub side: Side,
    pub page: Page,
    /// The colour picker (pane 5) is on screen.
    pub picker_visible: bool,
    /// The font sample (pane 3) is on screen.
    pub sample_visible: bool,
    /// Cursor in the colour list (group and colour rows); `sel` is the colour.
    pub row_sel: usize,
    /// Open colour groups (indexes into GROUPS).
    pub expanded_groups: std::collections::HashSet<usize>,
    pub fonts: FontsTab,
    /// Draws images in the terminal (kitty graphics); None in tests.
    pub picker: Option<Picker>,
    pub sample_image: Option<StatefulProtocol>,
    pub sample_error: Option<String>,
    pub keymap: Keymap,
    /// Every keymap preset (listed on the Settings page, rework step 6).
    #[allow(dead_code)]
    pub presets: Vec<Keymap>,
    pub settings: crate::config::Settings,
    /// Keys of a sequence typed so far (e.g. the leader), waiting for the rest.
    pub pending_keys: Vec<Key>,
    /// The Open / Save as pop-up is for font presets, not palettes.
    pub overlay_fonts: bool,
    quit: bool,
}

pub enum Start {
    Active,
    Named(String),
    New,
}

impl App {
    /// Starts with the settings in ~/.config/theme-studio (creating or
    /// topping up the files).
    pub fn new(store: Store, start: Start) -> Result<App> {
        App::with_config(
            store,
            start,
            crate::config::load(&crate::config::dir(), true),
        )
    }

    pub fn with_config(store: Store, start: Start, config: crate::config::Loaded) -> Result<App> {
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
        let preview = Preview::new(&store.dir);
        let fonts = FontsTab::new(&store.dir, preview.draft_dir(), preview.dirty_flag());
        let mut app = App {
            preview,
            side: Side::Colours,
            page: Page::Main,
            picker_visible: config.settings.colours.picker_shown(),
            sample_visible: config.settings.fonts.sample_at_start != "hidden",
            row_sel: 0,
            expanded_groups: GROUPS
                .iter()
                .enumerate()
                .filter(|(_, (g, _))| {
                    config
                        .settings
                        .colours
                        .open_groups
                        .iter()
                        .any(|o| o.eq_ignore_ascii_case(g))
                })
                .map(|(i, _)| i)
                .collect(),
            fonts,
            picker: None,
            sample_image: None,
            sample_error: None,
            overlay_fonts: false,
            keymap: config.keymap,
            presets: config.presets,
            settings: config.settings.clone(),
            pending_keys: Vec::new(),
            store,
            name,
            is_new,
            saved: palette.clone(),
            palette,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            keys: GROUPS
                .iter()
                .flat_map(|(_, ks)| ks.iter().copied())
                .collect(),
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
        app.fonts.filters = app.settings.font_list.filters();
        app.fonts.unfold_current = app.settings.fonts.list_opens_to_current_font;
        app.fonts.refresh_installed();
        app.sync_row();
        if !config.warnings.is_empty() {
            app.error(format!("Settings: {}", config.warnings.join("; ")));
        }
        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let mut redraw = true;
        while !self.quit {
            // nvim in the preview re-reads this whenever it changes
            self.preview.sync_draft(&self.palette);
            if self.page == Page::Main && self.sample_visible {
                self.update_sample();
            }
            if self.preview.take_dirty() || redraw {
                terminal.draw(|f| crate::ui::draw(f, self))?;
                redraw = false;
            }
            // Short poll: the preview's programs can print at any time.
            if event::poll(Duration::from_millis(25))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => self.on_key(k),
                    Event::Mouse(m) => self.on_mouse(m),
                    Event::Paste(s) => self.on_paste(&s),
                    _ => {}
                }
                redraw = true;
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

    /// Asks for a new font sample when the fonts or colours changed, and
    /// shows a finished one.
    fn update_sample(&mut self) {
        let fonts = self.fonts.shown_fonts();
        let sizes = [0, 1, 2].map(|r| self.fonts.effective_size(r, fonts[r].size));
        let markup = crate::fonts::sample_markup(&fonts, sizes, &self.palette, &self.name);
        let dpi = self.fonts.dpi;
        self.fonts
            .sample
            .request(markup, self.palette.get("bg0"), dpi);
        match self.fonts.sample.take() {
            Some(Ok(path)) => match image::open(&path) {
                Ok(img) => {
                    self.sample_image = self.picker.as_ref().map(|p| p.new_resize_protocol(img));
                    self.sample_error = None;
                }
                Err(e) => self.sample_error = Some(format!("{e}")),
            },
            Some(Err(e)) => self.sample_error = Some(e),
            None => {}
        }
    }

    /// Starts with the font roles focused (`--fonts`).
    pub fn open_fonts_tab(&mut self) {
        self.focus_pane(1);
    }

    // ----- font presets ----------------------------------------------------

    fn fonts_save(&mut self, then_apply: bool) {
        if self.fonts.is_new {
            self.overlay_fonts = true;
            self.overlay = Overlay::SaveAs {
                input: String::new(),
                overwrite: false,
                then_apply,
            };
            return;
        }
        let name = self.fonts.name.clone();
        self.fonts_save_as(&name, then_apply);
    }

    fn fonts_save_as(&mut self, name: &str, then_apply: bool) {
        match self.fonts.save_as(name) {
            Ok(()) if then_apply => match self.fonts.store.apply(name) {
                Ok(()) => {
                    self.fonts.active = Some(name.to_string());
                    self.info(format!("Saved and applied font preset \"{name}\""));
                }
                Err(e) => self.error(format!("Apply failed: {e:#}")),
            },
            Ok(()) => self.info(format!("Saved font preset \"{name}\" to fonts.json")),
            Err(e) => self.error(format!("Save failed: {e:#}")),
        }
    }

    fn fonts_open_overlay(&mut self) {
        match self.fonts.store.load_all() {
            Ok(all) => {
                let entries: Vec<(String, Vec<Rgb>)> =
                    all.keys().map(|n| (n.clone(), Vec::new())).collect();
                let sel = entries
                    .iter()
                    .position(|(n, _)| *n == self.fonts.name)
                    .unwrap_or(0);
                self.overlay_fonts = true;
                self.overlay = Overlay::Open {
                    filter: String::new(),
                    sel,
                    entries,
                };
            }
            Err(e) => self.error(format!("{e:#}")),
        }
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

    /// Selects a colour without touching the list cursor.
    fn select_key(&mut self, i: usize) {
        if i < self.keys.len() && i != self.sel {
            self.sel = i;
            self.last_edit = None;
            self.load_picker();
        }
    }

    /// Selects a colour, opening its group and moving the list cursor to it.
    fn select(&mut self, i: usize) {
        if i < self.keys.len() {
            self.select_key(i);
            let g = self.group_of(i);
            self.expanded_groups.insert(g);
            self.sync_row();
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
        self.overlay_fonts = false;
        if self.is_new {
            self.overlay = Overlay::SaveAs {
                input: String::new(),
                overwrite: false,
                then_apply,
            };
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
        let dirty = match p {
            Pending::Quit => self.dirty() || self.fonts.dirty(),
            _ if p.is_fonts() => self.fonts.dirty(),
            _ => self.dirty(),
        };
        if dirty {
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
            Pending::NewFonts => {
                self.fonts.new_preset();
                self.info("New font preset (a copy of the current fonts); s asks for a name");
            }
            Pending::OpenFonts(n) => match self.fonts.open(&n) {
                Ok(()) => self.info(format!("Opened font preset \"{n}\"")),
                Err(e) => self.error(format!("{e:#}")),
            },
        }
    }

    /// "Save first" from the unsaved-changes prompt: saves whatever `p`
    /// would throw away (both for quit), if it already has a name.
    fn save_before(&mut self, p: &Pending) {
        let palette = matches!(p, Pending::Quit) || !p.is_fonts();
        let fonts = matches!(p, Pending::Quit) || p.is_fonts();
        if palette && self.dirty() && !self.is_new {
            let name = self.name.clone();
            self.save_as(&name, false);
        }
        if fonts && self.fonts.dirty() && !self.fonts.is_new {
            let name = self.fonts.name.clone();
            self.fonts_save_as(&name, false);
        }
    }

    fn open_overlay(&mut self) {
        self.overlay_fonts = false;
        match self.store.load_all() {
            Ok(all) => {
                let entries = all
                    .iter()
                    .map(|(name, v)| {
                        let sw = [
                            "bg0", "fg0", "red", "orange", "yellow", "green", "aqua", "blue",
                            "purple",
                        ]
                        .iter()
                        .filter_map(|k| v.get(*k).and_then(|x| x.as_str()).and_then(Rgb::parse))
                        .collect();
                        (name.clone(), sw)
                    })
                    .collect::<Vec<_>>();
                let sel = entries
                    .iter()
                    .position(|(n, _)| *n == self.name)
                    .unwrap_or(0);
                self.overlay = Overlay::Open {
                    filter: String::new(),
                    sel,
                    entries,
                };
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    // ----- input -----------------------------------------------------------

    /// Number keys jump between the numbered panes: ¹ font roles, ² font
    /// list, ³ font sample, ⁴ colour list, ⁵ colour picker, ⁶ nvim, ⁷ shell
    /// (or yazi). 6 and 7 give that preview window the keyboard; F12 hands it
    /// back. A hidden picker or sample is shown.
    pub fn focus_pane(&mut self, n: u8) {
        use crate::fonts::FontFocus;
        self.page = Page::Main;
        match n {
            1..=3 => {
                self.preview.focus = None;
                self.side = Side::Fonts;
                match n {
                    1 => self.fonts.focus = FontFocus::Slots,
                    2 => self.fonts.open_list(),
                    _ => {
                        self.sample_visible = true;
                        self.fonts.focus = FontFocus::Sample;
                    }
                }
            }
            4 => {
                self.preview.focus = None;
                self.side = Side::Colours;
                if self.focus != Focus::List {
                    self.leave_picker();
                }
            }
            5 => {
                self.preview.focus = None;
                self.open_picker();
            }
            6 | 7 => {
                self.preview.mode = PreviewMode::Desktop;
                let id = match (n, self.preview.show_yazi) {
                    (6, _) => PaneId::Nvim,
                    (_, true) => PaneId::Yazi,
                    _ => PaneId::Shell,
                };
                // the lower slot counts whether it last showed the shell or yazi
                let shown = self
                    .preview
                    .rects
                    .iter()
                    .any(|(p, _)| *p == id || (n == 7 && *p != PaneId::Nvim));
                if !shown && !self.preview.rects.is_empty() {
                    self.info("That preview window isn't shown at this window size");
                    return;
                }
                self.preview.focus = Some(id);
            }
            _ => {}
        }
    }

    /// The number of the pane with the keyboard.
    pub fn current_pane(&self) -> u8 {
        use crate::fonts::FontFocus;
        match (self.preview.focus, self.side) {
            (Some(PaneId::Nvim), _) => 6,
            (Some(_), _) => 7,
            (None, Side::Fonts) => match self.fonts.focus {
                FontFocus::Slots => 1,
                FontFocus::List | FontFocus::Search => 2,
                FontFocus::Sample => 3,
            },
            (None, Side::Colours) if self.focus == Focus::List => 4,
            (None, Side::Colours) => 5,
        }
    }

    /// `<C-w>h/j/k/l`: the neighbouring pane in the fonts and colour columns
    /// (the preview windows take the keyboard, so they're only entered on
    /// purpose: 6, 7, F12 or a click). Hidden panes are skipped.
    fn move_pane(&mut self, dx: i32, dy: i32) {
        let columns: [Vec<u8>; 2] = [
            [1, 2, 3]
                .into_iter()
                .filter(|&p| p != 3 || self.sample_visible)
                .collect(),
            [4, 5]
                .into_iter()
                .filter(|&p| p != 5 || self.picker_visible)
                .collect(),
        ];
        let cur = self.current_pane();
        let Some((c, r)) = columns
            .iter()
            .enumerate()
            .find_map(|(c, col)| col.iter().position(|&p| p == cur).map(|r| (c, r)))
        else {
            return;
        };
        let c2 = (c as i32 + dx).clamp(0, 1) as usize;
        let col = &columns[c2];
        let r2 = if dx != 0 {
            r.min(col.len() - 1)
        } else {
            (r as i32 + dy).clamp(0, col.len() as i32 - 1) as usize
        };
        let target = col[r2];
        if target != cur {
            self.focus_pane(target);
        }
    }

    /// Shows the picker and gives it the keyboard (editing a colour).
    fn open_picker(&mut self) {
        self.side = Side::Colours;
        self.picker_visible = true;
        if self.focus == Focus::List {
            self.focus = Focus::Square;
        }
    }

    /// Back to the colour list from the picker; the picker hides again
    /// unless Settings says it stays open.
    fn leave_picker(&mut self) {
        self.focus = Focus::List;
        if self.settings.colours.picker_hides_on_esc() {
            self.picker_visible = false;
        }
    }

    /// The keymap contexts for the pane with the keyboard, most specific first.
    pub fn contexts(&self) -> [Context; 3] {
        use crate::fonts::FontFocus;
        if self.page != Page::Main {
            return [Context::Global; 3];
        }
        match self.side {
            Side::Fonts => match self.fonts.focus {
                FontFocus::Slots => [Context::FontRoles, Context::Fonts, Context::Global],
                FontFocus::List | FontFocus::Search => {
                    [Context::FontList, Context::Fonts, Context::Global]
                }
                FontFocus::Sample => [Context::Fonts, Context::Global, Context::Global],
            },
            Side::Colours if self.focus == Focus::List => {
                [Context::ColourList, Context::Colours, Context::Global]
            }
            Side::Colours => [Context::Picker, Context::Colours, Context::Global],
        }
    }

    // ----- colour list rows -----------------------------------------------

    /// The colour list as shown: a row per group, and the colours of open groups.
    pub fn colour_rows(&self) -> Vec<ColourRow> {
        let mut out = Vec::new();
        let mut key = 0;
        for (index, (_, keys)) in GROUPS.iter().enumerate() {
            let expanded = self.expanded_groups.contains(&index);
            out.push(ColourRow::Group { index, expanded });
            for _ in keys.iter() {
                if expanded {
                    out.push(ColourRow::Key(key));
                }
                key += 1;
            }
        }
        out
    }

    fn group_of(&self, key: usize) -> usize {
        let mut start = 0;
        for (gi, (_, keys)) in GROUPS.iter().enumerate() {
            if key < start + keys.len() {
                return gi;
            }
            start += keys.len();
        }
        0
    }

    /// Moves the list cursor; landing on a colour selects it.
    fn move_row(&mut self, d: i32) {
        let rows = self.colour_rows();
        self.row_sel = (self.row_sel as i32 + d).clamp(0, rows.len() as i32 - 1) as usize;
        if let ColourRow::Key(k) = rows[self.row_sel] {
            self.select_key(k);
        }
    }

    /// Puts the list cursor on the selected colour if its group is open, else
    /// on its group.
    fn sync_row(&mut self) {
        let rows = self.colour_rows();
        let g = self.group_of(self.sel);
        self.row_sel = rows
            .iter()
            .position(|r| *r == ColourRow::Key(self.sel))
            .or_else(|| {
                rows.iter()
                    .position(|r| matches!(r, ColourRow::Group { index, .. } if *index == g))
            })
            .unwrap_or(0);
    }

    fn set_group_open(&mut self, g: usize, open: bool) {
        if open {
            self.expanded_groups.insert(g);
        } else {
            self.expanded_groups.remove(&g);
        }
    }

    /// Colour-list keys: open/close groups, move, start editing.
    fn list_action(&mut self, a: Action) {
        let rows = self.colour_rows();
        let last = rows.len() as i32 - 1;
        match (a, rows.get(self.row_sel).copied()) {
            (Action::Up, _) => self.move_row(-1),
            (Action::Down, _) => self.move_row(1),
            (Action::Top, _) => self.move_row(-(self.row_sel as i32)),
            (Action::Bottom, _) => self.move_row(last - self.row_sel as i32),
            (Action::PageDown, _) => self.move_row(10),
            (Action::PageUp, _) => self.move_row(-10),
            (Action::Confirm, Some(ColourRow::Group { index, expanded })) => {
                self.set_group_open(index, !expanded)
            }
            (
                Action::Right,
                Some(ColourRow::Group {
                    index,
                    expanded: false,
                }),
            ) => self.set_group_open(index, true),
            (Action::Right, Some(ColourRow::Group { expanded: true, .. })) => self.move_row(1),
            (
                Action::Left,
                Some(ColourRow::Group {
                    index,
                    expanded: true,
                }),
            ) => self.set_group_open(index, false),
            (Action::Left, Some(ColourRow::Key(k))) => {
                let g = self.group_of(k);
                self.row_sel = rows
                    .iter()
                    .position(|r| matches!(r, ColourRow::Group { index, .. } if *index == g))
                    .unwrap_or(0);
            }
            (Action::Right | Action::Confirm, Some(ColourRow::Key(_))) => self.open_picker(),
            _ => {}
        }
    }

    fn on_key(&mut self, k: KeyEvent) {
        use crate::fonts::FontFocus;
        let key = Key::from_event(k);
        // A preview window has the keyboard: everything but the key bound to
        // "type into the preview / next window / back" goes to it.
        if let Some(id) = self.preview.focus
            && matches!(self.overlay, Overlay::None)
        {
            if self.keymap.lookup(&[Context::Global], &[key]) == Lookup::Run(Action::PreviewCycle) {
                self.preview.cycle_focus();
            } else {
                self.preview.send_key(id, k);
            }
            return;
        }
        // Ctrl+C always quits (asking first about unsaved changes)
        if key == Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL) {
            self.pending_keys.clear();
            self.guard(Pending::Quit);
            return;
        }
        let copying = matches!(self.overlay, Overlay::CopyFrom { .. });
        if !matches!(self.overlay, Overlay::None) && !copying {
            self.on_overlay_key(k);
            return;
        }
        self.message = None;
        // Text fields: keys are typing, not actions.
        if self.pending_keys.is_empty() {
            if self.side == Side::Colours && self.focus == Focus::Hex {
                return self.on_hex_key(k);
            }
            if self.side == Side::Fonts && self.fonts.focus == FontFocus::Search {
                return self.fonts.on_search_key(k);
            }
        }
        self.pending_keys.push(key);
        let contexts = self.contexts();
        match self.keymap.lookup(&contexts, &self.pending_keys) {
            Lookup::Pending => {}
            Lookup::Run(a) => {
                self.pending_keys.clear();
                self.run_action(a);
            }
            Lookup::None => self.pending_keys.clear(),
        }
    }

    /// Does what an action means in the pane with the keyboard.
    pub fn run_action(&mut self, a: Action) {
        use crate::fonts::FontFocus;
        if let Overlay::CopyFrom { target } = self.overlay {
            return self.run_copy_from(target, a);
        }
        let fonts = self.side == Side::Fonts;
        let page_action = matches!(
            a,
            Action::Quit
                | Action::Help
                | Action::PageMain
                | Action::PageDownloads
                | Action::PageSettings
        );
        if self.page != Page::Main && !page_action {
            return;
        }
        match a {
            Action::Quit => self.guard(Pending::Quit),
            Action::Help => self.overlay = Overlay::Help,
            Action::PageMain => self.page = Page::Main,
            Action::PageDownloads => self.page = Page::Downloads,
            Action::PageSettings => self.page = Page::Settings,
            Action::PreviewCycle => {
                self.preview.mode = PreviewMode::Desktop;
                self.preview.cycle_focus();
            }
            Action::TogglePicker => {
                if self.picker_visible {
                    self.picker_visible = false;
                    self.focus = Focus::List;
                } else {
                    self.picker_visible = true;
                }
            }
            Action::ToggleSample => {
                use crate::fonts::FontFocus;
                self.sample_visible = !self.sample_visible;
                if !self.sample_visible && self.fonts.focus == FontFocus::Sample {
                    self.fonts.focus = FontFocus::Slots;
                }
            }
            Action::PaneLeft => self.move_pane(-1, 0),
            Action::PaneRight => self.move_pane(1, 0),
            Action::PaneUp => self.move_pane(0, -1),
            Action::PaneDown => self.move_pane(0, 1),
            Action::SavePalette => self.save(false),
            Action::SaveFonts => self.fonts_save(false),
            Action::ApplyPalette => self.apply(),
            Action::ApplyFonts => self.fonts_apply(),
            Action::Save if fonts => self.fonts_save(false),
            Action::Save => self.save(false),
            Action::SaveAs => {
                self.overlay_fonts = fonts;
                let input = if fonts {
                    self.fonts.name.clone()
                } else {
                    self.name.clone()
                };
                self.overlay = Overlay::SaveAs {
                    input,
                    overwrite: false,
                    then_apply: false,
                };
            }
            Action::Apply if fonts => self.fonts_apply(),
            Action::Apply => self.apply(),
            Action::Open if fonts => self.fonts_open_overlay(),
            Action::Open => self.open_overlay(),
            Action::New if fonts => self.guard(Pending::NewFonts),
            Action::New => self.guard(Pending::New),
            Action::Panel(n) => self.focus_pane(n),
            // Tab in the fonts column: roles → list → sample → roles
            Action::FocusNext | Action::FocusPrev if fonts => {
                let order: Vec<u8> = [1, 2, 3]
                    .into_iter()
                    .filter(|&p| p != 3 || self.sample_visible)
                    .collect();
                let i = order
                    .iter()
                    .position(|&p| p == self.current_pane())
                    .unwrap_or(0) as i32;
                let d = if a == Action::FocusNext { 1 } else { -1 };
                let next = order[(i + d).rem_euclid(order.len() as i32) as usize];
                self.focus_pane(next);
                if next == 1 {
                    self.fonts.focus = FontFocus::Slots;
                }
            }
            _ if fonts => {
                self.fonts.act(a);
            }
            _ => self.run_colours(a),
        }
    }

    /// Colour-tab actions: the list, the picker and the preview toggles.
    fn run_colours(&mut self, a: Action) {
        match a {
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Pin => self.toggle_pin(),
            Action::CopyHex => {
                let hex = self.palette.get(self.key()).hex();
                self.copy(&hex);
            }
            Action::CopyFrom => {
                self.overlay = Overlay::CopyFrom { target: self.key() };
                self.focus = Focus::List;
            }
            Action::HexEntry => {
                self.focus = Focus::Hex;
                self.hex_input = "#".into();
            }
            Action::FocusNext => self.cycle_focus(1),
            Action::FocusPrev => self.cycle_focus(-1),
            Action::Back if self.focus != Focus::List => self.leave_picker(),
            Action::ToggleYazi => {
                self.preview.mode = PreviewMode::Desktop;
                self.preview.toggle_yazi();
            }
            Action::ToggleRofi => {
                self.preview.mode = PreviewMode::Desktop;
                self.preview.show_rofi = !self.preview.show_rofi;
            }
            Action::ToggleView => {
                self.preview.mode = match self.preview.mode {
                    PreviewMode::Desktop => PreviewMode::Swatches,
                    PreviewMode::Swatches => PreviewMode::Desktop,
                };
            }
            _ if self.focus == Focus::List => self.list_action(a),
            _ => {
                let (dx, dy, big) = match a {
                    Action::Left => (-1, 0, false),
                    Action::Right => (1, 0, false),
                    Action::Up => (0, -1, false),
                    Action::Down => (0, 1, false),
                    Action::LeftBig => (-1, 0, true),
                    Action::RightBig => (1, 0, true),
                    Action::UpBig => (0, -1, true),
                    Action::DownBig => (0, 1, true),
                    _ => return,
                };
                self.nudge(dx, dy, big);
            }
        }
    }

    /// Moves the focused picker control: the square by saturation/value, the
    /// hue strip by hue, sliders by their value (up/down picks a slider).
    fn nudge(&mut self, dx: i32, dy: i32, big: bool) {
        match self.focus {
            Focus::Square => {
                let step = if big { 0.1 } else { 0.02 };
                let (h, s, v) = self.hsv;
                self.set_hsv(h, s + dx as f64 * step, v - dy as f64 * step);
            }
            Focus::Hue => {
                let step = if big { 15.0 } else { 2.0 };
                let (h, s, v) = self.hsv;
                self.set_hsv(h + (dy + dx) as f64 * step, s, v);
            }
            Focus::SliderH | Focus::SliderS | Focus::SliderL if dx != 0 => {
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
            Focus::SliderH | Focus::SliderS | Focus::SliderL => self.cycle_focus(dy),
            _ => {}
        }
    }

    /// While picking the key to copy a colour from: move, Enter copies, Esc cancels.
    fn run_copy_from(&mut self, target: &'static str, a: Action) {
        let on_key = matches!(
            self.colour_rows().get(self.row_sel),
            Some(ColourRow::Key(_))
        );
        match a {
            // a colour row: copy it; a group row: open or close it
            Action::Confirm | Action::Right if on_key => {
                let from = self.key();
                let c = self.palette.get(from);
                self.overlay = Overlay::None;
                let to = self.keys.iter().position(|k| *k == target).unwrap_or(0);
                self.select(to);
                self.set_key(target, c);
                self.info(format!("{target} = {from} ({})", c.hex()));
            }
            Action::Back => self.overlay = Overlay::None,
            Action::Up
            | Action::Down
            | Action::Top
            | Action::Bottom
            | Action::PageUp
            | Action::PageDown
            | Action::Confirm
            | Action::Right
            | Action::Left => self.list_action(a),
            _ => {}
        }
    }

    fn fonts_apply(&mut self) {
        if self.fonts.dirty() {
            self.fonts_save(true);
        } else {
            let name = self.fonts.name.clone();
            self.fonts_save_as(&name, true);
        }
    }

    fn on_hex_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Enter => match Rgb::parse(&self.hex_input) {
                Some(c) => {
                    self.last_edit = None;
                    self.edit(c);
                    self.load_picker();
                    self.leave_picker();
                }
                None => self.error(format!("Not a colour: {} (use #rrggbb)", self.hex_input)),
            },
            KeyCode::Esc => {
                self.load_picker();
                self.leave_picker();
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
                KeyCode::Char('s') => {
                    self.save_before(&p);
                    self.run_pending(p);
                    Overlay::None
                }
                KeyCode::Esc | KeyCode::Char('n') => Overlay::None,
                _ => Overlay::Confirm(p),
            },
            // keys while copying go through the keymap (run_copy_from)
            Overlay::CopyFrom { target } => Overlay::CopyFrom { target },
            Overlay::SaveAs {
                mut input,
                overwrite,
                then_apply,
            } => match k.code {
                KeyCode::Esc => Overlay::None,
                KeyCode::Enter => {
                    let name = input.trim().to_string();
                    let (exists, current) = if self.overlay_fonts {
                        let all = self.fonts.store.load_all();
                        (
                            all.map(|a| a.contains_key(&name)).unwrap_or(false),
                            self.fonts.name.clone(),
                        )
                    } else {
                        let all = self.store.load_all();
                        (
                            all.map(|a| a.contains_key(&name)).unwrap_or(false),
                            self.name.clone(),
                        )
                    };
                    if name.is_empty() {
                        Overlay::SaveAs {
                            input,
                            overwrite,
                            then_apply,
                        }
                    } else if self.overlay_fonts && (!exists || name == current || overwrite) {
                        self.fonts_save_as(&name, then_apply);
                        Overlay::None
                    } else if exists && name != current && !overwrite {
                        self.error(format!("\"{name}\" exists. Enter again to overwrite it"));
                        Overlay::SaveAs {
                            input,
                            overwrite: true,
                            then_apply,
                        }
                    } else {
                        self.save_as(&name, then_apply);
                        Overlay::None
                    }
                }
                KeyCode::Backspace => {
                    input.pop();
                    Overlay::SaveAs {
                        input,
                        overwrite: false,
                        then_apply,
                    }
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    Overlay::SaveAs {
                        input,
                        overwrite: false,
                        then_apply,
                    }
                }
                _ => Overlay::SaveAs {
                    input,
                    overwrite,
                    then_apply,
                },
            },
            Overlay::Open {
                mut filter,
                mut sel,
                entries,
            } => {
                let n = open_matches(&entries, &filter).len();
                match k.code {
                    KeyCode::Esc => Overlay::None,
                    KeyCode::Enter => match open_matches(&entries, &filter).get(sel) {
                        Some(&i) => {
                            let name = entries[i].0.clone();
                            self.guard(if self.overlay_fonts {
                                Pending::OpenFonts(name)
                            } else {
                                Pending::Open(name)
                            });
                            // guard() may have opened a confirmation instead
                            std::mem::replace(&mut self.overlay, Overlay::None)
                        }
                        None => Overlay::Open {
                            filter,
                            sel,
                            entries,
                        },
                    },
                    KeyCode::Down => {
                        sel = (sel + 1).min(n.saturating_sub(1));
                        Overlay::Open {
                            filter,
                            sel,
                            entries,
                        }
                    }
                    KeyCode::Up => {
                        sel = sel.saturating_sub(1);
                        Overlay::Open {
                            filter,
                            sel,
                            entries,
                        }
                    }
                    KeyCode::Char('n') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        sel = (sel + 1).min(n.saturating_sub(1));
                        Overlay::Open {
                            filter,
                            sel,
                            entries,
                        }
                    }
                    KeyCode::Char('p') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        sel = sel.saturating_sub(1);
                        Overlay::Open {
                            filter,
                            sel,
                            entries,
                        }
                    }
                    KeyCode::Backspace => {
                        filter.pop();
                        Overlay::Open {
                            filter,
                            sel: 0,
                            entries,
                        }
                    }
                    KeyCode::Char(c) => {
                        filter.push(c);
                        Overlay::Open {
                            filter,
                            sel: 0,
                            entries,
                        }
                    }
                    _ => Overlay::Open {
                        filter,
                        sel,
                        entries,
                    },
                }
            }
            Overlay::None => Overlay::None,
        };
    }

    fn on_mouse(&mut self, m: MouseEvent) {
        if self.page != Page::Main {
            return;
        }
        let (x, y) = (m.column, m.row);
        let inside = |r: Rect| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height;
        // The fonts column: roles, chips and rows (fonts.click), the sample.
        if inside(self.areas.fonts_column) && matches!(self.overlay, Overlay::None) {
            match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    self.preview.focus = None;
                    self.side = Side::Fonts;
                    if inside(self.areas.sample) {
                        self.fonts.focus = crate::fonts::FontFocus::Sample;
                    } else {
                        self.fonts.click(x, y);
                    }
                }
                MouseEventKind::ScrollDown => self.fonts.scroll(1),
                MouseEventKind::ScrollUp => self.fonts.scroll(-1),
                _ => {}
            }
            return;
        }
        // Clicking a preview window gives it the keyboard; clicking anywhere
        // else gives it back to the editor.
        if let MouseEventKind::Down(_) = m.kind {
            let hit = self
                .preview
                .rects
                .iter()
                .find(|(_, r)| inside(*r))
                .map(|(id, _)| *id);
            if hit.is_some() || self.preview.focus.is_some() {
                self.preview.focus = hit;
                if hit.is_some() {
                    return;
                }
            }
        }
        let frac = |r: Rect, v: u16, len: u16| {
            if len <= 1 {
                0.0
            } else {
                (v.saturating_sub(r.x.min(v)) as f64 / (len - 1) as f64).clamp(0.0, 1.0)
            }
        };
        match m.kind {
            MouseEventKind::ScrollDown if inside(self.areas.list) => self.move_row(1),
            MouseEventKind::ScrollUp if inside(self.areas.list) => self.move_row(-1),
            MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
                let a = &self.areas;
                let (sq, hue, sh, ss, sl, hex, list) = (
                    a.square, a.hue, a.slider_h, a.slider_s, a.slider_l, a.hex, a.list,
                );
                self.side = Side::Colours;
                if inside(list) {
                    if !matches!(m.kind, MouseEventKind::Down(_)) {
                        return; // dragging over the list does nothing
                    }
                    if let Some(Some(r)) = self.areas.list_rows.get((y - list.y) as usize).copied()
                    {
                        let again = r == self.row_sel
                            && self.focus == Focus::List
                            && self.side == Side::Colours;
                        self.row_sel = r;
                        match self.colour_rows().get(r).copied() {
                            Some(ColourRow::Group { index, expanded }) if again => {
                                self.set_group_open(index, !expanded)
                            }
                            Some(ColourRow::Key(k)) => {
                                self.select_key(k);
                                if again && matches!(self.overlay, Overlay::None) {
                                    self.open_picker();
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                    if matches!(self.overlay, Overlay::None) {
                        self.side = Side::Colours;
                        self.focus = Focus::List;
                    }
                } else if inside(sq) {
                    self.focus = Focus::Square;
                    let s = frac(sq, x, sq.width);
                    let v =
                        1.0 - ((y - sq.y) as f64 / (sq.height.max(2) - 1) as f64).clamp(0.0, 1.0);
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
        if self.side == Side::Fonts && matches!(self.overlay, Overlay::None) {
            if self.fonts.focus == crate::fonts::FontFocus::Search {
                self.fonts.search.push_str(s.trim());
                self.fonts.list_sel = 0;
            }
            return;
        }
        if let Some(id) = self.preview.focus {
            return self.preview.send_paste(id, s);
        }
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
            (Overlay::None, None) => {
                self.error(format!("Pasted text isn't a colour: {}", s.trim()))
            }
            _ => {}
        }
    }

    fn cycle_focus(&mut self, d: i32) {
        let i = FOCUS_ORDER
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0) as i32;
        let n = FOCUS_ORDER.len() as i32;
        self.focus = FOCUS_ORDER[(i + d).rem_euclid(n) as usize];
        if self.focus != Focus::List {
            self.picker_visible = true;
        }
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
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
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
        let dir =
            std::env::temp_dir().join(format!("theme-studio-app-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut two = Palette::blank();
        two.set("red", Rgb(200, 0, 0));
        let mut m = Map::new();
        m.insert("One".into(), Value::Object(Palette::blank().to_json()));
        m.insert("Two".into(), Value::Object(two.to_json()));
        std::fs::write(dir.join("palettes.json"), to_jq_string(&Value::Object(m))).unwrap();
        Store { dir }
    }

    /// An app whose preview never starts nvim, the shell or yazi.
    fn test_app(store: Store, start: Start) -> App {
        // the shipped defaults, never your ~/.config/theme-studio
        let config = crate::config::Loaded {
            keymap: Keymap::vim(),
            presets: Keymap::builtin(),
            settings: crate::config::Settings::default(),
            warnings: Vec::new(),
        };
        let mut app = App::with_config(store, start, config).unwrap();
        app.preview.set_headless();
        app
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
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn picker_edits_follow_through_derived_keys_and_undo() {
        let mut app = test_app(temp_store("edit"), Start::Named("One".into()));
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
        let mut app = test_app(temp_store("pin"), Start::Named("One".into()));
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
        let mut app = test_app(store, Start::Named("One".into()));
        press(&mut app, "#abcdef\n");
        press(&mut app, "s");
        assert!(!app.dirty());
        assert_eq!(
            Store { dir: dir.clone() }.load("One").unwrap().get("bg0"),
            Rgb(0xab, 0xcd, 0xef)
        );
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
        let mut app = test_app(temp_store("draw"), Start::Named("One".into()));
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("Palette: One"));
        // only Base is open; the other groups are rows that open
        assert!(
            screen.contains("▾ Base") && screen.contains("▸ Backgrounds"),
            "{screen}"
        );
        assert!(
            !screen.contains("bg_statusline1"),
            "Backgrounds starts closed"
        );
        assert!(!screen.contains("hex #1e1e22"), "the picker starts hidden");
        // contrast column fits: e.g. "fg0 ... 11.8:1"
        let fg_row = screen.lines().find(|l| l.contains(" fg0 ")).unwrap();
        assert!(fg_row.contains(":1"), "contrast cut off: {fg_row}");
        // the last row is the last group: open it and go to its last colour
        press(&mut app, "G\nG");
        assert!(render(&mut app, 160, 45).contains("bg_diff_blue"));
        // editing a colour shows the picker; Esc hides it again
        press(&mut app, "gj\n"); // g is the Base group row; j the first colour
        assert!(render(&mut app, 160, 45).contains("hex"));
        assert!(app.picker_visible);
        press(&mut app, "\x1b");
        assert!(!app.picker_visible);
        // <leader>cp toggles it, <leader>fs the font sample
        press(&mut app, " cp");
        assert!(app.picker_visible);
        press(&mut app, " cp");
        assert!(!app.picker_visible);
        press(&mut app, "1 fs");
        assert!(!app.sample_visible);
        press(&mut app, "4");
        // too small: what's needed
        press(&mut app, "o?");
        let small = render(&mut app, 80, 20);
        assert!(small.contains("Window too small"));
        assert!(small.contains("Width = 80  Height = 20"));
        assert!(small.contains("Width = 98"), "{small}");
        // every pop-up draws without panicking
        for keys in ["\x1b?", "\x1bS", "\x1b\x1bc", "\x1b"] {
            press(&mut app, keys);
            render(&mut app, 160, 45);
        }
        // the other pages
        for page in [2, 3, 1] {
            app.on_key(KeyEvent::new(KeyCode::F(page), KeyModifiers::NONE));
            render(&mut app, 160, 45);
        }
    }

    #[test]
    fn number_keys_jump_between_panels() {
        let mut app = test_app(temp_store("panels"), Start::Named("One".into()));
        render(&mut app, 200, 50); // lays out the preview windows
        assert!(!app.picker_visible, "the picker starts hidden");
        press(&mut app, "5");
        assert_eq!((app.side, app.focus), (Side::Colours, Focus::Square));
        assert!(app.picker_visible, "5 shows it");
        press(&mut app, "4");
        assert_eq!(app.focus, Focus::List);
        assert!(!app.picker_visible, "leaving it hides it again");
        press(&mut app, "1");
        assert_eq!(
            (app.side, app.fonts.focus),
            (Side::Fonts, crate::fonts::FontFocus::Slots)
        );
        press(&mut app, "2");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::List);
        press(&mut app, "3");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::Sample);
        press(&mut app, "6");
        assert_eq!(app.preview.focus, Some(PaneId::Nvim));
        // inside a preview window digits are typed into it, F12 comes back out
        press(&mut app, "1");
        assert_eq!(app.preview.focus, Some(PaneId::Nvim));
        while app.preview.focus.is_some() {
            app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        }
        press(&mut app, "4Y7");
        assert_eq!(app.preview.focus, Some(PaneId::Yazi));
        app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        // <C-w>: across and down between the fonts and colour columns
        press(&mut app, "1");
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "l");
        assert_eq!(app.current_pane(), 4);
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "h");
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "j");
        assert_eq!(app.current_pane(), 2);
        // digits still type into the hex box
        press(&mut app, "4#123456\n");
        assert_eq!(app.palette.get("bg0"), Rgb(0x12, 0x34, 0x56));
        // titles carry the numbers
        press(&mut app, "5");
        let screen = render(&mut app, 200, 50);
        for t in [
            "¹Fonts",
            "²Installed",
            "³Sample",
            "⁴Palette",
            "⁵Colour",
            "⁶nvim",
            "⁷yazi",
        ] {
            assert!(screen.contains(t), "missing {t}");
        }
    }

    #[test]
    fn fonts_tab_edits_saves_and_opens_presets() {
        let store = temp_store("fonts");
        let dir = store.dir.clone();
        std::fs::write(
            dir.join("fonts.json"),
            "{\n  \"Base\": {\n    \"mono\": {\n      \"family\": \"A\",\n      \"size\": 14\n    },\n    \"ui\": {\n      \"family\": \"B\",\n      \"size\": 9\n    }\n  }\n}\n",
        )
        .unwrap();
        let mut app = test_app(store, Start::Named("One".into()));
        app.fonts.store.dir = dir.clone();
        app.fonts.open("Base").unwrap();
        press(&mut app, "1"); // the font roles
        assert_eq!(app.side, Side::Fonts);
        app.fonts.set_installed(vec![
            crate::fonts::Installed {
                family: "A".into(),
                mono: true,
                nerd: true,
                bitmap: false,
                special: false,
            },
            crate::fonts::Installed {
                family: "B".into(),
                mono: false,
                nerd: false,
                bitmap: false,
                special: false,
            },
            crate::fonts::Installed {
                family: "C Mono".into(),
                mono: true,
                nerd: false,
                bitmap: false,
                special: false,
            },
        ]);
        // bar (third role): pick C Mono from the list, two sizes bigger
        press(&mut app, "jj\nG\nll");
        assert_eq!(app.fonts.preset.fonts[2].family, "C Mono");
        assert_eq!(app.fonts.preset.fonts[2].size, 11);
        // number keys: 2 opens the list, 1 goes back
        press(&mut app, "2");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::List);
        press(&mut app, "1");
        // unsaved: opening another preset asks first
        press(&mut app, "oBase\n");
        assert!(matches!(
            app.overlay,
            Overlay::Confirm(Pending::OpenFonts(_))
        ));
        press(&mut app, "\x1b");
        // save as a new preset, which lands in fonts.json in jq's format
        press(&mut app, "S");
        for _ in 0..4 {
            app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        press(&mut app, "Wide bar\n");
        assert!(!app.fonts.dirty());
        let text = std::fs::read_to_string(dir.join("fonts.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["Wide bar"]["bar"]["family"], "C Mono");
        assert_eq!(text, to_jq_string(&v));
        // the palette tab didn't change, and its keys still work after F1
        assert!(!app.dirty());
        press(&mut app, "5");
        assert_eq!((app.side, app.focus), (Side::Colours, Focus::Square));
        assert!(app.picker_visible);
        // the Fonts tab draws at every size (no image support in tests)
        let screen = render(&mut app, 160, 45);
        for t in [
            "¹Fonts: Wide bar",
            "²Installed fonts",
            "F2 Fonts",
            "Sample",
            "C Mono",
        ] {
            assert!(screen.contains(t), "missing {t}");
        }
        for w in (1..=200).step_by(7) {
            for h in (1..=70).step_by(5) {
                render(&mut app, w, h);
            }
        }
    }

    #[test]
    fn leader_sequences_popup_help_and_hints() {
        let store = temp_store("leader");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        // the hints come from the keymap
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("j/k move")
                && screen.contains("Enter edit")
                && screen.contains("? keys"),
            "{screen}"
        );
        // Space waits and shows what can follow
        press(&mut app, " ");
        assert_eq!(app.pending_keys.len(), 1);
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("<leader>") && screen.contains("Esc cancels"),
            "{screen}"
        );
        // Esc cancels without doing anything
        press(&mut app, "\x1b");
        assert!(app.pending_keys.is_empty());
        // <leader>sp saves the palette from anywhere
        press(&mut app, "#abcdef\n");
        assert!(app.dirty());
        press(&mut app, "1"); // the fonts column
        press(&mut app, " sp");
        assert!(!app.dirty(), "saved from the Fonts tab");
        assert_eq!(
            Store { dir }.load("One").unwrap().get("bg0"),
            Rgb(0xab, 0xcd, 0xef)
        );
        // an unbound sequence does nothing and clears
        press(&mut app, " zz");
        assert!(app.pending_keys.is_empty());
        // the help screen lists the real bindings
        press(&mut app, "?");
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("<leader>sp") && screen.contains("save palette"),
            "{screen}"
        );
        assert!(screen.contains("Vim preset"));
    }

    #[test]
    fn layout_three_columns_then_preview_row_then_too_small() {
        use crate::ui::{Arrangement, arrange};
        let r = |w, h| Rect::new(0, 0, w, h);
        // wide: fonts | colours | preview, the picker hidden, the sample shown
        let Arrangement::Main(l) = arrange(r(200, 50), Page::Main, false, true) else {
            panic!()
        };
        assert!(l.wide);
        assert!(l.roles.x < l.colour_list.x && l.colour_list.x < l.preview.x);
        assert_eq!(
            (l.preview.y, l.preview.height),
            (0, 49),
            "preview is a full-height column"
        );
        assert!(l.picker.is_none() && l.sample.is_some());
        let s = l.sample.unwrap();
        assert!(
            l.roles.y < l.font_list.y && l.font_list.y < s.y,
            "roles, list, sample top to bottom"
        );
        // picker shown: under the list in the same column
        let Arrangement::Main(l) = arrange(r(200, 50), Page::Main, true, true) else {
            panic!()
        };
        let p = l.picker.unwrap();
        assert_eq!(p.x, l.colour_list.x);
        assert!(p.y > l.colour_list.y);
        // narrow (your portrait monitor): columns on top, preview row underneath
        let Arrangement::Main(l) = arrange(r(137, 60), Page::Main, true, true) else {
            panic!()
        };
        assert!(!l.wide);
        assert_eq!((l.preview.x, l.preview.width), (0, 137));
        assert!(l.preview.y > l.roles.y && l.preview.y > l.colour_list.y);
        assert_eq!(l.roles.y, l.colour_list.y, "fonts and colours side by side");
        // too small, and what's needed (less height when things are hidden)
        assert_eq!(
            arrange(r(90, 60), Page::Main, false, false),
            Arrangement::TooSmall {
                need_w: 98,
                need_h: 30
            }
        );
        assert_eq!(
            arrange(r(137, 20), Page::Main, true, true),
            Arrangement::TooSmall {
                need_w: 98,
                need_h: 36
            }
        );
        assert_eq!(
            arrange(r(160, 20), Page::Main, true, true),
            Arrangement::TooSmall {
                need_w: 148,
                need_h: 24
            }
        );
        // other pages need little room
        assert!(matches!(
            arrange(r(60, 20), Page::Settings, true, true),
            Arrangement::Page { .. }
        ));
    }

    /// Dragging the window through every size never crashes the drawing
    /// (tests build in debug mode, where integer underflow panics).
    #[test]
    fn every_window_size_renders() {
        let mut app = test_app(temp_store("sizes"), Start::Named("One".into()));
        for w in (1..=220).step_by(3) {
            for h in (1..=80).step_by(2) {
                render(&mut app, w, h);
            }
        }
        // and with the picker focused and the selection near the end of the list
        press(&mut app, "G\t");
        for w in [52, 53, 111, 112, 113] {
            for h in [22, 23, 34, 35, 36, 60] {
                render(&mut app, w, h);
            }
        }
    }
}
