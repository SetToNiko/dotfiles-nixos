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

  environment.sessionVariables._JAVA_OPTIONS = "-Dsun.java2d.uiScale=1";
}
