#!/usr/bin/env bash
# theme-switch: apply a palette from palettes.json to every program
set -euo pipefail

PALETTES="${XDG_CONFIG_HOME:-$HOME/.config}/colors/palettes.json"
OUT="$HOME/.config/colors"
RELOAD=(ghostty i3 dunst nvim)

die() {
  printf 'theme-switch: %s\n' "$*" >&2
  exit 1
}

usage() {
  echo "usage: theme-switch [-l] [-n] NAME"
  echo "  -l  list palettes   -n  don't reload programs"
}

list_palettes() {
  jq -r '.palettes | keys[]' "$PALETTES"
}

write_files() {
  local name="$1" json
  json=$(jq -e --arg n "$name" '.palettes[$n]' "$PALETTES") \
    || die "no palette named \"$name\""
  mkdir -p "$OUT"
  echo "$json" > "$OUT/palette.json"
  jq -r 'to_entries[] | "\(.key)=\(.value)"' <<< "$json" > "$OUT/palette.env"
  printf '%s\n' "$name" > "$OUT/current"
}

reload() {
  for prog in "${RELOAD[@]}"; do
    case "$prog" in
      ghostty) pkill -USR2 -x ghostty || true ;;
      i3) i3-msg -q restart ;;
      dunst) systemctl --user restart dunst ;;
      nvim) for s in "$XDG_RUNTIME_DIR"/nvim.*.0; do
              nvim --server "$s" --remote-send '<Cmd>colorscheme palette<CR>' || true
            done ;;
    esac
  done
}

reload_after=1
while getopts "lnh" opt; do
  case "$opt" in
    l) list_palettes; exit 0 ;;
    n) reload_after=0 ;;
    *) usage; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
[[ -n "${1:-}" ]] || { usage; exit 2; }

write_files "$1"
(( reload_after )) && reload
echo "applied $1 ($(date +%H:%M))"
