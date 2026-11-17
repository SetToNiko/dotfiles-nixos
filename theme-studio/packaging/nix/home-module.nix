# Theme Studio for home-manager (programs.theme-studio): the studio and the
# switch commands, your data folder linked to ~/.config/theme-studio, the
# generated colour and font files refreshed on every switch, and the fonts
# the studio downloaded installed for good.
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.theme-studio;

  # Live: the same programs, running the checkout's scripts and reading its
  # share files and guide, so edits apply without a rebuild. (The package's
  # wrappers take THEME_STUDIO_SCRIPTS / THEME_STUDIO_SHARE when they're set.)
  programs =
    if cfg.live == null then
      cfg.package
    else
      pkgs.runCommand "theme-studio-live"
        {
          nativeBuildInputs = [ pkgs.makeWrapper ];
          meta.mainProgram = "theme-studio";
        }
        ''
          mkdir -p $out/bin
          for f in ${cfg.package}/bin/*; do
            makeWrapper "$f" "$out/bin/$(basename "$f")" \
              --set THEME_STUDIO_SCRIPTS ${lib.escapeShellArg "${cfg.live}/scripts"} \
              --set THEME_STUDIO_SHARE ${lib.escapeShellArg "${cfg.live}/share"}
          done
        '';

  fonts =
    if cfg.fontsList == null then
      {
        packages = [ ];
        ids = [ ];
      }
    else
      import ./fonts.nix {
        inherit pkgs lib;
        list = lib.importJSON cfg.fontsList;
      };

  # Regenerate the files a switch command writes, keeping what's current
  # (read from `current`), else the fallback. After linkGeneration: the
  # commands find the data through the ~/.config/theme-studio link.
  refresh = command: current: fallback: ''
    current="$(cat "$HOME/.config/${current}" 2>/dev/null || true)"
    for name in "$current" ${lib.escapeShellArgs (lib.optional (fallback != null) fallback)}; do
      if [ -n "$name" ] && run env NO_RESTART=1 ${programs}/bin/${command} "$name"; then
        break
      fi
    done
  '';
in
{
  options.programs.theme-studio = {
    enable = lib.mkEnableOption "Theme Studio, its switch commands and the files they generate";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      description = "The Theme Studio package (the studio and the theme-switch, themes-switch and font-switch commands).";
    };

    live = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/home/me/src/theme-studio";
      description = "A checkout of the project: the commands run its scripts/ and read its share/, so edits apply without a rebuild.";
    };

    dataDir = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "/home/me/dotfiles/theme-studio";
      description = "A folder (outside the store) holding palettes.json, fonts.json, themes.json, keymaps.json and settings.json, linked to ~/.config/theme-studio. Null: ~/.config/theme-studio is an ordinary folder.";
    };

    fontsList = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "The list of fonts downloaded with the studio (its `fonts.installed-list` setting); they're installed, and the studio's own copies removed.";
    };

    refresh = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Regenerate ~/.config/colors and ~/.config/fonts on every switch, so they never lag behind the data's format.";
    };

    fallbackPalette = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "The palette applied on a switch when none is current yet (or the current one is gone).";
    };

    fallbackFonts = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "The font preset applied on a switch when none is current yet (or the current one is gone).";
    };
  };

  config = lib.mkIf cfg.enable {
    home.packages = [ programs ] ++ fonts.packages;

    xdg.configFile."theme-studio" = lib.mkIf (cfg.dataDir != null) {
      source = config.lib.file.mkOutOfStoreSymlink cfg.dataDir;
    };

    home.activation = {
      themeStudioColors = lib.mkIf cfg.refresh (
        lib.hm.dag.entryAfter [ "linkGeneration" ] (refresh "theme-switch" "colors/current" cfg.fallbackPalette)
      );
      themeStudioFonts = lib.mkIf cfg.refresh (
        lib.hm.dag.entryAfter [ "linkGeneration" ] (refresh "font-switch" "fonts/current" cfg.fallbackFonts)
      );
      # Nix provides these now: drop the studio's downloaded copies
      themeStudioFontCopies = lib.mkIf (fonts.ids != [ ]) (
        lib.hm.dag.entryAfter [ "writeBoundary" ] ''
          studio="$HOME/.local/share/fonts/theme-studio"
          removed=
          for id in ${lib.escapeShellArgs fonts.ids}; do
            if [ -d "$studio/$id" ]; then
              run rm -rf "$studio/$id"
              removed=1
            fi
          done
          if [ -n "$removed" ]; then
            run ${pkgs.fontconfig.bin}/bin/fc-cache -f
          fi
        ''
      );
    };
  };
}
