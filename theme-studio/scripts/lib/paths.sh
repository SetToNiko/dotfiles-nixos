# Where the theme scripts find things (theme-studio's paths.rs does the same).
# Source this after setting $here to the script's own folder (resolved
# through links), then:
#
#   data_file NAME           your palettes.json, fonts.json or themes.json:
#                            $THEME_STUDIO_DATA/NAME, else
#                            ~/.config/theme-studio/NAME if it's there, else
#                            next to the script (the old layout)
#   share_file NAME [OLD]    a file the scripts need (terminal-colors.json,
#                            …): $THEME_STUDIO_SHARE/NAME, else ../share/NAME
#                            beside the script's folder if that exists, else
#                            next to the script as OLD (default NAME)

data_file() {
  if [ -n "$THEME_STUDIO_DATA" ]; then
    printf '%s/%s\n' "$THEME_STUDIO_DATA" "$1"
    return
  fi
  local xdg="${XDG_CONFIG_HOME:-$HOME/.config}/theme-studio/$1"
  if [ -e "$xdg" ]; then
    printf '%s\n' "$xdg"
  else
    printf '%s/%s\n' "$here" "$1"
  fi
}

share_file() {
  local old="${2:-$1}"
  if [ -n "$THEME_STUDIO_SHARE" ] && [ -e "$THEME_STUDIO_SHARE/$1" ]; then
    printf '%s/%s\n' "$THEME_STUDIO_SHARE" "$1"
  elif [ -n "$THEME_STUDIO_SHARE" ] && [ -e "$THEME_STUDIO_SHARE/$old" ]; then
    printf '%s/%s\n' "$THEME_STUDIO_SHARE" "$old"
  elif [ -e "$here/../share/$1" ]; then
    printf '%s/%s\n' "$here/../share" "$1"
  else
    printf '%s/%s\n' "$here" "$old"
  fi
}
