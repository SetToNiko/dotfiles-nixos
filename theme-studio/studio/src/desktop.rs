//! What the preview's model desktop is made of, read from the real configs:
//! which palette key each i3 and rofi colour role uses (i3/theme,
//! rofi/config.rasi), and the i3blocks bar text (i3blocks/config).

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::color::Rgb;
use crate::palette::{self, Palette};

/// Colour roles: role name -> palette keys (one per column, e.g.
/// `client.focused` -> [border, background, text, indicator, child_border]).
#[derive(Clone, Debug, Default)]
pub struct Roles {
    roles: HashMap<String, Vec<&'static str>>,
}

impl Roles {
    /// Reads i3/theme and rofi/config.rasi; anything missing falls back to
    /// what those files contain today.
    pub fn load(configs: &Path) -> Roles {
        let mut r = Roles::defaults();
        if let Ok(text) = std::fs::read_to_string(configs.join("i3/theme")) {
            r.read_i3(&text);
        }
        if let Ok(text) = std::fs::read_to_string(configs.join("rofi/config.rasi")) {
            r.read_rasi(&text);
        }
        r
    }

    fn defaults() -> Roles {
        let mut r = Roles::default();
        let set = |r: &mut Roles, name: &str, keys: &[&'static str]| {
            r.roles.insert(name.to_string(), keys.to_vec());
        };
        set(
            &mut r,
            "client.focused",
            &["yellow", "yellow", "bg0", "blue", "yellow"],
        );
        set(
            &mut r,
            "client.unfocused",
            &["bg0", "bg0", "grey2", "bg0", "bg0"],
        );
        set(&mut r, "background", &["bg0"]);
        set(&mut r, "statusline", &["fg0"]);
        set(&mut r, "separator", &["bg3"]);
        set(&mut r, "focused_workspace", &["yellow", "yellow", "bg0"]);
        set(&mut r, "inactive_workspace", &["bg0", "bg0", "grey2"]);
        set(&mut r, "rofi.background", &["bg0"]);
        set(&mut r, "rofi.foreground", &["fg0"]);
        set(&mut r, "rofi.selected-normal-background", &["yellow"]);
        set(&mut r, "rofi.selected-normal-foreground", &["bg0"]);
        set(&mut r, "rofi.border-color", &["yellow"]);
        set(&mut r, "rofi.separatorcolor", &["grey2"]);
        r
    }

    /// i3 lines like `client.focused $yellow $yellow $bg0 $blue $yellow` and,
    /// inside `bar { colors { … } }`, `focused_workspace $yellow $yellow $bg0`.
    fn read_i3(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let mut words = line.split_whitespace();
            let Some(name) = words.next() else { continue };
            let keys: Vec<&'static str> = words
                .filter_map(|w| w.strip_prefix('$'))
                .filter_map(palette::key)
                .collect();
            if !keys.is_empty() {
                self.roles.insert(name.to_string(), keys);
            }
        }
    }

    /// rasi lines like `selected-normal-background: var(yellow);`.
    fn read_rasi(&mut self, text: &str) {
        for line in text.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim().trim_end_matches(';').trim();
            let Some(var) = value.strip_prefix("var(").and_then(|v| v.strip_suffix(')')) else {
                continue;
            };
            if let Some(k) = palette::key(var.trim()) {
                self.roles.insert(format!("rofi.{}", name.trim()), vec![k]);
            }
        }
    }

    /// Colour of column `i` of a role, in palette `p`.
    pub fn get(&self, p: &Palette, role: &str, i: usize) -> Rgb {
        let key = self
            .roles
            .get(role)
            .and_then(|ks| ks.get(i).or(ks.last()))
            .copied()
            .unwrap_or("fg0");
        p.get(key)
    }
}

/// One i3blocks block as printed right now.
#[derive(Clone, Debug, PartialEq)]
pub struct BarBlock {
    pub text: String,
    /// Colour the script chose itself (third output line, or a Pango
    /// `color`/`foreground` attribute); None uses the bar's statusline colour.
    pub color: Option<Rgb>,
}

/// Runs every block in i3blocks/config once and takes its first output, so
/// the preview bar shows real text. Blocks that print nothing within the
/// timeout (or nothing at all, like battery on the desktop) are left out.
pub fn read_bar(configs: &Path, timeout: Duration) -> Vec<BarBlock> {
    let Ok(text) = std::fs::read_to_string(configs.join("i3blocks/config")) else {
        return Vec::new();
    };
    let blocks = parse_i3blocks(&text);
    let handles: Vec<_> = blocks
        .into_iter()
        .map(|(name, command, pango)| {
            std::thread::spawn(move || run_block(&name, &command, pango, timeout))
        })
        .collect();
    handles
        .into_iter()
        .filter_map(|h| h.join().ok().flatten())
        .collect()
}

/// (name, command, markup=pango) for each block section.
fn parse_i3blocks(text: &str) -> Vec<(String, String, bool)> {
    let mut out: Vec<(String, String, bool)> = Vec::new();
    let mut current: Option<(String, Option<String>, bool)> = None;
    let flush = |cur: &mut Option<(String, Option<String>, bool)>, out: &mut Vec<_>| {
        if let Some((n, Some(c), p)) = cur.take() {
            out.push((n, c, p));
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            flush(&mut current, &mut out);
            current = Some((name.to_string(), None, false));
        } else if let (Some(cur), Some((k, v))) = (current.as_mut(), line.split_once('=')) {
            match k.trim() {
                "command" => cur.1 = Some(v.trim().to_string()),
                "markup" => cur.2 = v.trim() == "pango",
                _ => {}
            }
        }
    }
    flush(&mut current, &mut out);
    out
}

fn run_block(name: &str, command: &str, pango: bool, timeout: Duration) -> Option<BarBlock> {
    use std::os::unix::process::CommandExt;
    let mut child = Command::new("bash")
        .args(["-c", command])
        .env("BLOCK_NAME", name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // own process group, so the whole script (pw-mon, sleep, …) can be stopped
        .process_group(0)
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    // Up to three lines: full_text, short_text, color. Persistent blocks
    // print a line and keep running, so take what has arrived and move on.
    let mut lines = Vec::new();
    if let Ok(l) = rx.recv_timeout(timeout) {
        lines.push(l);
        while lines.len() < 3 {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(l) => lines.push(l),
                Err(_) => break,
            }
        }
    }
    let _ = Command::new("kill")
        .args(["-TERM", "--", &format!("-{}", child.id())])
        .status();
    let _ = child.kill();
    let _ = child.wait();
    let full = lines.first()?.trim();
    if full.is_empty() {
        return None;
    }
    let (text, markup_color) = if pango {
        strip_pango(full)
    } else {
        (full.to_string(), None)
    };
    let color = lines.get(2).and_then(|c| Rgb::parse(c)).or(markup_color);
    Some(BarBlock { text, color })
}

/// Removes Pango tags; returns the text and the first colour attribute.
fn strip_pango(s: &str) -> (String, Option<Rgb>) {
    let mut text = String::new();
    let mut color = None;
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start..start + end];
        if color.is_none() {
            for attr in ["color=", "foreground=", "fgcolor="] {
                if let Some(i) = tag.find(attr) {
                    let v = tag[i + attr.len()..].trim_start_matches(['"', '\'']);
                    color = Rgb::parse(&v.chars().take(7).collect::<String>());
                }
            }
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    (text, color)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_i3_and_rasi_roles() {
        let mut r = Roles::default();
        r.read_i3(
            "# Columns\n  client.focused $red $yellow $bg0 $blue $green # trailing\nbar {\n colors {\n  background $bg1\n  focused_workspace  $aqua $aqua $bg0\n }\n}",
        );
        r.read_rasi("* {\n    selected-normal-background: var(purple);\n    foo: #123456;\n}");
        let p = Palette::blank();
        assert_eq!(r.get(&p, "client.focused", 4), p.get("green"));
        assert_eq!(r.get(&p, "background", 0), p.get("bg1"));
        assert_eq!(r.get(&p, "focused_workspace", 1), p.get("aqua"));
        assert_eq!(
            r.get(&p, "rofi.selected-normal-background", 0),
            p.get("purple")
        );
    }

    #[test]
    fn real_configs_parse() {
        let dir = crate::store::Store::locate().dir;
        if !dir.join("i3/theme").exists() {
            eprintln!("skipped: no configs");
            return;
        }
        let r = Roles::load(&dir);
        let p = Palette::blank();
        assert_eq!(r.get(&p, "client.focused", 4), p.get("yellow"));
        assert_eq!(r.get(&p, "rofi.border-color", 0), p.get("yellow"));
        let blocks = parse_i3blocks(&std::fs::read_to_string(dir.join("i3blocks/config")).unwrap());
        assert!(
            blocks
                .iter()
                .any(|(n, c, _)| n == "time" && c.contains("date"))
        );
        assert!(
            blocks
                .iter()
                .any(|(n, _, pango)| n == "bluetooth" && *pango)
        );
    }

    #[test]
    fn blocks_take_first_lines_and_time_out() {
        let dir = std::env::temp_dir().join(format!("theme-studio-bar-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("i3blocks")).unwrap();
        std::fs::write(
            dir.join("i3blocks/config"),
            "separator_block_width=15\n\
             [plain]\ncommand=echo hello\n\
             [colored]\ncommand=printf 'up\\nup\\n#00ff00\\n'\n\
             [pango]\ncommand=echo \"<span color='#ff0000'>bt</span> &amp; more\"\nmarkup=pango\n\
             [persist]\ncommand=echo first; sleep 30\ninterval=persist\n\
             [silent]\ncommand=sleep 30\n\
             [empty]\ncommand=true\n",
        )
        .unwrap();
        let start = std::time::Instant::now();
        let blocks = read_bar(&dir, Duration::from_millis(800));
        assert!(start.elapsed() < Duration::from_secs(5));
        assert_eq!(
            blocks,
            vec![
                BarBlock {
                    text: "hello".into(),
                    color: None
                },
                BarBlock {
                    text: "up".into(),
                    color: Some(Rgb(0, 255, 0))
                },
                BarBlock {
                    text: "bt & more".into(),
                    color: Some(Rgb(255, 0, 0))
                },
                BarBlock {
                    text: "first".into(),
                    color: None
                },
            ]
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
