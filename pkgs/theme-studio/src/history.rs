//! Version history of the config files (palettes.json, fonts.json,
//! keymaps.json, settings.json). Before every write the studio makes, the
//! file as it was is kept in `$XDG_STATE_HOME/theme-studio/history/<file>/`
//! (default `~/.local/state/…`), with a one-line summary of what the write
//! changed. The newest 100 per file are kept; named versions (saved on
//! purpose) are never pruned.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Automatic snapshots kept per file.
pub const KEEP: usize = 100;

/// Where history goes for this run (set by `init`; unset in tests, so tests
/// never write to your state folder).
static DIR: OnceLock<PathBuf> = OnceLock::new();

/// `$XDG_STATE_HOME/theme-studio/history`, or `~/.local/state/…`.
pub fn default_dir() -> PathBuf {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".local/state")
        });
    base.join("theme-studio/history")
}

pub fn init(dir: PathBuf) {
    let _ = DIR.set(dir);
}

/// One saved version of a file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Milliseconds since 1970, for ordering.
    pub time: u64,
    /// The local time, for showing.
    pub when: String,
    /// Set for versions saved on purpose (never pruned).
    pub name: Option<String>,
    /// What the write after this snapshot changed ("novmbr: red, aqua").
    pub summary: String,
    /// The file's contents.
    pub content: Value,
}

pub struct History {
    dir: PathBuf,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn local_time() -> String {
    Command::new("date")
        .arg("+%Y-%m-%d %H:%M:%S")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("{}", now_ms() / 1000))
}

/// Unique file names even for several snapshots in one millisecond.
fn stamp() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    format!(
        "{:013}-{:04}",
        now_ms(),
        N.fetch_add(1, Ordering::Relaxed) % 10_000
    )
}

impl History {
    /// The history for this run, if `init` was called.
    pub fn current() -> Option<History> {
        DIR.get().map(|d| History { dir: d.clone() })
    }

    #[allow(dead_code)] // used by the History tab (rework step 8)
    pub fn at(dir: &Path) -> History {
        History {
            dir: dir.to_path_buf(),
        }
    }

    fn folder(&self, file: &str) -> PathBuf {
        self.dir.join(file.trim_end_matches(".json"))
    }

    fn write(&self, path: &Path, s: &Snapshot) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        let text = serde_json::to_string_pretty(s)?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Keeps `before` (the file before a write of `after`), unless nothing
    /// changed; then prunes old automatic snapshots.
    pub fn record(&self, file: &str, before: &Value, after: &Value) -> Result<()> {
        if before == after {
            return Ok(());
        }
        let s = Snapshot {
            time: now_ms(),
            when: local_time(),
            name: None,
            summary: summarize(before, after),
            content: before.clone(),
        };
        self.write(&self.folder(file).join(format!("{}.json", stamp())), &s)?;
        self.prune(file);
        Ok(())
    }

    /// Saves `content` as a named version (never pruned).
    #[allow(dead_code)] // used by the History tab (rework step 8)
    pub fn save_named(&self, file: &str, name: &str, content: &Value) -> Result<PathBuf> {
        let s = Snapshot {
            time: now_ms(),
            when: local_time(),
            name: Some(name.to_string()),
            summary: format!("saved as \"{name}\""),
            content: content.clone(),
        };
        let path = self.folder(file).join(format!("named-{}.json", stamp()));
        self.write(&path, &s)?;
        Ok(path)
    }

    /// Every version of a file: named ones first, then newest first.
    pub fn list(&self, file: &str) -> Vec<(PathBuf, Snapshot)> {
        let Ok(entries) = std::fs::read_dir(self.folder(file)) else {
            return Vec::new();
        };
        let mut out: Vec<(PathBuf, Snapshot)> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .filter_map(|p| {
                let s =
                    serde_json::from_str::<Snapshot>(&std::fs::read_to_string(&p).ok()?).ok()?;
                Some((p, s))
            })
            .collect();
        out.sort_by_key(|(_, s)| std::cmp::Reverse((s.name.is_some(), s.time)));
        out
    }

    #[allow(dead_code)] // used by the History tab (rework step 8)
    pub fn delete(&self, path: &Path) -> Result<()> {
        std::fs::remove_file(path).with_context(|| format!("removing {}", path.display()))
    }

    fn prune(&self, file: &str) {
        let auto: Vec<PathBuf> = self
            .list(file)
            .into_iter()
            .filter(|(_, s)| s.name.is_none())
            .map(|(p, _)| p)
            .collect();
        for p in auto.iter().skip(KEEP) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Called before a file is written: keeps what it held (if this run has a
/// history and the file exists and parses).
pub fn record(file: &str, path: &Path, new: &Value) {
    let Some(h) = History::current() else { return };
    let Some(before) = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    else {
        return;
    };
    let _ = h.record(file, &before, new);
}

#[derive(Debug)]
enum Change {
    Changed(Vec<String>),
    Added(Vec<String>),
    Removed(Vec<String>),
}

fn diff(a: &Value, b: &Value, path: &mut Vec<String>, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                path.push(k.clone());
                match y.get(k) {
                    Some(vb) => diff(va, vb, path, out),
                    None => out.push(Change::Removed(path.clone())),
                }
                path.pop();
            }
            for k in y.keys().filter(|k| !x.contains_key(*k)) {
                path.push(k.clone());
                out.push(Change::Added(path.clone()));
                path.pop();
            }
        }
        _ if a != b => out.push(Change::Changed(path.clone())),
        _ => {}
    }
}

/// One line on what changed between two versions of a config file, e.g.
/// "novmbr: red, aqua", "added Wide bar", "Vim: quit, undo", "font-list:
/// hide-bitmap".
pub fn summarize(before: &Value, after: &Value) -> String {
    if before.get("version").is_none()
        && after.get("version").is_some()
        && after.as_object().is_some_and(|o| o.len() == 2)
    {
        return "file format updated to version 2".into();
    }
    let mut changes = Vec::new();
    diff(before, after, &mut Vec::new(), &mut changes);
    // path parts that are just containers
    const NOISE: [&str; 3] = ["palettes", "presets", "bindings"];
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let mut add = |group: String, item: String| match groups.iter_mut().find(|(g, _)| *g == group) {
        Some((_, items)) => {
            if !items.contains(&item) {
                items.push(item)
            }
        }
        None => groups.push((group, if item.is_empty() { vec![] } else { vec![item] })),
    };
    for c in &changes {
        let (path, what) = match c {
            Change::Changed(p) => (p, ""),
            Change::Added(p) => (p, "added "),
            Change::Removed(p) => (p, "removed "),
        };
        let parts: Vec<&String> = path
            .iter()
            .filter(|p| !NOISE.contains(&p.as_str()))
            .collect();
        match parts.as_slice() {
            [] => add("everything".into(), String::new()),
            [one] => add(format!("{what}{one}"), String::new()),
            [first, rest @ ..] => {
                let leaf = rest.last().map(|s| s.as_str()).unwrap_or("");
                add(first.to_string(), format!("{what}{leaf}"))
            }
        }
    }
    let mut text = groups
        .iter()
        .map(|(g, items)| {
            if items.is_empty() {
                g.clone()
            } else {
                format!("{g}: {}", items.join(", "))
            }
        })
        .collect::<Vec<_>>()
        .join("; ");
    if text.chars().count() > 90 {
        text = text.chars().take(89).collect::<String>() + "…";
    }
    if text.is_empty() {
        "no change".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("theme-studio-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn summaries_say_what_changed() {
        let a = json!({"version": 2, "palettes": {"novmbr": {"red": "#1", "aqua": "#2", "bg0": "#3"}, "Old": {}}});
        let b = json!({"version": 2, "palettes": {"novmbr": {"red": "#9", "aqua": "#8", "bg0": "#3"}, "New": {}}});
        assert_eq!(
            summarize(&a, &b),
            "novmbr: red, aqua; removed Old; added New"
        );
        let k1 = json!({"presets": {"Vim": {"bindings": {"global": {"quit": ["q"]}}}}});
        let k2 = json!({"presets": {"Vim": {"bindings": {"global": {"quit": ["Q"]}}}}});
        assert_eq!(summarize(&k1, &k2), "Vim: quit");
        assert_eq!(
            summarize(
                &json!({"a": 1}),
                &json!({"version": 2, "palettes": {"a": 1}})
            ),
            "file format updated to version 2"
        );
        assert_eq!(summarize(&json!({"x": 1}), &json!({"x": 1})), "no change");
    }

    #[test]
    fn records_lists_prunes_and_keeps_named() {
        let d = temp("prune");
        let h = History::at(&d);
        h.save_named("palettes.json", "before Emacs", &json!({"n": -1}))
            .unwrap();
        for i in 0..(KEEP + 5) {
            h.record("palettes.json", &json!({"n": i}), &json!({"n": i + 1}))
                .unwrap();
        }
        // nothing recorded when nothing changes
        h.record("palettes.json", &json!({"same": 1}), &json!({"same": 1}))
            .unwrap();
        let list = h.list("palettes.json");
        assert_eq!(list.len(), KEEP + 1, "100 automatic + the named one");
        assert_eq!(
            list[0].1.name.as_deref(),
            Some("before Emacs"),
            "named first"
        );
        assert_eq!(
            list[1].1.content,
            json!({"n": KEEP + 4}),
            "then newest first"
        );
        assert_eq!(list[1].1.summary, "n");
        assert!(
            list.iter()
                .all(|(_, s)| s.name.is_some() || s.content["n"].as_u64().unwrap() >= 5),
            "oldest pruned"
        );
        h.delete(&list[0].0).unwrap();
        assert_eq!(h.list("palettes.json").len(), KEEP);
        assert!(h.list("fonts.json").is_empty());
        std::fs::remove_dir_all(d).unwrap();
    }
}
