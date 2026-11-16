//! Drawing the Fonts tab: ¹ the three role slots, ² the installed fonts,
//! and the sample image in place of the preview.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui_image::{Resize, StatefulImage};

use crate::app::App;
use crate::fonts::{FontFocus, ROLE_USE, ROLES};
use crate::ui::panel_number;

fn block(title: Line<'static>, focused: bool) -> Block<'static> {
    let b = Block::bordered().title(title);
    if focused {
        b.border_style(Style::new().yellow())
    } else {
        b.border_style(Style::new().dark_gray())
    }
}

pub fn draw_fonts(f: &mut Frame, app: &mut App, area: Rect) {
    let t = &app.fonts;
    // three lines per role (name, size, first warning) plus borders
    let [slots_area, list_area] =
        Layout::vertical([Constraint::Length(3 * 3 + 2), Constraint::Min(0)]).areas(area);

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
    let b = block(title, t.focus == FontFocus::Slots);
    let inner = b.inner(slots_area);
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

    // ² installed fonts
    let browsing = matches!(t.focus, FontFocus::List | FontFocus::Search);
    let title = Line::from(vec![
        panel_number(2),
        "Installed fonts".into(),
        Span::styled(format!(" for {} ", ROLES[t.slot]), Style::new().dark_gray()),
        if t.mono_only {
            Span::styled("monospaced only ", Style::new().yellow())
        } else {
            "".into()
        },
    ]);
    let b = block(title, browsing);
    let inner = b.inner(list_area);
    f.render_widget(b, list_area);
    if inner.height < 2 {
        return;
    }
    let search_style = if t.focus == FontFocus::Search {
        Style::new().yellow()
    } else {
        Style::new().dark_gray()
    };
    let matches = t.matches();
    let mut lines = vec![Line::from(vec![
        Span::styled(" / ", search_style),
        Span::styled(
            if t.focus == FontFocus::Search {
                format!("{}▏", t.search)
            } else if t.search.is_empty() {
                "search".into()
            } else {
                t.search.clone()
            },
            search_style,
        ),
        Span::styled(
            format!("   {} fonts", matches.len()),
            Style::new().dark_gray(),
        ),
    ])];
    let rows = inner.height as usize - 1;
    let sel = t.list_sel.min(matches.len().saturating_sub(1));
    let current = t.preset.fonts[t.slot].family.to_lowercase();
    // keep the selection visible
    let app_fonts = &mut app.fonts;
    if sel < app_fonts.list_scroll {
        app_fonts.list_scroll = sel;
    } else if sel >= app_fonts.list_scroll + rows {
        app_fonts.list_scroll = sel + 1 - rows;
    }
    let t = &app.fonts;
    for (row, &i) in matches.iter().enumerate().skip(t.list_scroll).take(rows) {
        let font = &t.installed[i];
        let is_sel = browsing && row == sel;
        let mut spans = vec![
            Span::styled(
                if font.family.to_lowercase() == current {
                    " ● "
                } else {
                    "   "
                },
                Style::new().yellow(),
            ),
            Span::styled(
                format!("{:<32}", truncate(&font.family, 32)),
                if is_sel {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                },
            ),
        ];
        if font.mono {
            spans.push(Span::styled(" mono", Style::new().dark_gray()));
        }
        if font.nerd {
            spans.push(Span::styled(" NF", Style::new().green()));
        }
        lines.push(Line::from(spans));
    }
    if t.installed.is_empty() {
        lines.push(Line::styled(
            "   (no fonts from fc-list)",
            Style::new().red(),
        ));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// The sample image: the fonts drawn by Pango at this machine's sizes and
/// DPI, in the palette being edited.
pub fn draw_sample(f: &mut Frame, app: &mut App, area: Rect) {
    let browsing = matches!(app.fonts.focus, FontFocus::List | FontFocus::Search);
    let title = Line::from(vec![
        " Sample".into(),
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
    let b = Block::bordered()
        .title(title)
        .border_style(Style::new().dark_gray());
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
