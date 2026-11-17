#!/usr/bin/env bash
# Switch theme: a palette and a font preset together (themes.json, made in
# the theme studio). Fonts are applied first without restarting; the
# palette's switch then reloads Ghostty and restarts i3 once for both.
# Usage: themes-switch.sh <name>, or no argument for a rofi picker.
here=$(dirname "$(readlink -f "$0")")
. "$here/lib/paths.sh" # data_file, share_file
themes="${THEMES:-$(data_file themes.json)}"
palettes="${PALETTES:-$(data_file palettes.json)}"
out="$HOME/.config/colors"
. "$here/lib/rofi-picker.sh" # json_picker, json_data

# Menu rows: bold name, the palette's main colours, the parts' names, and
# "(active)" after the current theme
menu_rows='def esc: gsub("&"; "&amp;") | gsub("<"; "&lt;") | gsub(">"; "&gt;");
  to_entries[]
  | "<b>\(.key | esc)</b>   "
    + ([($p[0][.value.palette] // {}) | .bg0, .fg0, .red, .orange, .yellow, .green, .aqua, .blue, .purple
        | select(. != null) | "<span foreground=\"\(.)\">██</span>"] | join(""))
    + "   <i>\(.value.palette | esc) · \(.value.fonts | esc)</i>"
    + (if .key == $cur then "   <i>(active)</i>" else "" end)'

if [ -n "$1" ]; then
  name=$1
else
  [ -f "$themes" ] || { echo "no themes.json yet (save a theme in the theme studio)" >&2; exit 1; }
  current=$(cat "$out/theme" 2>/dev/null)
  palette_now=$(cat "$out/current" 2>/dev/null)
  red=$(json_data "$palettes" | jq -r --arg n "$palette_now" '.[$n].red // "#ff5555"')
  pal=$(mktemp)
  trap 'rm -f "$pal"' EXIT
  json_data "$palettes" > "$pal"
  # the rows see the palettes as $p
  JQ_ARGS=(--slurpfile p "$pal")
  name=$(json_picker "$themes" Theme "$menu_rows" "$current" "$red" Theme) || exit 0
fi

[ -n "$name" ] || exit 0
palette=$(json_data "$themes" | jq -r --arg n "$name" '.[$n].palette // empty')
fonts=$(json_data "$themes" | jq -r --arg n "$name" '.[$n].fonts // empty')
[ -n "$palette" ] && [ -n "$fonts" ] || { echo "no theme '$name'" >&2; exit 1; }

NO_RESTART=1 bash "$here/font-switch.sh" "$fonts" || exit 1
bash "$here/theme-switch.sh" "$palette" || exit 1
mkdir -p "$out"
printf '%s\n' "$name" > "$out/theme"
