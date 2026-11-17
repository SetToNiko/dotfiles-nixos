//! Images wait for a size to settle. While a pane's size keeps changing
//! (the terminal resizing, a border being dragged, resize mode), an image
//! stays at the last size that held still, so it isn't re-encoded (or
//! re-rendered by ImageMagick) on every step; once the size has stayed the
//! same for [`SETTLE`] it follows.

use std::time::{Duration, Instant};

use ratatui::layout::Rect;

/// How long a size must stay the same before images follow it.
pub const SETTLE: Duration = Duration::from_millis(150);

/// One image's size: the one asked for and the one it's drawn at.
#[derive(Clone, Debug, Default)]
pub struct Settle {
    wanted: Option<(u16, u16)>,
    since: Option<Instant>,
    settled: Option<(u16, u16)>,
}

impl Settle {
    /// The size to draw at for an area of `size`: `size` itself the first
    /// time and once it has held still for [`SETTLE`], else the last
    /// settled size.
    pub fn size(&mut self, size: (u16, u16)) -> (u16, u16) {
        self.size_at(size, Instant::now())
    }

    fn size_at(&mut self, size: (u16, u16), now: Instant) -> (u16, u16) {
        if self.wanted != Some(size) {
            self.wanted = Some(size);
            self.since = Some(now);
        }
        let held = self.since.is_some_and(|t| now.duration_since(t) >= SETTLE);
        if self.settled.is_none() || held {
            self.settled = Some(size);
        }
        self.settled.unwrap_or(size)
    }

    /// When the size asked for will have settled (None: it has).
    pub fn due(&self) -> Option<Instant> {
        (self.wanted != self.settled)
            .then(|| self.since.map(|t| t + SETTLE))
            .flatten()
    }

    /// Where to draw in `area`: all of it once settled; while waiting, the
    /// settled size at its top left if that fits, else nothing (the pane's
    /// background shows until the size settles).
    pub fn place(&mut self, area: Rect) -> Option<Rect> {
        let (w, h) = self.size((area.width, area.height));
        (w <= area.width && h <= area.height).then_some(Rect {
            width: w,
            height: h,
            ..area
        })
    }
}

/// The sooner of two redraw times.
pub fn sooner(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_a_size_once_it_holds_still() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut s = Settle::default();
        // the first size is taken at once
        assert_eq!(s.size_at((80, 20), t0), (80, 20));
        assert_eq!(s.due(), None);
        // a drag: each step waits, the old size stays
        assert_eq!(s.size_at((70, 20), ms(10)), (80, 20));
        assert_eq!(s.size_at((60, 20), ms(50)), (80, 20));
        assert_eq!(s.due(), Some(ms(50) + SETTLE));
        assert_eq!(s.size_at((60, 20), ms(120)), (80, 20));
        // still for 150 ms: followed
        assert_eq!(s.size_at((60, 20), ms(200)), (60, 20));
        assert_eq!(s.due(), None);
    }

    #[test]
    fn places_the_old_size_only_where_it_fits() {
        let mut s = Settle::default();
        let r = |w, h| Rect::new(2, 3, w, h);
        assert_eq!(s.place(r(40, 10)), Some(r(40, 10)));
        // growing: the old size fits
        assert_eq!(s.place(r(50, 12)), Some(r(40, 10)));
        // shrinking past it: nothing until it settles
        assert_eq!(s.place(r(30, 12)), None);
    }
}
