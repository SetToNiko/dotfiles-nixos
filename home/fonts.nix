# Fonts downloaded with the theme studio (its Download fonts page, F2).
# The studio records each download in fonts-installed.json; this installs
# them for good:
#   google      pkgs.google-fonts.override { fonts = [ … ]; }
#   nerd        pkgs.nerd-fonts.<attr>
#   fontsource  the recorded files, with fetchurl and their hashes
# The studio's own copies (~/.local/share/fonts/theme-studio/<id>) are then
# removed on activation, so there are no duplicate families.
{ pkgs, lib, ... }:

let
  list = lib.importJSON ./fonts-installed.json;
  fonts = list.fonts or { };
  from = source: lib.filterAttrs (_: f: f.source == source) fonts;

  # google: family names (google-fonts matches FamilyName[-*].ttf)
  google = lib.attrNames (from "google");

  # nerd: the release asset (folder) name → the nerd-fonts attribute
  nerdManifest = lib.importJSON "${pkgs.path}/pkgs/data/fonts/nerd-fonts/manifests/fonts.json";
  nerdAttr =
    asset:
    let
      found = lib.findFirst (m: m.folderName == asset) null nerdManifest;
      name = lib.toLower found.caskName;
    in
    if found == null then
      null
    else if builtins.match "^[[:digit:]].*" name != null then
      "_" + name
    else
      name;
  nerd = lib.concatLists (
    lib.mapAttrsToList (
      asset: f:
      let
        attr = nerdAttr (f.asset or asset);
      in
      if attr != null && pkgs.nerd-fonts ? ${attr} then
        [ pkgs.nerd-fonts.${attr} ]
      else
        lib.warn "fonts.nix: no nerd-fonts package for ${asset}" [ ]
    ) (from "nerd")
  );

  # fontsource: the files the studio downloaded, by hash
  fontsource = lib.mapAttrsToList (
    family: f:
    pkgs.runCommand "font-${f.id}" { } ''
      mkdir -p $out/share/fonts/truetype
      ${lib.concatMapStrings (file: ''
        cp ${
          pkgs.fetchurl {
            inherit (file) url hash;
            name = file.name;
          }
        } $out/share/fonts/truetype/${file.name}
      '') f.files}
    ''
  ) (from "fontsource");

  ids = lib.mapAttrsToList (_: f: f.id) fonts;
in
{
  home.packages =
    lib.optional (google != [ ]) (pkgs.google-fonts.override { fonts = google; }) ++ nerd ++ fontsource;

  # Nix provides these now: drop the studio's downloaded copies
  home.activation.studioFontCopies = lib.mkIf (ids != [ ]) (
    lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      studio="$HOME/.local/share/fonts/theme-studio"
      removed=
      for id in ${lib.escapeShellArgs ids}; do
        if [ -d "$studio/$id" ]; then
          run rm -rf "$studio/$id"
          removed=1
        fi
      done
      if [ -n "$removed" ]; then
        run ${pkgs.fontconfig.bin}/bin/fc-cache -f
      fi
    ''
  );
}
