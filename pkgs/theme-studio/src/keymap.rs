//! Keymaps: every key-driven action has a name and a context; bindings are
//! keys or sequences in Vim notation (`j`, `<C-r>`, `<leader>sp`). Text
//! fields (hex, search, names) don't go through the keymap.

use std::collections::HashSet;

use serde_json::{Map, Value};

use anyhow::{Result, anyhow, bail};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// One key press, normalised: Shift is part of the character for letters
/// and symbols (`J`, `?`), and Shift+Tab is `<S-Tab>`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub mods: KeyModifiers,
}

impl Key {
    pub const fn new(code: KeyCode, mods: KeyModifiers) -> Key {
        Key { code, mods }
    }

    pub fn from_event(k: KeyEvent) -> Key {
        let mut mods =
            k.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SHIFT);
        match k.code {
            KeyCode::Char(_) | KeyCode::BackTab => mods.remove(KeyModifiers::SHIFT),
            _ => {}
        }
        Key { code: k.code, mods }
    }

    /// Parses one key sequence, e.g. `<leader>cp`, `gg`, `<C-x><C-s>`.
    pub fn parse_seq(s: &str, leader: Key) -> Result<Vec<Key>> {
        let mut out = Vec::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '<' && s.contains('>') {
                let mut name = String::new();
                for c in chars.by_ref() {
                    if c == '>' {
                        break;
                    }
                    name.push(c);
                }
                out.push(Key::parse_named(&name, leader)?);
            } else {
                out.push(Key::new(KeyCode::Char(c), KeyModifiers::NONE));
            }
        }
        if out.is_empty() {
            bail!("empty key sequence");
        }
        Ok(out)
    }

    /// `leader`, `space`, `CR`, `C-r`, `S-Tab`, `M-x`, `F5`, `Up`, ….
    fn parse_named(name: &str, leader: Key) -> Result<Key> {
        let lower = name.to_lowercase();
        if lower == "leader" {
            return Ok(leader);
        }
        let mut mods = KeyModifiers::NONE;
        let mut rest = name;
        loop {
            let l = rest.to_lowercase();
            if l.len() > 2 && l.starts_with("c-") {
                mods |= KeyModifiers::CONTROL;
            } else if l.len() > 2 && (l.starts_with("m-") || l.starts_with("a-")) {
                mods |= KeyModifiers::ALT;
            } else if l.len() > 2 && l.starts_with("s-") {
                mods |= KeyModifiers::SHIFT;
            } else {
                break;
            }
            rest = &rest[2..];
        }
        let code = match rest.to_lowercase().as_str() {
            "space" => KeyCode::Char(' '),
            "cr" | "enter" | "return" => KeyCode::Enter,
            "esc" => KeyCode::Esc,
            "tab" if mods.contains(KeyModifiers::SHIFT) => {
                mods.remove(KeyModifiers::SHIFT);
                KeyCode::BackTab
            }
            "tab" => KeyCode::Tab,
            "bs" | "backspace" => KeyCode::Backspace,
            "del" | "delete" => KeyCode::Delete,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "lt" => KeyCode::Char('<'),
            f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
                KeyCode::F(f[1..].parse().unwrap())
            }
            _ if rest.chars().count() == 1 => {
                let c = rest.chars().next().unwrap();
                // Shift on a character is its capital, as key events report it:
                // <C-S-z> and <C-Z> are both Ctrl+Shift+Z
                if mods.contains(KeyModifiers::SHIFT) {
                    mods.remove(KeyModifiers::SHIFT);
                    return Ok(Key {
                        code: KeyCode::Char(c.to_ascii_uppercase()),
                        mods,
                    });
                }
                KeyCode::Char(c)
            }
            _ => return Err(anyhow!("unknown key <{name}>")),
        };
        Ok(Key { code, mods })
    }

    /// Vim notation, e.g. `j`, `<C-r>`, `<S-Tab>`, `<Space>`.
    pub fn display(&self) -> String {
        let mut prefix = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            prefix.push_str("C-");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            prefix.push_str("M-");
        }
        if self.mods.contains(KeyModifiers::SHIFT) {
            prefix.push_str("S-");
        }
        let name = match self.code {
            KeyCode::Char(' ') => "Space".to_string(),
            KeyCode::Char('<') => "lt".to_string(),
            KeyCode::Char(c) if prefix.is_empty() => return c.to_string(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "CR".into(),
            KeyCode::Esc => "Esc".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::BackTab => "S-Tab".into(),
            KeyCode::Backspace => "BS".into(),
            KeyCode::Delete => "Del".into(),
            KeyCode::Up => "Up".into(),
            KeyCode::Down => "Down".into(),
            KeyCode::Left => "Left".into(),
            KeyCode::Right => "Right".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            KeyCode::PageUp => "PageUp".into(),
            KeyCode::PageDown => "PageDown".into(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        };
        format!("<{prefix}{name}>")
    }
}

/// Display a sequence; the leader is shown as `<leader>`.
pub fn display_seq(keys: &[Key], leader: Key) -> String {
    keys.iter()
        .map(|k| {
            if *k == leader {
                "<leader>".to_string()
            } else {
                k.display()
            }
        })
        .collect()
}

/// Where a binding applies. Panes look in their own context first, then
/// their column's, then Global.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Context {
    Global,
    /// The colour column (list and picker)
    Colours,
    ColourList,
    Picker,
    /// The fonts column (roles and list)
    Fonts,
    FontRoles,
    FontList,
}

impl Context {
    pub const ALL: [Context; 7] = [
        Context::Global,
        Context::Colours,
        Context::ColourList,
        Context::Picker,
        Context::Fonts,
        Context::FontRoles,
        Context::FontList,
    ];

    /// The context's name in keymaps.json.
    pub fn id(self) -> &'static str {
        match self {
            Context::Global => "global",
            Context::Colours => "colours",
            Context::ColourList => "colour-list",
            Context::Picker => "picker",
            Context::Fonts => "fonts",
            Context::FontRoles => "font-roles",
            Context::FontList => "font-list",
        }
    }

    pub fn from_id(id: &str) -> Option<Context> {
        Context::ALL.into_iter().find(|c| c.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Context::Global => "Everywhere",
            Context::Colours => "Colours",
            Context::ColourList => "Colour list",
            Context::Picker => "Colour picker",
            Context::Fonts => "Fonts",
            Context::FontRoles => "Font roles",
            Context::FontList => "Font list",
        }
    }
}

/// Everything a key can do. Movement actions mean the obvious thing in each
/// pane (Right in the colour list starts editing, in the font list it opens
/// a group, in the font roles it makes the font bigger).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Quit,
    Help,
    PageMain,
    PageDownloads,
    PageSettings,
    TogglePicker,
    ToggleSample,
    PaneLeft,
    PaneRight,
    PaneUp,
    PaneDown,
    PreviewCycle,
    SavePalette,
    SaveFonts,
    ApplyPalette,
    ApplyFonts,
    Save,
    SaveAs,
    Apply,
    Open,
    New,
    Undo,
    Redo,
    Panel(u8),
    FocusNext,
    FocusPrev,
    Back,
    Confirm,
    Up,
    Down,
    Left,
    Right,
    UpBig,
    DownBig,
    LeftBig,
    RightBig,
    Top,
    Bottom,
    PageUp,
    PageDown,
    Pin,
    CopyHex,
    CopyFrom,
    HexEntry,
    ToggleYazi,
    ToggleRofi,
    ToggleView,
    SizeUp,
    SizeDown,
    PickFont,
    Search,
    Filter(char),
}

/// Every action, in the order the keymaps file and the Settings page list them.
pub fn all_actions() -> Vec<Action> {
    use Action as A;
    let mut v = vec![
        A::Quit,
        A::Help,
        A::PageMain,
        A::PageDownloads,
        A::PageSettings,
        A::TogglePicker,
        A::ToggleSample,
        A::PaneLeft,
        A::PaneRight,
        A::PaneUp,
        A::PaneDown,
        A::PreviewCycle,
        A::SavePalette,
        A::SaveFonts,
        A::ApplyPalette,
        A::ApplyFonts,
        A::Save,
        A::SaveAs,
        A::Apply,
        A::Open,
        A::New,
        A::Undo,
        A::Redo,
    ];
    v.extend((1..=9).map(A::Panel));
    v.extend([
        A::FocusNext,
        A::FocusPrev,
        A::Back,
        A::Confirm,
        A::Up,
        A::Down,
        A::Left,
        A::Right,
        A::UpBig,
        A::DownBig,
        A::LeftBig,
        A::RightBig,
        A::Top,
        A::Bottom,
        A::PageUp,
        A::PageDown,
        A::Pin,
        A::CopyHex,
        A::CopyFrom,
        A::HexEntry,
        A::ToggleYazi,
        A::ToggleRofi,
        A::ToggleView,
        A::SizeUp,
        A::SizeDown,
        A::PickFont,
        A::Search,
    ]);
    v.extend(crate::fonts::CHIPS.iter().map(|(c, _)| A::Filter(*c)));
    v
}

/// Names for the filter chips in the keymaps file.
const FILTER_IDS: [(char, &str); 8] = [
    ('m', "filter-mono"),
    ('p', "filter-propo"),
    ('n', "filter-nerd-font"),
    ('M', "filter-nerd-font-mono"),
    ('P', "filter-nerd-font-propo"),
    ('x', "filter-no-nerd-font"),
    ('b', "show-bitmap"),
    ('c', "show-cjk-emoji-math"),
];

impl Action {
    /// The action's name in keymaps.json.
    pub fn id(self) -> String {
        use Action as A;
        let s = match self {
            A::Quit => "quit",
            A::Help => "help",
            A::PageMain => "page-main",
            A::PageDownloads => "page-downloads",
            A::PageSettings => "page-settings",
            A::TogglePicker => "toggle-picker",
            A::ToggleSample => "toggle-sample",
            A::PaneLeft => "pane-left",
            A::PaneRight => "pane-right",
            A::PaneUp => "pane-up",
            A::PaneDown => "pane-down",
            A::PreviewCycle => "preview-cycle",
            A::SavePalette => "save-palette",
            A::SaveFonts => "save-fonts",
            A::ApplyPalette => "apply-palette",
            A::ApplyFonts => "apply-fonts",
            A::Save => "save",
            A::SaveAs => "save-as",
            A::Apply => "apply",
            A::Open => "open",
            A::New => "new",
            A::Undo => "undo",
            A::Redo => "redo",
            A::Panel(n) => return format!("panel-{n}"),
            A::FocusNext => "focus-next",
            A::FocusPrev => "focus-previous",
            A::Back => "back",
            A::Confirm => "confirm",
            A::Up => "up",
            A::Down => "down",
            A::Left => "left",
            A::Right => "right",
            A::UpBig => "up-big",
            A::DownBig => "down-big",
            A::LeftBig => "left-big",
            A::RightBig => "right-big",
            A::Top => "top",
            A::Bottom => "bottom",
            A::PageUp => "page-up",
            A::PageDown => "page-down",
            A::Pin => "pin",
            A::CopyHex => "copy-hex",
            A::CopyFrom => "copy-from",
            A::HexEntry => "hex-entry",
            A::ToggleYazi => "toggle-yazi",
            A::ToggleRofi => "toggle-rofi",
            A::ToggleView => "toggle-view",
            A::SizeUp => "size-up",
            A::SizeDown => "size-down",
            A::PickFont => "pick-font",
            A::Search => "search",
            A::Filter(c) => FILTER_IDS
                .iter()
                .find(|(k, _)| *k == c)
                .map(|(_, id)| *id)
                .unwrap_or("filter"),
        };
        s.to_string()
    }

    /// Names from earlier versions: renamed, or (None) dropped without a warning.
    pub fn legacy(id: &str) -> Option<Option<Action>> {
        match id {
            "tab-colours" => Some(Some(Action::PageMain)),
            "tab-fonts" => Some(None),
            _ => None,
        }
    }

    pub fn from_id(id: &str) -> Option<Action> {
        all_actions().into_iter().find(|a| a.id() == id)
    }
    /// Short description, for the help screen, hints and the leader popup.
    pub fn describe(self) -> String {
        match self {
            Action::Quit => "quit".into(),
            Action::Help => "all keys".into(),
            Action::PageMain => "main page".into(),
            Action::PageDownloads => "download fonts page".into(),
            Action::PageSettings => "settings page".into(),
            Action::TogglePicker => "show / hide the colour picker".into(),
            Action::ToggleSample => "show / hide the font sample".into(),
            Action::PaneLeft => "pane to the left".into(),
            Action::PaneRight => "pane to the right".into(),
            Action::PaneUp => "pane above".into(),
            Action::PaneDown => "pane below".into(),
            Action::PreviewCycle => "type into the preview / next window / back".into(),
            Action::SavePalette => "save palette".into(),
            Action::SaveFonts => "save font preset".into(),
            Action::ApplyPalette => "save and apply palette".into(),
            Action::ApplyFonts => "save and apply font preset".into(),
            Action::Save => "save".into(),
            Action::SaveAs => "save as".into(),
            Action::Apply => "save and apply".into(),
            Action::Open => "open".into(),
            Action::New => "new".into(),
            Action::Undo => "undo".into(),
            Action::Redo => "redo".into(),
            Action::Panel(n) => format!("panel {n}"),
            Action::FocusNext => "next control".into(),
            Action::FocusPrev => "previous control".into(),
            Action::Back => "back".into(),
            Action::Confirm => "edit / use / open".into(),
            Action::Up => "up".into(),
            Action::Down => "down".into(),
            Action::Left => "left / smaller / close".into(),
            Action::Right => "right / bigger / open".into(),
            Action::UpBig => "up (big step)".into(),
            Action::DownBig => "down (big step)".into(),
            Action::LeftBig => "left (big step)".into(),
            Action::RightBig => "right (big step)".into(),
            Action::Top => "first".into(),
            Action::Bottom => "last".into(),
            Action::PageUp => "page up".into(),
            Action::PageDown => "page down".into(),
            Action::Pin => "pin / follow again".into(),
            Action::CopyHex => "copy hex".into(),
            Action::CopyFrom => "copy another key's colour".into(),
            Action::HexEntry => "type a hex colour".into(),
            Action::ToggleYazi => "preview: shell / yazi".into(),
            Action::ToggleRofi => "preview: rofi menu".into(),
            Action::ToggleView => "preview: desktop / swatches".into(),
            Action::SizeUp => "font bigger".into(),
            Action::SizeDown => "font smaller".into(),
            Action::PickFont => "pick a font from the list".into(),
            Action::Search => "search".into(),
            Action::Filter(c) => {
                let label = crate::fonts::CHIPS
                    .iter()
                    .find(|(k, _)| *k == c)
                    .map(|(_, l)| *l)
                    .unwrap_or("?");
                format!("filter: {label}")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub context: Context,
    pub keys: Vec<Key>,
    pub action: Action,
}

/// The result of looking up the keys pressed so far.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lookup {
    Run(Action),
    /// The keys so far start a longer sequence: wait for more.
    Pending,
    None,
}

#[derive(Clone, Debug)]
pub struct Keymap {
    /// (context, action) pairs deliberately left without keys.
    pub unbound: HashSet<(Context, Action)>,
    pub name: String,
    pub leader: Key,
    pub bindings: Vec<Binding>,
}

impl Keymap {
    /// Builds a keymap from (context, action, sequences) rows.
    pub fn from_rows(
        name: &str,
        leader: Key,
        rows: &[(Context, Action, &[&str])],
    ) -> Result<Keymap> {
        let mut bindings = Vec::new();
        for (context, action, seqs) in rows {
            for s in *seqs {
                bindings.push(Binding {
                    context: *context,
                    keys: Key::parse_seq(s, leader)?,
                    action: *action,
                });
            }
        }
        Ok(Keymap {
            name: name.to_string(),
            leader,
            bindings,
            unbound: HashSet::new(),
        })
    }

    /// The keys pressed so far, in the given contexts (most specific first).
    pub fn lookup(&self, contexts: &[Context], seq: &[Key]) -> Lookup {
        let active = |b: &&Binding| contexts.contains(&b.context);
        if self
            .bindings
            .iter()
            .filter(active)
            .any(|b| b.keys.len() > seq.len() && b.keys.starts_with(seq))
        {
            return Lookup::Pending;
        }
        for c in contexts {
            if let Some(b) = self
                .bindings
                .iter()
                .find(|b| b.context == *c && b.keys == seq)
            {
                return Lookup::Run(b.action);
            }
        }
        Lookup::None
    }

    /// What can follow `prefix` (for the leader popup): next key, and what
    /// it does (or "…" when it starts a longer sequence).
    pub fn continuations(&self, contexts: &[Context], prefix: &[Key]) -> Vec<(String, String)> {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for c in contexts {
            for b in self.bindings.iter().filter(|b| b.context == *c) {
                if b.keys.len() > prefix.len() && b.keys.starts_with(prefix) {
                    let next = b.keys[prefix.len()];
                    if !seen.insert(next) {
                        continue;
                    }
                    let what = if b.keys.len() == prefix.len() + 1 {
                        b.action.describe()
                    } else {
                        "…".to_string()
                    };
                    out.push((next.display(), what));
                }
            }
        }
        out
    }

    /// The first sequence bound to `action` in one of the contexts, as text.
    pub fn key_for(&self, contexts: &[Context], action: Action) -> Option<String> {
        contexts.iter().find_map(|c| {
            self.bindings
                .iter()
                .find(|b| b.context == *c && b.action == action)
                .map(|b| display_seq(&b.keys, self.leader))
        })
    }

    /// The preset as stored in keymaps.json:
    /// `{ "leader": "<Space>", "bindings": { context: { action: [keys…] } } }`.
    /// Actions with no keys in a context are left out of that context.
    pub fn to_json(&self) -> Value {
        let mut contexts = Map::new();
        for ctx in Context::ALL {
            let mut actions = Map::new();
            for a in all_actions() {
                let seqs: Vec<Value> = self
                    .bindings
                    .iter()
                    .filter(|b| b.context == ctx && b.action == a)
                    .map(|b| Value::String(display_seq(&b.keys, self.leader)))
                    .collect();
                if !seqs.is_empty() || self.unbound.contains(&(ctx, a)) {
                    actions.insert(a.id(), Value::Array(seqs));
                }
            }
            if !actions.is_empty() {
                contexts.insert(ctx.id().to_string(), Value::Object(actions));
            }
        }
        let mut m = Map::new();
        m.insert("leader".into(), Value::String(self.leader.display()));
        m.insert("bindings".into(), Value::Object(contexts));
        Value::Object(m)
    }

    /// Reads a preset from keymaps.json. Problems (unknown names, keys that
    /// don't parse) are skipped and reported. With `base`, actions the file
    /// doesn't mention for a context keep `base`'s keys, so actions added in
    /// a newer version show up; an empty list means deliberately unbound.
    pub fn from_json(name: &str, v: &Value, base: Option<&Keymap>) -> (Keymap, Vec<String>) {
        let mut warnings = Vec::new();
        let space = Key::new(KeyCode::Char(' '), KeyModifiers::NONE);
        let leader = match v.get("leader").and_then(Value::as_str) {
            Some(s) => match Key::parse_seq(s, space) {
                Ok(k) if k.len() == 1 => k[0],
                _ => {
                    warnings.push(format!("{name}: leader \"{s}\" isn't one key; using Space"));
                    space
                }
            },
            None => base.map(|b| b.leader).unwrap_or(space),
        };
        let mut km = Keymap {
            name: name.to_string(),
            leader,
            bindings: Vec::new(),
            unbound: HashSet::new(),
        };
        let mut mentioned: HashSet<(Context, Action)> = HashSet::new();
        if let Some(contexts) = v.get("bindings").and_then(Value::as_object) {
            for (cid, actions) in contexts {
                let Some(ctx) = Context::from_id(cid) else {
                    warnings.push(format!("{name}: unknown context \"{cid}\""));
                    continue;
                };
                let Some(actions) = actions.as_object() else {
                    continue;
                };
                for (aid, seqs) in actions {
                    let action = match (Action::from_id(aid), Action::legacy(aid)) {
                        (Some(a), _) => a,
                        (None, Some(Some(renamed))) => renamed,
                        (None, Some(None)) => continue, // no longer exists
                        (None, None) => {
                            warnings.push(format!("{name}: unknown action \"{aid}\""));
                            continue;
                        }
                    };
                    mentioned.insert((ctx, action));
                    let list = seqs.as_array().cloned().unwrap_or_default();
                    if list.is_empty() {
                        km.unbound.insert((ctx, action));
                    }
                    for s in list.iter().filter_map(Value::as_str) {
                        match Key::parse_seq(s, leader) {
                            Ok(keys) => km.bindings.push(Binding {
                                context: ctx,
                                keys,
                                action,
                            }),
                            Err(e) => warnings.push(format!("{name}: {cid}.{aid}: {e}")),
                        }
                    }
                }
            }
        }
        if let Some(base) = base {
            for b in &base.bindings {
                if !mentioned.contains(&(b.context, b.action)) {
                    // re-read with this preset's leader
                    let text = display_seq(&b.keys, base.leader);
                    if let Ok(keys) = Key::parse_seq(&text, leader) {
                        km.bindings.push(Binding {
                            context: b.context,
                            keys,
                            action: b.action,
                        });
                    }
                }
            }
        }
        (km, warnings)
    }

    /// The presets that ship with the studio.
    pub fn builtin() -> Vec<Keymap> {
        vec![Keymap::vim()]
    }

    /// The shipped Vim preset: the studio's original keys, plus leader
    /// sequences for saving and applying either side.
    pub fn vim() -> Keymap {
        use Action as A;
        use Context as C;
        let mut rows: Vec<(Context, Action, &[&str])> = vec![
            (C::Global, A::Quit, &["q"]),
            (C::Global, A::Help, &["?"]),
            (C::Global, A::PageMain, &["<F1>"]),
            (C::Global, A::PageDownloads, &["<F2>"]),
            (C::Global, A::PageSettings, &["<F3>"]),
            (C::Global, A::PaneLeft, &["<C-w>h"]),
            (C::Global, A::PaneRight, &["<C-w>l"]),
            (C::Global, A::PaneUp, &["<C-w>k"]),
            (C::Global, A::PaneDown, &["<C-w>j"]),
            (C::Global, A::Panel(5), &["5"]),
            (C::Global, A::Panel(6), &["6"]),
            (C::Global, A::Panel(7), &["7"]),
            (C::Colours, A::TogglePicker, &["<leader>cp"]),
            (C::Fonts, A::ToggleSample, &["<leader>fs"]),
            (C::Global, A::PreviewCycle, &["<F12>"]),
            (C::Global, A::SavePalette, &["<leader>sp"]),
            (C::Global, A::SaveFonts, &["<leader>sf"]),
            (C::Global, A::ApplyPalette, &["<leader>ap"]),
            (C::Global, A::ApplyFonts, &["<leader>af"]),
            (C::Global, A::Save, &["s"]),
            (C::Global, A::SaveAs, &["S"]),
            (C::Global, A::Apply, &["a"]),
            (C::Global, A::Open, &["o"]),
            (C::Global, A::New, &["n"]),
            (C::Global, A::Panel(1), &["1"]),
            (C::Global, A::Panel(2), &["2"]),
            (C::Global, A::Panel(3), &["3"]),
            (C::Global, A::Panel(4), &["4"]),
            (C::Colours, A::Undo, &["u"]),
            (C::Colours, A::Redo, &["<C-r>"]),
            (C::Colours, A::Pin, &["p"]),
            (C::Colours, A::CopyHex, &["y"]),
            (C::Colours, A::CopyFrom, &["c"]),
            (C::Colours, A::HexEntry, &["#"]),
            (C::Colours, A::FocusNext, &["<Tab>"]),
            (C::Colours, A::FocusPrev, &["<S-Tab>"]),
            (C::Colours, A::Back, &["<Esc>"]),
            (C::Colours, A::ToggleYazi, &["Y"]),
            (C::Colours, A::ToggleRofi, &["R"]),
            (C::Colours, A::ToggleView, &["V"]),
            (C::ColourList, A::Up, &["k", "<Up>"]),
            (C::ColourList, A::Down, &["j", "<Down>"]),
            (C::ColourList, A::Right, &["l", "<Right>"]),
            (C::ColourList, A::Confirm, &["<CR>"]),
            (C::ColourList, A::Top, &["g", "<Home>"]),
            (C::ColourList, A::Bottom, &["G", "<End>"]),
            (C::ColourList, A::PageUp, &["<PageUp>"]),
            (C::ColourList, A::PageDown, &["<PageDown>"]),
            (C::Picker, A::Up, &["k", "<Up>"]),
            (C::Picker, A::Down, &["j", "<Down>"]),
            (C::Picker, A::Left, &["h", "<Left>"]),
            (C::Picker, A::Right, &["l", "<Right>"]),
            (C::Picker, A::UpBig, &["K", "<S-Up>"]),
            (C::Picker, A::DownBig, &["J", "<S-Down>"]),
            (C::Picker, A::LeftBig, &["H", "<S-Left>"]),
            (C::Picker, A::RightBig, &["L", "<S-Right>"]),
            (C::Fonts, A::FocusNext, &["<Tab>"]),
            (C::Fonts, A::FocusPrev, &["<S-Tab>"]),
            (C::FontRoles, A::Up, &["k", "<Up>"]),
            (C::FontRoles, A::Down, &["j", "<Down>"]),
            (C::FontRoles, A::Left, &["h", "<Left>"]),
            (C::FontRoles, A::Right, &["l", "<Right>"]),
            (C::FontRoles, A::LeftBig, &["H"]),
            (C::FontRoles, A::RightBig, &["L"]),
            (C::FontRoles, A::SizeUp, &["+", "="]),
            (C::FontRoles, A::SizeDown, &["-"]),
            (C::FontRoles, A::PickFont, &["<CR>", "f"]),
            (C::FontList, A::Up, &["k", "<Up>"]),
            (C::FontList, A::Down, &["j", "<Down>"]),
            (C::FontList, A::Left, &["h", "<Left>"]),
            (C::FontList, A::Right, &["l", "<Right>"]),
            (C::FontList, A::Top, &["g", "<Home>"]),
            (C::FontList, A::Bottom, &["G", "<End>"]),
            (C::FontList, A::PageUp, &["<PageUp>"]),
            (C::FontList, A::PageDown, &["<PageDown>"]),
            (C::FontList, A::Confirm, &["<CR>"]),
            (C::FontList, A::Back, &["<Esc>"]),
            (C::FontList, A::Search, &["/"]),
        ];
        const CHIP_KEYS: [&str; 8] = ["m", "p", "n", "M", "P", "x", "b", "c"];
        for (i, (c, _)) in crate::fonts::CHIPS.iter().enumerate() {
            rows.push((
                C::FontList,
                A::Filter(*c),
                std::slice::from_ref(&CHIP_KEYS[i]),
            ));
        }
        Keymap::from_rows(
            "Vim",
            Key::new(KeyCode::Char(' '), KeyModifiers::NONE),
            &rows,
        )
        .expect("the Vim preset parses")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn space() -> Key {
        Key::new(KeyCode::Char(' '), KeyModifiers::NONE)
    }

    #[test]
    fn parses_and_displays_vim_notation() {
        let l = space();
        let k = |s| Key::parse_seq(s, l).unwrap();
        assert_eq!(k("gg").len(), 2);
        assert_eq!(k("<leader>cp")[0], l);
        assert_eq!(
            k("<C-r>")[0],
            Key::new(KeyCode::Char('r'), KeyModifiers::CONTROL)
        );
        assert_eq!(
            k("<S-Tab>")[0],
            Key::new(KeyCode::BackTab, KeyModifiers::NONE)
        );
        assert_eq!(k("<F12>")[0], Key::new(KeyCode::F(12), KeyModifiers::NONE));
        assert_eq!(k("<C-x><C-s>").len(), 2);
        assert_eq!(k("<C-S-z>"), k("<C-Z>"));
        for s in [
            "j", "<C-r>", "<S-Tab>", "<F5>", "<CR>", "<Esc>", "<S-Up>", "<M-x>", "<C-Z>",
            "<Space>", "#",
        ] {
            assert_eq!(
                display_seq(&k(s), Key::new(KeyCode::Char('\\'), KeyModifiers::NONE)),
                s,
                "round trip"
            );
        }
        assert_eq!(display_seq(&k("<leader>sp"), l), "<leader>sp");
        assert!(Key::parse_seq("<nope>", l).is_err());
    }

    #[test]
    fn key_events_are_normalised() {
        let e = |c, m| Key::from_event(KeyEvent::new(c, m));
        assert_eq!(
            e(KeyCode::Char('J'), KeyModifiers::SHIFT),
            Key::new(KeyCode::Char('J'), KeyModifiers::NONE)
        );
        assert_eq!(
            e(KeyCode::BackTab, KeyModifiers::SHIFT),
            Key::new(KeyCode::BackTab, KeyModifiers::NONE)
        );
        assert_eq!(
            e(KeyCode::Up, KeyModifiers::SHIFT),
            Key::new(KeyCode::Up, KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn lookup_waits_for_sequences_and_prefers_specific_contexts() {
        let km = Keymap::vim();
        let l = space();
        let seq = |s: &str| Key::parse_seq(s, l).unwrap();
        let list = [Context::FontList, Context::Fonts, Context::Global];
        let roles = [Context::FontRoles, Context::Fonts, Context::Global];
        // n: a filter in the font list, "new" elsewhere
        assert_eq!(
            km.lookup(&list, &seq("n")),
            Lookup::Run(Action::Filter('n'))
        );
        assert_eq!(km.lookup(&roles, &seq("n")), Lookup::Run(Action::New));
        // the leader waits, then runs the sequence
        assert_eq!(km.lookup(&roles, &seq(" ")), Lookup::Pending);
        assert_eq!(km.lookup(&roles, &seq(" s")), Lookup::Pending);
        assert_eq!(
            km.lookup(&roles, &seq(" sf")),
            Lookup::Run(Action::SaveFonts)
        );
        assert_eq!(km.lookup(&roles, &seq(" z")), Lookup::None);
        let next: Vec<String> = km
            .continuations(&roles, &seq(" "))
            .into_iter()
            .map(|(k, _)| k)
            .collect();
        assert_eq!(next, ["f", "s", "a"]);
        assert_eq!(
            km.key_for(&roles, Action::SaveFonts).as_deref(),
            Some("<leader>sf")
        );
    }

    /// No key in a pane both runs something and starts a longer sequence
    /// (there's no timeout to tell them apart).
    #[test]
    fn vim_preset_has_no_ambiguous_prefixes() {
        let km = Keymap::vim();
        let stacks: [&[Context]; 4] = [
            &[Context::ColourList, Context::Colours, Context::Global],
            &[Context::Picker, Context::Colours, Context::Global],
            &[Context::FontRoles, Context::Fonts, Context::Global],
            &[Context::FontList, Context::Fonts, Context::Global],
        ];
        for stack in stacks {
            let active: Vec<&Binding> = km
                .bindings
                .iter()
                .filter(|b| stack.contains(&b.context))
                .collect();
            for a in &active {
                for b in &active {
                    assert!(
                        !(b.keys.len() > a.keys.len() && b.keys.starts_with(&a.keys)),
                        "{:?} {:?} is a prefix of {:?} {:?}",
                        a.context,
                        a.action,
                        b.context,
                        b.action
                    );
                }
            }
        }
    }
}
