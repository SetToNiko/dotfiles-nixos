# Fonts downloaded with Theme Studio (its Fonts page, F2), made permanent.
# The studio records each download in a list (settings.json
# `fonts.installed-list`); home-module.nix passes that list here, and this
# gives the packages that install them:
#   google      pkgs.google-fonts.override { fonts = [ … ]; }
#   nerd        pkgs.nerd-fonts.<attr>
#   fontsource  the recorded files, with fetchurl and their hashes
# and the ids of the studio's own copies (~/.local/share/fonts/theme-studio/<id>),
# which the module removes on activation so there are no duplicate families.
{
  pkgs,
  lib,
  list,
}:

let
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
        lib.warn "theme-studio fonts.nix: no nerd-fonts package for ${asset}" [ ]
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
in
{
  packages =
    lib.optional (google != [ ]) (pkgs.google-fonts.override { fonts = google; }) ++ nerd ++ fontsource;
  ids = lib.mapAttrsToList (_: f: f.id) fonts;
}
