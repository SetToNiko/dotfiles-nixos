//! Drawing the Fonts page's Tournament tab (tour_page.rs): the setup, the
//! previews being fetched, a match, the results, and the bracket.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::App;
use crate::hit::{Target, cursor_line, hover_line};
use crate::tour_page::{POOLS, SIZES, SetupRow, Stage};

pub fn draw_tournament(f: &mut Frame, app: &mut App, area: Rect) {
    match app.tour().stage {
        Stage::Setup => draw_setup(f, app, area),
        Stage::Fetching => draw_fetching(f, app, area),
        Stage::Playing => draw_match(f, app, area),
        Stage::Results => draw_results(f, app, area),
    }
    if app.tour().bracket {
        draw_bracket(f, app, area);
    }
}

/// A clickable row with the cursor (text only) on `on`.
fn row(f: &mut Frame, app: &mut App, r: Rect, i: usize, line: Line<'static>, on: bool) {
    let line = if on {
        cursor_line(line, Style::new().reversed())
    } else if app.hits.add(r, Target::TourRow(i)) {
        hover_line(line, 0)
    } else {
        line
    };
    if on {
        app.hits.add(r, Target::TourRow(i));
    }
    f.render_widget(Paragraph::new(line), r);
}

fn draw_setup(f: &mut Frame, app: &mut App, area: Rect) {
    let [list, about] = if area.width >= 100 {
        Layout::horizontal([Constraint::Percentage(55), Constraint::Min(30)]).areas(area)
    } else {
        Layout::vertical([Constraint::Min(8), Constraint::Length(7)]).areas(area)
    };
    let rows = app.tour_setup_rows();
    let sel = app.tour().sel.min(rows.len().saturating_sub(1));
    let focused = !app.dl.tabs_focused && !app.page_tabs_focused;
    let catalogue = app.dl.catalogue.is_some();
    let counts: Vec<usize> = POOLS.iter().map(|(k, _)| app.tour_pool(k).len()).collect();
    let size = SIZES[app.tour().size];
    let past = app.tour_past().clone();
    let mut y = list.y;
    let mut put = |f: &mut Frame, app: &mut App, line: Line<'static>, i: Option<usize>| {
        if y >= list.bottom() {
            return;
        }
        let r = Rect::new(list.x, y, list.width, 1);
        match i {
            Some(i) => row(f, app, r, i, line, focused && i == sel),
            None => f.render_widget(Paragraph::new(line), r),
        }
        y += 1;
    };
    put(
        f,
        app,
        Line::styled(" Pick a pool", Style::new().bold()),
        None,
    );
    for (i, r) in rows.iter().enumerate() {
        match r {
            SetupRow::Pool(p) => {
                let (key, label) = POOLS[*p];
                let n = if key.starts_with("installed") || catalogue {
                    format!("{} fonts", counts[*p])
                } else {
                    "loading…".into()
                };
                put(
                    f,
                    app,
                    Line::from(vec![
                        Span::raw(format!("   {label:<26}")),
                        Span::styled(n, Style::new().dark_gray()),
                    ]),
                    Some(i),
                );
            }
            SetupRow::Size => {
                put(f, app, Line::raw(""), None);
                let v = if size == 0 {
                    "all of them".to_string()
                } else {
                    format!("{size} at random")
                };
                put(
                    f,
                    app,
                    Line::from(vec![
                        Span::raw(format!("   {:<26}", "Pool size")),
                        Span::styled(format!("‹ {v} ›"), Style::new().yellow()),
                    ]),
                    Some(i),
                );
                if !past.is_empty() {
                    put(f, app, Line::raw(""), None);
                    put(
                        f,
                        app,
                        Line::styled(" Past tournaments (d deletes)", Style::new().bold()),
                        None,
                    );
                }
            }
            SetupRow::Past(k) => {
                let (_, t) = &past[*k];
                let state = match t.winner() {
                    Some(w) => format!("won by {}", t.entrants[w].name),
                    None => {
                        let s = t.status();
                        format!("unfinished: round {} of {}", s.round, s.rounds)
                    }
                };
                put(
                    f,
                    app,
                    Line::from(vec![
                        Span::raw(format!("   {:<34}", t.title)),
                        Span::styled(format!("{}  {state}", t.started), Style::new().dark_gray()),
                    ]),
                    Some(i),
                );
            }
        }
    }
    let text = vec![
        Line::styled("How it works", Style::new().bold()),
        Line::raw(""),
        Line::raw(
            "Every font of the pool is shuffled into a bracket. Each match shows the same text in two fonts, names hidden: pick the one you like (←/→ or a click) and it goes on. An odd one out gets a bye.",
        ),
        Line::raw(""),
        Line::raw(
            "It takes one pick fewer than there are fonts. The end shows a top 10 by how far each font got; from there install one, use it for a role, or fill every role from its family.",
        ),
        Line::raw(""),
        Line::raw("Tournaments are saved as you go: leave with Esc, pick one up again here."),
    ];
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(
                Block::new()
                    .borders(Borders::LEFT)
                    .border_style(Style::new().dark_gray())
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            )
            .dark_gray(),
        about,
    );
}

fn draw_fetching(f: &mut Frame, app: &mut App, area: Rect) {
    let (ok, failed, n) = app.tour_fetched();
    let title = app.tour().pool.1.clone();
    let w = area.width.saturating_sub(8).min(60) as usize;
    let filled = (ok + failed) * w / n.max(1);
    let lines = vec![
        Line::styled(format!(" {title}"), Style::new().bold()),
        Line::raw(""),
        Line::raw(format!(" Fetching the previews: {}/{n}", ok + failed)),
        Line::from(vec![
            " ".into(),
            Span::styled("█".repeat(filled), Style::new().yellow()),
            Span::styled("░".repeat(w - filled), Style::new().dark_gray()),
        ]),
        Line::raw(""),
        Line::styled(
            if failed > 0 {
                format!(" {failed} couldn't be fetched: they'll sit this one out.")
            } else {
                " The first match starts once they're all here.".to_string()
            },
            Style::new().dark_gray(),
        ),
        Line::styled(" Esc stops.", Style::new().dark_gray()),
    ];
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_match(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(t) = app.tour().t.clone() else {
        return;
    };
    let s = t.status();
    let names = app.tour().names;
    let [head, panes] = Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
    let head_line = Line::from(vec![
        Span::styled(format!(" {}", t.title), Style::new().bold()),
        Span::styled(
            format!(
                "   round {} of {} · match {} of {} · {} fonts left",
                s.round, s.rounds, s.game, s.games, s.left
            ),
            Style::new().dark_gray(),
        ),
    ]);
    f.render_widget(Paragraph::new(head_line), head);
    let [a, b] = if panes.width >= 100 {
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(panes)
    } else {
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(panes)
    };
    let pair = t.pairing();
    for (i, r) in [a, b].into_iter().enumerate() {
        let who = pair.map(|(x, y)| if i == 0 { x } else { y });
        let label = match (names, who) {
            (true, Some(w)) => t.entrants[w].name.clone(),
            _ => if i == 0 { "A" } else { "B" }.to_string(),
        };
        let title = if i == 0 {
            format!(" ← {label} ")
        } else {
            format!(" {label} → ")
        };
        draw_pane(f, app, r, i, title, true);
    }
}

/// A preview pane: its frame, its image (kept at its size while
/// resizing), a click target when `pick` (the match's panes).
fn draw_pane(f: &mut Frame, app: &mut App, area: Rect, i: usize, title: String, pick: bool) {
    let hovered = pick && app.hits.add(area, Target::TourPane(i as u8));
    let block = Block::bordered()
        .title(Span::styled(title, Style::new().yellow().bold()))
        .border_style(if hovered {
            Style::new().yellow()
        } else {
            Style::new().dark_gray()
        });
    let inner = block.inner(area);
    f.render_widget(block, area);
    let p = &mut app.tour().panes[i];
    let place = p.settle.place(inner);
    p.area = p.settle.size((inner.width, inner.height));
    let due = p.settle.due();
    let error = p.error.clone();
    app.redraw_at = crate::settle::sooner(app.redraw_at, due);
    let p = &mut app.tour().panes[i];
    match (p.image.as_mut(), place) {
        (Some(img), Some(r)) => f.render_stateful_widget(
            ratatui_image::StatefulImage::default().resize(ratatui_image::Resize::Crop(None)),
            r,
            img,
        ),
        (None, _) => {
            let text = error.map_or(" drawing…".to_string(), |e| {
                format!(" Couldn't draw it: {e}")
            });
            f.render_widget(Paragraph::new(text).dark_gray(), inner);
        }
        _ => {}
    }
}

fn draw_results(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(t) = app.tour().t.clone() else {
        return;
    };
    let [list, preview] = if area.width >= 100 {
        Layout::horizontal([Constraint::Length(48), Constraint::Min(30)]).areas(area)
    } else {
        Layout::vertical([Constraint::Length(16), Constraint::Min(6)]).areas(area)
    };
    let sel = app.tour().sel;
    let focused = !app.dl.tabs_focused && !app.page_tabs_focused;
    let installed: Vec<String> = app
        .fonts
        .installed
        .iter()
        .map(|f| f.family.to_lowercase().replace(' ', ""))
        .collect();
    let mut y = list.y;
    let header = Line::styled(format!(" {}: the top 10", t.title), Style::new().bold());
    f.render_widget(Paragraph::new(header), Rect::new(list.x, y, list.width, 1));
    y += 2;
    let mut i = 0;
    for (label, who) in t.top() {
        if y >= list.bottom() {
            break;
        }
        f.render_widget(
            Paragraph::new(Line::styled(format!(" {label}"), Style::new().yellow())),
            Rect::new(list.x, y, list.width, 1),
        );
        y += 1;
        for w in who {
            if y >= list.bottom() {
                break;
            }
            let e = &t.entrants[w];
            let have = e.installed
                || installed
                    .iter()
                    .any(|f| f.starts_with(&e.name.to_lowercase().replace(' ', "")));
            let line = Line::from(vec![
                Span::raw(format!("   {:<30}", e.name)),
                Span::styled(if have { "installed" } else { "" }, Style::new().green()),
            ]);
            row(
                f,
                app,
                Rect::new(list.x, y, list.width, 1),
                i,
                line,
                focused && i == sel,
            );
            y += 1;
            i += 1;
        }
        y += 1;
    }
    let title = t
        .top()
        .into_iter()
        .flat_map(|(_, v)| v)
        .nth(sel)
        .map_or(" Preview ".to_string(), |w| {
            format!(" {} ", t.entrants[w].name)
        });
    draw_pane(f, app, preview, 0, title, false);
}

/// `b`: every round so far, and who won each match.
fn draw_bracket(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(t) = app.tour().t.clone() else {
        return;
    };
    let name = |i: usize| t.entrants[i].name.clone();
    let mut lines: Vec<Line> = Vec::new();
    for (r, matches) in t.rounds.iter().enumerate() {
        let fonts: usize = matches.iter().map(|m| 1 + m.b.is_some() as usize).sum();
        lines.push(Line::styled(
            format!(" Round {} ({fonts} fonts)", r + 1),
            Style::new().yellow().bold(),
        ));
        for m in matches {
            let line = match (m.b, m.winner) {
                (None, _) => Line::from(vec![
                    Span::raw(format!("   {}", name(m.a))),
                    Span::styled("  (bye)", Style::new().dark_gray()),
                ]),
                (Some(b), Some(w)) => Line::from(vec![
                    Span::raw(format!("   {:<28} vs {:<28}", name(m.a), name(b))),
                    Span::styled(format!(" → {}", name(w)), Style::new().green()),
                ]),
                (Some(b), None) => Line::from(vec![
                    Span::raw(format!("   {:<28} vs {:<28}", name(m.a), name(b))),
                    Span::styled(" → ?", Style::new().dark_gray()),
                ]),
            };
            lines.push(line);
        }
        lines.push(Line::raw(""));
    }
    let r = Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    };
    let max = lines
        .len()
        .saturating_sub(r.height.saturating_sub(2) as usize);
    let tp = app.tour();
    tp.bracket_scroll = tp.bracket_scroll.min(max);
    let shown: Vec<Line> = lines.into_iter().skip(tp.bracket_scroll).collect();
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(shown).block(
            Block::bordered()
                .title(" Bracket (j/k scroll, b or Esc closes) ")
                .border_style(Style::new().yellow()),
        ),
        r,
    );
}
