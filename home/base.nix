# home-manager config for niko, shared by every host.
# Host-specific settings live in hosts/<host>/home.nix.
{
  config,
  pkgs,
  lib,
  host, # "laptop" or "desktop", passed in from flake.nix
  ...
}:

let
  dotfiles = "${config.home.homeDirectory}/dotfiles-nixos/configs";
  create_symlink = path: config.lib.file.mkOutOfStoreSymlink path;
  configs = {
    nvim = "nvim";
    i3 = "i3";
    i3blocks = "i3blocks";
    ghostty = "ghostty";
    ideavimrc = "ideavimrc";
    claude-code = "claude";
    starship = "starship";
    yazi = "yazi";
    rofi = "rofi";
    pywalfox = "pywalfox";
    # per-host fragments (e.g. ~/.config/host/i3.conf, included by i3/config)
    host = "hosts/${host}";
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
  programs.yazi.enable = true;
  # programs.starship = {
  #   enable = true;
  #   enableBashIntegration = true;
  # };

  home.file.".background-image".source = ../wallpaper/leaves.png;
  home.file.".gradle/gradle.properties".text = ''
    org.gradle.daemon.idletimeout=15000
  '';

  # First-time setup: pick a colour scheme if none has been chosen yet
  home.activation.defaultTheme = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    if [ ! -e "$HOME/.config/colors/current" ]; then
      run env PATH="${pkgs.jq}/bin:$PATH" NO_RESTART=1 \
        ${pkgs.bash}/bin/bash "${dotfiles}/theme-switch.sh" "Gruvbox Dark"
    fi
  '';

  home.stateVersion = "25.05";
  programs.bash = {
    enable = true;
    shellAliases = {
      btw = "echo fresh nixos install btw";
      nrs = "sudo nixos-rebuild switch --flake ~/dotfiles-nixos#${host}";
    };
  };

  # this function maps the create_symlink function to all of our configurable programs in the list
  xdg.configFile = builtins.mapAttrs (name: subpath: {
    source = create_symlink "${dotfiles}/${subpath}";
    recursive = true;
  }) configs;

  xdg.mimeApps = {
    enable = true;
    associations.added."application/pdf" = [ "firefox.desktop" ];
  };

  home.packages = with pkgs; [
    # list of packages to be installed for this user. Will not apply to root.
    git
    rofi
    (symlinkJoin {
      name = "gnome-control-center";
      paths = [ gnome-control-center ];
      nativeBuildInputs = [ makeWrapper ];
      postBuild = ''
        wrapProgram $out/bin/gnome-control-center \
            --set XDG_CURRENT_DESKTOP GNOME
      '';
    })

    # Core
    neovim
    android-studio
    android-tools
    ripgrep # telescope find_files / grep_string
    fd # telescope find_files (faster)
    tree-sitter
    gcc
    nil
    nixpkgs-fmt
    i3blocks
    autotiling
    discord
    claude-code
    starship
    pywalfox-native
    yazi # Yazi file explorer and dependencies
    ffmpeg
    jq
    fd
    poppler
    fzf
    zoxide
    resvg
    imagemagick
    brightnessctl

    # Fonts
    nerd-fonts.jetbrains-mono # mini.icons / neo-tree icons (can be replaced with any nerd-font)
    nerd-fonts.geist-mono

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

    # Web Dev
    nodejs
    deno
    moon
    browser-sync

    # Rust (rustaceanvim)
    rustup
    pkg-config
    librsvg
    webkitgtk_4_1
    openssl

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
