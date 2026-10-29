// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;
use zip::ZipArchive;

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_PACK: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackManifest {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    #[serde(default)]
    pub games: Vec<String>,
    #[serde(default)]
    pub items: Vec<PackItem>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackItem {
    pub target: PackTarget,
    pub variants: Vec<PackVariant>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackTarget {
    pub kind: String,
    pub id: String,
    pub grade: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackVariant {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub preview: Option<String>,
    pub resources: Vec<PackResource>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PackResource {
    pub role: String,
    pub path: String,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledPack {
    pub path: PathBuf,
    pub manifest: PackManifest,
}

fn safe_relative(s: &str) -> Result<()> {
    let p = Path::new(s);
    anyhow::ensure!(!s.is_empty() && !p.is_absolute(), "invalid pack path: {s}");
    anyhow::ensure!(
        !s.contains('\\') && !s.contains(':') && !s.starts_with('/'),
        "unsafe cross-platform pack path: {s}"
    );
    anyhow::ensure!(
        p.components().all(|c| matches!(c, Component::Normal(_))),
        "unsafe pack path: {s}"
    );
    Ok(())
}
fn safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn read_from_source(source: &Path, relative: &str) -> Result<Vec<u8>> {
    safe_relative(relative)?;
    if source.is_dir() {
        let path = source.join(relative);
        let canonical = path
            .canonicalize()
            .with_context(|| format!("missing pack file {relative}"))?;
        anyhow::ensure!(
            canonical.starts_with(source.canonicalize()?),
            "pack path escapes root: {relative}"
        );
        let meta = canonical.metadata()?;
        anyhow::ensure!(
            meta.is_file() && meta.len() <= MAX_FILE,
            "invalid/large pack file: {relative}"
        );
        Ok(fs::read(canonical)?)
    } else {
        let file = File::open(source)?;
        let mut archive = ZipArchive::new(file)?;
        let mut entry = archive
            .by_name(relative)
            .with_context(|| format!("missing pack file {relative}"))?;
        anyhow::ensure!(
            entry.is_file() && entry.size() <= MAX_FILE,
            "invalid/large pack file: {relative}"
        );
        let mut data = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut data)?;
        Ok(data)
    }
}

pub fn load_pack(source: &Path) -> Result<PackManifest> {
    anyhow::ensure!(
        source.is_dir() || source.is_file(),
        "pack not found: {}",
        source.display()
    );
    if source.is_file() {
        anyhow::ensure!(
            source.metadata()?.len() <= MAX_PACK,
            "pack archive too large"
        );
        let mut archive = ZipArchive::new(File::open(source)?)?;
        let mut seen = HashSet::new();
        let mut total = 0_u64;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name()?.into_owned();
            safe_relative(name.trim_end_matches('/'))?;
            anyhow::ensure!(seen.insert(name.clone()), "duplicate pack entry: {name}");
            anyhow::ensure!(entry.size() <= MAX_FILE, "pack entry too large");
            total = total.saturating_add(entry.size());
            anyhow::ensure!(total <= MAX_PACK, "pack too large");
        }
    }
    let bytes = read_from_source(source, "manifest.json")?;
    anyhow::ensure!(bytes.len() <= 1024 * 1024, "manifest too large");
    let manifest: PackManifest = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(manifest.format_version == 1, "unsupported pack format");
    anyhow::ensure!(
        safe_id(&manifest.id) && safe_id(&manifest.version),
        "invalid pack ID/version"
    );
    anyhow::ensure!(!manifest.name.trim().is_empty(), "pack name missing");
    anyhow::ensure!(!manifest.items.is_empty(), "pack has no items");
    for item in &manifest.items {
        anyhow::ensure!(
            safe_id(&item.target.kind) && safe_id(&item.target.id) && safe_id(&item.target.grade),
            "invalid target"
        );
        anyhow::ensure!(!item.variants.is_empty(), "item has no variants");
        for variant in &item.variants {
            anyhow::ensure!(safe_id(&variant.id), "invalid variant ID");
            if let Some(preview) = &variant.preview {
                read_from_source(source, preview)?;
            }
            anyhow::ensure!(!variant.resources.is_empty(), "variant has no resources");
            let mut roles = HashSet::new();
            for resource in &variant.resources {
                anyhow::ensure!(
                    safe_id(&resource.role) && roles.insert(resource.role.as_str()),
                    "invalid/duplicate resource role"
                );
                anyhow::ensure!(
                    resource.sha256.len() == 64
                        && resource.sha256.bytes().all(|b| b.is_ascii_hexdigit()),
                    "invalid sha256"
                );
                let data = read_from_source(source, &resource.path)?;
                anyhow::ensure!(
                    hash(&data).eq_ignore_ascii_case(&resource.sha256),
                    "checksum mismatch: {}",
                    resource.path
                );
            }
        }
    }
    Ok(manifest)
}

/// What is wrong with one file of a pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackIssueKind {
    /// The source (or a referenced file) does not exist or cannot be read.
    Missing,
    /// A path that could write outside the pack (`../x`, absolute, drive letters, backslashes).
    InvalidPath,
    /// Two archive entries with the same name.
    DuplicateEntry,
    /// A file larger than the limits.
    TooLarge,
    /// The file's SHA-256 does not match the manifest.
    ChecksumMismatch,
    /// `manifest.json` is not valid or breaks the pack format rules.
    InvalidManifest,
}

impl PackIssueKind {
    /// Stable English description; the UI translates by this text.
    pub fn describe(self) -> &'static str {
        match self {
            Self::Missing => "Missing file",
            Self::InvalidPath => "Invalid path",
            Self::DuplicateEntry => "Duplicate entry",
            Self::TooLarge => "File too large",
            Self::ChecksumMismatch => "Checksum mismatch",
            Self::InvalidManifest => "Invalid manifest",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackIssue {
    pub kind: PackIssueKind,
    pub file: String,
}

/// The error of an import that was refused: every problem found, so the UI can list them all.
#[derive(Debug)]
pub struct PackRejected {
    pub issues: Vec<PackIssue>,
}

impl std::fmt::Display for PackRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pack failed validation")?;
        for issue in &self.issues {
            write!(
                f,
                "; {}: {}",
                issue.kind.describe().to_lowercase(),
                issue.file
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for PackRejected {}

/// Scan a pack for every problem instead of stopping at the first one. An empty list means the
/// manifest and every referenced file passed the checks that can be listed per file; [`load_pack`]
/// stays the final gate for the structural rules.
pub fn pack_issues(source: &Path) -> Vec<PackIssue> {
    let issue = |kind, file: &str| PackIssue {
        kind,
        file: file.to_owned(),
    };
    let label = source
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let mut issues = Vec::new();
    if !(source.is_dir() || source.is_file()) {
        return vec![issue(PackIssueKind::Missing, &label)];
    }
    if source.is_file() {
        let Ok(file) = File::open(source) else {
            return vec![issue(PackIssueKind::Missing, &label)];
        };
        let Ok(mut archive) = ZipArchive::new(file) else {
            return vec![issue(PackIssueKind::InvalidManifest, &label)];
        };
        let mut seen = HashSet::new();
        for i in 0..archive.len() {
            let Ok(entry) = archive.by_index(i) else {
                continue;
            };
            let Ok(name) = entry.name().map(|n| n.into_owned()) else {
                issues.push(issue(PackIssueKind::InvalidPath, &format!("entry {i}")));
                continue;
            };
            if safe_relative(name.trim_end_matches('/')).is_err() {
                issues.push(issue(PackIssueKind::InvalidPath, &name));
            } else if !seen.insert(name.clone()) {
                issues.push(issue(PackIssueKind::DuplicateEntry, &name));
            } else if entry.size() > MAX_FILE {
                issues.push(issue(PackIssueKind::TooLarge, &name));
            }
        }
    }
    let manifest = match read_from_source(source, "manifest.json") {
        Ok(bytes) => serde_json::from_slice::<PackManifest>(&bytes).ok(),
        Err(_) => {
            issues.push(issue(PackIssueKind::Missing, "manifest.json"));
            return issues;
        }
    };
    let Some(manifest) = manifest else {
        issues.push(issue(PackIssueKind::InvalidManifest, "manifest.json"));
        return issues;
    };
    for item in &manifest.items {
        for variant in &item.variants {
            let previews = variant.preview.iter().map(|p| (p.as_str(), None));
            let resources = variant
                .resources
                .iter()
                .map(|r| (r.path.as_str(), Some(r.sha256.as_str())));
            for (path, sha256) in previews.chain(resources) {
                if safe_relative(path).is_err() {
                    issues.push(issue(PackIssueKind::InvalidPath, path));
                    continue;
                }
                match read_from_source(source, path) {
                    Ok(data) => {
                        if let Some(expected) = sha256
                            && !hash(&data).eq_ignore_ascii_case(expected)
                        {
                            issues.push(issue(PackIssueKind::ChecksumMismatch, path));
                        }
                    }
                    Err(_) => issues.push(issue(PackIssueKind::Missing, path)),
                }
            }
        }
    }
    issues.dedup();
    issues
}

pub fn read_pack_resource(pack: &InstalledPack, relative: &str) -> Result<Vec<u8>> {
    read_from_source(&pack.path, relative)
}

pub fn import_pack(source: &Path, store: &Path) -> Result<InstalledPack> {
    let issues = pack_issues(source);
    if !issues.is_empty() {
        return Err(PackRejected { issues }.into());
    }
    let manifest = load_pack(source)?;
    fs::create_dir_all(store)?;
    let file_name = format!("{}-{}", manifest.id, manifest.version);
    let destination = if source.is_dir() {
        store.join(&file_name)
    } else {
        store.join(format!("{file_name}.tankipack"))
    };
    if destination.exists() {
        bail!("pack version is already imported: {}", manifest.name);
    }
    let staging = store.join(format!(
        ".import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let result = (|| -> Result<()> {
        if source.is_dir() {
            fs::create_dir(&staging)?;
            for entry in WalkDir::new(source).follow_links(false) {
                let entry = entry?;
                let relative = entry.path().strip_prefix(source)?;
                if relative.as_os_str().is_empty() {
                    continue;
                }
                anyhow::ensure!(!entry.file_type().is_symlink(), "pack symlink rejected");
                let target = staging.join(relative);
                if entry.file_type().is_dir() {
                    fs::create_dir_all(target)?;
                } else if entry.file_type().is_file() {
                    fs::create_dir_all(target.parent().unwrap_or(&staging))?;
                    fs::copy(entry.path(), target)?;
                }
            }
        } else {
            fs::copy(source, &staging)?;
        }
        load_pack(&staging)?;
        fs::rename(&staging, &destination)?;
        Ok(())
    })();
    if result.is_err() {
        if staging.is_dir() {
            let _ = fs::remove_dir_all(&staging);
        } else {
            let _ = fs::remove_file(&staging);
        }
    }
    result?;
    Ok(InstalledPack {
        path: destination,
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_missing_source_is_one_missing_issue() {
        let issues = pack_issues(Path::new("/definitely/not/here.zip"));
        assert_eq!(
            issues,
            vec![PackIssue {
                kind: PackIssueKind::Missing,
                file: "here.zip".into()
            }]
        );
    }

    #[test]
    fn rejection_text_names_every_problem() {
        let rejected = PackRejected {
            issues: vec![
                PackIssue {
                    kind: PackIssueKind::ChecksumMismatch,
                    file: "details.jpg".into(),
                },
                PackIssue {
                    kind: PackIssueKind::InvalidPath,
                    file: "../outside.png".into(),
                },
            ],
        };
        let text = rejected.to_string();
        assert!(text.contains("checksum mismatch: details.jpg"), "{text}");
        assert!(text.contains("invalid path: ../outside.png"), "{text}");
    }

    #[test]
    fn refuses_traversal() {
        assert!(safe_relative("../escape.png").is_err());
        assert!(safe_relative("assets\\escape.png").is_err());
        assert!(safe_relative("C:escape.png").is_err());
        assert!(safe_relative("assets/skin.png").is_ok());
    }
}
