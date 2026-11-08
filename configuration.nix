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
    ./hardware-configuration.nix
  ];

  boot.loader.systemd-boot.enable = true; # Use the systemd-boot EFI boot loader.
  boot.loader.efi.canTouchEfiVariables = true;

  networking.hostName = "nixos-btw"; # Define your hostname.
  networking.networkmanager.enable = true; # configure network connections with nmtui.

  time.timeZone = "America/Montreal";

  nixpkgs.config.allowUnfree = true; # allows the use of proprietary packages. required for nvidia and android-studio
  nixpkgs.config.android_sdk.accept_license = true; # can't develop for android without it

  hardware = {
    graphics.enable = true; # enable gpu usage
    nvidia = {
      modesetting.enable = true;
      open = true; # using open source drivers
      nvidiaSettings = true; # enable nvidia settings menu
      package = config.boot.kernelPackages.nvidiaPackages.stable; # which nvidia drivers are being used.
      prime = {
        # enables switching between cpu and gpu on laptop
        sync.enable = true;
        intelBusId = "PCI:0:2:0";
        nvidiaBusId = "PCI:1:0:0";
      };

    };
  };

  services = {
    xserver = {
      videoDrivers = [ "nvidia" ]; # indicate usage of nvidia drivers
      # X11 windowing system
      enable = true;
      autoRepeatDelay = 200; # delay before autorepeat starts ms.
      autoRepeatInterval = 35; # autorepeat keypress/s.
      windowManager.i3.enable = true; # set window manager to i3.
      dpi = 144;
    };
    pipewire = {
      # enable sound
      enable = true;
      pulse.enable = true;
    };
    picom = {
      enable = true;
      backend = "xrender"; # other option : glx

      settings = {
        use-damage = false; # prevents partial screen updates from glitching
        unredir-if-possible = false; # should fix full screen flicker
        # srender-sync-fence = true; # helps stabilize sync on nvidia/intel
      };
    };
  };

  environment.sessionVariables = {
    GDK_SCALE = "1";
    GDK_DPI_SCALE = "1";
    _JAVA_OPTIONS = "-Dsun.java2d.uiScale=2";
  };

  # Enable CUPS to print documents.
  # services.printing.enable = true;

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
  programs = {
    firefox = {
      enable = true;
    };
    dconf = {
      enable = true; # allows settings from gnome-control-center to be saved.
    };
  };

  environment.systemPackages = with pkgs; [
    # packages installed at the system level
    vim # Do not forget to add an editor to edit configuration.nix! The Nano editor is also installed by default.
    wget
    git
    ghostty
    unzip
    libreoffice
  ];

  services.openssh.enable = true; # Enable the OpenSSH daemon.
  programs.ssh.startAgent = true;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  system.stateVersion = "26.05"; # DO NOT CHANGE THIS
}
