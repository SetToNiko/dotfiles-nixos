# Theme Studio: plan

A terminal tool for creating and editing colour palettes and choosing fonts.
The screen is split vertically: **editor on the left, live preview on the
right**. The preview shows a small model of the desktop: i3 borders and bar,
your real nvim config, a terminal running commands, yazi, and a rofi overlay,
all drawn in the palette you are editing before it is saved.

The plan also adds **system-wide fonts**, set up the same way as the
palettes: `fonts.json`, a switch script, a rofi picker and generated files in
`~/.config`.

Decisions already made are marked ✅. Defaults I picked for smaller details
are marked 🔸; say if you want any of them changed.

---

## Decisions so far

| | Choice |
|---|---|
| ✅ Stack | Rust + ratatui (crossterm backend) |
| ✅ Font roles | Two roles: **mono** (terminal, nvim, lock screen) and **ui** (i3 titles and bar, rofi, GTK) |
| ✅ Font install | Into `~/.local/share/fonts/theme-studio/` straight away, and recorded in a list that Nix installs on the next rebuild |
| ✅ Font preview | Sample text rendered to an image (ImageMagick + Pango) and shown inline using the kitty graphics protocol |
| ✅ Palette keys | All 34 keys can be edited. Derived keys follow their base colours until you edit one by hand, which **pins** it |
| ✅ Picker | HSV square, hue strip, H/S/L sliders, hex input; mouse and keyboard |
| ✅ Font sources | Google Fonts (search via Fontsource) and Nerd Fonts releases |
| ✅ Preview extras | Rofi menu overlay and a yazi file-browser view |
| ✅ Window | Opens as a floating i3 window (its own Ghostty window class) |

---

## Phase 0: groundwork in the existing setup ✅ done

Small changes the studio relies on. Each one is useful on its own.

1. **Ghostty: all 16 terminal colours.** `theme-switch.sh` currently writes
   colours 1–6 and 9–14. Add **0 / 8 / 7 / 15** as well (`bg1` /
   `grey0` / `fg0` / `fg1`). Also write the `theme = <name>` line only when
   Ghostty actually has a theme with that name. A palette made in the studio
   won't have one, and Ghostty reports an error when the theme is missing.
   After this, a palette fully defines the terminal by itself.
2. **nvim: let the colours file path be overridden.** `nvim/lua/theme.lua`
   always reads `~/.config/colors/colors.sh`. Change it to
   `os.getenv("THEME_COLORS") or <default>`. The studio then starts nvim
   pointed at a temporary draft file, and nvim's existing file watcher
   re-applies the colours on every edit. Normal nvim behaves exactly as now.
3. **Shared picker code.** The rofi loop in `theme-switch.sh` (open the menu,
   delete with Alt+BackSpace, show a message in the palette's red) moves to
   `configs/lib/rofi-picker.sh`, so the font picker can reuse it.
4. **Follow-ups found while building Phase 1** (done):
   - home-manager's activation step runs with a small fixed PATH that
     didn't include `ghostty`, so the theme check couldn't run during a
     rebuild and always wrote `theme = <name>`. For a studio-made palette
     that would be a Ghostty config error after every `nrs`. Ghostty is now
     on that PATH.
   - Palettes without a Ghostty theme also get cursor and selection
     colours from the palette (cursor `fg0` on `bg0`, selection `bg3` with
     `fg0`). Before, a Ghostty theme was the only source for these.

## Phase 1: system-wide fonts ✅ done

Same structure as palettes:

```
configs/fonts.json            presets, one per name (like palettes.json)
configs/font-switch.sh        <name> | no argument for a rofi picker
~/.config/fonts/current       name of the active preset
~/.config/fonts/*             generated per-application files
```

**`fonts.json`**

```json
{
  "JetBrains + Inter": {
    "mono": { "family": "JetBrainsMono Nerd Font", "size": 11 },
    "ui":   { "family": "Inter", "size": 10 }
  }
}
```

**Generated files and where each is used:**

| App | Generated file | Change needed in the existing config | When it updates |
|---|---|---|---|
| Ghostty | `fonts/ghostty` (`font-family`, `font-size`) | `config-file = ?~/.config/fonts/ghostty` | Immediately (SIGUSR2) |
| i3 titles + bar (i3blocks) | `fonts/i3` (`font pango:<ui> <size>`) | `include ~/.config/fonts/i3`; remove the `font` line from `hosts/*/i3.conf` | Immediately (`i3-msg restart`) |
| rofi | `fonts/rofi.rasi` (`* { font: "<ui> <size>"; }`) | `@import` in `rofi/config.rasi` | Next time rofi opens |
| Lock screen | `fonts/lock.sh` (`XSECURELOCK_FONT=<mono>:size=N`) | `lock-screen.sh` sources it instead of hard-coding GeistMono | Next lock |
| GTK 3/4 (blueman, Firefox UI, …) | `gtk-3.0/settings.ini`, `gtk-4.0/settings.ini` (`gtk-font-name`) | none; home-manager doesn't manage GTK yet | When each app is restarted |
| Everything that asks for "monospace" or "sans-serif" (Firefox page text, Electron apps, …) | `fontconfig/conf.d/60-theme-fonts.conf` (aliases to the mono and ui fonts) | none | When each app is restarted |
| nvim, yazi, starship | follow Ghostty | | |

Not covered: Discord's own UI font (hard-coded in its CSS), and the boot
splash in `modules/ascii-boot.nix` (only changes on rebuild; it could read
`fonts.json` at build time if you want).

- 🔸 **Per-host sizes.** At the moment the bar font is 9 on the desktop and 12
  on the laptop. Sizes in `fonts.json` would be the base, and an optional
  `configs/hosts/<host>/fonts.json` (`{ "mono": +0, "ui": +3 }`) adjusts them
  per machine.
- 🔸 **Shortcut:** `$mod+Shift+t` opens the font picker (`$mod+t` stays the
  palette picker). Each row shows the preset name followed by its families
  and sizes. Alt+BackSpace deletes a preset, the same as in the palette menu.
- A home-manager activation step regenerates the font files on every switch,
  as `defaultTheme` already does for colours.
- **Nerd Font icons:** the bar and yazi need Nerd Font icons. If the chosen
  font doesn't have them, fontconfig falls back to the installed JetBrainsMono
  Nerd Font, so icons keep working. The studio warns when the selected mono
  font isn't a Nerd Font.

## Phase 2: studio skeleton and palette engine ✅ done

- Code lives in `pkgs/theme-studio/` (outer repo), built with
  `rustPlatform.buildRustPackage`, exposed as a flake package and added to
  `home.packages`. Runtime tools (nvim, yazi, starship, git, ImageMagick,
  fontconfig) are put on its PATH with `wrapProgram`.
- **Reading and writing `palettes.json`** keeps the key order and the same
  formatting jq produces, so saving one palette leaves the rest of the file's
  diff unchanged. Confirmed: `palettes.json` and `fonts.json` are both
  byte-for-byte what `jq .` outputs, so matching that is the whole
  requirement. A test checks it against the real file.
- **Derived colours:** the rules in `palette-derive.jq` are rewritten in
  Rust. A test runs both on every palette and checks the results match.
- **One terminal-colour mapping.** Which palette key becomes each of the
  16 terminal colours (black `bg1`, bright black `grey0`, …), plus cursor and
  selection, moves out of `theme-switch.sh` into `configs/terminal-colors.json`.
  `theme-switch.sh` (with jq) and the studio's preview both read it, so the
  preview can't drift from what Ghostty actually shows.
- **Pins aren't stored in the file.** When a palette is loaded, a key counts
  as pinned if its value differs from what the rules would compute. Hand-tuned
  palettes like Gruvbox Dark Hard therefore load with the right keys pinned,
  and `palettes.json` doesn't change format.
- **Opening the studio:** start from the active palette, from any palette
  (searchable list), or from a blank 8-colour starting point.
- **Saving:** `s` saves (overwrite, or save under a new name), `a` applies
  (runs `theme-switch.sh <name>`), and `q` asks before discarding unsaved
  changes. Undo/redo with `u` / `Ctrl+r`.

## Phase 3: colour editor (left pane) ✅ done

```
┌ Palette: novmbr* ─────────────┐
│ ▸ Base                         │
│   bg0     ██ #241d1a           │
│   fg0     ██ #c7b8ac           │
│   red     ██ #9f6434           │
│ ▾ Derived                      │
│   bg1     ██ #2f2825  ⟵ bg0    │
│   bg3   🔒 ██ #4a3f39           │
│   ...                          │
├ Colour ─ red ─────────────────┤
│  [HSV square]      [hue strip] │
│  H ━━━━━━●──────  27°          │
│  S ━━━━━━━━━●───  51%          │
│  L ━━━━●────────  41%          │
│  hex #9f6434   old ██ new ██   │
└───────────────────────────────┘
```

- The upper half is the key list, grouped Base / Backgrounds / Accents /
  Greys / Tinted. Derived keys show what they come from (`⟵ bg0`), and pinned
  keys show 🔒 (`p` toggles a pin).
- The lower half is the picker. The SV square and hue strip are drawn with
  half-block characters in true colour, so each cell shows two colour rows.
  Use the mouse (click or drag) or `hjkl` and `HJKL`, with Tab moving between
  controls.
- 🔸 **Contrast hints:** next to each foreground key, the contrast ratio
  against `bg0` (for example `4.8:1`), shown in red below 3:1. Useful for
  greys and comments.
- **Extras:** `y` copies the hex (OSC 52, so no clipboard tool needed),
  pasting a hex with Ctrl+Shift+V sets the selected key (bracketed paste
  instead of a `P` key, for the same reason), `c` copies another key's colour
  into this one, `#` jumps to the hex box (typing replaces the colour, Esc
  restores it).
- **Built as:** the picker grows (bigger colour square) when the window has
  room beyond the whole key list; otherwise the list scrolls. Tests drive the
  editor with real key events and render it with ratatui's TestBackend.

## Phase 4: live preview (right pane)

A small desktop drawn in terminal cells. i3's 2-pixel borders become
1-cell lines, so the borders look heavier than on the real desktop.

```
┌─────────────────────┐┌──────────────────┐
│ nvim (real config)  ││ terminal          │   focused window: client.focused
│                     ││ $ ls  git log …   │   other window: client.unfocused
│                     ││                   │
└─────────────────────┘└──────────────────┘
 1  2  3                     vol 40%  wifi  bt  bat 80%  17:42
```

- **Mapping colours to roles.** `i3/theme` and `rofi/config.rasi` are read
  to find which palette key each role uses (for example `client.focused` uses
  `$yellow`). If you change those files later, the preview follows.
- **nvim:** your real nvim runs in a hidden terminal inside the studio,
  started with `THEME_COLORS=<draft>` and opening a sample file (🔸
  `nvim/lua/theme.lua` itself, read-only). Every colour change rewrites the
  draft file (with a short delay, ~50 ms), and nvim recolours itself.
  `Ctrl+w` moves the keyboard focus into nvim so you can scroll or try
  things; the same key gives focus back.
- **Terminal:** bash and starship in another hidden terminal, running a
  short demo script: `ls --color`, `git log --graph`, `git status`, a failing
  command (so the red `$` shows), and `bat` on a file. The renderer swaps
  colours 0–15 and the default fg/bg for the draft palette as it draws
  (using `terminal-colors.json`), so nothing needs to be reloaded.
- **yazi:** 🔸 `Y` switches the terminal window to yazi, browsing the
  dotfiles folder. Its colours are already named terminal colours, so the
  colour swap above covers it. Image previews are turned off inside the
  studio (the embedded terminal can't show images).
- **i3 bar:** workspace buttons drawn in the focused/inactive colours. The
  i3blocks scripts are each run once with a short timeout to get real text,
  with sample text as a fallback. The colours the scripts set themselves
  (bluetooth battery tint and so on) are kept.
- **rofi overlay:** `R` shows a centred mock of the palette menu (swatch
  rows, the selected row, the red delete message) using the rasi colour roles.
- **Size:** each window in the preview needs about 40×12 cells, so the
  studio wants roughly a 160×40 terminal and shows a message when it's
  smaller.

## Phase 5: fonts tab (left pane, second tab)

- `F1` / `F2` switch the left pane between **Colours** and **Fonts** (`Tab`
  already moves between the picker's controls); the
  preview stays visible.
- There are two slots, **mono** and **ui**, each with a family and size.
  Changing either updates a rendered sample image above the list: code in
  the mono font; an i3 title, bar text and rofi rows in the ui font; drawn
  in the current palette.
- **Sizes per machine:** each slot shows the base size saved in
  `fonts.json` and the size actually used on this host after the
  `hosts/<host>/fonts.json` offset (e.g. `14 → 17 on laptop`). Sample images
  use the actual size.
- **Installed fonts:** families from `fc-list`, searchable, with a filter
  for monospaced fonts and a Nerd Font badge. Monospace detection uses the
  same check as `font-switch.sh`: monospaced fonts report a `spacing` value
  in `fc-list`, proportional ones don't.
- **Warnings:** `font-switch.sh` only reports a missing font on stderr,
  which you never see when it runs from rofi. The studio shows these
  warnings itself: font not installed, mono font without Nerd Font icons.
- **Saving:** `s` saves the slots as a preset in `fonts.json`; `a` applies
  it with `font-switch.sh`.
- **Possible third slot:** the ui font currently covers the bar, rofi and
  GTK at one size (now 14). If one size doesn't suit all three, a separate
  **bar** role goes into `fonts.json` / `font-switch.sh`, and this tab gets a
  third slot.

## Phase 6: downloading fonts

- **Google Fonts:** search through the Fontsource API, which needs no key
  and lists ~2,100 fonts with category, licence and weights. The files come
  from the `google/fonts` GitHub repo (`ofl/<id>/*.ttf`), because Fontsource
  only serves files cut down to one script, which isn't good enough for a
  system font. Fonts Fontsource lists that aren't from Google fall back to
  its latin + latin-ext files.
- **Nerd Fonts:** the list comes from the latest
  `ryanoasis/nerd-fonts` GitHub release (currently v3.5.1); the
  `<Name>.tar.xz` file is downloaded.
- **Install:** unpack into `~/.local/share/fonts/theme-studio/<id>/`, run
  `fc-cache`, and the font is usable straight away (the preview image
  refreshes). Download progress shows in the status line, and the
  download runs in the background so the UI stays responsive.
- **Making it permanent in Nix:** each download is added to
  `configs/fonts-installed.json` (`{ "Inter": { "source": "google" } }`).
  A new `home/fonts.nix` reads that list and installs:
  - Google fonts with `pkgs.google-fonts.override { fonts = [ … ]; }`
  - Nerd Fonts with `pkgs.nerd-fonts.<attr>`
  - anything else with `fetchurl` plus the hash recorded at download time.

  After a rebuild, the studio's own copy in `~/.local/share/fonts` is no
  longer needed; activation removes copies that Nix now provides, so there
  are no duplicate families.
- 🔸 **Unused fonts:** an "uninstall" action for studio-installed fonts
  removes the files and the list entry.

## Phase 7: polish

- **Launching** ✅ (done early): `$mod+Ctrl+t` runs
  `ghostty --class=floating.theme-studio -e theme-studio`; an i3 rule floats
  it at 96% × 92% of the monitor (a fixed 1920×1080 hung off your portrait
  monitor) with the 1-pixel border your other Ghostty windows have.
- `?` help overlay ✅ (done early).
- A status line shows unsaved changes, the active palette and font preset,
  and warnings (missing Nerd Font, terminal too small, low contrast).
- README section and comments in the same style as the existing scripts.

---

## Rust crates

`ratatui`, `crossterm` (mouse), `tui-term` + `vt100` + `portable-pty`
(embedded terminals), `ratatui-image` (kitty graphics), `serde` /
`serde_json` with `preserve_order`, `ureq` (rustls; small, blocking, runs in
a thread), `tar` + `xz2`, `sha2`.

No new **system** dependencies: ImageMagick, fontconfig, nvim, yazi, git and
curl are already installed. `bat` is assumed for the demo script.

## Order and checkpoints

1. Phase 0 + Phase 1 (fonts across the system). Usable without the studio;
   I stop here for you to try it.
2. Phases 2–3: palette editing with a simple colour-swatch preview.
3. Phase 4: the full live preview.
4. Phases 5–6: fonts in the studio.
5. Phase 7.

Each step is committed separately in the repo it belongs to (`configs/` or
the outer repo).
