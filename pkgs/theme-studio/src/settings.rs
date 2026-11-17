//! The Settings page (F3): Keymaps, Behaviour and History tabs. Every change
//! is saved straight away (keymaps.json / settings.json), and every save goes
//! through the history, so it can be undone.

use std::path::PathBuf;

use serde_json::Value;

use crate::app::{App, Overlay};
use crate::color::Rgb;
use crate::history::{History, SnapshotInfo};
use crate::hit::{HoverLook, Underline};
use crate::keymap::{Action, Context, Key, Keymap, all_actions, display_seq};
use crate::palette::GROUPS;
use ratatui::style::{Color, Modifier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Keymaps,
    Behaviour,
    History,
}

pub const TABS: [(Tab, &str); 3] = [
    (Tab::Keymaps, "Keymaps"),
    (Tab::Behaviour, "Behaviour"),
    (Tab::History, "History"),
];

/// A row of the Keymaps tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KmRow {
    /// Which preset is active (left/right to switch).
    Preset,
    Header(Context),
    Binding(Context, Action),
}

/// A row of the Behaviour tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BhRow {
    Leader,
    Tabs,
    GroupOpen(usize),
    PickerAtStart,
    PickerAfterEditing,
    SampleAtStart,
    ListOpensToCurrent,
    HideBitmap,
    HideSpecial,
    Spacing,
    NerdFont,
    SaveSizes,
    ResetSizes,
    HoverHighlight,
    HoverHighlightColour,
    HoverUnderline,
    HoverUnderlineColour,
    HoverText,
    HoverTextColour,
    HoverTextWeight,
    PaneBorder,
    PreviewBorder,
}

impl BhRow {
    /// Rows whose value is a key of the system palette.
    pub fn is_colour(self) -> bool {
        matches!(
            self,
            BhRow::HoverHighlightColour
                | BhRow::HoverUnderlineColour
                | BhRow::HoverTextColour
                | BhRow::PaneBorder
                | BhRow::PreviewBorder
        )
    }
}

/// Every palette key, in the colour list's order (the choices for hover
/// colours).
pub fn theme_keys() -> Vec<&'static str> {
    GROUPS
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .collect()
}

pub fn behaviour_rows() -> Vec<BhRow> {
    let mut v = vec![
        BhRow::Leader,
        BhRow::Tabs,
        BhRow::HoverHighlight,
        BhRow::HoverHighlightColour,
        BhRow::HoverUnderline,
        BhRow::HoverUnderlineColour,
        BhRow::HoverText,
        BhRow::HoverTextColour,
        BhRow::HoverTextWeight,
        BhRow::PaneBorder,
        BhRow::PreviewBorder,
    ];
    v.extend((0..GROUPS.len()).map(BhRow::GroupOpen));
    v.extend([
        BhRow::PickerAtStart,
        BhRow::PickerAfterEditing,
        BhRow::SampleAtStart,
        BhRow::ListOpensToCurrent,
        BhRow::HideBitmap,
        BhRow::HideSpecial,
        BhRow::Spacing,
        BhRow::NerdFont,
        BhRow::SaveSizes,
        BhRow::ResetSizes,
    ]);
    v
}

/// The files with a history, as the History tab lists them.
pub const FILES: [&str; 4] = [
    "palettes.json",
    "fonts.json",
    "keymaps.json",
    "settings.json",
];

/// What a key press is being recorded for (the Keymaps / Behaviour tabs).
#[derive(Clone, Debug, PartialEq)]
pub enum Capture {
    /// New keys for an action (`add`: next to the ones it has).
    Binding {
        ctx: Context,
        action: Action,
        add: bool,
    },
    /// The leader key.
    Leader,
}

/// Something waiting for a name typed into the prompt.
#[derive(Clone, Debug, PartialEq)]
pub enum Naming {
    /// Save the active keymap as a new preset.
    Preset,
    /// Save a file's current state as a named version.
    Version(&'static str),
    /// Keys typed as text (Vim notation).
    BindingText {
        ctx: Context,
        action: Action,
        add: bool,
    },
}

/// Something asked about first (y / n).
#[derive(Clone, Debug, PartialEq)]
pub enum Deferred {
    Restore {
        file: &'static str,
        version: PathBuf,
    },
    DeleteVersion(PathBuf),
    DeletePreset(String),
    ResetPreset(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistFocus {
    Files,
    Versions,
}

pub struct SettingsPage {
    pub tab: Tab,
    pub km_sel: usize,
    pub km_scroll: usize,
    pub bh_sel: usize,
    pub bh_scroll: usize,
    pub hist_file: usize,
    pub hist_focus: HistFocus,
    pub hist_sel: usize,
    pub hist_scroll: usize,
    /// The difference view shows everything, not just what differs.
    pub show_all: bool,
    pub diff_scroll: usize,
    /// Cached until something changes (see history::generation).
    versions: Option<(VersionsKey, Vec<(PathBuf, SnapshotInfo)>)>,
    view: Option<(ViewKey, DiffView)>,
}

/// (history generation, file)
type VersionsKey = (u64, usize);
/// (history generation, file, selected version, show everything, palette)
type ViewKey = (u64, usize, Option<PathBuf>, bool, String);
/// A palette's colours by key.
pub type Colours = Vec<(String, Rgb)>;

/// The History tab's preview of the selected version.
#[derive(Clone, Debug, Default)]
pub struct DiffView {
    pub lines: Vec<DiffLine>,
    /// palettes.json: the palette being edited, now and in the selected
    /// version (key, colour).
    pub palette: Option<(Colours, Colours)>,
}

impl Default for SettingsPage {
    fn default() -> Self {
        SettingsPage {
            tab: Tab::Keymaps,
            km_sel: 0,
            km_scroll: 0,
            bh_sel: 0,
            bh_scroll: 0,
            hist_file: 0,
            hist_focus: HistFocus::Versions,
            hist_sel: 0,
            hist_scroll: 0,
            show_all: false,
            diff_scroll: 0,
            versions: None,
            view: None,
        }
    }
}

/// A line of the History tab's difference view.
#[derive(Clone, Debug, PartialEq)]
pub enum DiffLine {
    Head {
        depth: usize,
        key: String,
    },
    Same {
        depth: usize,
        key: String,
        value: String,
    },
    Changed {
        depth: usize,
        key: String,
        now: String,
        then: String,
    },
    /// Only in the selected version (restoring adds it).
    Added {
        depth: usize,
        key: String,
        value: String,
    },
    /// Only now (restoring removes it).
    Removed {
        depth: usize,
        key: String,
        value: String,
    },
}

fn show(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join("  "),
        Value::Object(o) => format!("{{{} entries}}", o.len()),
        other => other.to_string(),
    }
}

/// The selected version against the file now: every entry, with what
/// differs marked (`all`), or only the differences and the groups they're in.
pub fn diff_lines(now: &Value, then: &Value, all: bool) -> Vec<DiffLine> {
    fn walk(
        now: Option<&Value>,
        then: Option<&Value>,
        depth: usize,
        all: bool,
        out: &mut Vec<DiffLine>,
    ) {
        let (Some(Value::Object(n)), Some(Value::Object(t))) = (now, then) else {
            return;
        };
        let mut keys: Vec<&String> = t.keys().collect();
        keys.extend(n.keys().filter(|k| !t.contains_key(*k)));
        for k in keys {
            let (a, b) = (n.get(k), t.get(k));
            match (a, b) {
                (Some(Value::Object(_)), Some(Value::Object(_))) => {
                    let mut inner = Vec::new();
                    walk(a, b, depth + 1, all, &mut inner);
                    if all || !inner.is_empty() {
                        out.push(DiffLine::Head {
                            depth,
                            key: k.clone(),
                        });
                        out.extend(inner);
                    }
                }
                (Some(x), Some(y)) if x == y => {
                    if all {
                        out.push(DiffLine::Same {
                            depth,
                            key: k.clone(),
                            value: show(x),
                        })
                    }
                }
                (Some(x), Some(y)) => out.push(DiffLine::Changed {
                    depth,
                    key: k.clone(),
                    now: show(x),
                    then: show(y),
                }),
                (None, Some(y)) => out.push(DiffLine::Added {
                    depth,
                    key: k.clone(),
                    value: show(y),
                }),
                (Some(x), None) => out.push(DiffLine::Removed {
                    depth,
                    key: k.clone(),
                    value: show(x),
                }),
                (None, None) => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(Some(now), Some(then), 0, all, &mut out);
    out
}

fn wrap(i: usize, d: i32, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (i as i32 + d).rem_euclid(n as i32) as usize
    }
}

impl App {
    // ----- hover look --------------------------------------------------------

    /// A key of the active system palette as a colour (the terminal's own
    /// colours when the palette can't be read).
    pub fn theme_colour(&self, key: &str) -> Color {
        if let Some(p) = &self.system_palette
            && theme_keys().contains(&key)
        {
            let c = p.get(key);
            return Color::Rgb(c.0, c.1, c.2);
        }
        match key {
            "red" | "bg_red" => Color::Red,
            "green" | "bg_green" => Color::Green,
            "yellow" | "bg_yellow" | "orange" => Color::Yellow,
            "blue" => Color::Blue,
            "purple" => Color::Magenta,
            "aqua" => Color::Cyan,
            "fg0" | "fg1" => Color::White,
            k if k.starts_with("grey") => Color::Gray,
            _ => Color::DarkGray,
        }
    }

    /// The hover settings as colours.
    pub fn hover_look(&self, h: &crate::config::HoverSettings) -> HoverLook {
        HoverLook {
            bg: h.highlight.then(|| self.theme_colour(&h.highlight_colour)),
            underline: Underline::parse(&h.underline)
                .map(|u| (u, self.theme_colour(&h.underline_colour))),
            fg: h.text.then(|| self.theme_colour(&h.text_colour)),
            weight: match h.text_weight.as_str() {
                "bold" => Modifier::BOLD,
                "dim" => Modifier::DIM,
                _ => Modifier::empty(),
            },
            pane: self.theme_colour(&h.pane_border),
            preview: self.theme_colour(&h.preview_border),
        }
    }

    /// The key a colour row holds.
    fn colour_setting(&mut self, row: BhRow) -> Option<&mut String> {
        let h = &mut self.settings.hover;
        Some(match row {
            BhRow::HoverHighlightColour => &mut h.highlight_colour,
            BhRow::HoverUnderlineColour => &mut h.underline_colour,
            BhRow::HoverTextColour => &mut h.text_colour,
            BhRow::PaneBorder => &mut h.pane_border,
            BhRow::PreviewBorder => &mut h.preview_border,
            _ => return None,
        })
    }

    /// Sets a colour row to a palette key (from the key list).
    pub fn set_colour_setting(&mut self, row: BhRow, key: &str) {
        if let Some(v) = self.colour_setting(row) {
            *v = key.to_string();
            self.save_settings();
        }
    }

    /// Keys while the list of palette keys is open.
    pub fn choose_key_key(
        &mut self,
        row: BhRow,
        mut sel: usize,
        k: ratatui::crossterm::event::KeyEvent,
    ) {
        use ratatui::crossterm::event::KeyCode;
        let keys = theme_keys();
        match k.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                self.set_colour_setting(row, keys[sel]);
            }
            KeyCode::Up | KeyCode::Char('k') => sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => sel = (sel + 1).min(keys.len() - 1),
            KeyCode::PageUp => sel = sel.saturating_sub(10),
            KeyCode::PageDown => sel = (sel + 10).min(keys.len() - 1),
            KeyCode::Home | KeyCode::Char('g') => sel = 0,
            KeyCode::End | KeyCode::Char('G') => sel = keys.len() - 1,
            _ => {}
        }
        if matches!(self.overlay, Overlay::ChooseKey { .. }) {
            self.overlay = Overlay::ChooseKey { row, sel };
        }
    }

    // ----- shared ----------------------------------------------------------

    pub fn history(&self) -> Option<History> {
        History::current()
    }

    /// Where a file with a history lives.
    pub fn file_path(&self, file: &str) -> PathBuf {
        match file {
            "palettes.json" | "fonts.json" => self.store.dir.join(file),
            _ => self.config_dir.join(file),
        }
    }

    fn save_keymaps(&mut self) {
        let active = self.keymap.name.clone();
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == active) {
            *p = self.keymap.clone();
        } else {
            self.presets.push(self.keymap.clone());
        }
        if let Err(e) = crate::config::save_keymaps(&self.config_dir, &self.presets, &active) {
            self.error(format!("Saving keymaps.json: {e:#}"));
        }
    }

    fn save_settings(&mut self) {
        if let Err(e) = crate::config::save_settings(&self.config_dir, &self.settings) {
            self.error(format!("Saving settings.json: {e:#}"));
        }
    }

    /// After a config file changed on disk (restore, undo): use it.
    pub fn reload_file(&mut self, file: &str) {
        match file {
            "palettes.json" if !self.dirty() => {
                if let Ok(p) = self.store.load(&self.name) {
                    self.saved = p.clone();
                    self.palette = p;
                }
            }
            "fonts.json" if !self.fonts.dirty() => {
                let name = self.fonts.name.clone();
                let _ = self.fonts.open(&name);
            }
            "keymaps.json" | "settings.json" => {
                let l = crate::config::load(&self.config_dir, false);
                self.keymap = l.keymap;
                self.presets = l.presets;
                self.settings = l.settings;
            }
            _ => {}
        }
    }

    /// `<leader>u`: puts back the newest snapshot of the file the studio last
    /// wrote (which itself is kept, so doing it again redoes).
    pub fn undo_config(&mut self) {
        let Some((file, path)) = crate::history::last_write() else {
            return self.info("No config change to undo yet");
        };
        let Some(h) = self.history() else { return };
        let newest = h
            .list_info(&file)
            .into_iter()
            .find(|(_, s)| s.name.is_none());
        let Some(snap) = newest.and_then(|(p, _)| History::load(&p)) else {
            return self.info(format!("No earlier version of {file}"));
        };
        match crate::store::write_json_file(&path, &snap.content) {
            Ok(()) => {
                self.reload_file(&file);
                self.info(format!(
                    "Undid the last change to {file} ({}); again to redo",
                    snap.summary
                ));
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    // ----- mouse -----------------------------------------------------------

    /// A click on the Settings page: selects; on the selected row, acts
    /// like Enter.
    pub fn settings_click(&mut self, t: crate::hit::Target) {
        use crate::hit::Target as T;
        let p = &mut self.settings_page;
        match t {
            T::SettingsTab(i) => p.tab = TABS[i as usize].0,
            T::HistFile(i) => {
                p.hist_focus = HistFocus::Files;
                if p.hist_file != i {
                    p.hist_file = i;
                    p.hist_sel = 0;
                    p.diff_scroll = 0;
                }
            }
            T::SettingsRow(i) => {
                let sel = match p.tab {
                    Tab::Keymaps => &mut p.km_sel,
                    Tab::Behaviour => &mut p.bh_sel,
                    Tab::History => {
                        p.hist_focus = HistFocus::Versions;
                        if p.hist_sel != i {
                            p.diff_scroll = 0;
                        }
                        &mut p.hist_sel
                    }
                };
                let again = *sel == i;
                *sel = i;
                if again {
                    self.run_settings(Action::Confirm);
                }
            }
            _ => {}
        }
    }

    /// The wheel over a list moves its selection; over the preview, scrolls.
    pub fn settings_scroll(&mut self, t: crate::hit::Target, d: i32) {
        use crate::hit::Target as T;
        if t == T::SettingsDiff {
            let p = &mut self.settings_page;
            p.diff_scroll = (p.diff_scroll as i32 + d * 3).max(0) as usize;
            return;
        }
        if self.settings_page.tab == Tab::History {
            self.settings_page.hist_focus = HistFocus::Versions;
        }
        self.run_settings(if d > 0 { Action::Down } else { Action::Up });
    }

    // ----- keys ------------------------------------------------------------

    pub fn run_settings(&mut self, a: Action) {
        match a {
            Action::TabNext | Action::TabPrev => {
                let i = TABS
                    .iter()
                    .position(|(t, _)| *t == self.settings_page.tab)
                    .unwrap_or(0);
                let d = if a == Action::TabNext { 1 } else { -1 };
                self.settings_page.tab = TABS[wrap(i, d, TABS.len())].0;
                return;
            }
            // Esc leaves the page
            Action::Back => {
                self.page = crate::app::Page::Main;
                return;
            }
            _ => {}
        }
        match self.settings_page.tab {
            Tab::Keymaps => self.run_keymaps_tab(a),
            Tab::Behaviour => self.run_behaviour_tab(a),
            Tab::History => self.run_history_tab(a),
        }
    }

    /// Keymaps tab rows: the preset, then each context's actions (those it
    /// has keys for, in this preset or the shipped one, and unbound ones).
    pub fn keymap_rows(&self) -> Vec<KmRow> {
        let shipped = Keymap::vim();
        let mut rows = vec![KmRow::Preset];
        for ctx in Context::ALL {
            let actions: Vec<Action> = all_actions()
                .into_iter()
                .filter(|a| {
                    !self.keymap.keys_of(ctx, *a).is_empty()
                        || !shipped.keys_of(ctx, *a).is_empty()
                        || self.keymap.unbound.contains(&(ctx, *a))
                })
                .collect();
            if actions.is_empty() {
                continue;
            }
            rows.push(KmRow::Header(ctx));
            rows.extend(actions.into_iter().map(|a| KmRow::Binding(ctx, a)));
        }
        rows
    }

    fn move_sel(sel: &mut usize, n: usize, a: Action) -> bool {
        let last = n.saturating_sub(1) as i32;
        let d: i32 = match a {
            Action::Up => -1,
            Action::Down => 1,
            Action::PageUp => -10,
            Action::PageDown => 10,
            Action::Top => -(*sel as i32),
            Action::Bottom => last - *sel as i32,
            _ => return false,
        };
        *sel = (*sel as i32 + d).clamp(0, last) as usize;
        true
    }

    fn switch_preset(&mut self, d: i32) {
        let i = self
            .presets
            .iter()
            .position(|p| p.name == self.keymap.name)
            .unwrap_or(0);
        let next = self.presets[wrap(i, d, self.presets.len())].clone();
        self.keymap = next;
        self.save_keymaps();
        self.info(format!("Keymap preset: {}", self.keymap.name));
    }

    fn run_keymaps_tab(&mut self, a: Action) {
        let rows = self.keymap_rows();
        if Self::move_sel(&mut self.settings_page.km_sel, rows.len(), a) {
            return;
        }
        let row = rows.get(self.settings_page.km_sel).copied();
        let builtin = Keymap::builtin().iter().any(|b| b.name == self.keymap.name);
        match (row, a) {
            (Some(KmRow::Preset), Action::Left) => self.switch_preset(-1),
            (Some(KmRow::Preset), Action::Right | Action::Confirm) => self.switch_preset(1),
            (_, Action::SaveNamed) => {
                self.prompt("Save the keymap as a new preset", Naming::Preset, "")
            }
            (_, Action::ResetPreset) if builtin => self.ask(
                format!("Reset \"{}\" to the shipped keys?", self.keymap.name),
                Deferred::ResetPreset(self.keymap.name.clone()),
            ),
            (_, Action::ResetPreset) => {
                self.info("Only shipped presets can be reset; this one is yours")
            }
            (Some(KmRow::Preset), Action::Delete) if !builtin => self.ask(
                format!("Delete the preset \"{}\"?", self.keymap.name),
                Deferred::DeletePreset(self.keymap.name.clone()),
            ),
            (Some(KmRow::Preset), Action::Delete) => {
                self.info("Shipped presets can't be deleted (R resets them)")
            }
            (Some(KmRow::Binding(ctx, action)), Action::Confirm | Action::Right) => {
                self.overlay = Overlay::Capture {
                    what: Capture::Binding {
                        ctx,
                        action,
                        add: false,
                    },
                    keys: Vec::new(),
                }
            }
            (Some(KmRow::Binding(ctx, action)), Action::Add) => {
                self.overlay = Overlay::Capture {
                    what: Capture::Binding {
                        ctx,
                        action,
                        add: true,
                    },
                    keys: Vec::new(),
                }
            }
            (Some(KmRow::Binding(ctx, action)), Action::EditText) => {
                let current: Vec<String> = self
                    .keymap
                    .keys_of(ctx, action)
                    .iter()
                    .map(|k| display_seq(k, self.keymap.leader))
                    .collect();
                self.prompt(
                    &format!(
                        "Keys for \"{}\" (Vim notation; space between several)",
                        action.describe()
                    ),
                    Naming::BindingText {
                        ctx,
                        action,
                        add: false,
                    },
                    &current.join(" "),
                )
            }
            (Some(KmRow::Binding(ctx, action)), Action::Delete) => {
                self.keymap.set_keys(ctx, action, Vec::new());
                self.save_keymaps();
                self.info(format!(
                    "\"{}\" has no keys now ({})",
                    action.describe(),
                    ctx.title()
                ));
            }
            _ => {}
        }
    }

    /// Sets an action's keys from the capture or the text prompt.
    pub fn bind(&mut self, ctx: Context, action: Action, keys: Vec<Vec<Key>>, add: bool) {
        let mut all = if add {
            self.keymap.keys_of(ctx, action)
        } else {
            Vec::new()
        };
        let clashes: Vec<String> = keys
            .iter()
            .flat_map(|k| self.keymap.conflicts(ctx, action, k))
            .collect();
        all.extend(keys);
        self.keymap.set_keys(ctx, action, all);
        self.save_keymaps();
        if clashes.is_empty() {
            self.info(format!("Saved the keys for \"{}\"", action.describe()));
        } else {
            self.error(format!("Saved, but: {}", clashes.join("; ")));
        }
    }

    fn prompt(&mut self, title: &str, purpose: Naming, input: &str) {
        self.overlay = Overlay::Prompt {
            title: title.to_string(),
            input: input.to_string(),
            purpose,
        };
    }

    fn ask(&mut self, question: String, then: Deferred) {
        self.overlay = Overlay::Ask { question, then };
    }

    /// The value shown for a Behaviour row.
    pub fn behaviour_value(&self, row: BhRow) -> String {
        let s = &self.settings;
        let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
        match row {
            BhRow::Leader => display_seq(
                &[self.keymap.leader],
                Key::new(
                    ratatui::crossterm::event::KeyCode::Null,
                    ratatui::crossterm::event::KeyModifiers::NONE,
                ),
            ),
            BhRow::Tabs => s.layout.tabs.clone(),
            BhRow::GroupOpen(i) => yes(s
                .colours
                .open_groups
                .iter()
                .any(|g| g.eq_ignore_ascii_case(GROUPS[i].0))),
            BhRow::PickerAtStart => s.colours.picker_at_start.clone(),
            BhRow::PickerAfterEditing => s.colours.picker_after_editing.clone(),
            BhRow::SampleAtStart => s.fonts.sample_at_start.clone(),
            BhRow::ListOpensToCurrent => yes(s.fonts.list_opens_to_current_font),
            BhRow::HideBitmap => yes(s.font_list.hide_bitmap),
            BhRow::HideSpecial => yes(s.font_list.hide_cjk_emoji_math),
            BhRow::Spacing => s.font_list.spacing.clone(),
            BhRow::NerdFont => s.font_list.nerd_font.clone(),
            BhRow::SaveSizes => "Enter".into(),
            BhRow::ResetSizes => "Enter".into(),
            BhRow::HoverHighlight => yes(s.hover.highlight),
            BhRow::HoverUnderline => s.hover.underline.clone(),
            BhRow::HoverText => yes(s.hover.text),
            BhRow::HoverTextWeight => s.hover.text_weight.clone(),
            BhRow::HoverHighlightColour => s.hover.highlight_colour.clone(),
            BhRow::HoverUnderlineColour => s.hover.underline_colour.clone(),
            BhRow::HoverTextColour => s.hover.text_colour.clone(),
            BhRow::PaneBorder => s.hover.pane_border.clone(),
            BhRow::PreviewBorder => s.hover.preview_border.clone(),
        }
    }

    pub fn behaviour_label(row: BhRow) -> String {
        match row {
            BhRow::Leader => "Leader key (of the active keymap preset)".into(),
            BhRow::Tabs => "Page tabs".into(),
            BhRow::GroupOpen(i) => format!("Colour group \"{}\" open at start", GROUPS[i].0),
            BhRow::PickerAtStart => "Colour picker at start".into(),
            BhRow::PickerAfterEditing => "Colour picker after editing".into(),
            BhRow::SampleAtStart => "Font sample at start".into(),
            BhRow::ListOpensToCurrent => "Font list opens at the role's current font".into(),
            BhRow::HideBitmap => "Font list hides bitmap fonts at start".into(),
            BhRow::HideSpecial => "Font list hides CJK / emoji / math fonts at start".into(),
            BhRow::Spacing => "Font list spacing filter at start".into(),
            BhRow::NerdFont => "Font list Nerd Font filter at start".into(),
            BhRow::SaveSizes => "Save the current pane sizes as the default".into(),
            BhRow::ResetSizes => "Pane sizes back to the shipped defaults".into(),
            BhRow::HoverHighlight => "Hover: highlight the background".into(),
            BhRow::HoverHighlightColour => "Hover: highlight colour".into(),
            BhRow::HoverUnderline => "Hover: underline".into(),
            BhRow::HoverUnderlineColour => "Hover: underline colour".into(),
            BhRow::HoverText => "Hover: change the text".into(),
            BhRow::HoverTextColour => "Hover: text colour".into(),
            BhRow::HoverTextWeight => "Hover: text weight".into(),
            BhRow::PaneBorder => "Hover: pane border colour".into(),
            BhRow::PreviewBorder => "Hover: preview window frame colour".into(),
        }
    }

    fn run_behaviour_tab(&mut self, a: Action) {
        let rows = behaviour_rows();
        if Self::move_sel(&mut self.settings_page.bh_sel, rows.len(), a) {
            return;
        }
        let d = match a {
            Action::Left => -1,
            Action::Right | Action::Confirm => 1,
            _ => return,
        };
        let cycle = |cur: &str, opts: &[&str]| -> String {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0);
            opts[wrap(i, d, opts.len())].to_string()
        };
        let row = rows[self.settings_page.bh_sel];
        // colours: Enter opens the list of keys; left/right steps through it
        if row.is_colour() {
            let keys = theme_keys();
            let cur = self.colour_setting(row).cloned().unwrap_or_default();
            let i = keys.iter().position(|k| *k == cur).unwrap_or(0);
            if a == Action::Confirm {
                self.overlay = Overlay::ChooseKey { row, sel: i };
            } else {
                self.set_colour_setting(row, keys[wrap(i, d, keys.len())]);
            }
            return;
        }
        let s = &mut self.settings;
        match row {
            BhRow::Leader => {
                self.overlay = Overlay::Capture {
                    what: Capture::Leader,
                    keys: Vec::new(),
                };
                return;
            }
            BhRow::Tabs => s.layout.tabs = cycle(&s.layout.tabs, &["bottom", "top"]),
            BhRow::GroupOpen(i) => {
                let name = GROUPS[i].0;
                let open = s
                    .colours
                    .open_groups
                    .iter()
                    .any(|g| g.eq_ignore_ascii_case(name));
                if open {
                    s.colours
                        .open_groups
                        .retain(|g| !g.eq_ignore_ascii_case(name));
                } else {
                    s.colours.open_groups.push(name.to_string());
                }
            }
            BhRow::PickerAtStart => {
                s.colours.picker_at_start = cycle(&s.colours.picker_at_start, &["hidden", "shown"])
            }
            BhRow::PickerAfterEditing => {
                s.colours.picker_after_editing = cycle(
                    &s.colours.picker_after_editing,
                    &["hide-on-esc", "stay-open"],
                )
            }
            BhRow::SampleAtStart => {
                s.fonts.sample_at_start = cycle(&s.fonts.sample_at_start, &["shown", "hidden"])
            }
            BhRow::ListOpensToCurrent => {
                s.fonts.list_opens_to_current_font = !s.fonts.list_opens_to_current_font
            }
            BhRow::HideBitmap => s.font_list.hide_bitmap = !s.font_list.hide_bitmap,
            BhRow::HideSpecial => {
                s.font_list.hide_cjk_emoji_math = !s.font_list.hide_cjk_emoji_math
            }
            BhRow::Spacing => {
                s.font_list.spacing = cycle(&s.font_list.spacing, &["any", "mono", "propo"])
            }
            BhRow::NerdFont => {
                s.font_list.nerd_font = cycle(
                    &s.font_list.nerd_font,
                    &[
                        "any",
                        "nerd-font",
                        "nerd-font-mono",
                        "nerd-font-propo",
                        "no-nerd-font",
                    ],
                )
            }
            BhRow::SaveSizes => {
                s.layout.sizes = self.sizes.clone();
                self.info("Saved the current pane sizes as the default");
            }
            BhRow::ResetSizes => {
                s.layout.sizes = crate::config::Sizes::default();
                self.sizes = s.layout.sizes.clone();
                self.info("Pane sizes back to the shipped defaults");
            }
            BhRow::HoverHighlight => s.hover.highlight = !s.hover.highlight,
            BhRow::HoverUnderline => {
                s.hover.underline = cycle(
                    &s.hover.underline,
                    &["off", "single", "double", "curly", "dotted", "dashed"],
                )
            }
            BhRow::HoverText => s.hover.text = !s.hover.text,
            BhRow::HoverTextWeight => {
                s.hover.text_weight = cycle(&s.hover.text_weight, &["normal", "bold", "dim"])
            }
            // handled above
            BhRow::HoverHighlightColour
            | BhRow::HoverUnderlineColour
            | BhRow::HoverTextColour
            | BhRow::PaneBorder
            | BhRow::PreviewBorder => {}
        }
        self.save_settings();
    }

    /// The selected file's versions (named first, then newest first);
    /// read again only after something changed.
    pub fn versions(&mut self) -> Vec<(PathBuf, SnapshotInfo)> {
        let key = (crate::history::generation(), self.settings_page.hist_file);
        if let Some((k, v)) = &self.settings_page.versions
            && *k == key
        {
            return v.clone();
        }
        let v = self
            .history()
            .map(|h| h.list_info(FILES[key.1]))
            .unwrap_or_default();
        self.settings_page.versions = Some((key, v.clone()));
        v
    }

    /// The preview of the selected version against the file now.
    pub fn diff_view(&mut self) -> DiffView {
        let versions = self.versions();
        let p = &self.settings_page;
        let sel = versions.get(p.hist_sel).map(|(path, _)| path.clone());
        let key = (
            crate::history::generation(),
            p.hist_file,
            sel.clone(),
            p.show_all,
            self.name.clone(),
        );
        if let Some((k, v)) = &self.settings_page.view
            && *k == key
        {
            return v.clone();
        }
        let file = FILES[p.hist_file];
        let now: Value = std::fs::read_to_string(self.file_path(file))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let view = match sel.as_deref().and_then(History::load) {
            Some(then) => {
                let colours = |v: &Value| -> Colours {
                    let data = v.get("palettes").unwrap_or(v);
                    data.get(&self.name)
                        .and_then(|p| p.as_object())
                        .map(|o| {
                            o.iter()
                                .filter_map(|(k, c)| Some((k.clone(), Rgb::parse(c.as_str()?)?)))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let palette =
                    (file == "palettes.json").then(|| (colours(&now), colours(&then.content)));
                DiffView {
                    lines: diff_lines(&now, &then.content, p.show_all),
                    palette,
                }
            }
            None => DiffView::default(),
        };
        self.settings_page.view = Some((key, view.clone()));
        view
    }

    fn run_history_tab(&mut self, a: Action) {
        let p = &mut self.settings_page;
        if a == Action::ToggleAll {
            p.show_all = !p.show_all;
            p.diff_scroll = 0;
            return;
        }
        if matches!(
            a,
            Action::FocusNext | Action::FocusPrev | Action::Left | Action::Right
        ) {
            p.hist_focus = match (p.hist_focus, a) {
                (HistFocus::Files, Action::Right | Action::FocusNext | Action::FocusPrev) => {
                    HistFocus::Versions
                }
                (HistFocus::Versions, Action::Left | Action::FocusNext | Action::FocusPrev) => {
                    HistFocus::Files
                }
                (f, _) => f,
            };
            return;
        }
        if matches!(a, Action::PageUp | Action::PageDown) {
            let d: i32 = if a == Action::PageUp { -10 } else { 10 };
            p.diff_scroll = (p.diff_scroll as i32 + d).max(0) as usize;
            return;
        }
        if p.hist_focus == HistFocus::Files {
            let mut i = p.hist_file;
            if Self::move_sel(&mut i, FILES.len(), a) {
                p.hist_file = i;
                p.hist_sel = 0;
                p.diff_scroll = 0;
            } else if a == Action::Confirm {
                p.hist_focus = HistFocus::Versions;
            }
            return;
        }
        let versions = self.versions();
        let p = &mut self.settings_page;
        let before = p.hist_sel;
        if Self::move_sel(&mut p.hist_sel, versions.len(), a) {
            if p.hist_sel != before {
                p.diff_scroll = 0;
            }
            return;
        }
        let file = FILES[p.hist_file];
        let selected = versions.get(p.hist_sel).cloned();
        match a {
            Action::Restore | Action::Confirm => match selected {
                Some((path, s)) => self.ask(
                    format!(
                        "Restore {file} to the version from {} ({})? What you have now goes to the history.",
                        s.when,
                        s.name.unwrap_or(s.summary)
                    ),
                    Deferred::Restore { file, version: path },
                ),
                None => self.info(format!("{file} has no saved versions yet")),
            },
            Action::SaveNamed => self.prompt(&format!("Name for the current {file}"), Naming::Version(file), ""),
            Action::Delete => match selected {
                Some((path, s)) if s.name.is_some() => {
                    self.ask(format!("Delete the version \"{}\"?", s.name.unwrap()), Deferred::DeleteVersion(path))
                }
                Some(_) => self.info("Only named versions can be deleted (snapshots are pruned on their own)"),
                None => {}
            },
            _ => {}
        }
    }

    /// The prompt's text, confirmed.
    pub fn finish_prompt(&mut self, purpose: Naming, input: String) {
        let text = input.trim().to_string();
        if text.is_empty() {
            return;
        }
        match purpose {
            Naming::Preset => {
                if self.presets.iter().any(|p| p.name == text) {
                    return self.error(format!("There's already a preset \"{text}\""));
                }
                let mut km = self.keymap.clone();
                km.name = text.clone();
                self.keymap = km;
                self.save_keymaps();
                self.info(format!("Saved the keymap as \"{text}\" (now active)"));
            }
            Naming::Version(file) => {
                let now = std::fs::read_to_string(self.file_path(file))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
                match (self.history(), now) {
                    (Some(h), Some(content)) => match h.save_named(file, &text, &content) {
                        Ok(_) => self.info(format!("Saved {file} as \"{text}\"")),
                        Err(e) => self.error(format!("{e:#}")),
                    },
                    _ => self.error(format!("Can't read {file}")),
                }
            }
            Naming::BindingText { ctx, action, add } => {
                let mut seqs = Vec::new();
                for part in text.split_whitespace() {
                    match Key::parse_seq(part, self.keymap.leader) {
                        Ok(k) => seqs.push(k),
                        Err(e) => return self.error(format!("{part}: {e}")),
                    }
                }
                self.bind(ctx, action, seqs, add);
            }
        }
    }

    /// The question was answered yes.
    pub fn run_deferred(&mut self, d: Deferred) {
        match d {
            Deferred::Restore { file, version } => {
                let Some(snap) = History::load(&version) else {
                    return self.error("That version can't be read");
                };
                match crate::store::write_json_file(&self.file_path(file), &snap.content) {
                    Ok(()) => {
                        self.reload_file(file);
                        self.settings_page.hist_sel = 0;
                        self.info(format!(
                            "Restored {file} (the version before is in the history)"
                        ));
                    }
                    Err(e) => self.error(format!("{e:#}")),
                }
            }
            Deferred::DeleteVersion(path) => {
                if let Some(h) = self.history() {
                    match h.delete(&path) {
                        Ok(()) => self.info("Deleted that version"),
                        Err(e) => self.error(format!("{e:#}")),
                    }
                }
            }
            Deferred::DeletePreset(name) => {
                self.presets.retain(|p| p.name != name);
                self.keymap = self
                    .presets
                    .iter()
                    .find(|p| p.name == "Vim")
                    .cloned()
                    .unwrap_or_else(Keymap::vim);
                self.save_keymaps();
                self.info(format!("Deleted \"{name}\"; Vim is active"));
            }
            Deferred::ResetPreset(name) => {
                if let Some(b) = Keymap::builtin().into_iter().find(|b| b.name == name) {
                    self.keymap = b;
                    self.save_keymaps();
                    self.info(format!("\"{name}\" is back to the shipped keys"));
                }
            }
        }
    }

    /// Keys while recording new keys (Enter finishes, Esc cancels,
    /// Backspace takes the last one back).
    pub fn capture_key(
        &mut self,
        what: Capture,
        mut keys: Vec<Key>,
        k: ratatui::crossterm::event::KeyEvent,
    ) {
        use ratatui::crossterm::event::KeyCode;
        let key = Key::from_event(k);
        match k.code {
            KeyCode::Esc => self.overlay = Overlay::None,
            KeyCode::Enter if !keys.is_empty() => {
                self.overlay = Overlay::None;
                match what {
                    Capture::Binding { ctx, action, add } => {
                        self.bind(ctx, action, vec![keys], add)
                    }
                    Capture::Leader => {}
                }
            }
            KeyCode::Backspace if !keys.is_empty() => {
                keys.pop();
                self.overlay = Overlay::Capture { what, keys };
            }
            _ => match what {
                Capture::Leader => {
                    self.overlay = Overlay::None;
                    self.keymap = self.keymap.with_leader(key);
                    self.save_keymaps();
                    self.info(format!("Leader is now {}", key.display()));
                }
                _ => {
                    keys.push(key);
                    self.overlay = Overlay::Capture { what, keys };
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn difference_view_shows_changes_in_place() {
        let now = json!({"version": 2, "palettes": {"a": {"red": "#111111", "bg0": "#000000"}, "gone": {"x": 1}}});
        let then = json!({"version": 2, "palettes": {"a": {"red": "#222222", "bg0": "#000000"}, "new": {"x": 2}}});
        let only = diff_lines(&now, &then, false);
        assert_eq!(
            only,
            vec![
                DiffLine::Head {
                    depth: 0,
                    key: "palettes".into()
                },
                DiffLine::Head {
                    depth: 1,
                    key: "a".into()
                },
                DiffLine::Changed {
                    depth: 2,
                    key: "red".into(),
                    now: "#111111".into(),
                    then: "#222222".into()
                },
                DiffLine::Added {
                    depth: 1,
                    key: "new".into(),
                    value: "{1 entries}".into()
                },
                DiffLine::Removed {
                    depth: 1,
                    key: "gone".into(),
                    value: "{1 entries}".into()
                },
            ]
        );
        let all = diff_lines(&now, &then, true);
        assert!(all.contains(&DiffLine::Same {
            depth: 2,
            key: "bg0".into(),
            value: "#000000".into()
        }));
        assert!(all.contains(&DiffLine::Same {
            depth: 0,
            key: "version".into(),
            value: "2".into()
        }));
        assert!(diff_lines(&now, &now, false).is_empty());
    }
}
