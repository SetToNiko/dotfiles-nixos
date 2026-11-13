# System settings that only apply to the laptop.
{ ... }:

{
  imports = [ ./hardware-configuration.nix ];

  # HiDPI panel: scale Java (android-studio, etc.) UIs up
  environment.sessionVariables._JAVA_OPTIONS = "-Dsun.java2d.uiScale=2";
}
