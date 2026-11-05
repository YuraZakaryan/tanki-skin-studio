// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The Garage's 3D preview: the setting, the stage, and building its scenes. A scene is read and
//! decoded on a thread of its own (a model and three textures take a moment), and the stage shows
//! it once it is there; until then, or when it cannot be built, the Garage shows the flat picture.

use std::{
    collections::BTreeSet,
    sync::{Arc, mpsc},
};

use anyhow::{Context, Result};
use image::{DynamicImage, GenericImageView, imageops::FilterType};
use tanki_core::{
    look::{LookFiles, LookSource, cache_look},
    model::{Geometry, read},
    pack::PackTarget,
    sprite::still_frame,
};
use tanki_viewer::{Image, Material, Part, QualityPreference, Scene, Stage, stage};

/// Textures are scaled down to this on their longer side: more is not seen on a stage.
const MAX_TEXTURE: u32 = 2048;

/// A scene being built for a key.
struct Job {
    key: String,
    receiver: mpsc::Receiver<Result<Scene, String>>,
}

/// The paint every item wears in 3D until a skin or a paint says otherwise: the plain green of the
/// cache, the look a new tank has.
pub const DEFAULT_PAINT: &str = "green";

/// A look and its key, worked out once when what the Garage shows changes (finding an item's files
/// goes through the whole cache, too slow for every frame).
#[derive(Clone, Debug)]
pub struct Keyed {
    pub source: LookSource,
    pub key: String,
}

impl Keyed {
    pub fn new(source: LookSource) -> Keyed {
        Keyed {
            key: source.key(),
            source,
        }
    }
}

/// The looks of the stage, and what they were worked out from.
struct Memo {
    input: String,
    main: Option<Keyed>,
    compared: Option<Keyed>,
}

/// What the stage can show for a slot right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    /// The scene of the wanted look is on the stage.
    Ready,
    /// It is being built.
    Loading,
    /// It cannot be shown (no model, a damaged file): the flat picture stands in.
    Unavailable,
}

pub struct Preview3d {
    /// The user's choice (Settings, or the stage's own quick settings): on by default.
    pub enabled: bool,
    pub quality: QualityPreference,
    pub stage: Stage,
    /// Where the divider stands while comparing (a share of the stage's width), or `None`.
    pub compare: Option<f32>,
    /// The drag hint shows until the stage is first used.
    pub used: bool,
    /// Build scenes on their own thread; tests turn this off to get them at once.
    pub background: bool,
    jobs: [Option<Job>; 2],
    /// Looks that could not be built, so they are not tried again every frame.
    failed: BTreeSet<String>,
    /// The item the camera was last framed for: another item starts from the home view.
    framed: Option<String>,
    /// The hull last shown (id, modification): a paint is shown on it.
    pub hull: Option<(String, usize)>,
    memo: Option<Memo>,
}

impl Preview3d {
    /// The main scene's slot and the one it is compared with.
    pub const MAIN: usize = stage::MAIN;
    pub const COMPARED: usize = stage::COMPARED;

    pub fn new(enabled: bool, quality: QualityPreference, stage: Stage) -> Preview3d {
        let mut preview = Preview3d {
            enabled,
            quality,
            stage,
            compare: None,
            used: false,
            background: true,
            jobs: [None, None],
            failed: BTreeSet::new(),
            framed: None,
            hull: None,
            memo: None,
        };
        preview.apply_quality();
        preview
    }

    /// Whether the Garage draws the stage: on, and the window draws with a GPU.
    pub fn active(&self) -> bool {
        self.enabled && self.stage.is_available()
    }

    pub fn set_quality(&mut self, quality: QualityPreference) {
        self.quality = quality;
        self.apply_quality();
    }

    fn apply_quality(&mut self) {
        let detected = self.stage.detected_quality();
        self.stage.set_quality(self.quality.resolve(detected));
    }

    /// Ask for `look` in a slot: built if it is not on the stage, not being built and not known to
    /// fail. `None` empties the slot.
    pub fn want(&mut self, slot: usize, look: Option<&Keyed>) -> SlotState {
        let Some(Keyed { source: look, key }) = look else {
            self.stage.unload(slot);
            if let Some(job) = self.jobs.get_mut(slot) {
                *job = None;
            }
            return SlotState::Unavailable;
        };
        if self.stage.key(slot) == Some(key.as_str()) {
            return SlotState::Ready;
        }
        if self.failed.contains(key) {
            return SlotState::Unavailable;
        }
        let Some(job) = self.jobs.get_mut(slot) else {
            return SlotState::Unavailable;
        };
        if job.as_ref().is_some_and(|j| &j.key == key) {
            return SlotState::Loading;
        }
        let (sender, receiver) = mpsc::channel();
        let source = look.clone();
        let build = move || {
            let scene = source
                .read()
                .and_then(|files| build_scene(&files))
                .map_err(|e| format!("{e:#}"));
            let _ = sender.send(scene);
        };
        *job = Some(Job {
            key: key.clone(),
            receiver,
        });
        if self.background {
            std::thread::spawn(build);
            return SlotState::Loading;
        }
        build();
        self.poll();
        if self.stage.key(slot) == Some(key.as_str()) {
            SlotState::Ready
        } else {
            SlotState::Unavailable
        }
    }

    /// Take finished scenes onto the stage. Returns whether a scene is still being built.
    pub fn poll(&mut self) -> bool {
        let mut pending = false;
        for slot in 0..self.jobs.len() {
            let Some(job) = self.jobs.get(slot).and_then(Option::as_ref) else {
                continue;
            };
            let key = job.key.clone();
            match job.receiver.try_recv() {
                Ok(Ok(scene)) => {
                    self.stage.load(slot, &key, &scene);
                    self.jobs[slot] = None;
                }
                Ok(Err(error)) => {
                    log::warn!("could not build the 3D preview: {error}");
                    self.failed.insert(key);
                    self.jobs[slot] = None;
                }
                Err(mpsc::TryRecvError::Empty) => pending = true,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed.insert(key);
                    self.jobs[slot] = None;
                }
            }
        }
        pending
    }

    /// Start from the home view, not comparing, when the shown item changes.
    pub fn frame_item(&mut self, item: &str) {
        if self.framed.as_deref() != Some(item) {
            self.framed = Some(item.to_owned());
            self.stage.camera_mut().go_home();
            self.compare = None;
        }
    }

    pub fn toggle_compare(&mut self) {
        self.compare = match self.compare {
            Some(_) => None,
            None => Some(0.5),
        };
    }

    pub fn toggle_auto_rotate(&mut self) {
        let camera = self.stage.camera_mut();
        camera.auto_rotate = !camera.auto_rotate;
    }

    pub fn auto_rotating(&self) -> bool {
        self.stage.camera().auto_rotate
    }
}

impl crate::app::Studio {
    /// The looks the stage shows for `selection`: the picked skin (`chosen`), else the item as in
    /// the game; and the item as in the game to compare with, while a picked skin is not in it.
    /// A hull or turret wears the default paint under its details, a paint is shown on a hull.
    /// Worked out again only when what they depend on changes.
    pub fn stage_looks(
        &mut self,
        selection: &crate::app::Selection,
        chosen: Option<(usize, usize)>,
        in_game: bool,
    ) -> (Option<Keyed>, Option<Keyed>) {
        let input = format!(
            "{}|{chosen:?}|{in_game}|{:?}|{:?}|{:?}",
            selection.key(),
            self.active_pack
                .as_ref()
                .map(|p| (&p.path, &p.manifest.version)),
            self.scanned_at,
            self.preview.hull,
        );
        if let Some(memo) = &self.preview.memo
            && memo.input == input
        {
            return (memo.main.clone(), memo.compared.clone());
        }
        let target = PackTarget {
            kind: selection.kind.clone(),
            id: selection.id.clone(),
            grade: crate::model::grade_code(selection.grade).to_owned(),
        };
        let in_cache = self
            .scan
            .as_ref()
            .and_then(|scan| cache_look(scan, &target));
        // A picked skin shows as itself or not at all (one without a model has its picture).
        let main = match chosen {
            Some((i, v)) => self.active_pack.as_ref().and_then(|pack| {
                let variant = pack.manifest.items.get(i)?.variants.get(v)?;
                tanki_core::look::variant_look(pack, variant)
            }),
            None => in_cache.clone(),
        };
        let painted = |body: Option<LookSource>, paint: Option<LookSource>| {
            Some(LookSource::Painted {
                body: Box::new(body?),
                paint: Box::new(paint?),
            })
        };
        let (main, in_cache) = if selection.kind == "paint" {
            let body = self.paint_body();
            (painted(body.clone(), main), painted(body, in_cache))
        } else {
            match self.default_paint() {
                Some(green) => (
                    painted(main, Some(green.clone())),
                    painted(in_cache, Some(green)),
                ),
                None => (main, in_cache),
            }
        };
        let compared = (chosen.is_some() && !in_game).then_some(in_cache).flatten();
        let memo = Memo {
            input,
            main: main.map(Keyed::new),
            compared: compared.map(Keyed::new),
        };
        let looks = (memo.main.clone(), memo.compared.clone());
        self.preview.memo = Some(memo);
        looks
    }

    /// The default paint's picture in the cache, when the cache has it.
    fn default_paint(&self) -> Option<LookSource> {
        cache_look(
            self.scan.as_ref()?,
            &PackTarget {
                kind: "paint".into(),
                id: DEFAULT_PAINT.into(),
                grade: "m0".into(),
            },
        )
    }

    /// The body a paint is shown on: the hull last shown, else the first hull with a model in the
    /// cache (its highest modification).
    pub fn paint_body(&self) -> Option<LookSource> {
        let scan = self.scan.as_ref()?;
        let look = |id: &str, grade: usize| {
            cache_look(
                scan,
                &PackTarget {
                    kind: "hull".into(),
                    id: id.to_owned(),
                    grade: crate::model::grade_code(grade).to_owned(),
                },
            )
            .filter(|look| matches!(look, LookSource::Cache(files) if files.iter().any(|(role, ..)| role == "object.3ds")))
        };
        if let Some((id, grade)) = &self.preview.hull
            && let Some(found) = look(id, *grade)
        {
            return Some(found);
        }
        self.items
            .iter()
            .filter(|item| item.kind == "hull")
            .find_map(|item| (0..4).rev().find_map(|grade| look(&item.id, grade)))
    }
}

/// A scene of one part from a look's files: the model, its details over its paint (the paint's
/// picture when the look has one, a neutral colour else; the details' alpha from the separate file
/// the client keeps), and the lightmap.
pub fn build_scene(files: &LookFiles) -> Result<Scene> {
    let model = read(&files.model).context("reading the model")?;
    let geometry = Geometry::from_meshes(&model.meshes);
    let details = files
        .details
        .as_deref()
        .map(|bytes| texture(bytes, files.details_alpha.as_deref()).context("reading the details"))
        .transpose()?;
    let lightmap = files
        .lightmap
        .as_deref()
        .map(|bytes| texture(bytes, None).context("reading the lightmap"))
        .transpose()?;
    let paint = files
        .paint
        .as_deref()
        .map(|bytes| paint_picture(bytes).context("reading the paint"))
        .transpose()?;
    Ok(Scene {
        parts: vec![Part {
            geometry: Arc::new(geometry),
            material: Material {
                paint_picture: paint.map(Arc::new),
                details: details.map(Arc::new),
                lightmap: lightmap.map(Arc::new),
                ..Material::default()
            },
            offset: [0.0; 3],
        }],
    })
}

/// A picture as an RGBA texture, its alpha from `alpha` (a greyscale picture, stretched to the
/// same size) when given, scaled down to [`MAX_TEXTURE`].
fn texture(bytes: &[u8], alpha: Option<&[u8]>) -> Result<Image> {
    let picture = limited(image::load_from_memory(bytes)?);
    let mut rgba = picture.to_rgba8();
    if let Some(alpha) = alpha.and_then(|a| image::load_from_memory(a).ok()) {
        let alpha = alpha
            .resize_exact(rgba.width(), rgba.height(), FilterType::Triangle)
            .to_luma8();
        for (pixel, mask) in rgba.pixels_mut().zip(alpha.pixels()) {
            pixel.0[3] = mask.0[0];
        }
    }
    let (width, height) = rgba.dimensions();
    Image::new(width, height, rgba.into_raw()).context("texture size")
}

/// A picture of the stage as PNG bytes.
pub fn png(picture: &Image) -> Result<Vec<u8>> {
    let rgba =
        image::RgbaImage::from_raw(picture.width(), picture.height(), picture.rgba().to_vec())
            .context("picture size")?;
    let mut out = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba).write_to(&mut out, image::ImageFormat::Png)?;
    Ok(out.into_inner())
}

/// A paint's picture: an animated paint is a sheet of frames, and its most typical frame stands
/// for it.
fn paint_picture(bytes: &[u8]) -> Result<Image> {
    let picture = image::load_from_memory(bytes)?;
    let still = limited(still_frame(&picture).unwrap_or(picture)).to_rgba8();
    let (width, height) = still.dimensions();
    Image::new(width, height, still.into_raw()).context("paint size")
}

fn limited(picture: DynamicImage) -> DynamicImage {
    let (width, height) = picture.dimensions();
    if width.max(height) > MAX_TEXTURE {
        picture.resize(MAX_TEXTURE, MAX_TEXTURE, FilterType::Triangle)
    } else {
        picture
    }
}

#[cfg(test)]
mod tests;
