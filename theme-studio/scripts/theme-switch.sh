#!/usr/bin/env bash
# Switch colour scheme.
# Usage: theme-switch.sh <name>, or no argument for a rofi picker.
here=$(dirname "$(readlink -f "$0")")
. "$here/lib/paths.sh" # data_file, share_file
palettes="${PALETTES:-$(data_file palettes.json)}"
out="$HOME/.config/colors"
. "$here/lib/rofi-picker.sh" # json_picker, json_data (either palettes.json layout)

# Menu rows: bold name, one swatch per main colour,
# and "(active)" after the current scheme
menu_rows='to_entries[]
  | "<b>\(.key)</b>   "
    + ([.value | .bg0, .fg0, .red, .orange, .yellow, .green, .aqua, .blue, .purple | "<span foreground=\"\(.)\">██</span>"] | join(""))
    + (if .key == $cur then "   <i>(active)</i>" else "" end)'

if [ -n "$1" ]; then
  name=$1
else
  current=$(cat "$out/current" 2>/dev/null)
  # Delete messages use the active palette's red
  red=$(json_data "$palettes" | jq -r --arg n "$current" '.[$n].red // "#ff5555"')
  name=$(json_picker "$palettes" Theme "$menu_rows" "$current" "$red" Palette) || exit 0
fi

[ -n "$name" ] || exit 0
# Everything below reads the palettes map (whichever layout the file has)
data=$(mktemp)
trap 'rm -f "$data"' EXIT
json_data "$palettes" > "$data"
jq -e --arg n "$name" 'has($n)' "$data" >/dev/null || { echo "no scheme '$name'" >&2; exit 1; }

mkdir -p "$out"
jq -r --arg n "$name" '.[$n] | to_entries[] | "set $\(.key) \(.value)"' \
  "$data" > "$out/i3"
jq -r --arg n "$name" '.[$n] | to_entries[] | "\(.key)=\(.value)"' \
  "$data" > "$out/colors.sh"
echo "$name" > "$out/current"

# Ghostty: the palette sets background, foreground and all 16 ANSI colours,
# so programs using named terminal colours (starship, ls, git, yazi) match it.
# Which palette key goes where is in terminal-colors.json (shared with the
# theme studio's preview). A Ghostty theme of the same name, if there is one,
# still supplies the rest (cursor, selection). Palettes made by hand have
# none, and naming a missing theme is a Ghostty config error, so only name it
# when it exists, and take cursor and selection colours from the palette
# otherwise ("without_theme"). Without ghostty on PATH it can't be checked:
# keep the theme line.
theme_line=1
if ghostty_bin=$(command -v ghostty); then
  themes="$(dirname "$(readlink -f "$ghostty_bin")")/../share/ghostty/themes"
  [ -f "$themes/$name" ] || [ -f "$HOME/.config/ghostty/themes/$name" ] || theme_line=
fi
{ [ -n "$theme_line" ] && printf 'theme = %s\n' "$name"
  jq -r --arg n "$name" --arg theme "$theme_line" \
    --slurpfile map "$(share_file terminal-colors.json)" '
    .[$n] as $p | $map[0] as $m
    | (if $theme == "" then $m.without_theme | to_entries[] | "\(.key) = \($p[.value])" else empty end),
      "background = \($p[$m.background])", "foreground = \($p[$m.foreground])",
      ($m.palette | to_entries[] | "palette = \(.key)=\($p[.value])")' \
    "$data"; } > "$out/ghostty"

# rofi: raw colours only; config.rasi decides what uses them
{ echo '* {'
  jq -r --arg n "$name" '.[$n] | to_entries[] | "    \(.key | gsub("_"; "-")): \(.value);"' "$data"
  echo '}'; } > "$out/rofi.rasi"

# Firefox (Pywalfox): the mapping is pywalfox-colors.jq (configs/pywalfox/colors.jq before the move)
mkdir -p "$HOME/.cache/wal"
jq --arg n "$name" -f "$(share_file pywalfox-colors.jq pywalfox/colors.jq)" \
  "$data" > "$HOME/.cache/wal/colors.json"

if [ -z "$NO_RESTART" ]; then
  pkill -USR2 -f '^ghostty( |$)'   # Ghostty reloads its config on SIGUSR2
  i3-msg restart >/dev/null
  pywalfox update >/dev/null 2>&1
  # Wallpaper border colour follows bg0
  command -v wallpaper-rotate >/dev/null && wallpaper-rotate refresh >/dev/null 2>&1 &
fi
