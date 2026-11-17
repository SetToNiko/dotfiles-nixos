//! The Settings page's Behaviour tab: settings.json's values, in groups
//! (Keys, Layout, Hover, Palette pane, Derived colours, Fonts pane).

use ratatui::style::{Color, Modifier};

use crate::app::{App, Overlay};
use crate::config::{LookSettings, Settings};
use crate::hit::{Look, Looks, Underline};
use crate::keymap::{Action, Key, display_seq};
use crate::palette::GROUPS;
use crate::settings::{Capture, wrap};

/// Which highlight look a row sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookId {
    /// Clickable things under the mouse.
    Hover,
    /// The Palette pane's derivation relatives of the selected colour.
    Derived,
}

/// A row of the Behaviour tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BhRow {
    /// A blank line before a group (not selectable).
    Gap,
    /// A group's title (not selectable).
    Header(&'static str),
    /// A look as it's set (not selectable).
    Sample(LookId),
    Leader,
    Tabs,
    InnerTabs,
    SaveSizes,
    ResetSizes,
    Highlight(LookId),
    HighlightColour(LookId),
    Underline(LookId),
    UnderlineColour(LookId),
    Text(LookId),
    TextColour(LookId),
    TextWeight(LookId),
    PaneBorder,
    PreviewBorder,
    GroupOpen(usize),
    PickerAtStart,
    PickerAfterEditing,
    SampleAtStart,
    ListOpensToCurrent,
    HideBitmap,
    HideSpecial,
    Spacing,
    NerdFont,
}

impl BhRow {
    /// Rows whose value is a key of the system palette.
    pub fn is_colour(self) -> bool {
        matches!(
            self,
            BhRow::HighlightColour(_)
                | BhRow::UnderlineColour(_)
                | BhRow::TextColour(_)
                | BhRow::PaneBorder
                | BhRow::PreviewBorder
        )
    }

    pub fn selectable(self) -> bool {
        !matches!(self, BhRow::Gap | BhRow::Sample(_))
    }

    /// A button rather than a value.
    pub fn is_button(self) -> bool {
        matches!(self, BhRow::SaveSizes | BhRow::ResetSizes)
    }
}

fn look_rows(id: LookId) -> [BhRow; 7] {
    [
        BhRow::Highlight(id),
        BhRow::HighlightColour(id),
        BhRow::Underline(id),
        BhRow::UnderlineColour(id),
        BhRow::Text(id),
        BhRow::TextColour(id),
        BhRow::TextWeight(id),
    ]
}

pub fn behaviour_rows() -> Vec<BhRow> {
    use BhRow as R;
    let mut v = vec![R::Header("Keys"), R::Leader];
    v.extend([
        R::Header("Layout"),
        R::Tabs,
        R::InnerTabs,
        R::SaveSizes,
        R::ResetSizes,
    ]);
    v.push(R::Header("Hover (everything clickable)"));
    v.extend(look_rows(LookId::Hover));
    v.extend([R::PaneBorder, R::PreviewBorder, R::Sample(LookId::Hover)]);
    v.push(R::Header("Palette pane"));
    v.extend((0..GROUPS.len()).map(R::GroupOpen));
    v.extend([R::PickerAtStart, R::PickerAfterEditing]);
    v.push(R::Header(
        "Derived colours (Palette pane: what the selected colour comes from or feeds)",
    ));
    v.extend(look_rows(LookId::Derived));
    v.push(R::Sample(LookId::Derived));
    v.push(R::Header("Fonts pane"));
    v.extend([
        R::SampleAtStart,
        R::ListOpensToCurrent,
        R::HideBitmap,
        R::HideSpecial,
        R::Spacing,
        R::NerdFont,
    ]);
    // a blank line before each group's title but the first
    let mut out = Vec::new();
    for r in v {
        if matches!(r, R::Header(_)) && !out.is_empty() {
            out.push(R::Gap);
        }
        out.push(r);
    }
    out
}

/// The rows shown: a closed group keeps only its title.
pub fn visible_behaviour_rows(closed: &std::collections::HashSet<&'static str>) -> Vec<BhRow> {
    let mut shut = false;
    behaviour_rows()
        .into_iter()
        .filter(|r| match r {
            BhRow::Header(h) => {
                shut = closed.contains(h);
                true
            }
            BhRow::Gap => true,
            _ => !shut,
        })
        .collect()
}

/// The first and last selectable rows.
pub fn selectable_ends(rows: &[BhRow]) -> (usize, usize) {
    let first = rows.iter().position(|r| r.selectable()).unwrap_or(0);
    let last = rows.iter().rposition(|r| r.selectable()).unwrap_or(0);
    (first, last)
}

/// Every palette key, in the colour list's order (the choices for colours).
pub fn theme_keys() -> Vec<&'static str> {
    GROUPS
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .collect()
}

impl App {
    // ----- looks -------------------------------------------------------------

    /// A key of the active system palette as a colour (the terminal's own
    /// colours when the palette can't be read).
    pub fn theme_colour(&self, key: &str) -> Color {
        if let Some(p) = &self.system_palette
            && theme_keys().contains(&key)
        {
            let c = p.get(key);
            return Color::Rgb(c.0, c.1, c.2);
        }
        match key {
            "red" | "bg_red" => Color::Red,
            "green" | "bg_green" => Color::Green,
            "yellow" | "bg_yellow" | "orange" => Color::Yellow,
            "blue" => Color::Blue,
            "purple" => Color::Magenta,
            "aqua" => Color::Cyan,
            "fg0" | "fg1" => Color::White,
            k if k.starts_with("grey") => Color::Gray,
            _ => Color::DarkGray,
        }
    }

    /// A look's settings as colours.
    pub fn look(&self, l: &LookSettings) -> Look {
        Look {
            bg: l.highlight.then(|| self.theme_colour(&l.highlight_colour)),
            underline: Underline::parse(&l.underline)
                .map(|u| (u, self.theme_colour(&l.underline_colour))),
            fg: l.text.then(|| self.theme_colour(&l.text_colour)),
            weight: match l.text_weight.as_str() {
                "bold" => Modifier::BOLD,
                "dim" => Modifier::DIM,
                _ => Modifier::empty(),
            },
        }
    }

    pub fn looks(&self, s: &Settings) -> Looks {
        Looks {
            hover: self.look(&s.hover.look),
            derived: self.look(&s.colours.derived_highlight),
            pane: self.theme_colour(&s.hover.pane_border),
            preview: self.theme_colour(&s.hover.preview_border),
        }
    }

    fn look_settings(&mut self, id: LookId) -> &mut LookSettings {
        match id {
            LookId::Hover => &mut self.settings.hover.look,
            LookId::Derived => &mut self.settings.colours.derived_highlight,
        }
    }

    /// The key a colour row holds.
    fn colour_setting(&mut self, row: BhRow) -> Option<&mut String> {
        Some(match row {
            BhRow::HighlightColour(id) => &mut self.look_settings(id).highlight_colour,
            BhRow::UnderlineColour(id) => &mut self.look_settings(id).underline_colour,
            BhRow::TextColour(id) => &mut self.look_settings(id).text_colour,
            BhRow::PaneBorder => &mut self.settings.hover.pane_border,
            BhRow::PreviewBorder => &mut self.settings.hover.preview_border,
            _ => return None,
        })
    }

    /// Sets a colour row to a palette key (from the key list).
    pub fn set_colour_setting(&mut self, row: BhRow, key: &str) {
        if let Some(v) = self.colour_setting(row) {
            *v = key.to_string();
        }
    }

    /// Keys while the list of palette keys is open. Moving shows each key on
    /// the setting (in the draft); Enter keeps it, Esc puts `original` back.
    pub fn choose_key_key(
        &mut self,
        row: BhRow,
        mut sel: usize,
        original: String,
        k: ratatui::crossterm::event::KeyEvent,
    ) {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        let keys = theme_keys();
        let last = keys.len() - 1;
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => {
                self.overlay = Overlay::None;
                return self.set_colour_setting(row, &original);
            }
            KeyCode::Enter => {
                self.overlay = Overlay::None;
                return self.set_colour_setting(row, keys[sel]);
            }
            KeyCode::Up | KeyCode::Char('k') if !ctrl => sel = sel.saturating_sub(1),
            KeyCode::Char('p') if ctrl => sel = sel.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if !ctrl => sel = (sel + 1).min(last),
            KeyCode::Char('n') if ctrl => sel = (sel + 1).min(last),
            KeyCode::PageUp => sel = sel.saturating_sub(10),
            KeyCode::PageDown => sel = (sel + 10).min(last),
            KeyCode::Home | KeyCode::Char('g') => sel = 0,
            KeyCode::End | KeyCode::Char('G') => sel = last,
            _ => {}
        }
        // still open: preview the highlighted key
        self.set_colour_setting(row, keys[sel]);
        self.overlay = Overlay::ChooseKey { row, sel, original };
    }

    // ----- rows ----------------------------------------------------------------

    /// The value shown for a row.
    pub fn behaviour_value(&self, row: BhRow) -> String {
        let s = &self.settings;
        let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
        let look = |id: LookId| match id {
            LookId::Hover => &s.hover.look,
            LookId::Derived => &s.colours.derived_highlight,
        };
        let none = Key::new(
            ratatui::crossterm::event::KeyCode::Null,
            ratatui::crossterm::event::KeyModifiers::NONE,
        );
        match row {
            BhRow::Gap | BhRow::Header(_) | BhRow::Sample(_) => String::new(),
            BhRow::Leader => display_seq(&[self.keymap.leader], none),
            BhRow::Tabs => s.layout.tabs.clone(),
            BhRow::InnerTabs => s.layout.inner_tabs.clone(),
            BhRow::SaveSizes | BhRow::ResetSizes => "Enter".into(),
            BhRow::Highlight(id) => yes(look(id).highlight),
            BhRow::HighlightColour(id) => look(id).highlight_colour.clone(),
            BhRow::Underline(id) => look(id).underline.clone(),
            BhRow::UnderlineColour(id) => look(id).underline_colour.clone(),
            BhRow::Text(id) => yes(look(id).text),
            BhRow::TextColour(id) => look(id).text_colour.clone(),
            BhRow::TextWeight(id) => look(id).text_weight.clone(),
            BhRow::PaneBorder => s.hover.pane_border.clone(),
            BhRow::PreviewBorder => s.hover.preview_border.clone(),
            BhRow::GroupOpen(i) => yes(s
                .colours
                .open_groups
                .iter()
                .any(|g| g.eq_ignore_ascii_case(GROUPS[i].0))),
            BhRow::PickerAtStart => s.colours.picker_at_start.clone(),
            BhRow::PickerAfterEditing => s.colours.picker_after_editing.clone(),
            BhRow::SampleAtStart => s.fonts.sample_at_start.clone(),
            BhRow::ListOpensToCurrent => yes(s.fonts.list_opens_to_current_font),
            BhRow::HideBitmap => yes(s.font_list.hide_bitmap),
            BhRow::HideSpecial => yes(s.font_list.hide_cjk_emoji_math),
            BhRow::Spacing => s.font_list.spacing.clone(),
            BhRow::NerdFont => s.font_list.nerd_font.clone(),
        }
    }

    pub fn behaviour_label(row: BhRow) -> String {
        match row {
            BhRow::Gap => String::new(),
            BhRow::Header(h) => h.into(),
            BhRow::Sample(_) => "Sample".into(),
            BhRow::Leader => "Leader key (of the active keymap preset)".into(),
            BhRow::Tabs => "Page tabs (Main, Fonts, Settings)".into(),
            BhRow::InnerTabs => "Tabs inside a page (Keymaps, Behaviour, History)".into(),
            BhRow::SaveSizes => "Save the current pane sizes as the default".into(),
            BhRow::ResetSizes => "Pane sizes back to the shipped defaults".into(),
            BhRow::Highlight(_) => "Highlight the background".into(),
            BhRow::HighlightColour(_) => "Highlight colour".into(),
            BhRow::Underline(_) => "Underline".into(),
            BhRow::UnderlineColour(_) => "Underline colour".into(),
            BhRow::Text(_) => "Change the text".into(),
            BhRow::TextColour(_) => "Text colour".into(),
            BhRow::TextWeight(_) => "Text weight".into(),
            BhRow::PaneBorder => "Pane border colour".into(),
            BhRow::PreviewBorder => "Preview window frame colour".into(),
            BhRow::GroupOpen(i) => format!("Group \"{}\" open at start", GROUPS[i].0),
            BhRow::PickerAtStart => "Colour picker at start".into(),
            BhRow::PickerAfterEditing => "Colour picker after editing".into(),
            BhRow::SampleAtStart => "Sample at start".into(),
            BhRow::ListOpensToCurrent => "Installed fonts open at the role's current font".into(),
            BhRow::HideBitmap => "Hide bitmap fonts at start".into(),
            BhRow::HideSpecial => "Hide CJK / emoji / math fonts at start".into(),
            BhRow::Spacing => "Spacing filter at start".into(),
            BhRow::NerdFont => "Nerd Font filter at start".into(),
        }
    }

    /// The cursor is on the first (`up`) or last selectable row.
    pub fn behaviour_at_edge(&self, up: bool) -> bool {
        let (first, last) = selectable_ends(&visible_behaviour_rows(&self.settings_page.bh_closed));
        self.settings_page.bh_sel == if up { first } else { last }
    }

    pub(crate) fn run_behaviour_tab(&mut self, a: Action) {
        let rows = visible_behaviour_rows(&self.settings_page.bh_closed);
        if !rows[self.settings_page.bh_sel.min(rows.len() - 1)].selectable() {
            self.settings_page.bh_sel = selectable_ends(&rows).0;
        }
        let ok: Vec<bool> = rows.iter().map(|r| r.selectable()).collect();
        if crate::settings::move_over(&mut self.settings_page.bh_sel, &ok, a) {
            return;
        }
        let d = match a {
            Action::Left => -1,
            Action::Right | Action::Confirm => 1,
            _ => return,
        };
        let cycle = |cur: &str, opts: &[&str]| -> String {
            let i = opts.iter().position(|o| *o == cur).unwrap_or(0);
            opts[wrap(i, d, opts.len())].to_string()
        };
        let row = rows[self.settings_page.bh_sel];
        // a group's title: Enter toggles it, right opens, left closes
        if let BhRow::Header(h) = row {
            let closed = &mut self.settings_page.bh_closed;
            if (a == Action::Confirm && !closed.contains(h)) || a == Action::Left {
                closed.insert(h);
            } else {
                closed.remove(h);
            }
            return;
        }
        // colours: Enter opens the list of keys; left/right steps through it
        if row.is_colour() {
            let keys = theme_keys();
            let cur = self.colour_setting(row).cloned().unwrap_or_default();
            let i = keys.iter().position(|k| *k == cur).unwrap_or(0);
            if a == Action::Confirm {
                self.overlay = Overlay::ChooseKey {
                    row,
                    sel: i,
                    original: cur,
                };
            } else {
                self.set_colour_setting(row, keys[wrap(i, d, keys.len())]);
            }
            return;
        }
        match row {
            BhRow::Leader => {
                self.overlay = Overlay::Capture {
                    what: Capture::Leader,
                    keys: Vec::new(),
                };
            }
            BhRow::Highlight(id) => {
                let l = self.look_settings(id);
                l.highlight = !l.highlight;
            }
            BhRow::Underline(id) => {
                let l = self.look_settings(id);
                l.underline = cycle(&l.underline, &["off", "single", "double"]);
            }
            BhRow::Text(id) => {
                let l = self.look_settings(id);
                l.text = !l.text;
            }
            BhRow::TextWeight(id) => {
                let l = self.look_settings(id);
                l.text_weight = cycle(&l.text_weight, &["normal", "bold", "dim"]);
            }
            BhRow::SaveSizes => {
                self.settings.layout.sizes = self.sizes.clone();
                self.info("The current pane sizes become the default (when saved)");
            }
            BhRow::ResetSizes => {
                self.settings.layout.sizes = crate::config::Sizes::default();
                self.info("Default pane sizes back to the shipped ones (applied when saved)");
            }
            _ => {
                let s = &mut self.settings;
                match row {
                    BhRow::Tabs => s.layout.tabs = cycle(&s.layout.tabs, &["bottom", "top"]),
                    BhRow::InnerTabs => {
                        s.layout.inner_tabs = cycle(&s.layout.inner_tabs, &["top", "bottom"])
                    }
                    BhRow::GroupOpen(i) => {
                        let name = GROUPS[i].0;
                        if s.colours
                            .open_groups
                            .iter()
                            .any(|g| g.eq_ignore_ascii_case(name))
                        {
                            s.colours
                                .open_groups
                                .retain(|g| !g.eq_ignore_ascii_case(name));
                        } else {
                            s.colours.open_groups.push(name.to_string());
                        }
                    }
                    BhRow::PickerAtStart => {
                        s.colours.picker_at_start =
                            cycle(&s.colours.picker_at_start, &["hidden", "shown"])
                    }
                    BhRow::PickerAfterEditing => {
                        s.colours.picker_after_editing = cycle(
                            &s.colours.picker_after_editing,
                            &["hide-on-esc", "stay-open"],
                        )
                    }
                    BhRow::SampleAtStart => {
                        s.fonts.sample_at_start =
                            cycle(&s.fonts.sample_at_start, &["shown", "hidden"])
                    }
                    BhRow::ListOpensToCurrent => {
                        s.fonts.list_opens_to_current_font = !s.fonts.list_opens_to_current_font
                    }
                    BhRow::HideBitmap => s.font_list.hide_bitmap = !s.font_list.hide_bitmap,
                    BhRow::HideSpecial => {
                        s.font_list.hide_cjk_emoji_math = !s.font_list.hide_cjk_emoji_math
                    }
                    BhRow::Spacing => {
                        s.font_list.spacing = cycle(&s.font_list.spacing, &["any", "mono", "propo"])
                    }
                    BhRow::NerdFont => {
                        s.font_list.nerd_font = cycle(
                            &s.font_list.nerd_font,
                            &[
                                "any",
                                "nerd-font",
                                "nerd-font-mono",
                                "nerd-font-propo",
                                "no-nerd-font",
                            ],
                        )
                    }
                    _ => {}
                }
            }
        }
    }
}
