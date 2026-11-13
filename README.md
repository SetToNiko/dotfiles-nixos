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
3. Create `configs/hosts/<name>/i3.conf`.
