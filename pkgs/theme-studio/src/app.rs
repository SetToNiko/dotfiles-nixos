//! Editor state and input handling.

use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::Result;
/// The terminal the studio draws on.
pub type Term = ratatui::Terminal<ratatui::backend::CrosstermBackend<crate::out::Out>>;
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
use crate::theme::ThemeStore;
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
    NewTheme,
    OpenTheme(String),
}

/// What the Open / Save as pop-up is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum For {
    Palette,
    Fonts,
    Theme,
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
    /// Settings page: recording keys (a binding or the leader).
    Capture {
        what: crate::settings::Capture,
        keys: Vec<Key>,
    },
    /// Settings page: typing a name or keys.
    Prompt {
        title: String,
        input: String,
        purpose: crate::settings::Naming,
    },
    /// Settings page: choosing a palette key for a hover colour.
    ChooseKey {
        row: crate::behaviour::BhRow,
        sel: usize,
        /// The key before the list opened (Esc puts it back).
        original: String,
    },
    /// Settings page: a yes / no question.
    Ask {
        question: String,
        then: crate::settings::Deferred,
    },
    /// A dropdown list hanging under `at`: the preview's text kind (0) or
    /// language (1).
    Menu {
        menu: u8,
        sel: usize,
        at: Rect,
    },
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
    /// The active system palette (hover colours are its keys).
    pub system_palette: Option<Palette>,
    pub terminal: Option<TerminalMap>,
    /// Status-line message and whether it's an error.
    pub message: Option<(String, bool)>,
    /// When an info message was shown (it goes after settings.messages.seconds).
    pub message_at: Option<Instant>,
    /// A save confirmation in a box (settings.messages.saved = "popup");
    /// stays until a key or a click.
    pub notice: Option<String>,
    pub overlay: Overlay,
    /// What's clickable where (from the last draw), and where the mouse is.
    pub hits: crate::hit::HitMap,
    /// Pane sizes (shares), from settings.json; resizing changes them.
    pub sizes: crate::config::Sizes,
    /// The draggable borders from the last draw.
    pub borders: Vec<crate::hit::Border>,
    /// The border being dragged with the mouse.
    pub dragging: Option<crate::hit::BorderId>,
    /// Resize mode (`<leader>r`): keys move the focused pane's edges.
    pub resize_mode: bool,
    /// The pointer is currently the hand (OSC 22).
    pointer_hand: bool,
    pub preview: Preview,
    pub side: Side,
    pub page: Page,
    /// The palette (and undo depth) before the picker's first change, for
    /// Esc to put back. Dropped once the keyboard is back in the list.
    edit_session: Option<(Palette, usize)>,
    /// Three columns (false: the preview is a row underneath; no font sample).
    pub wide: bool,
    /// The colour picker (pane 5) is on screen.
    pub picker_visible: bool,
    /// The font sample (pane 3) is on screen.
    pub sample_visible: bool,
    /// The key hints in the status line, and the previews (Main: nvim and
    /// the shell; Fonts page: the preview pane), for this session.
    pub hints_visible: bool,
    pub main_preview_visible: bool,
    pub fonts_preview_visible: bool,
    /// The keyboard is on the Palette pane's dropdown row, or on the page
    /// tabs (F1 / F2 / F3).
    pub palette_menu_focused: bool,
    pub page_tabs_focused: bool,
    /// Cursor in the colour list (group and colour rows); `sel` is the colour.
    pub row_sel: usize,
    /// Open colour groups (indexes into GROUPS).
    pub expanded_groups: std::collections::HashSet<usize>,
    pub fonts: FontsTab,
    /// Draws images in the terminal (kitty graphics); None in tests.
    pub picker: Option<Picker>,
    pub sample_image: Option<StatefulProtocol>,
    /// The sample's size while resizing (re-encoded once it holds still).
    pub sample_settle: crate::settle::Settle,
    /// Previews fetched ahead of time (settings: fonts-page.prefetch).
    pub prefetch: crate::prefetch::Prefetch,
    /// When an image's size settles and the screen should be drawn again.
    pub redraw_at: Option<Instant>,
    pub sample_error: Option<String>,
    pub keymap: Keymap,
    /// Where keymaps.json and settings.json are (for saving).
    pub config_dir: std::path::PathBuf,
    /// Every keymap preset (listed on the Settings page).
    pub presets: Vec<Keymap>,
    pub settings: crate::config::Settings,
    /// The Download fonts page.
    pub dl: crate::fonts_dl::DlPage,
    /// The Settings page's tabs and selections.
    pub settings_page: crate::settings::SettingsPage,
    /// Keys of a sequence typed so far (e.g. the leader), waiting for the rest.
    pub pending_keys: Vec<Key>,
    /// What the Open / Save as pop-up is for.
    pub overlay_for: For,
    /// The palette list (For::Palette): each entry's palette, for previewing
    /// it while it's highlighted, and the palette from before the list opened.
    pub open_palettes: Vec<Option<Palette>>,
    pub open_original: Option<Palette>,
    /// Where the Palette pane was last drawn (the palette list hangs from it).
    pub palette_pane: Option<Rect>,
    /// The theme: a name for the palette plus the font preset.
    pub themes: ThemeStore,
    pub theme: String,
    /// Not in themes.json yet.
    pub theme_new: bool,
    /// The theme's parts as saved (None: never saved).
    pub theme_saved: Option<crate::theme::Theme>,
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
        let system_palette = active.as_ref().and_then(|n| store.load(n).ok());
        // a name on the command line: a theme, or a palette
        let named = match &start {
            Start::Named(n) => Some(n.clone()),
            _ => None,
        };
        let start_new = matches!(start, Start::New);
        let (name, palette, is_new) = match start {
            Start::Named(n) => match store.load(&n) {
                Ok(p) => (n, p, false),
                Err(e)
                    if !ThemeStore {
                        dir: store.dir.clone(),
                    }
                    .load_all()
                    .iter()
                    .any(|(t, _)| *t == n) =>
                {
                    return Err(e);
                }
                Err(_) => match active.as_deref().map(|n| (n, store.load(n))) {
                    Some((n, Ok(p))) => (n.to_string(), p, false),
                    _ => ("untitled".to_string(), Palette::blank(), true),
                },
            },
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
            hints_visible: config.settings.layout.key_hints != "hidden",
            main_preview_visible: config.settings.layout.main_preview != "hidden",
            fonts_preview_visible: config.settings.layout.fonts_preview != "hidden",
            palette_menu_focused: false,
            page_tabs_focused: false,
            row_sel: 0,
            edit_session: None,
            wide: true,
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
            sample_settle: Default::default(),
            prefetch: Default::default(),
            redraw_at: None,
            sample_error: None,
            overlay_for: For::Palette,
            open_palettes: Vec::new(),
            open_original: None,
            palette_pane: None,
            themes: ThemeStore {
                dir: store.dir.clone(),
            },
            theme: String::new(),
            theme_new: true,
            theme_saved: None,
            keymap: config.keymap,
            presets: config.presets,
            config_dir: config.dir.clone(),
            settings: config.settings.clone(),
            settings_page: Default::default(),
            dl: Default::default(),
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
            system_palette,
            terminal,
            message: None,
            message_at: None,
            notice: None,
            overlay: Overlay::None,
            hits: Default::default(),
            pointer_hand: false,
            sizes: config.settings.layout.sizes.clone(),
            borders: Vec::new(),
            dragging: None,
            resize_mode: false,
            quit: false,
        };
        app.load_picker();
        app.fonts.filters = app.settings.font_list.filters();
        app.fonts.unfold_current = app.settings.fonts.list_opens_to_current_font;
        app.fonts.role_filters = app.settings.fonts.role_filters();
        app.fonts.refresh_installed();
        // palettes.json / fonts.json: version 1 files become version 2 (once;
        // the old file goes to the history)
        for (what, result) in [
            ("palettes.json", app.store.migrate()),
            ("fonts.json", app.fonts.store.migrate()),
        ] {
            match result {
                Ok(true) => app.info(format!(
                    "{what} updated to file format version 2 (the old file is in the history)"
                )),
                Ok(false) => {}
                Err(e) => app.error(format!("{what}: {e:#}")),
            }
        }
        if start_new {
            app.new_theme();
            app.message = None;
        } else {
            app.init_theme(named.as_deref());
        }
        app.sync_row();
        if !config.warnings.is_empty() {
            app.error(format!("Settings: {}", config.warnings.join("; ")));
        }
        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut Term) -> Result<()> {
        let mut redraw = true;
        while !self.quit {
            // nvim in the preview re-reads this whenever it changes
            self.preview.sync_draft(&self.palette);
            if self.page == Page::Main && self.sample_visible {
                self.update_sample();
            }
            // downloads: the catalogue, progress, finished installs
            if self.poll_downloads() {
                redraw = true;
            }
            if self.poll_prefetch() {
                redraw = true;
            }
            if self.expire_message() {
                redraw = true;
            }
            // an image waiting for its pane's size to settle
            if self.redraw_at.is_some_and(|t| Instant::now() >= t) {
                redraw = true;
            }
            if self.preview.take_dirty() || redraw {
                terminal.draw(|f| crate::ui::draw(f, self))?;
                self.update_pointer();
                redraw = false;
            }
            // Short poll: the preview's programs can print at any time.
            if event::poll(Duration::from_millis(25))? {
                match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => self.on_key(k),
                    Event::Mouse(m) => self.on_mouse(m),
                    Event::Paste(s) => {
                        self.on_paste(&s);
                        self.sync_palette_preview();
                    }
                    _ => {}
                }
                redraw = true;
            }
        }
        Ok(())
    }

    /// The mouse pointer is a hand over anything clickable (OSC 22; Ghostty
    /// supports it, other terminals ignore it).
    fn update_pointer(&mut self) {
        let hand = self.hits.hovered().is_some();
        if hand != self.pointer_hand {
            self.pointer_hand = hand;
            set_pointer(if hand { "pointer" } else { "default" });
        }
    }

    pub fn key(&self) -> &'static str {
        self.keys[self.sel]
    }

    /// Unsaved palette edits (not counting a palette previewed from the
    /// palette list).
    pub fn dirty(&self) -> bool {
        self.is_new || self.open_original.as_ref().unwrap_or(&self.palette) != &self.saved
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
            Some(Ok(img)) => {
                self.sample_image = self.picker.as_ref().map(|p| p.new_resize_protocol(img));
                self.sample_error = None;
            }
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
            self.overlay_for = For::Fonts;
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
                    self.notify_saved(format!("Saved and applied \"{name}\""));
                }
                Err(e) => self.error(format!("Apply failed: {e:#}")),
            },
            Ok(()) => self.notify_saved(format!("Saved \"{name}\"")),
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
                self.overlay_for = For::Fonts;
                self.overlay = Overlay::Open {
                    filter: String::new(),
                    sel,
                    entries,
                };
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    pub fn info(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), false));
        self.message_at = Some(Instant::now());
    }

    pub fn error(&mut self, s: impl Into<String>) {
        self.message = Some((s.into(), true));
        self.message_at = None;
    }

    /// A save confirmation: in the status line like any message, or in a
    /// box (settings.messages.saved).
    pub fn notify_saved(&mut self, s: impl Into<String>) {
        if self.settings.messages.saved_in_popup() {
            self.message = None;
            self.notice = Some(s.into());
        } else {
            self.info(s);
        }
    }

    /// Takes an info message away once it has been shown long enough.
    /// Returns whether it went.
    fn expire_message(&mut self) -> bool {
        let secs = self.settings.messages.seconds;
        match self.message_at {
            Some(at) if secs > 0 && at.elapsed() >= Duration::from_secs(secs.into()) => {
                self.message = None;
                self.message_at = None;
                true
            }
            _ => false,
        }
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
        // the first change while in the picker: remember the palette, so
        // Esc can put it back
        if self.edit_session.is_none() && self.focus != Focus::List {
            self.edit_session = Some((self.palette.clone(), self.undo.len()));
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
        self.overlay_for = For::Palette;
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

    pub(crate) fn save_as(&mut self, name: &str, then_apply: bool) {
        match self.store.save(name, &self.palette) {
            Ok(()) => {
                self.name = name.to_string();
                self.is_new = false;
                self.saved = self.palette.clone();
                if then_apply {
                    self.apply_saved();
                } else {
                    self.notify_saved(format!("Saved \"{name}\""));
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
                self.system_palette = Some(self.palette.clone());
                self.notify_saved(format!("Saved and applied \"{name}\""));
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

    pub(crate) fn reset_history(&mut self) {
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
            Pending::NewTheme | Pending::OpenTheme(_) => self.theme_dirty(),
            Pending::Quit => self.theme_dirty() || self.settings_page.draft.is_some(),
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
            Pending::NewTheme => self.new_theme(),
            Pending::OpenTheme(n) => self.open_theme(&n),
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
        // themes (and quitting): the whole theme, parts included
        if matches!(p, Pending::Quit | Pending::NewTheme | Pending::OpenTheme(_)) && !self.theme_new
        {
            if self.theme_dirty() {
                self.theme_save(false);
            }
            if matches!(p, Pending::Quit) && self.settings_page.draft.is_some() {
                self.settings_save();
            }
            return;
        }
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
        if matches!(p, Pending::Quit) && self.settings_page.draft.is_some() {
            self.settings_save();
        }
    }

    fn open_overlay(&mut self) {
        self.overlay_for = For::Palette;
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
                // each palette, shown while it's highlighted
                self.open_palettes = all
                    .values()
                    .map(|v| v.as_object().and_then(|o| Palette::from_json(o).ok()))
                    .collect();
                self.open_original = Some(self.palette.clone());
                self.overlay = Overlay::Open {
                    filter: String::new(),
                    sel,
                    entries,
                };
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    /// The palette list shows the highlighted palette everywhere (the
    /// colour list, the preview windows, the sample); closing it any way
    /// but choosing puts back the palette from before.
    fn sync_palette_preview(&mut self) {
        let Some(original) = &self.open_original else {
            return;
        };
        match &self.overlay {
            Overlay::Open {
                filter,
                sel,
                entries,
            } if self.overlay_for == For::Palette => {
                let shown = open_matches(entries, filter)
                    .get(*sel)
                    .and_then(|&i| self.open_palettes.get(i).cloned().flatten());
                self.palette = shown.unwrap_or_else(|| original.clone());
            }
            _ => self.end_palette_preview(),
        }
    }

    /// Back to the palette from before the palette list opened.
    fn end_palette_preview(&mut self) {
        if let Some(p) = self.open_original.take() {
            self.palette = p;
        }
        self.open_palettes.clear();
    }

    // ----- input -----------------------------------------------------------

    /// Number keys jump between the numbered panes: ¹ font roles, ² font
    /// list, ³ font sample, ⁴ colour list, ⁵ colour picker, ⁶ nvim, ⁷ shell
    /// (or yazi). 6 and 7 give that preview window the keyboard; F12 hands it
    /// back. A hidden picker or sample is shown.
    /// Moving off the top (`up`) or bottom of the page: the keyboard goes to
    /// the page tabs if they're that way (settings: layout.tabs). True if so.
    pub fn focus_page_tabs(&mut self, up: bool) -> bool {
        if up == self.settings.layout.tabs_on_top() {
            self.page_tabs_focused = true;
        }
        self.page_tabs_focused
    }

    /// Keys with the keyboard on the page tabs: left / right change pages
    /// (the keyboard stays on the tabs); back towards the page, Enter or Esc
    /// return to it. True if handled here.
    fn page_tabs_action(&mut self, a: Action) -> bool {
        const PAGES: [Page; 3] = [Page::Main, Page::Downloads, Page::Settings];
        let top = self.settings.layout.tabs_on_top();
        let i = PAGES.iter().position(|p| *p == self.page).unwrap_or(0);
        match a {
            Action::Left | Action::TabPrev => self.page = PAGES[(i + 2) % 3],
            Action::Right | Action::TabNext => self.page = PAGES[(i + 1) % 3],
            Action::Down if top => self.page_tabs_focused = false,
            Action::Up if !top => self.page_tabs_focused = false,
            Action::Confirm | Action::Back => self.page_tabs_focused = false,
            // page keys, quitting and help work as anywhere
            Action::Quit
            | Action::Help
            | Action::PageMain
            | Action::PageDownloads
            | Action::PageSettings
            | Action::ToggleHints
            | Action::TogglePreview => return false,
            _ => {}
        }
        true
    }

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
                    _ if !self.wide => {} // not drawn at this width
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
                    self.leave_picker(true); // only Enter keeps a change
                }
            }
            5 => {
                self.preview.focus = None;
                self.open_picker();
            }
            6 | 7 => {
                // jumping to a hidden preview shows it again
                if !self.main_preview_visible {
                    self.main_preview_visible = true;
                    self.preview.rects.clear();
                }
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
                .filter(|&p| p != 3 || (self.sample_visible && self.wide))
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

    /// Back to the colour list from the picker: Esc (`discard`) puts back what
    /// the colours were when you started changing them, Enter keeps them. The
    /// picker hides again
    /// unless Settings says it stays open.
    fn leave_picker(&mut self, discard: bool) {
        if discard
            && let Some((before, undo_len)) = self.edit_session.take()
            && before != self.palette
        {
            self.palette = before;
            self.undo.truncate(undo_len);
            self.redo.clear();
            self.last_edit = None;
            self.load_picker();
            self.info(format!("Discarded the change to {}", self.key()));
        }
        self.edit_session = None;
        self.focus = Focus::List;
        if self.settings.colours.picker_hides_on_esc() {
            self.picker_visible = false;
        }
    }

    /// The mouse is over this pane (and no pop-up or drag is in the way).
    pub fn pane_hovered(&self, area: Rect) -> bool {
        self.hits.over(area)
            && self.dragging.is_none()
            && matches!(self.overlay, Overlay::None | Overlay::CopyFrom { .. })
    }

    /// The keymap contexts for the pane with the keyboard, most specific first.
    pub fn contexts(&self) -> [Context; 3] {
        use crate::fonts::FontFocus;
        if self.resize_mode {
            return [Context::Resize; 3];
        }
        match self.page {
            Page::Main => {}
            Page::Settings => return [Context::Settings, Context::Global, Context::Global],
            Page::Downloads => return [Context::Downloads, Context::Global, Context::Global],
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

    // ----- resizing --------------------------------------------------------

    /// Sets the size share a border controls.
    fn set_share(&mut self, id: crate::hit::BorderId, share: f64) {
        use crate::hit::BorderId as B;
        let s = &mut self.sizes;
        let field = match id {
            B::FontsColours => &mut s.fonts_width,
            B::ColoursPreview => &mut s.colours_width,
            B::ListSample => &mut s.sample_height,
            B::ListPicker => &mut s.picker_height,
            B::PreviewSplit => &mut s.preview_split,
            B::TopPreview => &mut s.narrow_preview_height,
            B::NarrowFontsColours => &mut s.narrow_fonts_width,
        };
        // keep a little away from the ends; the layout's minimums do the rest
        *field = share.clamp(0.02, 0.98);
    }

    /// Moves a border to `pos` (a column for vertical borders, a row otherwise).
    fn move_border(&mut self, id: crate::hit::BorderId, pos: u16) {
        if let Some(b) = self.borders.iter_mut().find(|b| b.id == id) {
            let share = b.share_at(pos);
            // remember where it went, so quick repeats add up before the next draw
            if b.vertical {
                b.hit.x = pos
            } else {
                b.hit.y = pos
            }
            self.set_share(id, share);
        }
    }

    /// The border resize mode moves for the focused pane: its side edge
    /// (`vertical`) or its lower edge.
    fn border_for_pane(&self, vertical: bool) -> Option<crate::hit::BorderId> {
        use crate::hit::BorderId as B;
        let pane = self.current_pane();
        let id = match (self.wide, vertical, pane) {
            (true, true, 1..=3) => B::FontsColours,
            (true, true, _) => B::ColoursPreview,
            (true, false, 1..=3) => B::ListSample,
            (true, false, 4 | 5) => B::ListPicker,
            (true, false, _) => B::PreviewSplit,
            (false, true, 6 | 7) => B::PreviewSplit,
            (false, true, _) => B::NarrowFontsColours,
            (false, false, 4 | 5) if self.picker_visible => B::ListPicker,
            (false, false, _) => B::TopPreview,
        };
        self.borders.iter().any(|b| b.id == id).then_some(id)
    }

    /// Resize-mode keys: h/l move the focused pane's side edge, j/k its lower
    /// edge, by one cell (capitals: five); = resets; Esc / Enter leave.
    fn run_resize(&mut self, a: Action) {
        let (dx, dy, n): (i32, i32, i32) = match a {
            Action::Left => (-1, 0, 1),
            Action::Right => (1, 0, 1),
            Action::Up => (0, -1, 1),
            Action::Down => (0, 1, 1),
            Action::LeftBig => (-1, 0, 5),
            Action::RightBig => (1, 0, 5),
            Action::UpBig => (0, -1, 5),
            Action::DownBig => (0, 1, 5),
            Action::ResetSizes => {
                self.sizes = self.settings.layout.sizes.clone();
                return;
            }
            Action::Back | Action::Confirm | Action::ResizeMode => {
                self.resize_mode = false;
                return;
            }
            _ => return,
        };
        let vertical = dx != 0;
        let Some(id) = self.border_for_pane(vertical) else {
            return;
        };
        let b = self.borders.iter().find(|b| b.id == id).copied().unwrap();
        let pos = (b.pos() as i32 + (dx + dy) * n).max(0) as u16;
        self.move_border(id, pos);
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
        let copying = matches!(self.overlay, Overlay::CopyFrom { .. });
        if self.palette_menu_focused && !copying {
            match a {
                Action::Down | Action::Back => self.palette_menu_focused = false,
                Action::Confirm | Action::Right => self.open_overlay(),
                Action::Up => {
                    self.focus_page_tabs(true);
                }
                _ => {}
            }
            return;
        }
        if !copying {
            match a {
                Action::Up if self.row_sel == 0 => {
                    self.palette_menu_focused = true;
                    return;
                }
                Action::Down
                    if self.row_sel as i32 >= last
                        && !self.picker_visible
                        && self.focus_page_tabs(false) =>
                {
                    return;
                }
                _ => {}
            }
        }
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
        self.handle_key(k);
        self.sync_palette_preview();
        self.end_session_outside_picker();
    }

    /// Only Enter keeps a picker change: leaving the picker any other way
    /// (Esc, another pane or column, a preview window, a click elsewhere,
    /// hiding it) puts the colours back.
    fn end_session_outside_picker(&mut self) {
        let left =
            self.focus == Focus::List || self.side != Side::Colours || self.preview.focus.is_some();
        if left && self.edit_session.is_some() {
            self.leave_picker(true);
        }
    }

    fn handle_key(&mut self, k: KeyEvent) {
        use crate::fonts::FontFocus;
        let key = Key::from_event(k);
        // A save confirmation box: any key closes it (Ctrl+C still goes on
        // to quit).
        if self.notice.take().is_some()
            && key != Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
        {
            return;
        }
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
        // Ctrl+C quits (asking first about unsaved changes), unless the
        // keymap uses it here (Emacs's leader, copying in VS Code)
        if key == Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL)
            && self.pending_keys.is_empty()
            && !matches!(self.overlay, Overlay::Capture { .. })
            && (!matches!(self.overlay, Overlay::None | Overlay::CopyFrom { .. })
                || self.keymap.lookup(&self.contexts(), &[key]) == Lookup::None)
        {
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
            if self.page == Page::Downloads && self.dl.searching {
                return self.dl_search_key(k);
            }
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
        if self.resize_mode {
            return self.run_resize(a);
        }
        if let Overlay::CopyFrom { target } = self.overlay {
            return self.run_copy_from(target, a);
        }
        if a == Action::UndoConfig {
            return self.undo_config();
        }
        if self.page_tabs_focused && self.page_tabs_action(a) {
            return;
        }
        let fonts = self.side == Side::Fonts;
        let page_action = matches!(
            a,
            Action::Quit
                | Action::Help
                | Action::PageMain
                | Action::PageDownloads
                | Action::PageSettings
                | Action::ToggleHints
                | Action::TogglePreview
        );
        if self.page == Page::Settings && !page_action {
            return self.run_settings(a);
        }
        if self.page == Page::Downloads && !page_action {
            return self.run_downloads(a);
        }
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
            Action::ToggleHints => self.hints_visible = !self.hints_visible,
            Action::TogglePreview => match self.page {
                Page::Main => {
                    self.main_preview_visible = !self.main_preview_visible;
                    // a hidden window can't keep the keyboard
                    if !self.main_preview_visible {
                        self.preview.focus = None;
                    }
                }
                Page::Downloads => self.fonts_preview_visible = !self.fonts_preview_visible,
                Page::Settings => self.info("This page has no preview"),
            },
            Action::ToggleSample => {
                use crate::fonts::FontFocus;
                self.sample_visible = !self.sample_visible;
                if !self.sample_visible && self.fonts.focus == FontFocus::Sample {
                    self.fonts.focus = FontFocus::Slots;
                }
            }
            Action::ResizeMode => self.resize_mode = true,
            Action::PaneLeft => self.move_pane(-1, 0),
            Action::PaneRight => self.move_pane(1, 0),
            Action::PaneUp => self.move_pane(0, -1),
            Action::PaneDown => self.move_pane(0, 1),
            Action::SavePalette => self.save(false),
            Action::SaveFonts => self.fonts_save(false),
            Action::ApplyPalette => self.apply(),
            Action::ApplyFonts => self.fonts_apply(),
            // s / S / a / o / n act on the theme (palette and fonts together)
            Action::Save => self.theme_save(false),
            Action::SaveAs => {
                self.overlay_for = For::Theme;
                self.overlay = Overlay::SaveAs {
                    input: self.theme.clone(),
                    overwrite: false,
                    then_apply: false,
                };
            }
            Action::Apply => self.theme_apply(),
            Action::Open => self.theme_open_overlay(),
            Action::New => self.guard(Pending::NewTheme),
            // a part of the theme: another palette or font preset, or a new one
            Action::OpenPalette => self.open_overlay(),
            Action::OpenFonts => self.fonts_open_overlay(),
            Action::NewPalette => self.guard(Pending::New),
            Action::NewFonts => self.guard(Pending::NewFonts),
            Action::Panel(n) => self.focus_pane(n),
            // Tab in the fonts column: roles → list → sample → roles
            Action::FocusNext | Action::FocusPrev if fonts => {
                let order: Vec<u8> = [1, 2, 3]
                    .into_iter()
                    .filter(|&p| p != 3 || (self.sample_visible && self.wide))
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
            Action::FillFamily if fonts && self.fonts.focus == FontFocus::List => {
                match self.fonts.fill_from_family() {
                    Ok(what) => self.apply_picked_fonts(what),
                    Err(e) => self.info(e),
                }
            }
            // the top of the roles, the bottom of the font list (without the
            // sample under it): on to the page tabs, when they're that way
            Action::Up
                if fonts
                    && self.fonts.focus == FontFocus::Slots
                    && self.fonts.slot == 0
                    && self.focus_page_tabs(true) => {}
            Action::Down
                if fonts
                    && self.fonts.focus == FontFocus::List
                    && !(self.sample_visible && self.wide)
                    && self.fonts.list_sel + 1 >= self.fonts.rows().len()
                    && self.focus_page_tabs(false) => {}
            _ if fonts => {
                let role = self.fonts.slot;
                self.fonts.act(a);
                if self.fonts.picked {
                    let what = format!(
                        "{}: {}",
                        crate::fonts::ROLES[role],
                        self.fonts.preset.fonts[role].family
                    );
                    self.apply_picked_fonts(what);
                }
            }
            _ => self.run_colours(a),
        }
    }

    /// A font was picked (or a family filled the roles): save the theme's
    /// font preset and apply it straight away.
    fn apply_picked_fonts(&mut self, what: String) {
        self.fonts.picked = false;
        let name = if self.fonts.is_new {
            self.theme.clone()
        } else {
            self.fonts.name.clone()
        };
        let result = self
            .fonts
            .save_as(&name)
            .and_then(|_| self.fonts.store.apply(&name));
        match result {
            Ok(()) => {
                self.fonts.active = Some(name.clone());
                self.info(format!("{what}: saved to \"{name}\" and applied"));
            }
            Err(e) => self.error(format!("{what}: {e:#}")),
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
            Action::Back if self.focus != Focus::List => self.leave_picker(true),
            Action::Confirm if self.focus != Focus::List => self.leave_picker(false),
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
                    self.leave_picker(false);
                }
                None => self.error(format!("Not a colour: {} (use #rrggbb)", self.hex_input)),
            },
            KeyCode::Esc => {
                self.load_picker();
                self.leave_picker(true);
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
            Overlay::Menu { menu, mut sel, at } => {
                let last = crate::fonts_preview::menu_items(menu).len() - 1;
                let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
                match k.code {
                    KeyCode::Esc => return,
                    KeyCode::Enter => return self.choose_menu(menu, sel),
                    KeyCode::Down | KeyCode::Char('j') if !ctrl => sel = (sel + 1).min(last),
                    KeyCode::Char('n') if ctrl => sel = (sel + 1).min(last),
                    KeyCode::Up | KeyCode::Char('k') if !ctrl => sel = sel.saturating_sub(1),
                    KeyCode::Char('p') if ctrl => sel = sel.saturating_sub(1),
                    KeyCode::Home | KeyCode::Char('g') => sel = 0,
                    KeyCode::End | KeyCode::Char('G') => sel = last,
                    _ => {}
                }
                Overlay::Menu { menu, sel, at }
            }
            // Settings page pop-ups edit the draft (see draft.rs)
            Overlay::ChooseKey { row, sel, original } => {
                self.with_draft(|app| app.choose_key_key(row, sel, original, k));
                return;
            }
            Overlay::Capture { what, keys } => {
                // capture_key sets the overlay itself
                self.with_draft(|app| app.capture_key(what, keys, k));
                return;
            }
            Overlay::Prompt {
                title,
                mut input,
                purpose,
            } => match k.code {
                KeyCode::Esc => Overlay::None,
                KeyCode::Enter => {
                    // finish_prompt may open something else
                    self.with_draft(|app| app.finish_prompt(purpose, input));
                    return;
                }
                KeyCode::Backspace => {
                    input.pop();
                    Overlay::Prompt {
                        title,
                        input,
                        purpose,
                    }
                }
                KeyCode::Char(c) => {
                    input.push(c);
                    Overlay::Prompt {
                        title,
                        input,
                        purpose,
                    }
                }
                _ => Overlay::Prompt {
                    title,
                    input,
                    purpose,
                },
            },
            Overlay::Ask { question, then } => match k.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.with_draft(|app| app.run_deferred(then));
                    return;
                }
                KeyCode::Esc | KeyCode::Char('n') => Overlay::None,
                _ => Overlay::Ask { question, then },
            },
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
                    if self.overlay_for == For::Theme {
                        let exists = self.themes.get(&name).is_some();
                        return self.overlay = if name.is_empty() {
                            Overlay::SaveAs {
                                input,
                                overwrite,
                                then_apply,
                            }
                        } else if exists && name != self.theme && !overwrite {
                            self.error(format!(
                                "The theme \"{name}\" exists. Enter again to overwrite it"
                            ));
                            Overlay::SaveAs {
                                input,
                                overwrite: true,
                                then_apply,
                            }
                        } else {
                            self.theme_save_as(&name, then_apply);
                            std::mem::replace(&mut self.overlay, Overlay::None)
                        };
                    }
                    let (exists, current) = if self.overlay_for == For::Fonts {
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
                    } else if self.overlay_for == For::Fonts
                        && (!exists || name == current || overwrite)
                    {
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
                            // the unsaved-changes check sees the real palette
                            self.end_palette_preview();
                            self.guard(match self.overlay_for {
                                For::Fonts => Pending::OpenFonts(name),
                                For::Palette => Pending::Open(name),
                                For::Theme => Pending::OpenTheme(name),
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
        self.handle_mouse(m);
        self.sync_palette_preview();
        self.end_session_outside_picker();
    }

    fn handle_mouse(&mut self, m: MouseEvent) {
        use crate::hit::Target as T;
        let (x, y) = (m.column, m.row);
        self.hits.mouse = Some((x, y));
        // a save confirmation box: a click anywhere closes it
        if self.notice.is_some() {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                self.notice = None;
            }
            return;
        }
        let target = self.hits.at(x, y);
        // With a pop-up open (other than "copy from"), only the pop-up responds.
        let popup = !matches!(self.overlay, Overlay::None | Overlay::CopyFrom { .. });
        if popup && !matches!(target, Some((_, T::OverlayRow(_) | T::ConfirmChoice(_)))) {
            // a click outside a dropdown closes it
            if matches!(self.overlay, Overlay::Menu { .. })
                && matches!(m.kind, MouseEventKind::Down(_))
            {
                self.overlay = Overlay::None;
            }
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                let d = if m.kind == MouseEventKind::ScrollDown {
                    1
                } else {
                    -1
                };
                match target {
                    Some((_, T::ColourRow(_))) => self.move_row(d),
                    Some((_, T::FontRow(_))) => self.fonts.scroll(d),
                    Some((_, t @ (T::SettingsRow(_) | T::SettingsDiff))) => {
                        self.settings_scroll(t, d)
                    }
                    Some((_, T::DlRow(_))) => {
                        self.run_downloads(if d > 0 { Action::Down } else { Action::Up })
                    }
                    Some((_, T::DlPreview)) => self.preview_scroll(d),
                    _ => {}
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                // a click outside a preview window gives the keyboard back
                self.page_tabs_focused = false;
                if !matches!(target, Some((_, T::Preview(_)))) {
                    self.preview.focus = None;
                }
                if let Some((r, t)) = target {
                    self.click(r, t, x, y, false);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) if self.dragging.is_some() => {
                let id = self.dragging.unwrap();
                let vertical = self
                    .borders
                    .iter()
                    .find(|b| b.id == id)
                    .is_some_and(|b| b.vertical);
                self.move_border(id, if vertical { x } else { y });
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                // dragging works on the picker's controls only
                if let Some((r, t @ (T::Square | T::Hue | T::SliderH | T::SliderS | T::SliderL))) =
                    target
                {
                    self.click(r, t, x, y, true);
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.last_edit = None;
                self.dragging = None;
            }
            _ => {}
        }
    }

    /// A click (or a drag step) on a target at `r`.
    fn click(&mut self, r: Rect, t: crate::hit::Target, x: u16, y: u16, drag: bool) {
        use crate::fonts::FontFocus;
        use crate::hit::Target as T;
        let frac = |v: u16, start: u16, len: u16| {
            if len <= 1 {
                0.0
            } else {
                (v.saturating_sub(start) as f64 / (len - 1) as f64).clamp(0.0, 1.0)
            }
        };
        match t {
            T::PageTab(p) => self.page = p,
            T::Border(id) => self.dragging = Some(id),
            T::PaneTitle(n) => self.focus_pane(n),
            T::PaneBody(3) => self.focus_pane(3),
            T::PaneBody(_) => {}
            T::Preview(id) => {
                self.preview.mode = PreviewMode::Desktop;
                self.preview.focus = Some(id);
            }
            T::ColourRow(row) => {
                self.palette_menu_focused = false;
                let copying = matches!(self.overlay, Overlay::CopyFrom { .. });
                let again =
                    row == self.row_sel && self.focus == Focus::List && self.side == Side::Colours;
                self.side = Side::Colours;
                self.focus = Focus::List;
                self.row_sel = row;
                match self.colour_rows().get(row).copied() {
                    Some(ColourRow::Group { index, expanded }) if again => {
                        self.set_group_open(index, !expanded)
                    }
                    Some(ColourRow::Key(k)) => {
                        self.select_key(k);
                        if again && !copying {
                            self.open_picker();
                        }
                    }
                    _ => {}
                }
            }
            T::Square => {
                self.side = Side::Colours;
                self.focus = Focus::Square;
                let s = frac(x, r.x, r.width);
                let v = 1.0 - frac(y, r.y, r.height.max(2));
                self.set_hsv(self.hsv.0, s, v);
            }
            T::Hue => {
                self.side = Side::Colours;
                self.focus = Focus::Hue;
                let (_, s, v) = self.hsv;
                self.set_hsv(frac(y, r.y, r.height.max(2)) * 360.0, s, v);
            }
            T::SliderH => {
                self.side = Side::Colours;
                self.focus = Focus::SliderH;
                let (_, s, v) = self.hsv;
                self.set_hsv(frac(x, r.x, r.width) * 360.0, s, v);
            }
            T::SliderS | T::SliderL => {
                self.side = Side::Colours;
                self.focus = if t == T::SliderS {
                    Focus::SliderS
                } else {
                    Focus::SliderL
                };
                let (s, l) = self.hsl();
                let u = frac(x, r.x, r.width);
                if t == T::SliderS {
                    self.set_hsl(u, l)
                } else {
                    self.set_hsl(s, u)
                }
            }
            T::Hex if !drag => {
                self.side = Side::Colours;
                self.focus = Focus::Hex;
                self.hex_input = "#".into();
            }
            T::Hex => {}
            T::FontRole(i) => {
                self.side = Side::Fonts;
                self.fonts.click_role(i);
            }
            T::FontRow(i) => {
                self.side = Side::Fonts;
                let role = self.fonts.slot;
                self.fonts.click_row(i);
                if self.fonts.picked {
                    let what = format!(
                        "{}: {}",
                        crate::fonts::ROLES[role],
                        self.fonts.preset.fonts[role].family
                    );
                    self.apply_picked_fonts(what);
                }
            }
            T::FontSearch => {
                self.side = Side::Fonts;
                self.fonts.focus = FontFocus::Search;
            }
            T::Chip(c) => {
                self.side = Side::Fonts;
                self.fonts.click_chip(c);
            }
            T::OverlayRow(i) => {
                if let Overlay::ChooseKey { row, sel, original } = &self.overlay {
                    let (row, sel, original) = (*row, *sel, original.clone());
                    // a click previews the key; a second click keeps it
                    let key = crate::behaviour::theme_keys()[i];
                    if sel == i {
                        self.overlay = Overlay::None;
                    } else {
                        self.overlay = Overlay::ChooseKey {
                            row,
                            sel: i,
                            original,
                        };
                    }
                    self.with_draft(|app| app.set_colour_setting(row, key));
                } else if let Overlay::Open { sel, .. } = &mut self.overlay {
                    if *sel == i {
                        // a second click opens it, like Enter
                        self.on_overlay_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
                    } else {
                        *sel = i;
                    }
                } else if let Overlay::Menu { menu, .. } = self.overlay {
                    // one click chooses
                    self.choose_menu(menu, i);
                }
            }
            T::SettingsTab(_)
            | T::SettingsRow(_)
            | T::HistFile(_)
            | T::SettingsDiff
            | T::PresetRow(_) => self.settings_click(t),
            T::ThemeName => self.theme_open_overlay(),
            T::PaletteMenu => {
                self.focus_pane(4);
                self.palette_menu_focused = true;
                self.open_overlay();
            }
            T::DlTab(_) | T::DlRow(_) | T::DlChip(_) | T::DlSearch | T::DlButton(_) => {
                self.dl_click(t)
            }
            T::PreviewMenu(n) => self.open_menu(n, r),
            T::DlPreview => {}
            T::SettingsButton('s') => self.settings_save(),
            T::SettingsButton(_) => self.settings_discard(),
            T::ConfirmChoice(c) => {
                let code = if c == 'n' {
                    KeyCode::Esc
                } else {
                    KeyCode::Char(c)
                };
                self.on_overlay_key(KeyEvent::new(code, KeyModifiers::NONE));
            }
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

/// Sets the mouse pointer shape (OSC 22): "pointer" (hand) or "default".
pub fn set_pointer(shape: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]22;{shape}\x1b\\").and_then(|_| out.flush());
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

pub(crate) fn base64(data: &[u8]) -> String {
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
        std::fs::write(
            dir.join("palettes.json"),
            to_jq_string(&crate::store::data_file("palettes", m)),
        )
        .unwrap();
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
            // a folder inside the test's own temp store
            dir: store.dir.join("studio-config"),
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
    fn themes_save_open_and_fork() {
        let store = temp_store("save");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        let themes = || {
            ThemeStore { dir: dir.clone() }
                .load_all()
                .into_iter()
                .map(|(n, _)| n)
                .collect::<Vec<_>>()
        };
        // s on a theme that was never saved asks for its name
        press(&mut app, "#abcdef\n");
        assert!(app.theme_dirty());
        if app.theme_new {
            press(&mut app, "s");
            assert!(matches!(app.overlay, Overlay::SaveAs { .. }));
            press(&mut app, "Work\n");
        } else {
            // (made at start from the active parts) save as Work
            press(&mut app, "S");
            for _ in 0..app.theme.len() {
                app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
            }
            press(&mut app, "Work\n");
        }
        assert_eq!(app.theme, "Work");
        assert!(!app.theme_dirty() && !app.dirty());
        assert!(themes().contains(&"Work".to_string()));
        assert_eq!(
            Store { dir: dir.clone() }
                .load(&app.name)
                .unwrap()
                .get("bg0"),
            Rgb(0xab, 0xcd, 0xef)
        );
        // <leader>op swaps another palette into the theme: unsaved
        press(&mut app, " opTwo\n");
        assert_eq!(app.name, "Two");
        assert_eq!(app.palette.get("red"), Rgb(200, 0, 0));
        assert!(app.theme_dirty() && !app.dirty());
        assert!(render(&mut app, 160, 45).contains("Theme: Work ●"));
        // an edited palette asks before another is opened
        press(&mut app, "#111111\n");
        press(&mut app, " opOne\n");
        assert!(matches!(app.overlay, Overlay::Confirm(Pending::Open(_))));
        press(&mut app, "\x1b");
        assert_eq!(app.name, "Two");
        // save as a new name forks the edited palette under that name
        press(&mut app, "S");
        for _ in 0.."Work".len() {
            app.on_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        }
        press(&mut app, "Play\n");
        assert_eq!((app.theme.as_str(), app.name.as_str()), ("Play", "Play"));
        let names: Vec<String> = Store { dir: dir.clone() }
            .load_all()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert!(
            names.ends_with(&["Two".to_string(), "Play".to_string()]),
            "{names:?}"
        );
        assert_eq!(
            Store { dir: dir.clone() }.load("Two").unwrap().get("bg0"),
            Palette::blank().get("bg0"),
            "Two kept"
        );
        assert!(themes().ends_with(&["Work".to_string(), "Play".to_string()]));
        // o opens a theme: Work's palette comes back
        press(&mut app, "oWork\n");
        assert_eq!(app.theme, "Work");
        assert_ne!(app.name, "Play");
        // n: a new theme, a copy of this one
        press(&mut app, "n");
        assert!(app.theme_new && app.theme == "untitled");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn renders_at_full_size_and_warns_when_small() {
        let mut app = test_app(temp_store("draw"), Start::Named("One".into()));
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("Theme: "), "{screen}");
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
        press(&mut app, " 1 fs");
        assert!(!app.sample_visible);
        press(&mut app, " 4");
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
        press(&mut app, " 5");
        assert_eq!((app.side, app.focus), (Side::Colours, Focus::Square));
        assert!(app.picker_visible, "5 shows it");
        press(&mut app, " 4");
        assert_eq!(app.focus, Focus::List);
        assert!(!app.picker_visible, "leaving it hides it again");
        press(&mut app, " 1");
        assert_eq!(
            (app.side, app.fonts.focus),
            (Side::Fonts, crate::fonts::FontFocus::Slots)
        );
        press(&mut app, " 2");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::List);
        press(&mut app, " 3");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::Sample);
        press(&mut app, " 6");
        assert_eq!(app.preview.focus, Some(PaneId::Nvim));
        // inside a preview window digits are typed into it, F12 comes back out
        press(&mut app, "1");
        assert_eq!(app.preview.focus, Some(PaneId::Nvim));
        while app.preview.focus.is_some() {
            app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        }
        press(&mut app, " 4Y 7");
        assert_eq!(app.preview.focus, Some(PaneId::Yazi));
        app.on_key(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
        // <C-w>: across and down between the fonts and colour columns
        press(&mut app, " 1");
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "l");
        assert_eq!(app.current_pane(), 4);
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "h");
        app.on_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL));
        press(&mut app, "j");
        assert_eq!(app.current_pane(), 2);
        // digits still type into the hex box
        press(&mut app, " 4#123456\n");
        assert_eq!(app.palette.get("bg0"), Rgb(0x12, 0x34, 0x56));
        // titles carry the numbers
        press(&mut app, " 5");
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
        press(&mut app, " 1"); // the font roles
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
        // Enter on bar opens the list with bar's filter (Nerd Font): only A
        press(&mut app, "jj\n");
        assert_eq!(app.fonts.picking, Some(2));
        assert_eq!(app.fonts.rows().len(), 1);
        // n turns the filter off; G Enter picks C Mono (saved and applied:
        // font-switch.sh isn't in the temp folder, so applying reports it)
        press(&mut app, "nG\n");
        assert_eq!(app.fonts.picking, None);
        assert!(!app.fonts.filters.is_on('n'), "the user's filters are back");
        assert!(!app.fonts.dirty(), "saved straight away");
        press(&mut app, "ll");
        assert_eq!(app.fonts.preset.fonts[2].family, "C Mono");
        assert_eq!(app.fonts.preset.fonts[2].size, 11);
        // number keys: 2 opens the list, 1 goes back
        press(&mut app, " 2");
        assert_eq!(app.fonts.focus, crate::fonts::FontFocus::List);
        press(&mut app, " 1");
        // unsaved: opening another preset asks first
        press(&mut app, " ofBase\n");
        assert!(matches!(
            app.overlay,
            Overlay::Confirm(Pending::OpenFonts(_))
        ));
        press(&mut app, "\x1b");
        // <leader>sf saves the preset in place, in jq's format
        press(&mut app, " sf");
        assert!(!app.fonts.dirty());
        let text = std::fs::read_to_string(dir.join("fonts.json")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["version"], 2, "saved as version 2");
        assert_eq!(v["presets"]["Base"]["bar"]["family"], "C Mono");
        assert_eq!(text, to_jq_string(&v));
        // the palette tab didn't change, and its keys still work after F1
        assert!(!app.dirty());
        press(&mut app, " 5");
        assert_eq!((app.side, app.focus), (Side::Colours, Focus::Square));
        assert!(app.picker_visible);
        // the Fonts tab draws at every size (no image support in tests)
        let screen = render(&mut app, 160, 45);
        for t in ["¹Fonts", "²Installed fonts", "F2 Fonts", "Sample", "C Mono"] {
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
        press(&mut app, " 1"); // the fonts column
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
    fn esc_discards_picker_changes_enter_keeps_them() {
        let mut app = test_app(temp_store("discard"), Start::Named("One".into()));
        let before = app.palette.clone();
        // edit bg0 in the square, then Esc: everything as it was, nothing to undo
        press(&mut app, "\nKK");
        assert_ne!(app.palette, before);
        assert_ne!(
            app.palette.get("grey1"),
            before.get("grey1"),
            "derived keys followed"
        );
        press(&mut app, "\x1b");
        assert_eq!(app.palette, before);
        assert!(!app.dirty() && !app.picker_visible);
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, _)| m.contains("Discarded"))
        );
        press(&mut app, "u");
        assert_eq!(app.palette, before, "the discarded steps aren't undoable");
        // the same edit, then Enter: kept (and undoable)
        press(&mut app, "\nKK\n");
        assert_ne!(app.palette, before);
        assert!(app.dirty() && app.focus == Focus::List);
        let kept = app.palette.clone();
        // the hex box: Esc discards whatever the session changed
        press(&mut app, "\nJ#abcdef");
        press(&mut app, "\x1b");
        assert_eq!(app.palette, kept);
        // only Enter keeps: every other way out of the picker discards
        for leave in [" 4", " 1", " cp"] {
            press(&mut app, "\nJ");
            assert_ne!(app.palette, kept);
            press(&mut app, leave);
            assert_eq!(app.palette, kept, "left with {leave:?}");
            press(&mut app, " 4");
        }
        // the hex box: Tab past the last control (back to the list) discards too
        press(&mut app, "\n#abcdef\t");
        assert_eq!(app.palette, kept);
    }

    #[test]
    fn narrow_layout_drops_the_font_sample_and_preview_capture_is_magenta() {
        let mut app = test_app(temp_store("narrow"), Start::Named("One".into()));
        let wide = render(&mut app, 200, 50);
        assert!(wide.contains("Sample"));
        let narrow = render(&mut app, 137, 50);
        assert!(
            !narrow.contains("Sample"),
            "no sample under the narrow layout"
        );
        press(&mut app, " 3");
        assert_ne!(app.current_pane(), 3, "the hidden sample can't be focused");
        // while nvim has the keyboard, the preview frame is magenta
        press(&mut app, " 6");
        let mut t = Terminal::new(TestBackend::new(200, 50)).unwrap();
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = t.backend().buffer().clone();
        let magenta = (0..200)
            .flat_map(|x| (0..50).map(move |y| (x, y)))
            .any(|(x, y)| {
                buf[(x, y)].symbol() == "─" && buf[(x, y)].fg == ratatui::style::Color::Magenta
            });
        assert!(
            magenta,
            "preview frame drawn in magenta while capturing keys"
        );
    }

    fn mouse(app: &mut App, kind: MouseEventKind, x: u16, y: u16) {
        app.on_mouse(MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        });
    }

    /// Clicks the middle of wherever `t` was last drawn.
    fn click_on(app: &mut App, t: crate::hit::Target) {
        let r = app
            .hits
            .rect_of(t)
            .unwrap_or_else(|| panic!("{t:?} not on screen"));
        let (x, y) = (r.x + r.width / 2, r.y + r.height / 2);
        mouse(app, MouseEventKind::Moved, x, y);
        mouse(app, MouseEventKind::Down(MouseButton::Left), x, y);
        mouse(app, MouseEventKind::Up(MouseButton::Left), x, y);
    }

    #[test]
    fn hover_highlights_and_clicks_through_the_click_map() {
        use crate::hit::Target as T;
        let mut app = test_app(temp_store("mouse"), Start::Named("One".into()));
        let mut t = Terminal::new(TestBackend::new(200, 50)).unwrap();
        let mut draw = |app: &mut App| {
            t.draw(|f| crate::ui::draw(f, app)).unwrap();
            t.backend().buffer().clone()
        };
        draw(&mut app);
        // hovering a tab highlights it; clicking it opens the page
        let tab = app.hits.rect_of(T::PageTab(Page::Settings)).unwrap();
        mouse(&mut app, MouseEventKind::Moved, tab.x + 1, tab.y);
        let buf = draw(&mut app);
        // the default look: underlined, in the theme's yellow
        let cell = &buf[(tab.x + 1, tab.y)];
        assert!(
            cell.modifier.contains(ratatui::style::Modifier::UNDERLINED),
            "hover"
        );
        assert_eq!(cell.fg, app.theme_colour("yellow"), "hover");
        assert_eq!(app.hits.hovered(), Some(T::PageTab(Page::Settings)));
        click_on(&mut app, T::PageTab(Page::Settings));
        assert_eq!(app.page, Page::Settings);
        draw(&mut app);
        click_on(&mut app, T::PageTab(Page::Main));
        assert_eq!(app.page, Page::Main);
        draw(&mut app);
        // a group row: select, then open
        let backgrounds = app
            .colour_rows()
            .iter()
            .position(|r| matches!(r, ColourRow::Group { index: 1, .. }))
            .unwrap();
        click_on(&mut app, T::ColourRow(backgrounds));
        click_on(&mut app, T::ColourRow(backgrounds));
        assert!(app.expanded_groups.contains(&1));
        draw(&mut app);
        // a colour row twice opens the picker; dragging in the square edits
        click_on(&mut app, T::ColourRow(1)); // bg0
        click_on(&mut app, T::ColourRow(1));
        assert!(app.picker_visible && app.focus == Focus::Square);
        draw(&mut app);
        let before = app.palette.get("bg0");
        let sq = app.hits.rect_of(T::Square).unwrap();
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            sq.x + sq.width - 1,
            sq.y,
        );
        mouse(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            sq.x + sq.width - 2,
            sq.y,
        );
        assert_ne!(app.palette.get("bg0"), before);
        press(&mut app, "\n"); // keep it
        draw(&mut app);
        // a pane title focuses the pane
        click_on(&mut app, T::PaneTitle(1));
        assert_eq!(app.current_pane(), 1);
        // a preview window takes the keyboard
        click_on(&mut app, T::Preview(PaneId::Nvim));
        assert_eq!(app.preview.focus, Some(PaneId::Nvim));
        // clicking the studio again gives it back
        draw(&mut app);
        click_on(&mut app, T::PaneTitle(4));
        assert_eq!(app.preview.focus, None);
        // pop-up buttons: the unsaved-changes prompt's Esc
        press(&mut app, "q");
        assert!(matches!(app.overlay, Overlay::Confirm(_)));
        draw(&mut app);
        click_on(&mut app, T::ConfirmChoice('n'));
        assert!(matches!(app.overlay, Overlay::None));
        // tabs on top: their own first row
        app.settings.layout.tabs = "top".into();
        let buf = draw(&mut app);
        let row0: String = (0..40).map(|x| buf[(x, 0)].symbol().to_string()).collect();
        assert!(row0.contains("F1 Main"), "{row0}");
        let last: String = (0..60).map(|x| buf[(x, 49)].symbol().to_string()).collect();
        assert!(!last.contains("F1 Main"), "{last}");
    }

    #[test]
    fn borders_drag_and_resize_mode_moves_edges() {
        use crate::hit::{BorderId as B, Target as T};
        let mut app = test_app(temp_store("resize"), Start::Named("One".into()));
        render(&mut app, 200, 50);
        // drag the border between fonts and colours 10 columns right
        let b = app.hits.rect_of(T::Border(B::FontsColours)).unwrap();
        mouse(
            &mut app,
            MouseEventKind::Down(MouseButton::Left),
            b.x,
            b.y + 5,
        );
        assert_eq!(app.dragging, Some(B::FontsColours));
        mouse(
            &mut app,
            MouseEventKind::Drag(MouseButton::Left),
            b.x + 10,
            b.y + 5,
        );
        mouse(
            &mut app,
            MouseEventKind::Up(MouseButton::Left),
            b.x + 10,
            b.y + 5,
        );
        assert_eq!(app.dragging, None);
        render(&mut app, 200, 50);
        assert_eq!(
            app.hits.rect_of(T::Border(B::FontsColours)).unwrap().x,
            b.x + 10
        );
        // resize mode on the colour list: l moves its right edge, = resets, Esc leaves
        press(&mut app, " 4 r");
        assert!(app.resize_mode);
        let before = app.hits.rect_of(T::Border(B::ColoursPreview)).unwrap().x;
        press(&mut app, "lll");
        render(&mut app, 200, 50);
        assert_eq!(
            app.hits.rect_of(T::Border(B::ColoursPreview)).unwrap().x,
            before + 3
        );
        press(&mut app, "H");
        render(&mut app, 200, 50);
        assert_eq!(
            app.hits.rect_of(T::Border(B::ColoursPreview)).unwrap().x,
            before - 2
        );
        // nvim / shell split from the preview panes isn't reachable from the list; j
        // here moves the list/picker border, which only exists with the picker shown
        press(&mut app, "j");
        press(&mut app, "=");
        render(&mut app, 200, 50);
        assert_eq!(app.sizes, crate::config::Sizes::default());
        press(&mut app, "\x1b");
        assert!(!app.resize_mode);
        // keys work normally again
        press(&mut app, "j");
        assert_eq!(app.row_sel, 2);
    }

    #[test]
    fn extreme_sizes_render_at_every_window_size() {
        let mut app = test_app(temp_store("extreme"), Start::Named("One".into()));
        app.picker_visible = true;
        for share in [0.0, 0.02, 0.5, 0.98, 1.0] {
            app.sizes = crate::config::Sizes {
                fonts_width: share,
                colours_width: share,
                sample_height: share,
                picker_height: share,
                preview_split: share,
                narrow_preview_height: share,
                narrow_fonts_width: share,
            };
            for w in (90..=260).step_by(17) {
                for h in (20..=70).step_by(9) {
                    render(&mut app, w, h);
                }
            }
        }
    }

    #[test]
    fn layout_three_columns_then_preview_row_then_too_small() {
        use crate::ui::{Arrangement, arrange};
        let r = |w, h| Rect::new(0, 0, w, h);
        // wide: fonts | colours | preview, the picker hidden, the sample shown
        let Arrangement::Main(l) = arrange(
            r(200, 50),
            Page::Main,
            false,
            true,
            true,
            &crate::config::Sizes::default(),
        ) else {
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
        let Arrangement::Main(l) = arrange(
            r(200, 50),
            Page::Main,
            true,
            true,
            true,
            &crate::config::Sizes::default(),
        ) else {
            panic!()
        };
        let p = l.picker.unwrap();
        assert_eq!(p.x, l.colour_list.x);
        assert!(p.y > l.colour_list.y);
        // narrow (your portrait monitor): columns on top, preview row underneath
        let Arrangement::Main(l) = arrange(
            r(137, 60),
            Page::Main,
            true,
            true,
            true,
            &crate::config::Sizes::default(),
        ) else {
            panic!()
        };
        assert!(!l.wide);
        assert_eq!((l.preview.x, l.preview.width), (0, 137));
        assert!(l.preview.y > l.roles.y && l.preview.y > l.colour_list.y);
        assert_eq!(l.roles.y, l.colour_list.y, "fonts and colours side by side");
        // too small, and what's needed (less height when things are hidden)
        assert_eq!(
            arrange(
                r(90, 60),
                Page::Main,
                false,
                false,
                true,
                &crate::config::Sizes::default()
            ),
            Arrangement::TooSmall {
                need_w: 98,
                need_h: 30
            }
        );
        assert_eq!(
            arrange(
                r(137, 20),
                Page::Main,
                true,
                true,
                true,
                &crate::config::Sizes::default()
            ),
            Arrangement::TooSmall {
                need_w: 98,
                need_h: 35
            }
        );
        assert_eq!(
            arrange(
                r(160, 20),
                Page::Main,
                true,
                true,
                true,
                &crate::config::Sizes::default()
            ),
            Arrangement::TooSmall {
                need_w: 148,
                need_h: 24
            }
        );
        // other pages need little room
        assert!(matches!(
            arrange(
                r(60, 20),
                Page::Settings,
                true,
                true,
                true,
                &crate::config::Sizes::default()
            ),
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

    #[test]
    fn settings_changes_wait_for_save() {
        use crate::settings::KmRow;
        let store = temp_store("settings-keys");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        app.on_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        assert_eq!(app.page, Page::Settings);
        assert_eq!(app.contexts()[0], Context::Settings);
        let row_of = |app: &mut App, r: KmRow| {
            app.show_draft(|a| a.keymap_rows().iter().position(|x| *x == r).unwrap())
        };
        let draft_keys =
            |app: &mut App, c: Context, a: Action| app.show_draft(|x| x.keymap.keys_of(c, a));
        let on_disk =
            |dir: &std::path::Path| crate::config::load(&dir.join("studio-config"), false);
        let q = Key::parse_seq("Q", app.keymap.leader).unwrap();
        // Enter on "quit" records new keys: Q, then Enter keeps them, unsaved
        app.settings_page.km_sel = row_of(&mut app, KmRow::Binding(Context::Global, Action::Quit));
        press(&mut app, "\n");
        assert!(matches!(app.overlay, Overlay::Capture { .. }));
        press(&mut app, "Q");
        assert!(render(&mut app, 160, 45).contains("Q▏"));
        press(&mut app, "\n");
        assert_eq!(
            draft_keys(&mut app, Context::Global, Action::Quit),
            vec![q.clone()]
        );
        assert_ne!(
            app.keymap.keys_of(Context::Global, Action::Quit),
            vec![q.clone()],
            "not live yet"
        );
        assert_ne!(
            on_disk(&dir).keymap.keys_of(Context::Global, Action::Quit),
            vec![q.clone()]
        );
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("● 1 unsaved")
                && screen.contains("s Save")
                && screen.contains("Settings ●"),
            "{screen}"
        );
        // s saves: live and on disk
        press(&mut app, "s");
        assert!(app.settings_page.draft.is_none());
        assert_eq!(
            app.keymap.keys_of(Context::Global, Action::Quit),
            vec![q.clone()]
        );
        assert_eq!(
            on_disk(&dir).keymap.keys_of(Context::Global, Action::Quit),
            vec![q]
        );
        // binding pin to y (copy-hex in Colours) warns; D discards
        app.settings_page.km_sel = row_of(&mut app, KmRow::Binding(Context::Colours, Action::Pin));
        press(&mut app, "ay\n");
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, err)| *err && m.contains("is also")),
            "{:?}",
            app.message
        );
        assert_eq!(draft_keys(&mut app, Context::Colours, Action::Pin).len(), 2);
        press(&mut app, "D");
        assert!(app.settings_page.draft.is_none());
        assert_eq!(app.keymap.keys_of(Context::Colours, Action::Pin).len(), 1);
        // d unbinds, s saves; <leader>u undoes that save, again redoes
        press(&mut app, "ds");
        assert!(app.keymap.keys_of(Context::Colours, Action::Pin).is_empty());
        press(&mut app, " u");
        assert_eq!(app.keymap.keys_of(Context::Colours, Action::Pin).len(), 1);
        press(&mut app, " u");
        assert!(app.keymap.keys_of(Context::Colours, Action::Pin).is_empty());
        // Behaviour tab: the leader, by pressing it
        press(&mut app, "]");
        app.settings_page.bh_sel = 1;
        press(&mut app, "\n,");
        let comma = Key::new(KeyCode::Char(','), KeyModifiers::NONE);
        assert_eq!(app.show_draft(|a| a.keymap.leader), comma);
        assert_ne!(app.keymap.leader, comma, "not live yet");
        // quitting asks, and "save first" saves the settings
        press(&mut app, "Q"); // quit is Q now
        assert!(matches!(app.overlay, Overlay::Confirm(Pending::Quit)));
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Unsaved: ") && screen.contains("settings."),
            "{screen}"
        );
        press(&mut app, "s");
        assert!(app.quit);
        assert_eq!(app.keymap.leader, comma);
        assert_eq!(on_disk(&dir).keymap.leader, comma);
        assert_eq!(
            app.keymap.keys_of(Context::Global, Action::SavePalette),
            vec![Key::parse_seq(",sp", comma).unwrap()]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_behaviour_tab_edits_then_saves() {
        use crate::behaviour::{BhRow, behaviour_rows};
        let store = temp_store("settings-behaviour");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        app.page = Page::Settings;
        render(&mut app, 160, 45);
        click_on(&mut app, crate::hit::Target::SettingsTab(1));
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Page tabs") && screen.contains("‹ bottom ›"),
            "{screen}"
        );
        let rows = behaviour_rows();
        let on_disk =
            |dir: &std::path::Path| crate::config::load(&dir.join("studio-config"), false);
        app.settings_page.bh_sel = rows.iter().position(|r| *r == BhRow::Tabs).unwrap();
        press(&mut app, "l");
        assert_eq!(app.show_draft(|a| a.settings.layout.tabs.clone()), "top");
        assert_eq!(app.settings.layout.tabs, "bottom", "not live yet");
        // changing it back leaves nothing unsaved
        render(&mut app, 160, 45);
        let i = app.settings_page.bh_sel;
        click_on(&mut app, crate::hit::Target::SettingsRow(i));
        assert!(app.settings_page.draft.is_none());
        // again, then the Save button
        press(&mut app, "l");
        render(&mut app, 160, 45);
        click_on(&mut app, crate::hit::Target::SettingsButton('s'));
        assert_eq!(app.settings.layout.tabs, "top");
        assert_eq!(on_disk(&dir).settings.layout.tabs, "top");
        // the current pane sizes become the default when saved
        app.sizes.fonts_width = 0.4;
        app.settings_page.bh_sel = rows.iter().position(|r| *r == BhRow::SaveSizes).unwrap();
        press(&mut app, "\ns");
        assert_eq!(on_disk(&dir).settings.layout.sizes.fonts_width, 0.4);
        // the Discard button drops changes
        app.settings_page.bh_sel = rows.iter().position(|r| *r == BhRow::Tabs).unwrap();
        press(&mut app, "l");
        render(&mut app, 160, 45);
        click_on(&mut app, crate::hit::Target::SettingsButton('d'));
        assert!(app.settings_page.draft.is_none());
        assert_eq!(app.settings.layout.tabs, "top");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_history_tab_previews_restores_and_names_versions() {
        use crate::settings::DiffLine;
        let store = temp_store("settings-history");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        let before = app.palette.get("red");
        app.palette.set("red", Rgb(1, 2, 3));
        app.save(false);
        app.page = Page::Settings;
        render(&mut app, 160, 45);
        click_on(&mut app, crate::hit::Target::SettingsTab(2));
        // palettes.json in the file list
        app.settings_page.hist_file = crate::settings::FILES
            .iter()
            .position(|f| *f == "palettes.json")
            .unwrap();
        assert_eq!(app.settings_page.tab, crate::settings::Tab::History);
        let versions = app.versions();
        assert_eq!(versions.len(), 1);
        // red, and the colours derived from it
        assert!(
            versions[0].1.summary.starts_with("One: ") && versions[0].1.summary.contains(", red,")
        );
        // the preview: red as it is now → as it was
        let view = app.diff_view();
        assert!(view.lines.contains(&DiffLine::Changed {
            depth: 2,
            key: "red".into(),
            now: "#010203".into(),
            then: before.hex(),
        }));
        let (now, then) = view.palette.unwrap();
        assert!(
            now.contains(&("red".into(), Rgb(1, 2, 3))) && then.contains(&("red".into(), before))
        );
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Restore preview") && screen.contains("One: bg_visual_red"),
            "{screen}"
        );
        render(&mut app, 100, 30); // stacked
        // A shows everything
        press(&mut app, "A");
        assert!(app.diff_view().lines.len() > 10);
        // r asks, y restores (and what was there goes to the history)
        press(&mut app, "r");
        assert!(matches!(app.overlay, Overlay::Ask { .. }));
        render(&mut app, 160, 45);
        click_on(&mut app, crate::hit::Target::ConfirmChoice('y'));
        assert_eq!(app.palette.get("red"), before);
        assert!(!app.dirty());
        assert_eq!(
            Store { dir: dir.clone() }.load("One").unwrap().get("red"),
            before
        );
        assert_eq!(app.versions().len(), 2);
        // <leader>u undoes the restore
        press(&mut app, " u");
        assert_eq!(app.palette.get("red"), Rgb(1, 2, 3));
        // n names the current file; named versions come first, d deletes them
        press(&mut app, "nmy reds\n");
        let versions = app.versions();
        assert_eq!(versions[0].1.name.as_deref(), Some("my reds"));
        app.settings_page.hist_sel = 1;
        press(&mut app, "d");
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, _)| m.contains("Only named"))
        );
        app.settings_page.hist_sel = 0;
        press(&mut app, "dy");
        assert!(app.versions().iter().all(|(_, s)| s.name.is_none()));
        // other files: the list follows
        press(&mut app, "hj");
        assert_eq!(
            crate::settings::FILES[app.settings_page.hist_file],
            "fonts.json"
        );
        assert!(app.versions().is_empty());
        assert!(render(&mut app, 160, 45).contains("No earlier versions yet"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn ctrl_c_quits_unless_the_keymap_uses_it() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut app = test_app(temp_store("ctrl-c"), Start::Named("One".into()));
        app.palette.set("red", Rgb(1, 2, 3));
        app.on_key(ctrl_c);
        assert!(
            matches!(app.overlay, Overlay::Confirm(Pending::Quit)),
            "Vim: asks, then quits"
        );
        app.overlay = Overlay::None;
        // Emacs: C-c is the leader; C-c C-c applies
        app.keymap = Keymap::emacs();
        app.on_key(ctrl_c);
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.pending_keys.len(), 1);
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        app.pending_keys.clear();
        // VS Code: Ctrl+C copies the colour in the colours pane
        app.keymap = Keymap::vscode();
        app.side = Side::Colours;
        app.focus = Focus::List;
        app.on_key(ctrl_c);
        assert!(matches!(app.overlay, Overlay::None), "copied, not quit");
        std::fs::remove_dir_all(app.store.dir.clone()).unwrap();
    }

    #[test]
    fn hover_look_is_configurable_and_pane_borders_light_up() {
        use crate::behaviour::{BhRow, LookId, behaviour_rows};
        use crate::hit::{Target as T, Underline};
        use ratatui::style::{Color, Modifier};
        let store = temp_store("hover-look");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        // a system palette whose keys we know
        let mut sys = Palette::blank();
        sys.set("aqua", Rgb(1, 2, 3));
        sys.set("bg3", Rgb(4, 5, 6));
        sys.set("purple", Rgb(7, 8, 9));
        app.system_palette = Some(sys);
        assert_eq!(app.theme_colour("aqua"), Color::Rgb(1, 2, 3));
        // the mouse over the colour list (without the keyboard: focus is
        // yellow) lights its border in the pane colour
        app.side = Side::Fonts;
        let mut t = Terminal::new(TestBackend::new(200, 50)).unwrap();
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let row = app.hits.rect_of(T::ColourRow(1)).unwrap();
        mouse(&mut app, MouseEventKind::Moved, row.x + 2, row.y);
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = t.backend().buffer().clone();
        assert_eq!(
            buf[(row.x - 1, row.y)].fg,
            Color::Rgb(1, 2, 3),
            "pane border in aqua"
        );
        // Behaviour tab: highlight on, in bg3; curly underline; bold text
        app.page = Page::Settings;
        app.settings_page.tab = crate::settings::Tab::Behaviour;
        let rows = behaviour_rows();
        let at = |r: BhRow| rows.iter().position(|x| *x == r).unwrap();
        app.settings_page.bh_sel = at(BhRow::Highlight(LookId::Hover));
        press(&mut app, "l");
        app.settings_page.bh_sel = at(BhRow::HighlightColour(LookId::Hover));
        press(&mut app, "\n");
        assert!(matches!(app.overlay, Overlay::ChooseKey { .. }));
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("Colour from the active theme"), "{screen}");
        // the list: move to bg3 and choose it
        let target = crate::behaviour::theme_keys()
            .iter()
            .position(|k| *k == "bg3")
            .unwrap();
        // moving previews; Esc puts the old colour back
        press(&mut app, "j");
        assert!(
            matches!(app.overlay, Overlay::ChooseKey { .. }),
            "stays open"
        );
        assert_ne!(
            app.show_draft(|a| a.settings.hover.look.highlight_colour.clone()),
            "bg3"
        );
        press(&mut app, "\x1b");
        assert!(matches!(app.overlay, Overlay::None));
        let before = app.show_draft(|a| a.settings.hover.look.highlight_colour.clone());
        press(&mut app, "\n");
        // move to bg3 with the keys, Enter keeps it
        let at_now = crate::behaviour::theme_keys()
            .iter()
            .position(|k| *k == before)
            .unwrap();
        let steps = if target >= at_now {
            "j".repeat(target - at_now)
        } else {
            "k".repeat(at_now - target)
        };
        press(&mut app, &steps);
        press(&mut app, "\n");
        assert_eq!(
            app.show_draft(|a| a.settings.hover.look.highlight_colour.clone()),
            "bg3"
        );
        app.settings_page.bh_sel = at(BhRow::Underline(LookId::Hover));
        press(&mut app, "l"); // single → double
        app.settings_page.bh_sel = at(BhRow::TextWeight(LookId::Hover));
        press(&mut app, "l");
        // the derived-colour highlight is its own look
        app.settings_page.bh_sel = at(BhRow::Underline(LookId::Derived));
        press(&mut app, "l"); // off → single
        press(&mut app, "s"); // save
        let looks = app.looks(&app.settings);
        assert_eq!(looks.hover.bg, Some(Color::Rgb(4, 5, 6)));
        assert_eq!(looks.hover.underline.map(|u| u.0), Some(Underline::Double));
        assert_eq!(looks.hover.weight, Modifier::BOLD);
        assert_eq!(
            looks.derived.underline.map(|u| u.0),
            Some(Underline::Single)
        );
        assert_eq!(looks.preview, Color::Rgb(7, 8, 9));
        // drawn: the marker that out::Out turns into a double underline
        crate::hit::set_looks(looks);
        let s = crate::hit::hover(ratatui::style::Style::new());
        assert!(
            s.add_modifier
                .contains(Modifier::RAPID_BLINK | Modifier::BOLD)
        );
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Sample:") && screen.contains("Derived colours"),
            "{screen}"
        );
        // saved
        let saved = crate::config::load(&dir.join("studio-config"), false);
        assert_eq!(saved.settings.hover.look.underline, "double");
        assert_eq!(saved.settings.colours.derived_highlight.underline, "single");
        crate::hit::set_looks(Default::default());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn selecting_a_colour_lights_up_its_derivation_relatives() {
        use crate::hit::Target as T;
        let mut app = test_app(temp_store("related"), Start::Named("One".into()));
        let lit = |app: &mut App, key: &str| -> bool {
            let mut t = Terminal::new(TestBackend::new(200, 60)).unwrap();
            t.draw(|f| crate::ui::draw(f, app)).unwrap();
            let i = app.keys.iter().position(|k| *k == key).unwrap();
            let Some(row) = app
                .colour_rows()
                .iter()
                .position(|r| *r == ColourRow::Key(i))
            else {
                return false;
            };
            let r = app.hits.rect_of(T::ColourRow(row)).unwrap();
            let buf = t.backend().buffer().clone();
            buf[(r.x + 5, r.y)].bg == app.looks(&app.settings).derived.bg.unwrap()
        };
        let group_lit = |app: &mut App, name: &str| -> bool {
            let mut t = Terminal::new(TestBackend::new(200, 60)).unwrap();
            t.draw(|f| crate::ui::draw(f, app)).unwrap();
            let g = GROUPS.iter().position(|(n, _)| *n == name).unwrap();
            let row = app
                .colour_rows()
                .iter()
                .position(|r| matches!(r, ColourRow::Group { index, .. } if *index == g))
                .unwrap();
            let r = app.hits.rect_of(T::ColourRow(row)).unwrap();
            let buf = t.backend().buffer().clone();
            buf[(r.x + 4, r.y)].bg == app.looks(&app.settings).derived.bg.unwrap()
        };
        let select = |app: &mut App, key: &str| {
            let i = app.keys.iter().position(|k| *k == key).unwrap();
            app.select_key(i);
        };
        // a base colour: what's derived from it lights up (its closed group)
        select(&mut app, "red");
        assert!(group_lit(&mut app, "Tinted backgrounds"));
        assert!(!lit(&mut app, "green"));
        // a derived colour: its sources light up
        let g = GROUPS
            .iter()
            .position(|(n, _)| *n == "Tinted backgrounds")
            .unwrap();
        app.set_group_open(g, true);
        select(&mut app, "bg_visual_red");
        assert!(lit(&mut app, "bg0") && lit(&mut app, "red"));
        assert!(!lit(&mut app, "green"));
        // pinned (setting a derived key by hand pins it): nothing lights up
        app.palette.set("bg_visual_red", Rgb(9, 9, 9));
        assert!(!lit(&mut app, "red"));
        std::fs::remove_dir_all(app.store.dir.clone()).unwrap();
    }

    #[test]
    fn settings_tabs_take_the_keyboard_at_the_list_edge() {
        let store = temp_store("tab-focus");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        app.page = Page::Settings;
        // top (the default): up from the first row reaches the tabs
        press(&mut app, "k");
        assert!(app.settings_page.tabs_focused);
        press(&mut app, "l");
        assert_eq!(app.settings_page.tab, crate::settings::Tab::Behaviour);
        assert!(render(&mut app, 160, 45).contains("back to the list"));
        press(&mut app, "j");
        assert!(!app.settings_page.tabs_focused);
        // the first row is the first group's title (groups fold open / shut)
        assert_eq!(
            crate::behaviour::behaviour_rows()[app.settings_page.bh_sel],
            crate::behaviour::BhRow::Header("Keys")
        );
        press(&mut app, "jk");
        assert!(!app.settings_page.tabs_focused);
        // tabs at the bottom: down from the last row reaches them
        app.settings.layout.inner_tabs = "bottom".into();
        press(&mut app, "G");
        assert!(!app.settings_page.tabs_focused);
        press(&mut app, "j");
        assert!(app.settings_page.tabs_focused);
        press(&mut app, "l");
        assert_eq!(app.settings_page.tab, crate::settings::Tab::History);
        press(&mut app, "k");
        assert!(!app.settings_page.tabs_focused);
        let screen = render(&mut app, 160, 45);
        let lines: Vec<&str> = screen.lines().collect();
        assert!(
            lines[lines.len() - 3].contains("Keymaps"),
            "tabs at the bottom:\n{screen}"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn editing_a_base_colour_moves_its_unpinned_derived_colours_live() {
        let mut app = test_app(temp_store("live-derive"), Start::Named("One".into()));
        let i = app.keys.iter().position(|k| *k == "red").unwrap();
        app.select_key(i);
        app.open_picker();
        let before = app.palette.get("bg_visual_red");
        app.palette.set("bg_diff_red", Rgb(1, 1, 1)); // pinned now
        press(&mut app, "lll"); // in the picker, before Enter
        assert!(app.edit_session.is_some(), "not kept yet");
        assert_ne!(
            app.palette.get("bg_visual_red"),
            before,
            "derived follows live"
        );
        assert_eq!(app.palette.get("bg_diff_red"), Rgb(1, 1, 1), "pinned stays");
        std::fs::remove_dir_all(app.store.dir.clone()).unwrap();
    }

    #[test]
    fn preset_browser_previews_loads_creates_and_deletes() {
        use crate::hit::Target as T;
        let store = temp_store("presets");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        app.page = Page::Settings;
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("[ Vim ▾ ]"), "{screen}");
        // Enter on the preset button opens the list, on the active preset
        app.settings_page.km_sel = 0;
        press(&mut app, "\n");
        assert_eq!(app.settings_page.browse, Some(0));
        assert!(render(&mut app, 160, 45).contains("Vim (the current keymap)"));
        // j highlights Helix: the left side shows what would change
        press(&mut app, "j");
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Vim → Helix:") && screen.contains("differ"),
            "{screen}"
        );
        assert!(
            screen.contains("redo") && !screen.contains("main page"),
            "only differences:\n{screen}"
        );
        press(&mut app, "A");
        assert!(
            render(&mut app, 160, 45).contains("main page"),
            "everything"
        );
        // n makes a new preset from the highlighted one
        press(&mut app, "nMy Helix\n");
        let names = |app: &mut App| {
            app.show_draft(|a| a.presets.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
        };
        assert!(names(&mut app).contains(&"My Helix".to_string()));
        let mine = names(&mut app)
            .iter()
            .position(|n| n == "My Helix")
            .unwrap();
        assert_eq!(app.settings_page.browse, Some(mine));
        // Enter loads it (unsaved); s saves
        press(&mut app, "\n");
        assert_eq!(app.settings_page.browse, None);
        assert_eq!(app.show_draft(|a| a.keymap.name.clone()), "My Helix");
        assert_eq!(app.keymap.name, "Vim", "not live yet");
        press(&mut app, "s");
        assert_eq!(app.keymap.name, "My Helix");
        let saved = crate::config::load(&dir.join("studio-config"), false);
        assert_eq!(saved.keymap.name, "My Helix");
        // shipped presets can't be deleted; yours can (asks first)
        app.settings_page.km_sel = 0;
        press(&mut app, "\n");
        assert_eq!(
            app.settings_page.browse,
            Some(mine),
            "opens on the active one"
        );
        press(&mut app, "ggd"); // Helix keys are live now: gg, ge
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, _)| m.contains("can't be deleted"))
        );
        press(&mut app, "ge");
        assert_eq!(app.settings_page.browse, Some(mine));
        press(&mut app, "dy");
        assert!(!names(&mut app).contains(&"My Helix".to_string()));
        assert_eq!(
            app.show_draft(|a| a.keymap.name.clone()),
            "Vim",
            "the active one went: Vim"
        );
        press(&mut app, "s");
        assert!(
            !crate::config::load(&dir.join("studio-config"), false)
                .presets
                .iter()
                .any(|p| p.name == "My Helix")
        );
        // the mouse: a click highlights, a second loads
        render(&mut app, 160, 45);
        click_on(&mut app, T::PresetRow(2));
        assert_eq!(app.settings_page.browse, Some(2));
        render(&mut app, 160, 45);
        click_on(&mut app, T::PresetRow(2));
        assert_eq!(app.show_draft(|a| a.keymap.name.clone()), "Emacs");
        press(&mut app, "D");
        // Esc closes the list first, then leaves the page
        app.settings_page.km_sel = 0;
        press(&mut app, "\n\x1b");
        assert_eq!((app.settings_page.browse, app.page), (None, Page::Settings));
        press(&mut app, "\x1b");
        assert_eq!(app.page, Page::Main);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_groups_fold_and_only_names_are_underlined() {
        use crate::behaviour::{BhRow, visible_behaviour_rows};
        use crate::hit::Target as T;
        use crate::settings::KmRow;
        let mut app = test_app(temp_store("fold"), Start::Named("One".into()));
        app.page = Page::Settings;
        // Keymaps: Enter on a group's title closes it, again opens it
        let n = app.keymap_rows().len();
        let g = app
            .keymap_rows()
            .iter()
            .position(|r| *r == KmRow::Header(Context::Global))
            .unwrap();
        app.settings_page.km_sel = g;
        press(&mut app, "\n");
        assert!(app.keymap_rows().len() < n);
        assert!(render(&mut app, 160, 45).contains("▸ Everywhere"));
        press(&mut app, "l");
        assert_eq!(app.keymap_rows().len(), n);
        // the cursor stops on titles but skips blank lines
        press(&mut app, "k");
        assert_eq!(app.keymap_rows()[app.settings_page.km_sel], KmRow::Preset);
        // Behaviour: a click on the selected title folds it
        app.settings_page.tab = crate::settings::Tab::Behaviour;
        render(&mut app, 160, 45);
        let rows = visible_behaviour_rows(&app.settings_page.bh_closed);
        let hover = rows
            .iter()
            .position(|r| *r == BhRow::Header("Hover (everything clickable)"))
            .unwrap();
        app.settings_page.bh_sel = hover;
        render(&mut app, 160, 45);
        click_on(&mut app, T::SettingsRow(hover));
        assert!(
            app.settings_page
                .bh_closed
                .contains("Hover (everything clickable)")
        );
        assert!(visible_behaviour_rows(&app.settings_page.bh_closed).len() < rows.len());
        // hover: only the option's name is underlined, not the row
        let mut t = Terminal::new(TestBackend::new(160, 45)).unwrap();
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let leader = visible_behaviour_rows(&app.settings_page.bh_closed)
            .iter()
            .position(|r| *r == BhRow::Leader)
            .unwrap();
        let r = app.hits.rect_of(T::SettingsRow(leader)).unwrap();
        mouse(&mut app, MouseEventKind::Moved, r.x + 5, r.y);
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = t.backend().buffer().clone();
        let under = |x: u16| {
            buf[(x, r.y)]
                .modifier
                .contains(ratatui::style::Modifier::UNDERLINED)
        };
        let row: String = (r.x..r.right())
            .map(|x| buf[(x, r.y)].symbol().to_string())
            .collect();
        let start = r.x + row.find("Leader").unwrap() as u16;
        assert!(under(start) && under(start + 10), "the name");
        assert!(
            !under(r.x) && !under(start + 45) && !under(start + 52),
            "not the padding or value:\n{row}"
        );
        std::fs::remove_dir_all(app.store.dir.clone()).unwrap();
    }

    #[test]
    fn download_page_lists_searches_and_removes() {
        use crate::download::{Catalogue, Entry, Source};
        use crate::fonts_dl::{DlRow, DlStatus, DlTab};
        use crate::hit::Target as T;
        let store = temp_store("dl");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        let entry = |id: &str, family: &str, cat: &str, source: Source| Entry {
            id: id.into(),
            family: family.into(),
            category: cat.into(),
            license: "OFL-1.1".into(),
            weights: vec![400, 700],
            italic: false,
            variable: false,
            source,
            subset: "latin".into(),
        };
        app.dl.catalogue = Some(Catalogue {
            google: vec![
                entry("inter", "Inter", "sans-serif", Source::Google),
                entry("roboto-mono", "Roboto Mono", "monospace", Source::Google),
                entry("zen-loop", "Zen Loop", "display", Source::Google),
            ],
            nerd: vec![entry(
                "nerd-heavydata",
                "HeavyData Nerd Font",
                "nerd font",
                Source::Nerd {
                    asset: "HeavyData".into(),
                    url: "u".into(),
                    size: 1,
                },
            )],
            nerd_tag: "v3.5.1".into(),
        });
        app.on_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert_eq!(app.page, Page::Downloads);
        assert_eq!(app.contexts()[0], Context::Downloads);
        let screen = render(&mut app, 160, 45);
        assert!(
            screen.contains("Google Fonts")
                && screen.contains("Roboto Mono")
                && screen.contains("3 fonts"),
            "{screen}"
        );
        // a category: right steps to Sans
        press(&mut app, "l");
        assert_eq!(app.dl_rows(), vec![DlRow::Entry(0)]);
        press(&mut app, "h");
        // search: / then typing; Enter leaves the box
        press(&mut app, "/mono\n");
        assert_eq!(app.dl_rows(), vec![DlRow::Entry(1)]);
        assert!(render(&mut app, 160, 45).contains("Install"));
        // the preview: Roboto Mono shows code; t / L / + change it
        let screen = render(&mut app, 160, 45);
        assert!(screen.contains("Preview  Code ▾  Rust ▾  12pt"), "{screen}");
        press(&mut app, "L+");
        assert!(render(&mut app, 160, 45).contains("Preview  Code ▾  Python ▾  13pt"));
        // > pages through the code, < comes back
        press(&mut app, ">");
        let pv = app.dl.preview.as_ref().unwrap();
        assert!(pv.page > 0 && pv.page == pv.step, "{} {}", pv.page, pv.step);
        press(&mut app, "<");
        assert_eq!(app.dl.preview.as_ref().unwrap().page, 0);
        // the wheel over the preview moves three lines
        render(&mut app, 160, 45);
        let r = app.hits.rect_of(crate::hit::Target::DlPreview).unwrap();
        mouse(&mut app, MouseEventKind::ScrollDown, r.x + 2, r.y + 2);
        assert_eq!(app.dl.preview.as_ref().unwrap().page, 3);
        // the language dropdown: a click opens it on the current one,
        // a click on a row chooses (and starts at the top)
        click_on(&mut app, crate::hit::Target::PreviewMenu(1));
        assert!(matches!(
            app.overlay,
            Overlay::Menu {
                menu: 1,
                sel: 1,
                ..
            }
        ));
        assert!(render(&mut app, 160, 45).contains(" Haskell "));
        click_on(&mut app, crate::hit::Target::OverlayRow(10));
        assert_eq!(app.dl.preview.as_ref().unwrap().lang, 10);
        assert_eq!(app.dl.preview.as_ref().unwrap().page, 0);
        assert!(render(&mut app, 160, 45).contains("Code ▾  Haskell ▾"));
        // the kind dropdown: keys move, Enter chooses; a click outside closes
        click_on(&mut app, crate::hit::Target::PreviewMenu(0));
        press(&mut app, "j\n");
        assert!(matches!(app.overlay, Overlay::None));
        assert!(render(&mut app, 160, 45).contains("Preview  Icons ▾  13pt"));
        click_on(&mut app, crate::hit::Target::PreviewMenu(0));
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 1, 1);
        assert!(matches!(app.overlay, Overlay::None));
        app.dl.preview.as_mut().unwrap().kind = Some(crate::samples::Kind::Code);
        press(&mut app, "t");
        assert!(render(&mut app, 160, 45).contains("Preview  Icons"));
        // a failed fetch says so
        app.dl
            .preview
            .as_mut()
            .unwrap()
            .families
            .insert("roboto-mono".into(), Err("offline".into()));
        assert!(render(&mut app, 160, 45).contains("Couldn't fetch it: offline"));
        app.dl.search.clear();
        // something in the list counts as installed and can be removed
        let list = app.installed_list_path();
        let mut m = serde_json::Map::new();
        m.insert(
            "Zen Loop".into(),
            serde_json::json!({"source": "google", "id": "zen-loop"}),
        );
        crate::download::write_list(&list, m).unwrap();

        let zen = app.dl_entries()[2].clone();
        let listed = crate::download::read_list(&list);
        assert_eq!(app.dl_status(&zen, &listed), DlStatus::Listed);
        // the Installed tab (a click) lists it; d removes it
        render(&mut app, 160, 45);
        click_on(&mut app, T::DlTab(2));
        assert_eq!(app.dl.tab, DlTab::Installed);
        assert_eq!(app.dl_rows(), vec![DlRow::Listed("Zen Loop".into())]);
        press(&mut app, "d");
        assert!(crate::download::read_list(&list).is_empty());
        assert!(app.dl_rows().is_empty());
        // Nerd Fonts tab, with the release
        press(&mut app, "]");
        assert_eq!(app.dl.tab, DlTab::Google, "wraps around");
        press(&mut app, "]");
        assert!(render(&mut app, 160, 45).contains("release v3.5.1"));
        // up from the first row reaches the tabs
        press(&mut app, "k");
        assert!(app.dl.tabs_focused);
        press(&mut app, "j\x1b");
        assert_eq!(app.page, Page::Main);
        render(&mut app, 100, 40); // narrow
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_family_fills_every_role() {
        use crate::fonts::{Installed, Row};
        let store = temp_store("family");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        app.fonts.store.dir = dir.clone();
        let f = |family: &str, mono: bool, nerd: bool| Installed {
            family: family.into(),
            mono,
            nerd,
            bitmap: false,
            special: false,
        };
        app.fonts.set_installed(vec![
            f("JetBrains Mono", true, false),
            f("JetBrains Mono NL", true, false),
            f("JetBrainsMono Nerd Font", true, true),
            f("JetBrainsMono Nerd Font Mono", true, true),
            f("JetBrainsMono Nerd Font Propo", false, true),
            f("Inter", false, false),
        ]);
        press(&mut app, " 2");
        // the family's group row, then F
        let g = app
            .fonts
            .rows()
            .iter()
            .position(|r| matches!(r, Row::Group { name, .. } if name.starts_with("JetBrains")))
            .unwrap();
        app.fonts.list_sel = g;
        press(&mut app, "F");
        let fams: Vec<&str> = app
            .fonts
            .preset
            .fonts
            .iter()
            .map(|f| f.family.as_str())
            .collect();
        assert_eq!(
            fams,
            [
                "JetBrainsMono Nerd Font Mono",
                "JetBrainsMono Nerd Font Propo",
                "JetBrainsMono Nerd Font"
            ]
        );
        assert!(!app.fonts.dirty(), "saved straight away");
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, _)| m.contains("From JetBrains")),
            "{:?}",
            app.message
        );
        // a font in no group: the family is its own name, so Inter for all
        let i = app
            .fonts
            .installed
            .iter()
            .position(|f| f.family == "Inter")
            .unwrap();
        let inter = app
            .fonts
            .rows()
            .iter()
            .position(|r| matches!(r, Row::Font { index, .. } if *index == i))
            .unwrap();
        app.fonts.focus = crate::fonts::FontFocus::List;
        app.fonts.list_sel = inter;
        press(&mut app, "F");
        let fams: Vec<&str> = app
            .fonts
            .preset
            .fonts
            .iter()
            .map(|f| f.family.as_str())
            .collect();
        // mono keeps its font: Inter has no monospaced face
        assert_eq!(fams, ["JetBrainsMono Nerd Font Mono", "Inter", "Inter"]);
        assert!(
            app.message
                .as_ref()
                .is_some_and(|(m, _)| m.contains("mono kept")),
            "{:?}",
            app.message
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn messages_time_out_and_saves_can_pop_up() {
        let store = temp_store("messages");
        let dir = store.dir.clone();
        crate::history::init_for_test(dir.join("history"));
        let mut app = test_app(store, Start::Named("One".into()));
        let ago = |s: u64| Some(Instant::now() - Duration::from_secs(s));
        // info messages go after settings.messages.seconds (3 by default)
        app.info("hello");
        assert!(!app.expire_message());
        app.message_at = ago(3);
        assert!(app.expire_message());
        assert_eq!(app.message, None);
        // errors stay until a key
        app.error("bad");
        assert!(!app.expire_message());
        assert!(app.message.is_some());
        // 0: until a key
        app.settings.messages.seconds = 0;
        app.info("stays");
        app.message_at = ago(100);
        assert!(!app.expire_message());
        app.settings.messages.seconds = 3;
        // saving the settings says "Saved"; switched to popup, that save
        // already shows in a box
        app.with_draft(|a| a.settings.messages.saved = "popup".into());
        app.settings_save();
        assert_eq!(app.notice.as_deref(), Some("Saved"));
        assert_eq!(app.message, None);
        assert!(render(&mut app, 160, 45).contains("press any key to dismiss"));
        // a key closes it and does nothing else
        let sel = app.sel;
        press(&mut app, "j");
        assert_eq!(app.notice, None);
        assert_eq!(app.sel, sel);
        // so does a click
        app.notify_saved("Saved \"One\"");
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 1, 1);
        assert_eq!(app.notice, None);
        // bar: the status line, like any message
        app.settings.messages.saved = "bar".into();
        app.notify_saved("Saved");
        assert_eq!(app.message, Some(("Saved".into(), false)));
        assert_eq!(app.notice, None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn status_line_hints_sit_at_the_right() {
        let store = temp_store("hints");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        let last_row = |app: &mut App| render(app, 160, 45).lines().last().unwrap().to_string();
        // tabs at the bottom: tabs and theme at the left, hints at the right end
        let row = last_row(&mut app);
        assert!(row.trim_end().ends_with("? keys"), "{row}");
        assert!(row.contains("F1"));
        // a message sits after the tabs; the hints stay at the right
        app.info("Nothing to undo");
        let row = last_row(&mut app);
        let (msg, keys) = (
            row.find("Nothing to undo").unwrap(),
            row.find("? keys").unwrap(),
        );
        assert!(row.find("F1").unwrap() < msg && msg < keys, "{row}");
        assert!(row.trim_end().ends_with("? keys"), "{row}");
        // tabs on top: the bottom line keeps the message and the hints
        app.settings.layout.tabs = "top".into();
        let screen = render(&mut app, 160, 45);
        let (top, bottom) = (
            screen.lines().next().unwrap(),
            screen.lines().last().unwrap(),
        );
        assert!(top.contains("F1") && !top.contains("? keys"), "{top}");
        assert!(bottom.starts_with(" Nothing to undo"), "{bottom}");
        assert!(bottom.trim_end().ends_with("? keys"), "{bottom}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn palette_list_hangs_from_its_row_and_previews() {
        use crate::hit::Target as T;
        let store = temp_store("palette-menu");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        let one_red = app.palette.get("red");
        let down = |app: &mut App| app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        // the title names it; the dropdown is the pane's first row
        let s = render(&mut app, 160, 45);
        assert!(s.contains("Palette: One ") && s.contains(" One ▾ "), "{s}");
        // a click on the row opens the list, hanging under it
        click_on(&mut app, T::PaletteMenu);
        assert!(matches!(app.overlay, Overlay::Open { .. }));
        render(&mut app, 160, 45);
        let pane = app.palette_pane.unwrap();
        assert_eq!(app.hits.rect_of(T::OverlayRow(0)).unwrap().y, pane.y + 5);
        // moving shows the highlighted palette; it isn't an unsaved edit
        down(&mut app);
        assert_eq!(app.palette.get("red"), Rgb(200, 0, 0));
        assert_eq!(app.name, "One");
        assert!(!app.dirty());
        assert!(render(&mut app, 160, 45).contains("Palette: One "));
        // Esc puts it back
        press(&mut app, "\x1b");
        assert!(matches!(app.overlay, Overlay::None));
        assert_eq!(app.palette.get("red"), one_red);
        assert!(app.open_original.is_none());
        // <leader>op opens the same list; Enter keeps the highlighted one
        press(&mut app, " op");
        down(&mut app);
        press(&mut app, "\n");
        assert_eq!(app.name, "Two");
        assert_eq!(app.palette.get("red"), Rgb(200, 0, 0));
        assert!(!app.dirty());
        // with unsaved edits, choosing still asks first (about the real palette)
        press(&mut app, "#111111\n");
        assert!(app.dirty());
        let edited = app.palette.clone();
        press(&mut app, " op");
        app.on_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
        assert_ne!(app.palette, edited);
        assert!(app.dirty());
        press(&mut app, "\n");
        assert!(matches!(app.overlay, Overlay::Confirm(Pending::Open(_))));
        assert_eq!(app.palette, edited);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn behaviour_tab_sets_up_prefetching() {
        use crate::behaviour::{BhRow, behaviour_rows};
        let store = temp_store("prefetch-rows");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        app.settings_page.bh_closed.clear();
        let at = |r: BhRow| behaviour_rows().iter().position(|x| *x == r).unwrap();
        let run = |app: &mut App, r: BhRow| {
            app.with_draft(|a| {
                a.settings_page.bh_sel = at(r);
                a.run_behaviour_tab(Action::Confirm);
            })
        };
        // prefetching: off by default, monospace chosen; edits wait for Save
        assert!(!app.settings.fonts_page.prefetch);
        run(&mut app, BhRow::Prefetch);
        run(&mut app, BhRow::PrefetchCategory(5));
        assert!(!app.settings.fonts_page.prefetch, "not live before Save");
        app.settings_save();
        assert!(app.settings.fonts_page.prefetch);
        assert_eq!(
            app.settings.fonts_page.prefetch_categories,
            ["monospace", "nerd font"]
        );
        // with the catalogue, each category shows what it takes
        let nerd = crate::download::Source::Nerd {
            asset: "Hack".into(),
            url: String::new(),
            size: 40_000_000,
        };
        let mut hack = crate::download::Entry {
            id: "hack".into(),
            family: "Hack".into(),
            category: "nerd font".into(),
            license: String::new(),
            weights: vec![400],
            italic: false,
            variable: false,
            source: nerd,
            subset: "latin".into(),
        };
        app.dl.catalogue = Some(crate::download::Catalogue {
            nerd: vec![hack.clone()],
            ..Default::default()
        });
        assert_eq!(
            app.behaviour_value(BhRow::PrefetchCategory(5)),
            "yes  (1 font: 40.0 MB to fetch, about 2.5 MB kept)"
        );
        hack.category = "monospace".into();
        hack.source = crate::download::Source::Google;
        app.dl.catalogue.as_mut().unwrap().google = vec![hack];
        assert_eq!(
            app.behaviour_value(BhRow::PrefetchCategory(0)),
            "yes  (1 font, about 100 KB)"
        );
        // Clear asks first (the answer isn't given here: it would delete
        // the real folder)
        run(&mut app, BhRow::ClearPreviews);
        assert!(matches!(
            app.overlay,
            Overlay::Ask {
                then: crate::settings::Deferred::ClearPreviews,
                ..
            }
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn hints_and_previews_toggle_per_page() {
        let store = temp_store("toggles");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        let screen = |app: &mut App| render(app, 160, 45);
        // Main: <leader>tp hides the preview; the columns take its room
        assert!(screen(&mut app).contains("Preview"));
        press(&mut app, " tp");
        assert!(!app.main_preview_visible);
        let s = screen(&mut app);
        assert!(!s.contains("Space 6 / Space 7"), "{s}");
        let pane = app.palette_pane.unwrap();
        assert_eq!(pane.x + pane.width, 160, "colours reach the right edge");
        // jumping to nvim shows it again
        press(&mut app, " 6");
        assert!(app.main_preview_visible);
        // (nvim has the keyboard now: take it back)
        app.preview.focus = None;
        press(&mut app, " tp");
        // the Fonts page's preview is its own
        app.on_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert!(app.fonts_preview_visible && !app.main_preview_visible);
        press(&mut app, " tp");
        assert!(!app.fonts_preview_visible && !app.main_preview_visible);
        // <leader>th hides the key hints anywhere; messages still show
        assert!(screen(&mut app).contains("? keys"));
        press(&mut app, " th");
        let s = screen(&mut app);
        assert!(!s.contains("? keys"), "{s}");
        app.info("Hello");
        assert!(screen(&mut app).contains("Hello"));
        // the settings say how a run starts
        let mut config = crate::config::Settings::default();
        config.layout.key_hints = "hidden".into();
        config.layout.main_preview = "hidden".into();
        let loaded = crate::config::Loaded {
            keymap: Keymap::vim(),
            presets: Keymap::builtin(),
            settings: config,
            warnings: Vec::new(),
            dir: dir.join("studio-config"),
        };
        let mut app2 = App::with_config(
            Store { dir: dir.clone() },
            Start::Named("One".into()),
            loaded,
        )
        .unwrap();
        app2.preview.set_headless();
        assert!(!app2.hints_visible && !app2.main_preview_visible && app2.fonts_preview_visible);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn behaviour_cursor_keeps_the_swatch() {
        use crate::behaviour::{BhRow, LookId, visible_behaviour_rows};
        use ratatui::style::Modifier;
        let store = temp_store("cursor-swatch");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        app.page = Page::Settings;
        app.settings_page.tab = crate::settings::Tab::Behaviour;
        app.settings_page.bh_closed.clear();
        let row = BhRow::HighlightColour(LookId::Hover);
        app.settings_page.bh_sel = visible_behaviour_rows(&app.settings_page.bh_closed)
            .iter()
            .position(|r| *r == row)
            .unwrap();
        let mut t = Terminal::new(TestBackend::new(160, 60)).unwrap();
        t.draw(|f| crate::ui::draw(f, &mut app)).unwrap();
        let buf = t.backend().buffer().clone();
        let label = App::behaviour_label(row);
        let y = (0..60)
            .find(|&y| {
                let s: String = (0..160).map(|x| buf[(x, y)].symbol().to_string()).collect();
                s.contains(&label) && s.contains("██")
            })
            .expect("the row is drawn");
        let cells: Vec<_> = (0..160).map(|x| buf[(x, y)].clone()).collect();
        let reversed = |c: &ratatui::buffer::Cell| c.modifier.contains(Modifier::REVERSED);
        // the swatch keeps its colour and isn't reversed
        let swatch = cells.iter().find(|c| c.symbol() == "█").unwrap();
        assert!(!reversed(swatch));
        assert_eq!(
            swatch.fg,
            app.theme_colour(&app.settings.hover.look.highlight_colour)
        );
        // the label is reversed, the padding after it isn't
        let start = (0..160).find(|&x| buf[(x, y)].symbol() == "H").unwrap() as usize;
        assert!(reversed(&cells[start]));
        let end = start + label.chars().count();
        assert!(!reversed(&cells[end + 1]), "padding after the label");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keyboard_reaches_the_palette_dropdown_and_the_page_tabs() {
        let store = temp_store("nav-up");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        app.settings.layout.tabs = "top".into();
        let key = |app: &mut App, c: KeyCode| app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        app.focus_pane(4);
        // the cursor starts on the first colour, under the Base group
        key(&mut app, KeyCode::Up);
        assert_eq!(app.row_sel, 0);
        // up from the first row: the dropdown row; Enter opens the list
        key(&mut app, KeyCode::Up);
        assert!(app.palette_menu_focused);
        key(&mut app, KeyCode::Enter);
        assert!(matches!(app.overlay, Overlay::Open { .. }));
        key(&mut app, KeyCode::Esc);
        assert!(app.palette_menu_focused, "back on the row");
        // up again: the page tabs; right changes pages, the keyboard stays
        key(&mut app, KeyCode::Up);
        assert!(app.page_tabs_focused);
        let s = render(&mut app, 160, 45);
        assert!(s.contains("F1 Main"), "{s}");
        key(&mut app, KeyCode::Right);
        assert_eq!(app.page, Page::Downloads);
        assert!(app.page_tabs_focused);
        // down: into the page (its tabs are on top, so the list)
        key(&mut app, KeyCode::Down);
        assert!(!app.page_tabs_focused && !app.dl.tabs_focused);
        // up: the page's tabs, then the page tabs; left back to Main
        key(&mut app, KeyCode::Up);
        assert!(app.dl.tabs_focused);
        key(&mut app, KeyCode::Up);
        assert!(app.page_tabs_focused);
        key(&mut app, KeyCode::Left);
        assert_eq!(app.page, Page::Main);
        key(&mut app, KeyCode::Down);
        assert!(!app.page_tabs_focused && app.palette_menu_focused);
        key(&mut app, KeyCode::Down);
        assert!(!app.palette_menu_focused && app.row_sel == 0);
        // tabs at the bottom: up from the dropdown row goes nowhere
        app.settings.layout.tabs = "bottom".into();
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::Up);
        assert!(app.palette_menu_focused && !app.page_tabs_focused);
        // a click anywhere takes the keyboard off the page tabs
        app.page_tabs_focused = true;
        mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 1, 1);
        assert!(!app.page_tabs_focused);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn installed_fonts_preview_from_their_files() {
        // a real font file to stand in (skipped without one, e.g. in the
        // Nix build's sandbox)
        let Some(file) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "monospace"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .filter(|f| f.ends_with(".ttf") || f.ends_with(".otf"))
        else {
            eprintln!("skipped: no font file");
            return;
        };
        let store = temp_store("installed-preview");
        let dir = store.dir.clone();
        let mut app = test_app(store, Start::Named("One".into()));
        let fonts = crate::download::fonts_dir().join("test-mono");
        std::fs::create_dir_all(&fonts).unwrap();
        let ext = if file.ends_with(".otf") { "otf" } else { "ttf" };
        std::fs::copy(&file, fonts.join(format!("TestMono-Regular.{ext}"))).unwrap();
        let list = app.installed_list_path();
        std::fs::create_dir_all(list.parent().unwrap()).unwrap();
        std::fs::write(
            &list,
            r#"{"version": 1, "fonts": {"Test Mono": {"source": "fontsource", "id": "test-mono"}}}"#,
        )
        .unwrap();
        app.page = Page::Downloads;
        app.dl.tab = crate::fonts_dl::DlTab::Installed;
        app.dl.listed = None;
        app.dl.sel = 0;
        // the highlighted installed font is the preview's, with code by default
        let e = app.preview_entry().unwrap();
        assert_eq!(
            (e.id.as_str(), e.category.as_str()),
            ("test-mono", "monospace")
        );
        // its family comes from its files, without fetching anything
        let start = std::time::Instant::now();
        loop {
            app.poll_downloads();
            if let Some(r) = app
                .dl
                .preview
                .as_ref()
                .and_then(|p| p.families.get("test-mono"))
            {
                assert!(r.as_ref().is_ok_and(|f| !f.is_empty()), "{r:?}");
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "no family name");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::fs::remove_dir_all(fonts).unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
