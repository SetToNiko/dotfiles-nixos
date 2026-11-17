//! Themes: a name for a palette plus a font preset, saved as one in
//! themes.json (`{"version": 1, "themes": {name: {palette, fonts}}}`). The
//! parts stay in palettes.json and fonts.json, so they can be shared. The
//! active theme is recorded in ~/.config/colors/theme.

use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Map, Value, json};

use crate::app::{App, For, Overlay};
use crate::color::Rgb;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub palette: String,
    pub fonts: String,
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
                    .filter_map(|(k, t)| {
                        Some((
                            k.clone(),
                            Theme {
                                palette: t.get("palette")?.as_str()?.to_string(),
                                fonts: t.get("fonts")?.as_str()?.to_string(),
                            },
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
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
            m.insert(n, json!({"palette": v.palette, "fonts": v.fonts}));
        }
        if !placed {
            m.insert(
                name.to_string(),
                json!({"palette": t.palette, "fonts": t.fonts}),
            );
        }
        crate::store::write_json_file(&self.path(), &json!({"version": 1, "themes": m}))
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
        if let Some((n, t)) = all.iter().find(|(_, t)| *t == now) {
            self.theme = n.clone();
            self.theme_new = false;
            self.theme_saved = Some(t.clone());
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
            self.info(format!("Saved the theme \"{name}\""));
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
        if let Err(e) = self.fonts.store.apply_quietly(&t.fonts) {
            return self.error(format!("Applying the fonts: {e:#}"));
        }
        self.fonts.active = Some(t.fonts.clone());
        if let Err(e) = self.store.apply(&t.palette) {
            return self.error(format!("Applying the palette: {e:#}"));
        }
        self.active = Some(t.palette.clone());
        self.system_palette = Some(self.palette.clone());
        let _ = ThemeStore::set_active(&self.theme);
        self.info(format!("Saved and applied the theme \"{}\"", self.theme));
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
        assert_eq!(v["version"], 1);
        std::fs::remove_dir_all(d).unwrap();
    }
}
