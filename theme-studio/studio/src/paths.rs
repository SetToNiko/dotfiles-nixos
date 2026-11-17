//! Where things are. Four kinds of place, kept apart so the studio doesn't
//! depend on one layout:
//!
//! - the **data folder**: your palettes.json, fonts.json and themes.json
//!   (`$THEME_STUDIO_DATA`, else `$XDG_CONFIG_HOME/theme-studio`);
//! - the **share folder**: files the studio and scripts need, such as
//!   terminal-colors.json (`$THEME_STUDIO_SHARE`, set by the package; else
//!   the one installed beside the program, or the project's own when run
//!   from a checkout);
//! - the **switch scripts**: commands on PATH (`theme-switch`,
//!   `font-switch`), else the scripts beside the share folder;
//! - the **desktop configs** the preview reads (i3, rofi, i3blocks, nvim):
//!   `$XDG_CONFIG_HOME`.

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

/// Your palettes, font presets and themes.
pub fn data_dir() -> PathBuf {
    env_dir("THEME_STUDIO_DATA").unwrap_or_else(|| config_home().join("theme-studio"))
}

/// Files the studio and the scripts need (terminal-colors.json, …):
/// `$THEME_STUDIO_SHARE`; else `share/theme-studio/share` beside the
/// program's `bin/` (an install); else the project's `share/` (a build in a
/// checkout, e.g. `cargo run`).
pub fn share_dir() -> PathBuf {
    if let Some(d) = env_dir("THEME_STUDIO_SHARE") {
        return d;
    }
    let installed = std::env::current_exe().ok().and_then(|exe| {
        let d = exe.parent()?.parent()?.join("share/theme-studio/share");
        d.is_dir().then_some(d)
    });
    installed.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../share"))
}

/// A file in the share folder.
pub fn share_file(share: &Path, name: &str) -> PathBuf {
    share.join(name)
}

/// Where the desktop's configs are read from for the preview.
pub fn desktop_dir() -> PathBuf {
    config_home()
}

/// Where the preview's demo shell starts (it shows `git log`): the git
/// checkout the share folder is in, when it's one (live), else home.
pub fn demo_dir() -> PathBuf {
    let share = share_dir();
    share
        .ancestors()
        .find(|d| d.join(".git").exists())
        .map(Path::to_path_buf)
        .unwrap_or_else(home)
}

/// The studio's guide (the README tab): `docs/` beside the share folder,
/// as in the project and in the package.
pub fn guide(share: &Path) -> PathBuf {
    share.parent().unwrap_or(share).join("docs/theme-studio.md")
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
/// to run: the command on PATH, else `bash <script>.sh` from the scripts
/// folder beside the share folder. It's told where the data and share
/// folders are, so it reads what the studio reads.
pub fn switch_command(name: &str, data: &Path, share: &Path) -> Command {
    // Tests never touch the real desktop: the switch scripts write
    // ~/.config/colors and ~/.config/fonts and restart i3, whatever data
    // folder they're given. A test that means it sets THEME_STUDIO_REAL_SWITCH.
    if cfg!(test) && std::env::var_os("THEME_STUDIO_REAL_SWITCH").is_none() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo \"$0 is off in tests\" >&2; exit 1", name]);
        return c;
    }
    let mut cmd = if on_path(name) {
        Command::new(name)
    } else {
        let script = share
            .parent()
            .unwrap_or(share)
            .join("scripts")
            .join(format!("{name}.sh"));
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
    fn tests_never_run_the_switch_scripts() {
        let d = temp("switch");
        let out = switch_command("font-switch", &d, &d)
            .arg("anything")
            .output()
            .unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("font-switch is off in tests"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// Without $THEME_STUDIO_SHARE (as in tests), a build in the checkout
    /// finds the project's share/, its guide and its scripts.
    #[test]
    fn a_checkout_build_finds_the_projects_files() {
        let share = share_dir();
        // the Nix build has only studio/, not the project around it
        if !share.is_dir() {
            eprintln!("skipped: not in a checkout ({} isn't there)", share.display());
            return;
        }
        assert!(
            share_file(&share, "terminal-colors.json").is_file(),
            "{}",
            share.display()
        );
        assert!(guide(&share).is_file());
        assert!(
            share
                .parent()
                .unwrap()
                .join("scripts/theme-switch.sh")
                .is_file()
        );
        assert_eq!(data_dir(), config_home().join("theme-studio"));
    }
}
