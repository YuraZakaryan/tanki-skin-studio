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
    model::{Bounds, Geometry, read},
    pack::PackTarget,
    session::StageSession,
    sprite::still_frame,
};
use tanki_viewer::{Image, Material, Part, QualityPreference, Scene, Stage, stage};

/// Textures are scaled down to this on their longer side: more is not seen on a stage.
const MAX_TEXTURE: u32 = 2048;

/// The share of the picture a whole tank fills at most. Framed to fit from every side, the
/// turret's barrel leaves a tank small from the front; it may come closer than an item alone.
const TANK_FILL: f32 = 0.78;

/// A scene being built for a key.
struct Job {
    key: String,
    receiver: mpsc::Receiver<Result<Scene, String>>,
}

/// The paint every item wears in 3D until a skin or a paint says otherwise: the plain green of the
/// cache, the look a new tank has.
pub const DEFAULT_PAINT: &str = "green";

/// What a stage shows and its key, worked out once when what the Garage shows changes (finding an
/// item's files goes through the whole cache, too slow for every frame): one look, or a tank put
/// together from its hull (the first part) and the turret set on the hull's mount.
#[derive(Clone, Debug)]
pub struct Keyed {
    pub parts: Vec<LookSource>,
    pub key: String,
}

impl Keyed {
    pub fn new(source: LookSource) -> Keyed {
        Keyed::assembled(vec![source])
    }

    pub fn assembled(parts: Vec<LookSource>) -> Keyed {
        let keys: Vec<String> = parts.iter().map(LookSource::key).collect();
        Keyed {
            key: keys.join(" & "),
            parts,
        }
    }
}

/// The tank shown whole: the hull and turret (id, modification) and the paint last chosen in the
/// Garage. Until the user chooses, the starter tank in the cache's plain green, at M3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tank {
    pub hull: (String, usize),
    pub turret: (String, usize),
    pub paint: String,
}

impl Default for Tank {
    fn default() -> Tank {
        Tank {
            hull: ("hunter".into(), 3),
            turret: ("smoky".into(), 3),
            paint: DEFAULT_PAINT.into(),
        }
    }
}

impl Tank {
    /// Take in `selection` when it is a part of a tank; returns the part and its saved value when
    /// it changed.
    pub fn remember(
        &mut self,
        selection: &crate::app::Selection,
    ) -> Option<(&'static str, String)> {
        let id = selection.id.clone();
        let (part, changed) = match selection.kind.as_str() {
            "hull" => ("hull", replace(&mut self.hull, (id, selection.grade))),
            "turret" => ("turret", replace(&mut self.turret, (id, selection.grade))),
            "paint" => ("paint", replace(&mut self.paint, id)),
            _ => return None,
        };
        changed.then(|| (part, self.saved(part)))
    }

    /// A part as the store keeps it: `id:m3`, a paint's id.
    pub fn saved(&self, part: &str) -> String {
        let with_grade =
            |(id, grade): &(String, usize)| format!("{id}:{}", crate::model::grade_code(*grade));
        match part {
            "hull" => with_grade(&self.hull),
            "turret" => with_grade(&self.turret),
            _ => self.paint.clone(),
        }
    }

    /// Restore the parts the store kept (`saved` gives a part's value), the starter tank's where
    /// none or a broken one is kept.
    pub fn restored(saved: impl Fn(&str) -> Option<String>) -> Tank {
        let mut tank = Tank::default();
        let with_grade = |value: String| {
            let (id, grade) = value.rsplit_once(':')?;
            Some((id.to_owned(), crate::model::grade_index(grade)?))
        };
        if let Some(hull) = saved("hull").and_then(with_grade) {
            tank.hull = hull;
        }
        if let Some(turret) = saved("turret").and_then(with_grade) {
            tank.turret = turret;
        }
        if let Some(paint) = saved("paint").filter(|p| !p.is_empty()) {
            tank.paint = paint;
        }
        tank
    }
}

/// The parts of the tank shown together on the item's page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TankParts {
    pub hull: crate::app::Selection,
    pub turret: crate::app::Selection,
    pub paint: Option<crate::app::Selection>,
}

/// Put `value` in `slot`; whether it was different.
fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
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
    /// Show the selected hull, turret or paint as a whole tank (Settings): on by default.
    pub whole_tank: bool,
    /// The parts of that tank, as last chosen; a paint alone is shown on its hull.
    pub tank: Tank,
    memo: Option<Memo>,
}

impl Preview3d {
    /// The main scene's slot and the one it is compared with.
    pub const MAIN: usize = stage::MAIN;
    pub const COMPARED: usize = stage::COMPARED;

    pub fn new(
        enabled: bool,
        quality: QualityPreference,
        stage: Stage,
        whole_tank: bool,
        tank: Tank,
    ) -> Preview3d {
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
            whole_tank,
            tank,
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
        let Some(Keyed { parts, key }) = look else {
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
        let parts = parts.clone();
        let build = move || {
            let scene = parts
                .iter()
                .map(LookSource::read)
                .collect::<Result<Vec<_>>>()
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

    /// Whether the main scene asked for is still being built.
    pub fn loading(&self) -> bool {
        self.jobs.get(Self::MAIN).is_some_and(Option::is_some)
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
                    if slot == Self::MAIN {
                        let fill = if scene.parts.len() > 1 {
                            TANK_FILL
                        } else {
                            tanki_viewer::camera::FILL_SHARE
                        };
                        self.stage.camera_mut().set_fill(fill);
                    }
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

    /// Stop comparing when the shown item changes. The view goes on as it was: the same angle
    /// and zoom, still turning if it turned.
    pub fn frame_item(&mut self, item: &str) {
        if self.framed.as_deref() != Some(item) {
            self.framed = Some(item.to_owned());
            self.compare = None;
        }
    }

    /// The view to keep for the next start: the camera at rest and the item it is framed for.
    /// While it turns by itself, the yaw is the one kept `before` (no write every frame).
    pub fn kept_view(&self, before: Option<&StageSession>) -> Option<StageSession> {
        let camera = self.stage.camera();
        let pose = camera.resting_pose()?;
        let yaw = match before {
            Some(before) if camera.turning_by_itself() => before.yaw,
            _ => pose.yaw,
        };
        Some(StageSession {
            item: self.framed.clone()?,
            yaw,
            pitch: pose.pitch,
            zoom: pose.zoom,
            auto_rotate: camera.auto_rotate,
        })
    }

    /// Show the view kept from the last time: the same item opens as it was left.
    pub fn restore_view(&mut self, view: &StageSession) {
        self.framed = Some(view.item.clone());
        let camera = self.stage.camera_mut();
        camera.set_pose(tanki_viewer::camera::Pose {
            yaw: view.yaw,
            pitch: view.pitch,
            zoom: view.zoom,
        });
        camera.auto_rotate = view.auto_rotate;
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
    /// A hull, turret or paint is shown in the whole tank when that is on and the tank's other
    /// parts are in the cache; else alone, a hull or turret in the default paint under its
    /// details, a paint on a hull. Worked out again only when what they depend on changes.
    pub fn stage_looks(
        &mut self,
        selection: &crate::app::Selection,
        chosen: Option<(usize, usize)>,
        in_game: bool,
    ) -> (Option<Keyed>, Option<Keyed>) {
        let input = format!(
            "{}|{chosen:?}|{in_game}|{:?}|{:?}|{}|{:?}",
            selection.key(),
            self.active_pack
                .as_ref()
                .map(|p| (&p.path, &p.manifest.version)),
            self.scanned_at,
            self.preview.whole_tank,
            self.preview.tank,
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
        let whole = self
            .preview
            .whole_tank
            .then(|| self.tank_looks(&selection.kind, main.clone(), in_cache.clone()))
            .flatten();
        let (main, in_cache) = whole.unwrap_or_else(|| {
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
            (main.map(Keyed::new), in_cache.map(Keyed::new))
        });
        let compared = (chosen.is_some() && !in_game).then_some(in_cache).flatten();
        let memo = Memo {
            input,
            main,
            compared,
        };
        let looks = (memo.main.clone(), memo.compared.clone());
        self.preview.memo = Some(memo);
        looks
    }

    /// The whole tank with the selected part in it, as `main` and as in the game (`in_cache`);
    /// the other parts are the ones last chosen, as in the game. `None` when the selection is not a
    /// part of a tank or the tank's other parts are not in the cache: the item is shown alone.
    fn tank_looks(
        &self,
        kind: &str,
        main: Option<LookSource>,
        in_cache: Option<LookSource>,
    ) -> Option<(Option<Keyed>, Option<Keyed>)> {
        let tank = &self.preview.tank;
        let hull = || self.part_look("hull", &tank.hull);
        let turret = || self.part_look("turret", &tank.turret);
        // Without the paint last chosen in the cache, the default one; without that, none.
        let paint = || {
            self.cache_paint(&tank.paint)
                .or_else(|| self.default_paint())
        };
        let (shown, as_in_game) = match kind {
            "hull" => {
                let (turret, paint) = (turret()?, paint());
                (
                    [main, Some(turret.clone()), paint.clone()],
                    [in_cache, Some(turret), paint],
                )
            }
            "turret" => {
                let (hull, paint) = (hull()?, paint());
                (
                    [Some(hull.clone()), main, paint.clone()],
                    [Some(hull), in_cache, paint],
                )
            }
            "paint" => {
                let (hull, turret) = (hull()?, turret()?);
                (
                    [Some(hull.clone()), Some(turret.clone()), main],
                    [Some(hull), Some(turret), in_cache],
                )
            }
            _ => return None,
        };
        let assembled = |[hull, turret, paint]: [Option<LookSource>; 3]| {
            let (hull, turret) = (hull?, turret?);
            let parts = match paint {
                Some(paint) => vec![
                    painted(Some(hull), Some(paint.clone()))?,
                    painted(Some(turret), Some(paint))?,
                ],
                // A picked paint that cannot be shown leaves nothing to show.
                None if kind == "paint" => return None,
                None => vec![hull, turret],
            };
            Some(Keyed::assembled(parts))
        };
        Some((assembled(shown), assembled(as_in_game)))
    }

    /// A paint's picture in the cache, when the cache has it.
    fn cache_paint(&self, id: &str) -> Option<LookSource> {
        cache_look(
            self.scan.as_ref()?,
            &PackTarget {
                kind: "paint".into(),
                id: id.into(),
                grade: "m0".into(),
            },
        )
    }

    /// The default paint's picture in the cache, when the cache has it.
    fn default_paint(&self) -> Option<LookSource> {
        self.cache_paint(DEFAULT_PAINT)
    }

    /// A hull or turret with a model in the cache: the modification asked for, else the highest
    /// one the cache has.
    fn part_look(&self, kind: &str, (id, grade): &(String, usize)) -> Option<LookSource> {
        let scan = self.scan.as_ref()?;
        std::iter::once(*grade)
            .chain((0..4).rev())
            .find_map(|grade| {
                cache_look(
                    scan,
                    &PackTarget {
                        kind: kind.into(),
                        id: id.clone(),
                        grade: crate::model::grade_code(grade).to_owned(),
                    },
                )
            })
            .filter(has_model)
    }

    /// The body a paint is shown on: the tank's hull, else the first hull with a model in the
    /// cache (its highest modification).
    pub fn paint_body(&self) -> Option<LookSource> {
        self.part_look("hull", &self.preview.tank.hull).or_else(|| {
            self.items
                .iter()
                .filter(|item| item.kind == "hull")
                .find_map(|item| self.part_look("hull", &(item.id.clone(), 3)))
        })
    }

    /// The parts of the tank the item's page shows together: its hull and turret (the selected
    /// one as selected) and its paint when the cache knows it. `None` while the whole tank is off,
    /// for an item that is not a part of a tank, or when the cache lacks the tank's hull or turret:
    /// the item is shown alone.
    pub fn tank_parts(&self, selection: &crate::app::Selection) -> Option<TankParts> {
        use crate::app::Selection;
        if !self.preview.whole_tank
            || !matches!(selection.kind.as_str(), "hull" | "turret" | "paint")
        {
            return None;
        }
        let tank = &self.preview.tank;
        let part = |kind: &str, (id, grade): &(String, usize)| {
            if selection.kind == kind {
                selection.clone()
            } else {
                Selection {
                    kind: kind.to_owned(),
                    id: id.clone(),
                    grade: *grade,
                }
            }
        };
        let known = |s: &Selection| self.items.iter().any(|i| i.kind == s.kind && i.id == s.id);
        let hull = part("hull", &tank.hull);
        let turret = part("turret", &tank.turret);
        let paint = part("paint", &(tank.paint.clone(), 0));
        (known(&hull) && known(&turret)).then(|| TankParts {
            hull,
            turret,
            paint: known(&paint).then_some(paint),
        })
    }

    /// Keep the selected hull, turret or paint as the tank's, saved for the next start.
    pub fn remember_tank_part(&mut self, selection: &crate::app::Selection) {
        if let Some((part, value)) = self.preview.tank.remember(selection)
            && let Err(error) = self.store.set_tank_part(part, &value)
        {
            log::warn!("could not save the tank's {part}: {error}");
        }
    }
}

/// `body` in `paint`, when both are there.
fn painted(body: Option<LookSource>, paint: Option<LookSource>) -> Option<LookSource> {
    Some(LookSource::Painted {
        body: Box::new(body?),
        paint: Box::new(paint?),
    })
}

/// Whether a look of the cache has a model.
fn has_model(look: &LookSource) -> bool {
    matches!(look, LookSource::Cache(files) if files.iter().any(|(role, ..)| role == "object.3ds"))
}

/// A scene of the looks' files, one part each: the model (its helpers left out), its details over
/// its paint (the paint's picture when the look has one, a neutral colour else; the details' alpha
/// from the separate file the client keeps), and the lightmap. A part after the first (a turret)
/// is set on the first's mount, its own pivot on it.
pub fn build_scene(looks: &[LookFiles]) -> Result<Scene> {
    let mut parts = Vec::with_capacity(looks.len());
    let mut mount = [0.0; 3];
    for (index, files) in looks.iter().enumerate() {
        let model = read(&files.model).context("reading the model")?;
        let geometry = Geometry::from_meshes(model.shown());
        anyhow::ensure!(
            !geometry.indices.is_empty(),
            "the model has nothing to draw"
        );
        let offset = if index == 0 {
            mount = model.mount().unwrap_or_else(|| top(&geometry.bounds));
            [0.0; 3]
        } else {
            let pivot = model.pivot().unwrap_or_else(|| bottom(&geometry.bounds));
            std::array::from_fn(|axis| mount[axis] - pivot[axis])
        };
        parts.push(Part {
            geometry: Arc::new(geometry),
            material: material(files)?,
            offset,
        });
    }
    Ok(Scene { parts })
}

/// Where a hull without a mount takes its turret: the middle of its top.
fn top(bounds: &Bounds) -> [f32; 3] {
    let [x, y, _] = bounds.center();
    [x, y, bounds.max[2]]
}

/// Where a turret without a pivot sits: the middle of its bottom.
fn bottom(bounds: &Bounds) -> [f32; 3] {
    let [x, y, _] = bounds.center();
    [x, y, bounds.min[2]]
}

fn material(files: &LookFiles) -> Result<Material> {
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
    Ok(Material {
        paint_picture: paint.map(Arc::new),
        details: details.map(Arc::new),
        lightmap: lightmap.map(Arc::new),
        ..Material::default()
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
