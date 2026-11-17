//! The Fonts page's Tournament tab: choosing a pool, fetching its previews,
//! playing the matches, and the results (tournament.rs has the bracket).
//! Stages: Setup → Fetching → Playing → Results; Esc goes back to Setup,
//! where an unfinished tournament can be picked up again.

use std::path::PathBuf;

use ratatui_image::protocol::StatefulProtocol;

use crate::app::{App, Overlay};
use crate::download::{self, Entry};
use crate::fonts::SampleRenderer;
use crate::keymap::Action;
use crate::samples::{self, Kind};
use crate::settle::Settle;
use crate::tournament::{self, Entrant, Rng, Tournament};

/// The pools: (key, label). Google's categories and Nerd Fonts come from
/// the catalogue; the installed ones from the system's fonts.
pub const POOLS: [(&str, &str); 8] = [
    ("monospace", "Google monospace"),
    ("sans-serif", "Google sans-serif"),
    ("serif", "Google serif"),
    ("display", "Google display"),
    ("handwriting", "Google handwriting"),
    ("nerd font", "Nerd Fonts"),
    ("installed-mono", "Installed monospaced"),
    ("installed-propo", "Installed proportional"),
];

/// Pool sizes: all the category's fonts (0), or some picked at random.
pub const SIZES: [usize; 4] = [0, 16, 32, 64];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Setup,
    /// The pool's previews being fetched before the first match.
    Fetching,
    Playing,
    Results,
}

/// A setup row.
#[derive(Clone, Debug, PartialEq)]
pub enum SetupRow {
    Pool(usize),
    Size,
    /// A past (or unfinished) tournament.
    Past(usize),
}

/// One of the two preview panes (the results use the first).
pub struct Pane {
    pub renderer: SampleRenderer,
    pub image: Option<StatefulProtocol>,
    pub error: Option<String>,
    pub settle: Settle,
    /// Its size in cells (from the last draw, once settled).
    pub area: (u16, u16),
    /// The entrant it shows.
    pub shows: Option<usize>,
}

pub struct TourPage {
    pub stage: Stage,
    /// The cursor: setup rows, or the results' fonts.
    pub sel: usize,
    pub size: usize,
    /// Fetching: the pool, before the bracket is drawn up.
    pub pending: Vec<Entrant>,
    pub pool: (String, String),
    pub t: Option<Tournament>,
    pub names: bool,
    pub bracket: bool,
    pub bracket_scroll: usize,
    pub kind: Kind,
    pub lang: usize,
    pub pt: u32,
    pub panes: [Pane; 2],
    /// Past tournaments (read again when the setup shows).
    pub past: Option<Vec<(PathBuf, Tournament)>>,
    rng: Rng,
}

impl App {
    pub fn tour(&mut self) -> &mut TourPage {
        if self.tour_page.is_none() {
            let pane = |name| Pane {
                renderer: SampleRenderer::named(
                    self.preview.draft_dir(),
                    self.preview.dirty_flag(),
                    name,
                ),
                image: None,
                error: None,
                settle: Settle::default(),
                area: (60, 20),
                shows: None,
            };
            let panes = [pane("tour-a"), pane("tour-b")];
            self.tour_page = Some(TourPage {
                stage: Stage::Setup,
                sel: 0,
                size: 0,
                pending: Vec::new(),
                pool: Default::default(),
                t: None,
                names: false,
                bracket: false,
                bracket_scroll: 0,
                kind: Kind::Code,
                lang: 0,
                pt: 13,
                panes,
                past: None,
                rng: Rng::new(),
            });
        }
        self.tour_page.as_mut().unwrap()
    }

    /// The setup's rows.
    pub fn tour_setup_rows(&mut self) -> Vec<SetupRow> {
        let past = self.tour_past().len();
        let mut v: Vec<SetupRow> = (0..POOLS.len()).map(SetupRow::Pool).collect();
        v.push(SetupRow::Size);
        v.extend((0..past).map(SetupRow::Past));
        v
    }

    pub fn tour_past(&mut self) -> &Vec<(PathBuf, Tournament)> {
        let t = self.tour();
        if t.past.is_none() {
            t.past = Some(tournament::past(&tournament::dir()));
        }
        t.past.as_ref().unwrap()
    }

    /// A pool's fonts (all of them).
    pub fn tour_pool(&self, key: &str) -> Vec<(Entrant, Option<Entry>)> {
        let installed = |mono: bool| -> Vec<(Entrant, Option<Entry>)> {
            self.fonts
                .installed
                .iter()
                .filter(|f| f.mono == mono && !f.bitmap && !f.special)
                .map(|f| {
                    (
                        Entrant {
                            id: f.family.clone(),
                            name: f.family.clone(),
                            installed: true,
                        },
                        None,
                    )
                })
                .collect()
        };
        match key {
            "installed-mono" => installed(true),
            "installed-propo" => installed(false),
            cat => self
                .dl
                .catalogue
                .iter()
                .flat_map(|c| c.google.iter().chain(c.nerd.iter()))
                .filter(|e| e.category == cat)
                .map(|e| {
                    (
                        Entrant {
                            id: e.id.clone(),
                            name: e.family.clone(),
                            installed: false,
                        },
                        Some(e.clone()),
                    )
                })
                .collect(),
        }
    }

    /// Enter on a pool: says what it takes and asks first.
    fn tour_ask(&mut self, pool: usize) {
        let (key, label) = POOLS[pool];
        let all = self.tour_pool(key);
        if self.dl.catalogue.is_none() && !key.starts_with("installed") {
            return self.info("The catalogue is still loading");
        }
        let size = SIZES[self.tour().size];
        let n = if size == 0 {
            all.len()
        } else {
            size.min(all.len())
        };
        if n < 2 {
            return self.info(format!("{label}: not enough fonts for a tournament"));
        }
        // what fetching the previews takes (a guess for the random pick)
        let missing: Vec<&Entry> = all
            .iter()
            .filter_map(|(_, e)| e.as_ref())
            .filter(|e| !download::has_preview(&e.id))
            .collect();
        let fetch = if key.starts_with("installed") {
            "nothing to fetch".to_string()
        } else {
            let share = n as f64 / all.len() as f64;
            let (_, bytes, _) = crate::prefetch::estimate(self.dl.catalogue.as_ref().unwrap(), key);
            let left = missing.len() as f64 * share;
            let bytes = bytes as f64 * missing.len() as f64 / all.len().max(1) as f64 * share;
            format!(
                "previews to fetch: about {} ({})",
                left.round() as usize,
                crate::prefetch::size(bytes as u64)
            )
        };
        self.overlay = Overlay::Ask {
            question: format!("{label}: {n} fonts, {} picks; {fetch}. Start?", n - 1),
            then: crate::settings::Deferred::StartTournament(pool),
        };
    }

    /// Starts a tournament of a pool: picks the fonts, fetches what's
    /// missing (the bracket is drawn up once the previews are in).
    pub fn tour_start(&mut self, pool: usize) {
        let (key, label) = POOLS[pool];
        let mut all = self.tour_pool(key);
        let size = SIZES[self.tour().size];
        self.tour().rng.shuffle(&mut all);
        if size > 0 {
            all.truncate(size);
        }
        let n = all.len();
        let title = if size == 0 {
            format!("{label}, all {n}")
        } else {
            format!("{label}, {n} at random")
        };
        for (e, entry) in &all {
            match entry {
                Some(entry) => self.prefetch.urgent.push(entry.clone()),
                // installed: drawn from the system's fonts as they are
                None => {
                    self.pv().families.insert(e.id.clone(), Ok(e.name.clone()));
                }
            }
        }
        let t = self.tour();
        t.pending = all.into_iter().map(|(e, _)| e).collect();
        t.pool = (key.to_string(), title);
        t.stage = Stage::Fetching;
        t.t = None;
    }

    /// Fetching: how many previews are in (or failed), of how many.
    pub fn tour_fetched(&mut self) -> (usize, usize, usize) {
        let ids: Vec<String> = self.tour().pending.iter().map(|e| e.id.clone()).collect();
        let pv = self.pv();
        let ok = ids
            .iter()
            .filter(|id| matches!(pv.families.get(*id), Some(Ok(_))))
            .count();
        let failed = ids
            .iter()
            .filter(|id| matches!(pv.families.get(*id), Some(Err(_))))
            .count();
        (ok, failed, ids.len())
    }

    /// Each loop on the Tournament tab: the bracket once the previews are
    /// in, the two fonts drawn. True when something changed.
    pub fn poll_tournament(&mut self) -> bool {
        if self.page != crate::app::Page::Downloads
            || self.dl.tab != crate::fonts_dl::DlTab::Tournament
            || self.tour_page.is_none()
        {
            return false;
        }
        let mut changed = false;
        if self.tour().stage == Stage::Fetching {
            let (ok, failed, n) = self.tour_fetched();
            if ok + failed < n {
                return false;
            }
            // every preview in: the fonts that can be drawn play
            let ready: Vec<Entrant> = {
                let pending = std::mem::take(&mut self.tour().pending);
                let pv = self.pv();
                pending
                    .into_iter()
                    .filter(|e| matches!(pv.families.get(&e.id), Some(Ok(_))))
                    .collect()
            };
            let t = self.tour();
            if ready.len() < 2 {
                t.stage = Stage::Setup;
                return {
                    self.error("Too few previews could be fetched for a tournament");
                    true
                };
            }
            let (key, title) = t.pool.clone();
            let tour = Tournament::new(&key, &title, ready, &mut t.rng);
            t.t = Some(tour);
            t.stage = Stage::Playing;
            t.past = None;
            self.tour_save();
            if failed > 0 {
                self.info(format!(
                    "{failed} fonts left out: their previews couldn't be fetched"
                ));
            }
            changed = true;
        }
        // which fonts the panes show
        let shows: [Option<usize>; 2] = {
            let t = self.tour();
            match (t.stage, &t.t) {
                (Stage::Playing, Some(x)) => match x.pairing() {
                    Some((a, b)) => [Some(a), Some(b)],
                    None => [None, None],
                },
                (Stage::Results, Some(x)) => {
                    [x.top().into_iter().flat_map(|(_, v)| v).nth(t.sel), None]
                }
                _ => [None, None],
            }
        };
        for (i, show) in shows.into_iter().enumerate() {
            changed |= self.tour_draw_pane(i, show);
        }
        changed
    }

    /// Asks pane `i` to show entrant `who`, and picks up a finished image.
    fn tour_draw_pane(&mut self, i: usize, who: Option<usize>) -> bool {
        let Some(who) = who else {
            let p = &mut self.tour().panes[i];
            let had = p.image.is_some();
            p.image = None;
            p.shows = None;
            return had;
        };
        let Some(e) = self.tour().t.as_ref().map(|t| t.entrants[who].clone()) else {
            return false;
        };
        let family = match self.pv().families.get(&e.id) {
            Some(Ok(f)) => f.clone(),
            _ => return false,
        };
        let (cell_w, cell_h) = self.picker.as_ref().map_or((8, 16), |p| {
            let s = p.font_size();
            (s.width as u32, s.height as u32)
        });
        let dpi = self.fonts.dpi;
        let palette = self.palette.clone();
        let bg = palette.get("bg0");
        let t = self.tour();
        if t.panes[i].shows != Some(who) {
            t.panes[i].shows = Some(who);
            t.panes[i].image = None;
        }
        let (cols, rows) = (t.panes[i].area.0 as u32, t.panes[i].area.1 as u32);
        let width = (cols * cell_w).saturating_sub(24).max(100);
        let st = samples::Style {
            family: &family,
            size: t.pt,
            palette: &palette,
        };
        let px = (t.pt * dpi / 72).max(6);
        let lines = ((rows * cell_h) / (px * 12 / 10).max(1)) as usize;
        let markup = match t.kind {
            Kind::Prose => {
                let per_line = width / (px * 55 / 100).max(1);
                samples::prose_page(&st, 0, (per_line as usize * lines * 85 / 100).max(200)).0
            }
            _ => samples::code(&st, t.lang, 0, lines + 1),
        };
        let fontconfig = (!e.installed).then(|| download::preview_fontconfig_for(&e.id));
        let wrap = (t.kind == Kind::Prose).then_some(width);
        let p = &mut t.panes[i];
        p.renderer.request_with(markup, bg, dpi, fontconfig, wrap);
        match p.renderer.take() {
            Some(Ok(img)) => {
                let proto = self.picker.as_ref().map(|pk| pk.new_resize_protocol(img));
                let p = &mut self.tour().panes[i];
                p.image = proto;
                p.error = None;
                true
            }
            Some(Err(err)) => {
                self.tour().panes[i].error = Some(err);
                true
            }
            None => false,
        }
    }

    fn tour_save(&mut self) {
        let dir = tournament::dir();
        if let Some(t) = &self.tour().t
            && let Err(e) = t.save(&dir)
        {
            let msg = format!("{e:#}");
            self.error(msg);
        }
    }

    /// Keys on the Tournament tab. True if handled.
    pub fn run_tournament(&mut self, a: Action) -> bool {
        if self.tour().bracket {
            let t = self.tour();
            match a {
                Action::Down => t.bracket_scroll += 1,
                Action::Up => t.bracket_scroll = t.bracket_scroll.saturating_sub(1),
                Action::PageDown => t.bracket_scroll += 10,
                Action::PageUp => t.bracket_scroll = t.bracket_scroll.saturating_sub(10),
                Action::Back | Action::TourBracket | Action::Confirm => t.bracket = false,
                _ => {}
            }
            return true;
        }
        match self.tour().stage {
            Stage::Setup => self.run_tour_setup(a),
            Stage::Fetching => match a {
                Action::Back => {
                    let t = self.tour();
                    t.stage = Stage::Setup;
                    t.pending.clear();
                    self.prefetch.urgent.clear();
                    true
                }
                _ => false,
            },
            Stage::Playing => self.run_tour_playing(a),
            Stage::Results => self.run_tour_results(a),
        }
    }

    fn run_tour_setup(&mut self, a: Action) -> bool {
        let rows = self.tour_setup_rows();
        let last = rows.len().saturating_sub(1);
        let t = self.tour();
        t.sel = t.sel.min(last);
        let row = rows[t.sel].clone();
        match a {
            Action::Up if t.sel == 0 => return false, // on to the tabs
            Action::Up => t.sel -= 1,
            Action::Down => t.sel = (t.sel + 1).min(last),
            Action::Top => t.sel = 0,
            Action::Bottom => t.sel = last,
            Action::Left | Action::Right if row == SetupRow::Size => {
                let d = if a == Action::Left { -1 } else { 1 };
                t.size = crate::settings::wrap(t.size, d, SIZES.len());
            }
            Action::Confirm => match row {
                SetupRow::Pool(p) => self.tour_ask(p),
                SetupRow::Size => t.size = (t.size + 1) % SIZES.len(),
                SetupRow::Past(i) => {
                    let Some((_, past)) = self.tour_past().get(i).cloned() else {
                        return true;
                    };
                    let t = self.tour();
                    t.stage = if past.finished() {
                        Stage::Results
                    } else {
                        Stage::Playing
                    };
                    t.pool = (past.category.clone(), past.title.clone());
                    t.t = Some(past);
                    t.sel = 0;
                }
            },
            Action::Delete => {
                if let SetupRow::Past(i) = row
                    && let Some((path, _)) = self.tour_past().get(i).cloned()
                {
                    let _ = std::fs::remove_file(&path);
                    self.tour().past = None;
                    self.info("Deleted that tournament");
                }
            }
            _ => return false,
        }
        true
    }

    fn run_tour_playing(&mut self, a: Action) -> bool {
        let t = self.tour();
        let Some(x) = t.t.as_mut() else {
            return false;
        };
        match a {
            Action::Left | Action::Right => {
                x.pick(a == Action::Left, &mut t.rng);
                if x.finished() {
                    t.stage = Stage::Results;
                    t.sel = 0;
                    t.names = true;
                }
                t.past = None;
                self.tour_save();
                if self.tour().stage == Stage::Results {
                    self.info("The tournament is over: the top 10");
                }
            }
            Action::TourUndo => {
                if x.undo() {
                    self.tour_save();
                } else {
                    self.info("No pick to take back");
                }
            }
            Action::TourNames => t.names = !t.names,
            Action::TourBracket => {
                t.bracket = true;
                t.bracket_scroll = 0;
            }
            Action::PreviewText => {
                t.kind = if t.kind == Kind::Prose {
                    Kind::Code
                } else {
                    Kind::Prose
                }
            }
            Action::PreviewLang => t.lang = (t.lang + 1) % samples::LANGS.len(),
            Action::SizeUp => t.pt = (t.pt + 1).min(48),
            Action::SizeDown => t.pt = t.pt.saturating_sub(1).max(6),
            Action::Back => {
                t.stage = Stage::Setup;
                t.past = None;
                self.info("Saved: pick it up again from the list");
            }
            _ => return false,
        }
        true
    }

    fn run_tour_results(&mut self, a: Action) -> bool {
        let t = self.tour();
        let Some(x) = t.t.as_ref() else {
            return false;
        };
        let top: Vec<usize> = x.top().into_iter().flat_map(|(_, v)| v).collect();
        let last = top.len().saturating_sub(1);
        let who = top.get(t.sel).copied();
        match a {
            Action::Up => t.sel = t.sel.saturating_sub(1),
            Action::Down => t.sel = (t.sel + 1).min(last),
            Action::TourBracket => {
                t.bracket = true;
                t.bracket_scroll = 0;
            }
            Action::TourUndo => {
                // back into the final
                if let Some(x) = t.t.as_mut()
                    && x.undo()
                {
                    t.stage = Stage::Playing;
                    self.tour_save();
                }
            }
            Action::PreviewText => {
                t.kind = if t.kind == Kind::Prose {
                    Kind::Code
                } else {
                    Kind::Prose
                }
            }
            Action::PreviewLang => t.lang = (t.lang + 1) % samples::LANGS.len(),
            Action::SizeUp => t.pt = (t.pt + 1).min(48),
            Action::SizeDown => t.pt = t.pt.saturating_sub(1).max(6),
            Action::Back => {
                t.stage = Stage::Setup;
                t.past = None;
            }
            Action::Confirm => {
                if let Some(w) = who {
                    self.tour_install(w);
                }
            }
            Action::UseFor(r) => {
                if let Some(w) = who {
                    self.tour_use(w, &[r as usize]);
                }
            }
            Action::FillFamily => {
                if let Some(w) = who {
                    self.tour_use(w, &[0, 1, 2]);
                }
            }
            _ => return false,
        }
        true
    }

    /// Whether a results font is installed (by its name, or the family its
    /// preview was drawn with).
    fn tour_installed(&mut self, who: usize) -> Option<String> {
        let e = self.tour().t.as_ref()?.entrants[who].clone();
        let family = match self.pv().families.get(&e.id) {
            Some(Ok(f)) => f.clone(),
            _ => e.name.clone(),
        };
        let norm = |s: &str| s.to_lowercase().replace(' ', "");
        self.fonts
            .installed
            .iter()
            .find(|f| {
                norm(&f.family).starts_with(&norm(&family)) || norm(&f.family) == norm(&e.name)
            })
            .map(|_| family)
    }

    /// Enter on a result: installs it (catalogue fonts).
    fn tour_install(&mut self, who: usize) {
        if self.tour_installed(who).is_some() {
            return self.info("Installed already: 1, 2 or 3 use it, F fills every role");
        }
        let Some(id) = self.tour().t.as_ref().map(|t| t.entrants[who].id.clone()) else {
            return;
        };
        let entry = self
            .dl
            .catalogue
            .iter()
            .flat_map(|c| c.google.iter().chain(c.nerd.iter()))
            .find(|e| e.id == id)
            .cloned();
        match entry {
            Some(e) => self.dl_install_entry(e),
            None => self.info("The catalogue is still loading"),
        }
    }

    /// 1 / 2 / 3 or F on a result: uses it for roles (applied at once).
    fn tour_use(&mut self, who: usize, roles: &[usize]) {
        let Some(family) = self.tour_installed(who) else {
            return self.info("Install it first (Enter)");
        };
        match self.fonts.fill_roles(&family, roles) {
            Ok(what) => self.apply_picked_fonts(what),
            Err(e) => self.info(e),
        }
    }

    /// A click on a pane while playing: picks that font.
    pub fn tour_click_pane(&mut self, i: u8) {
        if self.tour().stage == Stage::Playing {
            self.run_tour_playing(if i == 0 { Action::Left } else { Action::Right });
        }
    }

    /// A click on a row: setup rows and results move the cursor; a second
    /// click acts like Enter.
    pub fn tour_click_row(&mut self, row: usize) {
        let t = self.tour();
        if t.sel == row {
            self.run_tournament(Action::Confirm);
        } else {
            t.sel = row;
        }
    }
}
