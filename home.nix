# This file sets the config for a specific user @ username
{ config, pkgs, ... }:

let
  dotfiles = "${config.home.homeDirectory}/dotfiles-nixos/configs";
  create_symlink = path: config.lib.file.mkOutOfStoreSymlink path;
  configs = {
    nvim = "nvim";
    i3 = "i3";
  };
in
{
  home = {
    username = "niko";
    homeDirectory = "/home/niko";
  };
  programs.git = {
    enable = true;
    settings = {
      user = {
        name = "niko";
        email = "niko@email.com";
      };
      init.defaultBranch = "main";
    };

  };

  home.stateVersion = "25.05";
  programs.bash = {
    enable = true;
    shellAliases = {
      btw = "echo fresh nixos install btw";
    };
  };

  # this function maps the create_symlink function to all of our configurable programs in the list
  xdg.configFile = builtins.mapAttrs (name: subpath: {
    source = create_symlink "${dotfiles}/${subpath}";
    recursive = true;
  }) configs;

  home.packages = with pkgs; [
    # list of packages to be installed for this user. Will not apply to root.
    git

    # Core
    neovim
    ripgrep # telescope find_files / grep_string
    fd # telescope find_files (faster)
    tree-sitter
    gcc
    nil
    nixpkgs-fmt
    nodejs
    nerd-fonts.jetbrains-mono # mini.icons / neo-tree icons (can be replaced with any nerd-font)

    # LSP servers
    vscode-langservers-extracted # html, cssls, jsonls
    svelte-language-server
    typescript-language-server
    typescript
    clang-tools
    lua-language-server
    sqls
    gopls
    marksman
    nixd
    yaml-language-server
    tombi

    # Rust (rustaceanvim)
    rust-analyzer
    cargo
    rustc
    clippy
    rustfmt

    # Go toolchain + formatters/linters
    go
    gotools
    gofumpt
    golangci-lint

    # Formatters
    prettier
    stylua
    sqlfluff
    nixfmt

    # Linters
    deadnix
    eslint_d
    stylelint
    statix
    htmlhint
    markdownlint-cli2
    yamllint

  ];
}
