//! Drawing the Settings page (F3): tab strip, then the Keymaps, Behaviour or
//! History tab. Everything clickable goes into the hit map with a hover look.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::App;
use crate::behaviour::{BhRow, LookId};
use crate::color::Rgb;
use crate::hit::{Target, hover, hover_line, hover_row};
use crate::keymap::{Keymap, display_seq};
use crate::settings::{DiffLine, FILES, HistFocus, KmRow, TABS, Tab};

/// Keeps `sel` on screen in a list `height` rows tall.
fn follow(scroll: &mut usize, sel: usize, height: usize) {
    if height == 0 {
        return;
    }
    if sel < *scroll {
        *scroll = sel;
    } else if sel >= *scroll + height {
        *scroll = sel + 1 - height;
    }
}

fn row_rect(area: Rect, i: usize) -> Rect {
    Rect {
        y: area.y + i as u16,
        height: 1,
        ..area
    }
}

fn selected_style(on: bool, focused: bool) -> Style {
    match (on, focused) {
        (true, true) => Style::new().reversed(),
        (true, false) => Style::new().bold().underlined(),
        _ => Style::new(),
    }
}

fn colour_span(c: Rgb, text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::new().fg(Color::Rgb(c.0, c.1, c.2)))
}

/// A value as text, with a swatch in front when it's a colour.
fn value_spans(v: &str, style: Style) -> Vec<Span<'static>> {
    let mut out = Vec::new();
    if v.len() == 7
        && let Some(c) = Rgb::parse(v)
    {
        out.push(colour_span(c, "██ "));
    }
    out.push(Span::styled(v.to_string(), style));
    out
}

pub fn draw_settings(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::bordered()
        .title(" Settings ")
        .border_style(Style::new().yellow());
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height < 3 {
        return;
    }
    // the page's tabs: on top or at the bottom (a setting)
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
    let tabs_focused = app.settings_page.tabs_focused && !app.page_tabs_focused;
    // tab strip
    let mut x = tabs.x + 1;
    for (i, (tab, name)) in TABS.iter().enumerate() {
        let text = format!(" {name} ");
        let w = text.chars().count() as u16;
        let r = Rect {
            x,
            y: tabs.y,
            width: w.min(tabs.right().saturating_sub(x)),
            height: 1,
        };
        // with the keyboard on the tabs, the current one is yellow
        let mut style = match (*tab == app.settings_page.tab, tabs_focused) {
            (true, true) => Style::new().black().on_yellow().bold(),
            (true, false) => Style::new().reversed().bold(),
            _ => Style::new().dark_gray(),
        };
        let mut line = Line::from(Span::styled(text, style));
        if app.hits.add(r, Target::SettingsTab(i as u8)) {
            style = hover_row(style);
            line = hover_line(line, 0);
        }
        f.render_widget(Paragraph::new(line).style(style), r);
        x += w + 1;
    }
    match app.settings_page.tab {
        Tab::Keymaps => draw_keymaps(f, app, body),
        Tab::Behaviour => draw_behaviour(f, app, body),
        Tab::History => draw_history(f, app, body),
        Tab::Readme => draw_readme(f, app, body),
    }
}

// ----- README ----------------------------------------------------------------

/// docs/theme-studio.md, rendered for the pane (at most 100 columns wide, so
/// lines stay readable), from the scroll position, with a scrollbar.
fn draw_readme(f: &mut Frame, app: &mut App, area: Rect) {
    let path = crate::docs::path(&app.store.dir);
    let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let p = &mut app.settings_page;
    // read it (again, if the file changed)
    if p.readme.as_ref().is_none_or(|(m, _)| *m != modified) {
        let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()));
        p.readme = Some((modified, text));
        p.readme_doc = None;
    }
    let text = match &p.readme {
        Some((_, Ok(t))) => t,
        Some((_, Err(e))) => {
            let line = Line::styled(format!(" {e}"), Style::new().fg(Color::Red));
            f.render_widget(Paragraph::new(line), area);
            return;
        }
        None => return,
    };
    let width = area.width.saturating_sub(3).min(100);
    if p.readme_doc.as_ref().is_none_or(|(w, _)| *w != width) {
        p.readme_doc = Some((width, crate::docs::render(text, width)));
    }
    let doc = &p.readme_doc.as_ref().unwrap().1;
    let last = doc.lines.len().saturating_sub(1);
    p.readme_scroll = p.readme_scroll.min(last);
    let h = area.height as usize;
    let lines: Vec<Line> = doc
        .lines
        .iter()
        .skip(p.readme_scroll)
        .take(h)
        .cloned()
        .collect();
    let text_area = Rect {
        x: area.x + 1,
        width: width.min(area.width.saturating_sub(2)),
        ..area
    };
    f.render_widget(Paragraph::new(lines), text_area);
    // the wheel scrolls it
    app.hits.add(area, Target::SettingsDiff);
    if doc.lines.len() > h {
        let mut state = ratatui::widgets::ScrollbarState::new(doc.lines.len().saturating_sub(h))
            .position(p.readme_scroll);
        f.render_stateful_widget(
            ratatui::widgets::Scrollbar::new(ratatui::widgets::ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area,
            &mut state,
        );
    }
}

// ----- Keymaps ---------------------------------------------------------------

fn draw_keymaps(f: &mut Frame, app: &mut App, area: Rect) {
    let side = area.width >= 100;
    let (list, info) = if side {
        let [l, _, i] = Layout::horizontal([
            Constraint::Min(56),
            Constraint::Length(1),
            Constraint::Length(44),
        ])
        .areas(area);
        (l, Some(i))
    } else {
        (area, None)
    };
    // the preset list: the difference on the left, the list on the right
    // (stacked when narrow)
    if app.settings_page.browse.is_some() {
        let (diff, presets) = match info {
            Some(i) => (list, i),
            None => {
                let h = app.presets.len() as u16 + 10;
                let [p, _, d] = Layout::vertical([
                    Constraint::Length(h),
                    Constraint::Length(1),
                    Constraint::Min(3),
                ])
                .areas(area);
                (d, p)
            }
        };
        return crate::presets::draw_browse(f, app, diff, presets);
    }
    let rows = app.keymap_rows();
    let n = rows.len();
    app.settings_page.km_sel = app.settings_page.km_sel.min(n.saturating_sub(1));
    let sel = app.settings_page.km_sel;
    let mut scroll = app.settings_page.km_scroll;
    follow(&mut scroll, sel, list.height as usize);
    app.settings_page.km_scroll = scroll;
    let shipped = Keymap::builtin()
        .into_iter()
        .find(|b| b.name == app.keymap.name);
    let leader = app.keymap.leader;
    let w = list.width as usize;
    for (i, row) in rows
        .iter()
        .enumerate()
        .skip(scroll)
        .take(list.height as usize)
    {
        let r = row_rect(list, i - scroll);
        let on = i == sel;
        let line = match *row {
            KmRow::Preset => {
                let pos = app
                    .presets
                    .iter()
                    .position(|p| p.name == app.keymap.name)
                    .map_or(0, |p| p + 1);
                Line::from(vec![
                    Span::styled(" Preset  ", Style::new().bold()),
                    Span::styled(
                        format!("[ {} ▾ ]", app.keymap.name),
                        Style::new().yellow().bold(),
                    ),
                    Span::styled(
                        format!(
                            "  {pos}/{}  {}",
                            app.presets.len(),
                            if shipped.is_some() {
                                "shipped"
                            } else {
                                "yours"
                            }
                        ),
                        Style::new().dark_gray(),
                    ),
                ])
            }
            KmRow::Gap => Line::raw(""),
            KmRow::Header(ctx) => {
                let open = !app.settings_page.km_closed.contains(&ctx);
                Line::from(vec![
                    Span::styled(if open { " ▾ " } else { " ▸ " }, Style::new().yellow()),
                    Span::styled(ctx.title(), Style::new().bold().cyan()),
                ])
            }
            KmRow::Binding(ctx, action) => {
                let keys = app.keymap.keys_of(ctx, action);
                let text: Vec<String> = keys.iter().map(|k| display_seq(k, leader)).collect();
                let changed = shipped
                    .as_ref()
                    .is_some_and(|s| s.keys_of(ctx, action) != keys);
                let clash = keys
                    .iter()
                    .any(|k| !app.keymap.conflicts(ctx, action, k).is_empty());
                let label_w = w.saturating_sub(24).clamp(20, 48);
                let mut spans = vec![
                    Span::raw(format!(
                        "   {:<label_w$}",
                        truncate(&action.describe(), label_w)
                    )),
                    Span::styled(if changed { "• " } else { "  " }, Style::new().cyan()),
                ];
                if text.is_empty() {
                    spans.push(Span::styled("no keys", Style::new().dark_gray().italic()));
                } else {
                    spans.push(Span::styled(text.join("  "), Style::new().yellow()));
                }
                if clash {
                    spans.push(Span::styled("  ! clash", Style::new().red().bold()));
                }
                Line::from(spans)
            }
        };
        let clickable = !matches!(row, KmRow::Gap);
        let mut style = selected_style(on, true);
        let mut line = line;
        if clickable && app.hits.add(r, Target::SettingsRow(i)) && !on {
            // only the name is underlined
            style = hover_row(style);
            line = hover_line(
                line,
                if matches!(row, KmRow::Header(_)) {
                    1
                } else {
                    0
                },
            );
        }
        // the cursor covers the text, not padding or swatches
        let (line, style) = if on {
            (crate::hit::cursor_line(line, style), Style::new())
        } else {
            (line, style)
        };
        f.render_widget(Paragraph::new(line).style(style), r);
    }
    // what the selected row is, what can be done, and warnings
    let mut lines: Vec<Line> = Vec::new();
    match rows.get(sel) {
        Some(KmRow::Preset) => {
            lines.push(Line::styled("Keymap preset", Style::new().bold()));
            lines.push(Line::raw("Enter or a click opens the list of"));
            lines.push(Line::raw("presets: see what each would change,"));
            lines.push(Line::raw("load one, make a new one from it,"));
            lines.push(Line::raw("delete or reset one."));
        }
        Some(KmRow::Binding(ctx, action)) => {
            lines.push(Line::styled(action.describe(), Style::new().bold()));
            lines.push(Line::styled(
                format!("in {} · {}", ctx.title(), action.id()),
                Style::new().dark_gray(),
            ));
            lines.push(Line::raw(""));
            if let Some(s) = &shipped {
                let k: Vec<String> = s
                    .keys_of(*ctx, *action)
                    .iter()
                    .map(|k| display_seq(k, leader))
                    .collect();
                lines.push(Line::from(vec![
                    Span::raw("shipped: "),
                    Span::styled(
                        if k.is_empty() {
                            "no keys".into()
                        } else {
                            k.join("  ")
                        },
                        Style::new().yellow(),
                    ),
                ]));
            }
            for k in app.keymap.keys_of(*ctx, *action) {
                for c in app.keymap.conflicts(*ctx, *action, &k) {
                    lines.push(Line::styled(format!("! {c}"), Style::new().red()));
                }
            }
            lines.push(Line::raw(""));
            lines.push(Line::raw("Enter  press new keys (replace)"));
            lines.push(Line::raw("a      press keys to add"));
            lines.push(Line::raw("i      type keys (Vim notation)"));
            lines.push(Line::raw("d      unbind"));
            lines.push(Line::raw("n      save the keymap as a preset"));
        }
        _ => {}
    }
    let warnings = app.keymap.warnings();
    if !warnings.is_empty() {
        lines.push(Line::raw(""));
        for w in warnings {
            lines.push(Line::styled(format!("⚠ {w}"), Style::new().red()));
        }
    }
    if let Some(info) = info {
        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::new()
                    .borders(ratatui::widgets::Borders::LEFT)
                    .border_style(Style::new().dark_gray()),
            ),
            info,
        );
    }
}

// ----- Behaviour -------------------------------------------------------------

/// The Behaviour tab's value column (the longest value fits).
const VALUE_W: usize = 20;

fn draw_behaviour(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = crate::behaviour::visible_behaviour_rows(&app.settings_page.bh_closed);
    // the cursor never rests on a blank line or a sample
    if !rows[app.settings_page.bh_sel.min(rows.len() - 1)].selectable() {
        app.settings_page.bh_sel = crate::behaviour::selectable_ends(&rows).0;
    }
    let sel = app.settings_page.bh_sel.min(rows.len() - 1);
    // the list scrolls; a note stays below it
    let [list, _, note] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    let mut scroll = app.settings_page.bh_scroll;
    follow(&mut scroll, sel, list.height as usize);
    // show a group's title when its first row is at the top
    if scroll > 0
        && matches!(rows.get(scroll - 1), Some(BhRow::Header(_)))
        && sel < scroll + list.height as usize - 1
    {
        scroll -= 1;
    }
    app.settings_page.bh_scroll = scroll;
    // the looks as edited (this is drawn with the draft in place)
    let looks = app.looks(&app.settings);
    let focused = !app.settings_page.tabs_focused && !app.page_tabs_focused;
    for (i, row) in rows
        .iter()
        .enumerate()
        .skip(scroll)
        .take(list.height as usize)
    {
        let r = row_rect(list, i - scroll);
        let line = match *row {
            BhRow::Gap => Line::raw(""),
            BhRow::Header(h) => Line::from(vec![
                Span::styled(
                    if app.settings_page.bh_closed.contains(h) {
                        " ▸ "
                    } else {
                        " ▾ "
                    },
                    Style::new().yellow(),
                ),
                Span::styled(h, Style::new().bold().cyan()),
            ]),
            BhRow::Sample(LookId::Hover) => Line::from(vec![
                Span::styled("   Sample:  ", Style::new().dark_gray()),
                Span::styled(" a clickable row ", looks.hover.apply(Style::new())),
                Span::styled("   pane ", Style::new().dark_gray()),
                Span::styled("──────", Style::new().fg(looks.pane)),
                Span::styled("   preview window ", Style::new().dark_gray()),
                Span::styled("──────", Style::new().fg(looks.preview)),
            ]),
            BhRow::Sample(LookId::Derived) => {
                let mut spans = vec![Span::styled("   Sample:  ", Style::new().dark_gray())];
                for (key, lit) in [
                    ("bg_visual_red", false),
                    ("bg0", true),
                    ("red", true),
                    ("green", false),
                ] {
                    let st = if lit {
                        looks.derived.apply(Style::new())
                    } else {
                        Style::new()
                    };
                    spans.push(Span::styled(format!(" {key:<14}"), st));
                    spans.push(Span::raw(" "));
                }
                spans.push(Span::styled(
                    " (bg_visual_red selected)",
                    Style::new().dark_gray(),
                ));
                Line::from(spans)
            }
            _ => {
                // columns: label | swatch | value | what Enter does
                let label = App::behaviour_label(*row);
                let value = app.behaviour_value(*row);
                let (swatch, text, style, note) = if row.is_button() {
                    (None, "[ Enter ]".to_string(), Style::new().cyan(), "")
                } else if row.is_colour() {
                    // a colour: « key » in magenta, unlike the ‹ choices ›
                    (
                        Some(app.theme_colour(&value)),
                        format!("« {value} »"),
                        Style::new().magenta(),
                        "Enter: choose",
                    )
                } else if *row == BhRow::Leader {
                    (
                        None,
                        format!("‹ {value} ›"),
                        Style::new().yellow(),
                        "Enter: press the new key",
                    )
                } else {
                    (None, format!("‹ {value} ›"), Style::new().yellow(), "")
                };
                Line::from(vec![
                    Span::raw(format!("   {label:<50} ")),
                    match swatch {
                        Some(c) => Span::styled("██ ", Style::new().fg(c)),
                        None => Span::raw("   "),
                    },
                    Span::styled(format!("{text:<VALUE_W$}"), style),
                    Span::styled(format!("  {note}"), Style::new().dark_gray()),
                ])
            }
        };
        let on = i == sel;
        let mut style = selected_style(on, focused);
        let mut line = line;
        if row.selectable() && app.hits.add(r, Target::SettingsRow(i)) && !on {
            // only the name is underlined
            style = hover_row(style);
            line = hover_line(
                line,
                if matches!(row, BhRow::Header(_)) {
                    1
                } else {
                    0
                },
            );
        }
        // the cursor covers the text, not padding or swatches
        let (line, style) = if on {
            (crate::hit::cursor_line(line, style), Style::new())
        } else {
            (line, style)
        };
        f.render_widget(Paragraph::new(line).style(style), r);
    }
    f.render_widget(
        Paragraph::new(Line::styled(
            " ←/→ or Enter changes a value; click a selected row to change it.",
            Style::new().dark_gray(),
        )),
        note,
    );
}

// ----- History ---------------------------------------------------------------

fn draw_history(f: &mut Frame, app: &mut App, area: Rect) {
    let narrow = area.width < 110;
    let (files, versions, preview) = if narrow {
        let [top, prev] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Min(4)]).areas(area);
        let [fl, vl] = Layout::horizontal([Constraint::Length(18), Constraint::Min(0)]).areas(top);
        (fl, vl, prev)
    } else {
        let [fl, vl, prev] = Layout::horizontal([
            Constraint::Length(18),
            Constraint::Length(52),
            Constraint::Min(0),
        ])
        .areas(area);
        (fl, vl, prev)
    };
    let focus = app.settings_page.hist_focus;

    // files
    let block = Block::bordered()
        .title(" Files ")
        .border_style(if focus == HistFocus::Files {
            Style::new().yellow()
        } else {
            Style::new().dark_gray()
        });
    let inner = block.inner(files);
    f.render_widget(block, files);
    for (i, name) in FILES.iter().enumerate().take(inner.height as usize) {
        let r = row_rect(inner, i);
        let on = i == app.settings_page.hist_file;
        let mut style = selected_style(on, focus == HistFocus::Files);
        let mut line = Line::raw(format!(" {name}"));
        if app.hits.add(r, Target::HistFile(i)) && !(on && focus == HistFocus::Files) {
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

    // versions
    let list = app.versions();
    let n = list.len();
    app.settings_page.hist_sel = app.settings_page.hist_sel.min(n.saturating_sub(1));
    let sel = app.settings_page.hist_sel;
    let file = FILES[app.settings_page.hist_file];
    let block = Block::bordered()
        .title(format!(" Versions of {file} ({n}) "))
        .title_bottom(" n name the current file ")
        .border_style(if focus == HistFocus::Versions {
            Style::new().yellow()
        } else {
            Style::new().dark_gray()
        });
    let inner = block.inner(versions);
    f.render_widget(block, versions);
    if list.is_empty() {
        f.render_widget(
            Paragraph::new(vec![
                Line::raw(" No earlier versions yet."),
                Line::styled(
                    " Every save keeps the file as it was.",
                    Style::new().dark_gray(),
                ),
            ]),
            inner,
        );
    }
    let mut scroll = app.settings_page.hist_scroll;
    follow(&mut scroll, sel, inner.height as usize);
    app.settings_page.hist_scroll = scroll;
    let w = inner.width as usize;
    for (i, (_, s)) in list
        .iter()
        .enumerate()
        .skip(scroll)
        .take(inner.height as usize)
    {
        let r = row_rect(inner, i - scroll);
        let when = s.when.get(5..16).unwrap_or(&s.when).to_string();
        let (text, st) = match &s.name {
            Some(name) => (format!("★ {name}"), Style::new().yellow().bold()),
            None => (s.summary.clone(), Style::new()),
        };
        let line = Line::from(vec![
            Span::styled(format!(" {when}  "), Style::new().dark_gray()),
            Span::styled(truncate(&text, w.saturating_sub(15)), st),
        ]);
        let on = i == sel;
        let mut style = selected_style(on, focus == HistFocus::Versions);
        let mut line = line;
        if app.hits.add(r, Target::SettingsRow(i)) && !(on && focus == HistFocus::Versions) {
            style = hover_row(style);
            line = hover_line(line, 1);
        }
        // the cursor covers the text, not padding or swatches
        let (line, style) = if on {
            (crate::hit::cursor_line(line, style), Style::new())
        } else {
            (line, style)
        };
        f.render_widget(Paragraph::new(line).style(style), r);
    }

    // preview: the selected version in place of what's there now
    let all = app.settings_page.show_all;
    let title = match list.get(sel) {
        Some((_, s)) => format!(" Restore preview: now → {} ", s.when),
        None => " Restore preview ".into(),
    };
    let block = Block::bordered()
        .title(title)
        .title_bottom(format!(
            " r restore   A {}   PgUp/PgDn scroll ",
            if all {
                "only differences"
            } else {
                "everything"
            }
        ))
        .border_style(Style::new().dark_gray());
    let inner = block.inner(preview);
    f.render_widget(block, preview);
    app.hits.add(inner, Target::SettingsDiff);
    if list.is_empty() {
        return;
    }
    let view = app.diff_view();
    let mut lines: Vec<Line> = Vec::new();
    if let Some((now, then)) = &view.palette {
        let strip = |label: &str, cs: &[(String, Rgb)]| {
            let mut spans = vec![Span::styled(
                format!(" {label:<9}"),
                Style::new().dark_gray(),
            )];
            spans.extend(cs.iter().map(|(_, c)| colour_span(*c, "██")));
            Line::from(spans)
        };
        if !then.is_empty() || !now.is_empty() {
            lines.push(Line::styled(
                format!(" \"{}\" ", app.name),
                Style::new().bold(),
            ));
            lines.push(strip("now", now));
            lines.push(strip("selected", then));
            lines.push(Line::raw(""));
        }
    }
    if view.lines.is_empty() {
        lines.push(Line::styled(" Same as now.", Style::new().dark_gray()));
    } else {
        lines.push(Line::from(vec![
            Span::styled(" changed ", Style::new().black().on_yellow()),
            Span::styled(" now → selected   ", Style::new().dark_gray()),
            Span::styled("+ ", Style::new().green()),
            Span::styled("restoring adds   ", Style::new().dark_gray()),
            Span::styled("- ", Style::new().red()),
            Span::styled("restoring removes", Style::new().dark_gray()),
        ]));
    }
    let head = lines.len();
    let body_h = (inner.height as usize).saturating_sub(head);
    let max_scroll = view.lines.len().saturating_sub(body_h);
    app.settings_page.diff_scroll = app.settings_page.diff_scroll.min(max_scroll);
    let scroll = app.settings_page.diff_scroll;
    for l in view.lines.iter().skip(scroll).take(body_h) {
        let pad = |d: usize| "  ".repeat(d + 1);
        lines.push(match l {
            DiffLine::Head { depth, key } => {
                Line::styled(format!("{}{key}", pad(*depth)), Style::new().bold())
            }
            DiffLine::Same { depth, key, value } => {
                let mut s = vec![Span::raw(format!("{}{key}: ", pad(*depth)))];
                s.extend(value_spans(value, Style::new()));
                Line::from(s)
            }
            DiffLine::Changed {
                depth,
                key,
                now,
                then,
            } => {
                let mut s = vec![Span::styled(
                    format!("{}{key}: ", pad(*depth)),
                    Style::new().black().on_yellow(),
                )];
                s.extend(value_spans(
                    now,
                    Style::new().dark_gray().add_modifier(Modifier::CROSSED_OUT),
                ));
                s.push(Span::raw(" → "));
                s.extend(value_spans(then, Style::new().yellow().bold()));
                Line::from(s)
            }
            DiffLine::Added { depth, key, value } => {
                let mut s = vec![Span::styled(
                    format!("{}+ {key}: ", pad(*depth)),
                    Style::new().green(),
                )];
                s.extend(value_spans(value, Style::new().green()));
                Line::from(s)
            }
            DiffLine::Removed { depth, key, value } => {
                let mut s = vec![Span::styled(
                    format!("{}- {key}: ", pad(*depth)),
                    Style::new().red(),
                )];
                s.extend(value_spans(
                    value,
                    Style::new().red().add_modifier(Modifier::CROSSED_OUT),
                ));
                Line::from(s)
            }
        });
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

/// The Save and Discard buttons on the frame's bottom edge, while there are
/// unsaved changes; drawn with the live keymap (keys change only on Save).
pub fn draw_save_buttons(f: &mut Frame, app: &mut App, area: Rect, changes: usize) {
    if changes == 0 || area.height < 2 {
        return;
    }
    use crate::keymap::{Action, Context};
    let key = |a: Action| {
        app.keymap
            .key_for(&[Context::Settings, Context::Global], a)
            .map(|k| crate::ui::friendly(&k))
            .unwrap_or_default()
    };
    let note = format!(" ● {changes} unsaved ");
    let save = format!(" {} Save ", key(Action::Save));
    let discard = format!(" {} Discard ", key(Action::Discard));
    let w = |s: &str| s.chars().count() as u16;
    let total = w(&note) + w(&save) + w(&discard) + 2;
    let y = area.bottom() - 1;
    let mut x = area.right().saturating_sub(total + 2);
    f.render_widget(
        Paragraph::new(Span::styled(note.clone(), Style::new().yellow())),
        Rect {
            x,
            y,
            width: w(&note),
            height: 1,
        },
    );
    x += w(&note) + 1;
    for (text, id, base) in [
        (save, 's', Style::new().black().on_yellow().bold()),
        (discard, 'd', Style::new().reversed()),
    ] {
        let r = Rect {
            x,
            y,
            width: w(&text),
            height: 1,
        };
        let style = if app.hits.add(r, Target::SettingsButton(id)) {
            hover(base)
        } else {
            base
        };
        f.render_widget(Paragraph::new(Span::styled(text.clone(), style)), r);
        x += w(&text) + 1;
    }
}
