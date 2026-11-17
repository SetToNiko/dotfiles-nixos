//! The Settings page (F3): Keymaps, Behaviour and History tabs. Keymaps and
//! Behaviour changes wait in a draft until Save (see draft.rs); History acts
//! at once. Every write goes through the history, so it can be undone.

use std::path::PathBuf;

use serde_json::Value;

use crate::app::{App, Overlay};
use crate::color::Rgb;
use crate::history::{History, SnapshotInfo};
use crate::keymap::{Action, Context, Key, Keymap, all_actions, display_seq};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Keymaps,
    Behaviour,
    History,
    /// docs/theme-studio.md, rendered (docs.rs).
    Readme,
}

pub const TABS: [(Tab, &str); 4] = [
    (Tab::Keymaps, "Keymaps"),
    (Tab::Behaviour, "Behaviour"),
    (Tab::History, "History"),
    (Tab::Readme, "README"),
];

/// A row of the Keymaps tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KmRow {
    /// The preset button (opens the preset list).
    Preset,
    /// A blank line before a group (not selectable).
    Gap,
    /// A group's title (not selectable).
    Header(Context),
    Binding(Context, Action),
}

/// The files with a history, as the History tab lists them.
pub const FILES: [&str; 5] = [
    "themes.json",
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
    /// A new preset, a copy of the named one (the preset list).
    PresetFrom(String),
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
    /// Deletes the preview fonts (Behaviour tab).
    ClearPreviews,
    /// Starts a tournament of a pool (Fonts page, Tournament tab).
    StartTournament(usize),
    /// Downloads a family.s Nerd Font (catalogue id), then fills these
    /// roles from it.
    NerdForRoles {
        id: String,
        group: String,
        roles: Vec<usize>,
    },
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
    /// The keyboard is on the tab strip (left/right change tabs).
    pub tabs_focused: bool,
    /// Unsaved Keymaps / Behaviour changes.
    pub draft: Option<crate::draft::Draft>,
    /// A config file was reloaded during a draft edit (see with_draft).
    pub reloaded: bool,
    /// Keymaps tab: closed groups.
    pub km_closed: std::collections::HashSet<Context>,
    /// Behaviour tab: closed groups, by title.
    pub bh_closed: std::collections::HashSet<&'static str>,
    /// Keymaps tab: browsing presets, with this one highlighted.
    pub browse: Option<usize>,
    pub browse_scroll: usize,
    /// The preset list's difference shows every action.
    pub browse_all: bool,
    /// The difference view shows everything, not just what differs.
    pub show_all: bool,
    pub diff_scroll: usize,
    /// README tab: the guide's text with the file's modified time (read
    /// again when that changes), its render for the last width, and the
    /// first line shown.
    pub readme: Option<(
        Option<std::time::SystemTime>,
        std::result::Result<String, String>,
    )>,
    pub readme_doc: Option<(u16, crate::docs::Doc)>,
    pub readme_scroll: usize,
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
            tabs_focused: false,
            draft: None,
            reloaded: false,
            km_closed: Default::default(),
            bh_closed: Default::default(),
            browse: None,
            browse_scroll: 0,
            browse_all: false,
            show_all: false,
            diff_scroll: 0,
            readme: None,
            readme_doc: None,
            readme_scroll: 0,
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

/// The first and last rows the cursor can rest on.
pub(crate) fn ends(ok: &[bool]) -> (usize, usize) {
    (
        ok.iter().position(|o| *o).unwrap_or(0),
        ok.iter().rposition(|o| *o).unwrap_or(0),
    )
}

/// Moves `sel` over the rows where `ok` is true (skipping titles and
/// blank lines); false if `a` isn't a move.
pub(crate) fn move_over(sel: &mut usize, ok: &[bool], a: Action) -> bool {
    let next = |from: usize, up: bool| -> usize {
        let mut i = from;
        loop {
            if up {
                if i == 0 {
                    return from;
                }
                i -= 1;
            } else {
                if i + 1 >= ok.len() {
                    return from;
                }
                i += 1;
            }
            if ok[i] {
                return i;
            }
        }
    };
    let (first, last) = ends(ok);
    let s = (*sel).min(ok.len().saturating_sub(1));
    *sel = match a {
        Action::Up => next(s, true),
        Action::Down => next(s, false),
        Action::PageUp => (0..10).fold(s, |s, _| next(s, true)),
        Action::PageDown => (0..10).fold(s, |s, _| next(s, false)),
        Action::Top => first,
        Action::Bottom => last,
        _ => return false,
    };
    true
}

pub(crate) fn wrap(i: usize, d: i32, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (i as i32 + d).rem_euclid(n as i32) as usize
    }
}

impl App {
    // ----- shared ----------------------------------------------------------

    pub fn history(&self) -> Option<History> {
        History::current()
    }

    /// Where a file with a history lives.
    pub fn file_path(&self, file: &str) -> PathBuf {
        match file {
            "themes.json" | "palettes.json" | "fonts.json" => self.store.dir.join(file),
            _ => self.config_dir.join(file),
        }
    }

    /// Copies the active keymap into its preset (in the draft: Save writes).
    pub(crate) fn sync_preset(&mut self) {
        let active = self.keymap.name.clone();
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == active) {
            *p = self.keymap.clone();
        } else {
            self.presets.push(self.keymap.clone());
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
                // unsaved Settings page changes were made against the old file
                if self.settings_page.draft.take().is_some() && !self.settings_page.reloaded {
                    self.info("The unsaved settings were dropped");
                }
                self.settings_page.reloaded = true;
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
        if let T::PresetRow(i) = t {
            return self.with_draft(|app| app.preset_click(i));
        }
        let p = &mut self.settings_page;
        if !matches!(t, T::SettingsTab(_)) {
            p.tabs_focused = false;
        }
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
                    Tab::Readme => return,
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
        // the README: three lines a notch (and never up onto the tabs)
        if self.settings_page.tab == Tab::Readme {
            for _ in 0..3 {
                self.run_readme_tab(if d > 0 { Action::Down } else { Action::Up });
            }
            return;
        }
        if t == T::SettingsDiff && self.settings_page.browse.is_some() {
            let p = &mut self.settings_page;
            p.browse_scroll = (p.browse_scroll as i32 + d * 3).max(0) as usize;
            return;
        }
        if let T::PresetRow(_) = t {
            return self
                .with_draft(|app| app.run_browse(if d > 0 { Action::Down } else { Action::Up }));
        }
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
        let top = self.settings.layout.inner_tabs_on_top();
        let switch = |p: &mut SettingsPage, d: i32| {
            let i = TABS.iter().position(|(t, _)| *t == p.tab).unwrap_or(0);
            p.tab = TABS[wrap(i, d, TABS.len())].0;
        };
        // the keyboard on the tab strip: left/right change tabs; moving back
        // towards the page, Enter or Esc return to it
        if self.settings_page.tabs_focused {
            match a {
                Action::Left | Action::TabPrev => switch(&mut self.settings_page, -1),
                Action::Right | Action::TabNext => switch(&mut self.settings_page, 1),
                Action::Down if top => self.settings_page.tabs_focused = false,
                Action::Up if !top => self.settings_page.tabs_focused = false,
                // on past the page's tabs to the page tabs (F1 / F2 / F3)
                Action::Up if top => {
                    self.focus_page_tabs(true);
                }
                Action::Down if !top => {
                    self.focus_page_tabs(false);
                }
                Action::Confirm | Action::Back => self.settings_page.tabs_focused = false,
                _ => {}
            }
            return;
        }
        match a {
            Action::TabNext | Action::TabPrev => {
                switch(
                    &mut self.settings_page,
                    if a == Action::TabNext { 1 } else { -1 },
                );
                return;
            }
            // the preset's Save key saves the page; Discard drops the changes
            Action::Save => return self.settings_save(),
            Action::Discard => return self.settings_discard(),
            // Esc closes the preset list first, then leaves the page
            Action::Back if self.settings_page.browse.is_some() => {
                self.settings_page.browse = None;
                return;
            }
            Action::Back => {
                self.page = crate::app::Page::Main;
                return;
            }
            // past the edge of the list next to the tabs: onto the tabs
            Action::Up if top && self.settings_at_edge(true) => {
                self.settings_page.tabs_focused = true;
                return;
            }
            Action::Down if !top && self.settings_at_edge(false) => {
                self.settings_page.tabs_focused = true;
                return;
            }
            // the other edge: on to the page tabs, if they're there
            Action::Up if self.settings_at_edge(true) && self.focus_page_tabs(true) => return,
            Action::Down if self.settings_at_edge(false) && self.focus_page_tabs(false) => return,
            _ => {}
        }
        // Keymaps and Behaviour edit the draft; History acts at once
        match self.settings_page.tab {
            Tab::Keymaps => self.with_draft(|app| app.run_keymaps_tab(a)),
            Tab::Behaviour => self.with_draft(|app| app.run_behaviour_tab(a)),
            Tab::History => self.run_history_tab(a),
            Tab::Readme => self.run_readme_tab(a),
        }
    }

    /// README tab keys: scrolling, paging, and jumping between headings.
    fn run_readme_tab(&mut self, a: Action) {
        let p = &mut self.settings_page;
        let Some((_, doc)) = &p.readme_doc else {
            return;
        };
        let last = doc.lines.len().saturating_sub(1);
        let s = &mut p.readme_scroll;
        match a {
            Action::Down => *s = (*s + 1).min(last),
            Action::Up => *s = s.saturating_sub(1),
            Action::PageDown => *s = (*s + 20).min(last),
            Action::PageUp => *s = s.saturating_sub(20),
            Action::Top => *s = 0,
            Action::Bottom => *s = last,
            Action::NextHeading => {
                if let Some(h) = doc.headings.iter().find(|&&h| h > *s) {
                    *s = *h;
                }
            }
            Action::PrevHeading => {
                if let Some(h) = doc.headings.iter().rev().find(|&&h| h < *s) {
                    *s = *h;
                }
            }
            _ => {}
        }
    }

    /// The cursor is on the first (`up`) or last row of the tab's list.
    fn settings_at_edge(&mut self, up: bool) -> bool {
        let at = |sel: usize, n: usize| if up { sel == 0 } else { sel + 1 >= n };
        match self.settings_page.tab {
            Tab::Keymaps if self.settings_page.browse.is_some() => false,
            Tab::Keymaps => {
                let ok = self.show_draft(|app| Self::keymap_selectable(&app.keymap_rows()));
                let (first, last) = ends(&ok);
                self.settings_page.km_sel == if up { first } else { last }
            }
            Tab::Behaviour => self.behaviour_at_edge(up),
            // scrolled to the top: on to the tabs (never at the bottom, so
            // the last lines can be scrolled to)
            Tab::Readme => up && self.settings_page.readme_scroll == 0,
            Tab::History => match self.settings_page.hist_focus {
                HistFocus::Files => at(self.settings_page.hist_file, FILES.len()),
                HistFocus::Versions => {
                    let n = self.versions().len();
                    at(self.settings_page.hist_sel, n)
                }
            },
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
            // a blank line before each group's title; closed groups show
            // only the title
            rows.push(KmRow::Gap);
            rows.push(KmRow::Header(ctx));
            if !self.settings_page.km_closed.contains(&ctx) {
                rows.extend(actions.into_iter().map(|a| KmRow::Binding(ctx, a)));
            }
        }
        rows
    }

    /// The cursor can rest on everything but the blank lines.
    pub fn keymap_selectable(rows: &[KmRow]) -> Vec<bool> {
        rows.iter().map(|r| !matches!(r, KmRow::Gap)).collect()
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

    fn run_keymaps_tab(&mut self, a: Action) {
        if self.settings_page.browse.is_some() {
            return self.run_browse(a);
        }
        let rows = self.keymap_rows();
        if move_over(
            &mut self.settings_page.km_sel,
            &Self::keymap_selectable(&rows),
            a,
        ) {
            return;
        }
        let row = rows.get(self.settings_page.km_sel).copied();
        let builtin = Keymap::builtin().iter().any(|b| b.name == self.keymap.name);
        match (row, a) {
            // the preset button opens the preset list
            (Some(KmRow::Preset), Action::Right | Action::Confirm) => self.open_browse(),
            // a group's title: Enter toggles it, right opens, left closes
            (Some(KmRow::Header(ctx)), Action::Confirm | Action::Right | Action::Left) => {
                let closed = &mut self.settings_page.km_closed;
                let open = !closed.contains(&ctx);
                if (a == Action::Confirm && open) || a == Action::Left {
                    closed.insert(ctx);
                } else {
                    closed.remove(&ctx);
                }
            }
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
                self.sync_preset();
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
        self.sync_preset();
        if clashes.is_empty() {
            self.info(format!("New keys for \"{}\" (unsaved)", action.describe()));
        } else {
            self.error(format!("New keys (unsaved), but: {}", clashes.join("; ")));
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
            Naming::PresetFrom(from) => self.new_preset_from(&from, text),
            Naming::Preset => {
                if self.presets.iter().any(|p| p.name == text) {
                    return self.error(format!("There's already a preset \"{text}\""));
                }
                let mut km = self.keymap.clone();
                km.name = text.clone();
                self.keymap = km;
                self.sync_preset();
                self.info(format!("New preset \"{text}\", now active (unsaved)"));
            }
            Naming::Version(file) => {
                let now = std::fs::read_to_string(self.file_path(file))
                    .ok()
                    .and_then(|t| serde_json::from_str(&t).ok());
                match (self.history(), now) {
                    (Some(h), Some(content)) => match h.save_named(file, &text, &content) {
                        Ok(_) => self.notify_saved(format!("Saved {file} as \"{text}\"")),
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
            Deferred::DeletePreset(name) => self.delete_preset(&name),
            Deferred::ResetPreset(name) => self.reset_preset(&name),
            Deferred::StartTournament(pool) => self.tour_start(pool),
            Deferred::NerdForRoles { id, group, roles } => {
                self.install_nerd_for_roles(id, group, roles)
            }
            Deferred::ClearPreviews => match crate::download::clear_previews() {
                Ok(()) => {
                    let pv = self.pv();
                    pv.families.clear();
                    pv.image = None;
                    self.prefetch.reset();
                    self.info("Deleted the preview fonts");
                }
                Err(e) => self.error(format!("{e:#}")),
            },
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
                    self.sync_preset();
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
