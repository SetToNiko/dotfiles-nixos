# Shared rofi menu for theme-switch.sh and font-switch.sh: pick a preset from
# a JSON object (one preset per key), or delete one with Alt+BackSpace.
# Source this file, then:
#
#   name=$(json_picker FILE PROMPT ROWS_JQ CURRENT RED NOUN) || exit 0
#
#   FILE     JSON file holding the presets
#   PROMPT   rofi prompt
#   ROWS_JQ  jq program printing one Pango-markup row per key, in file order;
#            it gets the active preset's name as $cur
#   CURRENT  active preset: highlighted, and can't be deleted
#   RED      colour of the delete message
#   NOUN     what a preset is called in messages ("Palette", "Font preset")
#
# JQ_ARGS (an array, optional) adds arguments to the rows' jq call, e.g.
# (--slurpfile p palettes.json) so rows can show another file's data.
#
# Prints the chosen key; returns 1 if the menu was closed without choosing.
#
# The files are either the plain map (version 1) or, from version 2,
# {"version": 2, "palettes" / "presets": { ... }}; everything here reads and
# edits the map inside, whichever it is.

# jq: `data` is the map of presets in either layout
JQ_DATA='def data: if type == "object" and has("version") then (to_entries | map(select(.key != "version")) | .[0].value) else . end;'

# json_data FILE: prints the map of presets (either layout)
json_data() {
  jq "$JQ_DATA data" "$1"
}

# Escape text for rofi's Pango markup
markup_escape() {
  printf '%s' "$1" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g'
}

json_picker() {
  local file=$1 prompt=$2 rows=$3 current=$4 red=$5 noun=$6
  local row=-1 notice= active i mesg idx status label tmp
  local -a names

  while :; do
    mapfile -t names < <(jq -r "$JQ_DATA data | keys_unsorted[]" "$file")

    active=0
    for i in "${!names[@]}"; do
      [ "${names[$i]}" = "$current" ] && active=$i
    done
    # First time round, start on the active preset; after a delete, stay put
    [ "$row" -lt 0 ] && row=$active
    [ "$row" -ge "${#names[@]}" ] && row=$((${#names[@]} - 1))

    # Hint line, with the result of the last delete (if any) above it
    mesg="Enter: apply   Alt+BackSpace: delete from ${file##*/}"
    [ -n "$notice" ] && mesg="<span foreground=\"$red\"><b>$notice</b></span>"$'\n'"$mesg"
    notice=

    # Alt+BackSpace (kb-custom-1) deletes the highlighted preset; rofi exits with 10.
    idx=$(jq -r --arg cur "$current" "${JQ_ARGS[@]}" "$JQ_DATA data | $rows" "$file" |
      rofi -dmenu -i -markup-rows -format i \
        -kb-custom-1 Alt+BackSpace -mesg "$mesg" \
        -selected-row "$row" -a "$active" -p "$prompt")
    status=$?

    if [ "$status" -eq 10 ] && [ -n "$idx" ]; then
      row=$idx
      label=$(markup_escape "${names[$idx]}")
      # Keep the active preset: activation falls back to a default without it
      if [ "${names[$idx]}" = "$current" ]; then
        notice="$noun \"$label\" is active and can't be deleted"
      else
        tmp=$(mktemp) &&
          jq --arg n "${names[$idx]}" 'if type == "object" and has("version") then (to_entries | map(select(.key != "version")) | .[0].key) as $k | .[$k] |= del(.[$n]) else del(.[$n]) end' "$file" > "$tmp" &&
          cat "$tmp" > "$file" &&
          notice="$noun \"$label\" deleted"
        rm -f "$tmp"
      fi
      continue
    fi
    [ "$status" -eq 0 ] || return 1
    printf '%s\n' "${names[$idx]}"
    return 0
  done
}
