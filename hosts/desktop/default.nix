# System settings that only apply to the desktop.
{ ... }:

{
  imports = [ ./hardware-configuration.nix ];

  environment.sessionVariables._JAVA_OPTIONS = "-Dsun.java2d.uiScale=1";
}
