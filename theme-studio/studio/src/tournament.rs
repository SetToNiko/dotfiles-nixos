//! The font tournament (the Fonts page's Tournament tab), after
//! codingfont.com: the fonts of a category, shuffled into a single
//! elimination bracket (an odd one out gets a bye). Each match shows the
//! same text in two fonts, names hidden; the one picked goes on. The end
//! lists a top 10 by the round each font reached. Tournaments are saved as
//! they go, in `~/.local/state/theme-studio/tournaments/`, so one can be
//! left and picked up again, and past results reopened.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use serde::{Deserialize, Serialize};

/// A font taking part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entrant {
    /// The catalogue id (Google / Nerd Fonts), or the family (installed).
    pub id: String,
    /// The name shown once picked (and in the results).
    pub name: String,
    /// Already installed: drawn from the system's fonts, nothing to fetch.
    #[serde(default)]
    pub installed: bool,
}

/// A match: two entrants (indexes into `entrants`), or one with a bye.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Match {
    pub a: usize,
    pub b: Option<usize>,
    pub winner: Option<usize>,
}

impl Match {
    pub fn is_bye(&self) -> bool {
        self.b.is_none()
    }

    /// The one that went out (none for a bye or an undecided match).
    pub fn loser(&self) -> Option<usize> {
        let (b, w) = (self.b?, self.winner?);
        Some(if w == self.a { b } else { self.a })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tournament {
    pub version: u32,
    /// The pool's category key ("monospace", "nerd font", "installed-mono").
    pub category: String,
    /// What it is, for the list of past tournaments.
    pub title: String,
    /// When it started (local time), and the file it's saved in.
    pub started: String,
    pub file: String,
    pub entrants: Vec<Entrant>,
    /// Round by round; the last is the one being played.
    pub rounds: Vec<Vec<Match>>,
    /// Picks in order, as (round, match), for taking them back.
    pub picks: Vec<(usize, usize)>,
    /// The two round-of-16 losers in the top 10, chosen once it's over.
    #[serde(default)]
    pub extra: Vec<usize>,
}

/// A small random number generator (xorshift), seeded from the clock.
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Rng {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        Rng::seeded(t ^ (std::process::id() as u64) << 32)
    }

    pub fn seeded(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// 0..n
    pub fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

/// Pairs entrants in order; an odd one out gets a bye (it goes on).
fn pair(ids: &[usize]) -> Vec<Match> {
    ids.chunks(2)
        .map(|c| match c {
            [a, b] => Match {
                a: *a,
                b: Some(*b),
                winner: None,
            },
            [a] => Match {
                a: *a,
                b: None,
                winner: Some(*a),
            },
            _ => unreachable!(),
        })
        .collect()
}

/// Where the play stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// 1-based, and how many rounds there are in all.
    pub round: usize,
    pub rounds: usize,
    /// 1-based, among the round's real matches (byes don't count).
    pub game: usize,
    pub games: usize,
    /// Fonts still in.
    pub left: usize,
}

impl Tournament {
    /// A new tournament of `entrants` (already chosen; shuffled here).
    pub fn new(
        category: &str,
        title: &str,
        mut entrants: Vec<Entrant>,
        rng: &mut Rng,
    ) -> Tournament {
        rng.shuffle(&mut entrants);
        let ids: Vec<usize> = (0..entrants.len()).collect();
        let started = crate::history::local_time();
        let file = format!(
            "{}.json",
            started.replace([' ', ':'], "-").replace("--", "-")
        );
        Tournament {
            version: 1,
            category: category.into(),
            title: title.into(),
            started,
            file,
            entrants,
            rounds: vec![pair(&ids)],
            picks: Vec::new(),
            extra: Vec::new(),
        }
    }

    /// The match being played: (round, match).
    pub fn current(&self) -> Option<(usize, usize)> {
        let r = self.rounds.len().checked_sub(1)?;
        self.rounds[r]
            .iter()
            .position(|m| m.winner.is_none())
            .map(|m| (r, m))
    }

    pub fn finished(&self) -> bool {
        self.current().is_none()
    }

    pub fn winner(&self) -> Option<usize> {
        match self.rounds.last()?.as_slice() {
            [only] if self.finished() => only.winner,
            _ => None,
        }
    }

    /// The current match's two fonts.
    pub fn pairing(&self) -> Option<(usize, usize)> {
        let (r, m) = self.current()?;
        let x = self.rounds[r][m];
        Some((x.a, x.b?))
    }

    /// Picks the left (`left`) or right font of the current match.
    pub fn pick(&mut self, left: bool, rng: &mut Rng) {
        let Some((r, m)) = self.current() else {
            return;
        };
        let x = &mut self.rounds[r][m];
        x.winner = Some(if left { x.a } else { x.b.unwrap_or(x.a) });
        self.picks.push((r, m));
        // the round's done: the winners meet in the next
        if self.rounds[r].iter().all(|m| m.winner.is_some()) {
            let winners: Vec<usize> = self.rounds[r].iter().filter_map(|m| m.winner).collect();
            if winners.len() > 1 {
                self.rounds.push(pair(&winners));
            } else {
                self.choose_extra(rng);
            }
        }
    }

    /// Takes the last pick back. False if there's none.
    pub fn undo(&mut self) -> bool {
        let Some((r, m)) = self.picks.pop() else {
            return false;
        };
        self.rounds.truncate(r + 1);
        self.rounds[r][m].winner = None;
        self.extra.clear();
        true
    }

    /// Two of the round-of-16 losers, at random (the top 10's last two).
    fn choose_extra(&mut self, rng: &mut Rng) {
        let n = self.rounds.len();
        if n < 4 {
            return;
        }
        let mut losers: Vec<usize> = self.rounds[n - 4].iter().filter_map(Match::loser).collect();
        rng.shuffle(&mut losers);
        losers.truncate(2);
        self.extra = losers;
    }

    pub fn status(&self) -> Status {
        let n = self.entrants.len();
        let rounds = (usize::BITS - n.saturating_sub(1).leading_zeros()) as usize;
        let (r, m) = self
            .current()
            .unwrap_or((self.rounds.len().saturating_sub(1), 0));
        let real: Vec<usize> = self.rounds[r]
            .iter()
            .enumerate()
            .filter(|(_, x)| !x.is_bye())
            .map(|(i, _)| i)
            .collect();
        let game = real
            .iter()
            .position(|&i| i == m)
            .map_or(real.len(), |p| p + 1);
        let left = self.rounds[r]
            .iter()
            .map(|x| 1 + x.b.is_some() as usize)
            .sum::<usize>()
            - self.rounds[r][..m.min(self.rounds[r].len())]
                .iter()
                .filter(|x| x.loser().is_some())
                .count();
        Status {
            round: r + 1,
            rounds: rounds.max(1),
            game,
            games: real.len(),
            left: if self.finished() { 1 } else { left },
        }
    }

    /// The top 10, in groups: (label, fonts). A bracket doesn't order the
    /// fonts within a group.
    pub fn top(&self) -> Vec<(String, Vec<usize>)> {
        let mut out = Vec::new();
        let Some(w) = self.winner() else {
            return out;
        };
        out.push(("1st".to_string(), vec![w]));
        let n = self.rounds.len();
        let losers =
            |r: usize| -> Vec<usize> { self.rounds[r].iter().filter_map(Match::loser).collect() };
        for (back, label) in [(1, "2nd"), (2, "Semi-finals"), (3, "Quarter-finals")] {
            if n >= back {
                let l = losers(n - back);
                if !l.is_empty() {
                    out.push((label.to_string(), l));
                }
            }
        }
        if !self.extra.is_empty() {
            out.push((
                "Round of 16 (two of them, at random)".to_string(),
                self.extra.clone(),
            ));
        }
        out
    }

    // ----- saving ------------------------------------------------------------

    pub fn save(&self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(&self.file);
        crate::store::write_json_file(&path, &serde_json::to_value(self)?)
            .with_context(|| format!("saving {}", path.display()))
    }

    pub fn load(path: &Path) -> Result<Tournament> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))
    }
}

/// Where tournaments are kept: `$XDG_STATE_HOME/theme-studio/tournaments`
/// (a temporary folder in tests).
pub fn dir() -> PathBuf {
    if cfg!(test) {
        return std::env::temp_dir().join(format!(
            "theme-studio-test-tournaments-{}",
            std::process::id()
        ));
    }
    crate::history::default_dir()
        .parent()
        .map(|p| p.join("tournaments"))
        .unwrap_or_default()
}

/// Past tournaments, newest first: (file, title, started, finished).
pub fn past(dir: &Path) -> Vec<(PathBuf, Tournament)> {
    let mut v: Vec<(PathBuf, Tournament)> = std::fs::read_dir(dir)
        .map(|it| {
            it.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .filter_map(|p| Tournament::load(&p).ok().map(|t| (p, t)))
                .collect()
        })
        .unwrap_or_default();
    v.sort_by(|a, b| b.1.started.cmp(&a.1.started));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entrants(n: usize) -> Vec<Entrant> {
        (0..n)
            .map(|i| Entrant {
                id: format!("f{i}"),
                name: format!("Font {i}"),
                installed: false,
            })
            .collect()
    }

    #[test]
    fn a_bracket_of_five_with_byes_runs_to_a_winner() {
        let mut rng = Rng::seeded(7);
        let mut t = Tournament::new("monospace", "Mono", entrants(5), &mut rng);
        // 5 fonts: two matches and a bye, then 3 (a match and a bye), 2, 1
        assert_eq!(t.rounds[0].len(), 3);
        assert!(t.rounds[0][2].is_bye());
        let s = t.status();
        assert_eq!(
            (s.round, s.rounds, s.game, s.games, s.left),
            (1, 3, 1, 2, 5)
        );
        let mut picks = 0;
        while !t.finished() {
            t.pick(true, &mut rng);
            picks += 1;
        }
        assert_eq!(picks, 4, "one fewer than the fonts");
        let w = t.winner().unwrap();
        let top = t.top();
        assert_eq!(top[0], ("1st".to_string(), vec![w]));
        assert_eq!(top[1].0, "2nd");
        // every font placed at most once
        let mut seen: Vec<usize> = top.iter().flat_map(|(_, v)| v.clone()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), top.iter().map(|(_, v)| v.len()).sum::<usize>());
    }

    #[test]
    fn undo_goes_back_across_rounds() {
        let mut rng = Rng::seeded(3);
        let mut t = Tournament::new("serif", "Serif", entrants(4), &mut rng);
        t.pick(true, &mut rng);
        t.pick(false, &mut rng);
        assert_eq!(t.rounds.len(), 2, "the final is set");
        assert!(t.undo());
        assert_eq!(t.rounds.len(), 1);
        assert_eq!(t.current(), Some((0, 1)));
        assert!(t.undo() && !t.undo(), "back to the start");
        assert_eq!(t.current(), Some((0, 0)));
    }

    #[test]
    fn sixteen_fonts_give_a_top_ten_and_it_saves() {
        let mut rng = Rng::seeded(11);
        let mut t = Tournament::new("monospace", "Mono 16", entrants(16), &mut rng);
        while !t.finished() {
            let left = rng.below(2) == 0;
            t.pick(left, &mut rng);
        }
        let top = t.top();
        let sizes: Vec<usize> = top.iter().map(|(_, v)| v.len()).collect();
        assert_eq!(sizes, [1, 1, 2, 4, 2]);
        let dir = std::env::temp_dir().join(format!("theme-studio-tour-{}", std::process::id()));
        t.save(&dir).unwrap();
        let back = past(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].1, t);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
