# System settings and packages shared by every host.
# Host-specific settings (hardware scan, scaling, ...) live in hosts/<host>/default.nix.

{
  config,
  lib,
  pkgs,
  ...
}:

{
  boot.loader.systemd-boot.enable = true; # Use the systemd-boot EFI boot loader.
  boot.loader.efi.canTouchEfiVariables = true;

  # Boot animation (Plymouth) and login screen (LightDM): ascii-boot.nix.
  # Instead of scrolling console text (Esc shows the messages), boot fades the
  # ASCII wallpaper in, and shutdown stays black after ascii-power's dissolve.
  # Plymouth needs the GPU driver in the initrd to draw early: see
  # boot.initrd.kernelModules in hosts/<host>/default.nix.
  imports = [ ./ascii-boot.nix ];
  boot.consoleLogLevel = 3; # kernel: errors only
  boot.kernelParams = [
    "quiet" # (boot.plymouth adds "splash")
    "udev.log_level=3"
    "rd.udev.log_level=3"
  ];

  networking.hostName = "nixos-btw"; # Define your hostname.
  networking.networkmanager.enable = true; # configure network connections with nmtui.

  time.timeZone = "America/Montreal";

  nixpkgs.config.allowUnfree = true; # allows the use of proprietary packages. required for nvidia and android-studio
  nixpkgs.config.android_sdk.accept_license = true; # can't develop for android without it

  nixpkgs.overlays = [
    # Vial's AppImage sandbox is missing libgpg-error, so its Qt xcb plugin
    # fails to load and the app exits silently. Add it to the FHS env.
    (final: prev: {
      vial = prev.vial.override {
        appimageTools = prev.appimageTools // {
          wrapType2 =
            args:
            prev.appimageTools.wrapType2 (
              args
              // {
                extraPkgs = p: (args.extraPkgs or (_: [ ])) p ++ [ p.libgpg-error ];
              }
            );
        };
      };
    })
  ];

  hardware = {
    bluetooth = {
      enable = true;
      powerOnBoot = true; # adapter is on after boot, ready to reconnect paired devices
      settings.General.Experimental = true; # report battery level of connected devices
    };
    graphics.enable = true; # enable gpu usage
    nvidia = {
      modesetting.enable = true;
      open = true; # using open source drivers
      nvidiaSettings = true; # enable nvidia settings menu
      package = config.boot.kernelPackages.nvidiaPackages.stable; # which nvidia drivers are being used.
      # prime = {
      #   # enables switching between cpu and gpu on laptop
      #   sync.enable = true;
      #   intelBusId = "PCI:0:2:0";
      #   nvidiaBusId = "PCI:1:0:0";
      # };

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
      # dpi is set per host (hosts/<host>/default.nix)
    };
    pipewire = {
      # enable sound
      enable = true;
      pulse.enable = true;
    };
    blueman.enable = true; # bluetooth manager GUI + tray applet (started by dex in i3)
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

  security.rtkit.enable = true; # lets pipewire get realtime scheduling (avoids audio stutter)

  environment.sessionVariables = {
    GDK_SCALE = "1";
    GDK_DPI_SCALE = "1";
    # _JAVA_OPTIONS (uiScale) is set per host
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
    nix-ld = {
      enable = true; # enables linker for dynamically linked libraries
    };
    steam = {
      enable = true;
      remotePlay.openFirewall = true;
      localNetworkGameTransfers.openFirewall = true;
    };
  };

  environment.systemPackages = with pkgs; [
    # packages installed at the system level
    vim # Do not forget to add an editor to edit configuration.nix! The Nano editor is also installed by default.
    wget
    git
    lazygit
    bat
    xmousepasteblock # stops middle-click from pasting (started by i3)
    doxx # terminal .docx viewer
    ghostty
    unzip
    libreoffice
    autorandr
    glib
    vial # Vial keymap editor
  ];

  services.udev.packages = [ pkgs.vial ]; # udev rule granting hidraw access to Vial keyboards

  services.openssh.enable = true; # Enable the OpenSSH daemon.
  programs.ssh.startAgent = true;

  nix.settings.experimental-features = [
    "nix-command"
    "flakes"
  ];

  system.stateVersion = "26.05"; # DO NOT CHANGE THIS
}
