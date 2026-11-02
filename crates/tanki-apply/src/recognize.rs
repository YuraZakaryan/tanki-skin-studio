// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Recognise items in a cache by what is in it, with no hand labelling.
//!
//! The client names a resource by a numeric id the app has no table for, but a pack ships the model
//! (and usually the lightmap) of every item it covers, and the cache holds the originals of the same
//! files. When a cache resource's `object.3ds` is byte-for-byte a pack's model, it *is* that item;
//! one model is shared by all modifications of an item, so the modification is told by the lightmap
//! (which differs per modification). The garage preview of every item is cached as a picture and
//! the packs ship the same picture, so the preview names the item *and* its modification; the client
//! stores it on the id right after the model's. Only unambiguous matches are reported.
//!
//! A paint is one picture (`image.jpg`, alone on its id). The cache keeps the server's copy, saved
//! again, so a pack's paint is found by how it looks, not by its bytes: the same size, and the same
//! picture shrunk to a few pixels. Plain colours and twin pictures are left out: a picture that
//! looks like two paints, or a paint that looks like two pictures, names neither.

use std::collections::{BTreeMap, BTreeSet};

use tanki_cache::{
    cache::CacheScan,
    targets::{identify, marker_for_group},
};
use tanki_pack::pack::{InstalledPack, read_pack_resource};

use crate::geometry::{Fingerprint, fingerprint};

/// A cache resource id that was matched to an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recognition {
    /// The `/a/b/c/d/` marker of the cache resource.
    pub marker: String,
    pub kind: String,
    pub id: String,
    pub grade: String,
    /// What decided the modification: `preview` (the garage preview stored next to the model is the
    /// pack's preview of that modification), `lightmap` (this resource's own lightmap is the pack's
    /// lightmap of that modification) or `block order` (it sits at that position in a block of
    /// consecutive ids of the same item, anchored by a resource recognised directly).
    pub evidence: &'static str,
}

type Target = (String, String, String);

fn hash(bytes: &[u8]) -> String {
    tanki_pack::pack::sha256_hex(bytes)
}

/// Garage previews are 164 x 106 pictures; the cache keeps the colour in `image.jpg` and the
/// transparency in `alpha.jpg`.
const PREVIEW_SIZE: (u32, u32) = (164, 106);

/// How far (mean absolute difference per channel, 0..255) two previews may be to count as the same
/// picture, and how much farther the best other modification must be. The same preview stored as
/// PNG and as JPEG differs by about 1; different modifications by more than 4.
const SAME_PREVIEW: f32 = 3.0;
const PREVIEW_MARGIN: f32 = 2.0;

/// The preview as RGB over black, or `None` when it is not a garage preview.
fn preview_pixels(colour: &[u8], alpha: Option<&[u8]>) -> Option<Vec<u8>> {
    let rgb = image::load_from_memory(colour).ok()?.to_rgb8();
    if rgb.dimensions() != PREVIEW_SIZE {
        return None;
    }
    let alpha = alpha
        .and_then(|bytes| image::load_from_memory(bytes).ok())
        .map(|a| a.to_luma8())
        .filter(|a| a.dimensions() == PREVIEW_SIZE);
    let mut out = Vec::with_capacity(rgb.as_raw().len());
    for (x, y, pixel) in rgb.enumerate_pixels() {
        let opacity = alpha
            .as_ref()
            .map_or(1.0, |a| f32::from(a.get_pixel(x, y).0[0]) / 255.0);
        out.extend(pixel.0.iter().map(|c| (f32::from(*c) * opacity) as u8));
    }
    Some(out)
}

fn preview_distance(a: &[u8], b: &[u8]) -> f32 {
    let total: u64 = a
        .iter()
        .zip(b)
        .map(|(x, y)| u64::from(x.abs_diff(*y)))
        .sum();
    total as f32 / a.len().max(1) as f32
}

/// The colour texture and the lightmap of a modification, shrunk to a few pixels. A pack ships the
/// textures as PNG at the size of its source and the cache as JPEG, so they are never the same bytes
/// but a modification still looks like itself and not like its siblings.
#[derive(Clone, Default)]
struct Look {
    details: Option<Vec<u8>>,
    lightmap: Option<Vec<u8>>,
}

const LOOK_SIDE: u32 = 16;

/// Mean absolute difference per channel (0..255) at which two looks are the same modification, and
/// how much farther the best other modification of the item must be. Measured on a real cache: the
/// same texture differs by under 0.3 (PNG against JPEG), any other modification by more than 5.
const SAME_LOOK: f32 = 3.0;
const LOOK_MARGIN: f32 = 4.0;

fn tiny(bytes: &[u8], grey: bool) -> Option<Vec<u8>> {
    let small = image::load_from_memory(bytes).ok()?.resize_exact(
        LOOK_SIDE,
        LOOK_SIDE,
        image::imageops::FilterType::Triangle,
    );
    Some(if grey {
        small.to_luma8().into_raw()
    } else {
        small.to_rgb8().into_raw()
    })
}

/// Mean absolute difference over the textures both looks have, or `None` when they share none.
fn look_distance(a: &Look, b: &Look) -> Option<f32> {
    let parts: Vec<f32> = [(&a.details, &b.details), (&a.lightmap, &b.lightmap)]
        .into_iter()
        .filter_map(|(x, y)| Some(preview_distance(x.as_ref()?, y.as_ref()?)))
        .collect();
    (!parts.is_empty()).then(|| parts.iter().sum::<f32>() / parts.len() as f32)
}

/// Which items ship a file with this hash, for the `model` and `lightmap` roles, which items have a
/// mesh with these counts, and their previews and looks.
#[derive(Default)]
struct Index {
    models: BTreeMap<String, BTreeSet<Target>>,
    geometries: BTreeMap<Fingerprint, BTreeSet<Target>>,
    lightmaps: BTreeMap<String, BTreeSet<Target>>,
    previews: Vec<(Target, Vec<u8>)>,
    looks: Vec<(Target, Look)>,
}

fn index(packs: &[InstalledPack]) -> Index {
    let mut index = Index::default();
    for pack in packs {
        for item in &pack.manifest.items {
            let target = (
                item.target.kind.clone(),
                item.target.id.clone(),
                item.target.grade.clone(),
            );
            for variant in &item.variants {
                if let Some(pixels) = variant
                    .preview
                    .as_ref()
                    .and_then(|path| read_pack_resource(pack, path).ok())
                    .and_then(|bytes| preview_pixels(&bytes, None))
                {
                    index.previews.push((target.clone(), pixels));
                }
                let mut look = Look::default();
                for resource in &variant.resources {
                    let Ok(bytes) = read_pack_resource(pack, &resource.path) else {
                        continue;
                    };
                    match resource.role.as_str() {
                        "model" => {
                            index
                                .models
                                .entry(hash(&bytes))
                                .or_default()
                                .insert(target.clone());
                            if let Some(geometry) = fingerprint(&bytes) {
                                index
                                    .geometries
                                    .entry(geometry)
                                    .or_default()
                                    .insert(target.clone());
                            }
                        }
                        "lightmap" => {
                            index
                                .lightmaps
                                .entry(hash(&bytes))
                                .or_default()
                                .insert(target.clone());
                            look.lightmap = tiny(&bytes, true);
                        }
                        "details" => look.details = tiny(&bytes, false),
                        _ => {}
                    }
                }
                if look.details.is_some() || look.lightmap.is_some() {
                    index.looks.push((target.clone(), look));
                }
            }
        }
    }
    index
}

/// The hashes of the model and lightmap files found for one cache marker (every revision counts),
/// the geometry of its models and how it looks.
#[derive(Default)]
struct Found {
    models: BTreeSet<String>,
    geometries: BTreeSet<Fingerprint>,
    lightmaps: BTreeSet<String>,
    look: Look,
}

/// The numeric resource id of a `/a/b/c/d/` marker (four octal numbers, packed as the client does).
fn marker_number(marker: &str) -> Option<u64> {
    let parts: Vec<u64> = marker
        .trim_matches('/')
        .split('/')
        .map(|p| u64::from_str_radix(p, 8).ok())
        .collect::<Option<_>>()?;
    // A cache file name is outside data: numbers beyond the client's own ranges are no marker (and
    // would overflow the arithmetic on ids below).
    match parts[..] {
        [a, b, c, d] if a <= 0xffff_ffff && b <= 0xffff && c <= 0xff && d <= 0xff => {
            Some((a << 32) | (b << 16) | (c << 8) | d)
        }
        _ => None,
    }
}

fn grade_number(grade: &str) -> Option<i32> {
    match grade {
        "m0" => Some(0),
        "m1" => Some(1),
        "m2" => Some(2),
        "m3" => Some(3),
        _ => None,
    }
}

/// One cache resource whose item is known, and its modification if its own lightmap told it.
struct Member {
    marker: String,
    number: u64,
    direct: Option<i32>,
    /// What `direct` came from (`preview`, `lightmap` or `look`).
    evidence: &'static str,
    /// Already recognised (built in or labelled): an anchor for its neighbours, never reported.
    known: bool,
}

/// The item and modification whose preview is `pixels`, if exactly one is clearly the closest.
/// The same modification shipped twice (old and new look) counts once.
fn target_by_preview(index: &Index, pixels: &[u8]) -> Option<Target> {
    let mut best: BTreeMap<&Target, f32> = BTreeMap::new();
    for (target, reference) in &index.previews {
        let distance = preview_distance(pixels, reference);
        let entry = best.entry(target).or_insert(f32::MAX);
        *entry = entry.min(distance);
    }
    let mut ranked: Vec<(&Target, f32)> = best.into_iter().collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (target, distance) = *ranked.first()?;
    let clear = ranked
        .get(1)
        .is_none_or(|(_, next)| next - distance >= PREVIEW_MARGIN);
    (distance <= SAME_PREVIEW && clear).then(|| target.clone())
}

/// The modification of `item` whose textures the given look is clearly closest to.
fn grade_by_look(index: &Index, item: &(String, String), look: &Look) -> Option<i32> {
    let mut best: BTreeMap<i32, f32> = BTreeMap::new();
    for ((kind, id, grade), reference) in &index.looks {
        if (kind, id) != (&item.0, &item.1) {
            continue;
        }
        let (Some(grade), Some(distance)) = (grade_number(grade), look_distance(look, reference))
        else {
            continue;
        };
        let entry = best.entry(grade).or_insert(f32::MAX);
        *entry = entry.min(distance);
    }
    let mut ranked: Vec<(i32, f32)> = best.into_iter().collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (grade, distance) = *ranked.first()?;
    let clear = ranked
        .get(1)
        .is_none_or(|(_, next)| next - distance >= LOOK_MARGIN);
    (distance <= SAME_LOOK && clear).then_some(grade)
}

/// Match the cache's resources against what the packs ship: hulls, turrets and paints. Resources
/// that are already recognised (built in or labelled) are skipped, and so is everything that would
/// be a guess.
pub fn recognise_by_content(scan: &CacheScan, packs: &[InstalledPack]) -> Vec<Recognition> {
    let mut found = recognise_items(scan, packs);
    found.extend(recognise_paints(scan, packs));
    found.sort_by(|a, b| a.marker.cmp(&b.marker));
    found
}

/// How far (mean absolute difference per channel, 0..255, of the pictures shrunk to
/// [`LOOK_SIDE`]) a cache picture may be from a pack's paint of its size and still be it, and how
/// much farther the next paint must be. Measured on a real cache against a public pack: a paint
/// saved again by the server differs from the pack's by under 1, other pictures by more than 4.
const SAME_PAINT: f32 = 2.5;
const PAINT_MARGIN: f32 = 2.0;
/// The same for a paint's garage preview, shrunk to [`PAINT_PREVIEW`]. Measured on the same cache:
/// a paint's preview differs from the pack's by at most 1.1, any other paint's by 3.7 or more. The
/// server saves some paints anew (a little brighter or darker), so the preview names them when the
/// picture alone is too far.
const SAME_PAINT_PREVIEW: f32 = 3.0;
const PAINT_PREVIEW_MARGIN: f32 = 2.0;
const PAINT_PREVIEW: (u32, u32) = (32, 20);

/// The size of a picture, read from its header alone.
fn picture_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// A garage preview shrunk to [`PAINT_PREVIEW`], black where it is transparent: its own alpha (a
/// PNG) or the cache's alpha picture beside it. Previews come in a few sizes, so they are compared
/// shrunk.
fn preview_look(colour: &[u8], alpha: Option<&[u8]>) -> Option<Vec<u8>> {
    let mut rgba = image::load_from_memory(colour).ok()?.to_rgba8();
    if let Some(alpha) = alpha
        .and_then(|bytes| image::load_from_memory(bytes).ok())
        .map(|a| a.to_luma8())
        .filter(|a| a.dimensions() == rgba.dimensions())
    {
        for (x, y, pixel) in rgba.enumerate_pixels_mut() {
            pixel.0[3] = alpha.get_pixel(x, y).0[0];
        }
    }
    let over_black = image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let scale = |c: u8| (u16::from(c) * u16::from(a) / 255) as u8;
        image::Rgb([scale(r), scale(g), scale(b)])
    });
    Some(
        image::DynamicImage::ImageRgb8(over_black)
            .resize_exact(
                PAINT_PREVIEW.0,
                PAINT_PREVIEW.1,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8()
            .into_raw(),
    )
}

/// Whether a cache picture and its alpha are a paint's garage preview: a card about 164 × 106,
/// opaque for the most part. Effects are stored the same way (a texture, then a picture with its
/// alpha), but their pictures are glows: square or thin, and mostly transparent. Measured on a real
/// cache: paint previews 163-166 × 97-106 and 59-69 % opaque, effects 128 × 128 or 14 × 311 and
/// 10-28 % opaque.
fn is_paint_preview(colour: &[u8], alpha: &[u8]) -> bool {
    let shape = picture_size(colour)
        .is_some_and(|(w, h)| (150..=180).contains(&w) && (90..=115).contains(&h));
    shape
        && image::load_from_memory(alpha).is_ok_and(|a| {
            let gray = a.to_luma8();
            let total: u64 = gray.pixels().map(|p| u64::from(p.0[0])).sum();
            total as f64 / (255.0 * f64::from(gray.width() * gray.height()).max(1.0)) >= 0.45
        })
}

/// A paint of the packs, as it is compared with the cache.
struct PackPaint {
    id: String,
    /// The size of its picture and the picture shrunk.
    picture: Option<((u32, u32), Vec<u8>)>,
    preview: Option<Vec<u8>>,
}

/// The paints of the packs; the same paint in two packs counts once.
fn pack_paints(packs: &[InstalledPack]) -> Vec<PackPaint> {
    let mut out: Vec<PackPaint> = Vec::new();
    for pack in packs {
        for item in pack
            .manifest
            .items
            .iter()
            .filter(|i| i.target.kind == "paint")
        {
            if out.iter().any(|p| p.id == item.target.id) {
                continue;
            }
            let picture = item
                .variants
                .iter()
                .flat_map(|v| &v.resources)
                .find(|r| r.role == "image")
                .and_then(|r| read_pack_resource(pack, &r.path).ok())
                .and_then(|bytes| Some((picture_size(&bytes)?, tiny(&bytes, false)?)));
            let preview = item
                .variants
                .iter()
                .find_map(|v| v.preview.as_ref())
                .and_then(|path| read_pack_resource(pack, path).ok())
                .and_then(|bytes| preview_look(&bytes, None));
            out.push(PackPaint {
                id: item.target.id.clone(),
                picture,
                preview,
            });
        }
    }
    out
}

/// The one paint clearly closest to `look` among `candidates` (id and look), if close enough.
fn closest<'a>(
    look: &[u8],
    candidates: impl Iterator<Item = (&'a str, &'a [u8])>,
    same: f32,
    margin: f32,
) -> Option<&'a str> {
    let mut ranked: Vec<(&str, f32)> = candidates
        .map(|(id, reference)| (id, preview_distance(look, reference)))
        .collect();
    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    let (id, distance) = *ranked.first()?;
    let clear = ranked
        .get(1)
        .is_none_or(|(_, next)| next - distance >= margin);
    (distance <= same && clear).then_some(id)
}

/// The id of a paint the cache has and no pack names: `unnamed-` and its resource id.
pub fn unnamed_paint(number: u64) -> String {
    format!("unnamed-{number}")
}

/// Paints in the cache (see the module docs). A paint is a picture alone on its id; when the game
/// shows it in its garage, its preview sits on the next id. A paint is named by its picture (a
/// pack's paint of the same size that looks the same) or by its preview (a pack's paint whose
/// preview looks the same; the picture decides between paints whose previews are alike), and the
/// two must not disagree. A paint with a preview that no pack names is still a paint: it is listed
/// unnamed, and any paint can take its place.
fn recognise_paints(scan: &CacheScan, packs: &[InstalledPack]) -> Vec<Recognition> {
    let paints = pack_paints(packs);
    // The cache's groups by resource id, every revision.
    let mut ids: BTreeMap<u64, Vec<Vec<&tanki_cache::cache::CachedResource>>> = BTreeMap::new();
    let mut groups: BTreeMap<&str, Vec<&tanki_cache::cache::CachedResource>> = BTreeMap::new();
    for resource in &scan.resources {
        groups
            .entry(resource.group.as_str())
            .or_default()
            .push(resource);
    }
    for (group, files) in groups {
        if let Some(number) = marker_for_group(group).and_then(|m| marker_number(&m)) {
            ids.entry(number).or_default().push(files);
        }
    }
    let alone = |files: &Vec<&tanki_cache::cache::CachedResource>| matches!(&files[..], [picture] if picture.role == "image.jpg");
    let mut verdicts: BTreeMap<u64, (String, Option<String>, &'static str)> = BTreeMap::new();
    for (number, revisions) in &ids {
        if !revisions.iter().all(alone)
            || revisions
                .iter()
                .any(|files| identify(&scan.game, &files[0].group).is_some())
        {
            continue;
        }
        let Some(marker) = marker_for_group(&revisions[0][0].group) else {
            continue;
        };
        // The preview on the next id: a colour picture and its alpha.
        let preview = number
            .checked_add(1)
            .and_then(|next| ids.get(&next))
            .and_then(|next| {
                next.iter().find_map(|files| {
                    let colour = files.iter().find(|r| r.role == "image.jpg")?;
                    let alpha = files.iter().find(|r| r.role == "alpha.jpg")?;
                    (files.len() == 2).then_some((colour.path.clone(), alpha.path.clone()))
                })
            })
            .and_then(|(colour, alpha)| {
                let (colour, alpha) = (std::fs::read(colour).ok()?, std::fs::read(alpha).ok()?);
                is_paint_preview(&colour, &alpha)
                    .then(|| preview_look(&colour, Some(&alpha)))
                    .flatten()
            });
        // Every revision's picture must name the same paint (or none).
        let mut by_picture = BTreeSet::new();
        let mut first = None;
        for files in revisions {
            let Ok(bytes) = std::fs::read(&files[0].path) else {
                continue;
            };
            let found = picture_size(&bytes)
                .zip(tiny(&bytes, false))
                .and_then(|(size, look)| {
                    let same_size = paints.iter().filter_map(|p| {
                        p.picture
                            .as_ref()
                            .filter(|(s, _)| *s == size)
                            .map(|(_, l)| (p.id.as_str(), l.as_slice()))
                    });
                    closest(&look, same_size, SAME_PAINT, PAINT_MARGIN).map(str::to_owned)
                });
            if first.is_none() {
                first = picture_size(&bytes).zip(tiny(&bytes, false));
            }
            by_picture.insert(found);
        }
        let by_picture = match &by_picture.into_iter().collect::<Vec<_>>()[..] {
            [one] => one.clone(),
            _ => continue,
        };
        let by_preview = preview.as_ref().and_then(|look| {
            let with_preview = paints
                .iter()
                .filter_map(|p| p.preview.as_ref().map(|v| (p.id.as_str(), v.as_slice())));
            closest(
                look,
                with_preview.clone(),
                SAME_PAINT_PREVIEW,
                PAINT_PREVIEW_MARGIN,
            )
            .map(str::to_owned)
            .or_else(|| {
                // Paints whose previews are alike: the picture decides among them, by look
                // alone (the server may save it at another quality).
                let (size, picture) = first.as_ref()?;
                let alike: Vec<&PackPaint> = paints
                    .iter()
                    .filter(|p| {
                        p.preview
                            .as_ref()
                            .is_some_and(|v| preview_distance(look, v) <= SAME_PAINT_PREVIEW)
                    })
                    .collect();
                let same_size = alike.iter().filter_map(|p| {
                    p.picture
                        .as_ref()
                        .filter(|(s, _)| s == size)
                        .map(|(_, l)| (p.id.as_str(), l.as_slice()))
                });
                closest(picture, same_size, f32::MAX, PAINT_MARGIN).map(str::to_owned)
            })
        });
        let verdict = match (by_picture, by_preview) {
            // The picture and the preview name two paints: a paint still, of no name.
            (Some(a), Some(b)) if a != b => (marker, None, "preview"),
            (Some(id), _) => (marker, Some(id), "picture"),
            (None, Some(id)) => (marker, Some(id), "preview"),
            (None, None) if preview.is_some() => (marker, None, "preview"),
            (None, None) => continue,
        };
        verdicts.insert(*number, verdict);
    }
    // A pack's paint found on two ids names neither: both stay paints, unnamed.
    let mut found_on: BTreeMap<String, usize> = BTreeMap::new();
    for (_, id, _) in verdicts.values() {
        if let Some(id) = id {
            *found_on.entry(id.clone()).or_default() += 1;
        }
    }
    verdicts
        .into_iter()
        .map(|(number, (marker, id, evidence))| {
            let named = id.filter(|id| found_on.get(id) == Some(&1));
            Recognition {
                marker,
                kind: "paint".to_owned(),
                evidence: if named.is_some() { evidence } else { "preview" },
                id: named.unwrap_or_else(|| unnamed_paint(number)),
                grade: "m0".to_owned(),
            }
        })
        .collect()
}

/// Hulls and turrets in the cache (see the module docs).
fn recognise_items(scan: &CacheScan, packs: &[InstalledPack]) -> Vec<Recognition> {
    let index = index(packs);
    if index.models.is_empty()
        && index.geometries.is_empty()
        && index.lightmaps.is_empty()
        && index.previews.is_empty()
    {
        return Vec::new();
    }
    let mut markers: BTreeMap<String, Found> = BTreeMap::new();
    // Previews by the numeric id they are stored on, every revision of them.
    let mut previews: BTreeMap<u64, Vec<Vec<u8>>> = BTreeMap::new();
    for resource in scan.resources.iter().filter(|r| r.role == "image.jpg") {
        let Some(number) = marker_for_group(&resource.group).and_then(|m| marker_number(&m)) else {
            continue;
        };
        let alpha = scan
            .resources
            .iter()
            .find(|r| r.group == resource.group && r.role == "alpha.jpg")
            .and_then(|r| std::fs::read(&r.path).ok());
        let Ok(colour) = std::fs::read(&resource.path) else {
            continue;
        };
        if let Some(pixels) = preview_pixels(&colour, alpha.as_deref()) {
            previews.entry(number).or_default().push(pixels);
        }
    }
    let mut known_markers: BTreeMap<String, (String, String, i32)> = BTreeMap::new();
    for resource in &scan.resources {
        if !matches!(
            resource.role.as_str(),
            "object.3ds" | "lightmap.jpg" | "details.jpg"
        ) {
            continue;
        }
        let Some(marker) = marker_for_group(&resource.group) else {
            continue;
        };
        if let Some(known) = identify(&scan.game, &resource.group) {
            if let Some(grade) = grade_number(known.grade) {
                known_markers.insert(marker, (known.kind.to_owned(), known.id.to_owned(), grade));
            }
            continue;
        }
        let Ok(bytes) = std::fs::read(&resource.path) else {
            continue;
        };
        let found = markers.entry(marker).or_default();
        match resource.role.as_str() {
            "object.3ds" => {
                found.models.insert(hash(&bytes));
                found.geometries.extend(fingerprint(&bytes));
            }
            "lightmap.jpg" => {
                found.lightmaps.insert(hash(&bytes));
                if found.look.lightmap.is_none() {
                    found.look.lightmap = tiny(&bytes, true);
                }
            }
            _ => {
                if found.look.details.is_none() {
                    found.look.details = tiny(&bytes, false);
                }
            }
        }
    }

    // 1. Which item is each resource? The model is the same for every modification of an item, so
    //    it names the item; a lightmap, if known, must agree and additionally names the modification.
    let mut items: BTreeMap<(String, String), Vec<Member>> = BTreeMap::new();
    for (marker, found) in markers {
        let by_model: BTreeSet<&Target> = found
            .models
            .iter()
            .filter_map(|h| index.models.get(h))
            .flatten()
            .collect();
        let by_geometry: BTreeSet<&Target> = found
            .geometries
            .iter()
            .filter_map(|g| index.geometries.get(g))
            .flatten()
            .collect();
        let by_lightmap: BTreeSet<&Target> = found
            .lightmaps
            .iter()
            .filter_map(|h| index.lightmaps.get(h))
            .flatten()
            .collect();
        let Some(number) = marker_number(&marker) else {
            continue;
        };
        // The preview sits on the next id; every revision of it has to name the same target.
        let by_preview: BTreeSet<Target> = previews
            .get(&number.saturating_add(1))
            .into_iter()
            .flatten()
            .map(|pixels| target_by_preview(&index, pixels))
            .collect::<Option<_>>()
            .unwrap_or_default();
        let by_preview = match by_preview.iter().collect::<Vec<_>>()[..] {
            [only] => Some(only.clone()),
            _ => None,
        };
        let item_of = |set: &BTreeSet<&Target>| -> BTreeSet<(String, String)> {
            set.iter()
                .map(|(kind, id, _)| (kind.clone(), id.clone()))
                .collect()
        };
        let mut candidates: Option<BTreeSet<(String, String)>> = None;
        let mut narrow = |set: BTreeSet<(String, String)>| {
            if set.is_empty() {
                return;
            }
            candidates = Some(match candidates.take() {
                Some(known) => known.intersection(&set).cloned().collect(),
                None => set,
            });
        };
        narrow(item_of(&by_model));
        narrow(item_of(&by_geometry));
        narrow(item_of(&by_lightmap));
        if let Some((kind, id, _)) = &by_preview {
            narrow(BTreeSet::from([(kind.clone(), id.clone())]));
        }
        let mut candidates = candidates.unwrap_or_default().into_iter();
        let (Some(item), None) = (candidates.next(), candidates.next()) else {
            continue;
        };
        let grades: BTreeSet<i32> = by_lightmap
            .iter()
            .filter(|(kind, id, _)| (kind, id) == (&item.0, &item.1))
            .filter_map(|(_, _, grade)| grade_number(grade))
            .collect();
        let from_lightmap = match grades.iter().collect::<Vec<_>>()[..] {
            [only] => Some(*only),
            _ => None,
        };
        let from_preview = by_preview
            .as_ref()
            .and_then(|(_, _, grade)| grade_number(grade));
        // Two exact sources that disagree mean something is off: say nothing.
        if from_lightmap.zip(from_preview).is_some_and(|(a, b)| a != b) {
            continue;
        }
        // The exact sources win; how the textures look only decides when they say nothing.
        let from_look = if from_preview.is_none() && from_lightmap.is_none() {
            grade_by_look(&index, &item, &found.look)
        } else {
            None
        };
        let direct = from_preview.or(from_lightmap).or(from_look);
        let evidence = if from_preview.is_some() {
            "preview"
        } else if from_lightmap.is_some() {
            "lightmap"
        } else {
            "look"
        };
        items.entry(item).or_default().push(Member {
            marker,
            number,
            direct,
            evidence,
            known: false,
        });
    }
    // Resources that are already recognised take part as anchors of the blocks they sit in.
    for (marker, (kind, id, grade)) in known_markers {
        let Some(number) = marker_number(&marker) else {
            continue;
        };
        items.entry((kind, id)).or_default().push(Member {
            marker,
            number,
            direct: Some(grade),
            evidence: "known",
            known: true,
        });
    }
    // 2. The modifications of one item sit on consecutive even ids, lowest id = lowest modification.
    //    Within each run of such ids the position gives the modification, anchored by the lightmaps
    //    that were recognised directly; a run with no anchor is a guess and is dropped, and so are
    //    anchors that contradict the order.
    let mut out = Vec::new();
    for ((kind, id), mut members) in items {
        members.sort_by_key(|m| m.number);
        let mut runs: Vec<Vec<&Member>> = Vec::new();
        for member in &members {
            match runs.last_mut() {
                Some(run)
                    if run
                        .last()
                        .is_some_and(|last| last.number.checked_add(2) == Some(member.number)) =>
                {
                    run.push(member);
                }
                _ => runs.push(vec![member]),
            }
        }
        for run in runs {
            let anchors: BTreeSet<i32> = run
                .iter()
                .enumerate()
                .filter_map(|(position, m)| m.direct.map(|grade| grade - position as i32))
                .collect();
            let base = match anchors.iter().collect::<Vec<_>>()[..] {
                [only] => *only,
                _ => continue,
            };
            for (position, member) in run.iter().enumerate() {
                let grade = base + position as i32;
                if member.known || !(0..=3).contains(&grade) {
                    continue;
                }
                out.push(Recognition {
                    marker: member.marker.clone(),
                    kind: kind.clone(),
                    id: id.clone(),
                    grade: format!("m{grade}"),
                    evidence: if member.direct == Some(grade) {
                        member.evidence
                    } else {
                        "block order"
                    },
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::Path};
    use tanki_cache::cache::scan_cache;
    use tanki_pack::pack::{PackItem, PackManifest, PackResource, PackTarget, PackVariant};

    fn scratch(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "tanki-recognise-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cache_file(root: &Path, marker: &str, revision: &str, role: &str, bytes: &[u8]) {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let url = format!("http://cache.test/{marker}/{revision}/{role}");
        fs::write(root.join(STANDARD.encode(url)), bytes).unwrap();
    }

    type Entry<'a> = (&'a str, &'a str, &'a str, &'a [u8], Option<&'a [u8]>);

    fn pack(dir: &Path, items: &[Entry<'_>]) -> InstalledPack {
        let mut manifest_items = Vec::new();
        for (kind, id, grade, model, lightmap) in items {
            let mut resources = Vec::new();
            let base = format!("{kind}-{id}-{grade}");
            fs::write(dir.join(format!("{base}.3ds")), model).unwrap();
            resources.push(PackResource {
                role: "model".into(),
                path: format!("{base}.3ds"),
                sha256: hash(model),
            });
            if let Some(lightmap) = lightmap {
                fs::write(dir.join(format!("{base}.jpg")), lightmap).unwrap();
                resources.push(PackResource {
                    role: "lightmap".into(),
                    path: format!("{base}.jpg"),
                    sha256: hash(lightmap),
                });
            }
            manifest_items.push(PackItem {
                target: PackTarget {
                    kind: (*kind).into(),
                    id: (*id).into(),
                    grade: (*grade).into(),
                },
                variants: vec![PackVariant {
                    id: "v".into(),
                    name: "V".into(),
                    preview: None,
                    resources,
                }],
            });
        }
        InstalledPack {
            path: dir.to_path_buf(),
            manifest: PackManifest {
                format_version: 1,
                id: "p".into(),
                name: "P".into(),
                version: "1".into(),
                author: String::new(),
                license: String::new(),
                games: vec!["air-cache".into()],
                items: manifest_items,
            },
        }
    }

    #[test]
    fn the_item_comes_from_the_model_and_the_modification_from_the_lightmap_never_from_a_guess() {
        let root = scratch("cache");
        let packs_dir = scratch("pack");
        let hornet: &[u8] = b"hornet-model";
        let isida: &[u8] = b"isida-model";
        let pack = pack(
            &packs_dir,
            &[
                ("hull", "hornet", "m0", hornet, Some(b"hornet-light-0")),
                ("hull", "hornet", "m1", hornet, Some(b"hornet-light-1")),
                ("hull", "hornet", "m2", hornet, Some(b"hornet-light-2")),
                ("hull", "hornet", "m3", hornet, Some(b"hornet-light-3")),
                ("turret", "isida", "m3", isida, None),
                ("hull", "mammoth", "m3", b"mammoth-model", None),
            ],
        );
        // Hornet: ids 7/20, 7/22, 7/24 (M1, M2, M3 by their lightmaps); 7/16 and 7/18 are not there.
        for (marker, light) in [("0/0/7/20", "1"), ("0/0/7/22", "2"), ("0/0/7/24", "3")] {
            cache_file(&root, marker, "1", "object.3ds", hornet);
            cache_file(
                &root,
                marker,
                "1",
                "lightmap.jpg",
                format!("hornet-light-{light}").as_bytes(),
            );
        }
        // Isida: four consecutive ids with the shared model and nothing that names a modification:
        // the item is known, the order alone is a guess, so none of them is reported.
        for marker in ["0/0/7/40", "0/0/7/42", "0/0/7/44", "0/0/7/46"] {
            cache_file(&root, marker, "1", "object.3ds", isida);
        }
        // Mammoth: a lone resource with only a model: the item is known, the modification is not.
        cache_file(&root, "0/0/7/60", "1", "object.3ds", b"mammoth-model");
        // A model nobody ships, and the built-in Wasp (already recognised).
        cache_file(&root, "0/0/7/70", "1", "object.3ds", b"unknown");
        cache_file(&root, "0/0/10/14", "1", "object.3ds", hornet);
        let scan = scan_cache(&root, Some("air-cache")).unwrap();

        let found = recognise_by_content(&scan, &[pack]);
        let by_marker: BTreeMap<&str, (&str, &str, &str, &str)> = found
            .iter()
            .map(|r| {
                (
                    r.marker.as_str(),
                    (r.kind.as_str(), r.id.as_str(), r.grade.as_str(), r.evidence),
                )
            })
            .collect();
        assert_eq!(
            by_marker.get("/0/0/7/20/"),
            Some(&("hull", "hornet", "m1", "lightmap"))
        );
        assert_eq!(
            by_marker.get("/0/0/7/22/"),
            Some(&("hull", "hornet", "m2", "lightmap"))
        );
        assert_eq!(
            by_marker.get("/0/0/7/24/"),
            Some(&("hull", "hornet", "m3", "lightmap"))
        );
        for unknown in [
            "/0/0/7/40/",
            "/0/0/7/42/",
            "/0/0/7/44/",
            "/0/0/7/46/",
            "/0/0/7/60/",
            "/0/0/7/70/",
            "/0/0/10/14/",
        ] {
            assert!(
                !by_marker.contains_key(unknown),
                "{unknown} must not be guessed"
            );
        }
        assert_eq!(found.len(), 3);
    }

    #[test]
    fn anchors_shift_a_short_run_and_contradicting_anchors_drop_it() {
        let root = scratch("anchors");
        let packs_dir = scratch("anchor-pack");
        let gun: &[u8] = b"gun-model";
        let pack = pack(
            &packs_dir,
            &[
                ("turret", "twins", "m0", gun, Some(b"light-0")),
                ("turret", "twins", "m1", gun, Some(b"light-1")),
                ("turret", "twins", "m2", gun, Some(b"light-2")),
                ("turret", "twins", "m3", gun, Some(b"light-3")),
            ],
        );
        // Three ids in a row, the middle one tells it is M2: so the others are M1 and M3.
        for marker in ["0/0/7/10", "0/0/7/12", "0/0/7/14"] {
            cache_file(&root, marker, "1", "object.3ds", gun);
        }
        cache_file(&root, "0/0/7/12", "1", "lightmap.jpg", b"light-2");
        // Another run whose two anchors disagree with the order (M3 before M1): nothing is labelled.
        for marker in ["0/0/7/100", "0/0/7/102"] {
            cache_file(&root, marker, "1", "object.3ds", gun);
        }
        cache_file(&root, "0/0/7/100", "1", "lightmap.jpg", b"light-3");
        cache_file(&root, "0/0/7/102", "1", "lightmap.jpg", b"light-1");
        let scan = scan_cache(&root, Some("air-cache")).unwrap();
        let found = recognise_by_content(&scan, &[pack]);
        let grades: BTreeMap<&str, &str> = found
            .iter()
            .map(|r| (r.marker.as_str(), r.grade.as_str()))
            .collect();
        assert_eq!(grades.get("/0/0/7/10/"), Some(&"m1"));
        assert_eq!(grades.get("/0/0/7/12/"), Some(&"m2"));
        assert_eq!(grades.get("/0/0/7/14/"), Some(&"m3"));
        assert!(
            !grades.contains_key("/0/0/7/100/") && !grades.contains_key("/0/0/7/102/"),
            "{grades:?}"
        );
    }

    #[test]
    fn cache_names_with_huge_numbers_are_no_markers_and_never_overflow() {
        let root = scratch("huge-marker");
        let packs_dir = scratch("huge-pack");
        let gun: &[u8] = b"gun-model";
        let pack = pack(
            &packs_dir,
            &[("turret", "twins", "m3", gun, Some(b"light-3"))],
        );
        // Octal numbers far beyond the client's ids: u64::MAX and more.
        cache_file(
            &root,
            "1777777777777777777777/177777/377/377",
            "1",
            "object.3ds",
            gun,
        );
        cache_file(
            &root,
            "0/0/377/377777777777777777777",
            "1",
            "object.3ds",
            gun,
        );
        let scan = scan_cache(&root, Some("air-cache")).unwrap();
        assert!(recognise_by_content(&scan, &[pack]).is_empty());
        assert_eq!(marker_number("/1777777777777777777777/0/0/0/"), None);
        assert_eq!(marker_number("/0/0/10/14/"), Some(0x080c));
    }

    #[test]
    fn no_packs_or_no_models_recognise_nothing() {
        let root = scratch("empty-cache");
        cache_file(&root, "0/0/7/20", "1", "object.3ds", b"x");
        let scan = scan_cache(&root, Some("air-cache")).unwrap();
        assert!(recognise_by_content(&scan, &[]).is_empty());
    }

    fn picture(seed: u8) -> Vec<u8> {
        let image = image::RgbImage::from_fn(PREVIEW_SIZE.0, PREVIEW_SIZE.1, |x, y| {
            let on = (x / 20 + y / 20 + u32::from(seed)) % 2 == 0;
            let value = if on { 200 } else { 30 };
            image::Rgb([value, value / 2 + seed * 20, 40])
        });
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn the_same_mesh_in_other_bytes_names_the_item_and_a_known_neighbour_anchors_the_block() {
        let root = scratch("geometry-cache");
        let packs_dir = scratch("geometry-pack");
        let pack_model = crate::geometry::tests::model(&[(120, 200), (40, 60)]);
        // The cache's export of the same mesh: other bytes, other object order.
        let mut cache_model = crate::geometry::tests::model(&[(40, 60), (120, 200)]);
        cache_model.extend([0u8; 8]);
        assert_ne!(pack_model, cache_model);
        let pack = pack(&packs_dir, &[("hull", "wasp", "m3", &pack_model, None)]);
        // Wasp M3 is built in (10/14); 10/6, 10/10 and 10/12 are the three before it (octal ids, step 2).
        for marker in ["0/0/10/6", "0/0/10/10", "0/0/10/12", "0/0/10/14"] {
            cache_file(&root, marker, "1", "object.3ds", &cache_model);
        }
        // The same mesh far away from any known neighbour is the item, but its modification is a guess.
        cache_file(&root, "0/0/9/100", "1", "object.3ds", &cache_model);
        let scan = scan_cache(&root, Some("air-cache")).unwrap();
        let found = recognise_by_content(&scan, &[pack]);
        let by_marker: BTreeMap<&str, (&str, &str)> = found
            .iter()
            .map(|r| (r.marker.as_str(), (r.grade.as_str(), r.evidence)))
            .collect();
        assert_eq!(by_marker.get("/0/0/10/6/"), Some(&("m0", "block order")));
        assert_eq!(by_marker.get("/0/0/10/10/"), Some(&("m1", "block order")));
        assert_eq!(by_marker.get("/0/0/10/12/"), Some(&("m2", "block order")));
        assert!(!by_marker.contains_key("/0/0/10/14/"), "already known");
        assert!(!by_marker.contains_key("/0/0/9/100/"), "{by_marker:?}");
    }

    #[test]
    fn the_preview_next_to_the_model_names_the_item_and_the_modification() {
        let root = scratch("preview-cache");
        let packs_dir = scratch("preview-pack");
        let gun: &[u8] = b"gun-model";
        let mut pack = pack(
            &packs_dir,
            &[
                ("turret", "twins", "m2", gun, None),
                ("turret", "twins", "m3", gun, None),
            ],
        );
        for (item, seed) in pack.manifest.items.iter_mut().zip([1u8, 2]) {
            fs::write(packs_dir.join(format!("preview-{seed}.png")), picture(seed)).unwrap();
            item.variants[0].preview = Some(format!("preview-{seed}.png"));
        }
        // Twins M3: model on 7/30, its preview on 7/31. Twins M2: model on 7/20, preview on 7/21.
        for (model, preview, seed) in [("0/0/7/30", "0/0/7/31", 2u8), ("0/0/7/20", "0/0/7/21", 1)] {
            cache_file(&root, model, "1", "object.3ds", gun);
            cache_file(&root, preview, "1", "image.jpg", &picture(seed));
        }
        // A model whose neighbouring picture is nobody's preview: nothing is claimed for it.
        cache_file(&root, "0/0/7/50", "1", "object.3ds", gun);
        cache_file(&root, "0/0/7/51", "1", "image.jpg", &picture(0));
        let scan = scan_cache(&root, Some("air-cache")).unwrap();

        let found = recognise_by_content(&scan, &[pack]);
        let by_marker: BTreeMap<&str, (&str, &str)> = found
            .iter()
            .map(|r| (r.marker.as_str(), (r.grade.as_str(), r.evidence)))
            .collect();
        assert_eq!(by_marker.get("/0/0/7/30/"), Some(&("m3", "preview")));
        assert_eq!(by_marker.get("/0/0/7/20/"), Some(&("m2", "preview")));
        assert_eq!(by_marker.get("/0/0/7/50/"), None);
    }
}
