{
  # Theme Studio on its own: `nix build ./theme-studio`, `nix develop
  # ./theme-studio`. The dotfiles don't use this flake yet; they call
  # packaging/nix/ directly until the project is split into its own repo
  # (docs/migration.md).
  description = "Theme Studio: colour palettes and fonts with a live preview of the desktop";

  inputs.nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      theme-studio = pkgs.callPackage ./packaging/nix/package.nix { };
    in
    {
      packages.${system} = {
        inherit theme-studio;
        default = theme-studio;
      };
      # cargo, rustc and the crate's dependencies, for working in studio/
      devShells.${system}.default = pkgs.mkShell { inputsFrom = [ theme-studio.bin ]; };
    };
}
