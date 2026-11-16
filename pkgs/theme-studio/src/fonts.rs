//! Fonts tab: font presets (configs/fonts.json, applied by font-switch.sh),
//! the installed fonts (fontconfig), and the sample image (ImageMagick +
//! Pango, shown with the kitty graphics protocol).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Map, Value, json};

use crate::color::Rgb;
use crate::palette::Palette;
use crate::store::to_jq_string;

/// The three font roles, in fonts.json and on screen.
pub const ROLES: [&str; 3] = ["mono", "ui", "bar"];
/// What each role is used for (font-switch.sh).
pub const ROLE_USE: [&str; 3] = [
    "terminal, nvim, lock screen",
    "rofi, GTK apps",
    "i3 window titles and bar",
];

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: String,
    pub size: u32,
}

/// A font preset: mono, ui and bar fonts.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub fonts: [Font; 3],
    /// Other keys in the preset (kept as they are when saving).
    extra: Vec<(String, Value)>,
}

impl Preset {
    /// Reads a preset; a missing "bar" falls back to "ui", like font-switch.sh.
    pub fn from_json(obj: &Map<String, Value>) -> Result<Preset> {
        let font = |role: &str| -> Option<Font> {
            let f = obj.get(role)?;
            Some(Font {
                family: f.get("family")?.as_str()?.to_string(),
                size: f.get("size")?.as_u64()? as u32,
            })
        };
        let mono = font("mono").ok_or_else(|| anyhow!("no mono font"))?;
        let ui = font("ui").ok_or_else(|| anyhow!("no ui font"))?;
        let bar = font("bar").unwrap_or_else(|| ui.clone());
        let extra = obj
            .iter()
            .filter(|(k, _)| !ROLES.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(Preset {
            fonts: [mono, ui, bar],
            extra,
        })
    }

    pub fn to_json(&self) -> Map<String, Value> {
        let mut m = Map::new();
        for (role, f) in ROLES.iter().zip(&self.fonts) {
            m.insert(
                role.to_string(),
                json!({ "family": f.family, "size": f.size }),
            );
        }
        for (k, v) in &self.extra {
            m.insert(k.clone(), v.clone());
        }
        m
    }

    /// What a new preset starts from.
    pub fn default_preset() -> Preset {
        let f = |family: &str, size| Font {
            family: family.to_string(),
            size,
        };
        Preset {
            fonts: [
                f("JetBrainsMono Nerd Font", 14),
                f("JetBrainsMono Nerd Font", 11),
                f("JetBrainsMono Nerd Font", 11),
            ],
            extra: Vec::new(),
        }
    }
}

/// configs/fonts.json and the files around it.
pub struct FontStore {
    pub dir: PathBuf,
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

impl FontStore {
    fn path(&self) -> PathBuf {
        self.dir.join("fonts.json")
    }

    pub fn load_all(&self) -> Result<Map<String, Value>> {
        let text = std::fs::read_to_string(self.path())
            .with_context(|| format!("reading {}", self.path().display()))?;
        match serde_json::from_str(&text)? {
            Value::Object(m) => Ok(m),
            _ => bail!("fonts.json is not a JSON object"),
        }
    }

    pub fn load(&self, name: &str) -> Result<Preset> {
        let all = self.load_all()?;
        let obj = all
            .get(name)
            .and_then(Value::as_object)
            .ok_or_else(|| anyhow!("no font preset named '{name}'"))?;
        Preset::from_json(obj).with_context(|| format!("font preset '{name}'"))
    }

    /// Writes one preset (re-reading the file first), keeping jq's formatting.
    pub fn save(&self, name: &str, preset: &Preset) -> Result<()> {
        let mut all = self.load_all().unwrap_or_default();
        all.insert(name.to_string(), Value::Object(preset.to_json()));
        let tmp = self.path().with_extension("json.tmp");
        std::fs::write(&tmp, to_jq_string(&Value::Object(all)))?;
        std::fs::rename(&tmp, self.path())?;
        Ok(())
    }

    /// Runs font-switch.sh to make a saved preset the system fonts.
    pub fn apply(&self, name: &str) -> Result<()> {
        let out = Command::new("bash")
            .arg(self.dir.join("font-switch.sh"))
            .arg(name)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output()
            .context("running font-switch.sh")?;
        if !out.status.success() {
            bail!(
                "font-switch.sh: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    }

    /// The preset font-switch.sh last applied.
    pub fn active_name(&self) -> Option<String> {
        let s = std::fs::read_to_string(home().join(".config/fonts/current")).ok()?;
        Some(s.trim().to_string()).filter(|s| !s.is_empty())
    }

    /// This machine's size offsets (~/.config/host/fonts.json); bar falls
    /// back to the ui offset, like font-switch.sh.
    pub fn offsets() -> [i32; 3] {
        let v: Value = std::fs::read_to_string(home().join(".config/host/fonts.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or(Value::Null);
        let get = |k: &str| v.get(k).and_then(Value::as_i64).map(|n| n as i32);
        let ui = get("ui").unwrap_or(0);
        [get("mono").unwrap_or(0), ui, get("bar").unwrap_or(ui)]
    }
}

/// An installed font family, as fontconfig reports it.
#[derive(Clone, Debug, PartialEq)]
pub struct Installed {
    pub family: String,
    /// Monospaced (fontconfig reports a spacing, as font-switch.sh checks).
    pub mono: bool,
    /// Has Nerd Font icons (checked with one icon, U+F115).
    pub nerd: bool,
}

/// Installed font families, sorted by name.
pub fn installed_fonts() -> Vec<Installed> {
    let run = |args: &[&str]| {
        Command::new("fc-list")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    let mut fams: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for line in run(&["--format", "%{family[0]}\t%{spacing}\n"]).lines() {
        let (family, spacing) = line.split_once('\t').unwrap_or((line, ""));
        let family = family.trim();
        if family.is_empty() {
            continue;
        }
        let e = fams
            .entry(family.to_lowercase())
            .or_insert((family.to_string(), false));
        e.1 |= !spacing.trim().is_empty();
    }
    let nerd: HashSet<String> = run(&[":charset=f115", "--format", "%{family[0]}\n"])
        .lines()
        .map(|l| l.trim().to_lowercase())
        .collect();
    fams.into_iter()
        .map(|(lower, (family, mono))| Installed {
            nerd: nerd.contains(&lower),
            family,
            mono,
        })
        .collect()
}

/// The X server's DPI (Xft.dpi), so the sample is the size the fonts really are.
pub fn screen_dpi() -> u32 {
    Command::new("xrdb")
        .arg("-query")
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .find_map(|l| l.strip_prefix("Xft.dpi:").map(|v| v.trim().to_string()))
        })
        .and_then(|v| v.parse::<f64>().ok())
        .map(|d| d.round() as u32)
        .unwrap_or(96)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Pango markup for the sample: code in the mono font, rofi rows in the ui
/// font, the bar in the bar font, all in the palette's colours, at the sizes
/// used on this machine.
pub fn sample_markup(fonts: &[Font; 3], sizes: [u32; 3], p: &Palette) -> String {
    let c = |k: &str| p.get(k).hex();
    let span = |k: &str, t: &str| format!("<span foreground=\"{}\">{}</span>", c(k), esc(t));
    let label = |role: usize| {
        format!(
            "<span font=\"{} {}\" foreground=\"{}\">{} · {} {}</span>",
            esc(&fonts[0].family),
            (sizes[0] as f64 * 0.75).round().max(6.0),
            c("grey1"),
            ROLES[role],
            esc(&fonts[role].family),
            sizes[role]
        )
    };
    let font = |role: usize| format!("{} {}", esc(&fonts[role].family), sizes[role]);
    let mut m = String::new();
    // mono: code, as nvim shows it
    m += &label(0);
    m += "\n";
    m += &format!("<span font=\"{}\" foreground=\"{}\">", font(0), c("fg0"));
    m += &format!(
        "{} {}{}{}{}\n",
        span("purple", "local function"),
        span("blue", "apply"),
        span("grey2", "("),
        span("red", "path, opts"),
        span("grey2", ")")
    );
    m += &format!(
        "  {} {} {} {} {}\n",
        span("purple", "if"),
        span("red", "count"),
        span("fg0", "=="),
        span("orange", "0x2A"),
        span("purple", "then")
    );
    m += &format!(
        "    {}{}{}{}\n",
        span("red", "vim"),
        span("grey2", ".notify("),
        span("green", "\"theme applied 0O 1lI\""),
        span("grey2", ")")
    );
    m += &format!(
        "  {} {}\n",
        span("purple", "end"),
        span("grey1", "-- icons: \u{e0a0} \u{f07b} \u{f1d3} \u{f00af} ⇡2")
    );
    m += "</span>\n";
    // ui: the rofi theme menu
    m += &label(1);
    m += "\n";
    m += &format!("<span font=\"{}\" foreground=\"{}\">", font(1), c("fg0"));
    m += &format!("Theme: {}\n", span("grey1", "type to search"));
    m += &format!(
        "<span background=\"{}\" foreground=\"{}\"> novmbr  </span>  Gruvbox Dark   Tokyo Night\n",
        c("yellow"),
        c("bg0")
    );
    m += "</span>\n";
    // bar: workspaces and blocks on the bar background
    m += &label(2);
    m += "\n";
    m += &format!(
        "<span font=\"{}\" background=\"{}\" foreground=\"{}\"><span background=\"{}\" foreground=\"{}\"> 1 </span> 2  3        vol 73% {} 󰂯 HelixBrigitte {} 18:41 </span>",
        font(2),
        c("bg0"),
        c("fg0"),
        c("yellow"),
        c("bg0"),
        span("bg3", "│"),
        span("bg3", "│"),
    );
    m
}

/// A rendered sample image, or why it failed.
type SampleResult = std::result::Result<PathBuf, String>;

struct Job {
    id: u64,
    markup: String,
    bg: Rgb,
    dpi: u32,
    out: PathBuf,
}

/// Renders samples in the background (newest request wins).
pub struct SampleRenderer {
    tx: Sender<Job>,
    done: Arc<Mutex<Option<(u64, SampleResult)>>>,
    next: u64,
    pub last_markup: Option<String>,
    pub shown: u64,
    dir: PathBuf,
}

impl SampleRenderer {
    pub fn new(dir: &Path, dirty: Arc<AtomicBool>) -> SampleRenderer {
        let (tx, rx): (Sender<Job>, Receiver<Job>) = channel();
        let done = Arc::new(Mutex::new(None));
        let d = done.clone();
        std::thread::spawn(move || {
            while let Ok(mut job) = rx.recv() {
                // skip to the newest request
                while let Ok(newer) = rx.try_recv() {
                    job = newer;
                }
                let result = render_sample(&job);
                *d.lock().unwrap() = Some((job.id, result));
                dirty.store(true, Ordering::Relaxed);
            }
        });
        SampleRenderer {
            tx,
            done,
            next: 0,
            last_markup: None,
            shown: 0,
            dir: dir.to_path_buf(),
        }
    }

    /// Asks for a new sample if the markup changed.
    pub fn request(&mut self, markup: String, bg: Rgb, dpi: u32) {
        if self.last_markup.as_deref() == Some(markup.as_str()) {
            return;
        }
        self.next += 1;
        let out = self.dir.join(format!("sample-{}.png", self.next % 2));
        let _ = self.tx.send(Job {
            id: self.next,
            markup: markup.clone(),
            bg,
            dpi,
            out,
        });
        self.last_markup = Some(markup);
    }

    /// A finished sample newer than the one shown.
    pub fn take(&mut self) -> Option<std::result::Result<PathBuf, String>> {
        let mut done = self.done.lock().unwrap();
        match done.take() {
            Some((id, r)) if id > self.shown => {
                self.shown = id;
                Some(r)
            }
            other => {
                *done = other;
                None
            }
        }
    }
}

fn render_sample(job: &Job) -> std::result::Result<PathBuf, String> {
    let markup_file = job.out.with_extension("markup");
    std::fs::write(&markup_file, &job.markup).map_err(|e| e.to_string())?;
    let out = Command::new("magick")
        .args([
            "-density",
            &job.dpi.to_string(),
            "-background",
            &job.bg.hex(),
        ])
        .arg(format!("pango:@{}", markup_file.display()))
        .args(["-bordercolor", &job.bg.hex(), "-border", "12"])
        .arg(&job.out)
        .output()
        .map_err(|e| format!("ImageMagick: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ImageMagick: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(job.out.clone())
}

/// Which part of the Fonts tab has the keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FontFocus {
    /// ¹ the three role slots
    Slots,
    /// ² the installed fonts list
    List,
    /// typing into the list's search box
    Search,
}

/// State of the Fonts tab.
pub struct FontsTab {
    pub store: FontStore,
    pub name: String,
    pub is_new: bool,
    pub preset: Preset,
    pub saved: Preset,
    pub active: Option<String>,
    pub offsets: [i32; 3],
    pub slot: usize,
    pub focus: FontFocus,
    pub installed: Vec<Installed>,
    pub search: String,
    pub mono_only: bool,
    pub list_sel: usize,
    pub list_scroll: usize,
    pub dpi: u32,
    pub sample: SampleRenderer,
    pub load_error: Option<String>,
}

impl FontsTab {
    pub fn new(configs: &Path, draft_dir: &Path, dirty: Arc<AtomicBool>) -> FontsTab {
        let store = FontStore {
            dir: configs.to_path_buf(),
        };
        let active = store.active_name();
        let (name, preset, is_new, load_error) = match active.as_deref().map(|n| (n, store.load(n)))
        {
            Some((n, Ok(p))) => (n.to_string(), p, false, None),
            Some((n, Err(e))) => (
                n.to_string(),
                Preset::default_preset(),
                true,
                Some(format!("{e:#}")),
            ),
            None => ("untitled".to_string(), Preset::default_preset(), true, None),
        };
        FontsTab {
            store,
            name,
            is_new,
            saved: preset.clone(),
            preset,
            active,
            offsets: FontStore::offsets(),
            slot: 0,
            focus: FontFocus::Slots,
            installed: Vec::new(),
            search: String::new(),
            mono_only: false,
            list_sel: 0,
            list_scroll: 0,
            dpi: 96,
            sample: SampleRenderer::new(draft_dir, dirty),
            load_error,
        }
    }

    /// Reads the installed fonts and screen DPI (the first time the tab opens).
    pub fn refresh_installed(&mut self) {
        self.installed = installed_fonts();
        self.dpi = screen_dpi();
    }

    pub fn dirty(&self) -> bool {
        self.is_new || self.preset != self.saved
    }

    /// Size actually used on this machine (base + host offset).
    pub fn effective_size(&self, role: usize, base: u32) -> u32 {
        (base as i32 + self.offsets[role]).max(1) as u32
    }

    /// Installed fonts matching the search and filter.
    pub fn matches(&self) -> Vec<usize> {
        let q = self.search.to_lowercase();
        self.installed
            .iter()
            .enumerate()
            .filter(|(_, f)| (!self.mono_only || f.mono) && f.family.to_lowercase().contains(&q))
            .map(|(i, _)| i)
            .collect()
    }

    /// The fonts the sample shows: while browsing the list, the highlighted
    /// family stands in for the selected role.
    pub fn shown_fonts(&self) -> [Font; 3] {
        let mut fonts = self.preset.fonts.clone();
        if matches!(self.focus, FontFocus::List | FontFocus::Search)
            && let Some(&i) = self.matches().get(self.list_sel)
        {
            fonts[self.slot].family = self.installed[i].family.clone();
        }
        fonts
    }

    /// Problems with a role's font, for the slot and the status line.
    pub fn warnings(&self, role: usize) -> Vec<String> {
        let family = &self.preset.fonts[role].family;
        let Some(f) = self
            .installed
            .iter()
            .find(|f| f.family.eq_ignore_ascii_case(family))
        else {
            if self.installed.is_empty() {
                return Vec::new();
            }
            return vec![format!("\"{family}\" is not installed")];
        };
        let mut w = Vec::new();
        if role == 0 && !f.mono {
            w.push("not monospaced: terminal text won't line up".to_string());
        }
        if role != 1 && !f.nerd {
            w.push("no Nerd Font icons (another Nerd Font fills in)".to_string());
        }
        w
    }

    /// Selects the list row of the selected role's current family.
    pub fn open_list(&mut self) {
        self.search.clear();
        let family = self.preset.fonts[self.slot].family.to_lowercase();
        let m = self.matches();
        self.list_sel = m
            .iter()
            .position(|&i| self.installed[i].family.to_lowercase() == family)
            .unwrap_or(0);
        self.focus = FontFocus::List;
    }

    fn pick(&mut self) {
        if let Some(&i) = self.matches().get(self.list_sel) {
            self.preset.fonts[self.slot].family = self.installed[i].family.clone();
        }
        self.focus = FontFocus::Slots;
    }

    pub fn resize(&mut self, d: i32) {
        let f = &mut self.preset.fonts[self.slot];
        f.size = (f.size as i32 + d).clamp(4, 72) as u32;
    }

    pub fn open(&mut self, name: &str) -> Result<()> {
        let p = self.store.load(name)?;
        self.name = name.to_string();
        self.is_new = false;
        self.saved = p.clone();
        self.preset = p;
        self.focus = FontFocus::Slots;
        Ok(())
    }

    pub fn new_preset(&mut self) {
        self.name = "untitled".to_string();
        self.is_new = true;
        self.saved = self.preset.clone();
        self.focus = FontFocus::Slots;
    }

    pub fn save_as(&mut self, name: &str) -> Result<()> {
        self.store.save(name, &self.preset)?;
        self.name = name.to_string();
        self.is_new = false;
        self.saved = self.preset.clone();
        Ok(())
    }

    /// Keys for the tab's own panels; returns false for keys it doesn't use.
    pub fn on_key(&mut self, k: KeyEvent) -> bool {
        let n = self.matches().len();
        match self.focus {
            FontFocus::Search => match k.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Down => self.focus = FontFocus::List,
                KeyCode::Backspace => {
                    self.search.pop();
                    self.list_sel = 0;
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                    self.list_sel = 0;
                }
                _ => {}
            },
            FontFocus::List => match k.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list_sel = (self.list_sel + 1).min(n.saturating_sub(1))
                }
                KeyCode::Char('k') | KeyCode::Up => self.list_sel = self.list_sel.saturating_sub(1),
                KeyCode::Char('g') | KeyCode::Home => self.list_sel = 0,
                KeyCode::Char('G') | KeyCode::End => self.list_sel = n.saturating_sub(1),
                KeyCode::PageDown => self.list_sel = (self.list_sel + 10).min(n.saturating_sub(1)),
                KeyCode::PageUp => self.list_sel = self.list_sel.saturating_sub(10),
                KeyCode::Char('/') => self.focus = FontFocus::Search,
                KeyCode::Char('m') => {
                    self.mono_only = !self.mono_only;
                    self.list_sel = 0;
                }
                KeyCode::Enter => self.pick(),
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => self.focus = FontFocus::Slots,
                _ => return false,
            },
            FontFocus::Slots => match k.code {
                KeyCode::Char('j') | KeyCode::Down => self.slot = (self.slot + 1).min(2),
                KeyCode::Char('k') | KeyCode::Up => self.slot = self.slot.saturating_sub(1),
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=') => {
                    self.resize(1)
                }
                KeyCode::Char('h') | KeyCode::Left | KeyCode::Char('-') => self.resize(-1),
                KeyCode::Char('L') => self.resize(4),
                KeyCode::Char('H') => self.resize(-4),
                KeyCode::Enter | KeyCode::Char('f') => self.open_list(),
                _ => return false,
            },
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_and_bar_falls_back_to_ui() {
        let obj: Map<String, Value> = serde_json::from_str(
            r#"{"mono": {"family": "A", "size": 14}, "ui": {"family": "B", "size": 9}, "note": 1}"#,
        )
        .unwrap();
        let p = Preset::from_json(&obj).unwrap();
        assert_eq!(
            p.fonts[2],
            Font {
                family: "B".into(),
                size: 9
            }
        );
        let out = p.to_json();
        assert_eq!(
            out.keys().collect::<Vec<_>>(),
            ["mono", "ui", "bar", "note"]
        );
        assert_eq!(Preset::from_json(&out).unwrap(), p);
    }

    #[test]
    fn real_fonts_json_round_trips_byte_for_byte() {
        let dir = crate::store::Store::locate().dir;
        let Ok(text) = std::fs::read_to_string(dir.join("fonts.json")) else {
            eprintln!("skipped: no fonts.json");
            return;
        };
        let all: Map<String, Value> = serde_json::from_str(&text).unwrap();
        let mut rebuilt = Map::new();
        for (name, v) in &all {
            let p = Preset::from_json(v.as_object().unwrap()).unwrap();
            rebuilt.insert(name.clone(), Value::Object(p.to_json()));
        }
        assert_eq!(to_jq_string(&Value::Object(rebuilt)), text);
    }

    #[test]
    fn installed_fonts_know_mono_and_nerd() {
        let fonts = installed_fonts();
        if fonts.is_empty() {
            eprintln!("skipped: no fontconfig");
            return;
        }
        let find = |n: &str| fonts.iter().find(|f| f.family == n);
        if let Some(jb) = find("JetBrainsMono Nerd Font") {
            assert!(jb.mono && jb.nerd);
        }
        if let Some(dv) = find("DejaVu Sans") {
            assert!(!dv.mono && !dv.nerd);
        }
    }

    #[test]
    fn sample_renders_with_imagemagick() {
        if Command::new("magick").arg("-version").output().is_err() {
            eprintln!("skipped: no ImageMagick");
            return;
        }
        let dir = std::env::temp_dir().join(format!("theme-studio-sample-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = Palette::blank();
        let fonts = Preset::default_preset().fonts;
        let job = Job {
            id: 1,
            markup: sample_markup(&fonts, [14, 11, 11], &p),
            bg: p.get("bg0"),
            dpi: 96,
            out: dir.join("s.png"),
        };
        let path = render_sample(&job).unwrap();
        let img = image::open(&path).unwrap();
        assert!(
            img.width() > 200 && img.height() > 100,
            "{}x{}",
            img.width(),
            img.height()
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn list_preview_search_and_pick() {
        let dir = std::env::temp_dir();
        let mut t = FontsTab::new(
            &dir.join("nonexistent"),
            &dir,
            Arc::new(AtomicBool::new(false)),
        );
        t.installed = vec![
            Installed {
                family: "Alpha Mono".into(),
                mono: true,
                nerd: false,
            },
            Installed {
                family: "Beta Sans".into(),
                mono: false,
                nerd: false,
            },
            Installed {
                family: "Gamma Mono NF".into(),
                mono: true,
                nerd: true,
            },
        ];
        t.preset = Preset::default_preset();
        t.slot = 1;
        t.open_list();
        let key = |c| {
            KeyEvent::new(
                KeyCode::Char(c),
                ratatui::crossterm::event::KeyModifiers::NONE,
            )
        };
        t.on_key(key('m'));
        assert_eq!(t.matches(), vec![0, 2]);
        t.on_key(key('j'));
        assert_eq!(
            t.shown_fonts()[1].family,
            "Gamma Mono NF",
            "browsing previews the font"
        );
        assert_ne!(
            t.preset.fonts[1].family, "Gamma Mono NF",
            "but doesn't set it yet"
        );
        t.on_key(key('/'));
        for c in "alp".chars() {
            t.on_key(key(c));
        }
        assert_eq!(t.matches(), vec![0]);
        t.on_key(KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        t.on_key(KeyEvent::new(
            KeyCode::Enter,
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(t.preset.fonts[1].family, "Alpha Mono");
        assert_eq!(t.focus, FontFocus::Slots);
        assert!(t.dirty());
        // warnings: mono role needs monospace, mono/bar want Nerd Font icons
        t.preset.fonts[0].family = "Beta Sans".into();
        assert_eq!(t.warnings(0).len(), 2);
        t.preset.fonts[2].family = "Gamma Mono NF".into();
        assert!(t.warnings(2).is_empty());
        t.preset.fonts[2].family = "Nope".into();
        assert!(t.warnings(2)[0].contains("not installed"));
    }
}
