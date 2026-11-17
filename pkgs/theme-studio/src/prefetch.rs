//! Fetching font previews ahead of time (settings.json: fonts-page.prefetch
//! and prefetch-categories), so the Fonts page and the tournament draw at
//! once. A few download at a time in the background; fonts already fetched
//! are skipped, so it carries on where it stopped after a restart.

use std::collections::HashSet;

use crate::app::App;
use crate::download::{self, Catalogue, Entry, Source};

/// How many previews download at once.
const AT_ONCE: usize = 3;

/// A Google / Fontsource preview file, about.
const GOOGLE_FILE: u64 = 100_000;
/// A Nerd Font's Regular, kept once its archive is unpacked, about.
const NERD_KEPT: u64 = 2_500_000;

#[derive(Debug, Default)]
pub struct Prefetch {
    /// The categories the queue was made for (made again when they change).
    built_for: Option<Vec<String>>,
    /// Still to fetch (taken from the end).
    queue: Vec<Entry>,
    in_flight: HashSet<String>,
    /// Fonts in the chosen categories, and how many have a preview.
    pub total: usize,
    pub done: usize,
}

impl Prefetch {
    /// Downloading (or about to).
    pub fn active(&self) -> bool {
        !self.queue.is_empty() || !self.in_flight.is_empty()
    }

    /// A preview finished; true if it was one of these.
    pub fn finished(&mut self, id: &str) -> bool {
        let ours = self.in_flight.remove(id);
        if ours {
            self.done += 1;
        }
        ours
    }

    /// Starts over (after the previews were deleted).
    pub fn reset(&mut self) {
        *self = Prefetch::default();
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
    /// Each loop: keeps a few previews downloading while prefetching is on.
    /// True when something changed.
    pub fn poll_prefetch(&mut self) -> bool {
        let s = &self.settings.fonts_page;
        if !s.prefetch {
            // ones on their way still land; nothing new starts
            let was = self.prefetch.active();
            self.prefetch.queue.clear();
            self.prefetch.built_for = None;
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
        let mut changed = false;
        if self.prefetch.built_for.as_ref() != Some(&cats) {
            let (q, total) = queue(cat, &cats, download::has_preview);
            self.prefetch.done = total - q.len();
            self.prefetch.total = total;
            self.prefetch.queue = q;
            self.prefetch.built_for = Some(cats);
            changed = true;
        }
        while self.prefetch.in_flight.len() < AT_ONCE {
            let Some(e) = self.prefetch.queue.pop() else {
                break;
            };
            let tx = self.dl_sender();
            let pv = self.pv();
            if pv.families.contains_key(&e.id) {
                self.prefetch.done += 1;
                continue;
            }
            // the page may be fetching it already; its result counts here too
            if pv.fetching.insert(e.id.clone()) {
                download::fetch_preview(e.clone(), tx);
            }
            self.prefetch.in_flight.insert(e.id);
            changed = true;
        }
        changed
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
