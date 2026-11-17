//! The Keymaps tab's preset browser: the preset list in the right column,
//! and on the left the highlighted preset against the current keymap
//! (only what differs; `A` for everything). Enter loads a preset, `n` makes
//! a new one from it, `d` deletes one of yours, `R` resets a shipped one.
//! Like every Keymaps change, all of it waits for Save.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::App;
use crate::hit::{Target, hover_line};
use crate::keymap::{Action, Context, Keymap, all_actions, display_seq};
use crate::settings::{Deferred, Naming};

/// A line of the difference between two keymaps.
#[derive(Clone, Debug, PartialEq)]
pub enum KmDiff {
    Leader {
        now: String,
        then: String,
    },
    Context(Context),
    /// An action's keys now and in the preset (equal when shown for "all").
    Row {
        action: Action,
        now: String,
        then: String,
    },
}

fn keys_text(km: &Keymap, ctx: Context, a: Action) -> String {
    let keys: Vec<String> = km
        .keys_of(ctx, a)
        .iter()
        .map(|k| display_seq(k, km.leader))
        .collect();
    keys.join("  ")
}

/// `then` against `now`: the actions whose keys differ, by context (or every
/// action, with `all`).
pub fn keymap_diff(now: &Keymap, then: &Keymap, all: bool) -> Vec<KmDiff> {
    let none = crate::keymap::Key::new(
        ratatui::crossterm::event::KeyCode::Null,
        ratatui::crossterm::event::KeyModifiers::NONE,
    );
    let mut out = Vec::new();
    let (ln, lt) = (
        display_seq(&[now.leader], none),
        display_seq(&[then.leader], none),
    );
    if all || ln != lt {
        out.push(KmDiff::Leader { now: ln, then: lt });
    }
    for ctx in Context::ALL {
        let rows: Vec<KmDiff> = all_actions()
            .into_iter()
            .filter_map(|a| {
                let (n, t) = (keys_text(now, ctx, a), keys_text(then, ctx, a));
                ((n != t || all) && !(n.is_empty() && t.is_empty())).then_some(KmDiff::Row {
                    action: a,
                    now: n,
                    then: t,
                })
            })
            .collect();
        if !rows.is_empty() {
            out.push(KmDiff::Context(ctx));
            out.extend(rows);
        }
    }
    out
}

fn is_shipped(name: &str) -> bool {
    Keymap::builtin().iter().any(|b| b.name == name)
}

impl App {
    /// Opens the preset list, on the active preset.
    pub fn open_browse(&mut self) {
        let i = self
            .presets
            .iter()
            .position(|p| p.name == self.keymap.name)
            .unwrap_or(0);
        self.settings_page.browse = Some(i);
        self.settings_page.browse_scroll = 0;
    }

    fn highlighted(&self) -> Option<Keymap> {
        self.settings_page
            .browse
            .and_then(|i| self.presets.get(i).cloned())
    }

    /// Makes the highlighted preset the active keymap (unsaved).
    fn load_highlighted(&mut self) {
        let Some(p) = self.highlighted() else { return };
        // the current keymap's edits stay in its preset
        self.sync_preset();
        self.info(format!("Loaded the preset \"{}\" (unsaved)", p.name));
        self.keymap = p;
        self.settings_page.browse = None;
    }

    /// Keys while the preset list is open (inside the draft).
    pub fn run_browse(&mut self, a: Action) {
        let n = self.presets.len();
        let Some(i) = self.settings_page.browse else {
            return;
        };
        let p = &mut self.settings_page;
        match a {
            Action::Up | Action::Left => p.browse = Some(i.saturating_sub(1)),
            Action::Down => p.browse = Some((i + 1).min(n.saturating_sub(1))),
            Action::Top => p.browse = Some(0),
            Action::Bottom => p.browse = Some(n.saturating_sub(1)),
            Action::PageUp => p.browse_scroll = p.browse_scroll.saturating_sub(10),
            Action::PageDown => p.browse_scroll += 10,
            Action::ToggleAll => {
                p.browse_all = !p.browse_all;
                p.browse_scroll = 0;
            }
            Action::Confirm | Action::Right => return self.load_highlighted(),
            Action::Back => p.browse = None,
            Action::SaveNamed => {
                let name = self.presets[i].name.clone();
                self.overlay = crate::app::Overlay::Prompt {
                    title: format!("Name for a new preset made from \"{name}\""),
                    input: String::new(),
                    purpose: Naming::PresetFrom(name),
                };
            }
            Action::Delete if is_shipped(&self.presets[i].name) => {
                self.info("Shipped presets can't be deleted (R resets them)")
            }
            Action::Delete => {
                let name = self.presets[i].name.clone();
                self.overlay = crate::app::Overlay::Ask {
                    question: format!("Delete the preset \"{name}\"? (Save makes it final)"),
                    then: Deferred::DeletePreset(name),
                };
            }
            Action::ResetPreset if is_shipped(&self.presets[i].name) => {
                let name = self.presets[i].name.clone();
                self.overlay = crate::app::Overlay::Ask {
                    question: format!("Reset \"{name}\" to the shipped keys?"),
                    then: Deferred::ResetPreset(name),
                };
            }
            Action::ResetPreset => {
                self.info("Only shipped presets can be reset; this one is yours")
            }
            _ => {}
        }
        if self.settings_page.browse != Some(i) {
            self.settings_page.browse_scroll = 0;
        }
    }

    /// A click on a preset: highlights it; on the highlighted one, loads it.
    pub fn preset_click(&mut self, i: usize) {
        if self.settings_page.browse == Some(i) {
            self.load_highlighted();
        } else {
            self.settings_page.browse = Some(i);
            self.settings_page.browse_scroll = 0;
        }
    }

    /// A new preset, a copy of `from`, highlighted in the list.
    pub fn new_preset_from(&mut self, from: &str, name: String) {
        if self.presets.iter().any(|p| p.name == name) {
            return self.error(format!("There's already a preset \"{name}\""));
        }
        let Some(mut km) = self.presets.iter().find(|p| p.name == from).cloned() else {
            return;
        };
        km.name = name.clone();
        self.presets.push(km);
        self.settings_page.browse = Some(self.presets.len() - 1);
        self.info(format!(
            "New preset \"{name}\" from \"{from}\" (unsaved); Enter loads it"
        ));
    }

    /// Deletes a preset (the active one: Vim becomes active).
    pub fn delete_preset(&mut self, name: &str) {
        self.presets.retain(|p| p.name != name);
        if self.keymap.name == name {
            self.keymap = self
                .presets
                .iter()
                .find(|p| p.name == "Vim")
                .cloned()
                .unwrap_or_else(Keymap::vim);
        }
        if let Some(i) = self.settings_page.browse {
            self.settings_page.browse = Some(i.min(self.presets.len().saturating_sub(1)));
        }
        self.info(format!("Deleted the preset \"{name}\" (unsaved)"));
    }

    /// Puts a shipped preset back to its shipped keys.
    pub fn reset_preset(&mut self, name: &str) {
        let Some(b) = Keymap::builtin().into_iter().find(|b| b.name == name) else {
            return;
        };
        if let Some(p) = self.presets.iter_mut().find(|p| p.name == name) {
            *p = b.clone();
        }
        if self.keymap.name == name {
            self.keymap = b;
        }
        self.info(format!("\"{name}\" is back to the shipped keys (unsaved)"));
    }
}

// ----- drawing ---------------------------------------------------------------

/// The browser: the difference on the left (`diff`), the list on the right
/// (`list`).
pub fn draw_browse(f: &mut Frame, app: &mut App, diff: Rect, list: Rect) {
    let Some(sel) = app.settings_page.browse else {
        return;
    };
    let Some(then) = app.presets.get(sel).cloned() else {
        return;
    };

    // the list
    let mut lines = vec![Line::styled("Presets", Style::new().bold()), Line::raw("")];
    let top = list.y + 2;
    for (i, p) in app.presets.iter().enumerate() {
        if top + i as u16 >= list.bottom() {
            break;
        }
        let active = p.name == app.keymap.name;
        let style = if i == sel {
            Style::new().reversed()
        } else {
            Style::new()
        };
        let r = Rect {
            x: list.x,
            y: top + i as u16,
            width: list.width,
            height: 1,
        };
        let hovered = app.hits.add(r, Target::PresetRow(i)) && i != sel;
        let line = Line::from(vec![
            Span::styled(format!(" {:<16}", p.name), style),
            Span::styled(
                format!(
                    " {}{}",
                    if is_shipped(&p.name) {
                        "shipped"
                    } else {
                        "yours"
                    },
                    if active { ", active" } else { "" }
                ),
                Style::new().dark_gray(),
            ),
        ]);
        // only the name is underlined
        lines.push(if hovered { hover_line(line, 0) } else { line });
    }
    lines.push(Line::raw(""));
    for h in [
        "Enter  load it (then Save)",
        "n      new preset from it",
        "d      delete (yours only)",
        "R      reset (shipped only)",
        "A      everything / differences",
        "PgUp/PgDn  scroll the keys",
        "Esc    back",
    ] {
        lines.push(Line::styled(h, Style::new().dark_gray()));
    }
    f.render_widget(
        Paragraph::new(lines).block(
            Block::new()
                .borders(Borders::LEFT)
                .border_style(Style::new().dark_gray()),
        ),
        list,
    );

    // the difference
    let all = app.settings_page.browse_all;
    let rows = keymap_diff(&app.keymap, &then, all);
    let changed = keymap_diff(&app.keymap, &then, false)
        .iter()
        .filter(|d| !matches!(d, KmDiff::Context(_)))
        .count();
    let [head, body] = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(diff);
    let title = if then.name == app.keymap.name {
        format!(" {} (the current keymap)", then.name)
    } else if changed == 0 {
        format!(" {} → {}: the same keys", app.keymap.name, then.name)
    } else {
        format!(" {} → {}: {changed} differ", app.keymap.name, then.name)
    };
    f.render_widget(
        Paragraph::new(vec![
            Line::styled(title, Style::new().bold()),
            Line::from(vec![
                Span::styled(" changed ", Style::new().black().on_yellow()),
                Span::styled(" current → preset", Style::new().dark_gray()),
            ]),
        ]),
        head,
    );
    app.hits.add(body, Target::SettingsDiff);
    let h = body.height as usize;
    let max = rows.len().saturating_sub(h);
    app.settings_page.browse_scroll = app.settings_page.browse_scroll.min(max);
    let w = (body.width as usize).saturating_sub(4).min(46);
    let out: Vec<Line> = rows
        .iter()
        .skip(app.settings_page.browse_scroll)
        .take(h)
        .map(|d| match d {
            KmDiff::Context(c) => {
                Line::styled(format!(" {}", c.title()), Style::new().bold().cyan())
            }
            KmDiff::Leader { now, then } | KmDiff::Row { now, then, .. } => {
                let label = match d {
                    KmDiff::Row { action, .. } => action.describe(),
                    _ => "leader".into(),
                };
                let label = format!("   {:<w$} ", label.chars().take(w).collect::<String>());
                let none = |s: &str| {
                    if s.is_empty() {
                        "no keys".to_string()
                    } else {
                        s.to_string()
                    }
                };
                if now == then {
                    Line::from(vec![
                        Span::raw(label),
                        Span::styled(none(now), Style::new().yellow()),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(label, Style::new().black().on_yellow()),
                        Span::styled(
                            none(now),
                            Style::new().dark_gray().add_modifier(Modifier::CROSSED_OUT),
                        ),
                        Span::raw(" → "),
                        Span::styled(none(then), Style::new().yellow().bold()),
                    ])
                }
            }
        })
        .collect();
    f.render_widget(Paragraph::new(out), body);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differences_between_presets() {
        let (vim, emacs) = (Keymap::vim(), Keymap::emacs());
        let d = keymap_diff(&vim, &emacs, false);
        assert!(d.contains(&KmDiff::Leader {
            now: "<Space>".into(),
            then: "<C-c>".into()
        }));
        assert!(d.contains(&KmDiff::Context(Context::Global)));
        assert!(d.contains(&KmDiff::Row {
            action: Action::Quit,
            now: "q".into(),
            then: "<C-x><C-c>".into()
        }));
        // the same keys aren't listed, unless everything is asked for
        let same = KmDiff::Row {
            action: Action::PageMain,
            now: "<F1>".into(),
            then: "<F1>".into(),
        };
        assert!(!d.contains(&same));
        assert!(keymap_diff(&vim, &emacs, true).contains(&same));
        assert!(keymap_diff(&vim, &vim, false).is_empty());
    }
}
