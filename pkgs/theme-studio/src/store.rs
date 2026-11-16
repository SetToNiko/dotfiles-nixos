//! Files the studio reads and writes: configs/palettes.json,
//! configs/terminal-colors.json, ~/.config/colors/current, theme-switch.sh.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value};

use crate::palette::Palette;

pub struct Store {
    /// The dotfiles configs/ folder ($THEME_STUDIO_CONFIGS, or ~/dotfiles-nixos/configs).
    pub dir: PathBuf,
}

/// Which palette key becomes each terminal colour (configs/terminal-colors.json).
pub struct TerminalMap {
    pub background: String,
    pub foreground: String,
    pub ansi: Vec<String>,
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// palettes.json is formatted exactly like `jq .` output, which is what
/// serde_json's pretty printer produces plus a final newline.
pub fn to_jq_string(v: &Value) -> String {
    let mut s = serde_json::to_string_pretty(v).expect("JSON values always serialise");
    s.push('\n');
    s
}

impl Store {
    pub fn locate() -> Store {
        let dir = std::env::var_os("THEME_STUDIO_CONFIGS")
            .map(PathBuf::from)
            .unwrap_or_else(|| home().join("dotfiles-nixos/configs"));
        Store { dir }
    }

    fn palettes_path(&self) -> PathBuf {
        self.dir.join("palettes.json")
    }

    /// All palettes, in file order.
    pub fn load_all(&self) -> Result<Map<String, Value>> {
        let path = self.palettes_path();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        match serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))? {
            Value::Object(m) => Ok(m),
            _ => bail!("{} is not a JSON object", path.display()),
        }
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
        let path = self.palettes_path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, to_jq_string(&Value::Object(all)))
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("replacing {}", path.display()))
    }

    /// Runs theme-switch.sh to make a saved palette the system theme.
    pub fn apply(&self, name: &str) -> Result<()> {
        let out = Command::new("bash")
            .arg(self.dir.join("theme-switch.sh"))
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("running theme-switch.sh")?;
        if !out.status.success() {
            bail!("theme-switch.sh: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(())
    }

    pub fn terminal_map(&self) -> Result<TerminalMap> {
        let path = self.dir.join("terminal-colors.json");
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let v: Value = serde_json::from_str(&text)?;
        let s = |k: &str| v[k].as_str().map(str::to_string).ok_or_else(|| anyhow!("{k} missing"));
        let ansi = v["palette"]
            .as_array()
            .ok_or_else(|| anyhow!("palette missing"))?
            .iter()
            .map(|x| x.as_str().map(str::to_string).ok_or_else(|| anyhow!("bad palette entry")))
            .collect::<Result<Vec<_>>>()?;
        Ok(TerminalMap { background: s("background")?, foreground: s("foreground")?, ansi })
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
        let all = store.load_all().unwrap();
        assert_eq!(to_jq_string(&Value::Object(all)), text);
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
        let store = Store { dir: dir.clone() };

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
