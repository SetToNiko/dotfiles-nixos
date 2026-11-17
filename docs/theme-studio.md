# Theme Studio

A terminal tool for making themes: colour palettes and fonts, edited with a
live preview of the desktop (i3 borders and bar, your real nvim, a shell or
yazi, a rofi menu), all drawn in the palette before it's saved. The code is
in `pkgs/theme-studio/` (Rust, ratatui); how it came about is in
`theme-studio-plan.md`.

## Themes, palettes and font presets

A **theme** is a name plus a palette plus a font preset. Palettes and font
presets are building blocks: several themes can share one, and editing it
changes every theme that uses it.

- **Palettes** (`palettes.json`) have 34 keys. 8 are base colours; the rest
  are derived from them (`palette-derive.jq`) until edited by hand, which
  pins them. Pins aren't stored: a key whose value differs from what the
  rules give counts as pinned.
- **Font presets** (`fonts.json`) have three fonts, each named after what it
  has to be:

  | Role         | Needs             | Used for                                                  |
  | ------------ | ----------------- | --------------------------------------------------------- |
  | Monospace    | a Nerd Font Mono  | terminal (so nvim, yazi), lock and login screen, "monospace" |
  | Proportional | proportional      | rofi, GTK apps, "sans-serif" (web pages, Electron)        |
  | Icons        | a Nerd Font Propo | i3 window titles and the bar                              |

  Sizes get a per-machine offset from `configs/hosts/<host>/fonts.json`
  (`{"monospace": 0, "proportional": 3, "icons": 3}`).

## Switching

| Key            | What                                                    |
| -------------- | ------------------------------------------------------- |
| `$mod+t`       | theme picker (rofi): palette and fonts together         |
| `$mod+p`       | palette picker, for a quick colour change               |
| `$mod+Shift+t` | font preset picker                                      |
| `$mod+Ctrl+t`  | the studio, in a floating window                        |

In the pickers, Alt+BackSpace deletes the highlighted entry. The scripts
behind them also take a name: `themes-switch.sh <name>`,
`theme-switch.sh <name>`, `font-switch.sh <name>`. They write the files
the apps read (`~/.config/colors/`, `~/.config/fonts/`, GTK and fontconfig
settings) and reload Ghostty and i3; rofi, GTK apps and the lock and login
screens pick the change up next time they start.

## The studio

```sh
theme-studio                 # the active theme
theme-studio <name>          # a theme, or a palette
theme-studio --new           # a new theme
theme-studio --fonts         # start with the keyboard on the font roles
theme-studio --init-config   # write missing settings, update file formats, exit
```

Three pages, on `F1`–`F3`:

- **Main:** ¹ font roles, ² installed fonts, ³ font sample, ⁴ colour list,
  ⁵ colour picker (opens when you edit a colour), and the preview: ⁶ nvim
  and ⁷ the shell (`Y` for yazi, `R` for the rofi menu, `V` for a swatch
  view). `F12` or a click hands the keyboard to a preview window; `F12`
  again moves on and back.
- **Fonts:** download fonts from Google Fonts and Nerd Fonts (previewed
  before installing), the installed ones, and a **tournament**: pairs of
  fonts from a category, side by side, until one is left.
- **Settings:** Keymaps (rebind anything, presets), Behaviour (start-up
  defaults and looks), History (every config file's earlier versions,
  previewed against now before restoring) and README (this guide; `}` / `{`
  jump between headings).

`?` lists every key. Some of the Vim preset's:

| Key                        | What                                                  |
| -------------------------- | ----------------------------------------------------- |
| `s` / `S` / `a`            | save the theme / save as (forks edited parts) / apply |
| `o` / `n`                  | open / new theme                                      |
| `Space o p` / `Space o f`  | another palette / font preset for this theme          |
| `Space 1`–`Space 7`        | go to a pane                                          |
| `Enter` on a colour        | edit it in the picker (Enter keeps, Esc puts it back) |
| `u` / `Ctrl+r`             | undo / redo colour edits                              |
| `p`, `y`, `c`, `#`         | pin, copy hex, copy from another key, type a hex      |
| `Enter` on a font role     | pick from the fonts that fit it (applied at once)     |
| `r` on a font role         | the same family's face that fits, for one that doesn't |
| `F` on a font              | every role from its family                            |
| `Space r`                  | resize panes (or drag the borders)                    |
| `Space u`                  | undo the last config file change                      |
| `q`                        | quit (asks about unsaved changes)                     |

## Files

| Where                                   | What                                                   |
| --------------------------------------- | ------------------------------------------------------ |
| `configs/themes.json`                   | themes                                                 |
| `configs/palettes.json`                 | palettes                                               |
| `configs/fonts.json`                    | font presets                                           |
| `configs/terminal-colors.json`          | which palette key is each terminal colour              |
| `configs/theme-studio/`                 | the studio's `keymaps.json` and `settings.json`, linked to `~/.config/theme-studio` |
| `home/fonts-installed.json`             | fonts the studio downloaded, installed by `home/fonts.nix` on the next rebuild |
| `~/.local/share/fonts/theme-studio/`    | downloaded fonts until that rebuild                    |
| `~/.local/share/theme-studio/previews/` | preview files of fonts that aren't installed           |
| `~/.local/state/theme-studio/`          | config history and tournaments                         |
| `/var/lib/ascii-boot/`                  | the login screen's font and config (from `font-switch.sh`) |

The JSON files are written exactly as `jq .` would, so a save only changes
the lines that changed. Each has a `version`; the studio updates older
files when it starts, keeping the old one in the history.

## Building and testing

The studio is built by the flake (`nix run ~/dotfiles-nixos#theme-studio`)
and installed by `home/base.nix`. To work on it:

```sh
cd ~/dotfiles-nixos/pkgs/theme-studio
nix develop ~/dotfiles-nixos#theme-studio --command cargo test
```

Tests that need the real `configs/` folder or a tool that isn't there
skip themselves. Environment variables:

- `THEME_STUDIO_CONFIGS`: the configs folder (default
  `~/dotfiles-nixos/configs`).
- `THEME_STUDIO_TIMING=1`: logs each font preview's render time to
  `~/.local/state/theme-studio/timing.log`.
