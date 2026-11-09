# This file sets the config for a specific user @ username
{ config, pkgs, ... }:

let
  dotfiles = "${config.home.homeDirectory}/dotfiles-nixos/configs";
  create_symlink = path: config.lib.file.mkOutOfStoreSymlink path;
  configs = {
    nvim = "nvim";
    i3 = "i3";
    i3status-rust = "i3status-rust";
    ghostty = "ghostty";
    ideavimrc = "ideavimrc";

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

  home.file.".background-image".source = ./wallpaper/leaves.png;
  home.file.".gradle/gradle.properties".text = ''
    org.gradle.daemon.idletimeout=15000
  '';

  home.stateVersion = "25.05";
  programs.bash = {
    enable = true;
    shellAliases = {
      btw = "echo fresh nixos install btw";
      nrs = "sudo nixos-rebuild switch --impure --flake ~/dotfiles-nixos#nixos-btw";
    };
  };

  # this function maps the create_symlink function to all of our configurable programs in the list
  xdg.configFile = builtins.mapAttrs (name: subpath: {
    source = create_symlink "${dotfiles}/${subpath}";
    recursive = true;
  }) configs;

  programs.autorandr = {
    enable = true;
    profiles = {
      docked = {
        fingerprint = {
          DP-2 = "00ffffffffffff001c5413322905602b0b220104b5462778ff0000a75750a0270b5054ffff808100b30095008180714f81c08140d1fc6a5e00a0a0a0295030203500ba892100001a000000fd0030a5a0a046010a202020202020000000fc0047533332510a20202020202020000000ff00323431313242363030353239000242020332f1494003290f043f901f132309070783010000e305e3016d1a0000021130a5000000000000e6060701575725e200ea6fc200a0a0a0555020403500ba892100001aa08380a070382d4030203500ba892100001ed29680a070382d4030203500ba892100001e0000000000000000000000000000000000000000000000b77012790000030114a2030104ff099f002f001f009f0528000280040007000a00000008040004021000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000003990";
          HDMI-1 = "00ffffffffffff000472fd0a7fb440310e210103803c22782a2a15a7564f9e250e5054bfef80d1c0b30095008180814081c001010101f07300a0a0a00f503020350055502100001a000000ff0031333134304234374634323032000000fd00304b1e781e000a202020202020000000fc004b41323732550a2020202020200131020341714c0203121304901f0114056061230907078301000067030c001000383c67d85dc401788003681a00000101304be6e305e301e30f000ce6060701606045565e00a0a0a029503020350055502100001a011d007251d01e206e28550055502100001e2a4480a0703827400820980455502100001e000000000000000014";
        };
        config = {

          DP-2 = {
            enable = true;
            primary = true;
            mode = "2560x1440";
            rate = "165.00";
          };
          HDMI-1 = {
            enable = true;
            mode = "2560x1440";
            rate = "59.95";
            rotate = "left";
            position = "2560x0";
          };
        };

      };
    };
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
    i3status-rust
    discord
    claude-code

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
    browser-sync

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
