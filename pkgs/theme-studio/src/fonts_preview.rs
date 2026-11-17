//! The Fonts page's preview: the highlighted font drawn before installing
//! it. Google / Fontsource fonts are fetched (one small file) after a short
//! pause on them; Nerd Fonts on request (`p`), since they come as archives.
//! Drawn by ImageMagick + Pango with a fontconfig setup that includes the
//! preview cache (download.rs), in the theme's colours.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use ratatui_image::protocol::StatefulProtocol;

use crate::app::App;
use crate::download::{self, Entry, Source};
use crate::fonts::SampleRenderer;
use crate::keymap::Action;
use crate::samples::{self, KINDS, Kind, LANGS};

/// How long the highlight rests on a font before its preview is fetched.
const PAUSE: Duration = Duration::from_millis(300);

pub struct FontPreview {
    /// The text shown; None: the font's default (code, icons or prose).
    pub kind: Option<Kind>,
    pub lang: usize,
    /// Prose: the first paragraph shown, and where the next page starts.
    pub page: usize,
    pub next_page: usize,
    /// Points.
    pub size: u32,
    /// Fetched fonts: id → the family name to draw with (or what failed).
    pub families: HashMap<String, Result<String, String>>,
    pub fetching: HashSet<String>,
    pub image: Option<StatefulProtocol>,
    pub error: Option<String>,
    renderer: SampleRenderer,
    /// The highlighted font, and since when.
    last: Option<String>,
    since: Instant,
    /// The pane's size in cells (from the last draw).
    pub area: (u16, u16),
}

impl FontPreview {
    pub fn new(
        dir: &std::path::Path,
        dirty: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> FontPreview {
        FontPreview {
            kind: None,
            lang: 0,
            page: 0,
            next_page: 0,
            size: 12,
            families: HashMap::new(),
            fetching: HashSet::new(),
            image: None,
            error: None,
            renderer: SampleRenderer::named(dir, dirty, "preview"),
            last: None,
            since: Instant::now(),
            area: (60, 20),
        }
    }

    /// The text for a font: the chosen one, else its default.
    pub fn kind_for(&self, e: &Entry) -> Kind {
        self.kind.unwrap_or_else(|| {
            samples::default_kind(&e.category, matches!(e.source, Source::Nerd { .. }))
        })
    }
}

impl App {
    fn pv(&mut self) -> &mut FontPreview {
        if self.dl.preview.is_none() {
            self.dl.preview = Some(FontPreview::new(
                self.preview.draft_dir(),
                self.preview.dirty_flag(),
            ));
        }
        self.dl.preview.as_mut().unwrap()
    }

    /// Each loop: fetch the highlighted font after a pause, render it, pick
    /// up a finished image. True when something changed.
    pub fn poll_preview(&mut self) -> bool {
        if self.page != crate::app::Page::Downloads {
            return false;
        }
        let Some(e) = self.dl_selected() else {
            return false;
        };
        let pv = self.pv();
        if pv.last.as_deref() != Some(e.id.as_str()) {
            pv.last = Some(e.id.clone());
            pv.since = Instant::now();
            pv.page = 0;
            pv.image = None;
            pv.error = None;
        }
        let nerd = matches!(e.source, Source::Nerd { .. });
        if !nerd
            && pv.since.elapsed() >= PAUSE
            && !pv.families.contains_key(&e.id)
            && !pv.fetching.contains(&e.id)
        {
            pv.fetching.insert(e.id.clone());
            download::fetch_preview(e.clone(), self.dl_sender());
        }
        let Some(Ok(family)) = self.pv().families.get(&e.id).cloned() else {
            return false;
        };
        self.render_preview(&e, &family);
        match self.pv().renderer.take() {
            Some(Ok(path)) => {
                let img = image::open(&path);
                let proto = self
                    .picker
                    .as_ref()
                    .zip(img.ok())
                    .map(|(p, i)| p.new_resize_protocol(i));
                let pv = self.pv();
                pv.image = proto;
                pv.error = None;
                true
            }
            Some(Err(err)) => {
                self.pv().error = Some(err);
                true
            }
            None => false,
        }
    }

    /// Asks for the preview image (the renderer skips unchanged requests).
    fn render_preview(&mut self, e: &Entry, family: &str) {
        let (cell_w, cell_h) = self.picker.as_ref().map_or((8, 16), |p| {
            let s = p.font_size();
            (s.width as u32, s.height as u32)
        });
        let dpi = self.fonts.dpi;
        let palette = self.palette.clone();
        let bg = palette.get("bg0");
        let pv = self.pv();
        let kind = pv.kind_for(e);
        let (cols, rows) = (pv.area.0 as u32, pv.area.1 as u32);
        let width = (cols * cell_w).saturating_sub(24).max(100);
        let st = samples::Style {
            family,
            size: pv.size,
            palette: &palette,
        };
        let markup = match kind {
            Kind::Code => samples::code(&st, pv.lang),
            Kind::Icons => samples::icons(&st),
            Kind::Readme => samples::readme(&st),
            Kind::Prose => {
                // about as many characters as fit
                let px = (pv.size * dpi / 72).max(6);
                let per_line = width / (px * 55 / 100).max(1);
                let lines = (rows * cell_h) / (px * 16 / 10).max(1);
                let (m, next) =
                    samples::prose_page(&st, pv.page, (per_line * lines * 85 / 100) as usize);
                pv.next_page = next;
                m
            }
        };
        pv.renderer.request_with(
            markup,
            bg,
            dpi,
            Some(download::preview_fontconfig()),
            // prose and the README wrap; code and icons are cut, like a terminal
            matches!(kind, Kind::Prose | Kind::Readme).then_some(width),
        );
    }

    /// Keys for the preview (on the Fonts page). True if handled.
    pub fn preview_action(&mut self, a: Action) -> bool {
        let sel = self.dl_selected();
        let pv = self.pv();
        match a {
            Action::PreviewText => {
                let cur = sel.as_ref().map_or(Kind::Code, |e| pv.kind_for(e));
                let i = KINDS.iter().position(|(k, _)| *k == cur).unwrap_or(0);
                pv.kind = Some(KINDS[(i + 1) % KINDS.len()].0);
                pv.page = 0;
            }
            Action::PreviewLang => pv.lang = (pv.lang + 1) % LANGS.len(),
            Action::PreviewNext => pv.page = pv.next_page.max(pv.page),
            Action::PreviewPrev => pv.page = pv.page.saturating_sub(4),
            Action::SizeUp => pv.size = (pv.size + 1).min(48),
            Action::SizeDown => pv.size = pv.size.saturating_sub(1).max(6),
            Action::PreviewFetch => match sel {
                Some(e) if !pv.families.contains_key(&e.id) && !pv.fetching.contains(&e.id) => {
                    pv.fetching.insert(e.id.clone());
                    let what = match &e.source {
                        Source::Nerd { size, .. } => format!(" ({})", crate::fonts_dl::mb(*size)),
                        _ => String::new(),
                    };
                    let msg = format!("Fetching {} for its preview{what}…", e.family);
                    download::fetch_preview(e, self.dl_sender());
                    self.info(msg);
                }
                _ => {}
            },
            _ => return false,
        }
        true
    }

    /// What the preview pane says when there's no image.
    pub fn preview_note(&mut self) -> String {
        let Some(e) = self.dl_selected() else {
            return String::new();
        };
        let pv = self.pv();
        if let Some(Err(err)) = pv.families.get(&e.id) {
            return format!("Couldn't fetch it: {err}");
        }
        if let Some(err) = &pv.error {
            return format!("Couldn't draw it: {err}");
        }
        if pv.fetching.contains(&e.id) {
            return "fetching the font…".into();
        }
        match &e.source {
            Source::Nerd { size, .. } if !pv.families.contains_key(&e.id) => {
                format!(
                    "p fetches it for a preview ({})",
                    crate::fonts_dl::mb(*size)
                )
            }
            _ => "drawing…".into(),
        }
    }
}
