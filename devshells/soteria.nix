# Dev environment for ~/bajoding/Soteria (Tauri app): the system libraries
# and build tools Tauri needs. The Rust toolchain comes from the global rustup.
#
# Loaded automatically by direnv through the project's .envrc:
#   use flake ~/dotfiles-nixos#soteria
{ pkgs }:

pkgs.mkShell {
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
}
