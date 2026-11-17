//! Drawing the Download fonts page (F2): tabs, search and category chips,
//! the list, and the highlighted font's details with Install / Remove.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use serde_json::Value;

use crate::app::App;
use crate::download::Source;
use crate::fonts_dl::{CATEGORIES, DL_TABS, DlRow, DlStatus, DlTab, mb};
use crate::hit::{Target, hover, hover_line, hover_row};

fn status_span(s: &DlStatus) -> Span<'static> {
    match s {
        DlStatus::None => Span::raw(""),
        DlStatus::Downloading(d, t) if *t > 0 => {
            Span::styled(format!("↓ {}%", d * 100 / t), Style::new().yellow())
        }
        DlStatus::Downloading(d, _) => Span::styled(format!("↓ {}", mb(*d)), Style::new().yellow()),
        DlStatus::Listed => Span::styled("✓ installed", Style::new().green()),
        DlStatus::System => Span::styled("on the system", Style::new().dark_gray()),
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

pub fn draw_downloads(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .title(" Download fonts ")
        .border_style(Style::new().yellow());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 6 {
        return;
    }
    // the page's tabs, on top or at the bottom (the in-page tabs setting)
    let (tabs, body) = if app.settings.layout.inner_tabs_on_top() {
        let [t, _, b] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(inner);
        (t, b)
    } else {
        let [b, _, t] = Layout::vertical([
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(inner);
        (t, b)
    };
    draw_tabs(f, app, tabs);
    // wide: the list, then a column of details over the preview; narrow:
    // all three stacked. With the preview hidden, the list and the details.
    if !app.fonts_preview_visible {
        let (list, details) = if body.width >= 100 {
            let [l, r] =
                Layout::horizontal([Constraint::Min(44), Constraint::Percentage(55)]).areas(body);
            (l, r)
        } else {
            let [l, d] = Layout::vertical([Constraint::Min(5), Constraint::Length(9)]).areas(body);
            (l, d)
        };
        draw_list(f, app, list);
        draw_details(f, app, details);
        return;
    }
    let (list, details, preview) = if body.width >= 100 {
        let [l, r] =
            Layout::horizontal([Constraint::Min(44), Constraint::Percentage(55)]).areas(body);
        let [d, p] = Layout::vertical([Constraint::Length(12), Constraint::Min(4)]).areas(r);
        (l, d, p)
    } else {
        let [l, d, p] = Layout::vertical([
            Constraint::Min(5),
            Constraint::Length(9),
            Constraint::Min(6),
        ])
        .areas(body);
        (l, d, p)
    };
    draw_list(f, app, list);
    draw_details(f, app, details);
    draw_preview(f, app, preview);
}

/// The highlighted font drawn with a sample text (fonts_preview.rs).
fn draw_preview(f: &mut Frame, app: &mut App, area: Rect) {
    use crate::samples::{KINDS, Kind, LANGS};
    let selected = app.dl_selected();
    // " Preview  Code ▾  Rust ▾  12pt ": the kind and the language are
    // dropdowns
    let mut title: Vec<Span> = vec![" Preview ".into()];
    let mut kind = None;
    if let (Some(e), Some(pv)) = (&selected, app.dl.preview.as_ref()) {
        let k = pv.kind_for(e);
        kind = Some(k);
        let label = KINDS
            .iter()
            .find(|(x, _)| *x == k)
            .map_or("README", |(_, l)| crate::fonts_preview::kind_label(l));
        let mut menus = vec![(0u8, format!("{label} ▾"))];
        if k == Kind::Code {
            menus.push((1, format!("{} ▾", LANGS[pv.lang].0)));
        }
        let size = pv.size;
        // the title starts after the corner
        let mut x = area.x + 1 + title[0].width() as u16;
        for (menu, text) in menus {
            title.push(" ".into());
            x += 1;
            let w = (text.chars().count() as u16).min(area.right().saturating_sub(x));
            let r = Rect::new(x, area.y, w, 1);
            let base = Style::new().yellow();
            let hovered = app.hits.add(r, Target::PreviewMenu(menu));
            title.push(Span::styled(text, if hovered { hover(base) } else { base }));
            x += w + 1;
            title.push(" ".into());
        }
        title.push(format!(" {size}pt ").into());
    }
    let mut hints = String::from(" t text  +/- size ");
    if kind == Some(Kind::Code) {
        hints += " L language  </> page ";
    }
    if kind == Some(Kind::Prose) {
        hints += " </> page ";
    }
    let block = Block::new()
        .borders(Borders::LEFT | Borders::TOP)
        .title(Line::from(title))
        .title_bottom(Line::styled(hints, Style::new().dark_gray()))
        .border_style(Style::new().dark_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);
    // the wheel scrolls the text
    app.hits.add(inner, Target::DlPreview);
    if selected.is_none() {
        let text = if app.dl.tab == DlTab::Installed {
            " Installed: the main page's sample shows it."
        } else {
            ""
        };
        f.render_widget(Paragraph::new(text).dark_gray(), inner);
        return;
    }
    // while resizing, the preview keeps the size it had (and isn't
    // re-rendered) until the size holds still
    let mut place = Some(inner);
    if let Some(pv) = app.dl.preview.as_mut() {
        place = pv.settle.place(inner);
        pv.area = pv.settle.size((inner.width, inner.height));
        app.redraw_at = crate::settle::sooner(app.redraw_at, pv.settle.due());
    }
    let note = app.preview_note();
    match app.dl.preview.as_mut().and_then(|pv| pv.image.as_mut()) {
        Some(img) => {
            if let Some(r) = place {
                f.render_stateful_widget(
                    ratatui_image::StatefulImage::default()
                        .resize(ratatui_image::Resize::Crop(None)),
                    r,
                    img,
                )
            }
        }
        None => f.render_widget(Paragraph::new(format!(" {note}")).dark_gray(), inner),
    }
}

fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) {
    let mut x = area.x + 1;
    for (i, (tab, name)) in DL_TABS.iter().enumerate() {
        let text = format!(" {name} ");
        let w = text.chars().count() as u16;
        let r = Rect {
            x,
            y: area.y,
            width: w.min(area.right().saturating_sub(x)),
            height: 1,
        };
        let mut style = match (*tab == app.dl.tab, app.dl.tabs_focused) {
            (true, true) => Style::new().black().on_yellow().bold(),
            (true, false) => Style::new().reversed().bold(),
            _ => Style::new().dark_gray(),
        };
        let mut line = Line::from(Span::styled(text, style));
        if app.hits.add(r, Target::DlTab(i as u8)) {
            style = hover_row(style);
            line = hover_line(line, 0);
        }
        f.render_widget(Paragraph::new(line).style(style), r);
        x += w + 1;
    }
}

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    let mut y = area.y;
    // search line
    let rows = app.dl_rows();
    let count = match (&app.dl.catalogue, app.dl.tab) {
        (None, DlTab::Google | DlTab::Nerd) => " loading the font lists…".to_string(),
        (Some(c), DlTab::Nerd) if !c.nerd_tag.is_empty() => {
            format!("   {} fonts   release {}", rows.len(), c.nerd_tag)
        }
        _ => format!("   {} fonts", rows.len()),
    };
    let search = if app.dl.searching || !app.dl.search.is_empty() {
        format!(
            " / {}{}",
            app.dl.search,
            if app.dl.searching { "▏" } else { "" }
        )
    } else {
        " / search".to_string()
    };
    let sr = Rect {
        x: area.x,
        y,
        width: (search.chars().count() as u16).min(area.width),
        height: 1,
    };
    let mut style = if app.dl.searching {
        Style::new().yellow()
    } else {
        Style::new().dark_gray()
    };
    if app.hits.add(sr, Target::DlSearch) {
        style = hover(style);
    }
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(search, style),
            Span::styled(count, Style::new().dark_gray()),
        ])),
        Rect { height: 1, ..area },
    );
    y += 1;
    // category chips (Google)
    if app.dl.tab == DlTab::Google {
        let mut x = area.x + 1;
        for (i, (label, _)) in CATEGORIES.iter().enumerate() {
            let text = format!("[{label}]");
            let w = text.chars().count() as u16;
            let r = Rect {
                x,
                y,
                width: w.min(area.right().saturating_sub(x)),
                height: 1,
            };
            let mut st = if i == app.dl.category {
                Style::new().black().on_cyan()
            } else {
                Style::new().dark_gray()
            };
            if app.hits.add(r, Target::DlChip(i as u8)) {
                st = hover(st);
            }
            f.render_widget(Paragraph::new(Span::styled(text, st)), r);
            x += w + 1;
        }
        y += 1;
    }
    if let Some(e) = &app.dl.load_error {
        f.render_widget(
            Paragraph::new(format!(" Couldn't load the font lists: {e}"))
                .red()
                .wrap(Wrap { trim: false }),
            Rect {
                y,
                height: 3.min(area.bottom().saturating_sub(y)),
                ..area
            },
        );
        y += 3;
    }
    let list = Rect {
        y,
        height: area.bottom().saturating_sub(y),
        ..area
    };
    let h = list.height as usize;
    app.dl.sel = app.dl.sel.min(rows.len().saturating_sub(1));
    let sel = app.dl.sel;
    if sel < app.dl.scroll {
        app.dl.scroll = sel;
    } else if h > 0 && sel >= app.dl.scroll + h {
        app.dl.scroll = sel + 1 - h;
    }
    let listed = app.dl_listed();
    let name_w = (list.width as usize).saturating_sub(30).clamp(16, 40);
    for (i, row) in rows.iter().enumerate().skip(app.dl.scroll).take(h) {
        let r = Rect {
            y: list.y + (i - app.dl.scroll) as u16,
            height: 1,
            ..list
        };
        let (name, kind, status) = match row {
            DlRow::Entry(n) => {
                let e = &app.dl_entries()[*n];
                (
                    e.family.clone(),
                    e.category.clone(),
                    status_span(&app.dl_status(e, &listed)),
                )
            }
            DlRow::Listed(name) => {
                let src = listed
                    .get(name)
                    .and_then(|v| v.get("source"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                (
                    name.clone(),
                    src.to_string(),
                    status_span(&DlStatus::Listed),
                )
            }
        };
        let line = Line::from(vec![
            Span::raw(format!(" {:<name_w$}", truncate(&name, name_w))),
            Span::styled(format!(" {:<12}", kind), Style::new().dark_gray()),
            status,
        ]);
        let on = i == sel && !app.dl.tabs_focused;
        let mut style = if on {
            Style::new().reversed()
        } else {
            Style::new()
        };
        let mut line = line;
        if app.hits.add(r, Target::DlRow(i)) && !on {
            style = hover_row(style);
            line = hover_line(line, 0);
        }
        // the cursor covers the text, not padding or swatches
        let (line, style) = if on {
            (crate::hit::cursor_line(line, style), Style::new())
        } else {
            (line, style)
        };
        f.render_widget(Paragraph::new(line).style(style), r);
    }
    if rows.is_empty() && app.dl.catalogue.is_some() {
        let text = if app.dl.tab == DlTab::Installed {
            " Nothing installed by the studio yet."
        } else {
            " No fonts match."
        };
        f.render_widget(Paragraph::new(text).dark_gray(), Rect { height: 1, ..list });
    }
}

fn draw_details(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::new()
        .borders(Borders::LEFT | Borders::TOP)
        .border_style(Style::new().dark_gray());
    let inner = block.inner(area);
    // a column of padding, so wrapped lines keep the margin
    let inner = Rect {
        x: inner.x + 1,
        width: inner.width.saturating_sub(1),
        ..inner
    };
    f.render_widget(block, area);
    let listed = app.dl_listed();
    let mut lines: Vec<Line> = Vec::new();
    let mut button: Option<(&str, char)> = None;
    match app.dl_rows().get(app.dl.sel).cloned() {
        Some(DlRow::Entry(i)) => {
            let e = app.dl_entries()[i].clone();
            let status = app.dl_status(&e, &listed);
            lines.push(Line::styled(e.family.clone(), Style::new().bold()));
            lines.push(Line::raw(""));
            let weights = match (e.variable, e.weights.first(), e.weights.last()) {
                (true, Some(a), Some(b)) => format!("variable, {a}–{b}"),
                (false, Some(a), Some(b)) if a != b => {
                    format!("{a}–{b} ({} weights)", e.weights.len())
                }
                (_, Some(a), _) => a.to_string(),
                _ => "–".into(),
            };
            let source = match &e.source {
                Source::Google => "Google Fonts (the google/fonts repo)".to_string(),
                Source::Fontsource => "Fontsource (latin + latin-ext)".to_string(),
                Source::Nerd { asset, size, .. } => {
                    format!("Nerd Fonts {asset}.tar.xz, {}", mb(*size))
                }
            };
            for (k, v) in [
                ("category", e.category.clone()),
                (
                    "license",
                    if e.license.is_empty() {
                        "–".into()
                    } else {
                        e.license.clone()
                    },
                ),
                ("weights", weights),
                ("italic", if e.italic { "yes".into() } else { "no".into() }),
                ("from", source),
            ] {
                if !(k == "weights" || k == "italic") || !matches!(e.source, Source::Nerd { .. }) {
                    lines.push(Line::from(vec![
                        Span::styled(format!("{k:<9}"), Style::new().dark_gray()),
                        Span::raw(v),
                    ]));
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::from(vec![status_span(&status)]));
            button = match status {
                DlStatus::Listed => Some((" Remove ", 'r')),
                DlStatus::Downloading(..) => None,
                _ => Some((" Install ", 'i')),
            };
        }
        Some(DlRow::Listed(name)) => {
            let v = listed.get(&name).cloned().unwrap_or(Value::Null);
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("–").to_string();
            lines.push(Line::styled(name.clone(), Style::new().bold()));
            lines.push(Line::raw(""));
            lines.push(Line::raw(format!(" from     {}", s("source"))));
            let here = crate::download::fonts_dir().join(s("id")).exists();
            lines.push(Line::raw(if here {
                "files    downloaded here (a rebuild takes over)"
            } else {
                "files    provided by Nix"
            }));
            button = Some((" Remove ", 'r'));
        }
        None => {}
    }
    lines.push(Line::raw(""));
    let by = inner.y + lines.len() as u16;
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        format!(
            " Files go to {}; the list for Nix is",
            crate::download::fonts_dir().display()
        ),
        Style::new().dark_gray(),
    ));
    lines.push(Line::styled(
        format!("{}", app.installed_list_path().display()),
        Style::new().dark_gray(),
    ));
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    if let Some((text, id)) = button
        && by < inner.bottom()
    {
        let r = Rect {
            x: inner.x + 1,
            y: by,
            width: text.chars().count() as u16,
            height: 1,
        };
        let base = if id == 'i' {
            Style::new().black().on_green().bold()
        } else {
            Style::new().black().on_red().bold()
        };
        let st = if app.hits.add(r, Target::DlButton(id)) {
            hover(base)
        } else {
            base
        };
        f.render_widget(Paragraph::new(Span::styled(text, st)), r);
    }
}
