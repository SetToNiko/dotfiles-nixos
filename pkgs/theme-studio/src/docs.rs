//! The Settings page's README tab: docs/theme-studio.md (found by
//! paths::guide, read when the tab opens), rendered from Markdown into
//! styled lines wrapped to the pane: headings, paragraphs, lists, block
//! quotes, code, tables and rules. Headings are remembered so `}` / `{` can
//! jump between them.

use pulldown_cmark::{Alignment, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// The document, as rendered for one width.
#[derive(Debug, Default)]
pub struct Doc {
    pub lines: Vec<Line<'static>>,
    /// The line each heading starts on, in order.
    pub headings: Vec<usize>,
}

/// A piece of text in one style.
type Piece = (String, Style);

fn heading_style(level: HeadingLevel) -> Style {
    match level {
        HeadingLevel::H1 => Style::new().yellow().bold(),
        HeadingLevel::H2 => Style::new().yellow().bold(),
        _ => Style::new().bold(),
    }
}

fn code_style() -> Style {
    Style::new().cyan()
}

/// Splits styled text into words (with the spaces between them dropped) and
/// fills lines up to `width`. The first line starts with `first`, the rest
/// with `rest` (indents, bullets).
fn wrap(
    pieces: &[Piece],
    width: usize,
    first: Vec<Span<'static>>,
    rest: Vec<Span<'static>>,
) -> Vec<Line<'static>> {
    let prefix_w = |p: &[Span]| p.iter().map(|s| s.width()).sum::<usize>();
    // words: runs of non-space characters, possibly in several styles
    // (`**bold**text` is one word)
    let mut words: Vec<Vec<Piece>> = Vec::new();
    let mut gap = true;
    for (text, style) in pieces {
        for (i, part) in text.split(' ').enumerate() {
            if i > 0 {
                gap = true;
            }
            if part.is_empty() {
                continue;
            }
            if gap || words.is_empty() {
                words.push(Vec::new());
                gap = false;
            }
            words.last_mut().unwrap().push((part.to_string(), *style));
        }
    }
    // a word wider than a whole line is cut into line-sized pieces
    let max = width.saturating_sub(prefix_w(&rest)).max(8);
    let words: Vec<Vec<Piece>> = words.into_iter().flat_map(|w| split_word(w, max)).collect();
    let mut out = Vec::new();
    let mut line: Vec<Span<'static>> = first;
    let mut used = prefix_w(&line);
    let mut start = used;
    let room = |p: usize| width.saturating_sub(p).max(8);
    let mut empty = true;
    for w in words {
        let ww: usize = w.iter().map(|(t, _)| t.width()).sum();
        if !empty && used + 1 + ww > start + room(start) {
            out.push(Line::from(std::mem::take(&mut line)));
            line = rest.clone();
            used = prefix_w(&line);
            start = used;
            empty = true;
        }
        if !empty {
            line.push(Span::raw(" "));
            used += 1;
        }
        for (t, s) in w {
            line.push(Span::styled(t, s));
        }
        used += ww;
        empty = false;
    }
    if !empty || out.is_empty() {
        out.push(Line::from(line));
    }
    out
}

/// A word in pieces no wider than `max` columns each.
fn split_word(word: Vec<Piece>, max: usize) -> Vec<Vec<Piece>> {
    let total: usize = word.iter().map(|(t, _)| t.width()).sum();
    if total <= max {
        return vec![word];
    }
    let mut out: Vec<Vec<Piece>> = vec![Vec::new()];
    let mut used = 0;
    for (text, style) in word {
        for c in text.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if used + cw > max && used > 0 {
                out.push(Vec::new());
                used = 0;
            }
            let part = out.last_mut().unwrap();
            match part.last_mut() {
                Some((t, s)) if *s == style => t.push(c),
                _ => part.push((c.to_string(), style)),
            }
            used += cw;
        }
    }
    out
}

/// One list level: its next number (None: bullets).
struct List {
    next: Option<u64>,
}

/// A table being read: rows of cells of styled text.
#[derive(Default)]
struct Table {
    align: Vec<Alignment>,
    rows: Vec<Vec<Vec<Piece>>>,
    head_rows: usize,
}

/// Lays a table out in columns: as wide as their text, narrowed (widest
/// first) to fit `width`, cells wrapped inside their column.
fn render_table(t: &Table, width: usize, indent: &str) -> Vec<Line<'static>> {
    let cols = t.rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return Vec::new();
    }
    let text_w = |c: &[Piece]| c.iter().map(|(s, _)| s.width()).sum::<usize>();
    let mut w: Vec<usize> = (0..cols)
        .map(|i| {
            t.rows
                .iter()
                .filter_map(|r| r.get(i))
                .map(|c| text_w(c))
                .max()
                .unwrap_or(0)
                .max(1)
        })
        .collect();
    // " │ " between columns
    let avail = width.saturating_sub(indent.width() + 3 * (cols - 1));
    while w.iter().sum::<usize>() > avail {
        let (i, &max) = w.iter().enumerate().max_by_key(|(_, v)| **v).unwrap();
        if max <= 6 {
            break;
        }
        w[i] = max - 1;
    }
    let sep = Style::new().dark_gray();
    let mut out = Vec::new();
    for (ri, row) in t.rows.iter().enumerate() {
        let head = ri < t.head_rows;
        let cells: Vec<Vec<Line>> = (0..cols)
            .map(|i| {
                let mut c = row.get(i).cloned().unwrap_or_default();
                if head {
                    for p in &mut c {
                        p.1 = p.1.add_modifier(Modifier::BOLD);
                    }
                }
                wrap(&c, w[i], Vec::new(), Vec::new())
            })
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for li in 0..height {
            let mut spans = vec![Span::raw(indent.to_string())];
            for (i, cell) in cells.iter().enumerate() {
                if i > 0 {
                    spans.push(Span::styled(" │ ", sep));
                }
                let line = cell.get(li).cloned().unwrap_or_default();
                let lw = line.width();
                let pad = w[i].saturating_sub(lw);
                let right = t.align.get(i) == Some(&Alignment::Right);
                if right {
                    spans.push(Span::raw(" ".repeat(pad)));
                }
                spans.extend(line.spans);
                if !right {
                    spans.push(Span::raw(" ".repeat(pad)));
                }
            }
            out.push(Line::from(spans));
        }
        if head && ri + 1 == t.head_rows {
            let rule: Vec<String> = w.iter().map(|n| "─".repeat(*n)).collect();
            out.push(Line::styled(format!("{indent}{}", rule.join("─┼─")), sep));
        }
    }
    out
}

/// Renders Markdown into lines `width` columns wide.
pub fn render(md: &str, width: u16) -> Doc {
    let width = (width as usize).max(20);
    let mut doc = Doc::default();
    let mut pieces: Vec<Piece> = Vec::new();
    // the inline style: bold, italic, strikethrough, links
    let mut style = Style::new();
    let mut styles: Vec<Style> = Vec::new();
    let mut heading: Option<HeadingLevel> = None;
    let mut lists: Vec<List> = Vec::new();
    // the bullet waiting for the item's first line
    let mut bullet: Option<String> = None;
    let mut quote = 0usize;
    let mut code: Option<Vec<String>> = None;
    let mut table: Option<Table> = None;

    // the indent of the current block (lists, quotes)
    let indent = |lists: &Vec<List>, quote: usize| -> (String, Vec<Span<'static>>) {
        let mut q = Vec::new();
        for _ in 0..quote {
            q.push(Span::styled("│ ", Style::new().dark_gray()));
        }
        (" ".repeat(lists.len().saturating_sub(1) * 3), q)
    };
    // a blank line between blocks (not at the start, not twice)
    let gap = |doc: &mut Doc| {
        if doc.lines.last().is_some_and(|l| l.width() > 0) {
            doc.lines.push(Line::raw(""));
        }
    };
    // the text so far, as wrapped lines
    let flush = |doc: &mut Doc,
                 pieces: &mut Vec<Piece>,
                 bullet: &mut Option<String>,
                 lists: &Vec<List>,
                 quote: usize| {
        if pieces.is_empty() && bullet.is_none() {
            return;
        }
        let (ind, q) = indent(lists, quote);
        let mut first = q.clone();
        let mut rest = q;
        first.push(Span::raw(ind.clone()));
        rest.push(Span::raw(ind));
        if let Some(b) = bullet.take() {
            rest.push(Span::raw(" ".repeat(b.width())));
            first.push(Span::styled(b, Style::new().yellow()));
        } else if !lists.is_empty() {
            // a later paragraph in an item: under its text (after "• ")
            first.push(Span::raw("  "));
            rest.push(Span::raw("  "));
        }
        doc.lines.extend(wrap(pieces, width, first, rest));
        pieces.clear();
    };

    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    for ev in Parser::new_ext(md, opts) {
        if let Some(t) = table.as_mut() {
            match ev {
                Event::Start(Tag::TableHead) => {
                    t.rows.push(Vec::new());
                    t.head_rows += 1;
                }
                Event::Start(Tag::TableRow) => t.rows.push(Vec::new()),
                Event::Start(Tag::TableCell) => {
                    if let Some(r) = t.rows.last_mut() {
                        r.push(Vec::new());
                    }
                }
                Event::Text(s) => {
                    if let Some(c) = t.rows.last_mut().and_then(|r| r.last_mut()) {
                        c.push((s.to_string(), style));
                    }
                }
                Event::Code(s) => {
                    if let Some(c) = t.rows.last_mut().and_then(|r| r.last_mut()) {
                        c.push((s.to_string(), code_style()));
                    }
                }
                Event::Start(Tag::Strong) => {
                    styles.push(style);
                    style = style.bold();
                }
                Event::Start(Tag::Emphasis) => {
                    styles.push(style);
                    style = style.italic();
                }
                Event::Start(Tag::Link { .. }) => {
                    styles.push(style);
                    style = style.blue().underlined();
                }
                Event::End(TagEnd::Strong | TagEnd::Emphasis | TagEnd::Link) => {
                    style = styles.pop().unwrap_or_default();
                }
                Event::End(TagEnd::Table) => {
                    let t = table.take().unwrap();
                    // in a list item: under the item's text
                    let (mut ind, _) = indent(&lists, quote);
                    if !lists.is_empty() {
                        ind += "  ";
                    }
                    gap(&mut doc);
                    doc.lines.extend(render_table(&t, width, &ind));
                }
                _ => {}
            }
            continue;
        }
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                gap(&mut doc);
                heading = Some(level);
                styles.push(style);
                style = heading_style(level);
            }
            Event::End(TagEnd::Heading(level)) => {
                doc.headings.push(doc.lines.len());
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                if level == HeadingLevel::H1 {
                    let w = doc.lines.last().map_or(0, |l| l.width());
                    doc.lines
                        .push(Line::styled("─".repeat(w), Style::new().yellow()));
                }
                heading = None;
                style = styles.pop().unwrap_or_default();
            }
            // a paragraph opening a (loose) list item goes on its bullet
            Event::Start(Tag::Paragraph) if bullet.is_none() => gap(&mut doc),
            Event::End(TagEnd::Paragraph) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote)
            }
            Event::Start(Tag::List(start)) => {
                // a list inside an item: the item's text first
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                if lists.is_empty() {
                    gap(&mut doc);
                }
                lists.push(List { next: start });
            }
            Event::End(TagEnd::List(_)) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                bullet = Some(match lists.last_mut().and_then(|l| l.next.as_mut()) {
                    Some(n) => {
                        *n += 1;
                        format!("{}. ", *n - 1)
                    }
                    None => "• ".to_string(),
                });
            }
            Event::End(TagEnd::Item) => flush(&mut doc, &mut pieces, &mut bullet, &lists, quote),
            Event::Start(Tag::BlockQuote(_)) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                gap(&mut doc);
                quote += 1;
                styles.push(style);
                style = style.dark_gray();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                quote -= 1;
                style = styles.pop().unwrap_or_default();
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                gap(&mut doc);
                code = Some(Vec::new());
            }
            Event::End(TagEnd::CodeBlock) => {
                let (ind, q) = indent(&lists, quote);
                let extra = if lists.is_empty() { "" } else { "  " };
                for l in code.take().unwrap_or_default() {
                    let mut spans = q.clone();
                    spans.push(Span::raw(format!("{ind}{extra}  ")));
                    spans.push(Span::styled(l, code_style()));
                    doc.lines.push(Line::from(spans));
                }
                doc.lines.push(Line::raw(""));
            }
            Event::Start(Tag::Table(align)) => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                table = Some(Table {
                    align,
                    ..Default::default()
                });
            }
            Event::Start(Tag::Strong) => {
                styles.push(style);
                style = style.bold();
            }
            Event::Start(Tag::Emphasis) => {
                styles.push(style);
                style = style.italic();
            }
            Event::Start(Tag::Strikethrough) => {
                styles.push(style);
                style = style.crossed_out();
            }
            Event::Start(Tag::Link { .. }) => {
                styles.push(style);
                style = style.blue().underlined();
            }
            Event::End(
                TagEnd::Strong | TagEnd::Emphasis | TagEnd::Strikethrough | TagEnd::Link,
            ) => {
                style = styles.pop().unwrap_or_default();
            }
            Event::Text(s) => match code.as_mut() {
                Some(c) => c.extend(s.lines().map(str::to_string)),
                None => pieces.push((s.to_string(), style)),
            },
            Event::Code(s) => {
                let s_style = if heading.is_some() {
                    style
                } else {
                    code_style()
                };
                pieces.push((s.to_string(), s_style));
            }
            Event::SoftBreak => pieces.push((" ".into(), style)),
            Event::HardBreak => flush(&mut doc, &mut pieces, &mut bullet, &lists, quote),
            Event::Rule => {
                flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
                gap(&mut doc);
                doc.lines
                    .push(Line::styled("─".repeat(width), Style::new().dark_gray()));
            }
            _ => {}
        }
    }
    flush(&mut doc, &mut pieces, &mut bullet, &lists, quote);
    while doc.lines.last().is_some_and(|l| l.width() == 0) {
        doc.lines.pop();
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(d: &Doc) -> Vec<String> {
        d.lines
            .iter()
            .map(|l| l.to_string().trim_end().to_string())
            .collect()
    }

    #[test]
    fn headings_paragraphs_lists_and_code() {
        let d = render(
            "# Title\n\nSome *words* that wrap around here.\n\n## Part\n\n- one\n- two `code`\n  - nested\n\n1. first\n2. second\n\n```sh\nnrs\n```\n",
            24,
        );
        assert_eq!(
            text(&d),
            [
                "Title",
                "─────",
                "",
                "Some words that wrap",
                "around here.",
                "",
                "Part",
                "",
                "• one",
                "• two code",
                "   • nested",
                "",
                "1. first",
                "2. second",
                "",
                "  nrs",
            ]
        );
        assert_eq!(d.headings, [0, 6]);
    }

    #[test]
    fn tables_fit_the_width() {
        let d = render(
            "| Key | What |\n| --- | --- |\n| `s` | save the theme and everything in it |\n",
            30,
        );
        let t = text(&d);
        assert_eq!(t[0], "Key │ What");
        assert!(t[1].starts_with("────┼─"));
        assert!(t.iter().all(|l| l.width() <= 30), "{t:?}");
        assert!(t.len() > 3, "the long cell wraps: {t:?}");
    }

    #[test]
    fn the_real_guide_renders_at_any_width() {
        let p = crate::paths::guide(&crate::store::Store::locate().share);
        let Ok(md) = std::fs::read_to_string(&p) else {
            eprintln!("skipped: no {}", p.display());
            return;
        };
        // (code blocks aren't wrapped; the pane cuts them off)
        for w in [40u16, 60, 80, 100] {
            let d = render(&md, w);
            assert!(d.headings.len() > 5);
            let over: Vec<String> = d
                .lines
                .iter()
                .filter(|l| l.width() > w as usize)
                .map(|l| l.to_string())
                .filter(|l| !l.starts_with("  "))
                .collect();
            assert!(over.is_empty(), "width {w}: {over:#?}");
        }
    }
}

/// `cargo test show_the_guide -- --ignored --nocapture`: the real guide as
/// the README tab draws it (without colours), 90 columns wide.
#[cfg(test)]
#[test]
#[ignore]
fn show_the_guide() {
    let p = crate::paths::guide(&crate::store::Store::locate().share);
    let md = std::fs::read_to_string(&p).unwrap();
    for l in render(&md, 90).lines {
        println!("{l}");
    }
}
