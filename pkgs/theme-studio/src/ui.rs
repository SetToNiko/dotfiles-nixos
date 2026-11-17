//! Drawing. The studio's own chrome uses named terminal colours (so it
//! follows the system theme); only swatches and the preview use the palette
//! being edited.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::{App, ColourRow, Focus, Overlay, Page, Side, open_matches};
use crate::color::{self, Rgb};
use crate::config::Sizes;
use crate::hit::{Border, BorderId, Target, hover};
use crate::keymap::{Action, Context};
use crate::palette::{self, GROUPS, TEXT_KEYS};
use crate::preview::Mode as PreviewMode;

/// Narrowest the fonts column, the colour column and the preview can be.
const FONTS_W: u16 = 46;
const COLOURS_W: u16 = 52;
const PREVIEW_MIN_W: u16 = 50;
/// Font roles: three roles of three lines, plus borders.
const ROLES_H: u16 = 3 * 3 + 2;
const FONT_LIST_MIN: u16 = 6;
const SAMPLE_MIN: u16 = 6;
/// The picker is at least this tall, and grows (bigger colour square) when
/// the colour column has room beyond the whole list.
const PICKER_MIN: u16 = 14;
const PICKER_MAX: u16 = 30;
/// Rows the colour list needs with every group open: 34 colours, 5 groups, borders.
const LIST_FULL: u16 = 34 + 5 + 2;
const LIST_MIN: u16 = 8;
/// The preview as a row under the columns (narrow windows).
const PREVIEW_ROW_MIN: u16 = 12;

/// The main page's panes. `wide`: three columns; otherwise fonts and colours
/// side by side with the preview as a full-width row underneath.
#[derive(Debug, PartialEq)]
pub struct MainLayout {
    pub roles: Rect,
    pub font_list: Rect,
    pub sample: Option<Rect>,
    pub colour_list: Rect,
    pub picker: Option<Rect>,
    pub preview: Rect,
    pub status: Rect,
    pub wide: bool,
    /// The borders that can be dragged to resize panes.
    pub borders: Vec<Border>,
}

/// Where everything goes for a window size. Like btop, the layout follows
/// the window as it's resized, and below the minimum a message says what's
/// needed.
#[derive(Debug, PartialEq)]
pub enum Arrangement {
    Main(MainLayout),
    /// Download fonts / Settings
    Page {
        body: Rect,
        status: Rect,
    },
    TooSmall {
        need_w: u16,
        need_h: u16,
    },
}

/// Where a vertical border (the first pane's right edge) can be grabbed.
fn v_border(id: BorderId, first: Rect, start: u16, total: u16) -> Border {
    let hit = Rect {
        x: first.x + first.width.saturating_sub(1),
        y: first.y,
        width: 1,
        height: first.height,
    };
    Border {
        id,
        hit,
        vertical: true,
        start,
        total,
        from_end: false,
    }
}

/// A horizontal border (the first pane's bottom edge); `from_end`: the share
/// is of what comes after it.
fn h_border(id: BorderId, first: Rect, start: u16, total: u16, from_end: bool) -> Border {
    let hit = Rect {
        x: first.x,
        y: first.y + first.height.saturating_sub(1),
        width: first.width,
        height: 1,
    };
    Border {
        id,
        hit,
        vertical: false,
        start,
        total,
        from_end,
    }
}

/// `show_preview`: the preview column (or row) is shown; hidden, the fonts
/// and colour columns share its space.
pub fn arrange(
    area: Rect,
    page: Page,
    picker: bool,
    sample: bool,
    show_preview: bool,
    sizes: &Sizes,
) -> Arrangement {
    if page != Page::Main {
        if area.width < 40 || area.height < 10 {
            return Arrangement::TooSmall {
                need_w: 40,
                need_h: 10,
            };
        }
        let [body, status] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        return Arrangement::Page { body, status };
    }
    let wide = area.width >= FONTS_W + COLOURS_W + PREVIEW_MIN_W;
    let preview_w = if show_preview { PREVIEW_MIN_W } else { 0 };
    // the sample is too small to read under the narrow layout: left out there
    let sample = sample && wide;
    let fonts_need = ROLES_H + FONT_LIST_MIN + if sample { SAMPLE_MIN } else { 0 };
    let colours_need = LIST_MIN + if picker { PICKER_MIN } else { 0 };
    let columns_need = fonts_need.max(colours_need);
    let (need_w, need_h) = if wide {
        (FONTS_W + COLOURS_W + preview_w, columns_need + 1)
    } else {
        let row = if show_preview { PREVIEW_ROW_MIN } else { 0 };
        (FONTS_W + COLOURS_W, columns_need + row + 1)
    };
    if area.width < need_w || area.height < need_h {
        return Arrangement::TooSmall { need_w, need_h };
    }
    let [main, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    // rounded, so a border put at a cell stays exactly there
    let share = |n: u16, f: f64| (n as f64 * f.clamp(0.0, 1.0)).round() as u16;
    let mut borders = Vec::new();
    let (fonts_col, colours_col, preview) = if !show_preview {
        // fonts and colours share the width, in the proportion they have
        // when the preview is shown
        let fonts_share = sizes.fonts_width / (sizes.fonts_width + sizes.colours_width).max(0.01);
        let fw = share(main.width, fonts_share)
            .max(FONTS_W)
            .min(main.width - COLOURS_W);
        let [f, c] = Layout::horizontal([Constraint::Length(fw), Constraint::Min(0)]).areas(main);
        borders.push(v_border(BorderId::FontsColours, f, main.x, main.width));
        (f, c, Rect::default())
    } else if wide {
        let fw = share(main.width, sizes.fonts_width)
            .max(FONTS_W)
            .min(main.width - COLOURS_W - PREVIEW_MIN_W);
        let cw = share(main.width, sizes.colours_width)
            .max(COLOURS_W)
            .min(main.width - fw - PREVIEW_MIN_W);
        let [f, c, p] = Layout::horizontal([
            Constraint::Length(fw),
            Constraint::Length(cw),
            Constraint::Min(0),
        ])
        .areas(main);
        borders.push(v_border(BorderId::FontsColours, f, main.x, main.width));
        borders.push(v_border(BorderId::ColoursPreview, c, c.x, main.width));
        (f, c, p)
    } else {
        let preview_h = share(main.height, sizes.narrow_preview_height)
            .clamp(PREVIEW_ROW_MIN, main.height - columns_need);
        let [top, p] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(preview_h)]).areas(main);
        let fw = share(top.width, sizes.narrow_fonts_width)
            .max(FONTS_W)
            .min(top.width - COLOURS_W);
        let [f, c] = Layout::horizontal([Constraint::Length(fw), Constraint::Min(0)]).areas(top);
        borders.push(h_border(
            BorderId::TopPreview,
            top,
            main.y + main.height,
            main.height,
            true,
        ));
        borders.push(v_border(BorderId::NarrowFontsColours, f, top.x, top.width));
        (f, c, p)
    };
    // fonts column: roles, list, sample
    let sample_h = if sample {
        share(fonts_col.height, sizes.sample_height)
            .clamp(SAMPLE_MIN, fonts_col.height - ROLES_H - FONT_LIST_MIN)
    } else {
        0
    };
    let [roles, font_list, sample_area] = Layout::vertical([
        Constraint::Length(ROLES_H),
        Constraint::Min(0),
        Constraint::Length(sample_h),
    ])
    .areas(fonts_col);
    if sample {
        borders.push(h_border(
            BorderId::ListSample,
            font_list,
            fonts_col.y + fonts_col.height,
            fonts_col.height,
            true,
        ));
    }
    // colour column: list, picker (automatic size unless resized)
    let picker_h = if !picker {
        0
    } else if sizes.picker_height > 0.0 {
        share(colours_col.height, sizes.picker_height)
            .clamp(PICKER_MIN, colours_col.height - LIST_MIN)
    } else {
        colours_col
            .height
            .saturating_sub(LIST_FULL)
            .clamp(PICKER_MIN, PICKER_MAX)
            .min(colours_col.height - LIST_MIN)
    };
    let [colour_list, picker_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(picker_h)]).areas(colours_col);
    if picker {
        borders.push(h_border(
            BorderId::ListPicker,
            colour_list,
            colours_col.y + colours_col.height,
            colours_col.height,
            true,
        ));
    }
    // between nvim and the shell (inside the preview's frame, above the bar)
    if show_preview {
        let desk = crate::preview::desk_area(preview);
        let ((a, _), stacked) = crate::preview::split_windows(desk, wide, sizes.preview_split);
        borders.push(if stacked {
            h_border(BorderId::PreviewSplit, a, desk.y, desk.height, false)
        } else {
            v_border(BorderId::PreviewSplit, a, desk.x, desk.width)
        });
    }
    Arrangement::Main(MainLayout {
        roles,
        font_list,
        sample: sample.then_some(sample_area),
        colour_list,
        picker: picker.then_some(picker_area),
        preview,
        status,
        wide,
        borders,
    })
}

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    // images still waiting for a size to settle set this again
    app.redraw_at = None;
    // set again if the Palette pane is drawn (the palette list hangs from it)
    app.palette_pane = None;
    app.hover_note = None;
    crate::hit::set_looks(app.looks(&app.settings));
    let full = f.area();
    // tabs on top: their own row; at the bottom they share the status line
    let top = app.settings.layout.tabs_on_top();
    let (tabs_row, area) = if top && full.height > 1 {
        let [t, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(full);
        (Some(t), rest)
    } else {
        (None, full)
    };
    let arrangement = arrange(
        area,
        app.page,
        app.picker_visible,
        app.sample_visible,
        app.main_preview_visible,
        &app.sizes,
    );
    if let Arrangement::TooSmall { need_w, need_h } = arrangement {
        return draw_too_small(f, full, need_w, need_h + top as u16);
    }
    if let Some(t) = tabs_row {
        draw_tabs(f, app, t);
    }
    match arrangement {
        Arrangement::TooSmall { .. } => {}
        Arrangement::Page { body, status } => {
            if app.page == Page::Downloads {
                crate::fonts_dl_ui::draw_downloads(f, app, body);
            } else if app.page == Page::Settings {
                // as edited; Save / Discard drawn with what's live
                let changes = app.draft_changes();
                app.show_draft(|app| crate::settings_ui::draw_settings(f, app, body));
                crate::settings_ui::draw_save_buttons(f, app, body, changes);
            } else {
                draw_page(f, app, body);
            }
            draw_status(f, app, status);
        }
        Arrangement::Main(l) => {
            app.wide = l.wide;
            crate::fonts_ui::draw_fonts(f, app, l.roles, l.font_list);
            if let Some(s) = l.sample {
                crate::fonts_ui::draw_sample(f, app, s);
            }
            draw_list(f, app, l.colour_list);
            if let Some(p) = l.picker {
                draw_picker(f, app, p);
            }
            // hidden: nothing drawn, and nvim and the shell keep their size
            if !l.preview.is_empty() {
                draw_preview(f, app, l.preview, l.wide);
            }
            // draggable borders: highlighted under the mouse and while dragged
            for b in &l.borders {
                if app.preview.mode == PreviewMode::Swatches && b.id == BorderId::PreviewSplit {
                    continue;
                }
                let hovered = app.hits.add(b.hit, Target::Border(b.id));
                if hovered || app.dragging == Some(b.id) {
                    let buf = f.buffer_mut();
                    for x in b.hit.x..b.hit.x + b.hit.width {
                        for y in b.hit.y..b.hit.y + b.hit.height {
                            if let Some(c) = buf.cell_mut((x, y)) {
                                c.set_fg(Color::Yellow)
                                    .set_style(Style::new().add_modifier(Modifier::BOLD));
                            }
                        }
                    }
                }
            }
            app.borders = l.borders.clone();
            draw_status(f, app, l.status);
        }
    }
    draw_overlay(f, app, full);
    draw_pending(f, app, full);
    draw_notice(f, app, full);
}

/// A save confirmation in a box (settings.messages.saved = "popup").
fn draw_notice(f: &mut Frame, app: &App, area: Rect) {
    let Some(text) = &app.notice else {
        return;
    };
    let hint = "press any key to dismiss";
    let w = text.chars().count().max(hint.len()) as u16 + 4;
    let lines = vec![
        Line::styled(format!(" {text}"), Style::new().green().bold()),
        Line::raw(""),
        Line::styled(format!(" {hint}"), Style::new().dark_gray()),
    ];
    let r = centered(area, w, 5);
    f.render_widget(Clear, r);
    let block = Block::bordered().border_style(Style::new().green());
    f.render_widget(Paragraph::new(lines).block(block), r);
}

/// The page tabs (F1 Main, F2 Fonts, F3 Settings), clickable; returns the
/// width used.
fn draw_tabs(f: &mut Frame, app: &mut App, area: Rect) -> u16 {
    let mut x = area.x;
    for (label, action, page) in [
        ("Main", Action::PageMain, Page::Main),
        ("Fonts", Action::PageDownloads, Page::Downloads),
        ("Settings", Action::PageSettings, Page::Settings),
    ] {
        let key = app
            .keymap
            .key_for(&[Context::Global], action)
            .map(|k| friendly(&k))
            .unwrap_or_default();
        // a dot on Settings while it has unsaved changes
        let dot = if page == Page::Settings && app.settings_page.draft.is_some() {
            " ●"
        } else {
            ""
        };
        let text = format!(" {key} {label}{dot} ");
        let w = (text.chars().count() as u16).min((area.x + area.width).saturating_sub(x));
        let r = Rect {
            x,
            y: area.y,
            width: w,
            height: 1,
        };
        // with the keyboard on the tabs, the current one is yellow (like a
        // page's own tabs)
        let mut style = match (app.page == page, app.page_tabs_focused) {
            (true, true) => Style::new().black().on_yellow().bold(),
            (true, false) => Style::new().reversed().bold(),
            _ => Style::new().dark_gray(),
        };
        if app.hits.add(r, Target::PageTab(page)) {
            style = hover(style);
        }
        f.render_widget(Paragraph::new(Span::styled(text, style)), r);
        x += w;
    }
    // the theme: its name and unsaved dot (a click opens the theme list)
    let dot = if app.theme_dirty() { " ●" } else { "" };
    let label = format!("  Theme: {}{dot} ", app.theme);
    let w = (label.chars().count() as u16).min((area.x + area.width).saturating_sub(x));
    if w > 0 {
        let r = Rect {
            x,
            y: area.y,
            width: w,
            height: 1,
        };
        let mut line = Line::from(vec![
            Span::raw("  "),
            Span::styled(format!("Theme: {}", app.theme), Style::new().bold()),
            Span::styled(dot, Style::new().yellow()),
            Span::raw(" "),
        ]);
        if app.hits.add(r, Target::ThemeName) {
            line = crate::hit::hover_line(line, 1);
        }
        f.render_widget(Paragraph::new(line), r);
        x += w;
    }
    x - area.x
}

/// The Download fonts page (filled in by phase 6).
fn draw_page(f: &mut Frame, app: &App, area: Rect) {
    let (title, text) = match app.page {
        Page::Downloads => (
            " Download fonts ",
            vec![
                "Searching and installing fonts from Google Fonts and Nerd Fonts comes here (plan phase 6).".to_string(),
            ],
        ),
        _ => (" Settings ", Vec::new()),
    };
    let lines: Vec<Line> = text
        .into_iter()
        .map(|t| Line::raw(format!(" {t}")))
        .collect();
    let block = Block::bordered()
        .title(title)
        .border_style(Style::new().yellow());
    f.render_widget(Paragraph::new(lines).block(block), area);
}
/// btop-style: the current size (red where it falls short) and what's needed.
fn draw_too_small(f: &mut Frame, area: Rect, need_w: u16, need_h: u16) {
    let ok = |have: u16, need: u16| {
        if have >= need {
            Style::new().green()
        } else {
            Style::new().red().bold()
        }
    };
    let lines = vec![
        Line::styled("Window too small", Style::new().bold()),
        Line::from(vec![
            "Width = ".into(),
            Span::styled(area.width.to_string(), ok(area.width, need_w)),
            "  Height = ".into(),
            Span::styled(area.height.to_string(), ok(area.height, need_h)),
        ]),
        Line::raw(""),
        Line::styled("Needed:", Style::new().bold()),
        Line::raw(format!("Width = {need_w}  Height = {need_h}")),
    ];
    let h = (lines.len() as u16).min(area.height);
    let r = Rect {
        x: area.x,
        y: area.y + (area.height - h) / 2,
        width: area.width,
        height: h,
    };
    f.render_widget(Paragraph::new(lines).centered(), r);
}

fn focused_block(app: &App, area: Rect, title: Line<'static>, focused: bool) -> Block<'static> {
    Block::bordered()
        .title(title)
        .border_style(pane_border(app, area, focused))
}

/// A pane's border: yellow when it has the keyboard, the hover colour when
/// the mouse is over it, else grey.
pub fn pane_border(app: &App, area: Rect, focused: bool) -> Style {
    if focused {
        Style::new().yellow()
    } else if app.pane_hovered(area) {
        Style::new().fg(crate::hit::looks().pane)
    } else {
        Style::new().dark_gray()
    }
}

/// A pane title that focuses the pane when clicked (hover-highlighted).
pub fn pane_title(app: &mut App, area: Rect, title: Line<'static>, pane: u8) -> Line<'static> {
    let r = Rect {
        x: area.x + 1,
        y: area.y,
        width: (title.width() as u16).min(area.width.saturating_sub(2)),
        height: 1,
    };
    if app.hits.add(r, Target::PaneTitle(pane)) {
        title.patch_style(hover(Style::new()))
    } else {
        title
    }
}

/// A swatch with a thin frame, visible even when it matches the background.
fn framed(c: Rgb) -> Span<'static> {
    Span::styled("▕    ▏", Style::new().bg(c.color()).fg(Color::DarkGray))
}

/// A swatch: `width` cells of background colour.
fn swatch(c: Rgb, width: usize) -> Span<'static> {
    Span::styled(" ".repeat(width), Style::new().bg(c.color()))
}

// ----- key list -----------------------------------------------------------

fn draw_list(f: &mut Frame, app: &mut App, area: Rect) {
    app.palette_pane = Some(area);
    // the title names the palette; the dropdown is the pane's first row
    let dot = if app.dirty() {
        Span::styled(" ●", Style::new().yellow())
    } else {
        "".into()
    };
    let title = Line::from(vec![
        panel_number(4),
        "Palette: ".into(),
        app.name.clone().into(),
        dot,
        " ".into(),
    ]);
    let title = pane_title(app, area, title, 4);
    let focused = app.side == Side::Colours
        && app.focus == Focus::List
        && app.preview.focus.is_none()
        && !app.page_tabs_focused;
    let block = focused_block(app, area, title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    let [menu_row, inner] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    draw_palette_menu(f, app, menu_row, focused && app.palette_menu_focused);
    // the colour rows have the cursor unless the dropdown row does
    let focused = focused && !app.palette_menu_focused;

    let bg0 = app.palette.get("bg0");
    let copy_target = match app.overlay {
        Overlay::CopyFrom { target } => Some(target),
        _ => None,
    };
    let rows = app.colour_rows();
    // the selected colour's relatives by derivation (pinned keys are set by
    // hand, so they're left out) get the derived-colour highlight
    let selected = app.keys[app.sel];
    let related: Vec<&str> = if app.palette.is_pinned(selected) {
        Vec::new()
    } else {
        palette::related(selected)
            .into_iter()
            .filter(|k| palette::is_base(k) || !app.palette.is_pinned(k))
            .collect()
    };
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let cursor = r == app.row_sel && (focused || copy_target.is_some());
            let cursor_style = if cursor {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            match *row {
                ColourRow::Group { index, expanded } => {
                    let (name, keys) = GROUPS[index];
                    let line = Line::from(vec![
                        Span::styled(if expanded { " ▾ " } else { " ▸ " }, Style::new().yellow()),
                        Span::styled(name, cursor_style.bold()),
                        Span::styled(format!("  {}", keys.len()), Style::new().dark_gray()),
                    ]);
                    // a closed group stands in for its related keys
                    if !expanded && keys.iter().any(|k| related.contains(k)) {
                        line.patch_style(crate::hit::derived(Style::new()))
                    } else {
                        line
                    }
                }
                ColourRow::Key(i) => {
                    let key = app.keys[i];
                    let c = app.palette.get(key);
                    let pinned = app.palette.is_pinned(key);
                    // the colour the picker shows, when the cursor is elsewhere
                    let name_style = if cursor {
                        cursor_style
                    } else if i == app.sel {
                        Style::new().bold().underlined()
                    } else {
                        Style::new()
                    };
                    let marker = if copy_target == Some(key) {
                        Span::styled("→", Style::new().yellow())
                    } else if pinned {
                        Span::styled("", Style::new().yellow())
                    } else {
                        " ".into()
                    };
                    let mut spans = vec!["  ".into(), marker, " ".into()];
                    // the cursor on the name, not the padding before the swatch
                    spans.extend(crate::hit::cursor_spans(
                        format!("{key:<16}"),
                        Style::new(),
                        name_style,
                    ));
                    spans.push(swatch(c, 4));
                    spans.push(Span::raw(format!(" {} ", c.hex())));
                    let info = if palette::is_base(key) || pinned {
                        String::new()
                    } else {
                        format!("⟵ {}", palette::sources(key))
                    };
                    spans.push(Span::styled(
                        format!("{info:<11}"),
                        Style::new().dark_gray(),
                    ));
                    if TEXT_KEYS.contains(&key) {
                        let ratio = color::contrast(c, bg0);
                        let style = if ratio < 3.0 {
                            Style::new().red().bold()
                        } else if ratio < 4.5 {
                            Style::new().yellow()
                        } else {
                            Style::new().dark_gray()
                        };
                        spans.push(Span::styled(format!("{ratio:>4.1}:1"), style));
                    }
                    if related.contains(&key) {
                        Line::from(spans).patch_style(crate::hit::derived(Style::new()))
                    } else {
                        Line::from(spans)
                    }
                }
            }
        })
        .collect();

    // Scroll so the cursor stays visible.
    let h = inner.height as usize;
    if app.row_sel < app.list_scroll {
        app.list_scroll = app.row_sel;
    } else if h > 0 && app.row_sel >= app.list_scroll + h {
        app.list_scroll = app.row_sel + 1 - h;
    }
    app.list_scroll = app.list_scroll.min(rows.len().saturating_sub(h.max(1)));
    let shown: Vec<Line> = lines.into_iter().skip(app.list_scroll).take(h).collect();
    // each row is clickable; the one under the mouse is highlighted
    let shown: Vec<Line> = shown
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            let r = Rect {
                y: inner.y + i as u16,
                height: 1,
                ..inner
            };
            if app.hits.add(r, Target::ColourRow(app.list_scroll + i)) {
                // only the group's or key's name is underlined
                let name = match rows.get(app.list_scroll + i) {
                    Some(ColourRow::Group { .. }) => 1,
                    _ => 3,
                };
                crate::hit::hover_line(line, name)
            } else {
                line
            }
        })
        .collect();
    f.render_widget(Paragraph::new(shown), inner);
}

/// The Palette pane's dropdown row: " CRT Amber ▾ " and the palette's base
/// colours. Enter (with the keyboard on it) or a click opens the list.
fn draw_palette_menu(f: &mut Frame, app: &mut App, area: Rect, cursor: bool) {
    let text = format!(" {} ▾ ", app.name);
    let w = (text.chars().count() as u16).min(area.width);
    let hit = Rect { width: w, ..area };
    let base = Style::new().yellow().bold();
    let mut spans = if cursor {
        crate::hit::cursor_spans(text, base, Style::new().reversed())
    } else {
        vec![Span::styled(text, base)]
    };
    if app.hits.add(hit, Target::PaletteMenu) && !cursor {
        spans = vec![Span::styled(spans[0].content.to_string(), hover(base))];
    }
    spans.push(" ".into());
    for k in [
        "bg0", "fg0", "red", "orange", "yellow", "green", "aqua", "blue", "purple",
    ] {
        spans.push(swatch(app.palette.get(k), 2));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ----- picker -------------------------------------------------------------

fn set_cell(buf: &mut Buffer, x: u16, y: u16, sym: &str, fg: Rgb, bg: Rgb) {
    if let Some(cell) = buf.cell_mut((x, y)) {
        cell.set_symbol(sym).set_fg(fg.color()).set_bg(bg.color());
    }
}

fn draw_picker(f: &mut Frame, app: &mut App, area: Rect) {
    let key = app.key();
    let state = if palette::is_base(key) {
        Span::styled(" base ", Style::new().dark_gray())
    } else if app.palette.is_pinned(key) {
        Span::styled("  pinned (p to follow again) ", Style::new().yellow())
    } else {
        Span::styled(
            format!(" follows {} until edited ", palette::sources(key)),
            Style::new().dark_gray(),
        )
    };
    let title = Line::from(vec![
        panel_number(5),
        "Colour: ".into(),
        Span::styled(key, Style::new().bold()),
        " ".into(),
        state,
    ]);
    let focused = app.side == Side::Colours
        && !matches!(app.focus, Focus::List)
        && app.preview.focus.is_none();
    let title = pane_title(app, area, title, 5);
    let block = focused_block(app, area, title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Colour square rows: whatever the 3 sliders, a gap and the hex row leave
    let rows: u16 = inner.height.saturating_sub(5).max(2);
    let sq = Rect {
        x: inner.x + 1,
        y: inner.y,
        width: inner.width.saturating_sub(7),
        height: rows,
    };
    let hue = Rect {
        x: sq.x + sq.width + 1,
        y: inner.y,
        width: 2,
        height: rows,
    };
    let (h, s, v) = app.hsv;
    let current = app.palette.get(key);
    // clickable parts (registered before drawing, so hover can be shown)
    let track_w = inner.width.saturating_sub(10);
    let track = |row: u16| Rect {
        x: inner.x + 3,
        y: inner.y + rows + row,
        width: track_w,
        height: 1,
    };
    let sq_hover = app.hits.add(sq, Target::Square);
    let hue_hover = app.hits.add(hue, Target::Hue);
    let slider_hover = [
        app.hits.add(track(0), Target::SliderH),
        app.hits.add(track(1), Target::SliderS),
        app.hits.add(track(2), Target::SliderL),
    ];
    let hex_hover = app.hits.add(
        Rect {
            x: inner.x,
            y: inner.y + rows + 4,
            width: 14,
            height: 1,
        },
        Target::Hex,
    );
    let mouse = app.hits.mouse;
    let buf = f.buffer_mut();

    // Saturation (left to right) × value (top to bottom), two pixels per cell
    let px = (rows * 2 - 1) as f64;
    for cy in 0..rows {
        for cx in 0..sq.width {
            let sx = cx as f64 / (sq.width - 1).max(1) as f64;
            let top = color::hsv_to_rgb(h, sx, 1.0 - (2 * cy) as f64 / px);
            let bot = color::hsv_to_rgb(h, sx, 1.0 - (2 * cy + 1) as f64 / px);
            set_cell(buf, sq.x + cx, sq.y + cy, "▀", top, bot);
        }
    }
    let mx = sq.x + (s * (sq.width - 1) as f64).round() as u16;
    let my = sq.y + (((1.0 - v) * px).round() as u16 / 2).min(rows - 1);
    let marker = if app.focus == Focus::Square {
        "◉"
    } else {
        "○"
    };
    set_cell(buf, mx, my, marker, current.text_on(), current);
    // hovering the square: a crosshair where a click would pick
    if sq_hover && let Some((x, y)) = mouse {
        let under = buf
            .cell((x, y))
            .and_then(|c| match c.fg {
                Color::Rgb(r, g, b) => Some(Rgb(r, g, b)),
                _ => None,
            })
            .unwrap_or(current);
        set_cell(buf, x, y, "┼", under.text_on(), under);
    }

    // Hue strip
    for cy in 0..rows {
        let top = color::hsv_to_rgb((2 * cy) as f64 / px * 360.0, 1.0, 1.0);
        let bot = color::hsv_to_rgb((2 * cy + 1) as f64 / px * 360.0, 1.0, 1.0);
        for cx in 0..hue.width {
            set_cell(buf, hue.x + cx, hue.y + cy, "▀", top, bot);
        }
    }
    let hy = hue.y + ((h / 360.0 * px).round() as u16 / 2).min(rows - 1);
    if let Some(cell) = buf.cell_mut((hue.x + hue.width, hy)) {
        cell.set_symbol("◀");
        cell.set_style(if app.focus == Focus::Hue {
            Style::new().yellow()
        } else {
            Style::new()
        });
    }
    // hovering the hue strip: a pale marker at the hue a click would pick
    if hue_hover
        && let Some((_, y)) = mouse
        && let Some(cell) = buf.cell_mut((hue.x + hue.width, y))
        && cell.symbol() != "◀"
    {
        cell.set_symbol("◁").set_style(Style::new().dark_gray());
    }

    // H, S, L sliders, each showing what moving it would give
    let (sl_s, sl_l) = app.hsl();
    let sliders: [(Focus, &str, f64, String); 3] = [
        (
            Focus::SliderH,
            "H",
            h / 360.0,
            format!("{:>3}°", h.round() as i32),
        ),
        (
            Focus::SliderS,
            "S",
            sl_s,
            format!("{:>3}%", (sl_s * 100.0).round() as i32),
        ),
        (
            Focus::SliderL,
            "L",
            sl_l,
            format!("{:>3}%", (sl_l * 100.0).round() as i32),
        ),
    ];
    for (row, (focus, label, t, value)) in sliders.into_iter().enumerate() {
        let y = inner.y + rows + row as u16;
        let mut label_style = if app.focus == focus {
            Style::new().yellow().bold()
        } else {
            Style::new()
        };
        if slider_hover[row] {
            label_style = hover(label_style);
        }
        if let Some(cell) = buf.cell_mut((inner.x + 1, y)) {
            cell.set_symbol(label).set_style(label_style);
        }
        let track = Rect {
            x: inner.x + 3,
            y,
            width: track_w,
            height: 1,
        };
        for cx in 0..track.width {
            let u = cx as f64 / (track.width - 1).max(1) as f64;
            let c = match focus {
                Focus::SliderH => color::hsv_to_rgb(u * 360.0, 0.85, 0.9),
                Focus::SliderS => color::hsl_to_rgb(h, u, sl_l),
                _ => color::hsl_to_rgb(h, sl_s, u),
            };
            set_cell(buf, track.x + cx, y, " ", c, c);
        }
        let tx = track.x + (t * (track.width - 1) as f64).round() as u16;
        let under = buf.cell((tx, y)).map(|c| c.bg).unwrap_or(Color::Reset);
        let under = match under {
            Color::Rgb(r, g, b) => Rgb(r, g, b),
            _ => Rgb(0, 0, 0),
        };
        set_cell(buf, tx, y, "┃", under.text_on(), under);
        buf.set_string(track.x + track.width + 1, y, value, label_style);
    }

    // Hex input, and the colour before and after editing
    let y = inner.y + rows + 4;
    let editing = app.focus == Focus::Hex;
    let hex_style = if editing {
        Style::new().yellow().underlined()
    } else {
        Style::new()
    };
    let hex_text = if editing {
        format!("{:<7}▏", app.hex_input)
    } else {
        format!("{:<8}", current.hex())
    };
    let old = app.saved.get(key);
    let line = Line::from(vec![
        Span::styled(" hex ", {
            let s = if editing {
                Style::new().yellow().bold()
            } else {
                Style::new()
            };
            if hex_hover { hover(s) } else { s }
        }),
        Span::styled(hex_text, hex_style),
        "   saved ".dark_gray(),
        framed(old),
        "  now ".dark_gray(),
        framed(current),
    ]);
    let hex_row = Rect {
        x: inner.x,
        y,
        width: inner.width,
        height: 1,
    };
    f.render_widget(Paragraph::new(line), hex_row);
}

// ----- preview (simple, until the live desktop preview) --------------------

/// A line of sample code: (text, palette key) tokens, and an optional line background key.
type CodeLine<'a> = (Vec<(&'a str, &'a str)>, Option<&'a str>);

// ----- preview ------------------------------------------------------------

/// The live desktop (nvim, shell or yazi, i3 borders and bar, rofi), or the
/// swatch view.
/// `stack`: nvim above the shell (a preview column); otherwise side by side.
fn draw_preview(f: &mut Frame, app: &mut App, area: Rect, stack: bool) {
    if app.preview.mode == PreviewMode::Swatches {
        return draw_swatches(f, app, area);
    }
    let key = |a: Action| {
        app.keymap
            .key_for(&[Context::Global], a)
            .map(|k| friendly(&k))
            .unwrap_or_default()
    };
    // While nvim or the shell has the keyboard the frame turns magenta (your
    // palette's purple, as the terminal shows it): keys go there, not to the
    // studio, until F12 hands them back.
    let capture = Style::new().magenta();
    let title = match app.preview.focus {
        Some(id) => Line::from(vec![
            Span::styled(" Preview: typing into ", capture),
            Span::styled(format!("{id:?}"), capture.bold()),
            Span::styled(
                format!(
                    "  {} next window / back to the studio ",
                    key(Action::PreviewCycle)
                ),
                capture,
            ),
        ]),
        None => Line::raw(format!(
            " Preview  {} / {}, {} or click: use a window   Y yazi   R rofi   V swatches ",
            key(Action::Panel(6)),
            key(Action::Panel(7)),
            key(Action::PreviewCycle)
        )),
    };
    let block = match app.preview.focus {
        Some(_) => Block::bordered().title(title).border_style(capture.bold()),
        None => focused_block(app, area, title, false),
    };
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.preview.draw_desktop(
        f,
        inner,
        &app.palette,
        app.terminal.as_ref(),
        &app.name,
        crate::preview::Windows {
            stack,
            split: app.sizes.preview_split,
        },
    );
    // nvim and the shell are clickable (a click hands them the keyboard);
    // hovering one lights its frame in the capture colour
    let rects = app.preview.rects.clone();
    for (id, r) in rects {
        if app.hits.add(r, Target::Preview(id)) && app.preview.focus != Some(id) {
            let frame = crate::hit::looks().preview;
            let buf = f.buffer_mut();
            for x in r.x..r.x + r.width {
                for y in [r.y, r.y + r.height.saturating_sub(1)] {
                    if let Some(c) = buf.cell_mut((x, y)) {
                        c.set_fg(frame);
                    }
                }
            }
            for y in r.y..r.y + r.height {
                for x in [r.x, r.x + r.width.saturating_sub(1)] {
                    if let Some(c) = buf.cell_mut((x, y)) {
                        c.set_fg(frame);
                    }
                }
            }
        }
    }
}

/// Every key as a swatch, plus static samples (V switches to this view).
fn draw_swatches(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;
    let block = Block::bordered()
        .title(" Preview ")
        .border_style(Style::new().dark_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let bg = p.get("bg0");
    f.render_widget(
        Block::new().style(Style::new().bg(bg.color()).fg(p.get("fg0").color())),
        inner,
    );

    let fg = |k: &str| Style::new().fg(p.get(k).color());
    let mut lines: Vec<Line> = Vec::new();
    let heading = |t: &str| Line::styled(format!(" {t}"), fg("grey1").bold());

    // Terminal colours, as Ghostty gets them (terminal-colors.json)
    lines.push(heading("Terminal colours"));
    if let Some(map) = &app.terminal {
        for half in [0..8, 8..16] {
            let mut spans: Vec<Span> = vec!["  ".into()];
            for i in half {
                let c = map
                    .ansi
                    .get(i)
                    .and_then(|k| palette::key(k))
                    .map(|k| p.get(k))
                    .unwrap_or(bg);
                spans.push(Span::styled(format!("{i:>2} "), fg("grey0")));
                spans.push(swatch(c, 4));
                spans.push(" ".into());
            }
            lines.push(Line::from(spans));
        }
        let sample: Vec<Span> = ["red", "green", "yellow", "blue", "purple", "aqua"]
            .iter()
            .map(|k| Span::styled(format!(" {k}"), fg(k)))
            .collect();
        let term = Style::new()
            .fg(palette::key(&map.foreground)
                .map_or(bg, |k| p.get(k))
                .color())
            .bg(palette::key(&map.background)
                .map_or(bg, |k| p.get(k))
                .color());
        let mut spans = vec![Span::styled("  $ ls --color ", term)];
        spans.extend(sample);
        lines.push(Line::from(spans));
    } else {
        lines.push(Line::styled(
            "  (terminal-colors.json not found)",
            fg("red"),
        ));
    }
    lines.push(Line::raw(""));

    // Code as nvim shows it (mini.base16 mapping from nvim/lua/theme.lua)
    lines.push(heading("Editor"));
    let num = |n: u32, cur: bool| {
        Span::styled(
            format!("  {n:>3} "),
            if cur { fg("yellow") } else { fg("grey0") },
        )
    };
    let code: Vec<CodeLine> = vec![
        (
            vec![("-- Re-apply when the colours file changes", "grey1")],
            None,
        ),
        (
            vec![
                ("local ", "purple"),
                ("function ", "purple"),
                ("apply", "blue"),
                ("(", "grey2"),
                ("path", "red"),
                (", ", "grey2"),
                ("opts", "red"),
                (")", "grey2"),
            ],
            None,
        ),
        (
            vec![
                ("  if not ", "purple"),
                ("opts", "red"),
                (".", "grey2"),
                ("force ", "red"),
                ("then return ", "purple"),
                ("nil ", "orange"),
                ("end", "purple"),
            ],
            Some("bg1"),
        ),
        (
            vec![
                ("  local ", "purple"),
                ("count", "red"),
                (" = ", "fg0"),
                ("42", "orange"),
                (" + ", "fg0"),
                ("#", "aqua"),
                ("path", "red"),
            ],
            None,
        ),
        (
            vec![
                ("  vim", "red"),
                (".", "grey2"),
                ("notify", "blue"),
                ("(", "grey2"),
                ("\"theme applied\"", "green"),
                (", ", "grey2"),
                ("Level", "yellow"),
                (".", "grey2"),
                ("INFO", "orange"),
                (")", "grey2"),
            ],
            Some("bg3"),
        ),
        (vec![("end", "purple")], None),
    ];
    for (n, (tokens, line_bg)) in code.into_iter().enumerate() {
        let mut spans = vec![num(n as u32 + 1, line_bg == Some("bg1"))];
        for (text, k) in tokens {
            let mut st = fg(k);
            if let Some(b) = line_bg {
                st = st.bg(p.get(b).color());
            }
            spans.push(Span::styled(text, st));
        }
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(vec![
        "  ".into(),
        Span::styled(
            " NORMAL ",
            Style::new()
                .bg(p.get("green").color())
                .fg(bg.color())
                .bold(),
        ),
        Span::styled(
            " theme.lua ",
            Style::new()
                .bg(p.get("bg_statusline2").color())
                .fg(p.get("fg0").color()),
        ),
        Span::styled(
            " ".repeat(20),
            Style::new().bg(p.get("bg_statusline1").color()),
        ),
        Span::styled(
            " 3:14 ",
            Style::new()
                .bg(p.get("bg_statusline3").color())
                .fg(p.get("fg0").color()),
        ),
    ]));
    lines.push(Line::raw(""));

    // i3 window borders and bar (i3/theme roles)
    lines.push(heading("Window borders and bar"));
    lines.push(Line::from(vec![
        "  ".into(),
        Span::styled(
            " focused ",
            Style::new().bg(p.get("yellow").color()).fg(bg.color()),
        ),
        "  ".into(),
        Span::styled(
            " inactive ",
            Style::new()
                .bg(p.get("bg3").color())
                .fg(p.get("fg0").color()),
        ),
        "  ".into(),
        Span::styled(
            " urgent ",
            Style::new()
                .bg(p.get("red").color())
                .fg(p.get("fg0").color()),
        ),
        "  ".into(),
        Span::styled(
            " 1 ",
            Style::new().bg(p.get("yellow").color()).fg(bg.color()),
        ),
        Span::styled(
            " 2 ",
            Style::new()
                .bg(p.get("bg3").color())
                .fg(p.get("fg0").color()),
        ),
        Span::styled(" 3 ", fg("grey2")),
        Span::styled("   vol 40%  wifi  17:42", fg("fg0")),
    ]));
    lines.push(Line::raw(""));

    // Every key
    lines.push(heading("All colours"));
    let cols = ((inner.width.saturating_sub(2)) / 24).max(1) as usize;
    let all: Vec<&str> = GROUPS
        .iter()
        .flat_map(|(_, ks)| ks.iter().copied())
        .collect();
    for chunk in all.chunks(cols) {
        let mut spans: Vec<Span> = vec!["  ".into()];
        for k in chunk {
            spans.push(swatch(p.get(k), 3));
            spans.push(Span::styled(
                format!(" {k:<19}"),
                if *k == app.key() {
                    fg("yellow").bold()
                } else {
                    fg("grey2")
                },
            ));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ----- status line --------------------------------------------------------

fn draw_status(f: &mut Frame, app: &mut App, area: Rect) {
    // the page tabs start the line unless they're on top (settings: layout.tabs)
    let used = if app.settings.layout.tabs_on_top() {
        0
    } else {
        draw_tabs(f, app, area)
    };
    let area = Rect {
        x: area.x + used,
        width: area.width.saturating_sub(used),
        ..area
    };
    let app = &*app;
    let fonts = app.side == Side::Fonts;
    // the message at the left (after the tabs), the key hints at the right
    let message = app
        .message
        .as_ref()
        .map(|(msg, err)| {
            Span::styled(
                truncate(&format!(" {msg} "), area.width as usize),
                if *err {
                    Style::new().red().bold()
                } else {
                    Style::new().green()
                },
            )
        })
        // otherwise, a note about what the mouse is over
        .or_else(|| {
            app.hover_note.as_ref().map(|n| {
                Span::styled(
                    truncate(&format!(" {n} "), area.width as usize),
                    Style::new().dark_gray(),
                )
            })
        })
        // otherwise, previews being fetched ahead of time
        .or_else(|| {
            let p = &app.prefetch;
            p.active().then(|| {
                Span::styled(
                    format!(" fetching previews {}/{} ", p.done, p.total),
                    Style::new().dark_gray(),
                )
            })
        });
    let msg_w = message.as_ref().map_or(0, |s| s.width());
    let hints = {
        use Action as A;
        let hint = |actions: &[Action], label: &str| -> Option<String> {
            let ctx = app.contexts();
            let keys: Vec<String> = actions
                .iter()
                .filter_map(|a| app.keymap.key_for(&ctx, *a))
                .collect();
            let keys: Vec<String> = keys.iter().map(|k| friendly(k)).collect();
            (!keys.is_empty()).then(|| format!("{} {label}", keys.join("/")))
        };
        // picking for a role: only the chips that aren.t locked
        let chips: Vec<Action> = crate::fonts::CHIPS
            .iter()
            .filter(|(c, _)| app.fonts.filters.chip_free(*c))
            .map(|(c, _)| A::Filter(*c))
            .collect();
        let context: Vec<String> = if app.resize_mode {
            let k = |a| {
                app.keymap
                    .key_for(&[Context::Resize], a)
                    .map(|s| friendly(&s))
                    .unwrap_or_default()
            };
            vec![
                "RESIZE".into(),
                format!("{}/{} side edge", k(A::Left), k(A::Right)),
                format!("{}/{} lower edge", k(A::Down), k(A::Up)),
                "capitals by 5".into(),
                format!("{} reset", k(A::ResetSizes)),
                format!("{} done", k(A::Back)),
            ]
        } else if app.page_tabs_focused {
            let back = if app.settings.layout.tabs_on_top() {
                A::Down
            } else {
                A::Up
            };
            [
                hint(&[A::Left, A::Right], "pages"),
                hint(&[back], "back to the page"),
            ]
            .into_iter()
            .flatten()
            .collect()
        } else if app.page == Page::Main && app.palette_menu_focused && app.side == Side::Colours {
            [hint(&[A::Confirm], "palettes"), hint(&[A::Down], "colours")]
                .into_iter()
                .flatten()
                .collect()
        } else if app.page == Page::Downloads && app.dl.searching {
            vec!["type to search".into(), "Enter/Esc done".into()]
        } else if app.page == Page::Downloads
            && app.dl.tab == crate::fonts_dl::DlTab::Tournament
            && !app.dl.tabs_focused
        {
            use crate::tour_page::Stage;
            let stage = app.tour_page.as_ref().map_or(Stage::Setup, |t| t.stage);
            let bracket = app.tour_page.as_ref().is_some_and(|t| t.bracket);
            let rows: Vec<(&[Action], &str)> = match stage {
                _ if bracket => vec![(&[A::Down, A::Up], "scroll"), (&[A::Back], "close")],
                Stage::Setup => vec![
                    (&[A::Down, A::Up], "move"),
                    (&[A::Confirm], "start / open"),
                    (&[A::Left, A::Right], "pool size"),
                    (&[A::Delete], "delete"),
                ],
                Stage::Fetching => vec![(&[A::Back], "stop")],
                Stage::Playing => vec![
                    (&[A::Left, A::Right], "pick"),
                    (&[A::TourUndo], "take back"),
                    (&[A::TourNames], "names"),
                    (&[A::TourBracket], "bracket"),
                    (&[A::PreviewText], "code/prose"),
                    (&[A::PreviewLang], "language"),
                    (&[A::SizeUp, A::SizeDown], "size"),
                    (&[A::Back], "leave"),
                ],
                Stage::Results => vec![
                    (&[A::Down, A::Up], "move"),
                    (&[A::Confirm], "install"),
                    (
                        &[A::UseFor(0), A::UseFor(1), A::UseFor(2)],
                        "use for a role",
                    ),
                    (&[A::FillFamily], "every role"),
                    (&[A::TourBracket], "bracket"),
                    (&[A::Back], "leave"),
                ],
            };
            rows.into_iter().filter_map(|(a, l)| hint(a, l)).collect()
        } else if app.page == Page::Downloads {
            let mut v: Vec<String> = [
                hint(&[A::Down, A::Up], "move"),
                hint(&[A::Confirm], "install"),
                hint(&[A::Delete], "remove"),
                hint(&[A::Search], "search"),
            ]
            .into_iter()
            .flatten()
            .collect();
            if app.dl.tab == crate::fonts_dl::DlTab::Google {
                v.extend(hint(&[A::Left, A::Right], "category"));
            }
            v.extend(hint(&[A::TabPrev, A::TabNext], "tabs"));
            v
        } else if app.page == Page::Settings && app.settings_page.tabs_focused {
            let back = if app.settings.layout.inner_tabs_on_top() {
                A::Down
            } else {
                A::Up
            };
            [
                hint(&[A::Left, A::Right], "tabs"),
                hint(&[back], "back to the list"),
            ]
            .into_iter()
            .flatten()
            .collect()
        } else if app.page == Page::Settings {
            use crate::settings::Tab;
            let rows: Vec<(&[Action], &str)> = match app.settings_page.tab {
                Tab::Keymaps if app.settings_page.browse.is_some() => vec![
                    (&[A::Down, A::Up], "presets"),
                    (&[A::Confirm], "load"),
                    (&[A::SaveNamed], "new from it"),
                    (&[A::Delete], "delete"),
                    (&[A::ToggleAll], "everything"),
                    (&[A::Back], "back"),
                ],
                Tab::Keymaps => vec![
                    (&[A::Down, A::Up], "move"),
                    (&[A::Confirm], "rebind"),
                    (&[A::Add], "add keys"),
                    (&[A::Delete], "unbind"),
                    (&[A::SaveNamed], "save preset"),
                ],
                Tab::Behaviour => vec![
                    (&[A::Down, A::Up], "move"),
                    (&[A::Left, A::Right], "change"),
                ],
                Tab::History => vec![
                    (&[A::Left, A::Right], "files/versions"),
                    (&[A::Restore], "restore"),
                    (&[A::SaveNamed], "name"),
                    (&[A::ToggleAll], "everything"),
                    (&[A::Delete], "delete named"),
                ],
            };
            let mut v: Vec<String> = rows.into_iter().filter_map(|(a, l)| hint(a, l)).collect();
            v.extend(hint(&[A::TabPrev, A::TabNext], "tabs"));
            v
        } else if fonts && app.fonts.focus == crate::fonts::FontFocus::Search {
            vec!["type to search".into(), "Enter/Esc done".into()]
        } else if app.preview.focus.is_some() {
            let out = app
                .keymap
                .key_for(&[Context::Global], A::PreviewCycle)
                .unwrap_or_default();
            vec![
                "keys go to the preview window (Ctrl+C too)".into(),
                format!("{} next window / back", friendly(&out)),
            ]
        } else if !fonts && app.focus == Focus::Hex {
            vec![
                "type #rrggbb".into(),
                "Enter set".into(),
                "Esc cancel".into(),
            ]
        } else {
            let rows: Vec<(&[Action], &str)> = if fonts {
                match app.fonts.focus {
                    crate::fonts::FontFocus::Sample => vec![
                        (&[A::ToggleSample], "hide sample"),
                        (&[A::FocusNext], "next pane"),
                    ],
                    crate::fonts::FontFocus::Slots => {
                        let mut v: Vec<(&[Action], &str)> = vec![
                            (&[A::Down, A::Up], "role"),
                            (&[A::Left, A::Right], "size"),
                            (&[A::PickFont], "pick font"),
                        ];
                        // the role.s font doesn.t fit it
                        if app.fonts.misfit(app.fonts.slot) {
                            v.push((&[A::FixRoleFont], "fitting face"));
                        }
                        v
                    }
                    _ => vec![
                        (&[A::Down, A::Up], "browse"),
                        (&[A::Right, A::Left], "open/close"),
                        (&[A::Confirm], "use"),
                        (&[A::FillFamily], "whole family"),
                        (&[A::Search], "search"),
                        (&chips, "filters"),
                        (&[A::Back], "back"),
                    ],
                }
            } else {
                match app.focus {
                    Focus::List => vec![
                        (&[A::Down, A::Up], "move"),
                        (&[A::Confirm], "edit"),
                        (&[A::Pin], "pin"),
                        (&[A::CopyFrom], "copy"),
                        (&[A::HexEntry], "hex"),
                        (&[A::TogglePicker], "picker"),
                    ],
                    Focus::Square => vec![
                        (&[A::Left, A::Down, A::Up, A::Right], "saturation/value"),
                        (&[A::LeftBig, A::DownBig, A::UpBig, A::RightBig], "faster"),
                        (&[A::Back], "list"),
                    ],
                    Focus::Hue => vec![
                        (&[A::Down, A::Up], "hue"),
                        (&[A::DownBig, A::UpBig], "faster"),
                        (&[A::Back], "list"),
                    ],
                    _ => vec![
                        (&[A::Left, A::Right], "adjust"),
                        (&[A::LeftBig, A::RightBig], "faster"),
                        (&[A::Down, A::Up], "next slider"),
                    ],
                }
            };
            rows.into_iter().filter_map(|(a, l)| hint(a, l)).collect()
        };
        let panels = match (hint(&[A::Panel(1)], ""), hint(&[A::Panel(7)], "")) {
            // "Space 1-7 panels" rather than "Space 1-Space 7 panels"
            (Some(a), Some(b)) => {
                let (a, b) = (a.trim(), b.trim());
                let shared = a.len().saturating_sub(1);
                Some(if a.len() == b.len() && a[..shared] == b[..shared] {
                    format!("{a}-{} panels", &b[shared..])
                } else {
                    format!("{a}-{b} panels")
                })
            }
            _ => None,
        };
        // the main page's keys don't work on the other pages
        let main = app.page == Page::Main;
        let general: Vec<String> = [
            hint(&[A::FocusNext], "next").filter(|_| main),
            panels.filter(|_| main),
            hint(&[A::Save], "save").filter(|_| main),
            hint(&[A::Apply], "apply").filter(|_| main),
            hint(&[A::Open], "open").filter(|_| main),
        ]
        .into_iter()
        .flatten()
        .collect();
        // hidden (settings: layout.key-hints, or the toggle): none
        if !app.hints_visible {
            return draw_message_only(f, area, message);
        }
        let last = hint(&[A::Help], "keys").unwrap_or_default();
        // a message keeps a space between it and the hints
        let room = (area.width as usize).saturating_sub(msg_w + usize::from(msg_w > 0));
        fit_hints(&context, &general, &last, room)
    };
    if let Some(m) = message {
        f.render_widget(Paragraph::new(Line::from(m)), area);
    }
    let hints_w = (hints.chars().count() as u16).min(area.width);
    let right = Rect {
        x: area.right() - hints_w,
        width: hints_w,
        ..area
    };
    f.render_widget(
        Paragraph::new(Span::styled(hints, Style::new().dark_gray())),
        right,
    );
}

/// As many hints as fit in `room` columns: the ones for the current control
/// first, then the general ones, and `last` (the help key) always at the end.
/// The status line without key hints: just the message, if any.
fn draw_message_only(f: &mut Frame, area: Rect, message: Option<Span<'static>>) {
    if let Some(m) = message {
        f.render_widget(Paragraph::new(Line::from(m)), area);
    }
}

fn fit_hints(context: &[String], general: &[String], last: &str, room: usize) -> String {
    let mut out = String::from(" ");
    let budget = room.saturating_sub(last.chars().count() + 3);
    let mut general_started = false;
    for (i, hint) in context.iter().chain(general.iter()).enumerate() {
        let sep = if i == 0 {
            ""
        } else if i == context.len() && !general_started {
            general_started = true;
            " │ "
        } else {
            "  "
        };
        if out.chars().count() + sep.chars().count() + hint.chars().count() > budget {
            break;
        }
        out.push_str(sep);
        out.push_str(hint);
    }
    if out.trim().is_empty() {
        out = format!(" {last} ");
    } else {
        out.push_str("  ");
        out.push_str(last);
        out.push(' ');
    }
    if out.chars().count() > room {
        String::new()
    } else {
        out
    }
}

/// Key text for hints: `<CR>` → `Enter`, `<leader>sp` → `Space s p`,
/// `<C-r>` → `C-r`; plain characters stay as they are.
pub fn friendly(seq: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut rest = seq;
    while !rest.is_empty() {
        if rest.starts_with('<')
            && let Some(end) = rest.find('>')
            && end > 1
        {
            let name = &rest[1..end];
            parts.push(
                match name {
                    "CR" => "Enter",
                    "leader" => "Space",
                    "Up" => "↑",
                    "Down" => "↓",
                    "Left" => "←",
                    "Right" => "→",
                    other => other,
                }
                .to_string(),
            );
            rest = &rest[end + 1..];
        } else {
            let c = rest.chars().next().unwrap();
            parts.push(c.to_string());
            rest = &rest[c.len_utf8()..];
        }
    }
    parts.join(if parts.len() > 1 { " " } else { "" })
}

// ----- pop-ups ------------------------------------------------------------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn popup(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line>) {
    f.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(Style::new().yellow());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

/// While a sequence is half typed (e.g. after the leader): what can follow.
fn draw_pending(f: &mut Frame, app: &App, area: Rect) {
    if app.pending_keys.is_empty() {
        return;
    }
    let ctx = app.contexts();
    let next = app.keymap.continuations(&ctx, &app.pending_keys);
    let typed = crate::keymap::display_seq(&app.pending_keys, app.keymap.leader);
    let mut lines: Vec<Line> = next
        .iter()
        .map(|(k, what)| {
            Line::from(vec![
                Span::styled(format!(" {k:<6}"), Style::new().yellow()),
                Span::raw(what.clone()),
            ])
        })
        .collect();
    lines.push(Line::styled(" Esc cancels", Style::new().dark_gray()));
    let w = lines.iter().map(|l| l.width()).max().unwrap_or(10) as u16 + 4;
    let h = lines.len() as u16 + 2;
    let r = Rect {
        x: area.x + area.width.saturating_sub(w + 1),
        y: area.y + area.height.saturating_sub(h + 1),
        width: w.min(area.width),
        height: h.min(area.height),
    };
    f.render_widget(Clear, r);
    let block = Block::bordered()
        .title(format!(" {typed} "))
        .border_style(Style::new().yellow());
    f.render_widget(Paragraph::new(lines).block(block), r);
}
fn draw_overlay(f: &mut Frame, app: &mut App, area: Rect) {
    match &app.overlay {
        Overlay::None | Overlay::CopyFrom { .. } => {
            if let Overlay::CopyFrom { target } = app.overlay {
                let r = Rect {
                    x: area.x,
                    y: area.y + area.height - 1,
                    width: area.width,
                    height: 1,
                };
                f.render_widget(Clear, r);
                f.render_widget(
                    Paragraph::new(format!(" Copy into {target} from: choose a key with j/k, Enter to copy, Esc to cancel ")).yellow().reversed(),
                    r,
                );
            }
        }
        Overlay::Help { page } => {
            // Generated from the keymap, so it shows what the keys really are.
            let mut lines: Vec<Line> = Vec::new();
            for ctx in Context::ALL {
                // one row per action, all its keys together
                let mut rows: Vec<(Action, Vec<String>)> = Vec::new();
                for b in app.keymap.bindings.iter().filter(|b| b.context == ctx) {
                    let k = crate::keymap::display_seq(&b.keys, app.keymap.leader);
                    match rows.iter_mut().find(|(a, _)| *a == b.action) {
                        Some((_, keys)) => keys.push(k),
                        None => rows.push((b.action, vec![k])),
                    }
                }
                if rows.is_empty() {
                    continue;
                }
                lines.push(Line::styled(
                    format!(" {}", ctx.title()),
                    Style::new().bold(),
                ));
                for (a, keys) in rows {
                    lines.push(Line::from(vec![
                        Span::styled(format!("   {:<20}", keys.join(" ")), Style::new().yellow()),
                        Span::raw(a.describe()),
                    ]));
                }
                // what each role's font has to be (the list shows only those)
                if ctx == Context::FontRoles {
                    for (name, need) in crate::fonts::ROLE_NAMES
                        .iter()
                        .zip(crate::fonts::ROLE_NEEDS)
                    {
                        lines.push(Line::from(vec![
                            Span::styled(format!("   {name:<20}"), Style::new().dark_gray()),
                            Span::raw(format!("needs {need}")),
                        ]));
                    }
                }
            }
            lines.push(Line::styled(
                format!(
                    " Leader: {}   Text fields take keys as typing.   ←/→ more columns; any other key closes.",
                    crate::keymap::display_seq(&[app.keymap.leader], crate::keymap::Key::new(ratatui::crossterm::event::KeyCode::Null, ratatui::crossterm::event::KeyModifiers::NONE))
                ),
                Style::new().dark_gray(),
            ));
            // columns wide enough to read; when they don't all fit, !90/!92 page
            const COL_W: u16 = 52;
            let r = centered(
                area,
                area.width.saturating_sub(4).min(4 * COL_W + 2),
                area.height.saturating_sub(2),
            );
            f.render_widget(Clear, r);
            let inner_h = r.height.saturating_sub(2) as usize;
            let per_col = inner_h.max(1);
            let cols: Vec<&[Line]> = lines.chunks(per_col).collect();
            let fit = (r.width.saturating_sub(2) / COL_W).max(1) as usize;
            let page = (*page).min(cols.len().saturating_sub(fit));
            let more = if cols.len() > fit {
                format!(
                    " columns {} 13{} of {} (!90/!92 for more) ",
                    page + 1,
                    (page + fit).min(cols.len()),
                    cols.len()
                )
            } else {
                String::new()
            };
            let block = Block::bordered()
                .title(format!(" Keys: {} preset ", app.keymap.name))
                .title_bottom(Line::styled(more, Style::new().yellow()))
                .border_style(Style::new().yellow());
            let inner = block.inner(r);
            f.render_widget(block, r);
            for (i, chunk) in cols.iter().skip(page).take(fit).enumerate() {
                let col = Rect {
                    x: inner.x + i as u16 * COL_W,
                    width: COL_W.min(inner.right().saturating_sub(inner.x + i as u16 * COL_W)),
                    ..inner
                };
                f.render_widget(Paragraph::new(chunk.to_vec()), col);
            }
        }
        Overlay::Confirm(p) => {
            // what's unsaved: the palette, the font preset, the settings
            let quit = matches!(p, crate::app::Pending::Quit);
            let fonts = matches!(
                p,
                crate::app::Pending::NewFonts | crate::app::Pending::OpenFonts(_)
            );
            let mut what = Vec::new();
            if (quit || !fonts) && app.dirty() {
                what.push(format!("palette \"{}\"", app.name));
            }
            if (quit || fonts) && app.fonts.dirty() {
                what.push(format!("font preset \"{}\"", app.fonts.name));
            }
            if (quit
                || matches!(
                    p,
                    crate::app::Pending::NewTheme | crate::app::Pending::OpenTheme(_)
                ))
                && app.theme_dirty()
                && what.is_empty()
            {
                // only the parts swapped (another palette or preset)
                what.push(format!("theme \"{}\"", app.theme));
            }
            if quit && app.settings_page.draft.is_some() {
                what.push("settings".to_string());
            }
            let what = format!(" Unsaved: {}.", what.join(", "));
            let r = centered(area, (what.chars().count() as u16 + 4).max(56), 5);
            let y = r.y + 3;
            // the choices are clickable, like their keys
            let mut button = |text: &'static str, x: u16, choice: char, base: Style| {
                let w = text.chars().count() as u16;
                let hovered = app.hits.add(
                    Rect {
                        x,
                        y,
                        width: w,
                        height: 1,
                    },
                    Target::ConfirmChoice(choice),
                );
                (
                    Span::styled(text, if hovered { hover(base) } else { base }),
                    x + w,
                )
            };
            let (discard, x) = button(" y  discard them ", r.x + 1, 'y', Style::new().yellow());
            let mut spans = vec![discard];
            let mut x = x;
            if !app.is_new {
                spans.push("  ".into());
                let (save, nx) = button(" s  save first ", x + 2, 's', Style::new());
                spans.push(save);
                x = nx;
            }
            spans.push("  ".into());
            spans.push(button(" Esc  cancel ", x + 2, 'n', Style::new()).0);
            let lines = vec![Line::raw(what), Line::raw(""), Line::from(spans)];
            popup(f, r, "Unsaved changes", lines);
        }
        Overlay::SaveAs {
            input, overwrite, ..
        } => {
            let mut lines = vec![
                Line::raw(match app.overlay_for {
                    crate::app::For::Fonts => " Name in fonts.json:",
                    crate::app::For::Palette => " Name in palettes.json:",
                    crate::app::For::Theme => " Theme name (new parts get it too):",
                }),
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(format!("{input}▏"), Style::new().yellow()),
                ]),
            ];
            if *overwrite {
                lines.push(Line::styled(
                    " exists: Enter again to overwrite",
                    Style::new().red(),
                ));
            }
            popup(f, centered(area, 50, 5), "Save as", lines);
        }
        Overlay::Capture { what, keys } => {
            let typed = crate::keymap::display_seq(keys, app.keymap.leader);
            let lines = match what {
                crate::settings::Capture::Leader => vec![
                    Line::raw(" Press the new leader key."),
                    Line::styled(" Esc cancels.", Style::new().dark_gray()),
                ],
                crate::settings::Capture::Binding { action, add, .. } => vec![
                    Line::raw(format!(
                        " {} keys for \"{}\":",
                        if *add { "Extra" } else { "New" },
                        action.describe()
                    )),
                    Line::from(vec![
                        Span::raw(" "),
                        Span::styled(format!("{typed}▏"), Style::new().yellow().bold()),
                    ]),
                    Line::styled(
                        " Press keys (a sequence is fine). Enter keeps them, Backspace takes one back, Esc cancels.",
                        Style::new().dark_gray(),
                    ),
                ],
            };
            popup(
                f,
                centered(area, 64, lines.len() as u16 + 2),
                "Record keys",
                lines,
            );
        }
        Overlay::ChooseKey { sel, original, .. } => {
            let keys = crate::behaviour::theme_keys();
            // at the right edge, so the rows it previews stay in view
            let mut r = centered(
                area,
                40,
                area.height.saturating_sub(4).min(keys.len() as u16 + 3),
            );
            r.x = area.right().saturating_sub(r.width + 2).max(area.x);
            let list_h = r.height.saturating_sub(3) as usize;
            let start = sel.saturating_sub(list_h.saturating_sub(1));
            let current = original.clone();
            let mut lines = vec![Line::styled(
                " j/k move, Enter choose, Esc cancel",
                Style::new().dark_gray(),
            )];
            for (i, key) in keys.iter().enumerate().skip(start).take(list_h) {
                let rr = Rect {
                    x: r.x + 1,
                    y: r.y + 1 + lines.len() as u16,
                    width: r.width.saturating_sub(2),
                    height: 1,
                };
                let mut line = Line::from(vec![
                    Span::styled(" ██ ", Style::new().fg(app.theme_colour(key))),
                    Span::styled(format!("{key:<20}"), Style::new()),
                    Span::styled(
                        if *key == current { " current" } else { "" },
                        Style::new().dark_gray(),
                    ),
                ]);
                if i == *sel {
                    // the cursor on the text, not the swatch or padding
                    line = crate::hit::cursor_line(line, Style::new().reversed());
                }
                if app.hits.add(rr, Target::OverlayRow(i)) && i != *sel {
                    line = crate::hit::hover_line(line, 1);
                }
                lines.push(line);
            }
            popup(f, r, "Colour from the active theme", lines);
        }
        Overlay::Prompt { title, input, .. } => {
            let lines = vec![
                Line::raw(format!(" {title}:")),
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(format!("{input}▏"), Style::new().yellow()),
                ]),
                Line::styled(" Enter keeps it, Esc cancels.", Style::new().dark_gray()),
            ];
            popup(f, centered(area, 64, 5), "Name", lines);
        }
        Overlay::Ask { question, .. } => {
            let r = centered(area, 64, 6);
            let y = r.y + 4;
            let mut button = |text: &'static str, x: u16, choice: char, base: Style| {
                let w = text.chars().count() as u16;
                let hovered = app.hits.add(
                    Rect {
                        x,
                        y,
                        width: w,
                        height: 1,
                    },
                    Target::ConfirmChoice(choice),
                );
                (
                    Span::styled(text, if hovered { hover(base) } else { base }),
                    x + w,
                )
            };
            let (yes, x) = button(" y  yes ", r.x + 1, 'y', Style::new().yellow());
            let (no, _) = button(" Esc  no ", x + 2, 'n', Style::new());
            let lines = vec![
                Line::raw(format!(" {}", truncate(question, 60))),
                Line::raw(if question.chars().count() > 60 {
                    format!(" {}", question.chars().skip(60).collect::<String>())
                } else {
                    String::new()
                }),
                Line::raw(""),
                Line::from(vec![yes, "  ".into(), no]),
            ];
            popup(f, r, "Sure?", lines);
        }
        Overlay::Menu { menu, sel, at } => {
            // hangs under the title text that opened it
            let items = crate::fonts_preview::menu_items(*menu);
            let w = (items.iter().map(|s| s.chars().count()).max().unwrap_or(4) as u16 + 4)
                .min(area.width);
            let h = (items.len() as u16 + 2).min(area.bottom().saturating_sub(at.y + 1));
            let r = Rect {
                x: at.x.saturating_sub(1).min(area.right().saturating_sub(w)),
                y: at.y + 1,
                width: w,
                height: h,
            };
            let mut lines = Vec::new();
            for (i, item) in items.iter().enumerate().take(h.saturating_sub(2) as usize) {
                let rr = Rect::new(r.x + 1, r.y + 1 + i as u16, r.width.saturating_sub(2), 1);
                let mut line = Line::raw(format!(" {item:<width$} ", width = w as usize - 4));
                if i == *sel {
                    line = crate::hit::cursor_line(line, Style::new().reversed());
                }
                lines.push(if app.hits.add(rr, Target::OverlayRow(i)) {
                    crate::hit::hover_line(line, 0)
                } else {
                    line
                });
            }
            f.render_widget(Clear, r);
            f.render_widget(
                Paragraph::new(lines).block(Block::bordered().border_style(Style::new().yellow())),
                r,
            );
        }
        Overlay::Open {
            filter,
            sel,
            entries,
        } => {
            // the palette list hangs under the Palette pane's dropdown row; the
            // other lists (and this one on other pages) are centred
            let hanging = match app.palette_pane {
                Some(p) if app.overlay_for == crate::app::For::Palette => {
                    let w = 70.min(area.width);
                    let h = (entries.len() as u16 + 4)
                        .min(30)
                        .min(area.bottom().saturating_sub(p.y + 2));
                    Some(Rect {
                        x: p.x.min(area.right().saturating_sub(w)),
                        y: p.y + 2,
                        width: w,
                        height: h,
                    })
                }
                _ => None,
            };
            let r = hanging
                .unwrap_or_else(|| centered(area, 70, area.height.saturating_sub(6).min(30)));
            let matches = open_matches(entries, filter);
            let list_h = r.height.saturating_sub(4) as usize;
            let start = sel.saturating_sub(list_h.saturating_sub(1));
            let mut lines = vec![
                Line::from(vec![
                    " Search: ".into(),
                    Span::styled(format!("{filter}▏"), Style::new().yellow()),
                ]),
                Line::styled(
                    format!(
                        " {} of {}{}",
                        matches.len(),
                        entries.len(),
                        if app.dirty() {
                            "   (unsaved changes: you'll be asked)"
                        } else {
                            ""
                        }
                    ),
                    Style::new().dark_gray(),
                ),
            ];
            for (row, &i) in matches.iter().enumerate().skip(start).take(list_h) {
                let (name, sw) = &entries[i];
                let mut spans = vec![Span::raw(format!(" {:<34}", truncate(name, 33)))];
                for c in sw {
                    spans.push(swatch(*c, 2));
                }
                if app.active.as_deref() == Some(name) {
                    spans.push(Span::styled(" active", Style::new().dark_gray()));
                }
                // rows are clickable (a second click opens), hover-highlighted
                let rr = Rect {
                    x: r.x + 1,
                    y: r.y + 1 + lines.len() as u16,
                    width: r.width.saturating_sub(2),
                    height: 1,
                };
                let mut line = Line::from(spans);
                if row == *sel {
                    // the cursor on the text, not the padding or swatches
                    line = crate::hit::cursor_line(line, Style::new().reversed());
                }
                lines.push(if app.hits.add(rr, Target::OverlayRow(row)) {
                    crate::hit::hover_line(line, 0)
                } else {
                    line
                });
            }
            popup(
                f,
                r,
                match app.overlay_for {
                    crate::app::For::Fonts => "Open a font preset for this theme",
                    crate::app::For::Palette => "Open a palette for this theme",
                    crate::app::For::Theme => "Open theme",
                },
                lines,
            );
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).collect::<String>() + "…"
    }
}

/// Superscript panel number for a title, btop-style ("¹Palette"); the
/// number key jumps to that panel.
pub fn panel_number(n: u8) -> Span<'static> {
    const SUP: [&str; 10] = ["⁰", "¹", "²", "³", "⁴", "⁵", "⁶", "⁷", "⁸", "⁹"];
    Span::styled(format!(" {}", SUP[n as usize % 10]), Style::new().yellow())
}
#[cfg(test)]
mod tests {
    use super::{fit_hints, friendly};

    #[test]
    fn hints_shorten_to_fit_and_keep_help_last() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let ctx = s(&["j/k move", "Enter edit", "p pin"]);
        let general = s(&["Tab next", "1-4 panels", "s save", "a apply", "o open"]);
        let wide = fit_hints(&ctx, &general, "? keys", 200);
        assert_eq!(
            wide,
            " j/k move  Enter edit  p pin │ Tab next  1-4 panels  s save  a apply  o open  ? keys "
        );
        let narrow = fit_hints(&ctx, &general, "? keys", 40);
        assert!(narrow.chars().count() <= 40, "{narrow}");
        assert!(
            narrow.starts_with(" j/k move  Enter edit") && narrow.ends_with("? keys "),
            "{narrow}"
        );
        assert_eq!(fit_hints(&ctx, &general, "? keys", 10), " ? keys ");
        assert_eq!(fit_hints(&ctx, &general, "? keys", 5), "");
        for room in 0..120 {
            assert!(fit_hints(&ctx, &general, "? keys", room).chars().count() <= room);
        }
    }

    #[test]
    fn friendly_key_names() {
        assert_eq!(friendly("<CR>"), "Enter");
        assert_eq!(friendly("<leader>sp"), "Space s p");
        assert_eq!(friendly("<C-r>"), "C-r");
        assert_eq!(friendly("j"), "j");
        assert_eq!(friendly("<lt>"), "lt");
    }
}
