//! Build a pack from a folder of models and textures, whatever its layout.
//!
//! The whole tree is read, to any depth. Every folder that holds a model or a picture is read from
//! its path ([`layout`]): which hull or turret, which modification or edition, which skin. Its
//! files are given their roles by name and content ([`roles`]). Folders that turn out to be the
//! same skin (`M3/model` and `M3/textures`) are joined. What could not be used is reported
//! ([`ArchiveSummary`]), so an archive laid out or named in an unexpected way is noticed instead
//! of silently importing less.

mod layout;
mod roles;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::pack::{PackItem, PackManifest, PackResource, PackTarget, PackVariant, load_pack};

use layout::{Identity, interpret, slug};
use roles::{UNSUPPORTED, classify, ext, files_of, is_asset, is_junk, name_of};

/// Entries read when telling whether a folder is an archive at all (a choice of a big folder
/// must stay quick), and when building the pack.
const SURVEY_LIMIT: usize = 20_000;
const BUILD_LIMIT: usize = 500_000;

/// What was found in an archive, and what could not be used: the import report.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchiveSummary {
    pub items: usize,
    pub variants: usize,
    /// Variants that came without a picture.
    pub without_preview: usize,
    /// Files of a variant that were not recognised as its model, a texture or its preview (paths
    /// relative to the archive).
    pub unused: Vec<String>,
    /// Files in formats the client cannot use (`a3d`, `ktx`, `webp`...), by extension.
    pub unsupported: BTreeMap<String, usize>,
    /// Folders with models or pictures in which no hull or turret was recognised.
    pub unrecognised: Vec<String>,
    /// Folders recognised by an item name alone, left out because the archive has real hull or
    /// turret folders (`Misc/Overdrives/Wasp` is an effect, not a hull).
    pub skipped: Vec<String>,
    /// Guesses worth checking: misspelt names, items the catalog does not know, previews chosen
    /// without a preview name.
    pub notes: Vec<String>,
    /// The archive was larger than what is read; the rest was not imported.
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct GroupKey {
    /// The folder named after the item: folders of one skin join only below the same item folder,
    /// never across copies of an archive (`Flash/...` and `2.0/...`).
    item_dir: PathBuf,
    kind: &'static str,
    id: String,
    grade: String,
    variant: String,
}

struct Group {
    identity: Identity,
    dirs: Vec<PathBuf>,
}

#[derive(Default)]
struct Survey {
    groups: BTreeMap<GroupKey, Group>,
    unrecognised: Vec<String>,
    skipped: Vec<String>,
    notes: BTreeSet<String>,
    truncated: bool,
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// Read the tree under `root` (hidden folders and symbolic links are not followed).
fn survey(root: &Path, limit: usize) -> Survey {
    let root_name = name_of(root);
    let mut out = Survey::default();
    let mut seen = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut has_assets = false;
        for entry in entries.flatten() {
            seen += 1;
            if seen > limit {
                out.truncated = true;
                break;
            }
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push(entry.path());
            } else if kind.is_file() && is_asset(&entry.path()) {
                has_assets = true;
            }
        }
        if out.truncated {
            break;
        }
        if !has_assets {
            continue;
        }
        let mut parts = vec![root_name.clone()];
        parts.extend(
            dir.strip_prefix(root)
                .unwrap_or(&dir)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned()),
        );
        let Some(identity) = interpret(&parts) else {
            out.unrecognised.push(relative(root, &dir));
            continue;
        };
        if let Some(note) = &identity.note {
            out.notes
                .insert(format!("{}: {note}", relative(root, &dir)));
        }
        let item_dir = parts[1..=identity.depth]
            .iter()
            .fold(root.to_path_buf(), |path, part| path.join(part));
        let key = GroupKey {
            item_dir,
            kind: identity.kind.code(),
            id: identity.id.clone(),
            grade: identity.grade.clone(),
            variant: identity.variant.clone(),
        };
        out.groups
            .entry(key)
            .or_insert_with(|| Group {
                identity,
                dirs: Vec::new(),
            })
            .dirs
            .push(dir);
    }
    if out.groups.values().any(|g| g.identity.strong) {
        let Survey {
            groups, skipped, ..
        } = &mut out;
        groups.retain(|_, group| {
            if !group.identity.strong {
                skipped.extend(group.dirs.iter().map(|d| relative(root, d)));
            }
            group.identity.strong
        });
    }
    for group in out.groups.values_mut() {
        group.dirs.sort();
    }
    out.unrecognised.sort();
    out.skipped.sort();
    out
}

/// Does `source` hold hull or turret models or textures (and is it not a pack with a manifest)?
pub fn is_model_archive(source: &Path) -> bool {
    if !source.is_dir() || source.join("manifest.json").exists() {
        return false;
    }
    let Ok(root) = source.canonicalize() else {
        return false;
    };
    survey(&root, SURVEY_LIMIT).groups.values().any(|group| {
        group
            .dirs
            .iter()
            .any(|dir| !classify(&files_of(dir)).is_empty())
    })
}

fn copy_resource(from: &Path, pack_dir: &Path, relative: &str, role: &str) -> Result<PackResource> {
    let target = pack_dir.join(relative);
    fs::create_dir_all(target.parent().context("pack path has a parent")?)?;
    let bytes = fs::read(from).with_context(|| format!("read {}", from.display()))?;
    fs::write(&target, &bytes)?;
    Ok(PackResource {
        role: role.to_owned(),
        path: relative.to_owned(),
        sha256: format!("{:x}", Sha256::digest(&bytes)),
    })
}

/// Write a pack folder for the archive at `source` into `pack_dir` (created; it must be empty or
/// absent) and validate it with [`load_pack`]. The result is ready for `import_pack`. The pack is
/// named after the folder and versioned by its content, so importing the same archive again is
/// refused and an archive that changed comes in as a new version.
pub fn build_model_pack(source: &Path, pack_dir: &Path) -> Result<ArchiveSummary> {
    anyhow::ensure!(source.is_dir(), "{} is not a folder", source.display());
    let root = source
        .canonicalize()
        .with_context(|| format!("cannot open {}", source.display()))?;
    let survey = survey(&root, BUILD_LIMIT);
    fs::create_dir_all(pack_dir)?;
    let mut summary = ArchiveSummary {
        unrecognised: survey.unrecognised,
        skipped: survey.skipped,
        truncated: survey.truncated,
        ..ArchiveSummary::default()
    };
    let mut notes = survey.notes;
    // (kind, id, grade) -> variants, in the order found.
    let mut items: BTreeMap<(String, String, String), Vec<PackVariant>> = BTreeMap::new();
    for (key, group) in survey.groups {
        let files: Vec<PathBuf> = group.dirs.iter().flat_map(|dir| files_of(dir)).collect();
        for file in &files {
            let extension = ext(file);
            if UNSUPPORTED.contains(&extension.as_str()) {
                *summary.unsupported.entry(extension).or_default() += 1;
            }
        }
        let picked = classify(&files);
        if picked.is_empty() {
            continue;
        }
        let variants = items
            .entry((key.kind.to_owned(), key.id.clone(), key.grade.clone()))
            .or_default();
        // The same skin found in two places (two copies of an archive): both are kept, numbered.
        let (mut variant, mut variant_name) =
            (key.variant.clone(), group.identity.variant_name.clone());
        let mut n = 2;
        while variants.iter().any(|v| v.id == variant) {
            variant = format!("{}-{n}", key.variant);
            variant_name = format!("{} ({n})", group.identity.variant_name);
            n += 1;
        }
        let base = format!("assets/{}/{}/{}/{}", key.kind, key.id, key.grade, variant);
        let details_name = picked
            .details
            .as_deref()
            .map(|p| format!("details.{}", ext(p)))
            .unwrap_or_default();
        let mut resources = Vec::new();
        for (role, path, stored) in [
            ("model", &picked.model, "object.3ds"),
            ("lightmap", &picked.lightmap, "lightmap.jpg"),
            ("details", &picked.details, details_name.as_str()),
            ("details_alpha", &picked.details_alpha, "details_alpha.jpg"),
        ] {
            if let Some(path) = path {
                resources.push(copy_resource(
                    path,
                    pack_dir,
                    &format!("{base}/{stored}"),
                    role,
                )?);
            }
        }
        let preview = match &picked.preview {
            Some(path) => {
                let stored = format!("{base}/preview.{}", ext(path));
                copy_resource(path, pack_dir, &stored, "preview")?;
                if picked.preview_guessed {
                    notes.insert(format!("{}: taken as the preview", relative(&root, path)));
                }
                Some(stored)
            }
            None => {
                summary.without_preview += 1;
                None
            }
        };
        summary.unused.extend(
            files
                .iter()
                .filter(|f| {
                    !picked.uses(f) && !is_junk(f) && !UNSUPPORTED.contains(&ext(f).as_str())
                })
                .map(|f| relative(&root, f)),
        );
        variants.push(PackVariant {
            id: variant,
            name: variant_name,
            preview,
            resources,
        });
    }
    anyhow::ensure!(
        !items.is_empty(),
        "no hull or turret models or textures were found in {}",
        source.display()
    );
    summary.items = items.len();
    summary.variants = items.values().map(Vec::len).sum();
    summary.unused.sort();
    summary.notes = notes.into_iter().collect();

    let mut content = Sha256::new();
    for ((kind, id, grade), variants) in &items {
        content.update(format!("{kind}/{id}/{grade}"));
        for variant in variants {
            content.update(&variant.id);
            content.update(variant.preview.as_deref().unwrap_or_default());
            for resource in &variant.resources {
                content.update(&resource.sha256);
            }
        }
    }
    let name = name_of(&root);
    let manifest = PackManifest {
        format_version: 1,
        id: match slug(&name) {
            empty if empty.is_empty() => "model-archive".to_owned(),
            id => id,
        },
        name,
        version: format!("{:x}", content.finalize())[..12].to_owned(),
        author: String::new(),
        license: "unspecified".into(),
        games: vec!["air-cache".into()],
        items: items
            .into_iter()
            .map(|((kind, id, grade), variants)| PackItem {
                target: PackTarget { kind, id, grade },
                variants,
            })
            .collect(),
    };
    fs::write(
        pack_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    load_pack(pack_dir).context("the pack built from the archive does not validate")?;
    Ok(summary)
}
