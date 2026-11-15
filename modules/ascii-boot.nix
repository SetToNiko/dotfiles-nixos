# Boot animation and login screen, both drawn from the desktop's ASCII wallpaper.
# Plymouth fades the characters in one by one while the machine boots (the
# reverse of ascii-power's shutdown dissolve). LightDM's mini greeter then shows
# the finished image with a small password box on top.
# The images are made from the current desktop by `ascii-boot`
# (configs/lock/ascii-boot.sh), which ascii-power runs during the dissolve, and
# kept in /var/lib/ascii-boot. A plain `reboot` reuses the last set.
# Neither host has an encrypted disk; this theme draws no passphrase prompt.

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
  boot.plymouth = {
    enable = true;
    theme = "ascii";
    themePackages = [ theme ];
  };

  # Written by niko's session (ascii-boot), read by plymouthd and lightdm
  systemd.tmpfiles.rules = [
    "d ${dir} 0755 niko users -"
    "C ${dir}/greeter.png - - - - ${black}"
  ];

  # Minimal greeter: the finished image (centred and unscaled, like the
  # desktop) and a password box, black like the gaps between the characters.
  # Super+s / Super+r shut down / reboot from it. It has no session picker and
  # always starts the default session.
  services.displayManager.defaultSession = "none+i3";
  services.xserver.displayManager.lightdm = {
    background = "${dir}/greeter.png";
    greeters.mini = {
      enable = true;
      user = "niko";
      extraConfig = ''
        [greeter]
        show-password-label = false
        invalid-password-text = wrong password
        password-alignment = center
        password-input-width = 24
        show-image-on-all-monitors = true

        [greeter-theme]
        font = "monospace"
        font-size = 1em
        font-weight = normal
        text-color = "#cccccc"
        error-color = "#ff6b7f"
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
  };
}
