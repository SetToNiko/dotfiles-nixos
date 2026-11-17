# Theme Studio

Colour palettes and fonts for a Linux desktop, edited in a terminal with a
live preview of the desktop, and switched from rofi menus. A **theme** is a
palette plus a font preset; switching one rewrites the files each program
reads (Ghostty, i3 and its bar, rofi, GTK, fontconfig, nvim, the lock and
login screens) and reloads what can be reloaded.

This folder is laid out as a project of its own. For now it lives in the
author's NixOS dotfiles; it's being made independent of them, and later of
NixOS (`docs/migration.md`).

```
studio/           the studio (Rust, ratatui)
packaging/nix/    the Nix package (and, later, home-manager and NixOS modules)
docs/             theme-studio.md: the guide (also the studio's README tab)
                  plan.md: how it was built; migration.md: the move
```

Build and test:

```sh
nix build ./theme-studio                     # from the dotfiles folder
cd studio && nix develop .. --command cargo test
```

Your palettes, font presets and themes aren't part of the project: they're
read from `$THEME_STUDIO_DATA`, by default `~/.config/theme-studio/`.
