// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The stage as an egui widget: input turns the camera, and a picture is drawn only when what it
//! would show has changed (camera, size, quality or scene). Two scenes can be shown side by side
//! for a before/after comparison, seen from the same camera.

use egui::{Color32, Key, Pos2, Rect, Response, Sense, Ui, Vec2};
use egui_wgpu::{RenderState, wgpu};

use crate::{
    camera::OrbitCamera,
    gpu::{Gpu, GpuScene, Target, View},
    quality::Quality,
    scene::{Image, Scene},
};

/// The keyboard turns the camera by this much per press.
const KEY_STEP: f32 = 0.15;
/// How much one point of wheel travel zooms.
const WHEEL_ZOOM: f32 = 0.0025;

/// What the stage shows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StageView {
    /// The main scene alone.
    Single,
    /// The compared scene left of `split` (a share of the width), the main scene right of it.
    Compare { split: f32 },
}

/// The main scene's slot, and the slot of the scene it is compared with.
pub const MAIN: usize = 0;
pub const COMPARED: usize = 1;

/// What the last picture of a slot was drawn with: the same again needs no drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Drawn {
    size: [u32; 2],
    samples: u32,
    view: View,
}

#[derive(Default)]
struct Slot {
    key: Option<String>,
    scene: Option<GpuScene>,
    /// The pictures drawn into, each with its egui texture: one for the moving size and one for
    /// the still one, so starting and stopping a turn never makes new textures.
    targets: Vec<(Target, egui::TextureId)>,
    /// The texture of the last picture drawn.
    shown: Option<egui::TextureId>,
    drawn: Option<Drawn>,
}

/// Pictures kept per slot: the moving and the still one.
const KEPT_TARGETS: usize = 2;

pub struct Stage {
    render_state: Option<RenderState>,
    gpu: Option<Gpu>,
    detected: Quality,
    /// Whether the adapter can draw the picture with 4× anti-aliasing (asked once).
    msaa: bool,
    quality: Quality,
    camera: OrbitCamera,
    slots: [Slot; 2],
}

impl Stage {
    /// A stage drawing with the window's GPU; without one (a test harness, the OpenGL-less
    /// fallback) it is unavailable and the caller shows its flat picture instead.
    pub fn new(render_state: Option<&RenderState>) -> Stage {
        let detected = render_state
            .map(|state| Quality::detect(&state.adapter.get_info()))
            .unwrap_or(Quality::Economy);
        Stage {
            gpu: render_state.map(|state| Gpu::new(&state.device, &state.queue)),
            render_state: render_state.cloned(),
            detected,
            msaa: render_state.is_some_and(|state| Gpu::supports_samples(&state.adapter, 4)),
            quality: detected,
            camera: OrbitCamera::default(),
            slots: Default::default(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.gpu.is_some()
    }

    /// The level that suits this computer's graphics.
    pub fn detected_quality(&self) -> Quality {
        self.detected
    }

    pub fn set_quality(&mut self, quality: Quality) {
        self.quality = quality;
    }

    pub fn camera(&self) -> &OrbitCamera {
        &self.camera
    }

    pub fn camera_mut(&mut self) -> &mut OrbitCamera {
        &mut self.camera
    }

    /// The key of the scene a slot shows.
    pub fn key(&self, slot: usize) -> Option<&str> {
        self.slots.get(slot)?.key.as_deref()
    }

    /// Show `scene` in a slot under `key`. The main scene also frames the camera; its pose is
    /// kept, so going from one skin of a tank to another looks at it from the same side.
    pub fn load(&mut self, slot: usize, key: &str, scene: &Scene) {
        let (Some(gpu), Some(entry)) = (&self.gpu, self.slots.get_mut(slot)) else {
            return;
        };
        entry.key = Some(key.to_owned());
        entry.scene = Some(gpu.upload(scene));
        entry.drawn = None;
        if slot == MAIN {
            let points: Vec<[f32; 3]> = scene
                .parts
                .iter()
                .flat_map(|part| {
                    part.geometry.vertices.iter().map(move |v| {
                        std::array::from_fn(|axis| v.position[axis] + part.offset[axis])
                    })
                })
                .collect();
            self.camera.frame_points(&scene.bounds(), &points);
            for entry in &mut self.slots {
                entry.drawn = None;
            }
        }
    }

    /// Forget a slot's scene; its picture is cleared.
    pub fn unload(&mut self, slot: usize) {
        if let Some(entry) = self.slots.get_mut(slot) {
            entry.key = None;
            entry.scene = None;
            entry.drawn = None;
        }
    }

    /// Draw the stage into `rect` and handle its input. `label` names it for screen readers.
    pub fn show(&mut self, ui: &mut Ui, rect: Rect, view: StageView, label: &str) -> Response {
        self.show_in(ui, rect, rect, view, label, None)
    }

    /// [`Stage::show`] drawn in `picture` but turned only from `input` (so the buttons laid over
    /// the picture keep their clicks), and painted in the shape slot `under` when given: reserved
    /// with `painter.add(Shape::Noop)` before the widgets that go over the model.
    pub fn show_in(
        &mut self,
        ui: &mut Ui,
        picture: Rect,
        input: Rect,
        view: StageView,
        label: &str,
        under: Option<egui::layers::ShapeIdx>,
    ) -> Response {
        let rect = picture;
        let response = ui.interact(
            input,
            ui.make_persistent_id("tanki-stage"),
            Sense::click_and_drag(),
        );
        response
            .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, ui.is_enabled(), label));
        let mut moving = self.handle_input(ui, &response);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        moving |= self.camera.update(dt);
        let slots: &[usize] = match view {
            StageView::Single => &[MAIN],
            StageView::Compare { .. } => &[COMPARED, MAIN],
        };
        let ppp = ui.ctx().pixels_per_point();
        for slot in slots {
            self.draw(*slot, rect.size() * ppp, moving);
        }
        let shapes: Vec<egui::Shape> = match view {
            StageView::Single => self.paint(MAIN, rect, 0.0..=1.0).into_iter().collect(),
            StageView::Compare { split } => {
                let split = split.clamp(0.0, 1.0);
                [
                    self.paint(COMPARED, rect, 0.0..=split),
                    self.paint(MAIN, rect, split..=1.0),
                ]
                .into_iter()
                .flatten()
                .collect()
            }
        };
        let painter = ui.painter_at(rect);
        match under {
            Some(slot) => painter.set(slot, egui::Shape::Vec(shapes)),
            None => {
                painter.extend(shapes);
            }
        }
        if moving {
            ui.ctx().request_repaint();
        }
        response
    }

    /// Draw the main scene at `size` pixels with the best anti-aliasing the adapter has, and
    /// read it back: a picture to save.
    pub fn snapshot(&mut self, size: [u32; 2]) -> Option<Image> {
        let state = self.render_state.as_ref()?;
        let samples = [4, 1]
            .into_iter()
            .find(|n| Gpu::supports_samples(&state.adapter, *n))?;
        let gpu = self.gpu.as_mut()?;
        let scene = self.slots.get(MAIN)?.scene.as_ref()?;
        let target = gpu.target(size, samples);
        let view = view(&self.camera, target.size);
        gpu.draw(Some(scene), &target, &view);
        gpu.read(&target)
    }

    /// Turn the camera from the mouse, the wheel, a pinch and the keyboard. Returns whether the
    /// user is moving it right now.
    fn handle_input(&mut self, ui: &Ui, response: &Response) -> bool {
        if response.drag_started() {
            self.camera.hold();
        }
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        if response.double_clicked() {
            self.camera.go_home();
        }
        let dragging = response.dragged();
        if dragging {
            let delta = response.drag_delta();
            let dt = ui.input(|i| i.stable_dt).max(1.0 / 240.0);
            self.camera.drag(glam::Vec2::new(delta.x, delta.y), dt);
        }
        if response.hovered() {
            // Take the wheel, so the page behind the stage does not scroll while zooming.
            let (wheel, pinch) = ui.input_mut(|i| {
                let wheel = i.smooth_scroll_delta.y;
                i.smooth_scroll_delta = Vec2::ZERO;
                (wheel, i.zoom_delta())
            });
            if wheel != 0.0 {
                self.camera.zoom_by((wheel * WHEEL_ZOOM).exp());
            }
            if pinch != 1.0 {
                self.camera.zoom_by(pinch);
            }
        }
        if response.has_focus() {
            let steps = [
                (Key::ArrowLeft, KEY_STEP, 0.0),
                (Key::ArrowRight, -KEY_STEP, 0.0),
                (Key::ArrowUp, 0.0, KEY_STEP),
                (Key::ArrowDown, 0.0, -KEY_STEP),
            ];
            for (key, yaw, pitch) in steps {
                if ui.input(|i| i.key_pressed(key)) {
                    self.camera.nudge(yaw, pitch);
                }
            }
            if ui.input(|i| i.key_pressed(Key::Num0) || i.key_pressed(Key::Home)) {
                self.camera.go_home();
            }
        }
        dragging
    }

    /// Draw a slot's picture if anything it shows changed since the last one.
    fn draw(&mut self, slot: usize, pixels: Vec2, moving: bool) {
        let (Some(gpu), Some(state)) = (self.gpu.as_mut(), self.render_state.as_ref()) else {
            return;
        };
        let Some(entry) = self.slots.get_mut(slot) else {
            return;
        };
        let tier = self.quality.tier();
        let (wanted, scale) = if moving {
            (tier.moving_samples, tier.moving_scale)
        } else {
            (tier.samples, 1.0)
        };
        let samples = if wanted == 1 || self.msaa { wanted } else { 1 };
        let size = picture_size(pixels * scale, tier.max_side);
        let drawn = Drawn {
            size,
            samples,
            view: view(&self.camera, size),
        };
        if entry.drawn == Some(drawn) {
            return;
        }
        let index = match entry
            .targets
            .iter()
            .position(|(t, _)| t.size == size && t.samples == samples)
        {
            Some(index) => index,
            None => {
                let target = gpu.target(size, samples);
                let mut renderer = state.renderer.write();
                if entry.targets.len() >= KEPT_TARGETS {
                    let (_, old) = entry.targets.remove(0);
                    renderer.free_texture(&old);
                }
                let id = renderer.register_native_texture(
                    &state.device,
                    &target.colour_view,
                    wgpu::FilterMode::Linear,
                );
                entry.targets.push((target, id));
                entry.targets.len() - 1
            }
        };
        if let Some((target, id)) = entry.targets.get(index) {
            gpu.draw(entry.scene.as_ref(), target, &drawn.view);
            entry.shown = Some(*id);
            entry.drawn = Some(drawn);
        }
    }

    /// The part of a slot's picture between the shares `span` of the stage's width.
    fn paint(
        &self,
        slot: usize,
        rect: Rect,
        span: std::ops::RangeInclusive<f32>,
    ) -> Option<egui::Shape> {
        let entry = self.slots.get(slot)?;
        let (Some(texture), Some(_)) = (entry.shown, &entry.scene) else {
            return None;
        };
        let (from, to) = (*span.start(), *span.end());
        if to <= from {
            return None;
        }
        let part = Rect::from_min_max(
            Pos2::new(rect.min.x + rect.width() * from, rect.min.y),
            Pos2::new(rect.min.x + rect.width() * to, rect.max.y),
        );
        let uv = Rect::from_min_max(Pos2::new(from, 0.0), Pos2::new(to, 1.0));
        Some(egui::Shape::image(texture, part, uv, Color32::WHITE))
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        if let Some(state) = &self.render_state {
            let mut renderer = state.renderer.write();
            for (_, texture) in self.slots.iter().flat_map(|slot| &slot.targets) {
                renderer.free_texture(texture);
            }
        }
    }
}

/// The camera's view for a picture of `size` pixels.
fn view(camera: &OrbitCamera, size: [u32; 2]) -> View {
    let aspect = size[0].max(1) as f32 / size[1].max(1) as f32;
    View {
        view_proj: camera.view_projection(aspect),
        eye: camera.eye(),
        lights: camera.lights(),
    }
}

/// Whole pixels, at least one, the longer side at most `max_side` (the shorter scaled with it).
fn picture_size(pixels: Vec2, max_side: u32) -> [u32; 2] {
    let pixels = Vec2::new(pixels.x.max(1.0), pixels.y.max(1.0));
    let longest = pixels.x.max(pixels.y);
    let shrink = (max_side as f32 / longest).min(1.0);
    let fitted = pixels * shrink;
    [
        (fitted.x.round() as u32).clamp(1, max_side),
        (fitted.y.round() as u32).clamp(1, max_side),
    ]
}

#[cfg(test)]
mod tests;
