# Theme Studio: TUI palette editor with a live preview (docs/theme-studio-plan.md).
{ lib, rustPlatform }:
rustPlatform.buildRustPackage {
  pname = "theme-studio";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;
  # Tests that need the real configs/ folder or jq skip themselves in the sandbox.
  meta.mainProgram = "theme-studio";
}
