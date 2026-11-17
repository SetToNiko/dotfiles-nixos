//! Files the studio reads and writes: palettes.json (the data folder),
//! terminal-colors.json (the share folder), ~/.config/colors/current, and
//! theme-switch (paths.rs says where each is).

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value};

use crate::palette::Palette;

pub struct Store {
    /// The data folder: palettes.json, fonts.json, themes.json.
    pub dir: PathBuf,
    /// The share folder: terminal-colors.json and the other files the
    /// studio and the scripts need.
    pub share: PathBuf,
    /// Where the preview reads the desktop's configs (i3, rofi, nvim).
    pub desktop: PathBuf,
}

/// Which palette key becomes each terminal colour (share/terminal-colors.json).
pub struct TerminalMap {
    pub background: String,
    pub foreground: String,
    pub ansi: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// palettes.json is formatted exactly like `jq .` output, which is what
/// serde_json's pretty printer produces plus a final newline.
pub fn to_jq_string(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).expect("JSON values always serialise");
    s.push('\n');
    s
}

/// palettes.json and fonts.json: version 1 was the plain map of palettes /
/// presets; version 2 wraps it as `{"version": 2, "palettes": { … }}` (fonts:
/// `"presets"`), so the files can be migrated later. Both are read.
pub const DATA_VERSION: u64 = 2;
/// fonts.json's version. 3: the roles are `monospace`, `proportional` and
/// `icons` (were `mono`, `ui` and `bar`). 4: presets have no sizes (the
/// theme has them, in themes.json).
pub const FONTS_VERSION: u64 = 4;

/// The version a data file is written as (by its map's field).
fn version_of(field: &str) -> u64 {
    if field == "presets" {
        FONTS_VERSION
    } else {
        DATA_VERSION
    }
}

/// The map inside a data file, whichever layout it has.
pub fn data_map(v: Value, field: &str) -> Option<Map<String, Value>> {
    let Value::Object(mut m) = v else { return None };
    if m.contains_key("version") {
        match m.remove(field) {
            Some(Value::Object(inner)) => Some(inner),
            _ => None,
        }
    } else {
        Some(m)
    }
}

/// The current file for a map of palettes / presets.
pub fn data_file(field: &str, map: Map<String, Value>) -> Value {
    let mut m = Map::new();
    m.insert("version".into(), Value::from(version_of(field)));
    m.insert(field.into(), Value::Object(map));
    Value::Object(m)
}

/// Writes a JSON file in jq's format, first keeping what it held in the
/// history (labelled with the file's name).
pub fn write_json_file(path: &std::path::Path, value: &Value) -> Result<()> {
    let label = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::history::record(&label, path, value);
    // through a link: write the file it points at, so the link stays
    let path = &crate::paths::write_target(path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, to_jq_string(value))
        .with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

/// Rewrites an older data file as the current version (once), passing its
/// map through `update` on the way. The version it now has, if it did.
pub fn migrate_data_file(
    path: &std::path::Path,
    field: &str,
    update: impl Fn(Map<String, Value>) -> Map<String, Value>,
) -> Result<Option<u64>> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(None);
    };
    let v: Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    let current = version_of(field);
    if v.get("version").and_then(Value::as_u64).unwrap_or(1) >= current {
        return Ok(None);
    }
    let map =
        data_map(v, field).ok_or_else(|| anyhow!("{} is not a JSON object", path.display()))?;
    write_json_file(path, &data_file(field, update(map)))?;
    Ok(Some(current))
}

impl Store {
    pub fn locate() -> Store {
        Store {
            dir: crate::paths::data_dir(),
            share: crate::paths::share_dir(),
            desktop: crate::paths::desktop_dir(),
        }
    }

    /// Everything in one folder (the tests' temporary stores).
    #[cfg(test)]
    pub fn at(dir: PathBuf) -> Store {
        Store {
            share: dir.clone(),
            desktop: dir.clone(),
            dir,
        }
    }

    /// A switch script (`theme-switch`, `font-switch`), told where this
    /// store's data and share folders are.
    pub fn switch_command(&self, name: &str) -> Command {
        crate::paths::switch_command(name, &self.dir, &self.share)
    }

    fn palettes_path(&self) -> PathBuf {
        self.dir.join("palettes.json")
    }

    /// All palettes, in file order.
    /// All palettes, in file order (either layout).
    pub fn load_all(&self) -> Result<Map<String, Value>> {
        let path = self.palettes_path();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let v: Value =
            serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
        data_map(v, "palettes").ok_or_else(|| anyhow!("{} has no palettes", path.display()))
    }

    /// Rewrites palettes.json as version 2 if it's still version 1.
    pub fn migrate(&self) -> Result<Option<u64>> {
        migrate_data_file(&self.palettes_path(), "palettes", |m| m)
    }

    pub fn load(&self, name: &str) -> Result<Palette> {
        let all = self.load_all()?;
        let obj = all
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("no palette named '{name}'"))?;
        Palette::from_json(obj).with_context(|| format!("palette '{name}'"))
    }

    /// The palette theme-switch.sh last applied.
    pub fn active_name(&self) -> Option<String> {
        let s = std::fs::read_to_string(home().join(".config/colors/current")).ok()?;
        Some(s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// Writes one palette into palettes.json (re-read first, so changes made
    /// elsewhere meanwhile, like a delete from the rofi menu, are kept).
    /// An existing palette keeps its place; a new one goes at the end.
    pub fn save(&self, name: &str, palette: &Palette) -> Result<()> {
        let mut all = self.load_all()?;
        all.insert(name.to_string(), Value::Object(palette.to_json()));
        write_json_file(&self.palettes_path(), &data_file("palettes", all))
    }

    /// Runs theme-switch to make a saved palette the system theme.
    pub fn apply(&self, name: &str) -> Result<()> {
        let out = self
            .switch_command("theme-switch")
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("running theme-switch.sh")?;
        if !out.status.success() {
            bail!(
                "theme-switch.sh: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    pub fn terminal_map(&self) -> Result<TerminalMap> {
        let path =
            crate::paths::share_file(&self.share, "terminal-colors.json", "terminal-colors.json");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let v: Value = serde_json::from_str(&text)?;
        let s = |k: &str| {
            v[k].as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("{k} missing"))
        };
        let ansi = v["palette"]
            .as_array()
            .ok_or_else(|| anyhow!("palette missing"))?
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| anyhow!("bad palette entry"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(TerminalMap {
            background: s("background")?,
            foreground: s("foreground")?,
            ansi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Re-serialising the real palettes.json reproduces it byte for byte,
    /// so saving one palette doesn't reformat the rest of the file.
    #[test]
    fn palettes_json_round_trips_byte_for_byte() {
        let store = Store::locate();
        let Ok(text) = std::fs::read_to_string(store.palettes_path()) else {
            eprintln!("skipped: no palettes.json");
            return;
        };
        // whichever layout: the palettes come out of it, and writing the file
        // back gives the same bytes
        assert!(!store.load_all().unwrap().is_empty());
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(to_jq_string(&v), text);
    }

    #[test]
    fn version_1_files_migrate_once_and_stay_version_2() {
        let dir = std::env::temp_dir().join(format!("theme-studio-migrate-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut m = Map::new();
        m.insert("A".into(), Value::Object(Palette::blank().to_json()));
        std::fs::write(
            dir.join("palettes.json"),
            to_jq_string(&Value::Object(m.clone())),
        )
        .unwrap();
        let store = Store::at(dir.clone());
        assert_eq!(store.load_all().unwrap(), m, "version 1 reads");
        assert_eq!(store.migrate().unwrap(), Some(2));
        assert_eq!(store.migrate().unwrap(), None, "only once");
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("palettes.json")).unwrap())
                .unwrap();
        assert_eq!(v, data_file("palettes", m.clone()));
        store.save("B", &Palette::blank()).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("palettes.json")).unwrap())
                .unwrap();
        assert_eq!(v["version"], 2);
        assert_eq!(
            store.load_all().unwrap().keys().collect::<Vec<_>>(),
            ["A", "B"]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn save_replaces_in_place_and_appends_new() {
        let dir = std::env::temp_dir().join(format!("theme-studio-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let blank = Value::Object(Palette::blank().to_json());
        let mut m = Map::new();
        m.insert("A".into(), blank.clone());
        m.insert("B".into(), blank);
        std::fs::write(dir.join("palettes.json"), to_jq_string(&Value::Object(m))).unwrap();
        let store = Store::at(dir.clone());

        let mut p = Palette::blank();
        p.set("red", crate::color::Rgb(1, 2, 3));
        store.save("A", &p).unwrap();
        store.save("C", &p).unwrap();
        let all = store.load_all().unwrap();
        assert_eq!(all.keys().collect::<Vec<_>>(), ["A", "B", "C"]);
        assert_eq!(store.load("A").unwrap(), p);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
