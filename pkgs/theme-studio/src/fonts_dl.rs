//! The Download fonts page (F2): Google Fonts, Nerd Fonts and Installed tabs;
//! search, category chips, and installing / removing (see download.rs).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use ratatui::crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value;

use crate::app::App;
use crate::download::{self, Catalogue, Entry, Msg, Source};
use crate::keymap::Action;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DlTab {
    Google,
    Nerd,
    Installed,
}

pub const DL_TABS: [(DlTab, &str); 3] = [
    (DlTab::Google, "Google Fonts"),
    (DlTab::Nerd, "Nerd Fonts"),
    (DlTab::Installed, "Installed"),
];

/// The Google tab's category chips: (label, Fontsource category).
pub const CATEGORIES: [(&str, &str); 6] = [
    ("All", ""),
    ("Sans", "sans-serif"),
    ("Serif", "serif"),
    ("Mono", "monospace"),
    ("Display", "display"),
    ("Handwriting", "handwriting"),
];

/// A row of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum DlRow {
    /// A catalogue entry (index into the tab's catalogue).
    Entry(usize),
    /// An entry of the installed-fonts list, by name.
    Listed(String),
}

/// Where a font stands.
#[derive(Clone, Debug, PartialEq)]
pub enum DlStatus {
    None,
    /// Downloading: bytes so far, total (0 if unknown).
    Downloading(u64, u64),
    /// In the installed-fonts list (downloaded here, or provided by Nix).
    Listed,
    /// Already on the system some other way.
    System,
}

pub struct DlPage {
    pub tab: DlTab,
    pub sel: usize,
    pub scroll: usize,
    pub search: String,
    pub searching: bool,
    pub category: usize,
    pub tabs_focused: bool,
    pub catalogue: Option<Catalogue>,
    pub loading: bool,
    pub load_error: Option<String>,
    /// Running installs: id → (family, done, total).
    pub jobs: HashMap<String, (String, u64, u64)>,
    /// The installed-fonts list as last read (None: read it again).
    pub(crate) listed: Option<serde_json::Map<String, Value>>,
    /// The preview pane (made on first use).
    pub preview: Option<crate::fonts_preview::FontPreview>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
}

impl Default for DlPage {
    fn default() -> Self {
        let (tx, rx) = download::channel_pair();
        DlPage {
            tab: DlTab::Google,
            sel: 0,
            scroll: 0,
            search: String::new(),
            searching: false,
            category: 0,
            tabs_focused: false,
            catalogue: None,
            loading: false,
            load_error: None,
            jobs: HashMap::new(),
            listed: None,
            preview: None,
            tx,
            rx,
        }
    }
}

fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .collect::<String>()
        .to_lowercase()
}

impl App {
    /// fonts-installed.json (a setting; default next to settings.json).
    pub fn installed_list_path(&self) -> PathBuf {
        let p = &self.settings.fonts.installed_list;
        if p.is_empty() {
            self.config_dir.join("fonts-installed.json")
        } else {
            download::expand(p)
        }
    }

    /// The installed-fonts list's name for an entry.
    fn list_name(e: &Entry) -> String {
        match &e.source {
            Source::Nerd { asset, .. } => asset.clone(),
            _ => e.family.clone(),
        }
    }

    /// The installed-fonts list (read once, again after changes).
    pub fn dl_listed(&mut self) -> serde_json::Map<String, Value> {
        if self.dl.listed.is_none() {
            self.dl.listed = Some(download::read_list(&self.installed_list_path()));
        }
        self.dl.listed.clone().unwrap_or_default()
    }

    pub fn dl_status(&self, e: &Entry, listed: &serde_json::Map<String, Value>) -> DlStatus {
        if let Some((_, d, t)) = self.dl.jobs.get(&e.id) {
            return DlStatus::Downloading(*d, *t);
        }
        if listed.contains_key(&Self::list_name(e)) {
            return DlStatus::Listed;
        }
        let want = match &e.source {
            Source::Nerd { asset, .. } => format!("{}nerdfont", norm(asset)),
            _ => norm(&e.family),
        };
        let on_system = self.fonts.installed.iter().any(|f| {
            let n = norm(&f.family);
            if matches!(e.source, Source::Nerd { .. }) {
                n.starts_with(&want)
            } else {
                n == want
            }
        });
        if on_system {
            DlStatus::System
        } else {
            DlStatus::None
        }
    }

    /// The tab's catalogue entries.
    pub fn dl_entries(&self) -> &[Entry] {
        match (&self.dl.catalogue, self.dl.tab) {
            (Some(c), DlTab::Google) => &c.google,
            (Some(c), DlTab::Nerd) => &c.nerd,
            _ => &[],
        }
    }

    /// The rows shown: search and category applied.
    pub fn dl_rows(&mut self) -> Vec<DlRow> {
        let q = self.dl.search.to_lowercase();
        if self.dl.tab == DlTab::Installed {
            return self
                .dl_listed()
                .keys()
                .filter(|k| k.to_lowercase().contains(&q))
                .map(|k| DlRow::Listed(k.clone()))
                .collect();
        }
        let cat = if self.dl.tab == DlTab::Google {
            CATEGORIES[self.dl.category].1
        } else {
            ""
        };
        self.dl_entries()
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.family.to_lowercase().contains(&q) && (cat.is_empty() || e.category == cat)
            })
            .map(|(i, _)| DlRow::Entry(i))
            .collect()
    }

    /// The highlighted catalogue entry.
    pub fn dl_selected(&mut self) -> Option<Entry> {
        match self.dl_rows().get(self.dl.sel) {
            Some(DlRow::Entry(i)) => self.dl_entries().get(*i).cloned(),
            _ => None,
        }
    }

    /// Background results: the catalogue, progress, finished installs. True
    /// when something changed (redraw).
    pub fn poll_downloads(&mut self) -> bool {
        if self.page == crate::app::Page::Downloads
            && self.dl.catalogue.is_none()
            && !self.dl.loading
        {
            self.dl.loading = true;
            download::load_catalogue(self.dl.tx.clone());
        }
        let mut changed = false;
        // only what shows on screen asks for a redraw (prefetching sends a lot)
        while let Ok(m) = self.dl.rx.try_recv() {
            match m {
                Msg::Catalogue(Ok(c)) => {
                    self.dl.loading = false;
                    changed = true;
                    self.dl.catalogue = Some(c);
                }
                Msg::Catalogue(Err(e)) => {
                    self.dl.loading = false;
                    changed = true;
                    self.dl.catalogue = Some(Catalogue::default());
                    self.dl.load_error = Some(e);
                }
                Msg::Progress { id, done, total } => {
                    let Some(job) = self.dl.jobs.get_mut(&id) else {
                        continue;
                    };
                    changed = true;
                    (job.1, job.2) = (done, total);
                    let pct = if total > 0 {
                        format!("{}%", done * 100 / total.max(1))
                    } else {
                        mb(done)
                    };
                    let family = job.0.clone();
                    self.info(format!("Downloading {family}: {pct}"));
                }
                Msg::Preview { id, result } => {
                    // the font on screen, or one the page asked for
                    let ours = self.prefetch.finished(&id);
                    let pv = self.pv();
                    changed |= !ours || pv.last.as_deref() == Some(id.as_str());
                    pv.fetching.remove(&id);
                    pv.families.insert(id, result);
                }
                Msg::PrefetchQueue {
                    categories,
                    queue,
                    total,
                } => {
                    self.prefetch.queued(categories, queue, total);
                    changed = true;
                }
                Msg::PreviewsSize(bytes) => {
                    self.prefetch.measured(bytes);
                    changed = true;
                }
                Msg::Installed { id, result } => {
                    changed = true;
                    let family = self.dl.jobs.remove(&id).map_or(id, |j| j.0);
                    match result {
                        Ok((name, entry)) => self.record_install(&name, entry),
                        Err(e) => self.error(format!("Downloading {family} failed: {e}")),
                    }
                }
            }
        }
        // the preview: fetch, draw, pick up a finished image
        self.poll_preview() || changed
    }

    /// Where background work reports back.
    pub fn dl_sender(&self) -> Sender<Msg> {
        self.dl.tx.clone()
    }

    fn record_install(&mut self, name: &str, entry: Value) {
        let path = self.installed_list_path();
        let mut list = download::read_list(&path);
        list.insert(name.to_string(), entry);
        self.dl.listed = None;
        match download::write_list(&path, list) {
            Ok(()) => self.info(format!(
                "Installed {name}: it's in the font list now; a rebuild keeps it (listed in {})",
                path.display()
            )),
            Err(e) => self.error(format!("Installed {name}, but {}: {e:#}", path.display())),
        }
        self.fonts.refresh_installed();
    }

    pub fn dl_install(&mut self) {
        let Some(e) = self.dl_selected() else { return };
        let listed = self.dl_listed();
        match self.dl_status(&e, &listed) {
            DlStatus::Downloading(..) => self.info(format!("{} is downloading", e.family)),
            DlStatus::Listed => self.info(format!("{} is installed already", e.family)),
            _ => {
                self.dl.jobs.insert(e.id.clone(), (e.family.clone(), 0, 0));
                self.info(format!("Downloading {}…", e.family));
                download::install(e, download::fonts_dir(), self.dl.tx.clone());
            }
        }
    }

    /// Removes a font from the list and its downloaded files.
    pub fn dl_remove(&mut self) {
        let name = match self.dl_rows().get(self.dl.sel) {
            Some(DlRow::Listed(n)) => n.clone(),
            Some(DlRow::Entry(i)) => match self.dl_entries().get(*i) {
                Some(e) => Self::list_name(e),
                None => return,
            },
            None => return,
        };
        let path = self.installed_list_path();
        let mut list = download::read_list(&path);
        let Some(entry) = list.remove(&name) else {
            return self.info(format!("{name} wasn't installed by the studio"));
        };
        let id = entry
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let result = download::remove_files(&download::fonts_dir(), &id)
            .and_then(|_| download::write_list(&path, list));
        self.dl.listed = None;
        match result {
            Ok(()) => self.info(format!("Removed {name} (a rebuild drops it from Nix too)")),
            Err(e) => self.error(format!("{e:#}")),
        }
        self.fonts.refresh_installed();
        let n = self.dl_rows().len();
        self.dl.sel = self.dl.sel.min(n.saturating_sub(1));
    }

    fn dl_switch(&mut self, d: i32) {
        let i = DL_TABS
            .iter()
            .position(|(t, _)| *t == self.dl.tab)
            .unwrap_or(0);
        self.dl.tab = DL_TABS[crate::settings::wrap(i, d, DL_TABS.len())].0;
        // read the list again (it may have changed outside the studio)
        self.dl.listed = None;
        self.dl.sel = 0;
        self.dl.scroll = 0;
    }

    /// Keys on the page (not while typing in the search box).
    pub fn run_downloads(&mut self, a: Action) {
        let top = self.settings.layout.inner_tabs_on_top();
        if self.dl.tabs_focused {
            match a {
                Action::Left | Action::TabPrev => self.dl_switch(-1),
                Action::Right | Action::TabNext => self.dl_switch(1),
                Action::Down if top => self.dl.tabs_focused = false,
                Action::Up if !top => self.dl.tabs_focused = false,
                Action::Up if top => {
                    self.focus_page_tabs(true);
                }
                Action::Down if !top => {
                    self.focus_page_tabs(false);
                }
                Action::Confirm | Action::Back => self.dl.tabs_focused = false,
                _ => {}
            }
            return;
        }
        // the preview's keys (text, language, size, pages, fetch)
        if self.preview_action(a) {
            return;
        }
        let n = self.dl_rows().len();
        let ok = vec![true; n];
        let edge = |sel: usize, up: bool| if up { sel == 0 } else { sel + 1 >= n };
        match a {
            Action::Up if top && edge(self.dl.sel, true) => self.dl.tabs_focused = true,
            Action::Down if !top && edge(self.dl.sel, false) => self.dl.tabs_focused = true,
            Action::Up if edge(self.dl.sel, true) && self.focus_page_tabs(true) => {}
            Action::Down if edge(self.dl.sel, false) && self.focus_page_tabs(false) => {}
            Action::TabNext => self.dl_switch(1),
            Action::TabPrev => self.dl_switch(-1),
            Action::Left | Action::Right if self.dl.tab == DlTab::Google => {
                let d = if a == Action::Left { -1 } else { 1 };
                self.dl.category = crate::settings::wrap(self.dl.category, d, CATEGORIES.len());
                self.dl.sel = 0;
            }
            Action::Search => self.dl.searching = true,
            Action::Confirm => self.dl_install(),
            Action::Delete => self.dl_remove(),
            Action::Back => self.page = crate::app::Page::Main,
            _ => {
                crate::settings::move_over(&mut self.dl.sel, &ok, a);
            }
        }
    }

    /// Typing into the search box (Enter or Esc leave it).
    pub fn dl_search_key(&mut self, k: KeyEvent) {
        match k.code {
            KeyCode::Esc | KeyCode::Enter | KeyCode::Down => self.dl.searching = false,
            KeyCode::Backspace => {
                self.dl.search.pop();
            }
            KeyCode::Char(c) => self.dl.search.push(c),
            _ => {}
        }
        self.dl.sel = 0;
        self.dl.scroll = 0;
    }

    /// A click on the page.
    pub fn dl_click(&mut self, t: crate::hit::Target) {
        use crate::hit::Target as T;
        self.dl.tabs_focused = false;
        match t {
            T::DlTab(i) => {
                self.dl.tab = DL_TABS[i as usize].0;
                self.dl.listed = None;
                self.dl.sel = 0;
                self.dl.scroll = 0;
            }
            T::DlRow(i) => {
                self.dl.searching = false;
                self.dl.sel = i;
            }
            T::DlChip(i) => {
                self.dl.category = i as usize;
                self.dl.sel = 0;
            }
            T::DlSearch => self.dl.searching = true,
            T::DlButton('i') => self.dl_install(),
            T::DlButton(_) => self.dl_remove(),
            _ => {}
        }
    }
}

/// Bytes as megabytes.
pub fn mb(b: u64) -> String {
    format!("{:.1} MB", b as f64 / 1_000_000.0)
}
