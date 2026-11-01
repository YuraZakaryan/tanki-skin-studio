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
    /// The run of "apply to all items" this replacement was part of: its transactions are undone
    /// together and listed as one backup. Missing for a single replacement and in backups written
    /// by older versions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<Batch>,
    /// The modification or edition of the pack item the skin came from (`xt-m0`): a variant id
    /// repeats in every modification of a pack item, so this tells which one is worn. Missing in
    /// older backups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skin_grade: Option<String>,
}

/// What a transaction records beyond the replacement itself (see [`apply_variant_in`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyNote {
    pub batch: Option<Batch>,
    pub skin_grade: Option<String>,
}

/// One run that put a look on many items: every replacement of the run carries it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Batch {
    /// Unique per run (the time it started, in nanoseconds).
    pub id: String,
    /// The look the run put on, as the user saw it ("XT").
    pub name: String,
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
pub const SKIN_FILES: &[&str] = &[
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
    apply_variant_in(
        scan,
        pack,
        target,
        variant,
        backup_root,
        &ApplyNote::default(),
    )
}

/// A new backup folder named after `stamp` (nanoseconds), or the next free number when a folder of
/// that name exists: two replacements in the same clock tick (a batch on a coarse clock) must
/// never share a folder, or the second would overwrite the first one's originals. Ids keep their
/// length, so the folders still sort by time.
fn claim_backup_dir(backup_root: &Path, stamp: u128) -> Result<(String, PathBuf)> {
    fs::create_dir_all(backup_root)?;
    for offset in 0..1024_u128 {
        let id = format!("{}", stamp.saturating_add(offset));
        let dir = backup_root.join(&id);
        match fs::create_dir(&dir) {
            Ok(()) => return Ok((id, dir)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    anyhow::bail!("no free backup folder name")
}

/// [`apply_variant`] that records `note` with the transaction: the batch it is part of, so the
/// whole run can be undone together ([`restore_batch`]) and listed as one backup, and which pack
/// item the skin came from.
pub fn apply_variant_in(
    scan: &CacheScan,
    pack: &InstalledPack,
    target: &PackTarget,
    variant: &PackVariant,
    backup_root: &Path,
    note: &ApplyNote,
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
    let (id, dir) = claim_backup_dir(backup_root, stamp)?;
    let mut tx = Transaction {
        id,
        pack_id: pack.manifest.id.clone(),
        variant_id: variant.id.clone(),
        changes: Vec::new(),
        target: Some(target.clone()),
        variant_name: variant.name.clone(),
        batch: note.batch.clone(),
        skin_grade: note.skin_grade.clone(),
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

/// The saved original of one change, checked: a plain backup name, the bytes it recorded, and a
/// cache file still there to receive them.
fn checked_original(dir: &Path, change: &FileChange) -> Result<Vec<u8>> {
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
    Ok(bytes)
}

pub fn restore_transaction(dir: &Path, tx: &Transaction) -> Result<()> {
    for change in &tx.changes {
        let bytes = checked_original(dir, change)?;
        atomic_replace(&change.target, &bytes)?;
    }
    fs::write(dir.join(RESTORED), &tx.id).context("could not mark the backup as restored")?;
    Ok(())
}

/// Every backup not restored yet, newest first: the order that puts the cache back as it was
/// before the first of them.
pub fn pending_restores(backup_root: &Path) -> Result<Vec<BackupEntry>> {
    Ok(list_backups(backup_root)?
        .into_iter()
        .filter(|backup| !backup.restored)
        .collect())
}

/// A skin that is in the game cache right now.
#[derive(Clone, Debug)]
pub struct AppliedSkin {
    /// The cache item that wears it.
    pub target: PackTarget,
    pub pack_id: String,
    pub variant_id: String,
    pub variant_name: String,
    /// The pack item's modification or edition, when the backup recorded it.
    pub skin_grade: Option<String>,
    /// The batch the replacement was part of.
    pub batch: Option<String>,
}

/// The skins in the cache now: for each item, its newest replacement not restored yet, when every
/// file it wrote is still byte for byte what it wrote. A file changed since (the game updated it,
/// another tool) means the item no longer wears that skin.
pub fn applied_skins(backup_root: &Path) -> Result<Vec<AppliedSkin>> {
    let mut seen = BTreeSet::new();
    let mut applied = Vec::new();
    for backup in pending_restores(backup_root)? {
        let Some(target) = backup.tx.target.clone() else {
            continue;
        };
        if !seen.insert((target.kind.clone(), target.id.clone(), target.grade.clone())) {
            continue;
        }
        let intact = backup.tx.changes.iter().all(|change| {
            fs::read(&change.target).is_ok_and(|bytes| hash(&bytes) == change.after_sha256)
        });
        if intact {
            applied.push(AppliedSkin {
                target,
                pack_id: backup.tx.pack_id.clone(),
                variant_id: backup.tx.variant_id.clone(),
                variant_name: backup.tx.variant_name.clone(),
                skin_grade: backup.tx.skin_grade.clone(),
                batch: backup.tx.batch.as_ref().map(|b| b.id.clone()),
            });
        }
    }
    Ok(applied)
}

/// How much [`restore_all`] put back.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RestoreSummary {
    pub replacements: usize,
    pub files: usize,
}

/// Put the cache back as it was before every replacement not restored yet, newest first. Each
/// file's last write is the original from its oldest replacement. Every backup is checked before
/// anything is written, so a damaged one stops the restore with the cache untouched.
pub fn restore_all(backup_root: &Path) -> Result<RestoreSummary> {
    let pending = pending_restores(backup_root)?;
    for backup in &pending {
        for change in &backup.tx.changes {
            checked_original(&backup.dir, change)?;
        }
    }
    let mut summary = RestoreSummary::default();
    for backup in &pending {
        restore_transaction(&backup.dir, &backup.tx)?;
        summary.replacements += 1;
        summary.files += backup.tx.changes.len();
    }
    Ok(summary)
}

/// Undo one batch ([`apply_variant_in`]): put back what its items wore before it. A later
/// replacement of a file the batch wrote sits on top of the batch's, so it is undone too (first);
/// later replacements of other files stay. Newest first, and every backup is checked before
/// anything is written, as in [`restore_all`].
pub fn restore_batch(backup_root: &Path, batch: &str) -> Result<RestoreSummary> {
    let pending = pending_restores(backup_root)?;
    let in_batch = |backup: &BackupEntry| backup.tx.batch.as_ref().is_some_and(|b| b.id == batch);
    // Newest first: the batch's oldest replacement is its last one in the list.
    let Some(oldest) = pending.iter().rposition(in_batch) else {
        return Ok(RestoreSummary::default());
    };
    let mut touched: BTreeSet<&Path> = BTreeSet::new();
    let mut chosen = Vec::new();
    // From the oldest member up, so a replacement on top of one already chosen is found too.
    for backup in pending.iter().take(oldest.saturating_add(1)).rev() {
        let on_top = backup
            .tx
            .changes
            .iter()
            .any(|change| touched.contains(change.target.as_path()));
        if in_batch(backup) || on_top {
            touched.extend(backup.tx.changes.iter().map(|c| c.target.as_path()));
            chosen.push(backup);
        }
    }
    chosen.reverse();
    for backup in &chosen {
        for change in &backup.tx.changes {
            checked_original(&backup.dir, change)?;
        }
    }
    let mut summary = RestoreSummary::default();
    for backup in chosen {
        restore_transaction(&backup.dir, &backup.tx)?;
        summary.replacements += 1;
        summary.files += backup.tx.changes.len();
    }
    Ok(summary)
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

    #[test]
    fn restore_all_gives_back_the_original_and_a_damaged_backup_writes_nothing() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "tanki-apply-all-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let (cache, pack_dir, backups) =
            (root.join("cache"), root.join("pack"), root.join("backups"));
        fs::create_dir_all(&cache)?;
        fs::create_dir_all(&pack_dir)?;
        let model = cache.join(STANDARD.encode("http://example.test/0/0/10/14/111/object.3ds"));
        fs::write(&model, b"original")?;
        let target = PackTarget {
            kind: "hull".into(),
            id: "wasp".into(),
            grade: "m3".into(),
        };
        let mut variants = Vec::new();
        for id in ["one", "two", "three"] {
            fs::write(pack_dir.join(format!("{id}.3ds")), id)?;
            variants.push(PackVariant {
                id: id.into(),
                name: id.into(),
                preview: None,
                resources: vec![PackResource {
                    role: "model".into(),
                    path: format!("{id}.3ds"),
                    sha256: hash(id.as_bytes()),
                }],
            });
        }
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
                    variants: variants.clone(),
                }],
            },
        };
        let scan = scan_cache(&cache, Some("air-cache"))?;
        let mut first_dir = None;
        for variant in &variants {
            let tx = apply_variant(&scan, &pack, &target, variant, &backups)?;
            first_dir.get_or_insert(backups.join(&tx.id));
        }
        assert_eq!(fs::read(&model)?, b"three");
        assert_eq!(pending_restores(&backups)?.len(), 3);
        let applied = applied_skins(&backups)?;
        assert_eq!(applied.len(), 1, "one item, its newest skin");
        assert_eq!(
            (
                applied[0].variant_id.as_str(),
                applied[0].target.id.as_str()
            ),
            ("three", "wasp")
        );
        // A cache file changed by something else: the item no longer wears that skin.
        fs::write(&model, b"game update")?;
        assert!(applied_skins(&backups)?.is_empty());
        fs::write(&model, b"three")?;

        // A damaged oldest backup: nothing is written, not even the newer, healthy ones.
        let damaged = first_dir.expect("a backup").join("0000.bin");
        let good = fs::read(&damaged)?;
        fs::write(&damaged, b"tampered")?;
        assert!(restore_all(&backups).is_err());
        assert_eq!(fs::read(&model)?, b"three", "the cache is untouched");
        assert_eq!(pending_restores(&backups)?.len(), 3);

        fs::write(&damaged, good)?;
        let summary = restore_all(&backups)?;
        assert_eq!(
            summary,
            RestoreSummary {
                replacements: 3,
                files: 3
            }
        );
        assert_eq!(fs::read(&model)?, b"original", "every replacement undone");
        assert!(
            applied_skins(&backups)?.is_empty(),
            "nothing applied any more"
        );
        assert!(pending_restores(&backups)?.is_empty());
        assert_eq!(restore_all(&backups)?, RestoreSummary::default());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn two_replacements_in_one_clock_tick_get_folders_of_their_own() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "tanki-apply-tick-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let (first, first_dir) = claim_backup_dir(&root, 1_700_000_000_000_000_000)?;
        let (second, second_dir) = claim_backup_dir(&root, 1_700_000_000_000_000_000)?;
        assert_ne!(first_dir, second_dir, "a folder is never shared");
        assert_eq!(
            first.len(),
            second.len(),
            "ids keep their length, so they sort by time"
        );
        assert!(second > first);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn undoing_a_batch_undoes_what_sits_on_it_and_keeps_the_rest() -> Result<()> {
        let root = std::env::temp_dir().join(format!(
            "tanki-apply-batch-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
        let (cache, pack_dir, backups) =
            (root.join("cache"), root.join("pack"), root.join("backups"));
        fs::create_dir_all(&cache)?;
        fs::create_dir_all(&pack_dir)?;
        let wasp = cache.join(STANDARD.encode("http://example.test/0/0/10/14/111/object.3ds"));
        let rail = cache.join(STANDARD.encode("http://example.test/0/0/10/64/111/object.3ds"));
        fs::write(&wasp, b"wasp original")?;
        fs::write(&rail, b"rail original")?;
        let target = |kind: &str, id: &str| PackTarget {
            kind: kind.into(),
            id: id.into(),
            grade: "m3".into(),
        };
        let variant = |id: &str| -> Result<PackVariant> {
            fs::write(pack_dir.join(format!("{id}.3ds")), id)?;
            Ok(PackVariant {
                id: id.into(),
                name: id.into(),
                preview: None,
                resources: vec![PackResource {
                    role: "model".into(),
                    path: format!("{id}.3ds"),
                    sha256: hash(id.as_bytes()),
                }],
            })
        };
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
                items: Vec::new(),
            },
        };
        let scan = scan_cache(&cache, Some("air-cache"))?;
        let batch = Batch {
            id: "42".into(),
            name: "XT".into(),
        };
        let note = ApplyNote {
            batch: Some(batch.clone()),
            skin_grade: Some("xt-m3".into()),
        };
        let before_batch = variant("before")?;
        apply_variant(
            &scan,
            &pack,
            &target("turret", "railgun"),
            &before_batch,
            &backups,
        )?;
        let (xt_wasp, xt_rail) = (variant("xt-wasp")?, variant("xt-rail")?);
        apply_variant_in(
            &scan,
            &pack,
            &target("hull", "wasp"),
            &xt_wasp,
            &backups,
            &note,
        )?;
        apply_variant_in(
            &scan,
            &pack,
            &target("turret", "railgun"),
            &xt_rail,
            &backups,
            &note,
        )?;
        // Later, one by one: a skin on top of the batch's Wasp.
        apply_variant(
            &scan,
            &pack,
            &target("hull", "wasp"),
            &variant("late")?,
            &backups,
        )?;
        let tagged = list_backups(&backups)?
            .iter()
            .filter(|b| b.tx.batch.as_ref() == Some(&batch))
            .count();
        assert_eq!(tagged, 2, "both replacements of the run carry the batch");
        let worn = applied_skins(&backups)?;
        let rail_skin = worn
            .iter()
            .find(|s| s.target.id == "railgun")
            .expect("the Railgun wears the batch's skin");
        assert_eq!(rail_skin.batch.as_deref(), Some("42"));
        assert_eq!(rail_skin.skin_grade.as_deref(), Some("xt-m3"));

        assert_eq!(restore_batch(&backups, "nope")?, RestoreSummary::default());
        let summary = restore_batch(&backups, "42")?;
        assert_eq!(
            summary,
            RestoreSummary {
                replacements: 3,
                files: 3
            },
            "the two of the batch and the one on top of it"
        );
        assert_eq!(fs::read(&wasp)?, b"wasp original");
        assert_eq!(
            fs::read(&rail)?,
            b"before",
            "what the Railgun wore before the batch"
        );
        let pending = pending_restores(&backups)?;
        assert_eq!(pending.len(), 1, "the earlier replacement stays");
        assert_eq!(pending[0].tx.variant_id, "before");
        restore_all(&backups)?;
        assert_eq!(fs::read(&rail)?, b"rail original");
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn a_backup_without_a_batch_still_loads() -> Result<()> {
        let old = r#"{"id":"1","pack_id":"p","variant_id":"v","changes":[]}"#;
        let tx: Transaction = serde_json::from_str(old)?;
        assert!(tx.batch.is_none());
        let written = serde_json::to_string(&tx)?;
        assert!(
            !written.contains("batch"),
            "a single replacement writes no batch"
        );
        Ok(())
    }
}
