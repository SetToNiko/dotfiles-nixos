//! Themes: a name for a palette, a font preset and the fonts' sizes, saved
//! as one in themes.json (`{"version": 2, "themes": {name: {palette, fonts,
//! sizes: {monospace, proportional, icons}}}}`). The palette and preset stay
//! in palettes.json and fonts.json, so they can be shared; the sizes are the
//! theme's own, so swapping the preset keeps them. The active theme is
//! recorded in ~/.config/colors/theme.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::app::{App, For, Overlay};
use crate::color::Rgb;
use crate::fonts::ROLES;

/// themes.json's version. 2: each theme has its fonts' sizes.
pub const THEMES_VERSION: u64 = 2;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub palette: String,
    pub fonts: String,
    /// Base sizes for Monospace, Proportional and Icons (before the host
    /// offsets). None in a version 1 file, until it's migrated.
    pub sizes: Option<[u32; 3]>,
}

impl Theme {
    fn from_json(t: &Value) -> Option<Theme> {
        let sizes = t.get("sizes").and_then(|s| {
            let get = |r: usize| s.get(ROLES[r])?.as_u64().map(|n| n as u32);
            Some([get(0)?, get(1)?, get(2)?])
        });
        Some(Theme {
            palette: t.get("palette")?.as_str()?.to_string(),
            fonts: t.get("fonts")?.as_str()?.to_string(),
            sizes,
        })
    }

    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("palette".into(), json!(self.palette));
        m.insert("fonts".into(), json!(self.fonts));
        if let Some(s) = self.sizes {
            let mut sizes = Map::new();
            for (r, n) in ROLES.iter().zip(s) {
                sizes.insert(r.to_string(), json!(n));
            }
            m.insert("sizes".into(), Value::Object(sizes));
        }
        Value::Object(m)
    }

    /// The same palette and font preset (sizes aside).
    fn same_parts(&self, other: &Theme) -> bool {
        self.palette == other.palette && self.fonts == other.fonts
    }
}

pub struct ThemeStore {
    /// The folder with palettes.json (themes.json sits next to it).
    pub dir: PathBuf,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

impl ThemeStore {
    pub fn path(&self) -> PathBuf {
        self.dir.join("themes.json")
    }

    pub fn exists(&self) -> bool {
        self.path().exists()
    }

    /// Every theme, in file order.
    pub fn load_all(&self) -> Vec<(String, Theme)> {
        let v: Value = std::fs::read_to_string(self.path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        v.get("themes")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, t)| Some((k.clone(), Theme::from_json(t)?)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Version 1 themes get sizes: their font preset's (fonts.json before
    /// version 4 has them), else the defaults. Run before fonts.json's
    /// migration drops them from the presets. The version it now has, if it
    /// did.
    pub fn migrate(&self, fonts: &crate::fonts::FontStore) -> Result<Option<u64>> {
        let Ok(text) = std::fs::read_to_string(self.path()) else {
            return Ok(None);
        };
        let v: Value = serde_json::from_str(&text)?;
        if v.get("version").and_then(Value::as_u64).unwrap_or(1) >= THEMES_VERSION {
            return Ok(None);
        }
        let mut m = Map::new();
        for (n, mut t) in self.load_all() {
            if t.sizes.is_none() {
                t.sizes = Some(
                    fonts
                        .load(&t.fonts)
                        .map(|p| p.sizes())
                        .unwrap_or(crate::fonts::DEFAULT_SIZES),
                );
            }
            m.insert(n, t.to_json());
        }
        crate::store::write_json_file(
            &self.path(),
            &json!({"version": THEMES_VERSION, "themes": m}),
        )?;
        Ok(Some(THEMES_VERSION))
    }

    pub fn get(&self, name: &str) -> Option<Theme> {
        self.load_all()
            .into_iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }

    /// Adds or replaces a theme (in place, keeping the file's order).
    pub fn save(&self, name: &str, t: &Theme) -> Result<()> {
        let mut m = Map::new();
        let mut placed = false;
        for (n, old) in self.load_all() {
            let v = if n == name {
                placed = true;
                t.clone()
            } else {
                old
            };
            m.insert(n, v.to_json());
        }
        if !placed {
            m.insert(name.to_string(), t.to_json());
        }
        crate::store::write_json_file(
            &self.path(),
            &json!({"version": THEMES_VERSION, "themes": m}),
        )
    }

    /// The theme applied last (themes-switch.sh or the studio).
    pub fn active_name() -> Option<String> {
        let s = std::fs::read_to_string(home().join(".config/colors/theme")).ok()?;
        Some(s.trim().to_string()).filter(|s| !s.is_empty())
    }

    pub fn set_active(name: &str) -> Result<()> {
        // tests never touch your ~/.config
        if cfg!(test) {
            return Ok(());
        }
        let dir = home().join(".config/colors");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("theme"), format!("{name}\n"))?;
        Ok(())
    }
}

impl App {
    /// The theme's parts as they are now.
    fn theme_now(&self) -> Theme {
        Theme {
            palette: self.name.clone(),
            fonts: self.fonts.name.clone(),
            sizes: Some(self.fonts.preset.sizes()),
        }
    }

    /// Unsaved: new, a part edited, or another palette / preset swapped in.
    pub fn theme_dirty(&self) -> bool {
        self.theme_new
            || self.dirty()
            || self.fonts.dirty()
            || self.theme_saved.as_ref() != Some(&self.theme_now())
    }

    /// At start: the theme to show. `named` (the command line) or the active
    /// theme is opened if it exists; else the one whose parts are open; the
    /// first start makes a theme from the active palette and font preset.
    pub fn init_theme(&mut self, named: Option<&str>) {
        let all = self.themes.load_all();
        let wanted = match named {
            Some(n) => Some(n.to_string()),
            None => ThemeStore::active_name(),
        };
        if let Some(w) = wanted
            && let Some((n, t)) = all.iter().find(|(n, _)| *n == w)
        {
            let (n, t) = (n.clone(), t.clone());
            self.load_theme_parts(&t);
            self.theme = n;
            self.theme_new = false;
            self.theme_saved = Some(t);
            return;
        }
        let now = self.theme_now();
        if let Some((n, t)) = all.iter().find(|(_, t)| t.same_parts(&now)) {
            let (n, t) = (n.clone(), t.clone());
            if let Some(s) = t.sizes {
                self.fonts.preset.set_sizes(s);
            }
            self.theme = n;
            self.theme_new = false;
            self.theme_saved = Some(t);
            return;
        }
        self.theme = self.name.clone();
        self.theme_new = true;
        self.theme_saved = None;
        // the first start: the active palette and fonts become a theme
        if !self.themes.exists() && !self.is_new && !self.fonts.is_new {
            match self.themes.save(&self.name, &now) {
                Ok(()) => {
                    self.theme_new = false;
                    self.theme_saved = Some(now);
                    let _ = ThemeStore::set_active(&self.theme);
                    self.info(format!(
                        "Made the theme \"{}\" from the active palette and fonts",
                        self.theme
                    ));
                }
                Err(e) => self.error(format!("themes.json: {e:#}")),
            }
        }
    }

    /// Opens a theme's palette and font preset (messages say what's missing).
    fn load_theme_parts(&mut self, t: &Theme) {
        match self.store.load(&t.palette) {
            Ok(p) => {
                self.name = t.palette.clone();
                self.is_new = false;
                self.saved = p.clone();
                self.palette = p;
                self.reset_history();
            }
            Err(e) => self.error(format!("Palette \"{}\": {e:#}", t.palette)),
        }
        if let Err(e) = self.fonts.open(&t.fonts) {
            self.error(format!("Font preset \"{}\": {e:#}", t.fonts));
        }
        if let Some(s) = t.sizes {
            self.fonts.preset.set_sizes(s);
            self.fonts.saved.set_sizes(s);
        }
    }

    /// A role's size changed (h / l) and the keys have been still a moment:
    /// the theme keeps the new sizes and they're applied, like a picked font.
    /// An unsaved theme gets them when it's saved; they're applied now if
    /// its font preset is saved.
    pub fn apply_sizes(&mut self) {
        self.fonts.sized_at = None;
        let sizes = self.fonts.preset.sizes();
        let shown = sizes.map(|s| s.to_string()).join(" · ");
        let mut saved = String::new();
        if !self.theme_new
            && let Some(mut t) = self.themes.get(&self.theme)
        {
            t.sizes = Some(sizes);
            if let Err(e) = self.themes.save(&self.theme, &t) {
                return self.error(format!("Saving the sizes: {e:#}"));
            }
            if let Some(s) = self.theme_saved.as_mut() {
                s.sizes = Some(sizes);
            }
            saved = format!(", saved to \"{}\"", self.theme);
        }
        if self.fonts.is_new {
            return self.info(format!(
                "Sizes {shown}: applied when the font preset is saved"
            ));
        }
        let name = self.fonts.name.clone();
        match self.fonts.store.apply(&name, sizes) {
            Ok(()) => {
                self.fonts.active = Some(name);
                self.info(format!("Sizes {shown}{saved} and applied"));
            }
            Err(e) => self.error(format!("Applying the sizes: {e:#}")),
        }
    }

    pub fn open_theme(&mut self, name: &str) {
        let Some(t) = self.themes.get(name) else {
            return self.error(format!("No theme \"{name}\""));
        };
        self.load_theme_parts(&t);
        self.theme = name.to_string();
        self.theme_new = false;
        self.theme_saved = Some(t);
        self.info(format!("Opened the theme \"{name}\""));
    }

    /// A new theme: a copy of this one, named when saved.
    pub fn new_theme(&mut self) {
        self.theme = "untitled".into();
        self.theme_new = true;
        self.theme_saved = None;
        self.info("New theme (a copy of the current one); s asks for a name");
    }

    /// Save (s): asks for a name the first time.
    pub fn theme_save(&mut self, then_apply: bool) {
        if self.theme_new {
            self.overlay_for = For::Theme;
            self.overlay = Overlay::SaveAs {
                input: String::new(),
                overwrite: false,
                then_apply,
            };
            return;
        }
        let name = self.theme.clone();
        self.theme_save_as(&name, then_apply);
    }

    /// Saves the parts that changed, then the theme; with `then_apply`,
    /// applies it. New parts get the theme's name. Saving a saved theme
    /// under another name forks: edited parts are saved under the new name
    /// (other themes using them keep theirs); unedited parts stay shared.
    pub fn theme_save_as(&mut self, name: &str, then_apply: bool) {
        let fork = !self.theme_new && name != self.theme;
        let palettes = self.store.load_all().unwrap_or_default();
        let presets = self.fonts.store.load_all().unwrap_or_default();
        if self.is_new || (fork && self.dirty()) {
            if palettes.contains_key(name) && name != self.name {
                return self.error(format!(
                    "There's a palette \"{name}\" already: pick another theme name"
                ));
            }
            self.save_as(name, false);
        } else if self.dirty() {
            let n = self.name.clone();
            self.save_as(&n, false);
        }
        if self.fonts.is_new || self.fonts.dirty() {
            let n = if self.fonts.is_new || fork {
                name.to_string()
            } else {
                self.fonts.name.clone()
            };
            if n != self.fonts.name && presets.contains_key(&n) {
                return self.error(format!(
                    "There's a font preset \"{n}\" already: pick another theme name"
                ));
            }
            if let Err(e) = self.fonts.save_as(&n) {
                return self.error(format!("Saving the fonts: {e:#}"));
            }
        }
        if self.dirty() || self.fonts.dirty() {
            // a part's save failed (its message says why)
            return;
        }
        let t = self.theme_now();
        if let Err(e) = self.themes.save(name, &t) {
            return self.error(format!("Saving the theme: {e:#}"));
        }
        self.theme = name.to_string();
        self.theme_new = false;
        self.theme_saved = Some(t);
        if then_apply {
            self.theme_apply_saved();
        } else {
            self.notify_saved(format!("Saved \"{name}\""));
        }
    }

    /// Apply (a): saves if needed, then applies fonts and palette (one i3
    /// restart: the palette's) and records the theme as active.
    pub fn theme_apply(&mut self) {
        if self.theme_dirty() {
            self.theme_save(true);
        } else {
            self.theme_apply_saved();
        }
    }

    fn theme_apply_saved(&mut self) {
        let t = self.theme_now();
        let sizes = t.sizes.unwrap_or(crate::fonts::DEFAULT_SIZES);
        if let Err(e) = self.fonts.store.apply_quietly(&t.fonts, sizes) {
            return self.error(format!("Applying the fonts: {e:#}"));
        }
        self.fonts.active = Some(t.fonts.clone());
        if let Err(e) = self.store.apply(&t.palette) {
            return self.error(format!("Applying the palette: {e:#}"));
        }
        self.active = Some(t.palette.clone());
        self.system_palette = Some(self.palette.clone());
        let _ = ThemeStore::set_active(&self.theme);
        self.notify_saved(format!("Saved and applied \"{}\"", self.theme));
    }

    /// Open (o): the list of themes, with their palettes' colours.
    pub fn theme_open_overlay(&mut self) {
        let palettes = self.store.load_all().unwrap_or_default();
        let entries: Vec<(String, Vec<Rgb>)> = self
            .themes
            .load_all()
            .into_iter()
            .map(|(n, t)| {
                let sw = palettes
                    .get(&t.palette)
                    .map(|v| {
                        [
                            "bg0", "fg0", "red", "orange", "yellow", "green", "aqua", "blue",
                            "purple",
                        ]
                        .iter()
                        .filter_map(|k| v.get(*k).and_then(|x| x.as_str()).and_then(Rgb::parse))
                        .collect()
                    })
                    .unwrap_or_default();
                (n, sw)
            })
            .collect();
        if entries.is_empty() {
            return self.info("No themes saved yet (s saves this one)");
        }
        let sel = entries
            .iter()
            .position(|(n, _)| *n == self.theme)
            .unwrap_or(0);
        self.overlay_for = For::Theme;
        self.overlay = Overlay::Open {
            filter: String::new(),
            sel,
            entries,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_save_in_place_and_read_back() {
        let d = std::env::temp_dir().join(format!("theme-studio-themes-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let s = ThemeStore { dir: d.clone() };
        assert!(s.load_all().is_empty() && !s.exists());
        let t = |p: &str, f: &str| Theme {
            palette: p.into(),
            fonts: f.into(),
            sizes: Some([14, 9, 9]),
        };
        s.save("Work", &t("One", "Mono")).unwrap();
        s.save("Play", &t("Two", "Mono")).unwrap();
        s.save("Work", &t("Two", "Wide")).unwrap();
        let all = s.load_all();
        assert_eq!(
            all.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            ["Work", "Play"]
        );
        assert_eq!(s.get("Work"), Some(t("Two", "Wide")));
        let v: Value = serde_json::from_str(&std::fs::read_to_string(s.path()).unwrap()).unwrap();
        assert_eq!(v["version"], 2);
        assert_eq!(
            v["themes"]["Work"]["sizes"],
            json!({"monospace": 14, "proportional": 9, "icons": 9})
        );
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn version_1_themes_take_their_presets_sizes_once() {
        let d = std::env::temp_dir().join(format!("theme-studio-themes-v2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("fonts.json"),
            r#"{"version": 3, "presets": {"P": {"monospace": {"family": "M", "size": 13}, "proportional": {"family": "U", "size": 10}, "icons": {"family": "B", "size": 9}}}}"#,
        )
        .unwrap();
        std::fs::write(
            d.join("themes.json"),
            r#"{"version": 1, "themes": {"A": {"palette": "x", "fonts": "P"}, "B": {"palette": "y", "fonts": "gone"}}}"#,
        )
        .unwrap();
        let themes = ThemeStore { dir: d.clone() };
        let fonts = crate::fonts::FontStore {
            dir: d.clone(),
            share: d.clone(),
        };
        // themes first (from the presets' sizes), then fonts.json drops them
        assert_eq!(themes.migrate(&fonts).unwrap(), Some(2));
        assert_eq!(themes.migrate(&fonts).unwrap(), None, "only once");
        assert_eq!(fonts.migrate().unwrap(), Some(4));
        assert_eq!(themes.get("A").unwrap().sizes, Some([13, 10, 9]));
        assert_eq!(
            themes.get("B").unwrap().sizes,
            Some(crate::fonts::DEFAULT_SIZES),
            "a missing preset: the defaults"
        );
        let f: Value =
            serde_json::from_str(&std::fs::read_to_string(d.join("fonts.json")).unwrap()).unwrap();
        assert_eq!(f["version"], 4);
        assert_eq!(f["presets"]["P"]["monospace"], json!({"family": "M"}));
        std::fs::remove_dir_all(d).unwrap();
    }
}
