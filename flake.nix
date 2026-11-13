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

      # One dev shell per file in devshells/: devshells/<name>.nix becomes
      # devShells.<name>, used from a project with `use flake ~/dotfiles-nixos#<name>`.
      devShells.${system} = nixpkgs.lib.mapAttrs' (
        file: _:
        nixpkgs.lib.nameValuePair (nixpkgs.lib.removeSuffix ".nix" file) (
          import ./devshells/${file} { inherit pkgs; }
        )
      ) (builtins.readDir ./devshells);
    };
}
