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
| ✅ Font roles | Three roles: **mono** (terminal, nvim, lock screen), **ui** (rofi, GTK) and **bar** (i3 titles and bar; added after Phase 4, falls back to ui) |
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
    "ui":   { "family": "Inter", "size": 10 },
    "bar":  { "family": "Inter", "size": 9 }
  }
}
```

**Generated files and where each is used:**

| App | Generated file | Change needed in the existing config | When it updates |
|---|---|---|---|
| Ghostty | `fonts/ghostty` (`font-family`, `font-size`) | `config-file = ?~/.config/fonts/ghostty` | Immediately (SIGUSR2) |
| i3 titles + bar (i3blocks) | `fonts/i3` (`font pango:<bar> <size>`) | `include ~/.config/fonts/i3`; remove the `font` line from `hosts/*/i3.conf` | Immediately (`i3-msg restart`) |
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
  `configs/hosts/<host>/fonts.json` (`{ "mono": 0, "ui": 3, "bar": 3 }`) adjusts them
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

## Phase 4: live preview (right pane) ✅ done

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
- **Size and resizing:** the layout already follows the window as it's
  resized (see "Resizing" below). The preview desktop has to do the same:
  every layout change resizes the embedded terminals (nvim, terminal, yazi)
  through their PTYs, so they reflow like a real window would. The two
  preview windows sit side by side when the preview is wide enough and
  stack when it isn't; below about 40×12 cells each, the preview raises
  the minimum size the "too small" message asks for.

**Built as (differences from the above):**
- **Keyboard into the preview:** `F12` or a click, not `Ctrl+w` (nvim uses
  `Ctrl+w` for its own windows). F12 again moves to the next preview
  window, then back to the editor; while a window has the keyboard, every
  key (Ctrl+C too) goes to it, and pastes are sent as bracketed paste.
- **Demo shell:** `ls`, `git --no-pager log` (with the pager, `less` ate
  the next typed command) and `false`, typed once the first prompt appears
  so they aren't echoed early. It runs with `HISTFILE=/dev/null`, so the
  demo stays out of your history. No `bat`/`git status`: the window is
  small and they pushed the prompt out of view.
- **The draft palette** is written to `$XDG_RUNTIME_DIR/theme-studio-<pid>-<n>/colors.sh`
  on every change; nvim's file watcher picks it up. Folders left by a studio
  that was killed (closing its window) are removed the next time one starts.
  Closing the window also ends nvim, the shell and yazi (they get a hangup).
- **yazi:** image previews aren't turned off explicitly; the embedded
  terminal ignores graphics escapes, so they simply don't show.
- **i3blocks:** each block runs in its own process group with a 3 s
  timeout, and the whole group is stopped afterwards (so `volume.sh`'s
  `pw-mon` doesn't outlive it). Blocks that print nothing are left out, as
  i3bar does.
- **rofi overlay:** only the palette being edited shows swatches; the
  other rows are names.
- **`V`** switches the preview to the earlier swatch view (every key, the
  16 terminal colours, a code sample), which is still handy for small
  keys like `bg_statusline2`.
- Two windows side by side from 60 preview columns, stacked below that;
  the minimum window size didn't need to change.

## Resizing ✅ done (added after Phase 3)

Works like btop: the window is resized by i3 (with the mouse:
`$mod` + right-drag, or by dragging the border), and the studio reflows
to whatever size it gets.

- **Side by side** (editor 52 columns, preview the rest) when the window
  is at least 112 columns wide.
- **Stacked** below that: editor on top (key list, then the picker), the
  preview underneath with 40% of the height.
- **Too small:** below 112×23 side by side, or 52×35 stacked, the window
  shows the current and needed width and height, with the short side in
  red, and redraws as soon as it fits again.
- A test renders every window size from 1×1 to 220×80 (debug build, so
  arithmetic underflow would fail it).

## Phase 5: fonts tab (left pane, second tab) ✅ done

- `F1` / `F2` switch the left pane between **Colours** and **Fonts** (`Tab`
  already moves between the picker's controls); the
  preview stays visible.
- There are three slots, **mono**, **ui** and **bar**, each with a family
  and size (bar can also be left to follow ui). Changing one updates a
  rendered sample image above the list: code in the mono font, rofi rows
  in the ui font, an i3 title and bar text in the bar font, drawn in the
  current palette.
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

**Built as (differences from the above):**
- **Panels:** ¹ the three roles (j/k role, h/l size, H/L by 4, Enter or `f`
  opens the list), ² installed fonts (`/` search, `m` monospaced only,
  Enter uses the font, Esc back). `3`/`4` switch to the Colours tab and its
  preview windows. The sample replaces the preview while this tab is open.
- **Browsing previews:** while the list has the keyboard, the sample shows
  the highlighted font in the selected role, before you pick it.
- **Sample:** rendered in the background by ImageMagick/Pango (about 35 ms)
  at the X server's DPI (`Xft.dpi`) and this machine's sizes, so it is
  the real size; shown with the kitty graphics protocol (half blocks if a
  terminal doesn't answer the query). It includes Nerd Font icons, to show
  whether a font has them.
- **Nerd Font check:** a font counts as a Nerd Font when fontconfig says it
  has U+F115 (an icon every Nerd Font carries).
- **Unsaved changes:** the quit prompt covers both tabs; `s` there saves
  whichever has a name.
- **`theme-studio --fonts`** starts on this tab.
- **Not yet:** mouse in this tab (keyboard only).

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
- **Numbered panels** ✅ (added after Phase 4): btop-style superscript
  numbers in the titles, ¹ key list, ² colour picker, ³ nvim, ⁴ shell/yazi;
  the number keys jump there (3 and 4 give that window the keyboard, F12
  hands it back). The Phase 5 fonts tab continues the numbering.
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
