# Boot animation and login screen, both drawn from the desktop's ASCII wallpaper.
# Plymouth fades the characters in one by one while the machine boots (the
# reverse of ascii-power's shutdown dissolve). LightDM's mini greeter then shows
# the finished image with a small password box on top.
# The images are made from the current desktop by `ascii-boot`
# (configs/lock/ascii-boot.sh), which ascii-power runs during the dissolve, and
# kept in /var/lib/ascii-boot. A plain `reboot` reuses the last set.
# Neither host has an encrypted disk; this theme draws no passphrase prompt.
# The greeter's font is the font preset's monospace font: font-switch.sh
# copies its file to /var/lib/ascii-boot/font and fills in greeter.conf from
# the template below, so a font change shows at the next login without a
# rebuild. (Plymouth plays images and draws no text.)

{ lib, pkgs, ... }:

let
  dir = "/var/lib/ascii-boot";

  theme = pkgs.runCommand "plymouth-theme-ascii" { } ''
    d=$out/share/plymouth/themes/ascii
    mkdir -p $d
    cp ${./ascii-boot.script} $d/ascii.script
    cat > $d/ascii.plymouth <<EOF
    [Plymouth Theme]
    Name=ascii
    Description=The ASCII wallpaper fading in
    ModuleName=script

    [script]
    ImageDir=${dir}
    ScriptFile=$d/ascii.script
    EOF
  '';

  # Minimal greeter: the finished image (centred and unscaled, like the
  # desktop) and a password box, black like the gaps between the characters.
  # Super+s / Super+r shut down / reboot from it. It has no session picker and
  # always starts the default session. @FONT@ and @SIZE@ are filled in by
  # font-switch.sh (the monospace font, in points).
  greeterTemplate = pkgs.writeText "lightdm-mini-greeter.conf.in" ''
    [greeter]
    user = niko
    show-password-label = false
    invalid-password-text = wrong password
    show-input-cursor = true
    password-alignment = center
    password-input-width = 24
    show-image-on-all-monitors = true

    [greeter-hotkeys]
    mod-key = meta
    shutdown-key = s
    restart-key = r
    hibernate-key = h
    suspend-key = u

    [greeter-theme]
    font = "@FONT@"
    font-size = @SIZE@
    font-weight = normal
    font-style = normal
    text-color = "#cccccc"
    error-color = "#ff6b7f"
    background-image = "${dir}/greeter.png"
    background-color = "#000000"
    window-color = "#000000"
    border-color = "#000000"
    border-width = 0px
    layout-space = 20
    password-color = "#cccccc"
    password-background-color = "#000000"
    password-border-color = "#555555"
    password-border-width = 1px
    password-border-radius = 0
  '';

  # The greeter config until font-switch.sh has run once
  greeterDefault = pkgs.runCommand "lightdm-mini-greeter.conf" { } ''
    sed 's/@FONT@/monospace/; s/@SIZE@/1em/' ${greeterTemplate} > $out
  '';

  # Greeter background until ascii-boot has run once
  black = pkgs.runCommand "black.png" {
    nativeBuildInputs = [ (pkgs.python3.withPackages (ps: [ ps.pillow ])) ];
  } ''python3 -c "from PIL import Image; Image.new('RGB', (1, 1)).save('$out', 'PNG')"'';
in
{
  boot.plymouth = {
    enable = true;
    theme = "ascii";
    themePackages = [ theme ];
  };

  # Written by niko's session (ascii-boot, font-switch.sh), read by plymouthd
  # and lightdm
  systemd.tmpfiles.rules = [
    "d ${dir} 0755 niko users -"
    "d ${dir}/font 0755 niko users -"
    "C ${dir}/greeter.png - - - - ${black}"
    "C ${dir}/greeter.conf - - - - ${greeterDefault}"
  ];

  # The greeter runs as lightdm and never sees niko's fonts: the copied font
  # is in a folder of the system fontconfig.
  fonts.fontconfig.localConf = ''
    <?xml version="1.0"?>
    <!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
    <fontconfig>
      <dir>${dir}/font</dir>
    </fontconfig>
  '';

  services.displayManager.defaultSession = "none+i3";
  services.xserver.displayManager.lightdm = {
    background = "${dir}/greeter.png";
    greeters.mini = {
      enable = true;
      user = "niko";
    };
  };
  # The greeter reads the config font-switch.sh writes (the module's own
  # file is replaced); the template is where font-switch.sh finds it.
  environment.etc."lightdm/lightdm-mini-greeter.conf".source =
    lib.mkForce "${dir}/greeter.conf";
  environment.etc."ascii-boot/greeter.conf.in".source = greeterTemplate;
}
