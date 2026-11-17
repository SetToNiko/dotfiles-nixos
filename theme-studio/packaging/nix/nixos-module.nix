# Theme Studio for NixOS (services.theme-studio.loginFont): the login
# screen (LightDM's mini greeter) uses the font preset's Monospace font.
# font-switch copies the font's file into a folder the system fontconfig
# reads and fills in the greeter's config from `template`, so a font change
# shows at the next login without a rebuild.
#
# The paths are the ones font-switch uses today (/var/lib/ascii-boot,
# /etc/ascii-boot/greeter.conf.in), named after the dotfiles' boot animation
# whose folder they share; making them settings is in docs/migration.md's
# list for the refactor. The folder itself is created by whoever owns it
# (the boot animation here).
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.theme-studio.loginFont;
  dir = "/var/lib/ascii-boot";

  template = pkgs.writeText "lightdm-mini-greeter.conf.in" cfg.template;
  # until font-switch has run once
  initial = pkgs.runCommand "lightdm-mini-greeter.conf" { } ''
    sed 's/@FONT@/monospace/; s/@SIZE@/1em/' ${template} > $out
  '';
in
{
  options.services.theme-studio.loginFont = {
    enable = lib.mkEnableOption "the font preset's Monospace font on the login screen (LightDM's mini greeter)";

    user = lib.mkOption {
      type = lib.types.str;
      description = "Who runs font-switch (owns the font folder).";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "users";
      description = "The font folder's group.";
    };

    template = lib.mkOption {
      type = lib.types.lines;
      description = "The whole lightdm-mini-greeter.conf, with @FONT@ (the family) and @SIZE@ (e.g. 14pt) where the font goes.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.tmpfiles.rules = [
      "d ${dir}/font 0755 ${cfg.user} ${cfg.group} -"
      "C ${dir}/greeter.conf - - - - ${initial}"
    ];

    # The greeter runs as lightdm and never sees the user's fonts: the
    # copied font is in a folder of the system fontconfig.
    fonts.fontconfig.localConf = ''
      <?xml version="1.0"?>
      <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
      <fontconfig>
        <dir>${dir}/font</dir>
      </fontconfig>
    '';

    # The greeter reads the config font-switch writes (the mini greeter
    # module's own file is replaced); the template is where font-switch
    # finds it.
    environment.etc."lightdm/lightdm-mini-greeter.conf".source = lib.mkForce "${dir}/greeter.conf";
    environment.etc."ascii-boot/greeter.conf.in".source = template;
  };
}
