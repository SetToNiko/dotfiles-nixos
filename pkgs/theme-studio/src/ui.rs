//! Drawing. The studio's own chrome uses named terminal colours (so it
//! follows the system theme); only swatches and the preview use the palette
//! being edited.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::{App, Focus, Overlay, open_matches};
use crate::color::{self, Rgb};
use crate::palette::{self, GROUPS, TEXT_KEYS};

/// Width of the editor column (key list and picker) when beside the preview;
/// also the narrowest the editor can be.
const EDITOR_W: u16 = 52;
/// The picker is at least this tall, and grows (bigger colour square) when
/// the editor has room beyond the whole key list.
const PICKER_MIN: u16 = 14;
const PICKER_MAX: u16 = 30;
/// Rows the key list needs to show every key: 34 keys, 5 headers, 4 gaps, borders.
const LIST_FULL: u16 = 34 + 5 + 4 + 2;
/// Smallest useful key list (it scrolls) and preview.
const LIST_MIN: u16 = 8;
const PREVIEW_MIN_W: u16 = 60;
const PREVIEW_MIN_H: u16 = 12;
/// Share of the height the preview gets when stacked under the editor.
const STACKED_PREVIEW_SHARE: f64 = 0.4;

/// Where everything goes for a window size. Like btop, the layout follows
/// the window as it's resized: editor and preview side by side when both
/// fit, stacked (editor on top) when the window is too narrow, and a "too
/// small" message with the needed size below the minimum.
#[derive(Debug, PartialEq)]
pub enum Arrangement {
    Panels { list: Rect, picker: Rect, preview: Rect, status: Rect, stacked: bool },
    TooSmall { need_w: u16, need_h: u16 },
}

pub fn arrange(area: Rect) -> Arrangement {
    let side_by_side = area.width >= EDITOR_W + PREVIEW_MIN_W;
    let (need_w, need_h) = if side_by_side {
        (EDITOR_W + PREVIEW_MIN_W, LIST_MIN + PICKER_MIN + 1)
    } else {
        (EDITOR_W, LIST_MIN + PICKER_MIN + PREVIEW_MIN_H + 1)
    };
    if area.width < need_w || area.height < need_h {
        return Arrangement::TooSmall { need_w, need_h };
    }
    let [main, status] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let (editor, preview) = if side_by_side {
        let [e, p] = Layout::horizontal([Constraint::Length(EDITOR_W), Constraint::Min(0)]).areas(main);
        (e, p)
    } else {
        let most = main.height - LIST_MIN - PICKER_MIN;
        let preview_h = ((main.height as f64 * STACKED_PREVIEW_SHARE) as u16).clamp(PREVIEW_MIN_H, most);
        let [e, p] = Layout::vertical([Constraint::Min(0), Constraint::Length(preview_h)]).areas(main);
        (e, p)
    };
    let picker_h = editor.height.saturating_sub(LIST_FULL).clamp(PICKER_MIN, PICKER_MAX);
    let picker_h = picker_h.min(editor.height - LIST_MIN);
    let [list, picker] = Layout::vertical([Constraint::Min(0), Constraint::Length(picker_h)]).areas(editor);
    Arrangement::Panels { list, picker, preview, status, stacked: !side_by_side }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    match arrange(area) {
        Arrangement::TooSmall { need_w, need_h } => draw_too_small(f, area, need_w, need_h),
        Arrangement::Panels { list, picker, preview, status, .. } => {
            draw_list(f, app, list);
            draw_picker(f, app, picker);
            draw_preview(f, app, preview);
            draw_status(f, app, status);
            draw_overlay(f, app, area);
        }
    }
}

/// btop-style: the current size (red where it falls short) and what's needed.
fn draw_too_small(f: &mut Frame, area: Rect, need_w: u16, need_h: u16) {
    let ok = |have: u16, need: u16| if have >= need { Style::new().green() } else { Style::new().red().bold() };
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
    let r = Rect { x: area.x, y: area.y + (area.height - h) / 2, width: area.width, height: h };
    f.render_widget(Paragraph::new(lines).centered(), r);
}

fn focused_block(title: Line<'static>, focused: bool) -> Block<'static> {
    let b = Block::bordered().title(title);
    if focused { b.border_style(Style::new().yellow()) } else { b.border_style(Style::new().dark_gray()) }
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
    let title = Line::from(vec![
        " Palette: ".into(),
        Span::styled(app.name.clone(), Style::new().bold()),
        if app.dirty() { Span::styled(" ●", Style::new().yellow()) } else { "".into() },
        " ".into(),
    ]);
    let block = focused_block(title, app.focus == Focus::List);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Rows: group headers and keys; `Some(i)` marks key index i.
    let mut rows: Vec<(Line, Option<usize>)> = Vec::new();
    let mut i = 0;
    let bg0 = app.palette.get("bg0");
    let copy_target = match app.overlay {
        Overlay::CopyFrom { target } => Some(target),
        _ => None,
    };
    for (gi, (group, keys)) in GROUPS.iter().enumerate() {
        if gi > 0 {
            rows.push((Line::raw(""), None));
        }
        rows.push((Line::styled(format!(" {group}"), Style::new().bold().dark_gray()), None));
        for key in keys.iter().copied() {
            let c = app.palette.get(key);
            let selected = i == app.sel;
            let pinned = app.palette.is_pinned(key);
            let mut name_style = Style::new();
            if selected {
                name_style = name_style.add_modifier(Modifier::REVERSED);
            }
            let marker = if copy_target == Some(key) {
                Span::styled("→", Style::new().yellow())
            } else if pinned {
                Span::styled("", Style::new().yellow())
            } else {
                " ".into()
            };
            let mut spans = vec![
                " ".into(),
                marker,
                " ".into(),
                Span::styled(format!("{key:<16}"), name_style),
                swatch(c, 4),
                Span::styled(format!(" {} ", c.hex()), Style::new()),
            ];
            let info = if palette::is_base(key) || pinned {
                String::new()
            } else {
                format!("⟵ {}", palette::sources(key))
            };
            let mut info_span = Span::styled(format!("{info:<12}"), Style::new().dark_gray());
            if TEXT_KEYS.contains(&key) {
                let ratio = color::contrast(c, bg0);
                let style = if ratio < 3.0 {
                    Style::new().red().bold()
                } else if ratio < 4.5 {
                    Style::new().yellow()
                } else {
                    Style::new().dark_gray()
                };
                spans.push(info_span);
                info_span = Span::styled(format!("{ratio:>4.1}:1"), style);
            }
            spans.push(info_span);
            rows.push((Line::from(spans), Some(i)));
            i += 1;
        }
    }

    // Scroll so the selected key stays visible.
    let h = inner.height as usize;
    let sel_row = rows.iter().position(|(_, k)| *k == Some(app.sel)).unwrap_or(0);
    if sel_row < app.list_scroll + 1 {
        app.list_scroll = sel_row.saturating_sub(1);
    } else if sel_row >= app.list_scroll + h {
        app.list_scroll = sel_row + 1 - h;
    }
    let visible: Vec<_> = rows.into_iter().skip(app.list_scroll).take(h).collect();
    app.areas.list = inner;
    app.areas.list_rows = visible.iter().map(|(_, k)| *k).collect();
    let lines: Vec<Line> = visible.into_iter().map(|(l, _)| l).collect();
    f.render_widget(Paragraph::new(lines), inner);
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
        Span::styled(format!(" follows {} until edited ", palette::sources(key)), Style::new().dark_gray())
    };
    let title = Line::from(vec![" Colour: ".into(), Span::styled(key, Style::new().bold()), " ".into(), state]);
    let focused = !matches!(app.focus, Focus::List);
    let block = focused_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Colour square rows: whatever the 3 sliders, a gap and the hex row leave
    let rows: u16 = inner.height.saturating_sub(5).max(2);
    let sq = Rect { x: inner.x + 1, y: inner.y, width: inner.width.saturating_sub(7), height: rows };
    let hue = Rect { x: sq.x + sq.width + 1, y: inner.y, width: 2, height: rows };
    let (h, s, v) = app.hsv;
    let current = app.palette.get(key);
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
    let marker = if app.focus == Focus::Square { "◉" } else { "○" };
    set_cell(buf, mx, my, marker, current.text_on(), current);

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
        cell.set_style(if app.focus == Focus::Hue { Style::new().yellow() } else { Style::new() });
    }

    // H, S, L sliders, each showing what moving it would give
    let (sl_s, sl_l) = app.hsl();
    let track_w = inner.width.saturating_sub(10);
    let sliders: [(Focus, &str, f64, String); 3] = [
        (Focus::SliderH, "H", h / 360.0, format!("{:>3}°", h.round() as i32)),
        (Focus::SliderS, "S", sl_s, format!("{:>3}%", (sl_s * 100.0).round() as i32)),
        (Focus::SliderL, "L", sl_l, format!("{:>3}%", (sl_l * 100.0).round() as i32)),
    ];
    for (row, (focus, label, t, value)) in sliders.into_iter().enumerate() {
        let y = inner.y + rows + row as u16;
        let label_style = if app.focus == focus { Style::new().yellow().bold() } else { Style::new() };
        if let Some(cell) = buf.cell_mut((inner.x + 1, y)) {
            cell.set_symbol(label).set_style(label_style);
        }
        let track = Rect { x: inner.x + 3, y, width: track_w, height: 1 };
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
        match focus {
            Focus::SliderH => app.areas.slider_h = track,
            Focus::SliderS => app.areas.slider_s = track,
            _ => app.areas.slider_l = track,
        }
    }

    // Hex input, and the colour before and after editing
    let y = inner.y + rows + 4;
    let editing = app.focus == Focus::Hex;
    let hex_style = if editing { Style::new().yellow().underlined() } else { Style::new() };
    let hex_text = if editing { format!("{:<7}▏", app.hex_input) } else { format!("{:<8}", current.hex()) };
    let old = app.saved.get(key);
    let line = Line::from(vec![
        Span::styled(" hex ", if editing { Style::new().yellow().bold() } else { Style::new() }),
        Span::styled(hex_text, hex_style),
        "   saved ".dark_gray(),
        framed(old),
        "  now ".dark_gray(),
        framed(current),
    ]);
    let hex_row = Rect { x: inner.x, y, width: inner.width, height: 1 };
    f.render_widget(Paragraph::new(line), hex_row);
    app.areas.square = sq;
    app.areas.hue = hue;
    app.areas.hex = Rect { x: inner.x, y, width: 14, height: 1 };
}

// ----- preview (simple, until the live desktop preview) --------------------

/// A line of sample code: (text, palette key) tokens, and an optional line background key.
type CodeLine<'a> = (Vec<(&'a str, &'a str)>, Option<&'a str>);

fn draw_preview(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.palette;
    let block = Block::bordered()
        .title(" Preview ")
        .border_style(Style::new().dark_gray());
    let inner = block.inner(area);
    f.render_widget(block, area);
    let bg = p.get("bg0");
    f.render_widget(Block::new().style(Style::new().bg(bg.color()).fg(p.get("fg0").color())), inner);

    let fg = |k: &str| Style::new().fg(p.get(k).color());
    let mut lines: Vec<Line> = Vec::new();
    let heading = |t: &str| Line::styled(format!(" {t}"), fg("grey1").bold());

    // Terminal colours, as Ghostty gets them (terminal-colors.json)
    lines.push(heading("Terminal colours"));
    if let Some(map) = &app.terminal {
        for half in [0..8, 8..16] {
            let mut spans: Vec<Span> = vec!["  ".into()];
            for i in half {
                let c = map.ansi.get(i).and_then(|k| palette::key(k)).map(|k| p.get(k)).unwrap_or(bg);
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
        let term = Style::new().fg(palette::key(&map.foreground).map_or(bg, |k| p.get(k)).color()).bg(palette::key(&map.background).map_or(bg, |k| p.get(k)).color());
        let mut spans = vec![Span::styled("  $ ls --color ", term)];
        spans.extend(sample);
        lines.push(Line::from(spans));
    } else {
        lines.push(Line::styled("  (terminal-colors.json not found)", fg("red")));
    }
    lines.push(Line::raw(""));

    // Code as nvim shows it (mini.base16 mapping from nvim/lua/theme.lua)
    lines.push(heading("Editor"));
    let num = |n: u32, cur: bool| {
        Span::styled(format!("  {n:>3} "), if cur { fg("yellow") } else { fg("grey0") })
    };
    let code: Vec<CodeLine> = vec![
        (vec![("-- Re-apply when the colours file changes", "grey1")], None),
        (vec![("local ", "purple"), ("function ", "purple"), ("apply", "blue"), ("(", "grey2"), ("path", "red"), (", ", "grey2"), ("opts", "red"), (")", "grey2")], None),
        (vec![("  if not ", "purple"), ("opts", "red"), (".", "grey2"), ("force ", "red"), ("then return ", "purple"), ("nil ", "orange"), ("end", "purple")], Some("bg1")),
        (vec![("  local ", "purple"), ("count", "red"), (" = ", "fg0"), ("42", "orange"), (" + ", "fg0"), ("#", "aqua"), ("path", "red")], None),
        (vec![("  vim", "red"), (".", "grey2"), ("notify", "blue"), ("(", "grey2"), ("\"theme applied\"", "green"), (", ", "grey2"), ("Level", "yellow"), (".", "grey2"), ("INFO", "orange"), (")", "grey2")], Some("bg3")),
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
        Span::styled(" NORMAL ", Style::new().bg(p.get("green").color()).fg(bg.color()).bold()),
        Span::styled(" theme.lua ", Style::new().bg(p.get("bg_statusline2").color()).fg(p.get("fg0").color())),
        Span::styled(" ".repeat(20), Style::new().bg(p.get("bg_statusline1").color())),
        Span::styled(" 3:14 ", Style::new().bg(p.get("bg_statusline3").color()).fg(p.get("fg0").color())),
    ]));
    lines.push(Line::raw(""));

    // i3 window borders and bar (i3/theme roles)
    lines.push(heading("Window borders and bar"));
    lines.push(Line::from(vec![
        "  ".into(),
        Span::styled(" focused ", Style::new().bg(p.get("yellow").color()).fg(bg.color())),
        "  ".into(),
        Span::styled(" inactive ", Style::new().bg(p.get("bg3").color()).fg(p.get("fg0").color())),
        "  ".into(),
        Span::styled(" urgent ", Style::new().bg(p.get("red").color()).fg(p.get("fg0").color())),
        "  ".into(),
        Span::styled(" 1 ", Style::new().bg(p.get("yellow").color()).fg(bg.color())),
        Span::styled(" 2 ", Style::new().bg(p.get("bg3").color()).fg(p.get("fg0").color())),
        Span::styled(" 3 ", fg("grey2")),
        Span::styled("   vol 40%  wifi  17:42", fg("fg0")),
    ]));
    lines.push(Line::raw(""));

    // Every key
    lines.push(heading("All colours"));
    let cols = ((inner.width.saturating_sub(2)) / 24).max(1) as usize;
    let all: Vec<&str> = GROUPS.iter().flat_map(|(_, ks)| ks.iter().copied()).collect();
    for chunk in all.chunks(cols) {
        let mut spans: Vec<Span> = vec!["  ".into()];
        for k in chunk {
            spans.push(swatch(p.get(k), 3));
            spans.push(Span::styled(format!(" {k:<19}"), if *k == app.key() { fg("yellow").bold() } else { fg("grey2") }));
        }
        lines.push(Line::from(spans));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

// ----- status line --------------------------------------------------------

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled(
        format!(" {}{} ", app.name, if app.dirty() { " ●" } else { "" }),
        Style::new().reversed(),
    )];
    if let Some(active) = &app.active {
        spans.push(Span::styled(format!(" active: {active} "), Style::new().dark_gray()));
    }
    if let Some((msg, err)) = &app.message {
        spans.push(Span::styled(format!(" {msg} "), if *err { Style::new().red().bold() } else { Style::new().green() }));
    } else {
        let hints = match app.focus {
            Focus::List => "j/k move  Enter edit  p pin  c copy  # hex",
            Focus::Square => "hjkl saturation/value (HJKL faster)  Esc list",
            Focus::Hue => "j/k hue (J/K faster)  Esc list",
            Focus::SliderH | Focus::SliderS | Focus::SliderL => "h/l adjust (H/L faster)  j/k next slider",
            Focus::Hex => "type #rrggbb  Enter set  Esc cancel",
        };
        spans.push(Span::styled(format!(" {hints} │ Tab next  s save  a apply  o open  ? keys "), Style::new().dark_gray()));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

// ----- pop-ups ------------------------------------------------------------

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

fn popup(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line>) {
    f.render_widget(Clear, area);
    let block = Block::bordered().title(format!(" {title} ")).border_style(Style::new().yellow());
    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_overlay(f: &mut Frame, app: &App, area: Rect) {
    match &app.overlay {
        Overlay::None | Overlay::CopyFrom { .. } => {
            if let Overlay::CopyFrom { target } = app.overlay {
                let r = Rect { x: area.x, y: area.y + area.height - 1, width: area.width, height: 1 };
                f.render_widget(Clear, r);
                f.render_widget(
                    Paragraph::new(format!(" Copy into {target} from: choose a key with j/k, Enter to copy, Esc to cancel ")).yellow().reversed(),
                    r,
                );
            }
        }
        Overlay::Help => {
            let rows = [
                ("Tab / Shift+Tab", "next / previous control"),
                ("Esc", "back to the key list"),
                ("j k  ↑ ↓", "move in the list; adjust hue"),
                ("h j k l  (H J K L)", "move in the colour square (faster)"),
                ("h l  (H L)", "adjust a slider (faster)"),
                ("#", "type a hex colour"),
                ("Ctrl+Shift+V", "paste a hex colour into the key"),
                ("p", "pin a derived key / let it follow again"),
                ("c", "copy another key's colour into this one"),
                ("y", "copy this colour's hex to the clipboard"),
                ("u  Ctrl+r", "undo  redo"),
                ("s  S", "save  save as"),
                ("a", "save and apply as the system theme"),
                ("o  n", "open a palette  new palette"),
                ("q", "quit"),
                ("mouse", "click or drag in the list, square, hue strip, sliders"),
            ];
            let lines = rows
                .iter()
                .map(|(k, d)| Line::from(vec![Span::styled(format!(" {k:<20}"), Style::new().yellow()), Span::raw(*d)]))
                .chain([Line::raw(""), Line::styled(" any key to close", Style::new().dark_gray())])
                .collect();
            popup(f, centered(area, 64, rows.len() as u16 + 4), "Keys", lines);
        }
        Overlay::Confirm(_) => {
            let save = if app.is_new { "" } else { "   s  save first" };
            let lines = vec![
                Line::raw(format!(" \"{}\" has unsaved changes.", app.name)),
                Line::raw(""),
                Line::from(vec![Span::styled(" y  discard them", Style::new().yellow()), Span::raw(save), Span::raw("   Esc  cancel")]),
            ];
            popup(f, centered(area, 56, 5), "Unsaved changes", lines);
        }
        Overlay::SaveAs { input, overwrite, .. } => {
            let mut lines = vec![
                Line::raw(" Name in palettes.json:"),
                Line::from(vec![Span::raw(" "), Span::styled(format!("{input}▏"), Style::new().yellow())]),
            ];
            if *overwrite {
                lines.push(Line::styled(" exists: Enter again to overwrite", Style::new().red()));
            }
            popup(f, centered(area, 50, 5), "Save as", lines);
        }
        Overlay::Open { filter, sel, entries } => {
            let r = centered(area, 70, area.height.saturating_sub(6).min(30));
            let matches = open_matches(entries, filter);
            let list_h = r.height.saturating_sub(4) as usize;
            let start = sel.saturating_sub(list_h.saturating_sub(1));
            let mut lines = vec![
                Line::from(vec![" Search: ".into(), Span::styled(format!("{filter}▏"), Style::new().yellow())]),
                Line::styled(
                    format!(" {} of {}{}", matches.len(), entries.len(), if app.dirty() { "   (unsaved changes: you'll be asked)" } else { "" }),
                    Style::new().dark_gray(),
                ),
            ];
            for (row, &i) in matches.iter().enumerate().skip(start).take(list_h) {
                let (name, sw) = &entries[i];
                let mut spans = vec![Span::styled(
                    format!(" {:<34}", truncate(name, 33)),
                    if row == *sel { Style::new().reversed() } else { Style::new() },
                )];
                for c in sw {
                    spans.push(swatch(*c, 2));
                }
                if app.active.as_deref() == Some(name) {
                    spans.push(Span::styled(" active", Style::new().dark_gray()));
                }
                lines.push(Line::from(spans));
            }
            popup(f, r, "Open palette", lines);
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n { s.to_string() } else { s.chars().take(n - 1).collect::<String>() + "…" }
}
