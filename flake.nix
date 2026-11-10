{
  description = "NixOS from scratch";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/master";
    home-manager = {
      url = "github:nix-community/home-manager/master";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      home-manager,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      nixosConfigurations.nixos-btw = nixpkgs.lib.nixosSystem {
        modules = [
          ./configuration.nix
          home-manager.nixosModules.home-manager
          {
            home-manager = {
              useGlobalPkgs = true;
              useUserPackages = true;
              users.niko = import ./home.nix;
              backupFileExtension = "backup";
            };
          }
        ];
      };

      devShells.${system}.soteria = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          pkg-config
          gobject-introspection
          wrapGAppsHook3
        ];

        buildInputs = with pkgs; [
          glib
          gtk3
          webkitgtk_4_1
          libsoup_3
          cairo
          pango
          gdk-pixbuf
          atk
          harfbuzz
          librsvg
          openssl
          libayatana-appindicator
          xdotool
        ];

        shellHook = ''
          export WEBKIT_DISABLE_DMABUF_RENDERER=1
          export XDG_DATA_DIRS="$GSETTINGS_SCHEMAS_PATH:$XDG_DATA_DIRS"
          export LD_LIBRARY_PATH="${
            pkgs.lib.makeLibraryPath [
              pkgs.libayatana-appindicator
              pkgs.webkitgtk_4_1
              pkgs.gtk3
            ]
          }:$LD_LIBRARY_PATH"
        '';
      };
    };
}
