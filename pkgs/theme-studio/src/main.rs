//! Theme Studio: edit palettes.json palettes with a live preview.

mod app;
mod color;
mod config;
mod desktop;
mod font_tree;
mod fonts;
mod fonts_ui;
mod hit;
mod keymap;
mod palette;
mod preview;
mod store;
mod ui;

use std::io::stdout;

use anyhow::Result;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
};
use ratatui::crossterm::execute;

use app::{App, Start};
use store::Store;

const USAGE: &str = "\
Usage: theme-studio [--fonts] [PALETTE | --new]
       theme-studio --init-config

Edit a palette from palettes.json (default: the active one), or start a
new one; --fonts opens on the Fonts tab. Press ? inside for keys.
--init-config writes any missing settings files and exits.

Settings: $XDG_CONFIG_HOME/theme-studio (default ~/.config/theme-studio):
  keymaps.json   keymap presets and the active one
  settings.json  startup defaults

Environment:
  THEME_STUDIO_CONFIGS  dotfiles configs folder (default ~/dotfiles-nixos/configs)";

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let fonts_tab = args.iter().any(|a| a == "--fonts");
    args.retain(|a| a != "--fonts");
    let start = match args.first().map(String::as_str) {
        Some("-h" | "--help") => {
            println!("{USAGE}");
            return Ok(());
        }
        Some("--new") => Start::New,
        Some("--init-config") => {
            let dir = config::dir();
            let loaded = config::load(&dir, true);
            println!("{}", dir.display());
            for w in loaded.warnings {
                eprintln!("warning: {w}");
            }
            return Ok(());
        }
        Some(name) => Start::Named(name.to_string()),
        None => Start::Active,
    };
    // Load before taking over the terminal, so errors print normally.
    let mut app = App::new(Store::locate(), start)?;
    if fonts_tab {
        app.open_fonts_tab();
    }

    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    // Which image protocol the terminal speaks (kitty graphics in Ghostty);
    // asked now, before key events are read. Half blocks if it won't say.
    app.picker = Some(
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks()),
    );
    let result = app.run(&mut terminal);
    app::set_pointer("default");
    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    result
}
