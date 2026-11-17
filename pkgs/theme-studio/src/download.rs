//! Downloading fonts (the F2 page). Google fonts are listed by the Fontsource
//! API and downloaded whole from the google/fonts GitHub repo; other
//! Fontsource fonts come from Fontsource (latin + latin-ext); Nerd Fonts come
//! from the latest ryanoasis/nerd-fonts release. Files go to
//! `~/.local/share/fonts/theme-studio/<id>/`, and each download is added to
//! the installed-fonts list that home/fonts.nix reads, so a rebuild makes it
//! permanent. Network and archive work goes through curl, tar and fc-cache,
//! in background threads.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};

/// Where a font comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// In the google/fonts repo (license folder guessed from `license`).
    Google,
    /// Fontsource's own files (fonts not from Google).
    Fontsource,
    /// A Nerd Fonts release asset: `<asset>.tar.xz`.
    Nerd {
        asset: String,
        url: String,
        size: u64,
    },
}

/// A font that can be downloaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: String,
    pub family: String,
    /// sans-serif, serif, monospace, display, handwriting; "nerd font".
    pub category: String,
    pub license: String,
    pub weights: Vec<u64>,
    pub italic: bool,
    pub variable: bool,
    pub source: Source,
    /// Fontsource's default subset ("latin"; the preview file's).
    pub subset: String,
}

/// The two catalogues.
#[derive(Clone, Debug, Default)]
pub struct Catalogue {
    pub google: Vec<Entry>,
    pub nerd: Vec<Entry>,
    pub nerd_tag: String,
}

/// What a background thread reports.
#[derive(Debug)]
pub enum Msg {
    Catalogue(Result<Catalogue, String>),
    /// Bytes so far of the total (0 if unknown).
    Progress {
        id: String,
        done: u64,
        total: u64,
    },
    /// An install finished: the list entry to record (name, value).
    Installed {
        id: String,
        result: Result<(String, Value), String>,
    },
    /// A preview font is ready: the family name to draw it with.
    Preview {
        id: String,
        result: Result<String, String>,
    },
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    // tests never touch your folders: a temporary home of their own
    if cfg!(test) {
        return std::env::temp_dir()
            .join(format!("theme-studio-test-home-{}", std::process::id()))
            .join(fallback);
    }
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(fallback))
}

/// `~/.local/share/fonts/theme-studio` (or under `$XDG_DATA_HOME`).
pub fn fonts_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("fonts/theme-studio")
}

fn cache_dir() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("theme-studio")
}

/// Expands a leading `~/`.
pub fn expand(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

// ----- curl --------------------------------------------------------------------

/// GETs a URL's body (small JSON answers).
fn get(url: &str) -> Result<String> {
    let out = Command::new("curl")
        .args([
            "-sS",
            "-f",
            "-L",
            "--max-time",
            "60",
            "-H",
            "Accept: application/json",
            url,
        ])
        .output()
        .context("running curl")?;
    if !out.status.success() {
        bail!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Downloads a URL to `out`, calling `progress(bytes so far)` as it goes.
fn fetch(url: &str, out: &Path, mut progress: impl FnMut(u64)) -> Result<()> {
    let mut child = Command::new("curl")
        .args(["-sS", "-f", "-L", "--max-time", "600", "-o"])
        .arg(out)
        .arg(url)
        .stderr(Stdio::piped())
        .spawn()
        .context("running curl")?;
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut err);
                }
                bail!("{url}: {}", err.trim());
            }
            progress(std::fs::metadata(out).map(|m| m.len()).unwrap_or(0));
            return Ok(());
        }
        progress(std::fs::metadata(out).map(|m| m.len()).unwrap_or(0));
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// A JSON answer, cached for a week (an old copy is used when offline).
fn cached(name: &str, url: &str) -> Result<Value> {
    let path = cache_dir().join(name);
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < Duration::from_secs(7 * 24 * 3600));
    let read =
        || -> Option<Value> { serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok() };
    if fresh && let Some(v) = read() {
        return Ok(v);
    }
    match get(url).and_then(|t| Ok(serde_json::from_str::<Value>(&t)?)) {
        Ok(v) => {
            let _ = std::fs::create_dir_all(cache_dir());
            let _ = std::fs::write(&path, v.to_string());
            Ok(v)
        }
        Err(e) => read().ok_or(e),
    }
}

// ----- catalogue -----------------------------------------------------------------

/// Fontsource's font list.
pub fn parse_fontsource(v: &Value) -> Vec<Entry> {
    let Some(list) = v.as_array() else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|f| {
            let s = |k: &str| f.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            Some(Entry {
                id: f.get("id")?.as_str()?.to_string(),
                family: s("family"),
                category: s("category"),
                license: s("license"),
                weights: f
                    .get("weights")
                    .and_then(Value::as_array)
                    .map_or(Vec::new(), |w| w.iter().filter_map(Value::as_u64).collect()),
                italic: f
                    .get("styles")
                    .and_then(Value::as_array)
                    .is_some_and(|s| s.iter().any(|x| x == "italic")),
                variable: f
                    .get("variable")
                    .is_some_and(|x| x.as_bool() == Some(true) || x.is_object()),
                source: if s("type") == "google" {
                    Source::Google
                } else {
                    Source::Fontsource
                },
                subset: Some(s("defSubset"))
                    .filter(|x| !x.is_empty())
                    .unwrap_or_else(|| "latin".into()),
            })
        })
        .collect()
}

/// The Nerd Fonts release's `.tar.xz` assets.
pub fn parse_nerd(v: &Value) -> (String, Vec<Entry>) {
    let tag = v
        .get("tag_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let entries = v
        .get("assets")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let name = x
                        .get("name")?
                        .as_str()?
                        .strip_suffix(".tar.xz")?
                        .to_string();
                    Some(Entry {
                        id: format!("nerd-{}", name.to_lowercase()),
                        family: format!("{name} Nerd Font"),
                        category: "nerd font".into(),
                        license: String::new(),
                        weights: Vec::new(),
                        italic: false,
                        variable: false,
                        source: Source::Nerd {
                            url: x.get("browser_download_url")?.as_str()?.to_string(),
                            size: x.get("size").and_then(Value::as_u64).unwrap_or(0),
                            asset: name,
                        },
                        subset: String::new(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (tag, entries)
}

/// Fetches both catalogues in the background.
pub fn load_catalogue(tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let google = cached("fontsource.json", "https://api.fontsource.org/v1/fonts")
            .map(|v| parse_fontsource(&v));
        let nerd = cached(
            "nerd-fonts.json",
            "https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest",
        )
        .map(|v| parse_nerd(&v));
        let result = match (google, nerd) {
            (Err(e), _) => Err(format!("{e:#}")),
            (Ok(google), nerd) => {
                let (nerd_tag, nerd) = nerd.unwrap_or_default();
                Ok(Catalogue {
                    google,
                    nerd,
                    nerd_tag,
                })
            }
        };
        let _ = tx.send(Msg::Catalogue(result));
    });
}

// ----- installing ----------------------------------------------------------------

/// The google/fonts folders to look in: the license's first.
pub fn google_dirs(e: &Entry) -> Vec<String> {
    let dir = e.id.replace('-', "");
    let first = match e.license.as_str() {
        l if l.starts_with("Apache") => "apache",
        l if l.starts_with("UFL") => "ufl",
        _ => "ofl",
    };
    let mut v = vec![format!("{first}/{dir}")];
    v.extend(
        ["ofl", "apache", "ufl"]
            .iter()
            .filter(|l| **l != first)
            .map(|l| format!("{l}/{dir}")),
    );
    v
}

/// sha256 as a Nix SRI hash (for fetchurl in home/fonts.nix).
fn sri(file: &Path) -> Result<String> {
    let out = Command::new("sha256sum")
        .arg(file)
        .output()
        .context("running sha256sum")?;
    let hex = String::from_utf8_lossy(&out.stdout);
    let hex = hex
        .split_whitespace()
        .next()
        .ok_or_else(|| anyhow!("no checksum"))?;
    let bytes: Vec<u8> = (0..hex.len() / 2)
        .filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok())
        .collect();
    Ok(format!("sha256-{}", crate::app::base64(&bytes)))
}

/// Downloads `files` (name, url, size) into `dir`, reporting progress.
fn fetch_all(
    id: &str,
    files: &[(String, String, u64)],
    dir: &Path,
    tx: &Sender<Msg>,
) -> Result<()> {
    let total: u64 = files.iter().map(|f| f.2).sum();
    let mut before = 0;
    for (name, url, size) in files {
        fetch(url, &dir.join(name), |n| {
            let _ = tx.send(Msg::Progress {
                id: id.to_string(),
                done: before + n,
                total,
            });
        })?;
        before += size;
    }
    Ok(())
}

/// Google: every .ttf of the font's folder in the google/fonts repo.
fn install_google(e: &Entry, dir: &Path, tx: &Sender<Msg>) -> Result<Value> {
    for repo_dir in google_dirs(e) {
        let Ok(text) = get(&format!(
            "https://api.github.com/repos/google/fonts/contents/{repo_dir}"
        )) else {
            continue;
        };
        let list: Value = serde_json::from_str(&text)?;
        let files: Vec<(String, String, u64)> = list
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|f| {
                        let name = f.get("name")?.as_str()?;
                        name.ends_with(".ttf").then(|| {
                            Some((
                                name.to_string(),
                                f.get("download_url")?.as_str()?.to_string(),
                                f.get("size").and_then(Value::as_u64).unwrap_or(0),
                            ))
                        })?
                    })
                    .collect()
            })
            .unwrap_or_default();
        if files.is_empty() {
            continue;
        }
        fetch_all(&e.id, &files, dir, tx)?;
        return Ok(json!({"source": "google", "id": e.id, "dir": repo_dir}));
    }
    // not in the repo: Fontsource's files
    install_fontsource(e, dir, tx)
}

/// Fontsource: a ttf per weight and style, latin and latin-ext.
fn install_fontsource(e: &Entry, dir: &Path, tx: &Sender<Msg>) -> Result<Value> {
    let v: Value = serde_json::from_str(&get(&format!(
        "https://api.fontsource.org/v1/fonts/{}",
        e.id
    ))?)?;
    let mut files = Vec::new();
    for (weight, styles) in v
        .get("variants")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        for (style, subsets) in styles.as_object().into_iter().flatten() {
            let subsets = subsets.as_object().cloned().unwrap_or_default();
            let wanted: Vec<&String> = subsets
                .keys()
                .filter(|s| *s == "latin" || *s == "latin-ext")
                .collect();
            let pick = if wanted.is_empty() {
                subsets.keys().take(1).collect()
            } else {
                wanted
            };
            for subset in pick {
                if let Some(url) = subsets[subset].pointer("/url/ttf").and_then(Value::as_str) {
                    files.push((
                        format!("{}-{subset}-{weight}-{style}.ttf", e.id),
                        url.to_string(),
                        0,
                    ));
                }
            }
        }
    }
    if files.is_empty() {
        bail!("Fontsource has no files for {}", e.family);
    }
    fetch_all(&e.id, &files, dir, tx)?;
    let mut list = Vec::new();
    for (name, url, _) in &files {
        list.push(json!({"name": name, "url": url, "hash": sri(&dir.join(name))?}));
    }
    Ok(json!({"source": "fontsource", "id": e.id, "files": list}))
}

/// Nerd Fonts: the release's `<asset>.tar.xz`, unpacked.
fn install_nerd(e: &Entry, dir: &Path, tx: &Sender<Msg>) -> Result<Value> {
    let Source::Nerd { asset, url, size } = &e.source else {
        bail!("not a Nerd Font")
    };
    let archive = dir.join(format!("{asset}.tar.xz"));
    fetch_all(
        &e.id,
        &[(format!("{asset}.tar.xz"), url.clone(), *size)],
        dir,
        tx,
    )?;
    let out = Command::new("tar")
        .arg("-xJf")
        .arg(&archive)
        .arg("-C")
        .arg(dir)
        .output()?;
    let _ = std::fs::remove_file(&archive);
    if !out.status.success() {
        bail!(
            "unpacking {asset}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(json!({"source": "nerd", "id": e.id, "asset": asset}))
}

/// Installs a font in the background into `root/<id>/`, then refreshes
/// fontconfig's cache.
pub fn install(e: Entry, root: PathBuf, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let dir = root.join(&e.id);
        let result = (|| -> Result<(String, Value)> {
            std::fs::create_dir_all(&dir)?;
            let entry = match e.source {
                Source::Google => install_google(&e, &dir, &tx)?,
                Source::Fontsource => install_fontsource(&e, &dir, &tx)?,
                Source::Nerd { .. } => install_nerd(&e, &dir, &tx)?,
            };
            let _ = Command::new("fc-cache").arg(&root).output();
            let name = match &e.source {
                Source::Nerd { asset, .. } => asset.clone(),
                _ => e.family.clone(),
            };
            Ok((name, entry))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        let _ = tx.send(Msg::Installed {
            id: e.id.clone(),
            result: result.map_err(|e| format!("{e:#}")),
        });
    });
}

// ----- previews ---------------------------------------------------------------------

/// Fonts fetched only to preview them:
/// `~/.local/share/theme-studio/previews/<id>/` (not the cache, so cleanup
/// tools don't undo a prefetch or a tournament's downloads).
pub fn preview_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("theme-studio/previews")
}

/// Moves previews from where older versions kept them
/// (`~/.cache/theme-studio/preview/`), keeping only each font's Regular.
pub fn migrate_previews() {
    let (old, new) = (cache_dir().join("preview"), preview_dir());
    if !old.is_dir() || new.exists() {
        return;
    }
    if let Some(parent) = new.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // on another file system the old ones stay (and are fetched again)
    if std::fs::rename(&old, &new).is_ok()
        && let Ok(dirs) = std::fs::read_dir(&new)
    {
        for d in dirs.flatten() {
            keep_regular_only(&d.path());
        }
    }
}

/// A fontconfig setup that adds the preview fonts to the system's, so they
/// can be drawn without installing them.
pub fn preview_fontconfig() -> PathBuf {
    let path = cache_dir().join("preview-fonts.conf");
    let conf = format!(
        "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n  \
         <include ignore_missing=\"yes\">/etc/fonts/fonts.conf</include>\n  <dir>{}</dir>\n  \
         <cachedir>{}</cachedir>\n</fontconfig>\n",
        preview_dir().display(),
        cache_dir().join("fontconfig").display()
    );
    // written again when the folders change (they moved once)
    if std::fs::read_to_string(&path).ok().as_deref() != Some(conf.as_str()) {
        let _ = std::fs::create_dir_all(cache_dir());
        let _ = std::fs::write(&path, conf);
    }
    path
}

/// Whether a font's preview file is already here.
pub fn has_preview(id: &str) -> bool {
    preview_file(&preview_dir().join(id)).is_some()
}

/// The space the preview fonts take, in bytes.
pub fn previews_size() -> u64 {
    fn walk(p: &Path) -> u64 {
        std::fs::read_dir(p).map_or(0, |entries| {
            entries
                .flatten()
                .map(|e| match e.file_type() {
                    Ok(t) if t.is_dir() => walk(&e.path()),
                    _ => e.metadata().map_or(0, |m| m.len()),
                })
                .sum()
        })
    }
    walk(&preview_dir())
}

/// Deletes every preview font (they're fetched again when needed).
pub fn clear_previews() -> Result<()> {
    let dir = preview_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

/// Leaves only the face a preview draws with (a Nerd Font archive unpacks
/// to dozens).
fn keep_regular_only(dir: &Path) {
    let Some(keep) = preview_file(dir) else {
        return;
    };
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p != keep {
                let _ = if p.is_dir() {
                    std::fs::remove_dir_all(&p)
                } else {
                    std::fs::remove_file(&p)
                };
            }
        }
    }
}

/// The family name inside a font file (fc-scan).
fn family_of(file: &Path) -> Option<String> {
    let out = Command::new("fc-scan")
        .args(["--format", "%{family[0]}"])
        .arg(file)
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// The font file to read the family from: a Regular, not a Mono / Propo
/// variant, when there's a choice.
fn preview_file(dir: &Path) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|f| f.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ttf" || x == "otf"))
        .collect();
    files.sort_by_key(|p| {
        let n = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        (
            n.contains("Mono-") || n.contains("Propo-"),
            !n.contains("Regular"),
            n.len(),
        )
    });
    files.into_iter().next()
}

/// Fetches a font for its preview in the background (Google / Fontsource:
/// one file of the regular weight; Nerd Fonts: the archive, of which only
/// the Regular is kept), unless already here.
pub fn fetch_preview(e: Entry, tx: Sender<Msg>) {
    std::thread::spawn(move || {
        let dir = preview_dir().join(&e.id);
        let result = (|| -> Result<String> {
            if let Some(f) = preview_file(&dir) {
                return family_of(&f).ok_or_else(|| anyhow!("no family name in {}", f.display()));
            }
            std::fs::create_dir_all(&dir)?;
            match &e.source {
                Source::Nerd { .. } => {
                    install_nerd(&e, &dir, &tx)?;
                    keep_regular_only(&dir);
                }
                _ => {
                    // the regular weight, else the nearest
                    let w = e
                        .weights
                        .iter()
                        .min_by_key(|w| (**w as i64 - 400).abs())
                        .copied()
                        .unwrap_or(400);
                    let url = format!(
                        "https://cdn.jsdelivr.net/fontsource/fonts/{}@latest/{}-{w}-normal.ttf",
                        e.id, e.subset
                    );
                    fetch(&url, &dir.join(format!("{}-{w}.ttf", e.id)), |_| {})?;
                }
            }
            let f = preview_file(&dir).ok_or_else(|| anyhow!("no font file came with it"))?;
            family_of(&f).ok_or_else(|| anyhow!("no family name in {}", f.display()))
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&dir);
        }
        let _ = tx.send(Msg::Preview {
            id: e.id.clone(),
            result: result.map_err(|e| format!("{e:#}")),
        });
    });
}

/// Removes a downloaded font's files and refreshes fontconfig's cache.
pub fn remove_files(root: &Path, id: &str) -> Result<()> {
    let dir = root.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
        let _ = Command::new("fc-cache").arg(root).output();
    }
    Ok(())
}

// ----- the installed-fonts list ------------------------------------------------------

/// fonts-installed.json: `{"version": 1, "fonts": {name: {source, id, …}}}`,
/// read by home/fonts.nix.
pub fn read_list(path: &Path) -> Map<String, Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("fonts").and_then(Value::as_object).cloned())
        .unwrap_or_default()
}

pub fn write_list(path: &Path, fonts: Map<String, Value>) -> Result<()> {
    crate::store::write_json_file(path, &json!({"version": 1, "fonts": fonts}))
}

/// A channel for the page's background work.
pub fn channel_pair() -> (Sender<Msg>, Receiver<Msg>) {
    channel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_keep_only_the_regular() {
        let dir = std::env::temp_dir().join(format!("theme-studio-keep-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        for f in [
            "HackNerdFont-Regular.ttf",
            "HackNerdFontMono-Regular.ttf",
            "HackNerdFont-Bold.ttf",
            "LICENSE.md",
            "sub/x.ttf",
        ] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        keep_regular_only(&dir);
        let left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into())
            .collect();
        assert_eq!(left, ["HackNerdFont-Regular.ttf"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn catalogues_parse() {
        let fs = json!([
            {"id": "roboto-mono", "family": "Roboto Mono", "category": "monospace", "license": "Apache-2.0",
             "weights": [100, 400, 700], "styles": ["normal", "italic"], "variable": true, "type": "google"},
            {"id": "zen-loop", "family": "Zen Loop", "category": "display", "license": "OFL-1.1",
             "weights": [400], "styles": ["normal"], "variable": false, "type": "other"}
        ]);
        let e = parse_fontsource(&fs);
        assert_eq!(e.len(), 2);
        assert_eq!(
            (e[0].family.as_str(), e[0].italic, e[0].variable),
            ("Roboto Mono", true, true)
        );
        assert_eq!(e[1].source, Source::Fontsource);
        assert_eq!(
            google_dirs(&e[0]),
            ["apache/robotomono", "ofl/robotomono", "ufl/robotomono"]
        );
        assert_eq!(google_dirs(&e[1])[0], "ofl/zenloop");
        let nerd = json!({"tag_name": "v3.5.1", "assets": [
            {"name": "JetBrainsMono.tar.xz", "size": 10, "browser_download_url": "https://x/JetBrainsMono.tar.xz"},
            {"name": "JetBrainsMono.zip", "size": 12, "browser_download_url": "https://x/JetBrainsMono.zip"}
        ]});
        let (tag, n) = parse_nerd(&nerd);
        assert_eq!((tag.as_str(), n.len()), ("v3.5.1", 1));
        assert_eq!(n[0].family, "JetBrainsMono Nerd Font");
        assert_eq!(n[0].id, "nerd-jetbrainsmono");
    }

    #[test]
    fn list_round_trips_and_sri_hashes() {
        let d = std::env::temp_dir().join(format!("theme-studio-dl-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("fonts-installed.json");
        assert!(read_list(&path).is_empty());
        let mut m = Map::new();
        m.insert("Inter".into(), json!({"source": "google", "id": "inter"}));
        write_list(&path, m.clone()).unwrap();
        assert_eq!(read_list(&path), m);
        // the SRI form Nix expects (sha256 of "abc")
        std::fs::write(d.join("f"), "abc").unwrap();
        assert_eq!(
            sri(&d.join("f")).unwrap(),
            "sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
        assert_eq!(expand("~/x"), home().join("x"));
        std::fs::remove_dir_all(d).unwrap();
    }

    /// Real downloads (network): `cargo test -- --ignored live_installs`.
    #[test]
    #[ignore]
    fn live_installs() {
        let root = std::env::temp_dir().join(format!("theme-studio-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (tx, rx) = channel_pair();
        let fs = cached("fontsource.json", "https://api.fontsource.org/v1/fonts").unwrap();
        let all = parse_fontsource(&fs);
        let pick = |id: &str| all.iter().find(|e| e.id == id).unwrap().clone();
        let (_, nerd) = parse_nerd(
            &cached(
                "nerd-fonts.json",
                "https://api.github.com/repos/ryanoasis/nerd-fonts/releases/latest",
            )
            .unwrap(),
        );
        let heavy = nerd
            .into_iter()
            .find(|e| e.family == "HeavyData Nerd Font")
            .unwrap();
        for e in [pick("zen-loop"), pick("bagnard"), heavy] {
            let id = e.id.clone();
            install(e, root.clone(), tx.clone());
            let mut progress = 0;
            let result = loop {
                match rx.recv_timeout(Duration::from_secs(120)).unwrap() {
                    Msg::Progress { .. } => progress += 1,
                    Msg::Installed { result, .. } => break result,
                    Msg::Catalogue(_) | Msg::Preview { .. } => {}
                }
            };
            let (name, entry) = result.unwrap();
            let ttf = std::fs::read_dir(root.join(&id))
                .unwrap()
                .flatten()
                .filter(|f| {
                    f.path()
                        .extension()
                        .is_some_and(|x| x == "ttf" || x == "otf")
                })
                .count();
            println!("{name}: {entry}  ({ttf} font files, {progress} progress reports)");
            assert!(ttf > 0, "{name} has font files");
        }
        remove_files(&root, "zen-loop").unwrap();
        assert!(!root.join("zen-loop").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
