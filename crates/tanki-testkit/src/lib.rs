// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic fixtures shared by integration tests and `cargo xtask e2e-ui`.
//!
//! Everything here is generated at run time from flat colours, so the repository never
//! contains proprietary game models or textures.

use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage, codecs::jpeg::JpegEncoder};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
};

/// Cache URL markers of the two items of the built-in table.
pub const HULL_MARKER: &str = "0/0/10/14";
pub const TURRET_MARKER: &str = "0/0/10/64";
/// A hull the built-in adapter does not know: it can only be used after labelling it by hand.
pub const HORNET_MARKER: &str = "0/0/10/20";
pub const HORNET_MODEL: &[u8] = b"fixture-hornet-model";
/// Two cache revisions per item, like the real client keeps.
pub const REVISIONS: [&str; 2] = ["111", "222"];
const ROLES: [&str; 4] = [
    "object.3ds",
    "lightmap.jpg",
    "details.jpg",
    "details_alpha.jpg",
];

pub struct CacheFixture {
    pub root: PathBuf,
    pub hull_files: Vec<PathBuf>,
    pub turret_files: Vec<PathBuf>,
    pub hornet_files: Vec<PathBuf>,
    pub unrelated_files: Vec<PathBuf>,
}

impl CacheFixture {
    /// Every original file with its bytes, for before/after comparisons.
    pub fn snapshot(&self, files: &[PathBuf]) -> Vec<(PathBuf, Vec<u8>)> {
        files
            .iter()
            .map(|p| (p.clone(), fs::read(p).expect("fixture file readable")))
            .collect()
    }
}

pub fn sha256(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

pub fn jpeg(width: u32, height: u32, rgb: [u8; 3]) -> Vec<u8> {
    let image =
        DynamicImage::ImageRgb8(image::RgbImage::from_pixel(width, height, image::Rgb(rgb)));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 90)
        .encode_image(&image)
        .expect("encode jpeg");
    out
}

pub fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba(rgba));
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut out, ImageFormat::Png)
        .expect("encode png");
    out.into_inner()
}

fn write_cache_file(root: &Path, url: &str, bytes: &[u8]) -> Result<PathBuf> {
    let path = root.join(STANDARD.encode(url));
    fs::write(&path, bytes)?;
    Ok(path)
}

/// A cache laid out like the client's: hull and turret in two revisions each, plus unrelated resources.
pub fn build_cache(root: &Path) -> Result<CacheFixture> {
    fs::create_dir_all(root)?;
    // Scans return canonical paths (`/var` is a symlink on macOS), so fixtures must too.
    let root = &root.canonicalize()?;
    let mut fixture = CacheFixture {
        root: root.to_path_buf(),
        hull_files: Vec::new(),
        turret_files: Vec::new(),
        hornet_files: Vec::new(),
        unrelated_files: Vec::new(),
    };
    for (marker, files, tint) in [
        (HULL_MARKER, 0, 40_u8),
        (TURRET_MARKER, 1, 120_u8),
        (HORNET_MARKER, 2, 200_u8),
    ] {
        for revision in REVISIONS {
            for role in ROLES {
                let url = format!("http://cache.test/{marker}/{revision}/{role}");
                let bytes = if role.ends_with(".jpg") {
                    jpeg(16, 16, [tint, 60, 60])
                } else {
                    format!("original-{marker}-{revision}-{role}").into_bytes()
                };
                let path = write_cache_file(root, &url, &bytes)?;
                match files {
                    0 => fixture.hull_files.push(path),
                    1 => fixture.turret_files.push(path),
                    _ => fixture.hornet_files.push(path),
                }
            }
        }
    }
    for (url, bytes) in [
        (
            "http://cache.test/0/0/99/1/777/image.tara",
            b"archive".to_vec(),
        ),
        (
            "http://cache.test/0/0/99/2/888/sound.ogg",
            b"sound".to_vec(),
        ),
    ] {
        fixture
            .unrelated_files
            .push(write_cache_file(root, url, &bytes)?);
    }
    Ok(fixture)
}

/// Four consecutive even resource ids (M0..M3) starting at `first_id`, the way the real client
/// numbers one item's modifications. Returns the files of each modification in order.
pub fn build_item_block(root: &Path, first_id: u64) -> Result<Vec<Vec<PathBuf>>> {
    fs::create_dir_all(root)?;
    let root = &root.canonicalize()?;
    let mut block = Vec::new();
    for j in 0..4_u64 {
        let id = first_id + j * 2;
        let marker = format!(
            "{:o}/{:o}/{:o}/{:o}",
            id >> 32,
            (id >> 16) & 0xffff,
            (id >> 8) & 0xff,
            id & 0xff
        );
        let mut files = Vec::new();
        for revision in REVISIONS {
            for role in ROLES {
                let url = format!("http://cache.test/{marker}/{revision}/{role}");
                let bytes = if role.ends_with(".jpg") {
                    jpeg(16, 16, [90, 60, 60])
                } else {
                    format!("original-{id}-{revision}-{role}").into_bytes()
                };
                files.push(write_cache_file(root, &url, &bytes)?);
            }
        }
        block.push(files);
    }
    Ok(block)
}

pub struct PackOptions {
    pub id: &'static str,
    pub version: &'static str,
    pub games: &'static [&'static str],
    /// Lie about a resource checksum to exercise validation.
    pub corrupt_checksum: bool,
    /// Add a third item for the unknown Hornet hull.
    pub with_hornet: bool,
    /// Leave the model out of the Wasp skin (textures only, like an archive's incomplete
    /// edition), so it cannot be applied.
    pub hull_without_model: bool,
}

impl Default for PackOptions {
    fn default() -> Self {
        Self {
            id: "fixture-pack",
            version: "1.0.0",
            games: &["air-cache"],
            corrupt_checksum: false,
            with_hornet: false,
            hull_without_model: false,
        }
    }
}

/// Flat preview colours, so UI tests can tell which item a rendered cover shows.
pub const HULL_PREVIEW: [u8; 4] = [30, 160, 90, 255];
pub const TURRET_PREVIEW: [u8; 4] = [230, 140, 20, 255];

/// Replacement bytes the fixture pack ships, so tests can assert on cache contents.
pub const HULL_MODEL: &[u8] = b"fixture-hull-model";
pub const TURRET_MODEL: &[u8] = b"fixture-turret-model";

/// A two-item pack folder: the hull ships a PNG texture (converted on apply, alpha included), the
/// turret JPEGs with a separate alpha. Every skin is complete: it brings each model and texture
/// file the item has in the cache (only the preview is optional).
pub fn build_pack_dir(dir: &Path, options: &PackOptions) -> Result<PathBuf> {
    fs::create_dir_all(dir.join("hull"))?;
    fs::create_dir_all(dir.join("turret"))?;
    let hull_png = png(32, 32, [200, 30, 30, 255]);
    let turret_jpg = jpeg(16, 16, [30, 30, 200]);
    let lightmap = jpeg(16, 16, [120, 120, 120]);
    let alpha = jpeg(16, 16, [255, 255, 255]);
    let hull_preview = png(64, 48, HULL_PREVIEW);
    let turret_preview = png(64, 48, TURRET_PREVIEW);
    fs::write(dir.join("hull/model.3ds"), HULL_MODEL)?;
    fs::write(dir.join("hull/details.png"), &hull_png)?;
    fs::write(dir.join("hull/lightmap.jpg"), &lightmap)?;
    fs::write(dir.join("hull/preview.png"), &hull_preview)?;
    fs::write(dir.join("turret/model.3ds"), TURRET_MODEL)?;
    fs::write(dir.join("turret/details.jpg"), &turret_jpg)?;
    fs::write(dir.join("turret/details_alpha.jpg"), &alpha)?;
    fs::write(dir.join("turret/lightmap.jpg"), &lightmap)?;
    fs::write(dir.join("turret/preview.png"), &turret_preview)?;
    let hull_model_sum = if options.corrupt_checksum {
        "0".repeat(64)
    } else {
        sha256(HULL_MODEL)
    };
    let mut manifest = json!({
        "format_version": 1,
        "id": options.id,
        "name": "Fixture Pack",
        "version": options.version,
        "author": "tanki-testkit",
        "license": "CC0-1.0",
        "games": options.games,
        "items": [
            {
                "target": {"kind": "hull", "id": "wasp", "grade": "m3"},
                "variants": [{
                    "id": "red", "name": "Wasp Red", "preview": "hull/preview.png",
                    "resources": [
                        {"role": "model", "path": "hull/model.3ds", "sha256": hull_model_sum},
                        {"role": "details", "path": "hull/details.png", "sha256": sha256(&hull_png)},
                        {"role": "lightmap", "path": "hull/lightmap.jpg", "sha256": sha256(&lightmap)}
                    ]
                }]
            },
            {
                "target": {"kind": "turret", "id": "railgun", "grade": "m3"},
                "variants": [{
                    "id": "blue", "name": "Railgun Blue", "preview": "turret/preview.png",
                    "resources": [
                        {"role": "model", "path": "turret/model.3ds", "sha256": sha256(TURRET_MODEL)},
                        {"role": "details", "path": "turret/details.jpg", "sha256": sha256(&turret_jpg)},
                        {"role": "details_alpha", "path": "turret/details_alpha.jpg", "sha256": sha256(&alpha)},
                        {"role": "lightmap", "path": "turret/lightmap.jpg", "sha256": sha256(&lightmap)}
                    ]
                }]
            }
        ]
    });
    if options.hull_without_model {
        manifest["items"][0]["variants"][0]["resources"]
            .as_array_mut()
            .expect("hull resources")
            .retain(|resource| resource["role"] != "model");
    }
    if options.with_hornet {
        fs::create_dir_all(dir.join("hornet"))?;
        let details = jpeg(16, 16, [20, 200, 60]);
        fs::write(dir.join("hornet/model.3ds"), HORNET_MODEL)?;
        fs::write(dir.join("hornet/details.jpg"), &details)?;
        fs::write(dir.join("hornet/details_alpha.jpg"), &alpha)?;
        fs::write(dir.join("hornet/lightmap.jpg"), &lightmap)?;
        manifest["items"]
            .as_array_mut()
            .expect("items array")
            .push(json!({
                "target": {"kind": "hull", "id": "hornet", "grade": "m3"},
                "variants": [{
                    "id": "green", "name": "Hornet Green",
                    "resources": [
                        {"role": "model", "path": "hornet/model.3ds", "sha256": sha256(HORNET_MODEL)},
                        {"role": "details", "path": "hornet/details.jpg", "sha256": sha256(&details)},
                        {"role": "details_alpha", "path": "hornet/details_alpha.jpg", "sha256": sha256(&alpha)},
                        {"role": "lightmap", "path": "hornet/lightmap.jpg", "sha256": sha256(&lightmap)}
                    ]
                }]
            }));
    }
    fs::write(
        dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    Ok(dir.to_path_buf())
}

/// Zip a folder into a `.tankipack`; `extra` adds raw entries (used for hostile archives).
pub fn zip_dir(dir: &Path, out: &Path, extra: &[(&str, &[u8])]) -> Result<PathBuf> {
    let mut writer = zip::ZipWriter::new(fs::File::create(out)?);
    let options = zip::write::SimpleFileOptions::default();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = path.strip_prefix(dir)?.to_string_lossy().replace('\\', "/");
                writer.start_file(name, options)?;
                writer.write_all(&fs::read(&path)?)?;
            }
        }
    }
    for (name, bytes) in extra {
        writer.start_file(*name, options)?;
        writer.write_all(bytes)?;
    }
    writer.finish()?;
    Ok(out.to_path_buf())
}

/// A unique scratch directory under the system temp dir.
pub fn scratch(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("tanki-{label}-{}-{stamp}", std::process::id()));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// One item of [`build_looks`] in the cache: which modifications it has, and their files.
pub struct LooksItem {
    pub kind: &'static str,
    pub id: &'static str,
    /// Resource id of M0; M`j` is `first_id + 2 j`.
    pub first_id: u64,
    pub present: [bool; 4],
    /// The cache files of each modification (empty when it is not in the cache).
    pub files: [Vec<PathBuf>; 4],
}

impl LooksItem {
    /// Every cache file of the item.
    pub fn all_files(&self) -> Vec<PathBuf> {
        self.files.iter().flatten().cloned().collect()
    }
}

/// A cache and two packs whose skins share names across items and modifications, for "apply a
/// look to all items". Synthetic: every file is generated here.
pub struct LooksFixture {
    pub cache: PathBuf,
    pub items: Vec<LooksItem>,
    /// Pack folders, ready to import: "Looks A" (`looks-a`) and "Looks B" (`looks-b`).
    pub packs: Vec<PathBuf>,
    /// Hand labels that make the items known: (marker, kind, id, grade).
    pub labels: Vec<(String, String, String, String)>,
}

impl LooksFixture {
    pub fn item(&self, id: &str) -> &LooksItem {
        self.items
            .iter()
            .find(|item| item.id == id)
            .expect("an item of the fixture")
    }
}

/// The model bytes the looks fixture's skin of `item` at `grade` in `pack` ships, so a test can
/// tell which skin reached a cache file.
pub fn looks_model(pack: &str, item: &str, grade: &str) -> Vec<u8> {
    format!("{pack}:{item}:{grade}:model").into_bytes()
}

/// The cache marker of a resource id (`/0/0/40/0/`).
pub fn id_marker(id: u64) -> String {
    format!(
        "/{:o}/{:o}/{:o}/{:o}/",
        id >> 32,
        (id >> 16) & 0xffff,
        (id >> 8) & 0xff,
        id & 0xff
    )
}

/// The looks fixture:
///
/// - the cache has hulls Viking, Hunter (no M0) and Dictator, turrets Smoky and Thunder, each
///   modification a block of consecutive ids, known through [`LooksFixture::labels`];
/// - "Looks A" has XT for Viking M0–M3, Hunter M1–M3, Dictator M0–M3 (its M2 without a model, so
///   incomplete) and Smoky M3 only; Legacy for Viking and Hunter M3; Prime (an edition with no
///   modification) for Viking and Thunder; and Viking's plain M2 skin, which is no look;
/// - "Looks B" has XT for Smoky M0 and again Viking M3, and Desert Camo for Thunder M3.
pub fn build_looks(work: &Path) -> Result<LooksFixture> {
    let cache = work.join("cache");
    fs::create_dir_all(&cache)?;
    let cache = cache.canonicalize()?;
    let plan: [(&str, &str, u64, [bool; 4]); 5] = [
        ("hull", "viking", 16_384, [true; 4]),
        ("hull", "hunter", 16_400, [false, true, true, true]),
        ("hull", "dictator", 16_416, [true; 4]),
        ("turret", "smoky", 16_432, [true; 4]),
        ("turret", "thunder", 16_448, [true; 4]),
    ];
    let mut items = Vec::new();
    let mut labels = Vec::new();
    for (kind, id, first_id, present) in plan {
        let mut files: [Vec<PathBuf>; 4] = Default::default();
        for (j, slot) in files.iter_mut().enumerate() {
            if !present[j] {
                continue;
            }
            let resource = first_id + 2 * j as u64;
            let marker = id_marker(resource);
            labels.push((
                marker.clone(),
                kind.to_owned(),
                id.to_owned(),
                format!("m{j}"),
            ));
            for revision in REVISIONS {
                for role in ROLES {
                    let url = format!("http://cache.test{marker}{revision}/{role}");
                    let bytes = if role.ends_with(".jpg") {
                        jpeg(16, 16, [60, 60 + 20 * j as u8, 90])
                    } else {
                        format!("original-{id}-m{j}-{revision}-{role}").into_bytes()
                    };
                    slot.push(write_cache_file(&cache, &url, &bytes)?);
                }
            }
        }
        items.push(LooksItem {
            kind,
            id,
            first_id,
            present,
            files,
        });
    }
    // (kind, item, grade, variant id, variant name, with a model)
    type Skin = (
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        &'static str,
        bool,
    );
    let pack_a: &[Skin] = &[
        ("hull", "viking", "xt-m0", "default", "Viking", true),
        ("hull", "viking", "xt-m1", "default", "Viking", true),
        ("hull", "viking", "xt-m2", "default", "Viking", true),
        ("hull", "viking", "xt-m3", "default", "Viking", true),
        ("hull", "viking", "m3", "legacy", "Viking Legacy", true),
        ("hull", "viking", "prime", "default", "Viking", true),
        ("hull", "viking", "m2", "default", "Viking", true),
        ("hull", "hunter", "xt-m1", "default", "Hunter", true),
        ("hull", "hunter", "xt-m2", "default", "Hunter", true),
        ("hull", "hunter", "xt-m3", "default", "Hunter", true),
        ("hull", "hunter", "m3", "legacy", "Hunter Legacy", true),
        ("hull", "dictator", "xt-m0", "default", "Dictator", true),
        ("hull", "dictator", "xt-m1", "default", "Dictator", true),
        ("hull", "dictator", "xt-m2", "default", "Dictator", false),
        ("hull", "dictator", "xt-m3", "default", "Dictator", true),
        ("turret", "smoky", "xt-m3", "default", "Smoky", true),
        ("turret", "thunder", "prime", "default", "Thunder", true),
    ];
    let pack_b: &[Skin] = &[
        ("turret", "smoky", "xt-m0", "default", "Smoky", true),
        ("hull", "viking", "xt-m3", "default", "Viking", true),
        (
            "turret",
            "thunder",
            "m3",
            "desert-camo",
            "Desert Camo",
            true,
        ),
    ];
    let mut packs = Vec::new();
    for (pack_id, name, skins) in [
        ("looks-a", "Looks A", pack_a),
        ("looks-b", "Looks B", pack_b),
    ] {
        let dir = work.join(pack_id);
        fs::create_dir_all(&dir)?;
        let details = jpeg(16, 16, [180, 40, 40]);
        let alpha = jpeg(16, 16, [255, 255, 255]);
        let lightmap = jpeg(16, 16, [120, 120, 120]);
        let mut manifest_items = Vec::new();
        for &(kind, item, grade, variant, variant_name, with_model) in skins {
            let folder = format!("{item}/{grade}/{variant}");
            fs::create_dir_all(dir.join(&folder))?;
            let model = looks_model(pack_id, item, grade);
            let mut resources = Vec::new();
            if with_model {
                fs::write(dir.join(&folder).join("model.3ds"), &model)?;
                resources.push(json!({"role": "model", "path": format!("{folder}/model.3ds"), "sha256": sha256(&model)}));
            }
            for (role, file, bytes) in [
                ("details", "details.jpg", &details),
                ("details_alpha", "details_alpha.jpg", &alpha),
                ("lightmap", "lightmap.jpg", &lightmap),
            ] {
                fs::write(dir.join(&folder).join(file), bytes)?;
                resources.push(json!({"role": role, "path": format!("{folder}/{file}"), "sha256": sha256(bytes)}));
            }
            manifest_items.push(json!({
                "target": {"kind": kind, "id": item, "grade": grade},
                "variants": [{"id": variant, "name": variant_name, "resources": resources}]
            }));
        }
        let manifest = json!({
            "format_version": 1,
            "id": pack_id,
            "name": name,
            "version": "1.0.0",
            "author": "tanki-testkit",
            "license": "CC0-1.0",
            "games": ["air-cache"],
            "items": manifest_items,
        });
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec_pretty(&manifest)?,
        )?;
        packs.push(dir);
    }
    Ok(LooksFixture {
        cache,
        items,
        packs,
        labels,
    })
}
