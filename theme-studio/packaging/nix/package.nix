# Theme Studio's Nix package: the studio (studio/, a Rust crate).
# Used by the project's flake.nix and, until the project is its own repo,
# by the dotfiles directly (docs/migration.md).
{ lib, rustPlatform }:
let
  studio = ../../studio;
in
rustPlatform.buildRustPackage {
  pname = "theme-studio";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = studio;
    fileset = lib.fileset.unions [
      (studio + "/Cargo.toml")
      (studio + "/Cargo.lock")
      (studio + "/src")
    ];
  };
  cargoLock.lockFile = studio + "/Cargo.lock";
  # Tests that need the real configs folder or jq skip themselves in the sandbox.
  meta.mainProgram = "theme-studio";
}
