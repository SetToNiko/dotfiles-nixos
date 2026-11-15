# System settings that only apply to the laptop.
{ ... }:

{
  imports = [ ./hardware-configuration.nix ];

  # Laptop panel is wired to the Intel iGPU; render on NVIDIA, display via Intel.
  # Without this, X only drives NVIDIA (no connected outputs) and the screen
  # stays black at the login screen.
  hardware.nvidia.prime = {
    sync.enable = true;
    intelBusId = "PCI:0:2:0";
    nvidiaBusId = "PCI:1:0:0";
  };

  # Load the panel's driver in the initrd, so the boot animation has a screen
  # to draw on from the start
  boot.initrd.kernelModules = [ "i915" ];

  # HiDPI panel: scale Java (android-studio, etc.) UIs up
  environment.sessionVariables._JAVA_OPTIONS = "-Dsun.java2d.uiScale=2";
}
