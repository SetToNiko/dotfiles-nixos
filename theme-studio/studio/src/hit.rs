//! What's clickable where on screen. Rebuilt on every draw; clicks, hover
//! highlights and the mouse pointer shape all use it.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};

use crate::app::Page;
use crate::preview::PaneId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// F1 / F2 / F3 tabs
    PageTab(Page),
    /// A pane's title: a click focuses the pane (1-5).
    PaneTitle(u8),
    /// A pane's body where a click only focuses it (the font sample).
    PaneBody(u8),
    /// A row of the colour list (index into its rows).
    ColourRow(usize),
    /// One of the three font roles.
    FontRole(usize),
    /// A row of the font list (index into its rows).
    FontRow(usize),
    FontSearch,
    Chip(char),
    Square,
    Hue,
    SliderH,
    SliderS,
    SliderL,
    Hex,
    /// nvim or the shell: a click hands it the keyboard.
    Preview(PaneId),
    /// A row of the Open pop-up's list (index into the matches).
    OverlayRow(usize),
    /// A border between panes that can be dragged to resize them.
    Border(BorderId),
    /// A choice in the unsaved-changes pop-up ('y', 's', or Esc as 'n').
    ConfirmChoice(char),
    /// Settings page: one of its tabs (index into settings::TABS).
    SettingsTab(u8),
    /// Settings page: a row of the tab's list (Keymaps, Behaviour, or the
    /// History tab's versions).
    SettingsRow(usize),
    /// History tab: one of the files.
    HistFile(usize),
    /// History tab: the preview (scrolls; no hover).
    SettingsDiff,
    /// Settings page: Save ('s') or Discard ('d').
    SettingsButton(char),
    /// Keymaps tab: a preset in the preset list.
    PresetRow(usize),
    /// The theme's name in the tab row (opens the theme list).
    ThemeName,
    /// The palette's name in the Palette pane's title (opens the palette
    /// list under it).
    PaletteMenu,
    /// Download fonts page: a tab, a list row, a category chip, the search
    /// box, Install ('i') / Remove ('r').
    DlTab(u8),
    DlRow(usize),
    DlChip(u8),
    DlSearch,
    DlButton(char),
    /// Download fonts page: the preview (the wheel scrolls it; no hover).
    DlPreview,
    /// The preview's title dropdowns: 0 the text kind, 1 the language.
    PreviewMenu(u8),
    /// Tournament: a pane (a click picks that font), a row (setup, results).
    TourPane(u8),
    TourRow(usize),
}

impl Target {
    /// Gets a hover highlight (and the hand pointer).
    pub fn shows_hover(self) -> bool {
        !matches!(
            self,
            Target::PaneBody(_) | Target::SettingsDiff | Target::DlPreview
        )
    }
}

/// The borders that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BorderId {
    /// Three columns: between fonts and colours.
    FontsColours,
    /// Three columns: between colours and the preview.
    ColoursPreview,
    /// Between the font list and the sample.
    ListSample,
    /// Between the colour list and the picker.
    ListPicker,
    /// Between nvim and the shell.
    PreviewSplit,
    /// Narrow layout: between the columns and the preview row.
    TopPreview,
    /// Narrow layout: between fonts and colours.
    NarrowFontsColours,
}

/// A draggable border: where it can be grabbed (`hit`, the border line of
/// the first pane), and how a position on it becomes the size share it
/// controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub id: BorderId,
    pub hit: Rect,
    /// Dragged left/right (else up/down).
    pub vertical: bool,
    /// Where the measured pane starts (or, with `from_end`, where it ends).
    pub start: u16,
    /// The length the share is of.
    pub total: u16,
    /// The share is of the pane after the border (sample, picker, preview row).
    pub from_end: bool,
}

impl Border {
    /// The border's position (x for vertical borders, y otherwise).
    pub fn pos(&self) -> u16 {
        if self.vertical {
            self.hit.x
        } else {
            self.hit.y
        }
    }

    /// The share the border at `pos` gives its pane.
    pub fn share_at(&self, pos: u16) -> f64 {
        let n = if self.from_end {
            self.start as f64 - pos as f64 - 1.0
        } else {
            pos as f64 - self.start as f64 + 1.0
        };
        (n / self.total.max(1) as f64).clamp(0.0, 1.0)
    }
}

/// Underline styles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Underline {
    Single,
    Double,
}

impl Underline {
    pub fn parse(s: &str) -> Option<Underline> {
        match s {
            "single" => Some(Underline::Single),
            "double" => Some(Underline::Double),
            _ => None,
        }
    }
}

/// A highlight look as colours (settings with the theme's keys looked up).
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    pub bg: Option<Color>,
    pub underline: Option<(Underline, Color)>,
    pub fg: Option<Color>,
    /// BOLD, DIM or nothing (with `fg`).
    pub weight: Modifier,
}

impl Look {
    pub fn apply(&self, style: Style) -> Style {
        let mut s = style;
        if let Some(bg) = self.bg {
            s = s.bg(bg);
        }
        if let Some(fg) = self.fg {
            s = s.fg(fg).add_modifier(self.weight);
        }
        match self.underline {
            Some((Underline::Single, c)) => s.add_modifier(Modifier::UNDERLINED).underline_color(c),
            // ratatui only knows single underlines: rapid blink (never used
            // otherwise) marks the cell, and out::Out makes it a double one
            Some((Underline::Double, c)) => {
                s.add_modifier(Modifier::RAPID_BLINK).underline_color(c)
            }
            None => s,
        }
    }
}

/// Every highlight look, and the hover border colours.
#[derive(Clone, Debug, PartialEq)]
pub struct Looks {
    /// Clickable things under the mouse.
    pub hover: Look,
    /// The Palette pane: the selected colour's derivation relatives.
    pub derived: Look,
    /// Pane borders under the mouse.
    pub pane: Color,
    /// Preview window frames under the mouse.
    pub preview: Color,
}

impl Default for Looks {
    fn default() -> Self {
        Looks {
            hover: Look {
                bg: None,
                underline: Some((Underline::Single, Color::Yellow)),
                fg: Some(Color::Yellow),
                weight: Modifier::empty(),
            },
            derived: Look {
                bg: Some(Color::DarkGray),
                underline: None,
                fg: None,
                weight: Modifier::empty(),
            },
            pane: Color::Cyan,
            preview: Color::Magenta,
        }
    }
}

thread_local! {
    static LOOKS: std::cell::RefCell<Looks> = std::cell::RefCell::new(Looks::default());
}

/// Sets the looks (each draw, from the settings).
pub fn set_looks(l: Looks) {
    LOOKS.with(|x| *x.borrow_mut() = l);
}

pub fn looks() -> Looks {
    LOOKS.with(|x| x.borrow().clone())
}

/// The hover look for rows and labels (configurable on the Settings page).
pub fn hover(style: Style) -> Style {
    LOOKS.with(|x| x.borrow().hover.apply(style))
}

/// The hover look for a whole row: everything but the underline, which
/// goes under the option's name only (see `hover_line`).
pub fn hover_row(style: Style) -> Style {
    LOOKS.with(|x| {
        let mut l = x.borrow().hover.clone();
        l.underline = None;
        l.apply(style)
    })
}

/// A hovered row: the row look on every span, and the full look (with the
/// underline) on span `name` without its leading and trailing spaces, so
/// only the option's name is underlined, not padding or values.
pub fn hover_line(line: ratatui::text::Line<'_>, name: usize) -> ratatui::text::Line<'_> {
    use ratatui::text::Span;
    let mut spans = Vec::new();
    for (i, s) in line.spans.into_iter().enumerate() {
        if i != name {
            spans.push(Span::styled(s.content, hover_row(s.style)));
            continue;
        }
        let text = s.content.to_string();
        let core = text.trim();
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[lead.len() + core.len()..];
        for (part, style) in [
            (lead, hover_row(s.style)),
            (core, hover(s.style)),
            (trail, hover_row(s.style)),
        ] {
            if !part.is_empty() {
                spans.push(Span::styled(part.to_string(), style));
            }
        }
    }
    ratatui::text::Line { spans, ..line }
}

/// The look of the selected colour's derivation relatives (Palette pane).
pub fn derived(style: Style) -> Style {
    LOOKS.with(|x| x.borrow().derived.apply(style))
}

#[derive(Default, Debug)]
pub struct HitMap {
    hits: Vec<(Rect, Target)>,
    /// Where the mouse is (from motion events), if known.
    pub mouse: Option<(u16, u16)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.hits.clear();
    }

    /// Records a clickable area; true when the mouse is over it, so the
    /// caller draws it hovered.
    pub fn add(&mut self, r: Rect, t: Target) -> bool {
        self.hits.push((r, t));
        t.shows_hover() && self.over(r)
    }

    pub fn over(&self, r: Rect) -> bool {
        self.mouse
            .is_some_and(|(x, y)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
    }

    /// The topmost target at a position (pop-ups are drawn, and so added, last).
    pub fn at(&self, x: u16, y: u16) -> Option<(Rect, Target)> {
        self.hits
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            .copied()
    }

    /// Where a target was drawn (the first one, if several).
    #[cfg(test)]
    pub fn rect_of(&self, t: Target) -> Option<Rect> {
        self.hits.iter().find(|(_, x)| *x == t).map(|(r, _)| *r)
    }

    /// The target under the mouse that shows a hover state.
    pub fn hovered(&self) -> Option<Target> {
        let (x, y) = self.mouse?;
        self.at(x, y).map(|(_, t)| t).filter(|t| t.shows_hover())
    }
}

/// Characters that draw a colour swatch; the cursor never covers them.
const SWATCH: &str = "█▕▏▀▄▌▐";

/// A piece of text with the keyboard cursor's `style` on the text only, not
/// the spaces around it (`base` everywhere).
pub fn cursor_spans(text: String, base: Style, style: Style) -> Vec<ratatui::text::Span<'static>> {
    use ratatui::text::Span;
    // no cursor: one span, as before (hover_line counts spans)
    let core = text.trim();
    if core.is_empty() || style == Style::new() {
        return vec![Span::styled(text.clone(), base)];
    }
    let lead = text.len() - text.trim_start().len();
    let end = lead + core.len();
    let mut out = Vec::new();
    if lead > 0 {
        out.push(Span::styled(text[..lead].to_string(), base));
    }
    out.push(Span::styled(text[lead..end].to_string(), base.patch(style)));
    if end < text.len() {
        out.push(Span::styled(text[end..].to_string(), base));
    }
    out
}

/// The keyboard cursor on a row: `style` on each piece of text, but not on
/// padding or colour swatches, which keep their look.
pub fn cursor_line(line: ratatui::text::Line<'_>, style: Style) -> ratatui::text::Line<'static> {
    let mut spans = Vec::new();
    for s in line.spans {
        let text = s.content.to_string();
        let swatch = text.trim().chars().all(|c| SWATCH.contains(c));
        if swatch {
            spans.push(ratatui::text::Span::styled(text, s.style));
        } else {
            spans.extend(cursor_spans(text, s.style, style));
        }
    }
    ratatui::text::Line::from(spans).style(line.style)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_name_is_underlined() {
        use ratatui::text::{Line, Span};
        set_looks(Looks::default());
        let line = Line::from(vec![Span::raw("   quit       "), Span::raw("q")]);
        let out = hover_line(line, 0);
        let under = |s: &Span| s.style.add_modifier.contains(Modifier::UNDERLINED);
        let texts: Vec<(&str, bool)> = out
            .spans
            .iter()
            .map(|s| (s.content.as_ref(), under(s)))
            .collect();
        assert_eq!(
            texts,
            [
                ("   ", false),
                ("quit", true),
                ("       ", false),
                ("q", false)
            ]
        );
        // the rest of the look still applies to the whole row
        assert_eq!(out.spans[3].style.fg, Some(Color::Yellow));
    }

    #[test]
    fn topmost_target_wins_and_hover_follows_the_mouse() {
        let mut m = HitMap::default();
        assert!(!m.add(Rect::new(0, 0, 10, 10), Target::Square));
        m.add(Rect::new(2, 2, 3, 1), Target::OverlayRow(0));
        assert_eq!(m.at(3, 2).map(|(_, t)| t), Some(Target::OverlayRow(0)));
        assert_eq!(m.at(3, 5).map(|(_, t)| t), Some(Target::Square));
        assert_eq!(m.at(30, 5), None);
        m.mouse = Some((3, 2));
        assert_eq!(m.hovered(), Some(Target::OverlayRow(0)));
        assert!(m.add(Rect::new(3, 2, 1, 1), Target::Hex));
        assert!(
            !m.add(Rect::new(3, 2, 1, 1), Target::PaneBody(3)),
            "bodies don't highlight"
        );
    }

    #[test]
    fn the_cursor_covers_text_not_padding_or_swatches() {
        use ratatui::text::{Line, Span};
        let rev = Style::new().reversed();
        let red = Style::new().fg(Color::Red);
        let line = cursor_line(
            Line::from(vec![
                Span::raw("   Label    "),
                Span::styled("██ ", red),
                Span::styled("    ", Style::new().bg(Color::Blue)),
                Span::raw("‹ yes ›   "),
            ]),
            rev,
        );
        let parts: Vec<(String, bool)> = line
            .spans
            .iter()
            .map(|s| {
                (
                    s.content.to_string(),
                    s.style.add_modifier.contains(Modifier::REVERSED),
                )
            })
            .collect();
        let on: Vec<&str> = parts.iter().filter(|p| p.1).map(|p| p.0.as_str()).collect();
        assert_eq!(on, ["Label", "‹ yes ›"]);
        // swatches untouched
        assert!(
            line.spans
                .iter()
                .any(|s| s.content == "██ " && s.style == red)
        );
        assert!(
            line.spans
                .iter()
                .any(|s| s.content == "    " && s.style.bg == Some(Color::Blue))
        );
        // no cursor: one span, unchanged
        assert_eq!(cursor_spans(" ab  ".into(), red, Style::new()).len(), 1);
        assert_eq!(cursor_spans(" ab  ".into(), red, rev).len(), 3);
    }
}
