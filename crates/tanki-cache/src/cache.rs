use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CachedResource {
    pub path: PathBuf,
    pub logical_url: Option<String>,
    pub group: String,
    pub role: String,
    pub kind: String,
    pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CacheScan {
    pub root: PathBuf,
    pub game: String,
    pub resources: Vec<CachedResource>,
}

/// The kind of cache this app reads: the URL-keyed cache an Adobe AIR client keeps in its
/// application storage. Packs name it in `games`. It names a format, never a game.
pub const GAME: &str = "air-cache";

/// The folders Adobe AIR keeps every application's storage in, `<base>/<application id>/Local
/// Store/` (Adobe's documented layout; macOS has used both bases over the years).
fn storage_bases() -> Vec<PathBuf> {
    let mut bases = Vec::new();
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        bases.push(home.join("Library/Application Support"));
        bases.push(home.join("Library/Preferences"));
    }
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        bases.push(PathBuf::from(appdata));
    }
    bases
}

/// The id of the AIR application a cache belongs to: the folder above `Local Store`, read from
/// the cache's own path. `None` for a folder outside an application's storage.
pub fn client_id(root: &Path) -> Option<String> {
    let local_store = root.parent()?;
    if local_store.file_name()? != "Local Store" {
        return None;
    }
    Some(local_store.parent()?.file_name()?.to_str()?.to_owned())
}

/// `TANKI_CACHE_DIR` (plus optional `TANKI_GAME`, default [`GAME`]) replaces automatic discovery,
/// so tests run against a fixture cache instead of the real game. Otherwise every AIR
/// application's `Local Store/cache` is a candidate; no client is known by name.
pub fn candidate_cache_roots() -> Vec<(String, PathBuf)> {
    if let Some(dir) = std::env::var_os("TANKI_CACHE_DIR") {
        let game = std::env::var("TANKI_GAME").unwrap_or_else(|_| GAME.into());
        return vec![(game, PathBuf::from(dir))];
    }
    let mut roots = Vec::new();
    for base in storage_bases() {
        let Ok(apps) = std::fs::read_dir(&base) else {
            continue;
        };
        let mut found: Vec<PathBuf> = apps
            .filter_map(|app| app.ok())
            .map(|app| app.path().join("Local Store").join("cache"))
            .filter(|cache| cache.is_dir())
            .collect();
        found.sort();
        roots.extend(found.into_iter().map(|cache| (GAME.to_owned(), cache)));
    }
    roots
}

/// The candidates that exist and hold cache entries (another AIR application's storage may have a
/// `cache` folder of its own).
pub fn existing_cache_roots() -> Vec<(String, PathBuf)> {
    let pinned = std::env::var_os("TANKI_CACHE_DIR").is_some();
    candidate_cache_roots()
        .into_iter()
        .filter(|(_, p)| p.is_dir() && (pinned || looks_like_cache(p)))
        .collect()
}

/// Does this folder hold game cache entries? The client names its files after the base64 of the
/// resource URL, so a cache has such names and an arbitrary folder (a model archive, Downloads) does
/// not. Looks at the first few hundred files only, so choosing a huge folder stays quick.
pub fn looks_like_cache(root: &Path) -> bool {
    WalkDir::new(root)
        .follow_links(false)
        .max_depth(8)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .take(400)
        .any(|e| decoded_name(e.path()).is_some())
}

pub fn decoded_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let bytes = STANDARD.decode(name).ok()?;
    let s = String::from_utf8(bytes).ok()?;
    if s.starts_with("http://") || s.starts_with("https://") {
        Some(s)
    } else {
        None
    }
}

fn classify(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".3ds") || lower.ends_with(".a3d") {
        "model".into()
    } else if lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
    {
        "image".into()
    } else if lower.ends_with(".tara") {
        "archive".into()
    } else if lower.ends_with(".xml") || lower.ends_with(".json") {
        "metadata".into()
    } else if lower.ends_with(".swf") {
        "flash".into()
    } else if lower.ends_with(".mp3") || lower.ends_with(".ogg") || lower.ends_with(".wav") {
        "sound".into()
    } else {
        "other".into()
    }
}

pub fn scan_cache(root: &Path, game_hint: Option<&str>) -> Result<CacheScan> {
    anyhow::ensure!(
        root.is_dir(),
        "cache folder does not exist: {}",
        root.display()
    );
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot open {}", root.display()))?;
    let mut resources = Vec::new();
    for entry in WalkDir::new(&root)
        .follow_links(false)
        .max_depth(8)
        .into_iter()
        .filter_map(|x| x.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let logical_url = decoded_name(path);
        let role = logical_url
            .as_deref()
            .unwrap_or_else(|| {
                path.file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
            })
            .rsplit('/')
            .next()
            .unwrap_or("unknown")
            .to_owned();
        let group = logical_url
            .as_deref()
            .and_then(|s| s.rsplit_once('/').map(|(g, _)| g.to_owned()))
            .unwrap_or_else(|| {
                path.parent()
                    .unwrap_or(&root)
                    .to_string_lossy()
                    .into_owned()
            });
        let kind = classify(&role);
        resources.push(CachedResource {
            path: path.to_path_buf(),
            logical_url,
            group,
            role,
            kind,
            size: meta.len(),
        });
    }
    resources.sort_by(|a, b| a.group.cmp(&b.group).then(a.role.cmp(&b.role)));
    let game = game_hint.map(str::to_owned).unwrap_or_else(|| {
        candidate_cache_roots()
            .into_iter()
            .find_map(|(game, path)| {
                path.canonicalize()
                    .ok()
                    .filter(|candidate| candidate == &root)
                    .map(|_| game)
            })
            .unwrap_or_else(|| "unknown".into())
    });
    Ok(CacheScan {
        root,
        game,
        resources,
    })
}

pub fn known_target_group(
    game: &str,
    group: &str,
) -> Option<(&'static str, &'static str, &'static str)> {
    crate::targets::identify(game, group).map(|t| (t.kind, t.id, t.grade))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_cache_folder_is_told_from_an_ordinary_one() {
        let root = std::env::temp_dir().join(format!("tanki-looks-{}", std::process::id()));
        let cache = root.join("cache");
        let other = root.join("other");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::create_dir_all(other.join("Flash")).unwrap();
        std::fs::write(other.join("Flash/object.3ds"), b"x").unwrap();
        std::fs::write(other.join("readme.md"), b"x").unwrap();
        assert!(!looks_like_cache(&cache), "an empty folder is not a cache");
        assert!(
            !looks_like_cache(&other),
            "ordinary files are not cache entries"
        );
        let name = STANDARD.encode("http://example.test/0/0/10/14/1/details.jpg");
        std::fs::write(cache.join(name), b"x").unwrap();
        assert!(looks_like_cache(&cache));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn decodes_a_cache_key() {
        let url = "http://example.test/0/0/10/14/1/details.jpg";
        let name = STANDARD.encode(url);
        assert_eq!(decoded_name(Path::new(&name)).as_deref(), Some(url));
    }
    #[test]
    fn identifies_known_groups_only() {
        assert_eq!(
            known_target_group("air-cache", "http://x/0/0/10/14/123"),
            Some(("hull", "wasp", "m3"))
        );
        assert_eq!(known_target_group("other", "http://x/0/0/10/14/123"), None);
    }
}
