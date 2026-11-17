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
use crate::font_tree::Node;
use crate::keymap::Action;
use crate::palette::Palette;

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

    /// All presets, in file order (either layout of fonts.json).
    pub fn load_all(&self) -> Result<Map<String, Value>> {
        let text = std::fs::read_to_string(self.path())
            .with_context(|| format!("reading {}", self.path().display()))?;
        let v: Value = serde_json::from_str(&text)?;
        crate::store::data_map(v, "presets").ok_or_else(|| anyhow!("fonts.json has no presets"))
    }

    /// Rewrites fonts.json as version 2 if it's still version 1.
    pub fn migrate(&self) -> Result<bool> {
        crate::store::migrate_data_file(&self.path(), "presets")
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
        crate::store::write_json_file(&self.path(), &crate::store::data_file("presets", all))
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
    /// Only bitmap files (PCF/BDF): render badly in Ghostty and GTK.
    pub bitmap: bool,
    /// Made for other scripts or symbols (CJK, emoji, math, Unifont), by name.
    pub special: bool,
}

/// Name parts of fonts made for other scripts or for symbols.
const SPECIAL: [&str; 5] = ["CJK", "Emoji", "Math", "Symbol", "Unifont"];

/// Installed font families, sorted by name.
pub fn installed_fonts() -> Vec<Installed> {
    let run = |args: &[&str]| {
        Command::new("fc-list")
            .args(args)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    };
    // family → (name, monospaced, has a scalable file)
    let mut fams: BTreeMap<String, (String, bool, bool)> = BTreeMap::new();
    for line in run(&["--format", "%{family[0]}\t%{spacing}\t%{fontformat}\n"]).lines() {
        let mut parts = line.split('\t');
        let family = parts.next().unwrap_or("").trim();
        let spacing = parts.next().unwrap_or("").trim();
        let format = parts.next().unwrap_or("").trim();
        if family.is_empty() {
            continue;
        }
        let e = fams
            .entry(family.to_lowercase())
            .or_insert((family.to_string(), false, false));
        e.1 |= !spacing.is_empty();
        e.2 |= !matches!(format, "PCF" | "BDF");
    }
    let nerd: HashSet<String> = run(&[":charset=f115", "--format", "%{family[0]}\n"])
        .lines()
        .map(|l| l.trim().to_lowercase())
        .collect();
    fams.into_iter()
        .map(|(lower, (family, mono, scalable))| Installed {
            nerd: nerd.contains(&lower),
            special: SPECIAL.iter().any(|s| family.contains(s)),
            bitmap: !scalable,
            family,
            mono,
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spacing {
    #[default]
    Any,
    Mono,
    Propo,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NerdKind {
    #[default]
    Any,
    Nerd,
    NerdMono,
    NerdPropo,
    NotNerd,
}

/// The font list's filter chips. A font has to pass all of them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Filters {
    pub spacing: Spacing,
    pub nerd: NerdKind,
    /// Bitmap fonts are hidden unless this is on.
    pub show_bitmap: bool,
    /// CJK / emoji / math fonts are hidden unless this is on.
    pub show_special: bool,
}

/// Chip key and label, in display order.
pub const CHIPS: [(char, &str); 8] = [
    ('m', "mono"),
    ('p', "propo"),
    ('n', "NF"),
    ('M', "NF Mono"),
    ('P', "NF Propo"),
    ('x', "no NF"),
    ('b', "bitmap"),
    ('c', "CJK·emoji·math"),
];

impl Filters {
    pub fn allows(&self, f: &Installed) -> bool {
        let spacing = match self.spacing {
            Spacing::Any => true,
            Spacing::Mono => f.mono,
            Spacing::Propo => !f.mono,
        };
        let nerd = match self.nerd {
            NerdKind::Any => true,
            NerdKind::Nerd => f.nerd,
            NerdKind::NerdMono => f.family.ends_with("Nerd Font Mono"),
            NerdKind::NerdPropo => f.family.ends_with("Nerd Font Propo"),
            NerdKind::NotNerd => !f.nerd,
        };
        spacing && nerd && (self.show_bitmap || !f.bitmap) && (self.show_special || !f.special)
    }

    pub fn is_on(&self, chip: char) -> bool {
        match chip {
            'm' => self.spacing == Spacing::Mono,
            'p' => self.spacing == Spacing::Propo,
            'n' => self.nerd == NerdKind::Nerd,
            'M' => self.nerd == NerdKind::NerdMono,
            'P' => self.nerd == NerdKind::NerdPropo,
            'x' => self.nerd == NerdKind::NotNerd,
            'b' => self.show_bitmap,
            'c' => self.show_special,
            _ => false,
        }
    }

    /// Turns a chip on or off. Chips in the same row of choices (mono/propo,
    /// the Nerd Font kinds) replace each other.
    pub fn toggle(&mut self, chip: char) -> bool {
        let flip_spacing =
            |cur: Spacing, want: Spacing| if cur == want { Spacing::Any } else { want };
        let flip_nerd =
            |cur: NerdKind, want: NerdKind| if cur == want { NerdKind::Any } else { want };
        match chip {
            'm' => self.spacing = flip_spacing(self.spacing, Spacing::Mono),
            'p' => self.spacing = flip_spacing(self.spacing, Spacing::Propo),
            'n' => self.nerd = flip_nerd(self.nerd, NerdKind::Nerd),
            'M' => self.nerd = flip_nerd(self.nerd, NerdKind::NerdMono),
            'P' => self.nerd = flip_nerd(self.nerd, NerdKind::NerdPropo),
            'x' => self.nerd = flip_nerd(self.nerd, NerdKind::NotNerd),
            'b' => self.show_bitmap = !self.show_bitmap,
            'c' => self.show_special = !self.show_special,
            _ => return false,
        }
        true
    }
}

/// A visible row of the font list.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    /// A design or variant group; `count` is how many fonts it shows.
    Group {
        depth: usize,
        path: String,
        name: String,
        expanded: bool,
        count: usize,
    },
    Font {
        depth: usize,
        index: usize,
    },
}

impl Row {
    pub fn depth(&self) -> usize {
        match self {
            Row::Group { depth, .. } | Row::Font { depth, .. } => *depth,
        }
    }
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
/// `name` is the palette being edited (shown as the selected rofi row).
pub fn sample_markup(fonts: &[Font; 3], sizes: [u32; 3], p: &Palette, name: &str) -> String {
    let c = |k: &str| p.get(k).hex();
    let span = |k: &str, t: &str| format!("<span foreground=\"{}\">{}</span>", c(k), esc(t));
    let font = |role: usize| format!("{} {}", esc(&fonts[role].family), sizes[role]);
    let mut m = String::new();
    // mono: code, as nvim shows it
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
    m += "\n";
    m += &format!("<span font=\"{}\" foreground=\"{}\">", font(1), c("fg0"));
    m += &format!("Theme: {}\n", span("grey1", "type to search"));
    m += &format!(
        "<span background=\"{}\" foreground=\"{}\"> {}  </span>  Gruvbox Dark   Tokyo Night\n",
        c("yellow"),
        c("bg0"),
        esc(name)
    );
    m += "</span>\n";
    // bar: workspaces and blocks on the bar background
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
    /// ³ the sample (nothing to type; focusable so it can be resized)
    Sample,
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
    pub tree: Vec<Node>,
    /// Open groups, by path ("JetBrains Mono", "JetBrains Mono/Nerd Font").
    pub expanded: HashSet<String>,
    pub search: String,
    pub filters: Filters,
    /// Opening the list unfolds the groups of the role's current font (a setting).
    pub unfold_current: bool,
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
            tree: Vec::new(),
            expanded: HashSet::new(),
            search: String::new(),
            filters: Filters::default(),
            unfold_current: true,
            list_sel: 0,
            list_scroll: 0,
            dpi: 96,
            sample: SampleRenderer::new(draft_dir, dirty),
            load_error,
        }
    }

    /// Reads the installed fonts and screen DPI (the first time the tab opens).
    pub fn refresh_installed(&mut self) {
        let fonts = installed_fonts();
        self.set_installed(fonts);
        self.dpi = screen_dpi();
    }

    pub fn set_installed(&mut self, fonts: Vec<Installed>) {
        self.tree = crate::font_tree::build(&fonts);
        self.installed = fonts;
    }

    pub fn dirty(&self) -> bool {
        self.is_new || self.preset != self.saved
    }

    /// Size actually used on this machine (base + host offset).
    pub fn effective_size(&self, role: usize, base: u32) -> u32 {
        (base as i32 + self.offsets[role]).max(1) as u32
    }

    /// The rows the list shows: the grouped tree (open groups expanded) when
    /// not searching, a flat list of matching fonts while searching. Groups
    /// with no font passing the filters are left out.
    pub fn rows(&self) -> Vec<Row> {
        let allowed = |i: usize| self.filters.allows(&self.installed[i]);
        if !self.search.is_empty() {
            let q = self.search.to_lowercase();
            return (0..self.installed.len())
                .filter(|&i| allowed(i) && self.installed[i].family.to_lowercase().contains(&q))
                .map(|index| Row::Font { depth: 0, index })
                .collect();
        }
        fn walk(t: &FontsTab, nodes: &[Node], depth: usize, parent: &str, out: &mut Vec<Row>) {
            for n in nodes {
                match n {
                    Node::Leaf(i) => {
                        if t.filters.allows(&t.installed[*i]) {
                            out.push(Row::Font { depth, index: *i });
                        }
                    }
                    Node::Group { name, children } => {
                        let count = n
                            .leaves()
                            .into_iter()
                            .filter(|&i| t.filters.allows(&t.installed[i]))
                            .count();
                        if count == 0 {
                            continue;
                        }
                        let path = if parent.is_empty() {
                            name.clone()
                        } else {
                            format!("{parent}/{name}")
                        };
                        let expanded = t.expanded.contains(&path);
                        out.push(Row::Group {
                            depth,
                            path: path.clone(),
                            name: name.clone(),
                            expanded,
                            count,
                        });
                        if expanded {
                            walk(t, children, depth + 1, &path, out);
                        }
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(self, &self.tree, 0, "", &mut out);
        out
    }

    /// The font on the highlighted row, if it is a font.
    pub fn highlighted(&self) -> Option<usize> {
        match self.rows().get(self.list_sel) {
            Some(Row::Font { index, .. }) => Some(*index),
            _ => None,
        }
    }

    /// The fonts the sample shows: while browsing the list, the highlighted
    /// font stands in for the selected role.
    pub fn shown_fonts(&self) -> [Font; 3] {
        let mut fonts = self.preset.fonts.clone();
        if matches!(self.focus, FontFocus::List | FontFocus::Search)
            && let Some(i) = self.highlighted()
        {
            fonts[self.slot].family = self.installed[i].family.clone();
        }
        fonts
    }

    /// Group paths leading to a font in the tree.
    fn path_to(&self, index: usize) -> Vec<String> {
        fn find(nodes: &[Node], index: usize, parent: &str, out: &mut Vec<String>) -> bool {
            for n in nodes {
                match n {
                    Node::Leaf(i) if *i == index => return true,
                    Node::Leaf(_) => {}
                    Node::Group { name, children } => {
                        let path = if parent.is_empty() {
                            name.clone()
                        } else {
                            format!("{parent}/{name}")
                        };
                        out.push(path.clone());
                        if find(children, index, &path, out) {
                            return true;
                        }
                        out.pop();
                    }
                }
            }
            false
        }
        let mut out = Vec::new();
        find(&self.tree, index, "", &mut out);
        out
    }

    fn clamp_sel(&mut self) {
        self.list_sel = self.list_sel.min(self.rows().len().saturating_sub(1));
    }

    /// Opens or closes the group on the highlighted row.
    fn set_expanded(&mut self, open: bool) {
        if let Some(Row::Group { path, .. }) = self.rows().get(self.list_sel) {
            if open {
                self.expanded.insert(path.clone());
            } else {
                self.expanded.remove(path);
            }
        }
    }

    /// Enter / click on the highlighted row: a group opens or closes, a font
    /// is used for the selected role.
    fn activate(&mut self) {
        match self.rows().get(self.list_sel) {
            Some(Row::Group { expanded, .. }) => {
                let open = !expanded;
                self.set_expanded(open);
            }
            Some(Row::Font { .. }) => self.pick(),
            None => {}
        }
    }

    /// Left / h: close the highlighted group, or go up to the group a font is in.
    fn go_up(&mut self) {
        let rows = self.rows();
        match rows.get(self.list_sel) {
            Some(Row::Group { expanded: true, .. }) => self.set_expanded(false),
            Some(r) if r.depth() > 0 => {
                let d = r.depth();
                if let Some(i) = (0..self.list_sel).rev().find(|&i| rows[i].depth() < d) {
                    self.list_sel = i;
                }
            }
            _ => {}
        }
    }

    /// Mouse: a click on one of the three roles.
    pub fn click_role(&mut self, role: usize) {
        if role < 3 {
            self.slot = role;
            self.focus = FontFocus::Slots;
        }
    }

    /// Mouse: a click on a filter chip.
    pub fn click_chip(&mut self, c: char) {
        self.filters.toggle(c);
        self.focus = FontFocus::List;
        self.clamp_sel();
    }

    /// Mouse: a click on a list row; a second click on the highlighted row
    /// opens the group or uses the font.
    pub fn click_row(&mut self, i: usize) {
        if i >= self.rows().len() {
            return;
        }
        if i == self.list_sel && self.focus == FontFocus::List {
            self.activate();
        } else {
            self.list_sel = i;
            self.focus = FontFocus::List;
        }
    }

    pub fn scroll(&mut self, d: i32) {
        let n = self.rows().len();
        self.list_sel = (self.list_sel as i32 + d).clamp(0, n.saturating_sub(1) as i32) as usize;
        self.focus = FontFocus::List;
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
    /// Opens the groups the role's current font is in and highlights it.
    pub fn open_list(&mut self) {
        self.search.clear();
        let family = self.preset.fonts[self.slot].family.to_lowercase();
        if let Some(index) = self
            .installed
            .iter()
            .position(|f| f.family.to_lowercase() == family)
        {
            for p in self
                .path_to(index)
                .into_iter()
                .filter(|_| self.unfold_current)
            {
                self.expanded.insert(p);
            }
            if let Some(i) = self.rows().iter().position(|r| {
                *r == Row::Font {
                    depth: r.depth(),
                    index,
                }
            }) {
                self.list_sel = i;
            }
        }
        self.clamp_sel();
        self.focus = FontFocus::List;
    }

    fn pick(&mut self) {
        if let Some(i) = self.highlighted() {
            self.preset.fonts[self.slot].family = self.installed[i].family.clone();
            self.focus = FontFocus::Slots;
        }
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

    /// Typing into the search box (the keymap doesn't apply while typing).
    pub fn on_search_key(&mut self, k: KeyEvent) {
        match k.code {
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
        }
    }

    /// Runs an action in the roles or the font list; returns false if it
    /// means nothing there.
    pub fn act(&mut self, a: Action) -> bool {
        let n = self.rows().len();
        let last = n.saturating_sub(1);
        match self.focus {
            FontFocus::Search | FontFocus::Sample => return false,
            FontFocus::List => match a {
                Action::Down => self.list_sel = (self.list_sel + 1).min(last),
                Action::Up => self.list_sel = self.list_sel.saturating_sub(1),
                Action::Top => self.list_sel = 0,
                Action::Bottom => self.list_sel = last,
                Action::PageDown => self.list_sel = (self.list_sel + 10).min(last),
                Action::PageUp => self.list_sel = self.list_sel.saturating_sub(10),
                Action::Search => self.focus = FontFocus::Search,
                Action::Right => match self.rows().get(self.list_sel) {
                    Some(Row::Group {
                        expanded: false, ..
                    }) => self.set_expanded(true),
                    // already open: step into it
                    Some(Row::Group { expanded: true, .. }) => {
                        self.list_sel = (self.list_sel + 1).min(last)
                    }
                    _ => {}
                },
                Action::Left => self.go_up(),
                Action::Confirm => self.activate(),
                Action::Back => self.focus = FontFocus::Slots,
                Action::Filter(c) => {
                    self.filters.toggle(c);
                    self.clamp_sel();
                }
                _ => return false,
            },
            FontFocus::Slots => match a {
                Action::Down => self.slot = (self.slot + 1).min(2),
                Action::Up => self.slot = self.slot.saturating_sub(1),
                Action::Right | Action::SizeUp => self.resize(1),
                Action::Left | Action::SizeDown => self.resize(-1),
                Action::RightBig => self.resize(4),
                Action::LeftBig => self.resize(-4),
                Action::Confirm | Action::PickFont => self.open_list(),
                _ => return false,
            },
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::to_jq_string;

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
        // either layout: rebuilding every preset gives back the same file
        let v: Value = serde_json::from_str(&text).unwrap();
        let versioned = v.get("version").is_some();
        let all = crate::store::data_map(v, "presets").unwrap();
        let mut rebuilt = Map::new();
        for (name, v) in &all {
            let p = Preset::from_json(v.as_object().unwrap()).unwrap();
            rebuilt.insert(name.clone(), Value::Object(p.to_json()));
        }
        let file = if versioned {
            crate::store::data_file("presets", rebuilt)
        } else {
            Value::Object(rebuilt)
        };
        assert_eq!(to_jq_string(&file), text);
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
            markup: sample_markup(&fonts, [14, 11, 11], &p, "Test & <palette>"),
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

    fn font(family: &str, mono: bool, nerd: bool) -> Installed {
        Installed {
            family: family.into(),
            mono,
            nerd,
            bitmap: family == "Fixed",
            special: family.contains("CJK"),
        }
    }

    fn tab_with(fonts: Vec<Installed>) -> FontsTab {
        let dir = std::env::temp_dir();
        let mut t = FontsTab::new(
            &dir.join("nonexistent"),
            &dir,
            Arc::new(AtomicBool::new(false)),
        );
        t.set_installed(fonts);
        t.preset = Preset::default_preset();
        t
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(
            KeyCode::Char(c),
            ratatui::crossterm::event::KeyModifiers::NONE,
        )
    }

    fn special(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE)
    }

    /// A key as the app routes it: typing while searching, otherwise the
    /// Vim keymap's action for the font list or roles.
    fn press(t: &mut FontsTab, k: KeyEvent) {
        use crate::keymap::{Context, Key, Keymap, Lookup};
        if t.focus == FontFocus::Search {
            return t.on_search_key(k);
        }
        let pane = if t.focus == FontFocus::Slots {
            Context::FontRoles
        } else {
            Context::FontList
        };
        if let Lookup::Run(a) = Keymap::vim().lookup(
            &[pane, Context::Fonts, Context::Global],
            &[Key::from_event(k)],
        ) {
            t.act(a);
        }
    }

    fn labels(t: &FontsTab) -> Vec<String> {
        t.rows()
            .iter()
            .map(|r| match r {
                Row::Group {
                    depth,
                    name,
                    expanded,
                    count,
                    ..
                } => {
                    format!(
                        "{}{}{name} {count}",
                        "  ".repeat(*depth),
                        if *expanded { "▾" } else { "▸" }
                    )
                }
                Row::Font { depth, index } => {
                    format!("{}{}", "  ".repeat(*depth), t.installed[*index].family)
                }
            })
            .collect()
    }

    #[test]
    fn grouped_list_opens_filters_searches_and_picks() {
        let mut t = tab_with(vec![
            font("DejaVu Sans", false, false),
            font("DejaVu Sans Mono", true, false),
            font("Fixed", true, false),
            font("JetBrainsMono Nerd Font", true, true),
            font("JetBrainsMono Nerd Font Mono", true, true),
            font("JetBrainsMono Nerd Font Propo", false, true),
            font("Noto Sans CJK JP", false, false),
            font("Noto Sans CJK KR", false, false),
        ]);
        // bitmap and CJK fonts are hidden at first; groups start closed
        assert_eq!(labels(&t), ["▸DejaVu 2", "▸JetBrains Mono 3"]);
        // opening the list shows where the role's font is
        t.open_list();
        assert_eq!(
            labels(&t),
            [
                "▸DejaVu 2",
                "▾JetBrains Mono 3",
                "  JetBrainsMono Nerd Font",
                "  JetBrainsMono Nerd Font Mono",
                "  JetBrainsMono Nerd Font Propo",
            ]
        );
        assert_eq!(t.list_sel, 2);
        // browsing previews the highlighted font without setting it
        press(&mut t, key('j'));
        assert_eq!(t.shown_fonts()[0].family, "JetBrainsMono Nerd Font Mono");
        assert_eq!(t.preset.fonts[0].family, "JetBrainsMono Nerd Font");
        // h goes up to the group, again closes it
        press(&mut t, key('h'));
        assert_eq!(t.list_sel, 1);
        press(&mut t, key('h'));
        assert_eq!(labels(&t), ["▸DejaVu 2", "▸JetBrains Mono 3"]);
        // filters: NF Mono only, then show the hidden kinds
        press(&mut t, key('M'));
        assert_eq!(labels(&t), ["▸JetBrains Mono 1"]);
        press(&mut t, key('M'));
        press(&mut t, key('b'));
        press(&mut t, key('c'));
        press(&mut t, key('p'));
        // (only Noto Sans CJK fonts here, so that is the shared name)
        assert_eq!(
            labels(&t),
            ["▸DejaVu 1", "▸JetBrains Mono 1", "▸Noto Sans CJK 2"]
        );
        press(&mut t, key('p'));
        assert!(labels(&t).contains(&"Fixed".to_string()));
        // l opens a group, l again steps in, Enter uses the font
        t.list_sel = 0;
        press(&mut t, key('l'));
        press(&mut t, key('l'));
        press(&mut t, special(KeyCode::Enter));
        assert_eq!(t.preset.fonts[t.slot].family, "DejaVu Sans");
        assert_eq!(t.focus, FontFocus::Slots);
        // searching shows a flat list of matches
        t.open_list();
        press(&mut t, key('/'));
        for c in "font mono".chars() {
            press(&mut t, key(c));
        }
        assert_eq!(labels(&t), ["JetBrainsMono Nerd Font Mono"]);
        press(&mut t, special(KeyCode::Enter));
        press(&mut t, special(KeyCode::Enter));
        assert_eq!(
            t.preset.fonts[t.slot].family,
            "JetBrainsMono Nerd Font Mono"
        );
        assert!(t.dirty());
    }

    #[test]
    fn warnings_for_role_fonts() {
        let mut t = tab_with(vec![
            font("Beta Sans", false, false),
            font("Gamma Mono NF", true, true),
        ]);
        t.preset.fonts[0].family = "Beta Sans".into();
        assert_eq!(
            t.warnings(0).len(),
            2,
            "mono role: not monospaced, no icons"
        );
        t.preset.fonts[2].family = "Gamma Mono NF".into();
        assert!(t.warnings(2).is_empty());
        t.preset.fonts[2].family = "Nope".into();
        assert!(t.warnings(2)[0].contains("not installed"));
    }

    #[test]
    fn mouse_clicks_select_open_and_toggle() {
        let mut t = tab_with(vec![
            font("JetBrainsMono Nerd Font", true, true),
            font("JetBrainsMono Nerd Font Mono", true, true),
            font("Zeta", false, false),
        ]);
        t.click_role(2); // bar
        assert_eq!((t.slot, t.focus), (2, FontFocus::Slots));
        t.click_row(0); // the group: select it
        assert_eq!((t.list_sel, t.focus), (0, FontFocus::List));
        t.click_row(0); // again: open it
        assert_eq!(t.rows().len(), 4);
        t.click_row(2); // second font: select, then use it
        t.click_row(2);
        assert_eq!(t.preset.fonts[2].family, "JetBrainsMono Nerd Font Mono");
        t.click_chip('m');
        assert!(t.filters.is_on('m'));
        assert!(!labels(&t).contains(&"Zeta".to_string()));
        t.click_row(99); // past the end: nothing
    }
}
