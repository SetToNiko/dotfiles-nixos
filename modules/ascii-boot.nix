# Boot animation and login screen, both drawn from the desktop's ASCII wallpaper.
# Plymouth fades the characters in one by one while the machine boots (the
# reverse of ascii-power's shutdown dissolve). LightDM's mini greeter then shows
# the finished image with a small password box on top.
# The images are made from the current desktop by `ascii-boot`
# (configs/lock/ascii-boot.sh), which ascii-power runs during the dissolve, and
# kept in /var/lib/ascii-boot. A plain `reboot` reuses the last set.
# Neither host has an encrypted disk; this theme draws no passphrase prompt.
# The greeter's font is the font preset's Monospace font, set by Theme
# Studio's NixOS module (theme-studio/packaging/nix/nixos-module.nix) from
# the greeter config below. (Plymouth plays images and draws no text.)

{ pkgs, ... }:

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

  # Greeter background until ascii-boot has run once
  black = pkgs.runCommand "black.png" {
    nativeBuildInputs = [ (pkgs.python3.withPackages (ps: [ ps.pillow ])) ];
  } ''python3 -c "from PIL import Image; Image.new('RGB', (1, 1)).save('$out', 'PNG')"'';
in
{
  imports = [ ../theme-studio/packaging/nix/nixos-module.nix ];

  boot.plymouth = {
    enable = true;
    theme = "ascii";
    themePackages = [ theme ];
  };

  # Written by niko's session (ascii-boot; font-switch for the login font),
  # read by plymouthd and lightdm
  systemd.tmpfiles.rules = [
    "d ${dir} 0755 niko users -"
    "C ${dir}/greeter.png - - - - ${black}"
  ];

  services.displayManager.defaultSession = "none+i3";
  services.xserver.displayManager.lightdm = {
    background = "${dir}/greeter.png";
    greeters.mini = {
      enable = true;
      user = "niko";
    };
  };

  # Minimal greeter: the finished image (centred and unscaled, like the
  # desktop) and a password box, black like the gaps between the characters.
  # Super+s / Super+r shut down / reboot from it. It has no session picker and
  # always starts the default session. @FONT@ and @SIZE@ are filled in by
  # font-switch (the Monospace font, in points).
  services.theme-studio.loginFont = {
    enable = true;
    user = "niko";
    template = ''
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
  };
}
