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

  # Wallpaper rotation and the xsecurelock lock screen. These wrappers only pin
  # the dependencies; the scripts run live from configs/lock, so edits to them
  # apply without a rebuild.
  liveScript =
    name: deps:
    pkgs.writeShellApplication {
      inherit name;
      runtimeInputs = deps;
      text = ''exec ${pkgs.bash}/bin/bash "${dotfiles}/lock/${name}.sh" "$@"'';
    };
  wallpaperRotate = liveScript "wallpaper-rotate" (
    with pkgs;
    [
      feh
      ffmpeg
      util-linux
      coreutils
      findutils
      gnugrep
      gnused
    ]
  );
  lockSaver = liveScript "lock-saver" [
    pkgs.mpv
    pkgs.gnused
  ];
  lockScreen = liveScript "lock-screen" [
    pkgs.xsecurelock
    lockSaver
    pkgs.gnused
  ];
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

  # Loads a project's dev shell automatically on cd (see devshells/ and the
  # project's .envrc). nix-direnv caches the shell so it loads instantly.
  programs.direnv = {
    enable = true;
    nix-direnv.enable = true;
  };
  # programs.starship = {
  #   enable = true;
  #   enableBashIntegration = true;
  # };

  # New random wallpaper every 2 minutes (the first one is set by i3 at login).
  # Images come from ~/dotfiles-nixos/wallpaper.
  systemd.user.services.wallpaper-rotate = {
    Unit.Description = "Switch to a random wallpaper";
    Service = {
      Type = "oneshot";
      ExecStart = "${wallpaperRotate}/bin/wallpaper-rotate";
    };
  };
  systemd.user.timers.wallpaper-rotate = {
    Unit.Description = "Switch wallpaper every 2 minutes";
    Timer = {
      OnStartupSec = "2min";
      OnUnitActiveSec = "2min";
    };
    Install.WantedBy = [ "timers.target" ];
  };

  home.file.".gradle/gradle.properties".text = ''
    org.gradle.daemon.idletimeout=15000
  '';

  # Regenerate ~/.config/colors from palettes.json on every switch, so the
  # generated files never lag behind the palette format. Keeps the current
  # scheme; picks a default on first setup or if the current one is gone.
  home.activation.defaultTheme = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    theme="$(cat "$HOME/.config/colors/current" 2>/dev/null || true)"
    for name in "$theme" "Gruvbox Dark"; do
      if [ -n "$name" ] && run env PATH="${pkgs.jq}/bin:$PATH" NO_RESTART=1 \
        ${pkgs.bash}/bin/bash "${dotfiles}/theme-switch.sh" "$name"; then
        break
      fi
    done
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
    dex # runs XDG autostart entries from the i3 config (blueman-applet)
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
    wallpaperRotate
    lockScreen
    xss-lock

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
