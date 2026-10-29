//! Which file of a variant folder is the model, which the textures and which the preview.
//!
//! The usual names come first (`object.3ds`, `lightmap.jpg`, `details.png`, `preview.png`); after
//! them the content and the name decide: a model is a 3DS by its signature whatever it is
//! called, a texture by a word in its name (`lightmaps.jpg`, `Wasp_diffuse.png`), the preview by
//! being the one remaining picture or the one of the size the garage shows. Anything ambiguous is
//! left out and reported: a wrong file is worse than none.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

/// A garage preview is this size (the picture the game shows).
const PREVIEW_SIZE: (u32, u32) = (164, 106);

/// Formats of other Tanki clients (HTML5, 2.0) and of 3D tools: real assets, but the Flash client
/// cannot use them, so they are counted in the report instead of listed one by one.
pub(super) const UNSUPPORTED: &[&str] = &[
    "a3d", "atf", "ktx", "webp", "glb", "gltf", "obj", "mtl", "dae", "fbx", "dds", "tga", "bmp",
];

const DETAIL_WORDS: &[&str] = &["details", "detail", "diffuse", "albedo", "basecolor"];
const ALPHA_WORDS: &[&str] = &["alpha", "opacity"];
/// Pictures that are maps of a model, never its preview.
const MAP_WORDS: &[&str] = &[
    "track",
    "wheel",
    "normal",
    "spec",
    "rough",
    "metal",
    "bump",
    "height",
    "mask",
    "alpha",
    "opacity",
    "lightmap",
    "detail",
    "diffuse",
    "albedo",
    "colormap",
    "surface",
    "occlusion",
];

pub(super) fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

pub(super) fn ext(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn has_word(path: &Path, words: &[&str]) -> bool {
    let stem = stem(path);
    words.iter().any(|w| stem.contains(w))
}

fn is_image(path: &Path) -> bool {
    matches!(ext(path).as_str(), "png" | "jpg" | "jpeg")
}

fn is_jpeg(path: &Path) -> bool {
    matches!(ext(path).as_str(), "jpg" | "jpeg")
}

/// A file that makes its folder worth reading: a model or a picture, supported or not.
pub(super) fn is_asset(path: &Path) -> bool {
    let ext = ext(path);
    matches!(ext.as_str(), "3ds" | "png" | "jpg" | "jpeg") || UNSUPPORTED.contains(&ext.as_str())
}

/// Files the system leaves in folders.
pub(super) fn is_junk(path: &Path) -> bool {
    let name = name_of(path).to_ascii_lowercase();
    matches!(name.as_str(), ".ds_store" | "thumbs.db" | "desktop.ini")
}

pub(super) fn files_of(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .collect();
    files.sort();
    files
}

/// A 3DS file starts with the main chunk `4D4D`, whose length is at most the size of the file.
fn is_3ds(path: &Path) -> bool {
    let mut head = [0u8; 6];
    let Ok(size) = fs::metadata(path).map(|m| m.len()) else {
        return false;
    };
    if fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_err()
    {
        return false;
    }
    let length = u64::from(u32::from_le_bytes([head[2], head[3], head[4], head[5]]));
    head[..2] == [0x4D, 0x4D] && (6..=size).contains(&length)
}

/// Width and height of a PNG, from its header.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = [0u8; 24];
    fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    (head[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]).then(|| {
        (
            u32::from_be_bytes([head[16], head[17], head[18], head[19]]),
            u32::from_be_bytes([head[20], head[21], head[22], head[23]]),
        )
    })
}

fn only(candidates: impl Iterator<Item = PathBuf>) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = candidates.collect();
    (found.len() == 1).then(|| found.remove(0))
}

/// The files chosen for each role of a variant.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Picked {
    pub model: Option<PathBuf>,
    pub lightmap: Option<PathBuf>,
    pub details: Option<PathBuf>,
    pub details_alpha: Option<PathBuf>,
    pub preview: Option<PathBuf>,
    /// The preview is a picture without a preview name (the only one left, or the only one of the
    /// preview size): worth a line in the report.
    pub preview_guessed: bool,
}

impl Picked {
    /// Nothing that can be put in the cache (a preview alone is not a skin).
    pub(super) fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.lightmap.is_none()
            && self.details.is_none()
            && self.details_alpha.is_none()
    }

    pub(super) fn uses(&self, path: &Path) -> bool {
        [
            &self.model,
            &self.lightmap,
            &self.details,
            &self.details_alpha,
            &self.preview,
        ]
        .iter()
        .any(|picked| picked.as_deref() == Some(path))
    }
}

/// Choose the file of each role among the files of one variant (one folder, or the folders of
/// one skin together: `M3/model/object.3ds` and `M3/textures/details.png`).
pub(super) fn classify(files: &[PathBuf]) -> Picked {
    let named = |name: &str| {
        files
            .iter()
            .find(|p| name_of(p).eq_ignore_ascii_case(name))
            .cloned()
    };
    let model = named("object.3ds").or_else(|| only(files.iter().filter(|p| is_3ds(p)).cloned()));
    let lightmap = named("lightmap.jpg").or_else(|| {
        only(
            files
                .iter()
                .filter(|p| is_jpeg(p) && has_word(p, &["lightmap"]) && !has_word(p, ALPHA_WORDS))
                .cloned(),
        )
    });
    let details_alpha = named("details_alpha.jpg").or_else(|| {
        only(
            files
                .iter()
                .filter(|p| is_jpeg(p) && has_word(p, ALPHA_WORDS) && has_word(p, DETAIL_WORDS))
                .cloned(),
        )
    });
    let details = named("details.png")
        .or_else(|| named("details.jpg"))
        .or_else(|| {
            only(
                files
                    .iter()
                    .filter(|p| {
                        is_image(p)
                            && has_word(p, DETAIL_WORDS)
                            && !has_word(p, ALPHA_WORDS)
                            && !has_word(p, &["lightmap", "track", "wheel"])
                    })
                    .cloned(),
            )
        });

    let taken = [&model, &lightmap, &details, &details_alpha];
    let rest: Vec<PathBuf> = files
        .iter()
        .filter(|p| is_image(p) && !taken.iter().any(|t| t.as_deref() == Some(p.as_path())))
        .cloned()
        .collect();
    let pick = |candidates: Vec<PathBuf>| {
        if candidates.len() == 1 {
            candidates.into_iter().next()
        } else {
            only(
                candidates
                    .into_iter()
                    .filter(|p| png_size(p) == Some(PREVIEW_SIZE)),
            )
        }
    };
    let (preview, preview_guessed) = match named("preview.png") {
        Some(path) => (Some(path), false),
        None => {
            let named_like: Vec<PathBuf> = rest
                .iter()
                .filter(|p| has_word(p, &["preview", "icon", "thumb"]))
                .cloned()
                .collect();
            if named_like.is_empty() {
                let plain = rest
                    .iter()
                    .filter(|p| !has_word(p, MAP_WORDS))
                    .cloned()
                    .collect();
                let preview = pick(plain);
                let guessed = preview.is_some();
                (preview, guessed)
            } else {
                (pick(named_like), false)
            }
        }
    };
    Picked {
        model,
        lightmap,
        details,
        details_alpha,
        preview,
        preview_guessed,
    }
}
