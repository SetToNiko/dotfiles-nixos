# Where the theme scripts find things (the studio's paths.rs does the same).
# Source this after setting $here to the script's own folder (resolved
# through links), then:
#
#   data_file NAME    your palettes.json, fonts.json or themes.json:
#                     $THEME_STUDIO_DATA/NAME, else
#                     ${XDG_CONFIG_HOME:-~/.config}/theme-studio/NAME
#   share_file NAME   a file the scripts need (terminal-colors.json, …):
#                     $THEME_STUDIO_SHARE/NAME (set by the package), else
#                     ../share/NAME beside the scripts' folder

data_file() {
  printf '%s/%s\n' "${THEME_STUDIO_DATA:-${XDG_CONFIG_HOME:-$HOME/.config}/theme-studio}" "$1"
}

share_file() {
  printf '%s/%s\n' "${THEME_STUDIO_SHARE:-$here/../share}" "$1"
}
