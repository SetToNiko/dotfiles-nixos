#This sets the system settings and packages for root.

{
  config,
  lib,
  pkgs,
  ...
}:

{
  imports = [
    # Include the results of the hardware scan.
    # Points to root file to isolate system hardware from config.
    /etc/nixos/hardware-configuration.nix
  ];

  boot.loader.systemd-boot.enable = true; # Use the systemd-boot EFI boot loader.
  boot.loader.efi.canTouchEfiVariables = true;

  networking.hostName = "nixos-btw"; # Define your hostname.

  networking.networkmanager.enable = true; # configure network connections with nmtui.

  time.timeZone = "America/Montreal";

  nixpkgs.config.allowUnfree = true; # allows the use of proprietary packages. required for nvidia and android-studio
  nixpkgs.config.android_sdk.accept_license = true; # can't develop for android without it

  hardware.graphics = {
    # enable GPU usage
    enable = true;
  };

  services.xserver.videoDrivers = [ "nvidia" ]; # indicate usage of nvidia drivers
  hardware.nvidia = {
    modesetting.enable = true;
    open = true; # using open source drivers
    nvidiaSettings = true; # enable nvidia settings menu
    package = config.boot.kernelPackages.nvidiaPackages.stable; # which nvidia drivers are being used.
  };

  services.xserver = {
    # X11 windowing system
    enable = true;
    autoRepeatDelay = 200; # delay before autorepeat starts ms.
    autoRepeatInterval = 35; # autorepeat keypress/s.
    windowManager.i3.enable = true; # set window manager to i3.
  };

  # Enable CUPS to print documents.
  # services.printing.enable = true;

  services.pipewire = {
    # Enable sound.
    enable = true;
    pulse.enable = true;
  };

  users.users.niko = {
    # Define a user account. Don't forget to set a password with ‘passwd’.
    isNormalUser = true;
    extraGroups = [
      "wheel"
      "kvm"
      "adbusers"
    ]; # Enable ‘sudo’ for the user.
    packages = with pkgs; [
      tree
    ];
  };

  programs.firefox.enable = true;

  environment.systemPackages = with pkgs; [
    # packages installed at the system level
    vim # Do not forget to add an editor to edit configuration.nix! The Nano editor is also installed by default.
    wget
    git
    ghostty
  ];

  services.openssh.enable = true; # Enable the OpenSSH daemon.

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  system.stateVersion = "26.05"; # DO NOT CHANGE THIS
}
