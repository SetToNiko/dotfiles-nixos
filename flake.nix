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

      # One NixOS system per machine: shared base + hosts/<host> overrides.
      # `host` is passed to every module so shared config can refer to it.
      mkHost =
        host:
        nixpkgs.lib.nixosSystem {
          specialArgs = { inherit host; };
          modules = [
            ./modules/base.nix
            ./hosts/${host}
            home-manager.nixosModules.home-manager
            {
              home-manager = {
                useGlobalPkgs = true;
                useUserPackages = true;
                extraSpecialArgs = { inherit host; };
                users.niko.imports = [
                  ./home/base.nix
                  ./hosts/${host}/home.nix
                ];
                backupFileExtension = "backup";
              };
            }
          ];
        };
    in
    {
      nixosConfigurations = {
        laptop = mkHost "laptop";
        desktop = mkHost "desktop";
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
