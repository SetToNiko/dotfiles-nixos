//! Live preview: a model of the i3 desktop with real programs in it. nvim
//! (your config), an interactive shell (starship) and yazi run in hidden
//! terminals; their screens are drawn with the draft palette, so nothing has
//! to be reloaded except nvim, which re-reads the draft colours file itself.

use std::io::{Read, Write};
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::color::Rgb;
use crate::desktop::{self, BarBlock, Roles};
use crate::palette::{self, KEYS, Palette};
use crate::store::TerminalMap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PaneId {
    Nvim,
    Shell,
    Yazi,
}

/// A program running in a hidden terminal.
pub struct Pane {
    parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    size: (u16, u16),
    /// Typed once the program has drawn something (the shell's first prompt),
    /// so it isn't echoed early as type-ahead.
    pending_input: Option<Vec<u8>>,
    spawned: std::time::Instant,
}

fn nz(n: u16) -> NonZeroU16 {
    NonZeroU16::new(n.max(1)).unwrap()
}

impl Pane {
    fn spawn(cmd: CommandBuilder, rows: u16, cols: u16, dirty: Arc<AtomicBool>) -> Result<Pane> {
        let (rows, cols) = (rows.max(2), cols.max(2));
        let pair = native_pty_system()
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("opening a pty")?;
        let child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), 0)));
        let p = parser.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 16384];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        p.lock().unwrap().process(&buf[..n]);
                        dirty.store(true, Ordering::Relaxed);
                    }
                }
            }
        });
        Ok(Pane {
            parser,
            master: pair.master,
            writer,
            child,
            size: (rows, cols),
            pending_input: None,
            spawned: std::time::Instant::now(),
        })
    }

    /// Follows the preview window's size, like a real terminal window would.
    fn resize(&mut self, rows: u16, cols: u16) {
        let (rows, cols) = (rows.max(2), cols.max(2));
        if self.size != (rows, cols) {
            self.size = (rows, cols);
            let _ = self.master.resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            });
            self.parser.lock().unwrap().set_size(nz(rows), nz(cols));
        }
    }

    /// Sends `pending_input` once the program has printed something.
    fn flush_pending(&mut self) {
        if self.pending_input.is_none() {
            return;
        }
        let ready = !self
            .parser
            .lock()
            .unwrap()
            .screen()
            .contents()
            .trim()
            .is_empty()
            || self.spawned.elapsed() > Duration::from_secs(3);
        if ready && let Some(bytes) = self.pending_input.take() {
            self.write(&bytes);
        }
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let _ = self
            .writer
            .write_all(bytes)
            .and_then(|_| self.writer.flush());
    }

    fn bracketed_paste(&self) -> bool {
        self.parser.lock().unwrap().screen().bracketed_paste()
    }

    fn application_cursor(&self) -> bool {
        self.parser.lock().unwrap().screen().application_cursor()
    }

    /// Draws the terminal screen, mapping its colours through `colors`.
    fn render(&self, buf: &mut Buffer, area: Rect, colors: &TermColors, cursor: bool) {
        let parser = self.parser.lock().unwrap();
        let screen = parser.screen();
        let (rows, cols) = screen.size();
        for y in 0..area.height.min(rows.get()) {
            for x in 0..area.width.min(cols.get()) {
                let Some(cell) = screen.cell(y, x) else {
                    continue;
                };
                let Some(out) = buf.cell_mut((area.x + x, area.y + y)) else {
                    continue;
                };
                if cell.is_wide_continuation() {
                    continue;
                }
                let mut fg = colors.map(cell.fgcolor(), colors.fg);
                let mut bg = colors.map(cell.bgcolor(), colors.bg);
                if cell.inverse() {
                    std::mem::swap(&mut fg, &mut bg);
                }
                if cell.dim() {
                    fg = blend(fg, bg, 0.4);
                }
                let mut style = Style::new().fg(fg.color()).bg(bg.color());
                if cell.bold() {
                    style = style.add_modifier(Modifier::BOLD);
                }
                if cell.italic() {
                    style = style.add_modifier(Modifier::ITALIC);
                }
                if cell.underline() {
                    style = style.add_modifier(Modifier::UNDERLINED);
                }
                let sym = if cell.has_contents() {
                    cell.contents()
                } else {
                    " "
                };
                out.set_symbol(sym).set_style(style);
            }
        }
        if cursor && !screen.hide_cursor() {
            let (cy, cx) = screen.cursor_position();
            if cy < area.height
                && cx < area.width
                && let Some(out) = buf.cell_mut((area.x + cx, area.y + cy))
            {
                out.set_style(Style::new().fg(colors.bg.color()).bg(colors.fg.color()));
            }
        }
    }
}

impl Drop for Pane {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn blend(a: Rgb, b: Rgb, t: f64) -> Rgb {
    let ch = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2))
}

/// Terminal colours as Ghostty would set them from the palette
/// (terminal-colors.json): default fg/bg and the 16 ANSI colours.
pub struct TermColors {
    pub fg: Rgb,
    pub bg: Rgb,
    pub ansi: [Rgb; 16],
}

impl TermColors {
    pub fn new(p: &Palette, map: Option<&TerminalMap>) -> TermColors {
        let k = |name: &str, fallback: &str| p.get(palette::key(name).unwrap_or(fallback));
        let mut ansi = [Rgb(0, 0, 0); 16];
        const DEFAULT: [&str; 16] = [
            "bg1", "red", "green", "yellow", "blue", "purple", "aqua", "fg0", "grey0", "red",
            "green", "yellow", "blue", "purple", "aqua", "fg1",
        ];
        for (i, slot) in ansi.iter_mut().enumerate() {
            let name = map
                .and_then(|m| m.ansi.get(i))
                .map(String::as_str)
                .unwrap_or(DEFAULT[i]);
            *slot = k(name, DEFAULT[i]);
        }
        TermColors {
            fg: k(map.map_or("fg0", |m| m.foreground.as_str()), "fg0"),
            bg: k(map.map_or("bg0", |m| m.background.as_str()), "bg0"),
            ansi,
        }
    }

    fn map(&self, c: vt100::Color, default: Rgb) -> Rgb {
        match c {
            vt100::Color::Default => default,
            vt100::Color::Idx(i) if i < 16 => self.ansi[i as usize],
            vt100::Color::Idx(i) => xterm256(i),
            vt100::Color::Rgb(r, g, b) => Rgb(r, g, b),
        }
    }
}

/// The standard xterm colours 16-255 (6×6×6 cube, then greys).
fn xterm256(i: u8) -> Rgb {
    if i >= 232 {
        let v = 8 + (i - 232) * 10;
        return Rgb(v, v, v);
    }
    let i = i - 16;
    let level = |n: u8| if n == 0 { 0 } else { 55 + n * 40 };
    Rgb(level(i / 36), level(i / 6 % 6), level(i % 6))
}

/// Bytes a terminal sends for a key.
pub fn key_bytes(k: KeyEvent, app_cursor: bool) -> Vec<u8> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);
    let arrow = |c: char| {
        if app_cursor {
            format!("\x1bO{c}")
        } else {
            format!("\x1b[{c}")
        }
        .into_bytes()
    };
    let mut out = match k.code {
        KeyCode::Char(c) if ctrl && c.is_ascii_alphabetic() => {
            vec![(c.to_ascii_lowercase() as u8) & 0x1f]
        }
        KeyCode::Char(' ') if ctrl => vec![0],
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => arrow('A'),
        KeyCode::Down => arrow('B'),
        KeyCode::Right => arrow('C'),
        KeyCode::Left => arrow('D'),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::F(n @ 1..=4) => format!("\x1bO{}", (b'P' + n - 1) as char).into_bytes(),
        KeyCode::F(n @ 5..=11) => {
            let code = [15, 17, 18, 19, 20, 21, 23][(n - 5) as usize];
            format!("\x1b[{code}~").into_bytes()
        }
        _ => Vec::new(),
    };
    if alt && !out.is_empty() {
        out.insert(0, 0x1b);
    }
    out
}

/// What the preview pane shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Desktop,
    Swatches,
}

pub struct Preview {
    configs: PathBuf,
    /// Where the shell and yazi start: the dotfiles folder.
    workdir: PathBuf,
    draft_dir: PathBuf,
    draft: PathBuf,
    written: Option<Palette>,
    dirty: Arc<AtomicBool>,
    nvim: Option<Pane>,
    shell: Option<Pane>,
    yazi: Option<Pane>,
    errors: Vec<String>,
    roles: Roles,
    bar: Arc<Mutex<Option<Vec<BarBlock>>>>,
    pub mode: Mode,
    /// The right-hand window shows yazi instead of the shell.
    pub show_yazi: bool,
    pub show_rofi: bool,
    /// Pane receiving the keyboard, if any (F12 cycles).
    pub focus: Option<PaneId>,
    /// Where each visible pane was drawn, for mouse clicks.
    pub rects: Vec<(PaneId, Rect)>,
    /// Programs aren't started (tests).
    headless: bool,
}

impl Preview {
    pub fn new(configs: &Path) -> Preview {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        remove_stale_drafts(&base);
        // pid, plus a counter so several previews in one process (tests) differ
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let draft_dir = base.join(format!("theme-studio-{}-{n}", std::process::id()));
        let _ = std::fs::create_dir_all(&draft_dir);
        let dirty = Arc::new(AtomicBool::new(true));
        let bar = Arc::new(Mutex::new(None));
        {
            let (bar, dirty, configs) = (bar.clone(), dirty.clone(), configs.to_path_buf());
            std::thread::spawn(move || {
                let blocks = desktop::read_bar(&configs, Duration::from_secs(3));
                *bar.lock().unwrap() = Some(blocks);
                dirty.store(true, Ordering::Relaxed);
            });
        }
        Preview {
            configs: configs.to_path_buf(),
            workdir: configs
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| configs.to_path_buf()),
            draft: draft_dir.join("colors.sh"),
            draft_dir,
            written: None,
            dirty,
            nvim: None,
            shell: None,
            yazi: None,
            errors: Vec::new(),
            roles: Roles::load(configs),
            bar,
            mode: Mode::Desktop,
            show_yazi: false,
            show_rofi: false,
            focus: None,
            rects: Vec::new(),
            headless: false,
        }
    }

    /// A preview that never starts programs (for tests).
    #[cfg(test)]
    pub fn headless(configs: &Path) -> Preview {
        let mut p = Preview::new(configs);
        p.headless = true;
        p
    }

    #[cfg(test)]
    pub fn set_headless(&mut self) {
        self.headless = true;
    }

    /// F12: editor → each preview window → editor.
    pub fn cycle_focus(&mut self) {
        let panes = self.focusable();
        // the right-hand slot is one stop whether it shows the shell or yazi
        let slot = |a: PaneId, b: PaneId| a == b || (a != PaneId::Nvim && b != PaneId::Nvim);
        self.focus = match self
            .focus
            .and_then(|f| panes.iter().position(|p| slot(*p, f)))
        {
            None => panes.first().copied(),
            Some(i) => panes.get(i + 1).copied(),
        };
    }

    /// Y: the right-hand window switches between the shell and yazi.
    pub fn toggle_yazi(&mut self) {
        self.show_yazi = !self.show_yazi;
        if let Some(f) = self.focus
            && f != PaneId::Nvim
        {
            self.focus = Some(if self.show_yazi {
                PaneId::Yazi
            } else {
                PaneId::Shell
            });
        }
    }

    /// True once since the last call if a program printed something.
    /// The folder for the draft colours (and the font samples).
    pub fn draft_dir(&self) -> &Path {
        &self.draft_dir
    }

    /// Set by anything that needs a redraw (programs printing, samples ready).
    pub fn dirty_flag(&self) -> Arc<AtomicBool> {
        self.dirty.clone()
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }

    /// Writes the draft palette for nvim (same format as ~/.config/colors/colors.sh).
    pub fn sync_draft(&mut self, p: &Palette) {
        if self.written.as_ref() == Some(p) {
            return;
        }
        let text: String = KEYS
            .iter()
            .map(|k| format!("{k}={}\n", p.get(k).hex()))
            .collect();
        if std::fs::write(&self.draft, text).is_ok() {
            self.written = Some(p.clone());
        }
    }

    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        match id {
            PaneId::Nvim => self.nvim.as_mut(),
            PaneId::Shell => self.shell.as_mut(),
            PaneId::Yazi => self.yazi.as_mut(),
        }
    }

    /// Panes that can take the keyboard, in F12 order.
    pub fn focusable(&self) -> Vec<PaneId> {
        self.rects.iter().map(|(id, _)| *id).collect()
    }

    pub fn send_key(&mut self, id: PaneId, k: KeyEvent) {
        if let Some(pane) = self.pane_mut(id) {
            let bytes = key_bytes(k, pane.application_cursor());
            pane.write(&bytes);
        }
    }

    pub fn send_paste(&mut self, id: PaneId, text: &str) {
        if let Some(pane) = self.pane_mut(id) {
            if pane.bracketed_paste() {
                pane.write(format!("\x1b[200~{text}\x1b[201~").as_bytes());
            } else {
                pane.write(text.as_bytes());
            }
        }
    }

    fn command(&self, program: &str) -> CommandBuilder {
        let mut c = CommandBuilder::new(program);
        c.cwd(&self.workdir);
        c.env("TERM", "xterm-256color");
        c.env("COLORTERM", "truecolor");
        c.env("THEME_COLORS", &self.draft);
        c
    }

    /// Starts a pane's program the first time it's shown, at the size it's shown at.
    fn ensure(&mut self, id: PaneId, rows: u16, cols: u16) {
        if self.headless || self.pane_mut(id).is_some() {
            if let Some(p) = self.pane_mut(id) {
                p.resize(rows, cols);
            }
            return;
        }
        let cmd = match id {
            PaneId::Nvim => {
                let mut c = self.command("nvim");
                // read-only, no swap file: it's only there to be looked at
                c.args(["-R", "-n"]);
                c.arg(self.configs.join("nvim/lua/theme.lua"));
                c
            }
            PaneId::Shell => {
                let mut c = self.command("bash");
                c.arg("-i");
                c.env("HISTFILE", "/dev/null"); // the demo commands stay out of your history
                c
            }
            PaneId::Yazi => {
                let mut c = self.command("yazi");
                c.arg(&self.workdir);
                c
            }
        };
        match Pane::spawn(cmd, rows, cols, self.dirty.clone()) {
            Ok(mut pane) => {
                if id == PaneId::Shell {
                    // Typed into the shell like you would: real prompts, real colours.
                    pane.pending_input = Some(
                        b"ls --color=auto\ngit --no-pager log --oneline --graph --decorate -4\nfalse\n"
                            .to_vec(),
                    );
                }
                match id {
                    PaneId::Nvim => self.nvim = Some(pane),
                    PaneId::Shell => self.shell = Some(pane),
                    PaneId::Yazi => self.yazi = Some(pane),
                }
            }
            Err(e) => self.errors.push(format!("{id:?}: {e:#}")),
        }
    }

    /// Draws the model desktop: two tiled windows with i3 borders, the bar,
    /// and optionally the rofi menu on top.
    pub fn draw_desktop(
        &mut self,
        f: &mut Frame,
        area: Rect,
        p: &Palette,
        map: Option<&TerminalMap>,
        name: &str,
        // nvim above the shell (a preview column) instead of side by side
        stack: bool,
    ) {
        let colors = TermColors::new(p, map);
        let roles = self.roles.clone();
        let role = |r: &str, i: usize| roles.get(p, r, i);

        // Desktop and bar
        let bar_area = Rect {
            y: area.y + area.height.saturating_sub(1),
            height: 1.min(area.height),
            ..area
        };
        let desk = Rect {
            height: area.height.saturating_sub(1),
            ..area
        };
        f.render_widget(
            Block::new().style(Style::new().bg(p.get("bg_dim").color())),
            desk,
        );
        self.draw_bar(f, bar_area, p);

        // Two windows: side by side when wide enough, else stacked
        let right_id = if self.show_yazi {
            PaneId::Yazi
        } else {
            PaneId::Shell
        };
        let (a, b) = if !stack && desk.width >= 2 * 30 {
            let w = desk.width / 2;
            (
                Rect { width: w, ..desk },
                Rect {
                    x: desk.x + w,
                    width: desk.width - w,
                    ..desk
                },
            )
        } else {
            let h = desk.height / 2;
            (
                Rect { height: h, ..desk },
                Rect {
                    y: desk.y + h,
                    height: desk.height - h,
                    ..desk
                },
            )
        };
        self.rects.clear();
        // The window with the keyboard looks focused; otherwise nvim does.
        let focused = self.focus.unwrap_or(PaneId::Nvim);
        for (id, r) in [(PaneId::Nvim, a), (right_id, b)] {
            if r.width < 4 || r.height < 3 {
                continue;
            }
            // column 4 is child_border, what a `border pixel` window shows
            let border = if id == focused {
                role("client.focused", 4)
            } else {
                role("client.unfocused", 4)
            };
            // Number and name on the border: 6 / 7 jump here (i3 itself shows no
            // title with `border pixel`; this is studio chrome).
            let (n, label) = match id {
                PaneId::Nvim => (6, "nvim"),
                PaneId::Shell => (7, "shell"),
                PaneId::Yazi => (7, "yazi"),
            };
            let label_fg = if id == focused {
                border
            } else {
                p.get("grey1")
            };
            let block = Block::bordered()
                .title(Line::from(vec![
                    crate::ui::panel_number(n),
                    Span::styled(format!("{label} "), Style::new().fg(label_fg.color())),
                ]))
                .border_style(Style::new().fg(border.color()).bg(p.get("bg_dim").color()));
            let inner = block.inner(r);
            f.render_widget(block, r);
            self.ensure(id, inner.height, inner.width);
            if let Some(pane) = self.pane_mut(id) {
                pane.flush_pending();
            }
            f.render_widget(
                Block::new().style(Style::new().bg(colors.bg.color()).fg(colors.fg.color())),
                inner,
            );
            if let Some(pane) = match id {
                PaneId::Nvim => self.nvim.as_ref(),
                PaneId::Shell => self.shell.as_ref(),
                PaneId::Yazi => self.yazi.as_ref(),
            } {
                pane.render(f.buffer_mut(), inner, &colors, id == focused);
            } else if self.headless {
                f.render_widget(Paragraph::new(format!("{id:?}")), inner);
            }
            self.rects.push((id, r));
        }
        if !self.errors.is_empty() {
            let r = Rect {
                height: (self.errors.len() as u16).min(desk.height),
                ..desk
            };
            let lines: Vec<Line> = self
                .errors
                .iter()
                .map(|e| Line::styled(e.clone(), Style::new().fg(p.get("red").color())))
                .collect();
            f.render_widget(Paragraph::new(lines), r);
        }
        if self.show_rofi {
            self.draw_rofi(f, desk, p, name);
        }
    }

    fn draw_bar(&self, f: &mut Frame, area: Rect, p: &Palette) {
        let role = |r: &str, i: usize| self.roles.get(p, r, i);
        let bg = role("background", 0);
        let mut left = vec![
            Span::styled(
                " 1 ",
                Style::new()
                    .bg(role("focused_workspace", 1).color())
                    .fg(role("focused_workspace", 2).color()),
            ),
            Span::styled(
                " 2 ",
                Style::new()
                    .bg(role("inactive_workspace", 1).color())
                    .fg(role("inactive_workspace", 2).color()),
            ),
            Span::styled(
                " 3 ",
                Style::new()
                    .bg(role("inactive_workspace", 1).color())
                    .fg(role("inactive_workspace", 2).color()),
            ),
        ];
        let statusline = role("statusline", 0);
        let sep = Span::styled(" │ ", Style::new().fg(role("separator", 0).color()));
        let mut right: Vec<Span> = Vec::new();
        match &*self.bar.lock().unwrap() {
            Some(blocks) => {
                for (i, b) in blocks.iter().enumerate() {
                    if i > 0 {
                        right.push(sep.clone());
                    }
                    right.push(Span::styled(
                        b.text.clone(),
                        Style::new().fg(b.color.unwrap_or(statusline).color()),
                    ));
                }
            }
            None => right.push(Span::styled(
                "reading i3blocks…",
                Style::new().fg(role("separator", 0).color()),
            )),
        }
        right.push(" ".into());
        let used: usize = left.iter().chain(&right).map(|s| s.width()).sum();
        left.push(
            " ".repeat((area.width as usize).saturating_sub(used))
                .into(),
        );
        left.extend(right);
        f.render_widget(
            Paragraph::new(Line::from(left))
                .style(Style::new().bg(bg.color()).fg(statusline.color())),
            area,
        );
    }

    /// The theme picker as rofi would show it, with this palette's roles.
    fn draw_rofi(&self, f: &mut Frame, area: Rect, p: &Palette, name: &str) {
        let role = |r: &str| self.roles.get(p, &format!("rofi.{r}"), 0);
        let w = 54.min(area.width);
        let h = 11.min(area.height);
        let r = Rect {
            x: area.x + (area.width - w) / 2,
            y: area.y + (area.height - h) / 2,
            width: w,
            height: h,
        };
        let (bg, fg) = (role("background"), role("foreground"));
        f.render_widget(Clear, r);
        let block = Block::bordered()
            .border_style(Style::new().fg(role("border-color").color()).bg(bg.color()))
            .style(Style::new().bg(bg.color()).fg(fg.color()));
        let inner = block.inner(r);
        f.render_widget(block, r);
        let swatches = |sel: bool| -> Vec<Span<'static>> {
            [
                "bg0", "fg0", "red", "orange", "yellow", "green", "aqua", "blue", "purple",
            ]
            .iter()
            .map(|k| {
                Span::styled(
                    "██",
                    Style::new().fg(p.get(k).color()).bg(if sel {
                        role("selected-normal-background")
                    } else {
                        bg
                    }
                    .color()),
                )
            })
            .collect()
        };
        let row = |label: &str, sel: bool| {
            let style = if sel {
                Style::new()
                    .bg(role("selected-normal-background").color())
                    .fg(role("selected-normal-foreground").color())
            } else {
                Style::new().fg(fg.color())
            };
            let mut spans = vec![Span::styled(
                format!(" {label:<22}"),
                style.add_modifier(Modifier::BOLD),
            )];
            // only the palette being edited has known colours; the other
            // rows are just names
            if sel {
                spans.extend(swatches(sel));
            }
            spans.push(Span::styled(" ".repeat(inner.width as usize), style));
            Line::from(spans)
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(" Theme: ", Style::new().fg(fg.color())),
                Span::styled("▏", Style::new().fg(fg.color())),
            ]),
            Line::styled(
                "─".repeat(inner.width as usize),
                Style::new().fg(role("separatorcolor").color()),
            ),
            Line::styled(
                " Palette \"Old Theme\" deleted",
                Style::new()
                    .fg(p.get("red").color())
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(
                " Enter: apply   Alt+BackSpace: delete",
                Style::new().fg(fg.color()),
            ),
            Line::styled(
                "─".repeat(inner.width as usize),
                Style::new().fg(role("separatorcolor").color()),
            ),
            row(name, true),
            row("Gruvbox Dark", false),
            row("Nord", false),
            row("Tokyo Night", false),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.nvim = None;
        self.shell = None;
        self.yazi = None;
        let _ = std::fs::remove_dir_all(&self.draft_dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xterm_colours() {
        assert_eq!(xterm256(16), Rgb(0, 0, 0));
        assert_eq!(xterm256(196), Rgb(255, 0, 0));
        assert_eq!(xterm256(231), Rgb(255, 255, 255));
        assert_eq!(xterm256(232), Rgb(8, 8, 8));
        assert_eq!(xterm256(255), Rgb(238, 238, 238));
    }

    #[test]
    fn keys_become_terminal_bytes() {
        let k = |c, m| KeyEvent::new(c, m);
        assert_eq!(
            key_bytes(k(KeyCode::Char('a'), KeyModifiers::NONE), false),
            b"a"
        );
        assert_eq!(
            key_bytes(k(KeyCode::Char('c'), KeyModifiers::CONTROL), false),
            [3]
        );
        assert_eq!(
            key_bytes(k(KeyCode::Up, KeyModifiers::NONE), false),
            b"\x1b[A"
        );
        assert_eq!(
            key_bytes(k(KeyCode::Up, KeyModifiers::NONE), true),
            b"\x1bOA"
        );
        assert_eq!(
            key_bytes(k(KeyCode::Char('x'), KeyModifiers::ALT), false),
            b"\x1bx"
        );
        assert_eq!(
            key_bytes(k(KeyCode::F(5), KeyModifiers::NONE), false),
            b"\x1b[15~"
        );
    }

    /// A real program in a pane: its ANSI colours come out as the palette's.
    #[test]
    fn pane_maps_ansi_colours_to_the_palette() {
        let dirty = Arc::new(AtomicBool::new(false));
        let mut cmd = CommandBuilder::new("printf");
        cmd.arg("\x1b[31mred\x1b[0m \x1b[42mbg\x1b[0m");
        let Ok(pane) = Pane::spawn(cmd, 5, 20, dirty.clone()) else {
            eprintln!("skipped: no pty (build sandbox)");
            return;
        };
        for _ in 0..100 {
            if pane
                .parser
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains("bg")
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(dirty.load(Ordering::Relaxed));
        let p = Palette::blank();
        let colors = TermColors::new(&p, None);
        let mut buf = Buffer::empty(Rect::new(0, 0, 20, 5));
        pane.render(&mut buf, Rect::new(0, 0, 20, 5), &colors, false);
        assert_eq!(buf[(0, 0)].symbol(), "r");
        assert_eq!(buf[(0, 0)].fg, p.get("red").color());
        assert_eq!(buf[(4, 0)].bg, p.get("green").color());
        assert_eq!(
            buf[(10, 0)].bg,
            p.get("bg0").color(),
            "default background is bg0"
        );
    }

    #[test]
    fn draft_file_matches_colors_sh_format() {
        let dir = std::env::temp_dir().join(format!("theme-studio-prev-{}", std::process::id()));
        let mut prev = Preview::headless(&dir);
        let p = Palette::blank();
        prev.sync_draft(&p);
        let text = std::fs::read_to_string(&prev.draft).unwrap();
        assert_eq!(text.lines().count(), 34);
        assert_eq!(
            text.lines().nth(1),
            Some(format!("bg0={}", p.get("bg0").hex()).as_str())
        );
        let draft_dir = prev.draft_dir.clone();
        drop(prev);
        assert!(!draft_dir.exists(), "draft folder removed on exit");
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    fn wait_for(pane: &Pane, text: &str) -> bool {
        for _ in 0..150 {
            if pane
                .parser
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains(text)
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        false
    }

    /// Keys typed into a pane reach the program; pending input waits for
    /// the program's first output.
    #[test]
    fn typing_reaches_the_program() {
        let dirty = Arc::new(AtomicBool::new(false));
        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["--norc", "--noprofile", "-i"]);
        cmd.env("PS1", "READY> ");
        let Ok(mut pane) = Pane::spawn(cmd, 10, 40, dirty) else {
            eprintln!("skipped: no pty (build sandbox)");
            return;
        };
        pane.pending_input = Some(b"echo $((6*7))\n".to_vec());
        assert!(wait_for(&pane, "READY>"));
        pane.flush_pending();
        assert!(wait_for(&pane, "42"));
        for c in "echo typed".chars() {
            let bytes = key_bytes(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE), false);
            pane.write(&bytes);
        }
        pane.write(&key_bytes(
            KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            false,
        ));
        assert!(wait_for(&pane, "\ntyped"));
        // the demo line was typed after the prompt, so it appears once
        let screen = pane.parser.lock().unwrap().screen().contents();
        assert_eq!(screen.matches("echo $((6*7))").count(), 1, "{screen}");
    }
}

/// Removes draft folders of studios that are no longer running (a closed
/// window kills the studio before it can clean up).
fn remove_stale_drafts(base: &Path) {
    let Ok(entries) = std::fs::read_dir(base) else {
        return;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(pid) = name
            .to_str()
            .and_then(|n| n.strip_prefix("theme-studio-"))
            .and_then(|n| n.split('-').next())
        else {
            continue;
        };
        let running = pid
            .parse::<u32>()
            .is_ok_and(|p| Path::new(&format!("/proc/{p}")).exists());
        if !running && e.path().join("colors.sh").exists() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}
