// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The files that make an item's look (model, details and their alpha, lightmap, or a paint's
//! picture), from the cache as the game shows it or from a pack variant. A paint has no model of
//! its own: it is seen on a hull ([`LookSource::Painted`]). Finding the files is cheap and done on
//! the interface thread; reading them is left to a background thread.

use std::path::PathBuf;

use anyhow::{Context, Result};
use tanki_cache::cache::{CacheScan, known_target_group};
use tanki_pack::pack::{InstalledPack, PackTarget, PackVariant, read_pack_resource};

/// Files larger than this are not read: no item model or texture comes near it.
const MAX_FILE: u64 = 64 << 20;

/// What a look is made of, read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LookFiles {
    pub model: Vec<u8>,
    /// JPEG or PNG; a PNG carries its own alpha.
    pub details: Option<Vec<u8>>,
    pub details_alpha: Option<Vec<u8>>,
    pub lightmap: Option<Vec<u8>>,
    /// A paint's picture (JPEG, a still or a sheet of frames).
    pub paint: Option<Vec<u8>>,
}

/// Where a look's files are.
#[derive(Clone, Debug)]
pub enum LookSource {
    /// Cache files by role (`object.3ds`, `details.jpg`, …), with their sizes.
    Cache(Vec<(String, PathBuf, u64)>),
    Pack {
        pack: Box<InstalledPack>,
        variant: Box<PackVariant>,
    },
    /// A body (a hull) wearing a paint: the body's model and textures, the paint's picture.
    Painted {
        body: Box<LookSource>,
        paint: Box<LookSource>,
    },
}

impl LookSource {
    /// Names the files as they are now: the same key means the same picture. A cache file
    /// replaced by a skin changes size; a pack resource is named by its checksum.
    pub fn key(&self) -> String {
        match self {
            LookSource::Cache(files) => {
                let parts: Vec<String> = files
                    .iter()
                    .map(|(_, path, size)| format!("{}#{size}", path.display()))
                    .collect();
                format!("cache:{}", parts.join("|"))
            }
            LookSource::Pack { pack, variant } => {
                let parts: Vec<&str> = variant
                    .resources
                    .iter()
                    .map(|r| r.sha256.as_str())
                    .collect();
                format!(
                    "pack:{}:{}:{}",
                    pack.path.display(),
                    variant.id,
                    parts.join("|")
                )
            }
            LookSource::Painted { body, paint } => {
                format!("painted:{}+{}", body.key(), paint.key())
            }
        }
    }

    /// Read the files. A look with neither a model nor a paint is an error; a missing texture is
    /// not.
    pub fn read(&self) -> Result<LookFiles> {
        let mut look = LookFiles::default();
        match self {
            LookSource::Painted { body, paint } => {
                look = body.read()?;
                look.paint = paint.read()?.paint;
            }
            LookSource::Cache(files) => {
                for (role, path, _) in files {
                    let size = std::fs::metadata(path)
                        .with_context(|| format!("reading {}", path.display()))?
                        .len();
                    anyhow::ensure!(size <= MAX_FILE, "{} is too large", path.display());
                    let bytes = std::fs::read(path)?;
                    match role.as_str() {
                        "object.3ds" => look.model = bytes,
                        "details.jpg" => look.details = Some(bytes),
                        "details_alpha.jpg" => look.details_alpha = Some(bytes),
                        "lightmap.jpg" => look.lightmap = Some(bytes),
                        "image.jpg" => look.paint = Some(bytes),
                        _ => {}
                    }
                }
            }
            LookSource::Pack { pack, variant } => {
                for resource in &variant.resources {
                    let bytes = read_pack_resource(pack, &resource.path)?;
                    anyhow::ensure!(
                        bytes.len() as u64 <= MAX_FILE,
                        "{} is too large",
                        resource.path
                    );
                    match resource.role.as_str() {
                        "model" => look.model = bytes,
                        "details" => look.details = Some(bytes),
                        "details_alpha" => look.details_alpha = Some(bytes),
                        "lightmap" => look.lightmap = Some(bytes),
                        "image" => look.paint = Some(bytes),
                        _ => {}
                    }
                }
            }
        }
        anyhow::ensure!(
            !look.model.is_empty() || look.paint.is_some(),
            "the look has no model and no paint"
        );
        Ok(look)
    }
}

/// The item's look in the cache: the files of its newest revision. `None` when the cache has
/// neither a model nor a paint picture for it.
pub fn cache_look(scan: &CacheScan, target: &PackTarget) -> Option<LookSource> {
    let wanted = Some((
        target.kind.as_str(),
        target.id.as_str(),
        target.grade.as_str(),
    ));
    let mine: Vec<_> = scan
        .resources
        .iter()
        .filter(|r| known_target_group(&scan.game, &r.group) == wanted)
        .collect();
    let newest = mine
        .iter()
        .filter(|r| matches!(r.role.as_str(), "object.3ds" | "image.jpg"))
        .map(|r| r.group.as_str())
        .max_by_key(|group| (revision(group), *group))?;
    let files = mine
        .iter()
        .filter(|r| r.group == newest)
        .filter(|r| {
            matches!(
                r.role.as_str(),
                "object.3ds" | "details.jpg" | "details_alpha.jpg" | "lightmap.jpg" | "image.jpg"
            )
        })
        .map(|r| (r.role.clone(), r.path.clone(), r.size))
        .collect();
    Some(LookSource::Cache(files))
}

/// A pack variant's look. `None` when the variant brings neither a model nor a paint picture.
pub fn variant_look(pack: &InstalledPack, variant: &PackVariant) -> Option<LookSource> {
    variant
        .resources
        .iter()
        .any(|r| matches!(r.role.as_str(), "model" | "image"))
        .then(|| LookSource::Pack {
            pack: Box::new(pack.clone()),
            variant: Box::new(variant.clone()),
        })
}

/// The revision a cache group ends with (`/0/0/10/14/222/` is 222), 0 when it ends otherwise.
fn revision(group: &str) -> u64 {
    group
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|last| last.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests;
