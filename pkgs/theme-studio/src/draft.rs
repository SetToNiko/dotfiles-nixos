//! Unsaved Settings page changes. Keymap and Behaviour edits go to a draft;
//! nothing is written or takes effect until Save (the preset's Save key, or
//! the button). Discard drops the draft; quitting with one asks first.

use serde_json::{Map, Value};

use crate::app::App;
use crate::config::Settings;
use crate::keymap::Keymap;

/// The Settings page's edited copy of what keymaps.json and settings.json
/// hold.
#[derive(Clone, Debug)]
pub struct Draft {
    pub settings: Settings,
    pub keymap: Keymap,
    pub presets: Vec<Keymap>,
}

/// keymaps.json's contents for a set of presets and the active one.
pub fn keymaps_value(presets: &[Keymap], active: &str) -> Value {
    let mut obj = Map::new();
    for p in presets {
        obj.insert(p.name.clone(), p.to_json());
    }
    let mut m = Map::new();
    m.insert(
        "version".into(),
        Value::from(crate::config::KEYMAPS_VERSION),
    );
    m.insert("active".into(), Value::String(active.to_string()));
    m.insert("presets".into(), Value::Object(obj));
    Value::Object(m)
}

impl Draft {
    fn of(app: &App) -> Draft {
        Draft {
            settings: app.settings.clone(),
            keymap: app.keymap.clone(),
            presets: app.presets.clone(),
        }
    }

    /// Both files' contents, for comparing and counting changes.
    fn files(&self) -> Value {
        serde_json::json!({
            "keymaps": keymaps_value(&self.presets, &self.keymap.name),
            "keymap": self.keymap.to_json(),
            "settings": serde_json::to_value(&self.settings).unwrap_or(Value::Null),
        })
    }
}

impl App {
    /// Swaps the draft and the live settings, keymap and presets.
    fn swap_draft(&mut self, d: &mut Draft) {
        std::mem::swap(&mut self.settings, &mut d.settings);
        std::mem::swap(&mut self.keymap, &mut d.keymap);
        std::mem::swap(&mut self.presets, &mut d.presets);
    }

    /// Runs a Settings page edit on the draft (made from what's live when
    /// there's none yet). A draft that ends up like what's saved goes away.
    pub fn with_draft<R>(&mut self, f: impl FnOnce(&mut App) -> R) -> R {
        self.settings_page.reloaded = false;
        let mut d = self
            .settings_page
            .draft
            .take()
            .unwrap_or_else(|| Draft::of(self));
        self.swap_draft(&mut d);
        // `d` is now what's live
        self.settings_page.draft = Some(d);
        let r = f(self);
        if std::mem::take(&mut self.settings_page.reloaded) {
            // a file was restored or undone: what's loaded now is live
            self.settings_page.draft = None;
            return r;
        }
        let Some(mut live) = self.settings_page.draft.take() else {
            return r;
        };
        self.swap_draft(&mut live);
        // `live` is now the edited draft
        if live.files() != Draft::of(self).files() {
            self.settings_page.draft = Some(live);
        }
        r
    }

    /// Draws (or reads) the Settings page as edited.
    pub fn show_draft<R>(&mut self, f: impl FnOnce(&mut App) -> R) -> R {
        let Some(mut d) = self.settings_page.draft.take() else {
            return f(self);
        };
        self.swap_draft(&mut d);
        let r = f(self);
        self.swap_draft(&mut d);
        self.settings_page.draft = Some(d);
        r
    }

    /// How many values the draft changes (0: nothing unsaved).
    pub fn draft_changes(&self) -> usize {
        let Some(d) = &self.settings_page.draft else {
            return 0;
        };
        let live = Draft::of(self);
        let mut n =
            crate::settings::diff_lines(&live.files()["settings"], &d.files()["settings"], false)
                .iter()
                .filter(|l| !matches!(l, crate::settings::DiffLine::Head { .. }))
                .count();
        n += crate::settings::diff_lines(&live.files()["keymaps"], &d.files()["keymaps"], false)
            .iter()
            .filter(|l| !matches!(l, crate::settings::DiffLine::Head { .. }))
            .count();
        n.max(1)
    }

    /// Writes the draft to keymaps.json and settings.json, and uses it.
    pub fn settings_save(&mut self) {
        let Some(mut d) = self.settings_page.draft.take() else {
            return self.info("No unsaved settings");
        };
        let old_sizes = self.settings.layout.sizes.clone();
        let before = Draft::of(self).files();
        self.swap_draft(&mut d);
        let after = Draft::of(self).files();
        // only the files that changed (so <leader>u undoes the right one)
        let mut errors = Vec::new();
        if before["keymaps"] != after["keymaps"] {
            let keymaps = keymaps_value(&self.presets, &self.keymap.name);
            if let Err(e) =
                crate::store::write_json_file(&self.config_dir.join("keymaps.json"), &keymaps)
            {
                errors.push(format!("keymaps.json: {e:#}"));
            }
        }
        if before["settings"] != after["settings"]
            && let Err(e) = crate::config::save_settings(&self.config_dir, &self.settings)
        {
            errors.push(format!("settings.json: {e:#}"));
        }
        // new default pane sizes apply now too
        if self.settings.layout.sizes != old_sizes {
            self.sizes = self.settings.layout.sizes.clone();
        }
        if errors.is_empty() {
            self.info("Saved the settings");
        } else {
            self.error(format!("Saving: {}", errors.join("; ")));
        }
    }

    /// Drops the unsaved changes.
    pub fn settings_discard(&mut self) {
        if self.settings_page.draft.take().is_some() {
            self.settings_page.browse = None;
            self.info("Discarded the unsaved settings");
        }
    }
}
