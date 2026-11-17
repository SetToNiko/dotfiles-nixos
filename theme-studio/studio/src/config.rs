//! The studio's own settings: `$XDG_CONFIG_HOME/theme-studio/` (default
//! `~/.config/theme-studio/`) with keymaps.json and settings.json. Missing
//! files are created and missing entries filled in from the defaults, so
//! things added in newer versions show up while your changes are kept.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::fonts::{Filters, NerdKind, Spacing};
use crate::keymap::Keymap;

/// `$XDG_CONFIG_HOME/theme-studio`, or `~/.config/theme-studio`.
pub fn dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".config")
        });
    base.join("theme-studio")
}

/// How the font list starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FontListSettings {
    pub hide_bitmap: bool,
    pub hide_cjk_emoji_math: bool,
    /// "any", "mono" or "propo"
    pub spacing: String,
    /// "any", "nerd-font", "nerd-font-mono", "nerd-font-propo" or "no-nerd-font"
    pub nerd_font: String,
}

impl Default for FontListSettings {
    fn default() -> Self {
        FontListSettings {
            hide_bitmap: true,
            hide_cjk_emoji_math: true,
            spacing: "any".into(),
            nerd_font: "any".into(),
        }
    }
}

impl FontListSettings {
    pub fn filters(&self) -> Filters {
        Filters {
            spacing: match self.spacing.as_str() {
                "mono" => Spacing::Mono,
                "propo" => Spacing::Propo,
                _ => Spacing::Any,
            },
            nerd: match self.nerd_font.as_str() {
                "nerd-font" => NerdKind::Nerd,
                "nerd-font-mono" => NerdKind::NerdMono,
                "nerd-font-propo" => NerdKind::NerdPropo,
                "no-nerd-font" => NerdKind::NotNerd,
                _ => NerdKind::Any,
            },
            show_bitmap: !self.hide_bitmap,
            show_special: !self.hide_cjk_emoji_math,
            need: None,
        }
    }
}

/// How the colour column starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct ColourSettings {
    /// Groups open at start, by name ("Base", "Backgrounds", …).
    pub open_groups: Vec<String>,
    /// "hidden" or "shown"
    pub picker_at_start: String,
    /// "hide-on-esc" or "stay-open"
    pub picker_after_editing: String,
    /// How the selected colour's derivation relatives are marked.
    pub derived_highlight: LookSettings,
}

impl Default for ColourSettings {
    fn default() -> Self {
        ColourSettings {
            open_groups: vec!["Base".into()],
            picker_at_start: "hidden".into(),
            picker_after_editing: "hide-on-esc".into(),
            derived_highlight: LookSettings::derived_default(),
        }
    }
}

impl ColourSettings {
    pub fn picker_shown(&self) -> bool {
        self.picker_at_start == "shown"
    }

    pub fn picker_hides_on_esc(&self) -> bool {
        self.picker_after_editing != "stay-open"
    }
}

/// How the fonts column starts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FontSettings {
    /// "shown" or "hidden"
    pub sample_at_start: String,
    /// Opening the font list unfolds the groups of the role's current font.
    pub list_opens_to_current_font: bool,
    /// The list of downloaded fonts that packaging/nix/fonts.nix reads ("": next to
    /// settings.json; "~/" works).
    pub installed_list: String,
    // (the per-role filters, mono-filter / ui-filter / bar-filter, went: each
    // role now has a fixed requirement, fonts_pick.rs)
}

impl Default for FontSettings {
    fn default() -> Self {
        FontSettings {
            sample_at_start: "shown".into(),
            list_opens_to_current_font: true,
            installed_list: String::new(),
        }
    }
}

/// The Fonts (download) page: fetching previews ahead of time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct FontsPageSettings {
    /// Fetch previews in the background (a few at a time).
    pub prefetch: bool,
    /// Which: "monospace", "sans-serif", "serif", "display", "handwriting",
    /// "nerd font".
    pub prefetch_categories: Vec<String>,
}

impl Default for FontsPageSettings {
    fn default() -> Self {
        FontsPageSettings {
            prefetch: false,
            prefetch_categories: vec!["monospace".into()],
        }
    }
}

/// The categories previews can be prefetched for: (catalogue category, label).
pub const PREFETCH_CATEGORIES: [(&str, &str); 6] = [
    ("monospace", "Google monospace"),
    ("sans-serif", "Google sans-serif"),
    ("serif", "Google serif"),
    ("display", "Google display"),
    ("handwriting", "Google handwriting"),
    ("nerd font", "Nerd Fonts"),
];

impl FontsPageSettings {
    pub fn prefetches(&self, category: &str) -> bool {
        self.prefetch_categories.iter().any(|c| c == category)
    }
}

/// Pane sizes, as shares of the space they divide (minimum sizes still
/// apply). Resizing changes them while the studio runs; these are where it
/// starts and what `=` in resize mode goes back to.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Sizes {
    /// Three columns: the fonts column's share of the width.
    pub fonts_width: f64,
    /// Three columns: the colour column's share of the width.
    pub colours_width: f64,
    /// The font sample's share of the fonts column.
    pub sample_height: f64,
    /// The picker's share of the colour column; 0 = automatic (it grows when
    /// the whole colour list fits).
    pub picker_height: f64,
    /// nvim's share of the preview (the shell gets the rest).
    pub preview_split: f64,
    /// Narrow layout: the preview row's share of the height.
    pub narrow_preview_height: f64,
    /// Narrow layout: the fonts column's share of the top row.
    pub narrow_fonts_width: f64,
}

impl Default for Sizes {
    fn default() -> Self {
        Sizes {
            fonts_width: 0.28,
            colours_width: 0.32,
            sample_height: 0.3,
            picker_height: 0.0,
            preview_split: 0.5,
            narrow_preview_height: 0.42,
            narrow_fonts_width: 0.45,
        }
    }
}

/// Where things go on screen.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LayoutSettings {
    /// The page tabs: "bottom" (in the status line) or "top" (their own row).
    pub tabs: String,
    /// Tabs inside a page (the Settings page's): "top" or "bottom".
    pub inner_tabs: String,
    /// The key hints in the status line: "shown" or "hidden".
    pub key_hints: String,
    /// The previews at start: the Main page's (nvim and the shell) and the
    /// Fonts page's, "shown" or "hidden".
    pub main_preview: String,
    pub fonts_preview: String,
    pub sizes: Sizes,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        LayoutSettings {
            tabs: "bottom".into(),
            inner_tabs: "top".into(),
            key_hints: "shown".into(),
            main_preview: "shown".into(),
            fonts_preview: "shown".into(),
            sizes: Sizes::default(),
        }
    }
}

impl LayoutSettings {
    pub fn tabs_on_top(&self) -> bool {
        self.tabs == "top"
    }

    pub fn inner_tabs_on_top(&self) -> bool {
        self.inner_tabs != "bottom"
    }
}

/// settings.json: how things look when the studio starts.
/// (`version` is the file format, for migrating later versions.)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub version: u32,
    pub layout: LayoutSettings,
    pub colours: ColourSettings,
    pub fonts: FontSettings,
    pub font_list: FontListSettings,
    pub hover: HoverSettings,
    pub messages: MessageSettings,
    pub fonts_page: FontsPageSettings,
}

/// How messages show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct MessageSettings {
    /// Seconds a message stays in the status line; 0 = until the next key.
    /// (Errors always stay until the next key.)
    pub seconds: u32,
    /// Save confirmations: "bar" (the status line) or "popup" (a box that
    /// stays until a key or a click closes it).
    pub saved: String,
}

impl Default for MessageSettings {
    fn default() -> Self {
        MessageSettings {
            seconds: 3,
            saved: "bar".into(),
        }
    }
}

impl MessageSettings {
    /// The choices for `seconds`.
    pub const SECONDS: [u32; 6] = [1, 2, 3, 5, 10, 0];

    pub fn saved_in_popup(&self) -> bool {
        self.saved == "popup"
    }
}

/// A highlight look: any mix of a background, an underline and a text
/// colour. Colours are keys of the active system palette ("yellow", "bg3").
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct LookSettings {
    /// A background colour.
    pub highlight: bool,
    pub highlight_colour: String,
    /// "off", "single" or "double"
    pub underline: String,
    pub underline_colour: String,
    /// A text colour and weight.
    pub text: bool,
    pub text_colour: String,
    /// "normal", "bold" or "dim"
    pub text_weight: String,
}

impl Default for LookSettings {
    fn default() -> Self {
        LookSettings {
            highlight: false,
            highlight_colour: "bg3".into(),
            underline: "single".into(),
            underline_colour: "yellow".into(),
            text: true,
            text_colour: "yellow".into(),
            text_weight: "normal".into(),
        }
    }
}

impl LookSettings {
    /// The derived-colour highlight's default: a background only.
    pub fn derived_default() -> Self {
        LookSettings {
            highlight: true,
            highlight_colour: "bg_visual_blue".into(),
            underline: "off".into(),
            text: false,
            ..Default::default()
        }
    }

    /// Underline styles that are no longer offered become single.
    fn normalise(&mut self) {
        if !["off", "single", "double"].contains(&self.underline.as_str()) {
            self.underline = "single".into();
        }
    }
}

/// How clickable things look under the mouse.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct HoverSettings {
    #[serde(flatten)]
    pub look: LookSettings,
    /// The border of a pane under the mouse.
    pub pane_border: String,
    /// The frame of a preview window (nvim, the shell) under the mouse.
    pub preview_border: String,
}

impl Default for HoverSettings {
    fn default() -> Self {
        HoverSettings {
            look: LookSettings::default(),
            pane_border: "aqua".into(),
            preview_border: "purple".into(),
        }
    }
}

impl Settings {
    /// Values from older versions that are no longer offered.
    pub fn normalise(&mut self) {
        self.hover.look.normalise();
        self.colours.derived_highlight.normalise();
        if !["top", "bottom"].contains(&self.layout.inner_tabs.as_str()) {
            self.layout.inner_tabs = "top".into();
        }
        if !["bar", "popup"].contains(&self.messages.saved.as_str()) {
            self.messages.saved = "bar".into();
        }
    }
}

/// settings.json's format version.
pub const SETTINGS_VERSION: u32 = 1;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            version: SETTINGS_VERSION,
            layout: LayoutSettings::default(),
            colours: ColourSettings::default(),
            fonts: FontSettings::default(),
            font_list: FontListSettings::default(),
            hover: HoverSettings::default(),
            messages: MessageSettings::default(),
            fonts_page: FontsPageSettings::default(),
        }
    }
}

/// What loading the config produced, and anything worth telling the user.
pub struct Loaded {
    pub keymap: Keymap,
    pub presets: Vec<Keymap>,
    pub settings: Settings,
    pub warnings: Vec<String>,
    /// The folder the files are in (for saving).
    pub dir: PathBuf,
}

fn write_json(path: &Path, v: &Value) -> Result<()> {
    crate::store::write_json_file(path, v)
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?,
        )),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// keymaps.json as the defaults would write it.
fn default_keymaps() -> Value {
    let mut presets = Map::new();
    for km in Keymap::builtin() {
        presets.insert(km.name.clone(), km.to_json());
    }
    let mut m = Map::new();
    m.insert("version".into(), Value::from(KEYMAPS_VERSION));
    m.insert("active".into(), Value::String("Vim".into()));
    m.insert("presets".into(), Value::Object(presets));
    Value::Object(m)
}

/// keymaps.json.s format version. 2: panes are `<leader>1`…`<leader>7`.
/// 3: `use-for-mono` / `-ui` / `-bar` are `use-for-monospace` /
/// `-proportional` / `-icons`, after the font roles.
pub const KEYMAPS_VERSION: u64 = 3;

/// Updates an older keymaps.json. Version 1 (no "version") had panes on the
/// plain digits; bindings still exactly like that move to the leader, so
/// your own changes are left alone.
fn migrate(mut v: Value) -> Value {
    let version = v.get("version").and_then(Value::as_u64).unwrap_or(1);
    if version < 2
        && let Some(presets) = v.get_mut("presets").and_then(Value::as_object_mut)
    {
        for preset in presets.values_mut() {
            let Some(global) = preset
                .pointer_mut("/bindings/global")
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            for n in 1..=9 {
                let id = format!("panel-{n}");
                if global.get(&id) == Some(&serde_json::json!([n.to_string()])) {
                    global.insert(id, serde_json::json!([format!("<leader>{n}")]));
                }
            }
        }
    }
    if version < 3
        && let Some(presets) = v.get_mut("presets").and_then(Value::as_object_mut)
    {
        for preset in presets.values_mut() {
            let Some(contexts) = preset.get_mut("bindings").and_then(Value::as_object_mut) else {
                continue;
            };
            for ctx in contexts.values_mut().filter_map(Value::as_object_mut) {
                let old = std::mem::take(ctx);
                *ctx = old
                    .into_iter()
                    .map(|(k, keys)| {
                        let role = k
                            .strip_prefix("use-for-")
                            .and_then(|r| crate::fonts::OLD_ROLES.iter().position(|o| *o == r));
                        match role {
                            Some(r) => (format!("use-for-{}", crate::fonts::ROLES[r]), keys),
                            None => (k, keys),
                        }
                    })
                    .collect();
            }
        }
    }
    v
}

/// Reads keymaps.json: every preset (shipped ones topped up with actions the
/// file doesn't mention), and the active one.
fn load_keymaps(v: &Value, warnings: &mut Vec<String>) -> (Keymap, Vec<Keymap>, Value) {
    let v = &migrate(v.clone());
    let builtin = Keymap::builtin();
    let mut presets: Vec<Keymap> = Vec::new();
    if let Some(obj) = v.get("presets").and_then(Value::as_object) {
        for (name, p) in obj {
            let base = builtin.iter().find(|b| b.name == *name);
            let (km, w) = Keymap::from_json(name, p, base);
            warnings.extend(w);
            presets.push(km);
        }
    }
    // shipped presets missing from the file come back
    for b in builtin {
        if !presets.iter().any(|p| p.name == b.name) {
            presets.push(b);
        }
    }
    let active_name = v
        .get("active")
        .and_then(Value::as_str)
        .unwrap_or("Vim")
        .to_string();
    let active = match presets.iter().find(|p| p.name == active_name) {
        Some(p) => p.clone(),
        None => {
            warnings.push(format!(
                "keymaps.json: no preset \"{active_name}\"; using Vim"
            ));
            presets
                .iter()
                .find(|p| p.name == "Vim")
                .cloned()
                .unwrap_or_else(Keymap::vim)
        }
    };
    // what the file should contain now (adds anything missing)
    let mut out = Map::new();
    out.insert("version".into(), Value::from(KEYMAPS_VERSION));
    out.insert("active".into(), Value::String(active.name.clone()));
    let mut obj = Map::new();
    for p in &presets {
        obj.insert(p.name.clone(), p.to_json());
    }
    out.insert("presets".into(), Value::Object(obj));
    (active, presets, Value::Object(out))
}

/// Writes settings.json.
pub fn save_settings(dir: &Path, s: &Settings) -> Result<()> {
    write_json(&dir.join("settings.json"), &serde_json::to_value(s)?)
}

/// Loads both files from `dir`, creating or topping them up when `write` is
/// set. A file that doesn't parse is left alone and the defaults are used.
pub fn load(dir: &Path, write: bool) -> Loaded {
    let mut warnings = Vec::new();
    let keymaps_path = dir.join("keymaps.json");
    let settings_path = dir.join("settings.json");

    let (keymap, presets) = match read_json(&keymaps_path) {
        Ok(found) => {
            let v = found.clone().unwrap_or_else(default_keymaps);
            let (active, presets, full) = load_keymaps(&v, &mut warnings);
            if write
                && found.as_ref() != Some(&full)
                && let Err(e) = write_json(&keymaps_path, &full)
            {
                warnings.push(format!("{e:#}"));
            }
            (active, presets)
        }
        Err(e) => {
            warnings.push(format!("{e:#} (using the built-in Vim keys)"));
            (Keymap::vim(), Keymap::builtin())
        }
    };

    let settings = match read_json(&settings_path) {
        Ok(found) => {
            let s: Settings = match found.clone().map(serde_json::from_value::<Settings>) {
                Some(Ok(mut s)) => {
                    s.normalise();
                    s
                }
                Some(Err(e)) => {
                    warnings.push(format!("settings.json: {e} (using defaults)"));
                    Settings::default()
                }
                None => Settings::default(),
            };
            let full = serde_json::to_value(&s).expect("settings serialise");
            // only top up a file that parsed (or create a missing one)
            let parsed = found
                .as_ref()
                .is_none_or(|f| serde_json::from_value::<Settings>(f.clone()).is_ok());
            if write
                && parsed
                && found.as_ref() != Some(&full)
                && let Err(e) = write_json(&settings_path, &full)
            {
                warnings.push(format!("{e:#}"));
            }
            s
        }
        Err(e) => {
            warnings.push(format!("{e:#} (using defaults)"));
            Settings::default()
        }
    };

    Loaded {
        dir: dir.to_path_buf(),
        keymap,
        presets,
        settings,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Action, Context, Key, Lookup};
    use crate::store::to_jq_string;

    fn temp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("theme-studio-config-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn creates_files_with_defaults_in_jq_format() {
        let d = temp("create");
        let l = load(&d, true);
        assert!(l.warnings.is_empty(), "{:?}", l.warnings);
        assert_eq!(l.keymap.name, "Vim");
        for f in ["keymaps.json", "settings.json"] {
            let text = std::fs::read_to_string(d.join(f)).unwrap();
            let v: Value = serde_json::from_str(&text).unwrap();
            assert_eq!(text, to_jq_string(&v), "{f} is jq-formatted");
        }
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("keymaps.json")).unwrap())
                .unwrap();
        assert_eq!(v["presets"]["Vim"]["leader"], "<Space>");
        assert_eq!(
            v["presets"]["Vim"]["bindings"]["global"]["save-palette"][0],
            "<leader>sp"
        );
        // loading again changes nothing
        let before = std::fs::read_to_string(d.join("keymaps.json")).unwrap();
        load(&d, true);
        assert_eq!(
            std::fs::read_to_string(d.join("keymaps.json")).unwrap(),
            before
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn edits_are_kept_and_missing_actions_filled_in() {
        let d = temp("edit");
        std::fs::create_dir_all(&d).unwrap();
        // rebinds quit, unbinds undo, sets the leader; mentions nothing else
        std::fs::write(
            d.join("keymaps.json"),
            r#"{"active": "Vim", "presets": {"Vim": {"leader": "<lt>", "bindings": {
                "global": {"quit": ["Q"], "nope": ["x"]},
                "colours": {"undo": []},
                "colour-list": {"down": ["<bogus>"]}
            }}}}"#,
        )
        .unwrap();
        let l = load(&d, true);
        assert_eq!(l.warnings.len(), 2, "{:?}", l.warnings); // unknown action, bad key
        let km = &l.keymap;
        let ctx = [Context::ColourList, Context::Colours, Context::Global];
        let seq = |s: &str| Key::parse_seq(s, km.leader).unwrap();
        assert_eq!(km.lookup(&ctx, &seq("Q")), Lookup::Run(Action::Quit));
        assert_eq!(km.lookup(&ctx, &seq("q")), Lookup::None);
        assert_eq!(
            km.lookup(&ctx, &seq("u")),
            Lookup::None,
            "unbound stays unbound"
        );
        // not mentioned: shipped keys, with the new leader
        assert_eq!(
            km.lookup(&ctx, &seq("<lt>sp")),
            Lookup::Run(Action::SavePalette)
        );
        assert_eq!(km.lookup(&ctx, &seq("y")), Lookup::Run(Action::CopyHex));
        // the file now lists everything, with the edits kept
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("keymaps.json")).unwrap())
                .unwrap();
        assert_eq!(
            v["presets"]["Vim"]["bindings"]["global"]["quit"],
            serde_json::json!(["Q"])
        );
        assert_eq!(
            v["presets"]["Vim"]["bindings"]["colours"]["undo"],
            serde_json::json!([])
        );
        assert_eq!(
            v["presets"]["Vim"]["bindings"]["colours"]["copy-hex"],
            serde_json::json!(["y"])
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn broken_files_are_left_alone() {
        let d = temp("broken");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("keymaps.json"), "{ not json").unwrap();
        std::fs::write(
            d.join("settings.json"),
            r#"{"font-list": {"hide-bitmap": "yes"}}"#,
        )
        .unwrap();
        let l = load(&d, true);
        assert_eq!(l.keymap.name, "Vim");
        assert_eq!(l.warnings.len(), 2, "{:?}", l.warnings);
        assert_eq!(
            std::fs::read_to_string(d.join("keymaps.json")).unwrap(),
            "{ not json"
        );
        assert!(
            std::fs::read_to_string(d.join("settings.json"))
                .unwrap()
                .contains("\"yes\"")
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn settings_fill_in_and_map_to_filters() {
        let d = temp("settings");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("settings.json"),
            r#"{"font-list": {"hide-bitmap": false, "spacing": "mono"}}"#,
        )
        .unwrap();
        let l = load(&d, true);
        let f = l.settings.font_list.filters();
        assert!(f.show_bitmap && !f.show_special);
        assert_eq!(f.spacing, Spacing::Mono);
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(
            v["font-list"]["hide-cjk-emoji-math"], true,
            "missing entries filled in"
        );
        assert_eq!(v["font-list"]["hide-bitmap"], false, "edits kept");
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn xdg_config_home_is_respected() {
        // only checks the path rule; doesn't change the environment
        let d = dir();
        assert!(d.ends_with("theme-studio"));
    }
}

#[cfg(test)]
mod migrate_tests {
    use super::*;

    #[test]
    fn version_1_panes_move_to_the_leader_but_edits_stay() {
        let old = serde_json::json!({"active": "Vim", "presets": {"Vim": {"leader": "<Space>", "bindings": {"global": {
            "panel-1": ["1"], "panel-2": ["9"], "panel-4": ["4", "x"]
        }}}}});
        let mut w = Vec::new();
        let (km, _, full) = load_keymaps(&old, &mut w);
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(full["version"], KEYMAPS_VERSION);
        let g = &full["presets"]["Vim"]["bindings"]["global"];
        assert_eq!(
            g["panel-1"],
            serde_json::json!(["<leader>1"]),
            "old default moves"
        );
        assert_eq!(
            g["panel-2"],
            serde_json::json!(["9"]),
            "your own binding stays"
        );
        assert_eq!(
            g["panel-4"],
            serde_json::json!(["4", "x"]),
            "changed lists stay"
        );
        assert_eq!(
            g["panel-7"],
            serde_json::json!(["<leader>7"]),
            "missing ones come from the preset"
        );
        // version 2 files aren't touched again
        let (_, _, again) = load_keymaps(&full, &mut w);
        assert_eq!(again, full);
        let _ = km;
    }

    #[test]
    fn version_2_use_for_keys_follow_the_role_names() {
        let old = serde_json::json!({"version": 2, "active": "Vim", "presets": {"Vim": {"leader": "<Space>", "bindings": {"downloads": {
            "use-for-mono": ["7"], "use-for-ui": ["2"], "use-for-bar": ["3"]
        }}}}});
        let mut w = Vec::new();
        let (km, _, full) = load_keymaps(&old, &mut w);
        assert!(w.is_empty(), "{w:?}");
        let d = &full["presets"]["Vim"]["bindings"]["downloads"];
        assert_eq!(
            d["use-for-monospace"],
            serde_json::json!(["7"]),
            "rebinds kept"
        );
        assert_eq!(d["use-for-icons"], serde_json::json!(["3"]));
        assert!(d.get("use-for-mono").is_none());
        assert_eq!(
            km.key_for(
                &[crate::keymap::Context::Downloads],
                crate::keymap::Action::UseFor(0)
            )
            .as_deref(),
            Some("7")
        );
    }
}
