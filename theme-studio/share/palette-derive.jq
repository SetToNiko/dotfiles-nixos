# Fills in every palette slot for every theme in palettes.json.
#
# The slots are the ones gruvbox-material defines (bg_dim, bg0..bg5, greys,
# orange, visual/diff backgrounds, ...). A theme only has to provide 8 base
# colours: bg0, fg0, red, green, yellow, blue, purple, aqua. Every slot it
# does not set itself is derived from those by blending. Slots a theme does
# set are kept, so hand-tuned themes (e.g. "Gruvbox Dark Hard") stay as they are
# and running this again changes nothing.
#
# Adding a theme: add it with the 8 base colours to palettes.json, then
#   jq -f palette-derive.jq palettes.json > /tmp/p.json && mv /tmp/p.json palettes.json
# (reads either layout of palettes.json and writes version 2).

def hexval: explode | map(if . >= 97 then . - 87 else . - 48 end) | .[0] * 16 + .[1];
def rgb: ascii_downcase | [.[1:3], .[3:5], .[5:7]] | map(hexval);
def hex2: (. + 0.5 | floor) as $n | "0123456789abcdef" as $d
  | $d[($n / 16 | floor):($n / 16 | floor) + 1] + $d[$n % 16:$n % 16 + 1];
# blend colour $a towards $b by $t (0..1)
def mix($a; $b; $t):
  [($a | rgb), ($b | rgb)] | transpose | map(.[0] + (.[1] - .[0]) * $t | hex2) | "#" + join("");
def lum: rgb | .[0] * 0.299 + .[1] * 0.587 + .[2] * 0.114;

def derive:
  . as $p
  | $p.bg0 as $bg | $p.fg0 as $fg
  | (($bg | lum) > ($fg | lum)) as $light
  | {
      # backgrounds, from dimmest to most raised (blends of bg towards fg)
      bg_dim: (if $light then mix($bg; $fg; 0.05) else mix($bg; "#000000"; 0.3) end),
      bg0: $bg,
      bg1: mix($bg; $fg; 0.05),
      bg2: mix($bg; $fg; 0.05),
      bg3: mix($bg; $fg; 0.15),
      bg4: mix($bg; $fg; 0.15),
      bg5: mix($bg; $fg; 0.25),
      bg_statusline1: mix($bg; $fg; 0.05),
      bg_statusline2: mix($bg; $fg; 0.09),
      bg_statusline3: mix($bg; $fg; 0.25),
      # tinted backgrounds (blends of bg towards an accent)
      bg_visual_red: mix($bg; $p.red; 0.18),
      bg_visual_yellow: mix($bg; $p.yellow; 0.18),
      bg_visual_green: mix($bg; $p.green; 0.18),
      bg_visual_blue: mix($bg; $p.blue; 0.18),
      bg_visual_purple: mix($bg; $p.purple; 0.18),
      bg_diff_red: mix($bg; $p.red; 0.12),
      bg_diff_green: mix($bg; $p.green; 0.12),
      bg_diff_blue: mix($bg; $p.blue; 0.12),
      bg_current_word: mix($bg; $fg; 0.1),
      # text and accents
      fg0: $fg,
      fg1: $fg,
      red: $p.red,
      orange: mix($p.red; $p.yellow; 0.5),
      yellow: $p.yellow,
      green: $p.green,
      aqua: $p.aqua,
      blue: $p.blue,
      purple: $p.purple,
      bg_red: $p.red,
      bg_green: $p.green,
      bg_yellow: $p.yellow,
      # greys, from closest to bg to closest to fg
      grey0: mix($bg; $fg; 0.43),
      grey1: mix($bg; $fg; 0.55),
      grey2: mix($bg; $fg; 0.67)
    } + $p; # anything the theme sets itself wins

# version 2: {"version": 2, "palettes": { name: palette }}
(if has("version") then .palettes else . end) | map_values(derive) | {version: 2, palettes: .}
