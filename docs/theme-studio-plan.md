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

|                   | Choice                                                                                                                                         |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| ✅ Stack          | Rust + ratatui (crossterm backend)                                                                                                             |
| ✅ Font roles     | Three roles: **mono** (terminal, nvim, lock screen), **ui** (rofi, GTK) and **bar** (i3 titles and bar; added after Phase 4, falls back to ui) |
| ✅ Font install   | Into `~/.local/share/fonts/theme-studio/` straight away, and recorded in a list that Nix installs on the next rebuild                          |
| ✅ Font preview   | Sample text rendered to an image (ImageMagick + Pango) and shown inline using the kitty graphics protocol                                      |
| ✅ Palette keys   | All 34 keys can be edited. Derived keys follow their base colours until you edit one by hand, which **pins** it                                |
| ✅ Picker         | HSV square, hue strip, H/S/L sliders, hex input; mouse and keyboard                                                                            |
| ✅ Font sources   | Google Fonts (search via Fontsource) and Nerd Fonts releases                                                                                   |
| ✅ Preview extras | Rofi menu overlay and a yazi file-browser view                                                                                                 |
| ✅ Window         | Opens as a floating i3 window (its own Ghostty window class)                                                                                   |

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
    "ui": { "family": "Inter", "size": 10 },
    "bar": { "family": "Inter", "size": 9 }
  }
}
```

**Generated files and where each is used:**

| App                                                                                        | Generated file                                                             | Change needed in the existing config                                        | When it updates                |
| ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------------------------ |
| Ghostty                                                                                    | `fonts/ghostty` (`font-family`, `font-size`)                               | `config-file = ?~/.config/fonts/ghostty`                                    | Immediately (SIGUSR2)          |
| i3 titles + bar (i3blocks)                                                                 | `fonts/i3` (`font pango:<bar> <size>`)                                     | `include ~/.config/fonts/i3`; remove the `font` line from `hosts/*/i3.conf` | Immediately (`i3-msg restart`) |
| rofi                                                                                       | `fonts/rofi.rasi` (`* { font: "<ui> <size>"; }`)                           | `@import` in `rofi/config.rasi`                                             | Next time rofi opens           |
| Lock screen                                                                                | `fonts/lock.sh` (`XSECURELOCK_FONT=<mono>:size=N`)                         | `lock-screen.sh` sources it instead of hard-coding GeistMono                | Next lock                      |
| GTK 3/4 (blueman, Firefox UI, …)                                                           | `gtk-3.0/settings.ini`, `gtk-4.0/settings.ini` (`gtk-font-name`)           | none; home-manager doesn't manage GTK yet                                   | When each app is restarted     |
| Everything that asks for "monospace" or "sans-serif" (Firefox page text, Electron apps, …) | `fontconfig/conf.d/60-theme-fonts.conf` (aliases to the mono and ui fonts) | none                                                                        | When each app is restarted     |
| nvim, yazi, starship                                                                       | follow Ghostty                                                             |                                                                             |                                |

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
│ nvim (real config)  ││ terminal         │   focused window: client.focused
│                     ││ $ ls  git log …  │   other window: client.unfocused
│                     ││                  │
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
- **Grouped list** (added after Phase 5): two levels, design then variant
  (JetBrains Mono → Nerd Font / NL Nerd Font; Noto → Sans CJK, Serif CJK…),
  single-member groups flattened; searching shows a flat list. Filter
  chips: `m` mono, `p` propo, `n` NF, `M` NF Mono, `P` NF Propo, `x` no NF
  (one of each kind at a time), `b` bitmap and `c` CJK·emoji·math (hidden
  until switched on). Families only; weights are not picked.
- **Mouse:** click a role, a chip or a row (a second click opens a group or
  uses the font); the wheel scrolls the list.

## Phase 5b: layout rework, settings and keymaps (before Phase 6)

### Decisions

|                        | Choice                                                                                                                                 |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| ✅ Layout              | One main page, three columns: fonts, colours, preview                                                                                  |
| ✅ Narrow windows      | Fonts and colours side by side on top; the preview becomes a full-width row underneath                                                 |
| ✅ Pages               | Main, Download fonts (Phase 6), Settings (Keymaps + Behaviour)                                                                         |
| ✅ Colour groups       | Collapsible like the font groups; only Base open at start                                                                              |
| ✅ Picker              | Hidden at start; opens when you edit a colour; hides on Esc; `<leader>cp` toggles it                                                   |
| ✅ Visibility defaults | All of them adjustable on the Settings page                                                                                            |
| ✅ Leader              | Space, changeable; a popup after it lists what can follow                                                                              |
| ✅ Keymaps             | Every action rebindable on the Settings page; presets Vim (current keys), Helix, Emacs, VS Code, JetBrains; editable, save your own    |
| ✅ Storage             | `$XDG_CONFIG_HOME/theme-studio/` (default `~/.config/theme-studio/`); on this system linked to `configs/theme-studio/` in the dotfiles |
| ✅ On start            | Always from the Settings defaults (sizes, open groups, picker); nothing remembered between runs                                        |

Smaller defaults I picked are marked 🔸.

### 1. Pages

- **Main** (`F1`): everything below.
- **Download fonts** (`F2`): Phase 6 goes here; a placeholder until then.
- **Settings** (`F3`): two tabs, **Keymaps** and **Behaviour**.
- 🔸 `F1`–`F3` in every preset (also on the leader: `<leader>1`–`3`).

### 2. Main page layout

```
wide (≈150+ columns)
┌¹Fonts────────┐┌⁴Palette: novmbr───┐┌⁶nvim──────────────────┐
│ mono  …      ││ ▾ Base            ││                        │
│ ui    …      ││   bg0  ██ #241d1a ││                        │
│ bar   …      ││ ▸ Backgrounds 10  │├⁷shell─────────────────┤
├²Installed────┤│ ▸ Text …       5  ││                        │
│ ▸ DejaVu  3  ││ ▸ Greys        3  ││                        │
│ ▾ JetBrains… │├⁵Colour (picker)───┤│                        │
├³Sample───────┤│ (hidden until you ││                        │
│ [image]      ││  edit a colour)   ││ 1 2 3      vol │ 18:41 │
└──────────────┘└───────────────────┘└────────────────────────┘

narrow
┌¹²³ fonts column─┐┌⁴⁵ colours column─┐
│                 ││                  │
└─────────────────┘└──────────────────┘
┌⁶nvim─────────────────┐┌⁷shell─────────────────┐
└──────────────────────┘└───────────────────────┘
 1 2 3                                vol │ 18:41
```

- **Left column, fonts:** ¹ roles, ² installed fonts, ³ sample image.
- **Centre column, colours:** ⁴ colour list, ⁵ picker. While the picker is
  hidden, the list gets the whole column.
- **Right column, preview:** ⁶ nvim on top, ⁷ shell (or yazi, `Y`) below,
  the i3 bar at the bottom; the rofi overlay (`R`) is drawn over the
  column. `V` still switches the column to the swatch view.
- **Narrow:** the preview's two windows go side by side in the bottom row.
- Panel numbers ¹–⁷ jump to each pane, as now.
- 🔸 **Saving:** `s`/`S`/`a`/`o`/`n` act on the column with focus (palette or
  font preset). Explicit versions on the leader: `<leader>sp` / `<leader>sf`
  save palette / fonts, `<leader>ap` / `<leader>af` apply.

### 3. Resizing panes

- **Mouse:** drag the border between two columns, or between two panes in
  a column.
- **Keyboard:** 🔸 `<leader>r` enters a resize mode, like i3's `$mod+r`:
  `h`/`l` move the focused pane's side edge, `j`/`k` its lower edge (capitals
  for bigger steps), `=` resets to the defaults, Esc or Enter leaves. The
  status line shows `RESIZE` while it's on.
- Every pane has a minimum size; sizes are shares of the window, so they
  scale when the window is resized. nvim, the shell and yazi follow, as now.
- Default sizes come from Settings (decision: nothing is remembered).

### 4. Visibility

- **Colour groups** get ▸/▾ rows like the font groups (`l`/`h`/Enter/click);
  only Base is open at start.
- **Picker:** hidden at start. Enter or a click on a colour opens it and moves
  the focus there; Esc back to the list hides it. `<leader>cp` shows or hides
  it whenever the colour list or picker has focus.
- **Behaviour settings** (the Settings page), all defaults for each start:
  - which colour groups start open (each group on/off; Base on)
  - picker at start: hidden / shown
  - picker after editing: hides on Esc / stays open
  - font list: groups closed / open to the role's current font when opened
  - font list filters at start (bitmap and CJK·emoji·math hidden, the chips)
  - font sample pane: shown / hidden (`<leader>fs` toggles it)
  - default pane sizes (set by resizing, then "save as default")

### 5. Leader key and keymaps

- **Actions:** every key-driven action gets a name and a context, e.g.
  `list.down` (colour list), `picker.toggle` (colour list and picker),
  `fonts.save` (fonts column), `preview.leave` (inside a preview window),
  `resize.left` (resize mode). The current keys become the **Vim** preset, and
  tests check it behaves exactly as today.
- **Bindings** are a key or a sequence: `j`, `C-r`, `<leader>cp`, `g g`,
  `C-x C-s`. After the first key of a sequence, a small popup lists what can
  follow (which-key style) until you press one or Esc.
- **Leader:** Space by default, set on the Settings page. Each preset brings
  its usual leader: Vim and Helix Space, Emacs `C-c`, VS Code `C-k`,
  JetBrains Space.
- **Presets:** Vim, Helix, Emacs, VS Code, JetBrains. They ship inside the
  program, are written to `keymaps.json` on first run, and can be edited,
  saved under a new name, or reset to the shipped version.
- **Keymaps tab:** actions grouped by context with their keys; Enter rebinds
  (press the new key or sequence, Enter to finish), Delete unbinds; it warns
  about conflicts in the same context, about keys i3 takes first (Alt is your
  `$mod`), and about leaving no way out of a preview window.
- 🔸 **More distinct keys:** the studio asks Ghostty for the kitty keyboard
  protocol (disambiguate mode) so combinations like Ctrl+Shift+Z or Ctrl+1
  arrive as themselves; other terminals keep working with plain keys.
- **JetBrains** Alt+number pane keys clash with i3's Alt, so they move to
  `<leader>1`…`7`.

### 6. Settings files

- `$XDG_CONFIG_HOME/theme-studio/` (default `~/.config/theme-studio/`):
  - `keymaps.json`: `{ "active": "Vim", "presets": { "Vim": { … }, … } }`
  - `settings.json`: leader, visibility defaults, default pane sizes
  - Missing files are created with the defaults; both are written in jq's
    format like the other JSON files.
- **On this system:** the files live in `configs/theme-studio/` (committed in
  the configs repo) and `home/base.nix` links `~/.config/theme-studio` to
  that folder, like nvim, i3 and the rest. I check the link after `nrs`.
- palettes.json and fonts.json stay where they are (`THEME_STUDIO_CONFIGS`).

### Progress

- **Step 1 ✅** keymap engine. Notes: Space was "open group" in the font
  list and is now the leader (`l`, Enter and clicks still open groups);
  hints show friendly names (`Enter`, `Space s p`) while the help screen
  and settings use Vim notation; Ctrl+C always quits; text fields (hex,
  search, names) and the pop-ups' y/s/Esc keys aren't rebindable.
- **Step 2 ✅** settings files. keymaps.json: `{ "active", "presets": { name:
  { "leader", "bindings": { context: { action: [keys] } } } } }`; an empty
  list means unbound; actions a file doesn't mention get the shipped keys
  (so new actions appear). settings.json so far: the font list's starting
  filters. Missing files and entries are written; files that don't parse
  are left alone and reported in the status line. `theme-studio
  --init-config` writes them and exits. Linked like nvim (after `nrs`).
- **Steps 3–4 ✅** layout and visibility. Three columns from 148 columns,
  fonts | colours over a preview row from 98; panes ¹–⁷ (number keys,
  `<C-w>h/j/k/l` between the fonts and colour panes, clicks). Colour groups
  collapse like font groups; the list cursor moves over group rows too.
  Picker: hidden at start, Enter (or a second click) on a colour opens it,
  Esc and leaving it hide it, `<leader>cp` toggles; `<leader>fs` toggles the
  sample. F2/F3 pages are placeholders. keymaps.json's `tab-colours` became
  `page-main`, `tab-fonts` was dropped (automatically).
- **Follow-ups** ✅: Esc in the picker discards the edit (back to the palette
  before its first change; those undo steps dropped), Enter keeps it, any
  other way out keeps it. No font sample under the narrow layout. Panes on
  `<leader>1`–`7` (keymaps.json version 2 migrates the old digit defaults).
  The preview frame is magenta while nvim or the shell has the keyboard.

### 7. Additions (asked for after step 4)

**Picker edits** ✅ (done): only Enter (the picker's Confirm key, or Enter in
the hex box) keeps a change. Leaving the picker any other way puts the
colours back: Esc, another pane or column, a preview window, a click
elsewhere, hiding the picker, or Tab past the last control.

**Status line** ✅ (done): just the page tabs, then hints and messages. The
palette or preset name and "active:" are gone; the unsaved-changes dot is in
the pane titles.

#### Tab bar position

- A setting, `layout.tabs`: `"bottom"` (default, as now) or `"top"`.
- On top, the tabs get their own row above the columns; the hints and
  messages stay in the bottom line. At the bottom they share that line, as
  now.
- Clicking a tab opens that page (see hover states below).

#### Hover states for everything clickable

- Every clickable thing shows that it's clickable when the mouse is over it:
  - **Rows:** colour, group, font, role, palette and preset rows in the
    pop-ups (a background highlight on the row).
  - **Controls:** filter chips, page tabs, pane titles (a click focuses the
    pane), and the picker's square, hue strip, sliders and hex box (the
    marker or label lights up).
  - **Preview windows:** nvim and the shell (the frame lights up in the
    capture colour, since a click hands them the keyboard).
  - **Borders:** pane borders that can be dragged to resize (step 6 below).
- **How:** each draw records a map of what's clickable where. Clicks,
  hovering and the later border dragging all use that one map, replacing
  the separate rectangles kept now. The terminal reports mouse movement,
  which the studio already asks for.
- 🔸 **Pointer shape:** in Ghostty, the mouse pointer also turns into a hand
  over clickable things (OSC 22), and back to the arrow elsewhere. Other
  terminals ignore it.

#### Versions and history for every config

- **Version numbers:** keymaps.json, settings.json, palettes.json and
  fonts.json each get `"version"`, so later format changes can be migrated
  automatically, as keymaps.json's panes were.
  - **New layout for the data files:** palettes.json becomes
    `{ "version": 2, "palettes": { … } }`, and fonts.json likewise with
    `"presets"`. A top-level `version` key would otherwise be read as a
    palette.
  - **Updated with it:** `theme-switch.sh`, `font-switch.sh`, their rofi
    menus and `lib/rofi-picker.sh`, the Nix activation steps,
    `palette-derive.jq`, and the studio. A one-time migration rewrites both
    files; the scripts accept either layout during the switch.
- **History:** before every save the studio makes (palette, font preset,
  keymaps, settings), the file as it was is kept in
  `~/.local/state/theme-studio/history/<file>/` (per machine, outside the
  dotfiles repo). The newest 100 per file are kept. A restore takes a
  snapshot first, so a restore can itself be undone.
- **Saved versions:** you can save the current state of a file under a name
  (e.g. "before trying Emacs keys"). Named versions are never pruned.
- **History tab** (Settings page, next to Keymaps and Behaviour):
  - Pick a file, then a named version or a snapshot (time, plus a one-line
    summary of what changed, e.g. "novmbr: red, aqua").
  - **Preview first:** the selected version is shown in full, with its values
    in place. Every entry that differs from what you have now is
    highlighted and shown as `current → selected`, so the changes are
    visible straight away. Nothing changes until you confirm.
  - For palettes.json, 🔸 the preview pane also shows the selected
    version's colours for the palette you're editing (a strip of
    swatches, now and selected, above the differences; colour values get a
    swatch too).
  - Restore (with the preview's confirm key), save the current state as a
    named version, delete a named version.
  - 🔸 `<leader>u` undoes the last config change from anywhere (restores the
    newest snapshot of the file the studio last saved).

### 8. Order of work

Each step is committed separately and keeps the studio working.

1. ✅ Action names and the keymap engine (sequences, leader, popup).
2. ✅ Settings files and the `~/.config/theme-studio` link.
3. ✅ Main page: three columns, the narrow layout, panes ¹–⁷, pages F1–F3.
4. ✅ Collapsible colour groups, the picker's visibility, the behaviour
   settings; follow-ups (picker discard, leader panes, capture colour,
   status line).
5. ✅ **Clickable map, hover states and tabs:** the click map, hover highlights
   (and the pointer shape), clickable page tabs, and the tab bar position
   setting.
6. ✅ **Resizing:** dragging borders (they get hover states too) and
   `<leader>r` resize mode.
7. ✅ **Versions and history (behind the scenes):**
   - version fields and migrations for all four files, with the new data
     file layout and the scripts updated
   - snapshots in `~/.local/state`, pruning to 100, and named versions
8. ✅ **Settings page:**
   - **Keymaps tab:** rebinding, conflicts, and presets.
   - **Behaviour tab:** the leader, visibility defaults, tab position,
     default pane sizes.
   - **History tab:** browsing, the preview, restoring, named versions.
9. ✅ **More presets:** Helix, Emacs, VS Code and JetBrains, plus Ghostty's
   extended keyboard mode (kitty protocol) for keys like Ctrl+Shift+Z.

## Phase 5c: fixes after trying 5b

Asked for after phase 5b; the decisions are settled.

### 1. Configurable hover look

- **Now:** everything clickable gets a dark grey background and an
  underline when the mouse is over it.
- **Becomes:** a Hover section on the Behaviour tab. Any combination of:
  - **Highlight:** on/off, and a background colour.
  - **Underline:** off, single or double (changed from five styles after
    trying them), and a colour.
  - **Text:** on/off, a colour, and a weight: normal, bold or dim (the
    weights terminals have).
- **Colours:** each is a key of the active system palette (e.g.
  `bg_visual`, `yellow`), so they follow theme switches. They're picked
  from a list of the palette's keys with swatches.
- **Default:** no highlight, a single underline, and the text in `yellow`
  (underline in `yellow` too).
- **Live sample:** a sample row on the Behaviour tab shows the look as set,
  before it's saved.
- **How:** ratatui only knows single underlines, so a double one is marked
  in the cell and the terminal output turns the mark into SGR 4:2;
  underline colours use SGR 58.

### 2. Pane borders light up on hover

- **Now:** only the preview windows (nvim, the shell) light up their frame
  when the mouse is over them. The other panes (¹ Fonts, ² Installed fonts,
  ³ Sample, ⁴ Palette, ⁵ picker) only highlight their title.
- **Becomes:** the mouse anywhere over a pane lights up its border, as the
  preview does.
- **Two colours**, both theme keys set on the Behaviour tab and separate
  from the element hover colours:
  - **Panes:** default `aqua`.
  - **Preview windows:** default `purple` (as now).
- **Focus:** the yellow border of the focused pane still wins.

### 3. Font sample

- **No extra text:** no "at 96 dpi, sizes as on this machine" or
  "previewing the highlighted font" in the title, and no "mono ·
  JetBrains Mono 11" label lines in the image.
- **Content:** the mock-ups stay, each in its role's font at its
  effective size (what it really looks like on this screen):
  - **mono:** code
  - **ui:** the rofi menu
  - **bar:** the bar strip

### 4. Font roles row layout

- **Wide and three-column layouts:** the size sits on the font name's row,
  to the right of the name.
- **Narrow layout:** the size stays below the name, as now.
- **Description:** stays on the lower row in every layout.

### 5. Palette: show what a colour is derived from, and what from it

- **Derived colour selected** (e.g. `bg_visual_red`): the rows it comes
  from (`bg0`, `red`) get the derived-colour highlight.
- **Base colour selected** (e.g. `red`): the colours derived from it
  (`bg_visual_red`, `bg_diff_red`, `bg_red`, …) get it too, so you can
  see what editing it changes.
- **Its own look:** the derived-colour highlight is configured separately
  from hover (the same choices: highlight, underline, text colour and
  weight), in its own group on the Behaviour tab. Default: a
  `bg_visual_blue` background.
- **Live:** editing a base colour in the picker moves every unpinned
  derived colour (and the preview) as you go, before Enter keeps it.
- **Pinned colours:** a pinned (hand-set) key isn't derived, so it's left
  out both ways.
- **Closed groups:** a related colour in a closed group lights up the
  group's row.

### 6. Keymaps tab: browsing presets

- **Preset button:** the preset row becomes a button. Enter or a click
  opens the preset list in the right column.
- **Browsing:** moving through the list shows, on the left, the
  highlighted preset against the current keymap:
  - only the actions whose keys differ, as `current → preset`, grouped by
    context
  - `A` toggles to the whole keymap with the differences highlighted (as
    in History)
  - the view scrolls
- **Loading:** Enter (or a click on the highlighted preset) loads it, as
  an unsaved change (see 7). Esc goes back without changing anything.
- **In the list:** `d` deletes a preset (shipped presets can only be
  reset), and `n` makes a new preset from the highlighted one (asks for a
  name).

### 7. Settings changes wait for Save

- **Now:** every change on the Settings page is saved at once.
- **Becomes:** a change makes a Save button appear. Nothing is written,
  and nothing takes effect, until the button is clicked or the save key is
  pressed. The "saved straight away" note goes.
- **One Save for the page:** Keymaps and Behaviour changes add up, and
  Save writes keymaps.json and settings.json together. The button shows
  on both tabs, with a count of what's unsaved.
- **Save key:** the preset's Save action: `s` in Vim and Helix, `C-x C-s`
  in Emacs, Ctrl+S in VS Code and JetBrains. It can be rebound like any
  other key.
- **Unsaved changes:**
  - They stay pending while you move between pages; a ● on the Settings
    tab shows it.
  - Quitting asks save / discard / cancel, as palettes do.
- **Discard:** `D` (the discard-changes action) or the button puts the page
  back as last saved.
- **Not affected:** the History tab's restore and naming act at once
  (they ask first), and `<leader>u` stays immediate.

### 8. Pages other than Main

- No "F1: back to the main page" at the bottom of the frame: the status
  line already shows the key next to Main.

### 8a. Behaviour tab groups and in-page tabs (asked for after step 3)

- **Groups:** the Behaviour tab's rows sit under titles, like the Keymaps
  tab's contexts: Keys, Layout, Hover, Palette pane, Derived colours,
  Fonts pane. The two looks end with a sample row. The cursor skips
  titles and samples.
- **In-page tabs position:** a Layout setting puts a page's own tabs
  (the Settings page's Keymaps / Behaviour / History) at the top or the
  bottom of the page. Default: top.
- **Tabs take the keyboard at the edge:** moving past the end of a tab's
  list towards the tabs (up from the first row when they're on top, down
  from the last when at the bottom) puts the keyboard on the tab strip.
  There, left/right change tabs; moving back, Enter or Esc return to the
  list.

### 9. Order of work

1. ✅ Small fixes: the page frames' "back" note, the sample's title and labels,
   and the roles' size placement.
2. ✅ Hover settings and their output (styles, colours, weights), pane border
   hover colours, and the Behaviour tab's Hover section with its sample.
3. ✅ Derived-colour highlights in the Palette.
3a. ✅ Their own configurable look; Behaviour tab groups; single/double
   underlines only; in-page tabs position and keyboard focus.
4. ✅ Settings pending changes: the Save / Discard buttons, the ● on the tab,
   and the quit prompt.
5. ✅ The preset browser with its difference view, delete and new-from.

## Phase 6: downloading fonts ✅

On the Fonts page (F2): Google Fonts, Nerd Fonts and Installed tabs, search
and category chips, details with Install / Remove; progress in the status
line. Below, what was planned; differences are noted.

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
  `home/fonts-installed.json` in the main repo (changed from
  `configs/`: that repo is ignored by the flake, and pure evaluation
  can't read it). The path is a setting, `fonts.installed-list`
  (default: next to settings.json). A new `home/fonts.nix` reads the
  list and installs:
  - Google fonts with `pkgs.google-fonts.override { fonts = [ … ]; }`
  - Nerd Fonts with `pkgs.nerd-fonts.<attr>`
  - anything else with `fetchurl` plus the hash recorded at download time.

  After a rebuild, the studio's own copy in `~/.local/share/fonts` is no
  longer needed; activation removes copies that Nix now provides, so there
  are no duplicate families.

- 🔸 **Unused fonts:** an "uninstall" action for studio-installed fonts
  removes the files and the list entry.

## Phase 7: themes, font picking, previews and a font tournament

Asked for after Phase 6; the decisions are settled. (Polish and the README
move to Phase 8.)

### 1. Themes

- **Now:** palettes and font presets are opened and saved separately; the
  pane titles carry their names ("¹Fonts: JetBrains Mono ●",
  "⁴Palette: One").
- **Becomes:** a **theme** is a name plus a palette plus a font preset,
  saved as one. It points at them by name, in `configs/themes.json`:
  `{"version": 1, "themes": {"Work": {"palette": "Gruvbox Dark",
  "fonts": "JetBrains Mono"}}}`. Palettes and font presets stay reusable
  building blocks: editing one changes every theme that uses it. The
  scripts keep reading palettes.json and fonts.json as now.
- **In the studio:** Save, Save as, Open and New act on the theme; the dot
  shows when either part changed. The pane titles lose the names ("¹Fonts",
  "⁴Palette"). The theme's name and its dot sit in the page tab row:
  " F1 Main  F2 Fonts  F3 Settings   Theme: Work ●", wherever the tabs
  are (top or bottom).
- **Parts stay reusable:** `<leader>op` / `<leader>of` swap another
  palette / font preset into the theme; `<leader>np` / `<leader>nf` start
  new ones. (Built this way rather than per pane: `o` opens a theme
  everywhere.)
- **Saving:** `s` saves edited parts in place and the theme; a new part
  takes the theme's name. **Save as** under a new name forks: edited parts
  are saved under the new name (other themes using them keep theirs),
  unedited parts stay shared. (Added while building: without it there was
  no way left to copy a palette.)
- **Apply:** applies both parts (theme-switch.sh and font-switch.sh) and
  records the active theme in `~/.config/colors/theme`.
- **The rofi picker:** a new `themes-switch` picker on the current key
  lists themes (palette and fonts together, with Alt+BackSpace to delete
  as now). The palette-only picker moves to another key for quick colour
  changes.
- **Migration:** on first start, the active palette and font preset become a
  theme.

### 2. Picking a role's font

- **Enter (or `f`) on a role** (mono, ui, bar) moves to Installed fonts with
  that role's filter on. The filter chips can be switched off as now, to
  pick anything.
  - **mono:** Mono (monospaced; this includes the Nerd Font Mono
    variants). "Nerd Font Mono, falling back to Mono" depends on the
    family, which a list filter can't express; `F` (below) does it.
  - **ui:** Propo (proportional).
  - **bar:** Nerd Font (the bar shows icons).
  - The filters per role are settings (Behaviour tab, Fonts pane group).
- **The list shows which role** you're picking for.
- **Choosing applies at once:** the font is set for the role, the theme's
  font preset is saved (font-switch.sh reads saved presets), and it's
  applied system-wide: Ghostty reloads and i3 restarts, as with Apply now.

### 3. A whole family at once

- **`F` on a family** in Installed fonts fills every role from it. The
  family is the group plus fonts named like it (the font tree can put a
  family's Nerd Font variants in a group of their own); a font in no group
  is its own family.
  - **mono:** the family's Nerd Font Mono, else its monospaced face.
  - **ui:** its Nerd Font Propo, else its proportional face, else the
    base.
  - **bar:** its Nerd Font, else its Nerd Font Propo, else the base.
- **Applies at once,** like a single pick; the message says what went where.
  A role the family has no face for keeps its font (no proportional font
  in the terminal).

### 4. Previews on the Fonts (download) page

- **A preview pane** next to the details: the highlighted font rendered
  before installing it.
- **Fetched just for the preview** into `~/.cache/theme-studio/preview/`:
  one regular (400) file per Google / Fontsource font from Fontsource's
  CDN (one small request), loaded automatically. Nerd Fonts only come as
  1–50 MB archives, so they preview on request (`p`, showing the size
  first); archives stay cached.
- **Rendered like the sample** (Pango through ImageMagick), with a
  fontconfig setup that includes the cache folder, so nothing is
  installed. Colours from the theme's palette.
- **Texts** (a key cycles them; the default fits the font):
  - **Code** for mono and Nerd Fonts: Rust, Python, TypeScript, Go, C, Lua
    and shell (another key cycles the language).
  - **Icons** for Nerd Fonts: a prompt line, a file tree with devicons, git
    status, a status bar.
  - **Prose** for sans, serif, display and handwriting: chapter 1 of
    *Frankenstein* (Project Gutenberg, public domain), with paging.
  - **README** for any font: a Markdown-style document (headings, lists, a
    code block, a table).
- **Size:** `+` / `-` change the preview size.
- **Keys:** `t` the next text, `L` the next language, `<` / `>` pages of
  prose, `p` fetches a Nerd Font. Code and icons are cut at the pane's
  edge like a terminal; prose and the README wrap.

### 5. Font tournament (after codingfont.com)

- **How codingfont.com does it:** 28 Google coding fonts, shuffled into a
  single-elimination bracket (an odd one out gets a bye). Each match shows
  the same code twice, names hidden unless "Show name" is on; you click
  the one you prefer; there are theme, size and ligature options; the
  winner gets a certificate. There's no ranking beyond the winner.
- **Here:** a fourth tab on the Fonts page, **Tournament**, next to
  Google Fonts, Nerd Fonts and Installed.
- **The pool:** you pick a category, and every font in it takes part:
  Google's monospace, sans-serif, serif, display or handwriting, Nerd
  Fonts, or installed fonts (monospaced or proportional).
- **Optional pool size:** all of them (the default), or 16, 32 or 64
  picked at random from the category.
- **Before it starts:** a warning with what it takes. The number of fonts,
  the picks it will need (one fewer than the fonts), and the preview
  download (estimated for Google fonts at ~100 KB each; the real sizes for
  Nerd Font archives, which add up to over a gigabyte for all 72; nothing
  for installed fonts). It starts only once you confirm. Then the preview
  files are fetched (with progress), so matches render instantly; ones
  already cached aren't fetched again.
- **A match:** two previews side by side (stacked when narrow), the same
  text and size; names hidden until you pick (a key shows them). The text
  fits the category (code in a language you pick for monospace and Nerd
  Fonts, the *Frankenstein* passage otherwise) and can be switched during
  the tournament. Pick with
  ←/→ (h/l) or a click; `u` takes the last pick back. A status line shows
  the round, the match and how many fonts are left.
- **The bracket:** `b` shows the rounds so far and who won each match.
- **The end:** the top 10, grouped by the round they reached: 1st, 2nd,
  the semi-finalists, the quarter-finalists, and two of the round-of-16
  losers chosen at random (labelled as groups, since a bracket doesn't
  order them). Each is listed with its preview; for each: install it (if
  needed), use it for mono, ui or bar, or fill every role from its family.
- **Results are kept** in `~/.local/state/theme-studio/tournaments/`, so a
  past result can be reopened.

### Progress

- ✅ Steps 1–3 (commits 1bc5ccf, 76a9954, ec48d38; configs cb22e1f and
  the keymaps / settings commits after it). Your first theme, "CRT
  Amber", was made from the active palette and fonts; `$mod+t` opens
  themes, `$mod+p` palettes.
- ⏳ Step 4, the tournament, comes after Phases 7b and 7c (it reuses 7b's
  download work).

### 6. Order of work

1. ✅ Themes: the data, migration, the studio's titles and Save / Open, the
   scripts and rofi.
2. ✅ Role picking with filters and applying at once; filling from a family.
3. ✅ Preview fetching and rendering, the texts, the preview pane.
4. The tournament (after Phase 7b).

## Phase 7b: changes asked for after Phase 7 step 3

Settled at the start of the session after Phase 7 step 3 (2026-11-17).
Built before the tournament (Phase 7 step 4), which reuses item 4's
download work.

### 0. Messages ✅ (asked for before starting 7b)

- **Info messages** leave the status line after a set time (Behaviour tab,
  Messages group: 1, 2, 3 (default), 5 or 10 seconds, or until a key).
  Errors still stay until a key.
- **Save confirmations** are short ("Saved" for the settings; `Saved
  "Work"`, `Saved and applied "Work"` for themes, palettes and font
  presets) and show either in the status line (default) or in a box that a
  key or a click closes. Smaller notes ("Nothing to undo") stay in the bar.
- settings.json: `"messages": {"seconds": 3, "saved": "bar" | "popup"}`.

### 1. Palette dropdown in the Palette pane ✅

(Revised in Phase 7c item 2: the dropdown moves into the pane, as its first
row.)

- **What:** ✅ the pane title is the dropdown:
  `⁴Palette: CRT Amber ▾` (the name comes back into the title, removed in
  Phase 7 step 1). Enter on the title or a click opens it: every palette
  in palettes.json, with swatches and type-to-search, in a list hanging
  from the pane (not a centred pop-up). No extra row is taken from the
  colour list.
- **Live preview:** ✅ moving through the list repaints the preview windows
  and the colour list with that palette, the way hover colours already
  preview. Enter keeps it; Esc goes back to the palette you had.
- **Choosing** swaps that palette into the theme, as `<leader>op` does
  (asks first if the current palette has unsaved edits).

### 2. Draw resized windows after resizing stops ✅

- **Now:** every resize step redraws everything, including the font
  sample and preview images (ImageMagick renders), so dragging a border
  or resizing the terminal stutters.
- **Becomes:** while the size is changing (terminal resize events, border
  drags, resize mode keys) the frame and text redraw, but images are
  re-rendered only once the size has stayed the same for ✅ **150 ms**;
  meanwhile the old image is shown cropped, or a placeholder.
- **Embedded terminals** (nvim, shell/yazi): ✅ resize at once. A PTY
  resize is cheap and nvim redraws on its own, so they don't wait.

### 3. Fuller code previews, and dropdowns ✅

- **More content:** each language's sample grows to a realistic file
  (~40–60 lines: imports, a type, a few functions, comments, strings,
  numbers, error handling), so a font is seen on real code.
- **Scrolling:** `<` / `>` (or the wheel over the preview) page through
  code as they do through prose.
- **Dropdowns:** ✅ the preview's title row gets two: the text kind and the
  language (`Code ▾  Rust ▾`); Enter or a click lists the choices. `t`
  and `L` still cycle them from the keyboard. The language dropdown shows
  only for code.
- **More languages:** ✅ Java, Kotlin, Ruby, Haskell, SQL and HTML/CSS, on
  top of Rust, Python, TypeScript, Go, C, Lua and shell. (Nix wasn't
  picked.)

### 4. Prefetching previews ✅

- **A setting** (Behaviour tab, Fonts page group): prefetch previews on or
  off, and which categories (monospace, sans-serif, serif, display,
  handwriting, Nerd Fonts), each shown with its count, the download size
  and the space kept (e.g. monospace 72 fonts ≈ 7 MB; Nerd Fonts 72
  archives ≈ 1.5 GB to download, a few hundred MB kept).
- **How:** in the background, a few downloads at a time, with progress in
  the status line; already-fetched fonts are skipped; it resumes after a
  restart.
- **Where:** ✅ moved from `~/.cache/theme-studio/preview/` to
  `~/.local/share/theme-studio/previews/`, so cache cleaners don't undo a
  prefetch or a tournament's downloads. Files already in the old folder
  are moved on first start.
- **Clear previews:** ✅ a button in the same Behaviour group, showing the
  space used.
- **Nerd Fonts:** ✅ only the Regular file is kept once the archive is
  unpacked; the archive is deleted. Installing the font later downloads
  the archive again.
- **Ties in with the tournament,** whose pool needs the same files.

### 5. Key hints at the other end of the bar ✅

- **Now:** the status line runs page tabs, theme name, then the hints.
- **Becomes:** tabs and theme name at the left, the key hints
  right-aligned at the right end; messages take the middle.
- **Tab bar on top:** ✅ the hints stay in the bottom line. Top row: tabs
  and theme name. Bottom row: messages at the left, hints at the right.

### 6. Order of work

Each step is committed separately and keeps the studio working.

1. ✅ Key hints at the right end of the bar. With a message showing, the
   hints that don't fit beside it are dropped (before, a message hid them
   all).
2. ✅ Images re-rendered after resizing stops (`settle.rs`): while waiting,
   an image keeps its old size where that fits, else the pane is blank.
3. ✅ The palette dropdown in the pane title, with live preview. A click on
   the name or `<leader>op` opens it (the list has no keyboard focus of its
   own); off the Main page it's centred as before.
4. ✅ Fuller code samples (51–66 lines, in `src/samples/code/`), the new
   languages, scrolling (`<` / `>` a page, the wheel three lines), the two
   dropdowns (opened with a click; `t` and `L` still cycle).
5. ✅ Previews moved to `~/.local/share` (moved over at start, each pruned
   to its Regular), Nerd Fonts kept as Regular only, the clear button
   (asks first), then prefetching (`prefetch.rs`: three at a time,
   "fetching previews 12/72" in the status line when no message shows;
   off by default). The Behaviour tab group is "Fonts page previews".
6. Then Phase 7c, then the tournament (Phase 7 step 4).

## Phase 7c: after trying 7b (before the tournament)

Asked for on 2026-11-17 after Phase 7b. Defaults I picked are marked 🔸.

### 1. Installed fonts get a preview

- **Now:** the Fonts page's Installed tab says "the main page's sample
  shows it" and draws nothing.
- **Becomes:** the highlighted installed font is previewed like the others,
  from its files in `~/.local/share/fonts/theme-studio/<id>/` (nothing to
  fetch). Its family name comes from `fc-scan` on a background thread.
- **Default text:** icons for Nerd Fonts, code for monospaced fonts, prose
  for the rest (spacing from `fc-scan`, since the list doesn't keep a
  category).

### 2. The palette dropdown inside the pane, reached by keyboard

- **Revises 7b step 3.** The title shows the name, not a dropdown:
  `⁴Palette: CRT Amber ●`. The dropdown is the pane's first row,
  `CRT Amber ▾`, above the colour groups.
- **Keyboard:** Up from the first colour row moves onto the dropdown row;
  Enter (or Right) opens the list under it, inside the pane, with the live
  preview as now. Down goes back to the colours. `<leader>op` still opens
  it from anywhere.
- **Mouse:** a click on the row opens it (the title is no longer a target
  beyond focusing the pane).

### 3. Moving the keyboard up to the page tabs

- **Like the Settings page's tabs:** Up at the top of a pane in the top
  row moves the keyboard to the page tab bar (F1 Main, F2 Fonts, F3
  Settings) when the tab bar is on top. 🔸 When the tab bar is at the
  bottom, Down at the bottom of a pane in the bottom row does the same.
- **On a page with its own tabs** (Fonts: Google / Nerd / Installed;
  Settings: Keymaps / Behaviour / History) the page's tabs come first:
  Up from the list goes to the page's tabs, Up again to the page tabs.
- **In the tab bar:** Left / Right switch pages at once and the keyboard
  stays on the tab bar; Down (Up when at the bottom) or Enter goes back
  into the page, to the pane it came from. The focused tab is drawn like
  the Settings page's focused tabs.

### 4. Faster font previews (150 ms or less)

- **Target:** a fetched font's preview shows within 150 ms of
  highlighting it (render, decode and the image on screen), measured on
  this machine.
- **Measured now** (Rust sample, 1004×1202): ImageMagick takes 100 ms with
  its 16-bit PNG output and the system fontconfig, 150 ms with the preview
  fontconfig. Starting magick is 10 ms; the same render to 8-bit BMP or
  raw RGB is 40 ms; the preview fontconfig adds about 50 ms (it scans the
  whole preview folder). Decoding the PNG and sending the image to Ghostty
  happen on the main thread on top of that.
- **Changes, in order (stopping once under target):**
  1. Render to 8-bit BMP (or raw RGB) instead of 16-bit PNG.
  2. A fontconfig file per previewed font, listing only that font's folder
     plus the system configuration, instead of the whole preview folder.
  3. Render only what the pane shows: the visible lines of code, at the
     pane's width, not the whole 60-line file.
  4. Decode and encode the image for the terminal off the main thread
     (ratatui-image's thread protocol), so the studio stays responsive
     while it's on its way.
  5. Keep the last few rendered images (per font, text, size, page and
     width), so going back to a font is instant.
  6. Shorten the pause before fetching an unfetched font from 300 ms to
     150 ms (it only stops fetching every font the cursor passes).
- **If that isn't enough:** draw previews in the studio itself with a
  Rust text renderer (cosmic-text) instead of ImageMagick: a few
  milliseconds per image, but a larger change, and Pango's look and
  fallbacks would need matching. Only with your go-ahead.
- **The main page's font sample** benefits from 1, 3, 4 and 5 too.
- **A timing switch:** `THEME_STUDIO_TIMING=1` logs each preview's
  render and display time to `~/.local/state/theme-studio/timing.log`, to
  check the target.

### 5. Hiding the key hints

- **A setting** (Behaviour tab, Layout group): key hints shown or hidden
  (🔸 shown by default). Hidden, the bar keeps the tabs, the theme name and
  messages; `?` still lists the keys.
- 🔸 Also a key to toggle them for the session: `<leader>th`.

### 6. Hiding the preview, per page

- **Main page:** the preview column (three columns) or the preview row
  (narrow layout) can be hidden; the fonts and colour columns then share
  the space. nvim and the shell keep running while hidden.
- **Fonts page:** the preview pane can be hidden; the list and details
  take its space.
- **Settings** (Behaviour tab, Layout group): "Main page preview at start"
  and "Fonts page preview at start", shown or hidden, separately.
- **Key:** 🔸 `<leader>tp` toggles the preview of the page you're on
  (each page's state is its own). The other presets get one in their
  style; all rebindable on the Keymaps tab. Like the picker, the state
  isn't remembered between runs (the settings say how it starts).

### 7. Prefetching that doesn't slow anything down

- **Already:** each download runs on its own thread, three at a time.
- **Still on the main thread, to move off it:**
  - Building the queue: checking which of ~1,700 fonts are already
    fetched (a folder read each). Done on a background thread.
  - The Behaviour tab's "space used": it walks the preview folder on every
    draw. Measured on a thread, at most every few seconds.
  - Redraws: every download progress message (Nerd Font archives send
    many) redraws the whole screen. Only progress shown on screen
    triggers a redraw, at most ~10 times a second.
- **Lower priority for the heavy work:** curl, tar (xz unpacking is CPU
  heavy), fc-scan and ImageMagick run under `nice`, so the studio, nvim
  and the shell come first.
- 🔸 **Nerd Font archives one at a time** (Google files three at a time).
- **A check:** a test with a fake catalogue of 2,000 fonts and stubbed
  downloads asserts the main loop's work per tick stays small.

### 8. Row highlights cover the text only

- **Now:** the keyboard cursor reverses the whole row, padding included;
  on the Behaviour tab it hides the colour swatches (a reversed `██` draws
  in the background colour).
- **Becomes:** the cursor's highlight covers each piece of text in the
  row (the label, the value) but not the padding between them, and never
  a swatch: swatches keep their colour.
- **Everywhere a row cursor shows:** the Behaviour and Keymaps tabs, the
  History tab's lists, the colour list, the font list and roles, the
  Fonts page's lists, the Open and palette lists and the dropdowns.
- **The unfocused cursor** (bold, underlined) follows the same rule.

### 9. Fixes found along the way

- **Tests write to your home folder:** the preview tests write
  `~/.cache/theme-studio/preview-fonts.conf`. They'll use a temporary
  folder instead.
- **Until you rebuild,** the installed studio's previews may use the wrong
  font: a test run already rewrote that file to point at the new
  `~/.local/share/theme-studio/previews/`. `nrs` fixes it, and the first
  start moves the old previews there.

### 10. Order of work

Each step is committed separately and keeps the studio working.

1. ✅ Fixes (9), the key hints setting (5), the preview toggles (6).
   `<leader>6` / `<leader>7` show a hidden Main preview again.
2. ✅ Row highlights (8).
3. ✅ The palette dropdown row (2) and moving up to the tabs (3). The row
   also shows the palette's base colours. Esc on the row goes back to the
   colours too.
4. Installed fonts' preview (1).
5. Faster previews (4), measured after each change.
6. Prefetching off the main thread (7).
7. Then the tournament (Phase 7 step 4).

## Phase 8: polish

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
5. Phase 7 steps 1–3, Phases 7b and 7c, then Phase 7 step 4 (the tournament).

Each step is committed separately in the repo it belongs to (`configs/` or
the outer repo).
