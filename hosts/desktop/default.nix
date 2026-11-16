# System settings that only apply to the desktop.
{ ... }:

{
  imports = [ ./hardware-configuration.nix ];

  # Load the GPU driver in the initrd, so the boot animation has a screen to
  # draw on from the start
  boot.initrd.kernelModules = [
    "nvidia"
    "nvidia_modeset"
    "nvidia_drm"
  ];

  # Native 1x scale. DP-2 (32" 2560x1440) is ~93 PPI, closest to the 96 DPI
  # baseline every toolkit treats as scale 1.0 (93 would make Qt 6 use a
  # blurry 0.97 fractional scale). HDMI-1 (27", ~109 PPI) renders ~15% smaller;
  # X11 can't set DPI per monitor. Xft.dpi is set to match in ./home.nix.
  services.xserver.dpi = 96;

  environment.sessionVariables = {
    _JAVA_OPTIONS = "-Dsun.java2d.uiScale=1";
    # Qt: no automatic per-screen scaling, no extra scale factor
    QT_AUTO_SCREEN_SCALE_FACTOR = "0"; # Qt 5
    QT_ENABLE_HIGHDPI_SCALING = "0"; # Qt 6
    QT_SCALE_FACTOR = "1";
    XCURSOR_SIZE = "24"; # standard 1x cursor
  };
}
