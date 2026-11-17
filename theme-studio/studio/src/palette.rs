//! A palette: the 34 keys of palettes.json, which of them follow the 8 base
//! colours, and the derive rules (a port of share/palette-derive.jq).

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Value};

use crate::color::Rgb;

/// Every key, in the order palette-derive.jq (and so palettes.json) uses.
pub const KEYS: [&str; 34] = [
    "bg_dim",
    "bg0",
    "bg1",
    "bg2",
    "bg3",
    "bg4",
    "bg5",
    "bg_statusline1",
    "bg_statusline2",
    "bg_statusline3",
    "bg_visual_red",
    "bg_visual_yellow",
    "bg_visual_green",
    "bg_visual_blue",
    "bg_visual_purple",
    "bg_diff_red",
    "bg_diff_green",
    "bg_diff_blue",
    "bg_current_word",
    "fg0",
    "fg1",
    "red",
    "orange",
    "yellow",
    "green",
    "aqua",
    "blue",
    "purple",
    "bg_red",
    "bg_green",
    "bg_yellow",
    "grey0",
    "grey1",
    "grey2",
];

/// The colours a palette is built from; everything else derives from these.
pub const BASE: [&str; 8] = [
    "bg0", "fg0", "red", "green", "yellow", "blue", "purple", "aqua",
];

/// How the editor groups the keys.
pub const GROUPS: [(&str, &[&str]); 5] = [
    ("Base", &BASE),
    (
        "Backgrounds",
        &[
            "bg_dim",
            "bg1",
            "bg2",
            "bg3",
            "bg4",
            "bg5",
            "bg_statusline1",
            "bg_statusline2",
            "bg_statusline3",
            "bg_current_word",
        ],
    ),
    (
        "Text and accents",
        &["fg1", "orange", "bg_red", "bg_green", "bg_yellow"],
    ),
    ("Greys", &["grey0", "grey1", "grey2"]),
    (
        "Tinted backgrounds",
        &[
            "bg_visual_red",
            "bg_visual_yellow",
            "bg_visual_green",
            "bg_visual_blue",
            "bg_visual_purple",
            "bg_diff_red",
            "bg_diff_green",
            "bg_diff_blue",
        ],
    ),
];

/// Keys used as text, so their contrast against bg0 matters.
pub const TEXT_KEYS: [&str; 12] = [
    "fg0", "fg1", "red", "orange", "yellow", "green", "aqua", "blue", "purple", "grey0", "grey1",
    "grey2",
];

pub fn is_base(key: &str) -> bool {
    BASE.contains(&key)
}

/// Interns a key name as the matching `&'static str` from KEYS.
pub fn key(name: &str) -> Option<&'static str> {
    KEYS.iter().copied().find(|k| *k == name)
}

/// Which base colours a derived key is computed from (for the editor's label).
pub fn sources(key: &str) -> &'static str {
    match key {
        "bg_dim" => "bg0",
        "fg1" => "fg0",
        "orange" => "red+yellow",
        "bg_red" => "red",
        "bg_green" => "green",
        "bg_yellow" => "yellow",
        "bg_visual_red" | "bg_diff_red" => "bg0+red",
        "bg_visual_yellow" => "bg0+yellow",
        "bg_visual_green" | "bg_diff_green" => "bg0+green",
        "bg_visual_blue" | "bg_diff_blue" => "bg0+blue",
        "bg_visual_purple" => "bg0+purple",
        k if is_base(k) => "",
        _ => "bg0+fg0",
    }
}

/// The keys tied to `key` by derivation: the base colours a derived key is
/// computed from, or the keys computed from a base colour.
pub fn related(key: &str) -> Vec<&'static str> {
    if is_base(key) {
        GROUPS
            .iter()
            .flat_map(|(_, keys)| keys.iter().copied())
            .filter(|k| !is_base(k) && sources(k).split('+').any(|s| s == key))
            .collect()
    } else {
        sources(key).split('+').filter(|s| !s.is_empty()).collect()
    }
}

/// Blends colour `a` towards `b` by `t`, rounding like palette-derive.jq.
fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let ch = |x: u8, y: u8| {
        let (x, y) = (x as f64, y as f64);
        (x + (y - x) * t + 0.5).floor() as u8
    };
    Rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

fn lum(c: Rgb) -> f64 {
    c.0 as f64 * 0.299 + c.1 as f64 * 0.587 + c.2 as f64 * 0.114
}

/// All 34 keys computed from the 8 base colours (palette-derive.jq's `derive`).
pub fn derive(base: &HashMap<&'static str, Rgb>) -> HashMap<&'static str, Rgb> {
    let b = |k: &str| base[k];
    let (bg, fg) = (b("bg0"), b("fg0"));
    let light = lum(bg) > lum(fg);
    let black = Rgb(0, 0, 0);
    let mut d = HashMap::new();
    let mut put = |k: &'static str, c: Rgb| {
        d.insert(k, c);
    };
    put(
        "bg_dim",
        if light {
            mix(bg, fg, 0.05)
        } else {
            mix(bg, black, 0.3)
        },
    );
    put("bg0", bg);
    put("bg1", mix(bg, fg, 0.05));
    put("bg2", mix(bg, fg, 0.05));
    put("bg3", mix(bg, fg, 0.15));
    put("bg4", mix(bg, fg, 0.15));
    put("bg5", mix(bg, fg, 0.25));
    put("bg_statusline1", mix(bg, fg, 0.05));
    put("bg_statusline2", mix(bg, fg, 0.09));
    put("bg_statusline3", mix(bg, fg, 0.25));
    put("bg_visual_red", mix(bg, b("red"), 0.18));
    put("bg_visual_yellow", mix(bg, b("yellow"), 0.18));
    put("bg_visual_green", mix(bg, b("green"), 0.18));
    put("bg_visual_blue", mix(bg, b("blue"), 0.18));
    put("bg_visual_purple", mix(bg, b("purple"), 0.18));
    put("bg_diff_red", mix(bg, b("red"), 0.12));
    put("bg_diff_green", mix(bg, b("green"), 0.12));
    put("bg_diff_blue", mix(bg, b("blue"), 0.12));
    put("bg_current_word", mix(bg, fg, 0.1));
    put("fg0", fg);
    put("fg1", fg);
    put("red", b("red"));
    put("orange", mix(b("red"), b("yellow"), 0.5));
    put("yellow", b("yellow"));
    put("green", b("green"));
    put("aqua", b("aqua"));
    put("blue", b("blue"));
    put("purple", b("purple"));
    put("bg_red", b("red"));
    put("bg_green", b("green"));
    put("bg_yellow", b("yellow"));
    put("grey0", mix(bg, fg, 0.43));
    put("grey1", mix(bg, fg, 0.55));
    put("grey2", mix(bg, fg, 0.67));
    d
}

#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    values: HashMap<&'static str, Rgb>,
    /// Derived keys set by hand: they keep their value when the base changes.
    pinned: HashSet<&'static str>,
    /// Keys in the file that aren't one of the 34 (kept as-is when saving).
    extra: Vec<(String, Value)>,
}

impl Palette {
    /// Reads a palette from its palettes.json object. A derived key counts as
    /// pinned when its value differs from what the base colours would give.
    pub fn from_json(obj: &Map<String, Value>) -> Result<Palette> {
        let mut values = HashMap::new();
        for k in BASE {
            let v = obj
                .get(k)
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("missing {k}"))?;
            values.insert(
                k,
                Rgb::parse(v).with_context(|| format!("bad colour for {k}: {v}"))?,
            );
        }
        let mut pinned = HashSet::new();
        for (k, derived) in derive(&values) {
            if is_base(k) {
                continue;
            }
            let given = obj.get(k).and_then(Value::as_str).and_then(Rgb::parse);
            match given {
                Some(c) if c != derived => {
                    pinned.insert(k);
                    values.insert(k, c);
                }
                _ => {
                    values.insert(k, derived);
                }
            }
        }
        let extra = obj
            .iter()
            .filter(|(k, _)| key(k).is_none())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Palette {
            values,
            pinned,
            extra,
        })
    }

    /// A neutral dark starting point for a new palette.
    pub fn blank() -> Palette {
        let base = [
            ("bg0", "#1e1e22"),
            ("fg0", "#d8d4cc"),
            ("red", "#e06c6c"),
            ("green", "#9cc47a"),
            ("yellow", "#e2b860"),
            ("blue", "#6fa3d8"),
            ("purple", "#b48ad8"),
            ("aqua", "#6cc4b4"),
        ];
        let mut values = HashMap::new();
        for (k, v) in base {
            values.insert(key(k).unwrap(), Rgb::parse(v).unwrap());
        }
        let mut p = Palette {
            values,
            pinned: HashSet::new(),
            extra: Vec::new(),
        };
        p.recompute();
        p
    }

    /// The palette's object for palettes.json, keys in the standard order.
    pub fn to_json(&self) -> Map<String, Value> {
        let mut m = Map::new();
        for k in KEYS {
            m.insert(k.to_string(), Value::String(self.get(k).hex()));
        }
        for (k, v) in &self.extra {
            m.insert(k.clone(), v.clone());
        }
        m
    }

    pub fn get(&self, key: &str) -> Rgb {
        self.values[key]
    }

    pub fn is_pinned(&self, key: &str) -> bool {
        self.pinned.contains(key)
    }

    /// Sets a key. A derived key set by hand becomes pinned.
    pub fn set(&mut self, key: &'static str, c: Rgb) {
        self.values.insert(key, c);
        if !is_base(key) {
            self.pinned.insert(key);
        }
        self.recompute();
    }

    /// Lets a derived key follow the base colours again.
    pub fn unpin(&mut self, key: &str) {
        self.pinned.remove(key);
        self.recompute();
    }

    /// Recomputes every derived key that isn't pinned.
    fn recompute(&mut self) {
        let base: HashMap<_, _> = BASE.iter().map(|k| (*k, self.values[k])).collect();
        for (k, c) in derive(&base) {
            if !is_base(k) && !self.pinned.contains(k) {
                self.values.insert(k, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::process::Command;

    fn configs() -> Option<PathBuf> {
        let dir = crate::store::Store::locate().dir;
        dir.join("palettes.json").exists().then_some(dir)
    }

    fn all_palettes(dir: &std::path::Path) -> Map<String, Value> {
        let text = std::fs::read_to_string(dir.join("palettes.json")).unwrap();
        crate::store::data_map(serde_json::from_str(&text).unwrap(), "palettes").unwrap()
    }

    #[test]
    fn every_key_is_in_exactly_one_group() {
        let mut seen: Vec<&str> = GROUPS
            .iter()
            .flat_map(|(_, ks)| ks.iter().copied())
            .collect();
        seen.sort();
        let mut keys = KEYS.to_vec();
        keys.sort();
        assert_eq!(seen, keys);
    }

    /// Runs palette-derive.jq on the base colours of every real palette and
    /// checks the Rust port gives exactly the same 34 colours.
    #[test]
    fn derive_matches_palette_derive_jq() {
        let Some(dir) = configs() else {
            eprintln!("skipped: no palettes.json in the data folder");
            return;
        };
        let all = all_palettes(&dir);
        let bases: Map<String, Value> = all
            .iter()
            .map(|(name, p)| {
                let o = p.as_object().unwrap();
                let b: Map<String, Value> = BASE
                    .iter()
                    .map(|k| (k.to_string(), o[*k].clone()))
                    .collect();
                (name.clone(), Value::Object(b))
            })
            .collect();
        let input =
            std::env::temp_dir().join(format!("theme-studio-derive-{}.json", std::process::id()));
        std::fs::write(&input, serde_json::to_string(&bases).unwrap()).unwrap();
        let share = crate::store::Store::locate().share;
        let out = Command::new("jq")
            .args(["-r", "-f"])
            .arg(crate::paths::share_file(&share, "palette-derive.jq"))
            .arg(&input)
            .output();
        let Ok(out) = out else {
            eprintln!("skipped: jq not available");
            return;
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        // palette-derive.jq writes the version-2 layout
        let expected =
            crate::store::data_map(serde_json::from_slice(&out.stdout).unwrap(), "palettes")
                .unwrap();
        assert_eq!(expected.len(), all.len());
        for (name, want) in &expected {
            let want = want.as_object().unwrap();
            let base: HashMap<_, _> = BASE
                .iter()
                .map(|k| (*k, Rgb::parse(want[*k].as_str().unwrap()).unwrap()))
                .collect();
            let got = derive(&base);
            let want_keys: Vec<&str> = want.keys().map(String::as_str).collect();
            assert_eq!(want_keys, KEYS.to_vec(), "key order for {name}");
            for k in KEYS {
                assert_eq!(got[k].hex(), want[k].as_str().unwrap(), "{name}: {k}");
            }
        }
    }

    /// Loading and saving every real palette gives back the same object,
    /// so pins are inferred correctly and nothing is lost.
    #[test]
    fn every_palette_round_trips() {
        let Some(dir) = configs() else {
            eprintln!("skipped: no palettes.json in the data folder");
            return;
        };
        let mut pinned_somewhere = 0;
        for (name, p) in all_palettes(&dir) {
            let obj = p.as_object().unwrap();
            let pal = Palette::from_json(obj).unwrap();
            pinned_somewhere += (!pal.pinned.is_empty()) as usize;
            assert_eq!(&pal.to_json(), obj, "{name}");
        }
        assert!(
            pinned_somewhere > 0,
            "expected hand-tuned palettes (e.g. Gruvbox Dark Hard)"
        );
    }

    #[test]
    fn derived_keys_follow_base_until_pinned() {
        let mut p = Palette::blank();
        p.set("bg0", Rgb(0, 0, 0));
        assert_eq!(p.get("grey1"), mix(Rgb(0, 0, 0), p.get("fg0"), 0.55));
        p.set("grey1", Rgb(1, 2, 3));
        assert!(p.is_pinned("grey1"));
        p.set("bg0", Rgb(10, 10, 10));
        assert_eq!(p.get("grey1"), Rgb(1, 2, 3));
        p.unpin("grey1");
        assert_eq!(p.get("grey1"), mix(Rgb(10, 10, 10), p.get("fg0"), 0.55));
    }
}

#[cfg(test)]
mod related_tests {
    use super::*;

    #[test]
    fn related_keys_match_derive() {
        // a dark palette; nudging a base colour must only move the keys that
        // name it as a source
        let mut base: HashMap<&'static str, Rgb> = HashMap::new();
        for (i, k) in BASE.iter().enumerate() {
            base.insert(k, Rgb(20 + 25 * i as u8, 60, 90));
        }
        base.insert("bg0", Rgb(20, 20, 24));
        base.insert("fg0", Rgb(220, 220, 210));
        let before = derive(&base);
        for b in BASE {
            let mut nudged = base.clone();
            let c = nudged[b];
            nudged.insert(b, Rgb(c.0.wrapping_add(40), c.1.wrapping_add(40), c.2));
            let after = derive(&nudged);
            for (k, v) in &after {
                if !is_base(k) && before[k] != *v {
                    assert!(related(k).contains(&b), "{k} changes with {b}");
                    assert!(related(b).contains(k), "{b} lists {k}");
                }
            }
        }
        assert_eq!(related("bg_visual_red"), ["bg0", "red"]);
        assert!(related("red").contains(&"orange") && related("red").contains(&"bg_diff_red"));
        assert!(!related("red").contains(&"bg3"));
    }
}
