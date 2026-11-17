//! Fetching font previews ahead of time (settings.json: fonts-page.prefetch
//! and prefetch-categories), so the Fonts page and the tournament draw at
//! once. Everything slow happens off the main thread: the queue is made on a
//! thread (it checks ~1,700 folders), each download is a thread of its own
//! (curl and tar under nice), and the screen is redrawn for the progress at
//! most a few times a second. Fonts already fetched are skipped, so it
//! carries on where it stopped after a restart.

use std::collections::HashSet;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use crate::app::App;
use crate::download::{self, Catalogue, Entry, Msg, Source};

/// How many previews download at once (Nerd Font archives: one).
const AT_ONCE: usize = 3;

/// The status line's progress is redrawn at most this often.
const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// The Behaviour tab's "space used" is measured again after this long.
const SIZE_EVERY: Duration = Duration::from_secs(5);

/// A Google / Fontsource preview file, about.
const GOOGLE_FILE: u64 = 100_000;
/// A Nerd Font's Regular, kept once its archive is unpacked, about.
const NERD_KEPT: u64 = 2_500_000;

/// What fetches a preview (a stub in tests).
pub type Fetch = fn(Entry, Sender<Msg>);

#[derive(Debug, Default)]
pub struct Prefetch {
    /// The categories the queue was made for, and ones it's being made for.
    built_for: Option<Vec<String>>,
    building: Option<Vec<String>>,
    /// Still to fetch (taken from the end).
    queue: Vec<Entry>,
    in_flight: HashSet<String>,
    /// Nerd Font archives among them.
    pub(crate) nerd_in_flight: HashSet<String>,
    /// Fonts in the chosen categories, and how many have a preview.
    pub total: usize,
    pub done: usize,
    /// The progress last drawn, and when.
    drawn: (usize, Option<Instant>),
    /// The space the previews take, and when it was measured (None while
    /// it's being measured for the first time).
    pub used: Option<u64>,
    measured: Option<Instant>,
    measuring: bool,
    /// Tests swap the downloads for a stub.
    pub fetch: Option<Fetch>,
}

impl Prefetch {
    /// Downloading (or about to).
    pub fn active(&self) -> bool {
        !self.queue.is_empty() || !self.in_flight.is_empty() || self.building.is_some()
    }

    /// A preview finished; true if it was one of these.
    pub fn finished(&mut self, id: &str) -> bool {
        self.nerd_in_flight.remove(id);
        let ours = self.in_flight.remove(id);
        if ours {
            self.done += 1;
        }
        ours
    }

    /// The queue made on a thread arrived.
    pub fn queued(&mut self, categories: Vec<String>, queue: Vec<Entry>, total: usize) {
        if self.building.as_ref() != Some(&categories) {
            return; // the choice changed since: another is on its way
        }
        self.building = None;
        self.done = total - queue.len();
        self.total = total;
        self.queue = queue;
        self.built_for = Some(categories);
    }

    /// The space used, measured.
    pub fn measured(&mut self, bytes: u64) {
        self.used = Some(bytes);
        self.measured = Some(Instant::now());
        self.measuring = false;
    }

    /// Starts over (after the previews were deleted).
    pub fn reset(&mut self) {
        let fetch = self.fetch;
        *self = Prefetch::default();
        self.fetch = fetch;
    }
}

/// The fonts of `categories` still to fetch (in catalogue order, Google's
/// before the Nerd Fonts' archives), and how many there are in all.
pub fn queue(
    cat: &Catalogue,
    categories: &[String],
    has: impl Fn(&str) -> bool,
) -> (Vec<Entry>, usize) {
    let all: Vec<&Entry> = cat
        .google
        .iter()
        .chain(cat.nerd.iter())
        .filter(|e| categories.contains(&e.category))
        .collect();
    let total = all.len();
    let mut q: Vec<Entry> = all.into_iter().filter(|e| !has(&e.id)).cloned().collect();
    // taken with pop(): the first ones last
    q.reverse();
    (q, total)
}

/// A category's fonts: (how many, bytes to download, bytes kept), about.
pub fn estimate(cat: &Catalogue, category: &str) -> (usize, u64, u64) {
    let mut out = (0, 0, 0);
    for e in cat.google.iter().chain(cat.nerd.iter()) {
        if e.category != category {
            continue;
        }
        out.0 += 1;
        match e.source {
            Source::Nerd { size, .. } => {
                out.1 += size;
                out.2 += NERD_KEPT;
            }
            _ => {
                out.1 += GOOGLE_FILE;
                out.2 += GOOGLE_FILE;
            }
        }
    }
    out
}

/// Bytes, readably: "850 KB", "7.2 MB", "1.5 GB".
pub fn size(b: u64) -> String {
    match b {
        b if b >= 1_000_000_000 => format!("{:.1} GB", b as f64 / 1e9),
        b if b >= 1_000_000 => format!("{:.1} MB", b as f64 / 1e6),
        b => format!("{} KB", b.div_ceil(1000)),
    }
}

impl App {
    /// Each loop: keeps a few previews downloading while prefetching is on,
    /// and the Behaviour tab's "space used" up to date. Quick: the slow
    /// parts run on threads. True when the screen should be redrawn.
    pub fn poll_prefetch(&mut self) -> bool {
        self.measure_previews();
        let s = &self.settings.fonts_page;
        if !s.prefetch {
            // ones on their way still land; nothing new starts
            let was = self.prefetch.active();
            self.prefetch.queue.clear();
            self.prefetch.built_for = None;
            self.prefetch.building = None;
            return was && !self.prefetch.active();
        }
        let Some(cat) = &self.dl.catalogue else {
            if !self.dl.loading {
                self.dl.loading = true;
                download::load_catalogue(self.dl_sender());
            }
            return false;
        };
        let cats = s.prefetch_categories.clone();
        if self.prefetch.built_for.as_ref() != Some(&cats) {
            // the queue is made on a thread (it checks every font's folder)
            if self.prefetch.building.as_ref() != Some(&cats) {
                self.prefetch.building = Some(cats.clone());
                let (cat, tx) = (cat.clone(), self.dl_sender());
                std::thread::spawn(move || {
                    let (queue, total) = queue(&cat, &cats, download::has_preview);
                    let _ = tx.send(Msg::PrefetchQueue {
                        categories: cats,
                        queue,
                        total,
                    });
                });
            }
            return false;
        }
        let fetch = self.prefetch.fetch.unwrap_or(download::fetch_preview);
        while self.prefetch.in_flight.len() < AT_ONCE {
            let Some(e) = self.prefetch.queue.last() else {
                break;
            };
            // Nerd Font archives one at a time
            let nerd = matches!(e.source, Source::Nerd { .. });
            if nerd && !self.prefetch.nerd_in_flight.is_empty() {
                break;
            }
            let e = self.prefetch.queue.pop().unwrap();
            if self.prefetch.in_flight.contains(&e.id) {
                continue;
            }
            let tx = self.dl_sender();
            let pv = self.pv();
            if pv.families.contains_key(&e.id) {
                self.prefetch.done += 1;
                continue;
            }
            // the page may be fetching it already; its result counts here too
            if pv.fetching.insert(e.id.clone()) {
                fetch(e.clone(), tx);
            }
            if nerd {
                self.prefetch.nerd_in_flight.insert(e.id.clone());
            }
            self.prefetch.in_flight.insert(e.id);
        }
        // the status line's "fetching previews 12/72", a few times a second
        let p = &mut self.prefetch;
        let due = p.drawn.1.is_none_or(|t| t.elapsed() >= PROGRESS_EVERY);
        if p.drawn.0 != p.done && due {
            p.drawn = (p.done, Some(Instant::now()));
            return true;
        }
        false
    }

    /// The Behaviour tab shows the space the previews take: measured on a
    /// thread while it's open, every few seconds.
    fn measure_previews(&mut self) {
        let shown = self.page == crate::app::Page::Settings
            && self.settings_page.tab == crate::settings::Tab::Behaviour;
        let p = &mut self.prefetch;
        if !shown || p.measuring || p.measured.is_some_and(|t| t.elapsed() < SIZE_EVERY) {
            return;
        }
        p.measuring = true;
        let tx = self.dl_sender();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::PreviewsSize(download::previews_size()));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, category: &str, source: Source) -> Entry {
        Entry {
            id: id.into(),
            family: id.into(),
            category: category.into(),
            license: String::new(),
            weights: vec![400],
            italic: false,
            variable: false,
            source,
            subset: "latin".into(),
        }
    }

    #[test]
    fn queues_the_chosen_categories_and_skips_fetched_ones() {
        let nerd = Source::Nerd {
            asset: "Hack".into(),
            url: String::new(),
            size: 40_000_000,
        };
        let cat = Catalogue {
            google: vec![
                entry("fira-code", "monospace", Source::Google),
                entry("inter", "sans-serif", Source::Google),
                entry("iosevka", "monospace", Source::Fontsource),
            ],
            nerd: vec![entry("hack", "nerd font", nerd)],
            nerd_tag: String::new(),
        };
        let mono = vec!["monospace".to_string(), "nerd font".to_string()];
        let (q, total) = queue(&cat, &mono, |id| id == "iosevka");
        assert_eq!(total, 3);
        // popped in catalogue order: fira-code first, the archive last
        let order: Vec<&str> = q.iter().rev().map(|e| e.id.as_str()).collect();
        assert_eq!(order, ["fira-code", "hack"]);
        assert_eq!(estimate(&cat, "monospace"), (2, 200_000, 200_000));
        assert_eq!(estimate(&cat, "nerd font"), (1, 40_000_000, NERD_KEPT));
        assert_eq!(size(200_000), "200 KB");
        assert_eq!(size(7_250_000), "7.2 MB");
        assert_eq!(size(1_500_000_000), "1.5 GB");
    }
}
