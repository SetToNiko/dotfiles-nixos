//! Drawing the Fonts tab: ¹ the three role slots, ² the installed fonts,
//! and the sample image in place of the preview.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui_image::{Resize, StatefulImage};

use crate::app::{App, Side};
use crate::fonts::{CHIPS, FontAreas, FontFocus, ROLE_USE, ROLES, Row};
use crate::ui::panel_number;

fn block(title: Line<'static>, focused: bool) -> Block<'static> {
    let b = Block::bordered().title(title);
    if focused {
        b.border_style(Style::new().yellow())
    } else {
        b.border_style(Style::new().dark_gray())
    }
}

/// ¹ the font roles and ² the installed fonts (the fonts column; the sample
/// is drawn by `draw_sample`).
pub fn draw_fonts(f: &mut Frame, app: &mut App, slots_area: Rect, list_area: Rect) {
    // the fonts column has the keyboard (and not a preview window)
    let active = app.side == Side::Fonts && app.preview.focus.is_none();
    let t = &app.fonts;

    // ¹ roles
    let title = Line::from(vec![
        panel_number(1),
        "Fonts: ".into(),
        Span::styled(t.name.clone(), Style::new().bold()),
        if t.dirty() {
            Span::styled(" ●", Style::new().yellow())
        } else {
            "".into()
        },
        " ".into(),
    ]);
    let b = block(title, active && t.focus == FontFocus::Slots);
    let inner = b.inner(slots_area);
    let slots_inner = inner;
    f.render_widget(b, slots_area);
    let mut lines = Vec::new();
    for (i, font) in t.preset.fonts.iter().enumerate() {
        let selected = i == t.slot;
        let name_style = if selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(Line::from(vec![
            Span::styled(if selected { " ▸ " } else { "   " }, Style::new().yellow()),
            Span::styled(format!("{:<5}", ROLES[i]), Style::new().bold()),
            Span::styled(format!(" {}", font.family), name_style),
        ]));
        let eff = t.effective_size(i, font.size);
        let size = if eff != font.size {
            format!("{} → {eff} here", font.size)
        } else {
            font.size.to_string()
        };
        lines.push(Line::from(vec![
            Span::raw(format!("         {size:<12}")),
            Span::styled(ROLE_USE[i], Style::new().dark_gray()),
        ]));
        match t.warnings(i).first() {
            Some(w) => lines.push(Line::styled(
                format!("         ⚠ {w}"),
                Style::new().yellow(),
            )),
            None => lines.push(Line::raw("")),
        }
    }
    if let Some(e) = &t.load_error {
        lines.insert(0, Line::styled(format!(" {e}"), Style::new().red()));
    }
    f.render_widget(Paragraph::new(lines), inner);

    // ² installed fonts: search, filter chips, then the grouped list
    let browsing = active && matches!(t.focus, FontFocus::List | FontFocus::Search);
    let title = Line::from(vec![
        panel_number(2),
        "Installed fonts".into(),
        Span::styled(format!(" for {} ", ROLES[t.slot]), Style::new().dark_gray()),
    ]);
    let b = block(title, browsing);
    let inner = b.inner(list_area);
    f.render_widget(b, list_area);
    if inner.height < 4 {
        return;
    }
    let rows = t.rows();
    let fonts_shown: usize = rows
        .iter()
        .map(|r| match r {
            Row::Group {
                depth: 0, count, ..
            } => *count,
            Row::Font { depth: 0, .. } => 1,
            _ => 0,
        })
        .sum();
    let search_style = if t.focus == FontFocus::Search {
        Style::new().yellow()
    } else {
        Style::new().dark_gray()
    };
    let search_text = if t.focus == FontFocus::Search {
        format!("{}▏", t.search)
    } else if t.search.is_empty() {
        "search".to_string()
    } else {
        t.search.clone()
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(" / ", search_style),
        Span::styled(search_text, search_style),
        Span::styled(format!("   {fonts_shown} fonts"), Style::new().dark_gray()),
    ])];

    // Chips, wrapped to the panel width; their positions are kept for clicks.
    let mut chips: Vec<(char, Rect)> = Vec::new();
    let mut chip_lines: Vec<Vec<Span>> = vec![vec![" ".into()]];
    let mut x = 1u16;
    for (c, label) in CHIPS {
        let text = format!("[{c} {label}]");
        let w = text.chars().count() as u16;
        if x + w > inner.width && x > 1 {
            chip_lines.push(vec![" ".into()]);
            x = 1;
        }
        let row = chip_lines.len() as u16 - 1;
        chips.push((
            c,
            Rect {
                x: inner.x + x,
                y: inner.y + 1 + row,
                width: w,
                height: 1,
            },
        ));
        let style = if t.filters.is_on(c) {
            Style::new().black().on_yellow()
        } else {
            Style::new().dark_gray()
        };
        let line = chip_lines.last_mut().unwrap();
        line.push(Span::styled(text, style));
        line.push(" ".into());
        x += w + 1;
    }
    lines.extend(chip_lines.into_iter().map(Line::from));

    let list_top = lines.len() as u16;
    let height = (inner.height - list_top.min(inner.height)) as usize;
    let sel = t.list_sel.min(rows.len().saturating_sub(1));
    let current = t.preset.fonts[t.slot].family.to_lowercase();
    // keep the selection visible
    let mut scroll = t.list_scroll;
    if sel < scroll {
        scroll = sel;
    } else if height > 0 && sel >= scroll + height {
        scroll = sel + 1 - height;
    }
    for (row_i, row) in rows.iter().enumerate().skip(scroll).take(height) {
        let is_sel = browsing && row_i == sel;
        let sel_style = if is_sel {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        let indent = "  ".repeat(row.depth());
        let line = match row {
            Row::Group {
                name,
                expanded,
                count,
                ..
            } => Line::from(vec![
                Span::raw(format!(" {indent}")),
                Span::styled(if *expanded { "▾ " } else { "▸ " }, Style::new().yellow()),
                Span::styled(name.clone(), sel_style.bold()),
                Span::styled(format!("  {count}"), Style::new().dark_gray()),
            ]),
            Row::Font { index, .. } => {
                let font = &t.installed[*index];
                let width = 34usize.saturating_sub(indent.len());
                let mut spans = vec![
                    Span::styled(
                        if font.family.to_lowercase() == current {
                            " ●"
                        } else {
                            "  "
                        },
                        Style::new().yellow(),
                    ),
                    Span::raw(indent.clone()),
                    Span::styled(
                        format!("{:<width$}", truncate(&font.family, width)),
                        sel_style,
                    ),
                ];
                if font.mono {
                    spans.push(Span::styled(" mono", Style::new().dark_gray()));
                }
                if font.nerd {
                    spans.push(Span::styled(" NF", Style::new().green()));
                }
                if font.bitmap {
                    spans.push(Span::styled(" bitmap", Style::new().red()));
                }
                Line::from(spans)
            }
        };
        lines.push(line);
    }
    if t.installed.is_empty() {
        lines.push(Line::styled(
            "   (no fonts from fc-list)",
            Style::new().red(),
        ));
    } else if rows.is_empty() {
        lines.push(Line::styled(
            "   no fonts match the search and filters",
            Style::new().dark_gray(),
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
    let t = &mut app.fonts;
    t.list_scroll = scroll;
    t.areas = FontAreas {
        slots: slots_inner,
        list: Rect {
            y: inner.y + list_top,
            height: height as u16,
            ..inner
        },
        chips,
    };
}

/// The sample image: the fonts drawn by Pango at this machine's sizes and
/// DPI, in the palette being edited.
pub fn draw_sample(f: &mut Frame, app: &mut App, area: Rect) {
    let browsing = matches!(app.fonts.focus, FontFocus::List | FontFocus::Search);
    let title = Line::from(vec![
        panel_number(3),
        "Sample".into(),
        Span::styled(
            format!(
                "  {}at {} dpi, sizes as on this machine ",
                if browsing {
                    "previewing the highlighted font, "
                } else {
                    ""
                },
                app.fonts.dpi
            ),
            Style::new().dark_gray(),
        ),
    ]);
    let b = Block::bordered().title(title).border_style(
        if app.side == Side::Fonts
            && app.preview.focus.is_none()
            && app.fonts.focus == FontFocus::Sample
        {
            Style::new().yellow()
        } else {
            Style::new().dark_gray()
        },
    );
    let inner = b.inner(area);
    f.render_widget(b, area);
    f.render_widget(
        Block::new().style(Style::new().bg(app.palette.get("bg0").color())),
        inner,
    );
    let inner = Rect {
        x: inner.x + 1,
        y: inner.y + 1,
        width: inner.width.saturating_sub(2),
        height: inner.height.saturating_sub(1),
    };
    if let Some(e) = &app.sample_error {
        f.render_widget(Paragraph::new(format!("Sample failed: {e}")).red(), inner);
    } else if let Some(img) = app.sample_image.as_mut() {
        // shrink to fit if needed; never stretch past real size
        f.render_stateful_widget(
            StatefulImage::default().resize(Resize::Fit(None)),
            inner,
            img,
        );
    } else {
        f.render_widget(Paragraph::new("rendering sample…").dark_gray(), inner);
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}
