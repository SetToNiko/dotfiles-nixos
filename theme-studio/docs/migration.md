# Theme Studio: moving it into its own project

A plan for gathering the studio, the scripts it uses and the files they
need into one folder, laid out as a project of its own, so it can later be
split into its own repo and refactored to work on other Linux distros.
This plan covers **the move only**; what the later refactor has to deal
with is listed at the end.

Decisions you made are marked ✅. Smaller defaults I picked are marked 🔸.

---

## Decisions

|                    | Choice                                                                                                                                         |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| ✅ Goal            | A project that doesn't depend on NixOS or on these dotfiles; NixOS becomes one way of packaging it                                             |
| ✅ Location        | `~/dotfiles-nixos/theme-studio/`, tracked by the main repo, laid out as its own repo (own `flake.nix`, nothing reaching outside it); split later with `git subtree split` |
| ✅ Your data       | Stays in the configs repo, in `configs/theme-studio/` next to `keymaps.json` (already linked to `~/.config/theme-studio`)                       |
| ✅ Nix parts       | In the project's `packaging/nix/`, as a home-manager module and a NixOS module the dotfiles import                                             |
| ✅ Scripts         | Installed as commands (`theme-switch`, `themes-switch`, `font-switch`); a `live` option runs them from the checkout so edits apply without a rebuild |
| ✅ Fonts list      | `home/fonts-installed.json` stays in the dotfiles; the module takes its path as an option                                                      |
| ✅ Scope           | The move, in steps that each keep everything working, plus an inventory for the refactor                                                       |

## What moves where

```
theme-studio/                       the project (main repo, for now)
  flake.nix                         standalone use: package, modules, dev shell
  README.md                         what it is, how to build and install
  studio/                           the Rust crate          ← pkgs/theme-studio/
  scripts/
    theme-switch.sh                                         ← configs/theme-switch.sh
    themes-switch.sh                                        ← configs/themes-switch.sh
    font-switch.sh                                          ← configs/font-switch.sh
    lib/rofi-picker.sh                                      ← configs/lib/rofi-picker.sh
  share/
    palette-derive.jq                                       ← configs/palette-derive.jq
    terminal-colors.json                                    ← configs/terminal-colors.json
    pywalfox-colors.jq                                      ← configs/pywalfox/colors.jq
  docs/
    theme-studio.md                 the guide (README tab)  ← docs/theme-studio.md
    plan.md, migration.md                                   ← docs/theme-studio-plan.md, this file
  packaging/nix/
    package.nix                     the package, scripts installed as commands
    home-module.nix                 commands (live or not), activation, links
    fonts.nix                       the downloaded-fonts list → Nix  ← home/fonts.nix
    nixos-module.nix                the login screen font            ← part of modules/ascii-boot.nix

configs/theme-studio/               your data (configs repo)
  palettes.json                                             ← configs/palettes.json
  fonts.json                                                ← configs/fonts.json
  themes.json                                               ← configs/themes.json
  keymaps.json, settings.json       (already here)

stays where it is
  configs/hosts/<host>/fonts.json   per-machine size offsets (host-specific, read via ~/.config/host)
  home/fonts-installed.json         downloaded-fonts list (must be readable by the flake)
  ~/.config/colors/, ~/.config/fonts/   the generated files every app reads: unchanged
  ~/.local/state/theme-studio/, ~/.local/share/theme-studio/, ~/.local/share/fonts/theme-studio/
  modules/ascii-boot.nix            the boot animation and greeter look (the font part moves)
```

🔸 Names: `studio/` for the crate (a `crates/` folder can come later if it
grows more), `share/` for files the scripts read, command names without
`.sh`.

## What doesn't change

- **Everything the apps read** (`~/.config/colors/*`, `~/.config/fonts/*`,
  GTK and fontconfig files, `/var/lib/ascii-boot/greeter.conf`): same
  paths, same contents. Ghostty, i3, rofi, nvim, the lock and login screens
  need no edits.
- **The history** (`~/.local/state/theme-studio/history/<file>/`) is filed
  by file name, so `palettes.json`'s history carries on after it moves.
- **File formats**: no version bumps.

## Where things are found (the core of the move)

Today one folder (`THEME_STUDIO_CONFIGS`, default `~/dotfiles-nixos/configs`)
is used for four different things. They get separated **before anything
moves**:

| What                                   | Today                                  | After                                                                                                                 |
| -------------------------------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| Your data (palettes, fonts, themes)    | the configs folder                     | the **data folder**: `$THEME_STUDIO_DATA`, else `$XDG_CONFIG_HOME/theme-studio` (here: `configs/theme-studio`)        |
| Files the scripts and studio need      | the configs folder                     | the **share folder**: `$THEME_STUDIO_SHARE`, set by the package (or, in live mode, the checkout)                      |
| The switch scripts                     | `configs/*.sh`, run with bash          | the **commands** on PATH; the studio calls them by name                                                               |
| Desktop configs the preview reads (`i3/theme`, `rofi/config.rasi`, `i3blocks/config`, `nvim/lua/theme.lua`) | the configs folder | `$XDG_CONFIG_HOME/…` (here: links into `configs/`), as on any distro                     |

During the move every lookup also falls back to the old place
(`~/dotfiles-nixos/configs/…`), so whatever step you're on, both old and
new layouts work. The fallbacks are removed in the last step.

## Risks and how each is handled

1. **Saving over a link.** The studio saves by writing a temporary file and
   renaming it over the target. Renaming over a symlink replaces the link
   with a plain file, so after the move a save to an old path would split
   your data in two. → Step 1 makes the studio resolve links before every
   write (and tests it). The rofi menus' delete already writes through the
   link (`cat > file`), so it's safe.
2. **A studio left running from before a step** still uses the old paths
   and the old save code. → Quit the studio before steps 2 and 4 (in the
   checklist).
3. **Untracked files are invisible to the flake.** A new file under
   `theme-studio/` that isn't `git add`ed isn't there for `nrs`. → Each step
   adds its files before building.
4. **Commands not installed yet.** If i3 called `theme-switch` before an
   `nrs` installed it, the keys would do nothing. → i3 and the activation
   steps keep calling the old paths (now links) until a step after the one
   that installs the commands, and that change is only made after `nrs`.
5. **The activation steps' PATH.** home-manager runs them with a small fixed
   PATH (the reason Ghostty was added in Phase 0). → The commands carry
   their own dependencies (jq, fontconfig, ghostty, rofi, coreutils) in the
   package's wrapper, so the activation no longer needs to set PATH.
6. **Two repos.** Scripts and share files move from the configs repo to the
   main repo; git can't follow a move between repos. → Their history stays
   in the configs repo (the commit that removes them names the main repo
   commit that adds them). The crate and docs move inside the main repo with
   `git mv`, history kept.
7. **Login screen.** The greeter's font is written by `font-switch.sh` from
   `/etc/ascii-boot/greeter.conf.in`. → The paths stay exactly as they are
   in this move; only which Nix file declares them changes. Renaming them
   is in the refactor list.

## Steps

Each step is one commit per repo it touches, leaves everything working,
and can be reverted on its own (in reverse order). "Check" lists what to
try before going on; `nrs` checkpoints are yours to run.

### 1. The code learns the new places (nothing moves)

- **Studio:**
  - data folder: `$THEME_STUDIO_DATA` → `$XDG_CONFIG_HOME/theme-studio` if
    it has a `palettes.json` → the old `THEME_STUDIO_CONFIGS` /
    `~/dotfiles-nixos/configs`;
  - share folder: `$THEME_STUDIO_SHARE` → the old configs folder;
  - desktop configs from `$XDG_CONFIG_HOME` (falling back to the configs
    folder);
  - the switch scripts: the command on PATH if there is one, else the old
    `configs/*.sh`;
  - the guide for the README tab: `$THEME_STUDIO_SHARE/../docs` → the old
    `docs/theme-studio.md`;
  - **saves resolve symlinks first** (risk 1);
  - `--help` lists the new variables; tests cover each fallback, and that
    a save through a link updates the link's target.
- **Scripts:** the same data-folder lookup
  (`THEME_STUDIO_DATA` → `~/.config/theme-studio/<file>` if it exists →
  next to the script, as now). `PALETTES` / `FONTS` / `THEMES` still win.
  Share files are found through `THEME_STUDIO_SHARE`, else in `../share`
  beside the script's folder if that exists (after step 4), else next to
  the script (as now).
- **Check:** `cargo test`; every lookup lands on today's files (nothing has
  moved, so the fallbacks are what's used); `$mod+t`, `$mod+p`,
  `$mod+Shift+t`; **`nrs`** (added while building: the *installed* studio
  must have the link-safe saving before step 2, not just the code); the
  studio opens, saves and applies a palette.

**Step 1 as built ✅** (main repo: `paths.rs`; configs: `lib/paths.sh`):

- The studio's `Store` has three folders: `dir` (data), `share`, `desktop`;
  the preview reads `~/.config/{i3,rofi,i3blocks,nvim}` (links into
  `configs/` here), and its demo shell starts in the dotfiles repo (else
  home). The guide is found from the share folder only.
- The studio runs the scripts with `THEME_STUDIO_DATA` and
  `THEME_STUDIO_SHARE` set to its own folders, so both always read the same
  files.
- Checked: the scripts' output (`~/.config/colors`, `~/.config/fonts`,
  Pywalfox's `colors.json`, the greeter config) is byte-for-byte the same,
  run plainly, through `themes-switch.sh`, and the way the studio calls them.
- **Found and fixed on the way:** `font-switch.sh` deleted the login screen's
  font copy before copying it. Its folder is a fontconfig folder since
  Phase 7d, so `fc-match` returned the copy itself; the folder ended up
  empty and the login screen would have fallen back to `monospace`. It now
  takes the original file (`fc-match -s`, skipping the copy), keeps the copy
  when it's already the right font, and copies before removing old files.

### 2. Your data moves into `configs/theme-studio/` (configs repo)

- Needs step 1's `nrs` first, and the studio quit (risk 2).
- `palettes.json`, `fonts.json`, `themes.json` move into
  `configs/theme-studio/` (`git mv`, history kept); each old path becomes a
  relative link to the new one, for anything that still uses it.
- Both the studio and the scripts now find them through
  `~/.config/theme-studio/` (the link home-manager already makes); no
  rebuild needed.
- **Check:** the three pickers show every palette, preset and theme; the
  studio opens the active theme; saving a palette changes
  `configs/theme-studio/palettes.json` and **the old link is still a link**
  (`ls -l configs/palettes.json`); the History tab still lists the earlier
  versions.

**Step 2 as built ✅** (configs `06d128e`): moved after step 1's `nrs`
(the installed studio checked for the new code) with no studio running;
the files were backed up and checked by hash after the move; the scripts
read `~/.config/theme-studio/…` and their output was byte-for-byte the
same; the history folders are by name, so they carry on. Your uncommitted
edits to `palettes.json` and `fonts.json` came along as uncommitted edits
at the new paths. Git records the old paths as a change of type (file →
link), not a rename, because the old paths still exist: the files' earlier
history is under the old names (`git log -- palettes.json`).

### 3. The project folder and the crate (main repo)

- `git mv pkgs/theme-studio theme-studio/studio`; `theme-studio/README.md`
  and a standalone `theme-studio/flake.nix` (package and dev shell for
  now).
- 🔸 The dotfiles use it **without a flake input**: `flake.nix` and
  `home/base.nix` call `./theme-studio/packaging/nix/package.nix` directly.
  A flake input would need relocking after every change; it comes at the
  split.
- `docs/theme-studio.md`, `docs/theme-studio-plan.md` and this file `git mv`
  into `theme-studio/docs/`; `docs/` keeps a one-line pointer.
- **Check:** `nix build .#theme-studio` gives the same program; `cargo test`
  from `theme-studio/studio`; **`nrs`**; the studio's README tab still shows
  the guide.

**Step 3 as built ✅:**

- The sources are the old ones apart from two lines naming the docs' new
  paths (checked with `diff -r` against the last commit); `Cargo.toml` and
  `Cargo.lock` are unchanged. 112 tests pass from `theme-studio/studio`.
- `docs/` is gone rather than keeping a pointer: the README links to the new
  place, and a `docs/theme-studio.md` left behind would be what the studio's
  README tab finds first (`paths::guide`).
- The project's own `flake.nix` follows `nixos-unstable` and has its own
  `flake.lock`; the dotfiles use `master`. They build with different
  toolchains, which only matters for `nix develop ./theme-studio` (a
  separate download); `nix develop .#theme-studio` from the dotfiles uses
  theirs.

### 4. Scripts and share files move (both repos)

- Quit the studio first.
- `theme-switch.sh`, `themes-switch.sh`, `font-switch.sh`,
  `lib/rofi-picker.sh` → `theme-studio/scripts/`; `palette-derive.jq`,
  `terminal-colors.json`, `pywalfox/colors.jq` → `theme-studio/share/`.
- Each old path in `configs/` becomes a relative link into the project
  (`configs/theme-switch.sh → ../theme-studio/scripts/theme-switch.sh`), so
  i3's keys and the activation steps keep working untouched. The scripts
  resolve their own path (`readlink -f`), so they find `lib/` and `share/`
  in the project.
- The package installs them as `theme-switch`, `themes-switch` and
  `font-switch` (with their dependencies), and sets `THEME_STUDIO_SHARE`
  for the studio.
- **Check:** **`nrs`**; the three i3 keys (still through the links); the
  commands by name (`theme-switch Gruvbox Dark`); the studio applies a
  theme (now through the command); `theme-switch.sh` from a shell.

**Step 4 as built ✅:**

- `scripts/` holds the three scripts and `lib/` (`rofi-picker.sh`,
  `paths.sh`); `share/` holds `palette-derive.jq`, `terminal-colors.json`
  and `pywalfox-colors.jq`. `configs/` keeps relative links at all eight old
  paths (`configs/pywalfox/colors.jq` too, since home-manager links that
  folder to `~/.config/pywalfox`).
- The package is two parts: the Rust build (`passthru.bin`; its source
  didn't change, so it's the same store path as before) and a wrapper
  derivation. `$out/share/theme-studio/` mirrors the project (`scripts/`,
  `share/`, `docs/theme-studio.md`); `$out/bin/theme-switch`,
  `themes-switch` and `font-switch` run `bash` on
  `${THEME_STUDIO_SCRIPTS:-$out/share/theme-studio/scripts}/<name>.sh`
  (step 5's live mode sets `THEME_STUDIO_SCRIPTS` to the checkout), with
  `THEME_STUDIO_SHARE` defaulting to the package's `share/`. Their PATH gets
  coreutils, sed, awk, grep, procps, jq and fontconfig first, and rofi,
  ghostty and i3 last (the desktop's own win).
- The studio is wrapped with the same `THEME_STUDIO_SHARE` and the package's
  `bin/` on PATH, so it calls the commands. Its README tab now reads the
  package's copy of the guide; edits show again with live mode (step 5).
- Checked before `nrs`: the scripts through the old links, from the project,
  and the built commands (with a bare PATH, and with
  `THEME_STUDIO_SCRIPTS` pointing at the checkout) all give byte-for-byte
  the same output, the login font copy included.

### 5. The Nix modules (main repo)

- `packaging/nix/home-module.nix`, options under `programs.theme-studio`:
  - `enable`, `package`;
  - `live` (🔸 a path; for you `~/dotfiles-nixos/theme-studio`): the
    commands run the scripts from there and `THEME_STUDIO_SHARE` points
    there, so edits apply without a rebuild (as `liveScript` does now);
  - `dataDir` (🔸 for you `configs/theme-studio`, linked to
    `~/.config/theme-studio` as now);
  - `fontsList` (for you `home/fonts-installed.json`) and the fonts that
    come from it (`home/fonts.nix` moves here);
  - the two activation steps (regenerate colours and fonts on every
    switch), now calling the commands.
- `packaging/nix/nixos-module.nix`, `services.theme-studio.loginFont`:
  the greeter template, the font folder in the system fontconfig, and the
  `/etc` link, with the folder and template text as options (🔸 for you:
  `/var/lib/ascii-boot` and the template from `ascii-boot.nix`, so nothing
  changes on disk).
- `home/base.nix`, `home/fonts.nix` and `modules/ascii-boot.nix` shrink to
  importing and setting these.
- **Check:** `nix eval` of the generated files gives the same results as
  before (greeter config link, fontconfig `local.conf`, the font packages);
  **`nrs`**; log out and in (login font); `nrs` again with a different
  active theme to see the activation regenerate it.

### 6. Callers switch to the new names (configs repo)

- After step 5's `nrs`: i3's keys call `themes-switch`, `theme-switch`,
  `font-switch`; comments in `i3/config`, `nvim/lua/theme.lua`,
  `lock/lock-screen.sh` name the commands instead of the old paths.
- `settings.json`'s `fonts.installed-list` is unchanged (still
  `home/fonts-installed.json`).
- **Check:** the three keys; `grep` finds no caller of the old paths left
  except the links themselves.

### 7. The links and fallbacks go (both repos)

- After a few days of normal use: remove the old-path links in `configs/`,
  the fallbacks to `~/dotfiles-nixos/configs` in the studio and scripts,
  and `THEME_STUDIO_CONFIGS`.
- **Check:** `grep -r 'dotfiles-nixos/configs'` in `theme-studio/` finds
  nothing; the full checklist below; `cargo test`; **`nrs`**.

### Later: the split

`git subtree split --prefix=theme-studio` gives a repo with the project's
history; the dotfiles then take it as a flake input
(`inputs.theme-studio.url = "github:…"`) instead of the path. Nothing else
changes, since by then nothing reaches across.

## Checklist (after each step, as it applies)

1. `$mod+t`, `$mod+p`, `$mod+Shift+t`: menus list everything; choosing one
   applies it (Ghostty, i3 colours and bar font change).
2. Alt+BackSpace in a menu deletes an entry, and the data file is still in
   its place (no stray copy at an old path).
3. `$mod+Ctrl+t`: the studio opens the active theme; edit, save, apply.
4. Pick a font for a role: saved and applied at once.
5. Fonts page: a download installs and appears in
   `home/fonts-installed.json`.
6. History tab: earlier versions of `palettes.json` still listed.
7. README tab shows the guide.
8. `nrs` with no errors, and the colours and fonts still applied after it.
9. Log out: the login screen uses the Monospace font.

## For the refactor: what still ties it to this setup

Collected while planning; none of it changes in the move.

- **NixOS**
  - the downloaded-fonts list and `fonts.nix` (on other distros: fonts stay
    in `~/.local/share/fonts`, nothing to record);
  - the activation steps (elsewhere: run once after install, or on login);
  - the login screen's `/etc` template and fontconfig folder (elsewhere:
    an optional step that needs root);
  - `/var/lib/ascii-boot` and `/etc/ascii-boot/` named inside
    `font-switch.sh`;
  - the studio's tools (ImageMagick, fontconfig, nvim, yazi, git, curl,
    tar/xz) come from the user's PATH, not the package (needs a wrapper or
    documented dependencies).
- **i3**: `i3-msg restart` after a switch; the bar font file; the preview
  reads `i3/theme` and `i3blocks/config` and draws an i3 desktop.
- **Ghostty**: reloaded with SIGUSR2; theme names checked against
  Ghostty's theme folder; the terminal preview's mapping assumes its 16
  colours.
- **X11**: the screen DPI from `xrdb` (`Xft.dpi`); xsecurelock; LightDM's
  mini greeter.
- **rofi** for the three pickers; **Pywalfox** and **wallpaper-rotate**
  hooks inside `theme-switch.sh`.
- **The kitty graphics protocol** for font previews (half blocks
  otherwise).
- **App integrations are hard-coded** in the scripts (which files to write
  and how to reload each app); the refactor's main job is making them a
  list that can be configured per desktop.
- **Paths to these dotfiles**: `~/dotfiles-nixos/…` in settings
  (`fonts.installed-list`), docs and comments.
- **No starter data**: a new user would start without palettes; ship a few
  in `share/` and copy them to the data folder on first run.
- **Install without Nix**: a Makefile or `install.sh` (cargo build, copy
  scripts and share files, a desktop entry), then distro packages.
