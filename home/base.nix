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
      mpv # renders the ASCII desktop still
      xvfb-run # ... in an invisible X server
      util-linux
      coreutils
      findutils
      gnugrep
      gnused
    ]
  );
  # The password prompt reduced to a bordered input box at the bottom of the
  # screen, whose border changes colour while Num Lock is on (colours and
  # margin are set in configs/lock/lock-screen.sh)
  xsecurelock = pkgs.xsecurelock.overrideAttrs (old: {
    patches = (old.patches or [ ]) ++ [ ./xsecurelock-password-box.patch ];
  });
  lockSaver = liveScript "lock-saver" [
    pkgs.mpv
    pkgs.gnused
  ];
  lockScreen = liveScript "lock-screen" [
    xsecurelock
    lockSaver
    pkgs.gnused
  ];
  # Next boot's animation and greeter image, from the current ASCII wallpaper
  asciiBoot = liveScript "ascii-boot" [
    (pkgs.python3.withPackages (ps: [
      ps.numpy
      ps.pillow
    ]))
    pkgs.xrandr
    pkgs.gnused
    pkgs.coreutils
  ];
  # Shutdown/reboot with the ASCII wallpaper dissolving first ($mod+Shift+p)
  asciiPower = liveScript "ascii-power" [
    xsecurelock
    lockSaver
    asciiBoot
    pkgs.rofi
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
  programs.starship = {
    enable = true;
    enableBashIntegration = true;
  };

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
      # ghostty: theme-switch.sh checks it for a theme matching the palette name
      if [ -n "$name" ] && run env PATH="${pkgs.jq}/bin:${pkgs.ghostty}/bin:$PATH" NO_RESTART=1 \
        ${pkgs.bash}/bin/bash "${dotfiles}/theme-switch.sh" "$name"; then
        break
      fi
    done
  '';
  # Same for the font preset (~/.config/fonts, from fonts.json).
  home.activation.defaultFonts = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
    preset="$(cat "$HOME/.config/fonts/current" 2>/dev/null || true)"
    for name in "$preset" "JetBrains Mono"; do
      if [ -n "$name" ] && run env PATH="${pkgs.jq}/bin:${pkgs.fontconfig.bin}/bin:$PATH" NO_RESTART=1 \
        ${pkgs.bash}/bin/bash "${dotfiles}/font-switch.sh" "$name"; then
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
    # dev <name>: enter a dev shell. Tries, in order:
    #   1. a full flake ref (anything containing '#' or ':'), used as-is
    #   2. devShells.<name> from this flake (devshells/<name>.nix)
    #   3. the <name> template from github:the-nix-way/dev-templates
    initExtra = ''
      dev() {
        local shells
        shells="$(ls ~/dotfiles-nixos/devshells | sed 's/\.nix$//')"
        case "$1" in
          -h | --help)
            echo "usage: dev [devshell|template|flake-ref]"
            echo "  no argument: pick a local devshell from a menu"
            echo "local devshells: $(echo $shells)"
            return 0
            ;;
        esac
        if [ $# -eq 0 ]; then
          local pick
          pick="$(printf '%s\n' "$shells" | fzf --prompt='devshell> ' --height=40% --reverse)" || return 1
          set -- "$pick"
        fi
        case "$1" in
          *'#'* | *:*) nix develop "$1" ;;
          *)
            if [ -f ~/dotfiles-nixos/devshells/"$1".nix ]; then
              nix develop ~/dotfiles-nixos#"$1"
            else
              nix develop "github:the-nix-way/dev-templates?dir=$1"
            fi
            ;;
        esac
      }
    '';
  };

  # this function maps the create_symlink function to all of our configurable programs in the list
  xdg.configFile = builtins.mapAttrs (name: subpath: {
    source = create_symlink "${dotfiles}/${subpath}";
    recursive = true;
  }) configs // {
    # Starship reads ~/.config/starship.toml; link it to the live file in
    # configs/ so prompt edits apply on the next prompt without a rebuild.
    "starship.toml".source = create_symlink "${dotfiles}/starship/starship.toml";
  };

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
    asciiPower
    asciiBoot
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
    pyright

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
