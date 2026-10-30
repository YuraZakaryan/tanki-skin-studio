// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Context, Result};
use image::{DynamicImage, ImageFormat, codecs::jpeg::JpegEncoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashMap},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tanki_cache::cache::{CacheScan, known_target_group};
use tanki_pack::pack::{InstalledPack, PackTarget, PackVariant, read_pack_resource};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileChange {
    pub target: PathBuf,
    pub backup_name: String,
    pub before_sha256: String,
    pub after_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub pack_id: String,
    pub variant_id: String,
    pub changes: Vec<FileChange>,
    /// The item the variant was applied to. Missing in backups written by older versions.
    #[serde(default)]
    pub target: Option<PackTarget>,
    #[serde(default)]
    pub variant_name: String,
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}
fn jpeg_rgb(image: &DynamicImage) -> Result<Vec<u8>> {
    let rgb = image.to_rgb8();
    let mut result = Vec::new();
    JpegEncoder::new_with_quality(&mut result, 98).encode_image(&DynamicImage::ImageRgb8(rgb))?;
    Ok(result)
}
fn jpeg_alpha(image: &DynamicImage) -> Result<Vec<u8>> {
    let rgba = image.to_rgba8();
    let gray = image::GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        image::Luma([rgba.get_pixel(x, y).0[3]])
    });
    let mut result = Vec::new();
    JpegEncoder::new_with_quality(&mut result, 98).encode_image(&DynamicImage::ImageLuma8(gray))?;
    Ok(result)
}
fn replacements(pack: &InstalledPack, variant: &PackVariant) -> Result<HashMap<String, Vec<u8>>> {
    let mut map = HashMap::new();
    for resource in &variant.resources {
        let bytes = read_pack_resource(pack, &resource.path)?;
        match resource.role.as_str() {
            "model" => {
                map.insert("object.3ds".into(), bytes);
            }
            "lightmap" => {
                map.insert("lightmap.jpg".into(), bytes);
            }
            "details" => {
                if resource.path.to_ascii_lowercase().ends_with(".png") {
                    let img = image::load_from_memory_with_format(&bytes, ImageFormat::Png)?;
                    anyhow::ensure!(
                        img.width() <= 8192 && img.height() <= 8192,
                        "image too large"
                    );
                    map.insert("details.jpg".into(), jpeg_rgb(&img)?);
                    map.insert("details_alpha.jpg".into(), jpeg_alpha(&img)?);
                } else {
                    map.insert("details.jpg".into(), bytes);
                }
            }
            "details_alpha" => {
                map.insert("details_alpha.jpg".into(), bytes);
            }
            other => anyhow::bail!("unsupported resource role for apply: {other}"),
        }
    }
    Ok(map)
}
/// The cache files a variant would overwrite, with the size of each replacement, sorted by name.
/// Lets the UI list exactly what an apply will touch before the user confirms it.
pub fn planned_files(pack: &InstalledPack, variant: &PackVariant) -> Result<Vec<(String, u64)>> {
    let mut files: Vec<(String, u64)> = replacements(pack, variant)?
        .into_iter()
        .map(|(name, bytes)| (name, bytes.len() as u64))
        .collect();
    files.sort();
    Ok(files)
}
fn atomic_replace(target: &Path, bytes: &[u8]) -> Result<()> {
    let parent = target.parent().context("target has no parent")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let temp = parent.join(format!(
        ".tanki-studio-{}-{stamp}-{}",
        std::process::id(),
        target.file_name().unwrap_or_default().to_string_lossy()
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::set_permissions(&temp, fs::metadata(target)?.permissions())?;
        fs::rename(&temp, target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result?;
    Ok(())
}

/// The model and texture files of an item in the cache, which a skin replaces.
const SKIN_FILES: &[&str] = &[
    "object.3ds",
    "lightmap.jpg",
    "details.jpg",
    "details_alpha.jpg",
];

/// The cache files the variant would write, from its roles alone (nothing is read).
fn produced_files(variant: &PackVariant) -> BTreeSet<&'static str> {
    let mut files = BTreeSet::new();
    for resource in &variant.resources {
        match resource.role.as_str() {
            "model" => {
                files.insert("object.3ds");
            }
            "lightmap" => {
                files.insert("lightmap.jpg");
            }
            "details" => {
                files.insert("details.jpg");
                // A PNG carries its own transparency, which becomes the alpha file.
                if resource.path.to_ascii_lowercase().ends_with(".png") {
                    files.insert("details_alpha.jpg");
                }
            }
            "details_alpha" => {
                files.insert("details_alpha.jpg");
            }
            _ => {}
        }
    }
    files
}

/// The model and texture files the item `target` has in this cache.
pub fn cache_skin_files(scan: &CacheScan, target: &PackTarget) -> BTreeSet<String> {
    scan.resources
        .iter()
        .filter(|r| SKIN_FILES.contains(&r.role.as_str()))
        .filter(|r| {
            known_target_group(&scan.game, &r.group)
                == Some((
                    target.kind.as_str(),
                    target.id.as_str(),
                    target.grade.as_str(),
                ))
        })
        .map(|r| r.role.clone())
        .collect()
}

/// The files of `cache_files` the variant does not bring. A skin must replace every model and
/// texture file of the item, or the game would show one skin's model with another's textures; the
/// preview is not one of them and is optional. Empty when the variant is complete.
pub fn missing_from(cache_files: &BTreeSet<String>, variant: &PackVariant) -> Vec<String> {
    let produced = produced_files(variant);
    cache_files
        .iter()
        .filter(|file| !produced.contains(file.as_str()))
        .cloned()
        .collect()
}

/// [`missing_from`] for the item `target` of this cache.
pub fn missing_files(scan: &CacheScan, target: &PackTarget, variant: &PackVariant) -> Vec<String> {
    missing_from(&cache_skin_files(scan, target), variant)
}

pub fn apply_variant(
    scan: &CacheScan,
    pack: &InstalledPack,
    target: &PackTarget,
    variant: &PackVariant,
    backup_root: &Path,
) -> Result<Transaction> {
    anyhow::ensure!(
        pack.manifest.games.is_empty() || pack.manifest.games.contains(&scan.game),
        "pack does not support {}",
        scan.game
    );
    let missing = missing_files(scan, target, variant);
    anyhow::ensure!(
        missing.is_empty(),
        "the skin is incomplete: the pack has no {} for it; every file except the preview is needed",
        missing.join(", ")
    );
    let replacement = replacements(pack, variant)?;
    anyhow::ensure!(
        !replacement.is_empty(),
        "variant has no applicable resources"
    );
    let groups: Vec<_> = scan
        .resources
        .iter()
        .filter(|r| {
            known_target_group(&scan.game, &r.group)
                == Some((
                    target.kind.as_str(),
                    target.id.as_str(),
                    target.grade.as_str(),
                ))
        })
        .collect();
    anyhow::ensure!(!groups.is_empty(), "target not recognized in this cache");
    let mut targets = Vec::<(PathBuf, Vec<u8>)>::new();
    for r in groups {
        if let Some(bytes) = replacement.get(&r.role) {
            targets.push((r.path.clone(), bytes.clone()));
        }
    }
    anyhow::ensure!(!targets.is_empty(), "target resources missing");
    for role in replacement.keys() {
        anyhow::ensure!(
            targets.iter().any(|(p, _)| scan
                .resources
                .iter()
                .any(|r| r.path == *p && r.role == *role)),
            "required cache resource missing: {role}"
        );
    }
    let cache = scan.root.canonicalize()?;
    for (p, _) in &targets {
        anyhow::ensure!(
            p.parent() == Some(cache.as_path()),
            "target outside selected cache"
        );
        anyhow::ensure!(
            !p.is_symlink() && p.is_file(),
            "invalid target path: {}",
            p.display()
        );
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let id = format!("{stamp}");
    let dir = backup_root.join(&id);
    fs::create_dir_all(&dir)?;
    let mut tx = Transaction {
        id,
        pack_id: pack.manifest.id.clone(),
        variant_id: variant.id.clone(),
        changes: Vec::new(),
        target: Some(target.clone()),
        variant_name: variant.name.clone(),
    };
    let mut old = Vec::new();
    for (index, (path, new)) in targets.iter().enumerate() {
        let before = fs::read(path)?;
        let backup_name = format!("{index:04}.bin");
        fs::write(dir.join(&backup_name), &before)?;
        tx.changes.push(FileChange {
            target: path.clone(),
            backup_name,
            before_sha256: hash(&before),
            after_sha256: hash(new),
        });
        old.push(before);
    }
    fs::write(
        dir.join("transaction.json"),
        serde_json::to_vec_pretty(&tx)?,
    )?;
    let mut done = 0;
    let result = (|| -> Result<()> {
        for (path, new) in &targets {
            atomic_replace(path, new)?;
            done += 1;
        }
        for (path, new) in &targets {
            anyhow::ensure!(
                hash(&fs::read(path)?) == hash(new),
                "post-write verification failed: {}",
                path.display()
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = targets
            .iter()
            .take(done)
            .enumerate()
            .map(|(i, (path, _))| atomic_replace(path, &old[i]))
            .collect::<Result<Vec<_>>>();
        return match rollback {
            Ok(_) => Err(error).context(format!("apply failed; rolled back {done} files")),
            Err(rollback_error) => Err(error).context(format!(
                "apply failed; rollback also failed: {rollback_error:#}; backup: {}",
                dir.display()
            )),
        };
    }
    Ok(tx)
}

/// Written into a backup folder once its originals are back in the cache. Restoring goes back one
/// replacement at a time: a restored backup is never the latest one again, so the next restore
/// reaches the replacement before it and, in the end, the original cache.
const RESTORED: &str = "restored";

/// Whether the originals of this backup have been put back.
pub fn is_restored(dir: &Path) -> bool {
    dir.join(RESTORED).is_file()
}

/// The newest backup that has not been restored yet.
pub fn latest_transaction(backup_root: &Path) -> Result<Option<(PathBuf, Transaction)>> {
    if !backup_root.is_dir() {
        return Ok(None);
    }
    let mut dirs = fs::read_dir(backup_root)?
        .filter_map(|x| x.ok())
        .filter(|x| x.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(|e| e.file_name());
    for entry in dirs.into_iter().rev() {
        let path = entry.path();
        if is_restored(&path) {
            continue;
        }
        if let Ok(bytes) = fs::read(path.join("transaction.json"))
            && let Ok(tx) = serde_json::from_slice(&bytes)
        {
            return Ok(Some((path, tx)));
        }
    }
    Ok(None)
}
/// One stored backup, for the history shown in the UI.
#[derive(Clone, Debug)]
pub struct BackupEntry {
    pub dir: PathBuf,
    pub tx: Transaction,
    /// When the replacement was made, seconds since the Unix epoch (taken from the transaction id).
    pub created_unix: u64,
    /// Size of the saved originals.
    pub bytes: u64,
    /// The originals are back in the cache (see [`latest_transaction`]).
    pub restored: bool,
}

/// Every readable backup, newest first.
pub fn list_backups(backup_root: &Path) -> Result<Vec<BackupEntry>> {
    if !backup_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut dirs = fs::read_dir(backup_root)?
        .filter_map(|x| x.ok())
        .filter(|x| x.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.file_name()));
    let mut entries = Vec::new();
    for entry in dirs {
        let dir = entry.path();
        let Ok(bytes) = fs::read(dir.join("transaction.json")) else {
            continue;
        };
        let Ok(tx) = serde_json::from_slice::<Transaction>(&bytes) else {
            continue;
        };
        let created_unix = tx
            .id
            .parse::<u128>()
            .map_or(0, |nanos| (nanos / 1_000_000_000) as u64);
        let bytes = tx
            .changes
            .iter()
            .filter_map(|change| fs::metadata(dir.join(&change.backup_name)).ok())
            .map(|meta| meta.len())
            .sum();
        let restored = is_restored(&dir);
        entries.push(BackupEntry {
            dir,
            tx,
            created_unix,
            bytes,
            restored,
        });
    }
    Ok(entries)
}

pub fn restore_transaction(dir: &Path, tx: &Transaction) -> Result<()> {
    for change in &tx.changes {
        anyhow::ensure!(
            change.backup_name.ends_with(".bin")
                && change
                    .backup_name
                    .trim_end_matches(".bin")
                    .bytes()
                    .all(|b| b.is_ascii_digit()),
            "invalid backup name"
        );
        let bytes = fs::read(dir.join(&change.backup_name))?;
        anyhow::ensure!(hash(&bytes) == change.before_sha256, "backup corrupt");
        anyhow::ensure!(
            change.target.is_file() && !change.target.is_symlink(),
            "target missing"
        );
        atomic_replace(&change.target, &bytes)?;
    }
    fs::write(dir.join(RESTORED), &tx.id).context("could not mark the backup as restored")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{Engine, engine::general_purpose::STANDARD};
    use tanki_cache::cache::scan_cache;
    use tanki_pack::pack::{PackItem, PackManifest, PackResource};

    #[test]
    fn applies_two_cache_versions_and_restores_originals() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "tanki-apply-test-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let cache = root.join("cache");
        let pack_dir = root.join("pack");
        fs::create_dir_all(&cache)?;
        fs::create_dir_all(&pack_dir)?;
        let mut originals = Vec::new();
        for version in ["111", "222"] {
            let name = STANDARD.encode(format!(
                "http://example.test/0/0/10/14/{version}/object.3ds"
            ));
            let path = cache.join(name);
            let old = format!("original-{version}").into_bytes();
            fs::write(&path, &old)?;
            originals.push((path, old));
        }
        let replacement = b"replacement-model";
        fs::write(pack_dir.join("model.3ds"), replacement)?;
        let target = PackTarget {
            kind: "hull".into(),
            id: "wasp".into(),
            grade: "m3".into(),
        };
        let variant = PackVariant {
            id: "classic".into(),
            name: "Classic".into(),
            preview: None,
            resources: vec![PackResource {
                role: "model".into(),
                path: "model.3ds".into(),
                sha256: hash(replacement),
            }],
        };
        let pack = InstalledPack {
            path: pack_dir,
            manifest: PackManifest {
                format_version: 1,
                id: "test".into(),
                name: "Test".into(),
                version: "1".into(),
                author: String::new(),
                license: String::new(),
                games: vec!["air-cache".into()],
                items: vec![PackItem {
                    target: target.clone(),
                    variants: vec![variant.clone()],
                }],
            },
        };
        let scan = scan_cache(&cache, Some("air-cache"))?;
        let tx = apply_variant(&scan, &pack, &target, &variant, &root.join("backups"))?;
        assert_eq!(tx.changes.len(), 2);
        for (path, _) in &originals {
            assert_eq!(fs::read(path)?, replacement);
        }
        restore_transaction(&root.join("backups").join(&tx.id), &tx)?;
        for (path, old) in &originals {
            assert_eq!(fs::read(path)?, *old);
        }
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn each_restore_goes_back_one_replacement_until_the_original() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "tanki-apply-chain-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let cache = root.join("cache");
        let pack_dir = root.join("pack");
        let backups = root.join("backups");
        fs::create_dir_all(&cache)?;
        fs::create_dir_all(&pack_dir)?;
        let model = cache.join(STANDARD.encode("http://example.test/0/0/10/14/111/object.3ds"));
        fs::write(&model, b"original")?;
        let target = PackTarget {
            kind: "hull".into(),
            id: "wasp".into(),
            grade: "m3".into(),
        };
        let variant = |id: &str, bytes: &[u8]| -> Result<PackVariant> {
            fs::write(pack_dir.join(format!("{id}.3ds")), bytes)?;
            Ok(PackVariant {
                id: id.into(),
                name: id.into(),
                preview: None,
                resources: vec![PackResource {
                    role: "model".into(),
                    path: format!("{id}.3ds"),
                    sha256: hash(bytes),
                }],
            })
        };
        let (first, second) = (variant("first", b"first")?, variant("second", b"second")?);
        let pack = InstalledPack {
            path: pack_dir.clone(),
            manifest: PackManifest {
                format_version: 1,
                id: "test".into(),
                name: "Test".into(),
                version: "1".into(),
                author: String::new(),
                license: String::new(),
                games: vec!["air-cache".into()],
                items: vec![PackItem {
                    target: target.clone(),
                    variants: vec![first.clone(), second.clone()],
                }],
            },
        };
        let scan = scan_cache(&cache, Some("air-cache"))?;
        apply_variant(&scan, &pack, &target, &first, &backups)?;
        apply_variant(&scan, &pack, &target, &second, &backups)?;
        assert_eq!(fs::read(&model)?, b"second");

        let (dir, tx) = latest_transaction(&backups)?.expect("the second replacement");
        restore_transaction(&dir, &tx)?;
        assert_eq!(fs::read(&model)?, b"first", "back to the first skin");
        let (dir, tx) = latest_transaction(&backups)?.expect("the first replacement");
        restore_transaction(&dir, &tx)?;
        assert_eq!(fs::read(&model)?, b"original", "back to the original");
        assert!(
            latest_transaction(&backups)?.is_none(),
            "nothing left to restore"
        );
        assert!(list_backups(&backups)?.iter().all(|b| b.restored));
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
