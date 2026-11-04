# Desktop dotfiles for nixOS

## Global

This repo contains my global nixOS nix configs.

configuration.nix : global system files
home.nix : individual user files, home-manager
flake.nix : used for building system using specific user configs

## User configs

The /config directory for user configs is absent from this repo.

After cloning, create the directory, the clone individual repos into it.

## Initial setup

to build the system, run:

```nix
sudo nixos-rebuild switch --impure --flake
```

afterwards, for a given `<user>` run

```nix
sudo nixos-rebuild switch --impure --flake ~/dotfiles-nixos#<user>
```

to rebuld the system with user specific settings from home-manager
