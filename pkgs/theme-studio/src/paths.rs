//! Where things are. Four kinds of place, kept apart so the studio doesn't
//! depend on one dotfiles layout (docs/theme-studio-migration.md):
//!
//! - the **data folder**: your palettes.json, fonts.json and themes.json
//!   (`$THEME_STUDIO_DATA`, else `$XDG_CONFIG_HOME/theme-studio` once it
//!   holds a palettes.json);
//! - the **share folder**: files the studio and scripts need, such as
//!   terminal-colors.json (`$THEME_STUDIO_SHARE`, set by the package);
//! - the **switch scripts**: commands on PATH (`theme-switch`,
//!   `font-switch`), else the scripts in the share folder's project;
//! - the **desktop configs** the preview reads (i3, rofi, i3blocks, nvim):
//!   `$XDG_CONFIG_HOME`, as on any distro.
//!
//! While the move is under way each of them falls back to the old place,
//! the dotfiles' configs folder (`$THEME_STUDIO_CONFIGS`, default
//! `~/dotfiles-nixos/configs`), so old and new layouts both work.

use std::path::{Path, PathBuf};
use std::process::Command;

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `$XDG_CONFIG_HOME`, default `~/.config`.
pub fn config_home() -> PathBuf {
    env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home().join(".config"))
}

/// The old place for everything: the dotfiles' configs folder.
pub fn legacy_configs() -> PathBuf {
    env_dir("THEME_STUDIO_CONFIGS").unwrap_or_else(|| home().join("dotfiles-nixos/configs"))
}

/// Your palettes, font presets and themes.
pub fn data_dir() -> PathBuf {
    if let Some(d) = env_dir("THEME_STUDIO_DATA") {
        return d;
    }
    let xdg = config_home().join("theme-studio");
    if xdg.join("palettes.json").exists() {
        return xdg;
    }
    legacy_configs()
}

/// Files the studio and the scripts need (terminal-colors.json, …).
pub fn share_dir() -> PathBuf {
    env_dir("THEME_STUDIO_SHARE").unwrap_or_else(legacy_configs)
}

/// A file in the share folder. Its name there, and (`legacy`) its name in
/// the old configs folder, where it may still be.
pub fn share_file(share: &Path, name: &str, legacy: &str) -> PathBuf {
    let p = share.join(name);
    if p.exists() {
        return p;
    }
    let old = share.join(legacy);
    if old.exists() {
        return old;
    }
    legacy_configs().join(legacy)
}

/// Where the desktop's configs are read from for the preview: the XDG
/// config folder when it has them, else the old configs folder.
pub fn desktop_dir() -> PathBuf {
    let xdg = config_home();
    if xdg.join("i3/theme").exists() {
        xdg
    } else {
        legacy_configs()
    }
}

/// Where the preview's demo shell starts (it shows `git log`): the
/// dotfiles repo when it's there, else the home folder.
pub fn demo_dir() -> PathBuf {
    legacy_configs()
        .parent()
        .filter(|p| p.join(".git").exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(home)
}

/// The studio's guide (the README tab), found from the share folder: in
/// the project, `docs/` beside `share/`; in the old layout (the share
/// folder is the dotfiles' configs folder), the dotfiles' `docs/` or, once
/// the project is there, `theme-studio/docs/`.
pub fn guide(share: &Path) -> PathBuf {
    let candidates: Vec<PathBuf> = ["docs/theme-studio.md", "theme-studio/docs/theme-studio.md"]
        .iter()
        .filter_map(|rel| share.parent().map(|p| p.join(rel)))
        .collect();
    candidates
        .iter()
        .find(|p| p.exists())
        .cloned()
        .unwrap_or_else(|| candidates[0].clone())
}

/// Whether a command is on PATH.
fn on_path(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|d| {
            let p = d.join(name);
            p.is_file()
                && std::fs::metadata(&p)
                    .map(|m| std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0)
                    .unwrap_or(false)
        })
    })
}

/// A switch script (`theme-switch`, `font-switch`, `themes-switch`) ready
/// to run: the command on PATH, else `bash <script>.sh` from the project's
/// scripts folder, else from the old configs folder. It's told where the
/// data and share folders are, so it reads what the studio reads.
pub fn switch_command(name: &str, data: &Path, share: &Path) -> Command {
    let mut cmd = if on_path(name) {
        Command::new(name)
    } else {
        let file = format!("{name}.sh");
        let script = [
            share.parent().map(|p| p.join("scripts").join(&file)),
            Some(legacy_configs().join(&file)),
        ]
        .into_iter()
        .flatten()
        .find(|p| p.exists())
        .unwrap_or_else(|| legacy_configs().join(&file));
        let mut c = Command::new("bash");
        c.arg(script);
        c
    };
    cmd.env("THEME_STUDIO_DATA", data);
    if share.exists() {
        cmd.env("THEME_STUDIO_SHARE", share);
    }
    cmd
}

/// A file to write: the file a symlink points at, so saving (a temporary
/// file renamed over the target) replaces the file, not the link.
pub fn write_target(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(p) => p,
        // a link to a file that isn't there yet: write where it points
        Err(_) => match std::fs::read_link(path) {
            Ok(t) if t.is_absolute() => t,
            Ok(t) => path.parent().map(|d| d.join(&t)).unwrap_or(t),
            Err(_) => path.to_path_buf(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("theme-studio-paths-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn saving_through_a_link_writes_its_target() {
        let d = temp("link");
        std::fs::create_dir_all(d.join("new")).unwrap();
        std::fs::write(d.join("new/palettes.json"), "{}\n").unwrap();
        std::os::unix::fs::symlink("new/palettes.json", d.join("palettes.json")).unwrap();
        assert_eq!(
            write_target(&d.join("palettes.json")),
            std::fs::canonicalize(d.join("new/palettes.json")).unwrap()
        );
        // the studio's save: through the link, which stays a link
        crate::store::write_json_file(&d.join("palettes.json"), &serde_json::json!({"a": 1}))
            .unwrap();
        assert!(
            std::fs::symlink_metadata(d.join("palettes.json"))
                .unwrap()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(d.join("new/palettes.json")).unwrap(),
            "{\n  \"a\": 1\n}\n"
        );
        // a plain file is written in place
        assert_eq!(write_target(&d.join("new/x.json")), d.join("new/x.json"));
        std::fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn share_files_fall_back_to_their_old_names() {
        let d = temp("share");
        std::fs::create_dir_all(d.join("pywalfox")).unwrap();
        std::fs::write(d.join("pywalfox/colors.jq"), "").unwrap();
        assert_eq!(
            share_file(&d, "pywalfox-colors.jq", "pywalfox/colors.jq"),
            d.join("pywalfox/colors.jq")
        );
        std::fs::write(d.join("pywalfox-colors.jq"), "").unwrap();
        assert_eq!(
            share_file(&d, "pywalfox-colors.jq", "pywalfox/colors.jq"),
            d.join("pywalfox-colors.jq")
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}
