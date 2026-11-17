# NixOS dotfiles

One branch for every machine: a shared base plus per-host modules, selected
through the flake.

## Layout

- `flake.nix` : one `nixosConfigurations.<host>` per machine (`laptop`, `desktop`)
- `modules/base.nix` : system settings shared by every host
- `home/base.nix` : home-manager settings shared by every host
- `hosts/<host>/default.nix` : system settings for one host
- `hosts/<host>/hardware-configuration.nix` : that host's hardware scan
- `hosts/<host>/home.nix` : home-manager settings for one host (e.g. the desktop's monitor layout)

## User configs

The `configs/` directory is a separate repo and is absent from this one.
After cloning, clone the configs repo into `configs/`.

Program configs are shared. Anything that differs per machine lives in
`configs/hosts/<host>/`, which home-manager links to `~/.config/host`
(e.g. `i3/config` includes `~/.config/host/i3.conf`).

## Themes and fonts

A theme is a colour palette plus a font preset, applied across the desktop
(Ghostty, nvim, i3 and its bar, rofi, GTK, the lock and login screens).

- `$mod+t` picks a theme, `$mod+p` only a palette, `$mod+Shift+t` only a
  font preset (rofi menus; Alt+BackSpace deletes).
- `$mod+Ctrl+t` opens Theme Studio (`theme-studio/`, a project of its own), which edits
  them with a live preview and downloads fonts.
- The data is in `configs/`: `themes.json`, `palettes.json` and
  `fonts.json`, applied by `themes-switch.sh`, `theme-switch.sh` and
  `font-switch.sh`. Font sizes get a per-host offset from
  `configs/hosts/<host>/fonts.json`.
- Fonts downloaded in the studio are listed in `home/fonts-installed.json`
  and installed by Nix on the next rebuild.

More in [theme-studio/docs/theme-studio.md](theme-studio/docs/theme-studio.md).

## Building

```sh
sudo nixos-rebuild switch --flake ~/dotfiles-nixos#laptop
sudo nixos-rebuild switch --flake ~/dotfiles-nixos#desktop
```

After the first switch, the `nrs` alias rebuilds the current host.

## Adding a host

1. Create `hosts/<name>/` with `default.nix`, `home.nix` and the machine's
   `hardware-configuration.nix` (from `nixos-generate-config`).
2. Add `<name> = mkHost "<name>";` to `flake.nix`.
3. Create `configs/hosts/<name>/i3.conf`, and optionally
   `configs/hosts/<name>/fonts.json` (font size offsets, e.g.
   `{"monospace": 0, "proportional": 3, "icons": 3}`).

## Dev shells

Project-specific dependencies live in `devshells/<name>.nix`; each file
becomes `devShells.<name>` in the flake. A project picks its shell with a
one-line `.envrc`, and direnv loads it on `cd`:

```sh
echo 'use flake ~/dotfiles-nixos#<name>' > .envrc
direnv allow
```

Toolchains, LSPs, formatters and linters stay global in `home/base.nix`;
dev shells hold the libraries and build tools a single project needs.
