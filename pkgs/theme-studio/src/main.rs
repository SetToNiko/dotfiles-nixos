//! Theme Studio: edit palettes.json palettes with a live preview.

mod app;
mod behaviour;
mod color;
mod config;
mod desktop;
mod font_tree;
mod fonts;
mod fonts_ui;
mod history;
mod hit;
mod keymap;
mod out;
mod palette;
mod preview;
mod settings;
mod settings_ui;
mod store;
mod ui;

use std::io::stdout;

use anyhow::Result;
use ratatui::crossterm::event::{
    DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use ratatui::crossterm::execute;

use app::{App, Start};
use store::Store;

const USAGE: &str = "\
Usage: theme-studio [--fonts] [PALETTE | --new]
       theme-studio --init-config

Edit a palette from palettes.json (default: the active one), or start a
new one; --fonts opens on the Fonts tab. Press ? inside for keys.
--init-config writes any missing settings files, updates palettes.json and
fonts.json to the current file format, and exits.

Settings: $XDG_CONFIG_HOME/theme-studio (default ~/.config/theme-studio):
  keymaps.json   keymap presets and the active one
  settings.json  startup defaults

Environment:
  THEME_STUDIO_CONFIGS  dotfiles configs folder (default ~/dotfiles-nixos/configs)";

fn main() -> Result<()> {
    // every write the studio makes keeps the old file in the history
    history::init(history::default_dir());
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
            // palettes.json / fonts.json to the version-2 layout too
            let store = Store::locate();
            let fonts = fonts::FontStore {
                dir: store.dir.clone(),
            };
            for (what, r) in [
                ("palettes.json", store.migrate()),
                ("fonts.json", fonts.migrate()),
            ] {
                match r {
                    Ok(true) => println!("{what}: updated to version 2"),
                    Ok(false) => {}
                    Err(e) => eprintln!("warning: {what}: {e:#}"),
                }
            }
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

    // raw mode, the alternate screen and the panic hook; drawing goes
    // through out::Out (for the hover underline styles)
    drop(ratatui::init());
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::CrosstermBackend::new(out::Out::default()))?;
    execute!(stdout(), EnableMouseCapture, EnableBracketedPaste)?;
    // The kitty keyboard protocol (Ghostty, kitty, foot, WezTerm…), so keys
    // like Ctrl+Shift+Z, Ctrl+1 or Ctrl+I (not Tab) arrive as themselves.
    let enhanced = ratatui::crossterm::terminal::supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        let _ = execute!(
            stdout(),
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    // Which image protocol the terminal speaks (kitty graphics in Ghostty);
    // asked now, before key events are read. Half blocks if it won't say.
    app.picker = Some(
        ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks()),
    );
    let result = app.run(&mut terminal);
    app::set_pointer("default");
    if enhanced {
        let _ = execute!(stdout(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(stdout(), DisableMouseCapture, DisableBracketedPaste);
    ratatui::restore();
    result
}
