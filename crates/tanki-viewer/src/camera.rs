// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A camera that circles the model: dragged by the mouse, it keeps turning a little after a flick
//! and slows down, zooms smoothly, eases back to the starting view, and can turn by itself while
//! nobody touches it. Pure arithmetic over time steps, so every behaviour is unit-tested.

use std::{
    cell::Cell,
    f32::consts::{PI, TAU},
};

use glam::{Mat4, Vec2, Vec3, camera::rh};
use tanki_model::Bounds;

/// The directions the studio's lights come from (unit vectors toward the lights): the key above and
/// to the left of the camera, so the shape reads; the fill low on the other side, so the shadows
/// keep their detail; the rim from behind, so the silhouette stands out from the background.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lights {
    pub key: Vec3,
    pub fill: Vec3,
    pub rim: Vec3,
    /// A faint light from the camera and a little below it, so the front and the lower hull,
    /// which face the viewer, never sink into the shadow.
    pub front: Vec3,
}

/// Where the camera stands: around the vertical axis, above the ground, and how far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Radians around the vertical (Z) axis, 0 looking from +X.
    pub yaw: f32,
    /// Radians above the ground plane.
    pub pitch: f32,
    /// 1 frames the whole model; smaller is closer.
    pub zoom: f32,
}

/// A three-quarter view from the front left and a little above, the way a garage shows a tank.
pub const HOME: Pose = Pose {
    yaw: 2.36,
    pitch: 0.36,
    zoom: 1.0,
};

/// Vertical field of view: narrow, like a product photo, so the model is not distorted.
const FOV_Y: f32 = 30.0 * PI / 180.0;
/// How far one point of mouse travel turns the camera.
const RADIANS_PER_POINT: f32 = 0.009;
/// How fast a flick dies down, per second.
const DAMPING: f32 = 5.0;
/// Below this speed (radians per second) the camera stops.
const STOP_SPEED: f32 = 0.03;
/// How quickly zoom and the way home catch up, per second.
const EASE: f32 = 12.0;
const PITCH_MIN: f32 = 0.02;
const PITCH_MAX: f32 = 1.35;
const ZOOM_MIN: f32 = 0.45;
const ZOOM_MAX: f32 = 2.2;
/// Turning by itself starts after this many seconds without input.
const AUTO_DELAY: f32 = 3.0;
const AUTO_SPEED: f32 = 0.35;
/// The model fills this share of the picture at most, seen from any side at the home pitch. The
/// Garage's picture reaches under the chips and the selector, so this is about two thirds of the
/// clear room between them: some 17% of air above and below, as product viewers and product
/// photography leave around an object.
const FILL_SHARE: f32 = 0.6;
/// Sides the framing checks the model from, all the way round.
const FRAMING_TURNS: usize = 24;
/// Points of a model the framing looks at, at most (spread over all of its vertices).
const FRAMING_POINTS: usize = 2048;

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    pose: Pose,
    target_zoom: f32,
    /// Yaw and pitch speed in radians per second, left over from a flick.
    velocity: Vec2,
    centre: Vec3,
    radius: f32,
    /// Points of the model, from its centre, that must stay in the picture.
    points: Vec<Vec3>,
    /// The last framing distance worked out, and the picture's aspect it is for.
    fit: Cell<Option<(u32, f32)>>,
    going_home: bool,
    idle: f32,
    /// Turn slowly by itself after a few seconds without input.
    pub auto_rotate: bool,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        OrbitCamera {
            pose: HOME,
            target_zoom: HOME.zoom,
            velocity: Vec2::ZERO,
            centre: Vec3::ZERO,
            radius: 1.0,
            points: Vec::new(),
            fit: Cell::new(None),
            going_home: false,
            idle: 0.0,
            auto_rotate: false,
        }
    }
}

impl OrbitCamera {
    pub fn pose(&self) -> Pose {
        self.pose
    }

    /// Look at a new model: centre on it and size the view to it. The pose is kept.
    pub fn frame(&mut self, bounds: &Bounds) {
        let centre = bounds.center();
        self.centre = Vec3::from_array(centre);
        self.radius = bounds.radius().max(1e-3);
        let half = Vec3::from_array(bounds.size()) / 2.0;
        self.points = (0..8)
            .map(|corner| {
                let sign = |bit: usize| if corner & bit == 0 { -1.0 } else { 1.0 };
                half * Vec3::new(sign(1), sign(2), sign(4))
            })
            .collect();
        self.fit.set(None);
    }

    /// Look at a new model, framed on its own points rather than its box: a box's corners are
    /// mostly empty around a tank (a sloped front, tracks below the hull), so the model can come
    /// closer. `points` are where the model's vertices stand.
    pub fn frame_points(&mut self, bounds: &Bounds, points: &[[f32; 3]]) {
        self.frame(bounds);
        if points.is_empty() {
            return;
        }
        let step = points.len().div_ceil(FRAMING_POINTS).max(1);
        self.points = points
            .iter()
            .step_by(step)
            .map(|p| Vec3::from_array(*p) - self.centre)
            .collect();
    }

    /// The mouse moved by `delta` points over `dt` seconds while dragging.
    pub fn drag(&mut self, delta: Vec2, dt: f32) {
        let turn = Vec2::new(-delta.x, delta.y) * RADIANS_PER_POINT;
        self.turn(turn);
        if dt > f32::EPSILON {
            // Average with the last speed so one jittery frame does not decide the flick.
            self.velocity = (self.velocity + turn / dt) / 2.0;
        }
    }

    /// Grabbing the model stops it.
    pub fn hold(&mut self) {
        self.velocity = Vec2::ZERO;
        self.touched();
    }

    /// Turn by a fixed step (the keyboard): `yaw` and `pitch` in radians.
    pub fn nudge(&mut self, yaw: f32, pitch: f32) {
        self.velocity = Vec2::ZERO;
        self.turn(Vec2::new(yaw, pitch));
    }

    /// Zoom in by `factor` (above 1) or out (below 1), eased.
    pub fn zoom_by(&mut self, factor: f32) {
        if factor.is_finite() && factor > 0.0 {
            self.target_zoom = (self.target_zoom / factor).clamp(ZOOM_MIN, ZOOM_MAX);
            self.touched();
        }
    }

    /// Ease back to the starting view.
    pub fn go_home(&mut self) {
        self.going_home = true;
        self.velocity = Vec2::ZERO;
        self.target_zoom = HOME.zoom;
        self.touched();
    }

    /// Advance `dt` seconds. Returns whether the camera is still moving (draw again next frame).
    pub fn update(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.25);
        self.idle += dt;
        let ease = 1.0 - (-EASE * dt).exp();
        let mut moving = false;
        if self.going_home {
            let yaw = wrap_angle(HOME.yaw - self.pose.yaw);
            let pitch = HOME.pitch - self.pose.pitch;
            if yaw.abs() < 1e-3 && pitch.abs() < 1e-3 {
                self.pose.yaw = HOME.yaw;
                self.pose.pitch = HOME.pitch;
                self.going_home = false;
            } else {
                self.pose.yaw += yaw * ease;
                self.pose.pitch += pitch * ease;
                moving = true;
            }
        } else if self.velocity.length() > STOP_SPEED {
            self.turn_by(self.velocity * dt);
            self.velocity *= (-DAMPING * dt).exp();
            moving = true;
        } else {
            self.velocity = Vec2::ZERO;
        }
        let zoom = self.target_zoom - self.pose.zoom;
        if zoom.abs() > 1e-4 {
            self.pose.zoom += zoom * ease;
            moving = true;
        } else {
            self.pose.zoom = self.target_zoom;
        }
        if self.auto_rotate && self.idle > AUTO_DELAY && !moving {
            self.turn_by(Vec2::new(AUTO_SPEED * dt, 0.0));
            moving = true;
        }
        moving
    }

    /// The camera's position.
    pub fn eye(&self) -> Vec3 {
        self.centre + self.direction() * self.distance(1.0)
    }

    /// The view and projection for a picture `aspect` times as wide as it is tall (wgpu's 0..1
    /// depth range).
    pub fn view_projection(&self, aspect: f32) -> Mat4 {
        let aspect = if aspect.is_finite() && aspect > 0.0 {
            aspect
        } else {
            1.0
        };
        let distance = self.distance(aspect);
        let eye = self.centre + self.direction() * distance;
        let near = (distance - self.radius * 1.6).max(distance * 0.02);
        let far = distance + self.radius * 1.6;
        let view = rh::view::look_at_mat4(eye, self.centre, Vec3::Z);
        // wgpu's clip space is Direct3D's: depth from 0 to 1.
        rh::proj::directx::perspective(FOV_Y, aspect, near, far) * view
    }

    /// The studio's three lights, which move with the camera so the model is always lit the same
    /// way from where it is seen.
    pub fn lights(&self) -> Lights {
        let toward = self.direction();
        // The viewer's left: the camera looks along -toward with Z up.
        let left = toward.cross(Vec3::Z).normalize_or_zero();
        Lights {
            key: (toward + left * 1.3 + Vec3::Z * 1.2).normalize_or(Vec3::Z),
            fill: (toward - left * 0.9 + Vec3::Z * 0.25).normalize_or(Vec3::Z),
            rim: (-toward + Vec3::Z * 1.0).normalize_or(Vec3::Z),
            front: (toward - Vec3::Z * 0.25).normalize_or(Vec3::Z),
        }
    }

    fn touched(&mut self) {
        self.idle = 0.0;
    }

    fn turn(&mut self, by: Vec2) {
        self.going_home = false;
        self.touched();
        self.turn_by(by);
    }

    fn turn_by(&mut self, by: Vec2) {
        self.pose.yaw = (self.pose.yaw + by.x).rem_euclid(TAU);
        self.pose.pitch = (self.pose.pitch + by.y).clamp(PITCH_MIN, PITCH_MAX);
    }

    /// Unit vector from the centre toward the camera.
    fn direction(&self) -> Vec3 {
        let (sin_yaw, cos_yaw) = self.pose.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pose.pitch.sin_cos();
        Vec3::new(cos_pitch * cos_yaw, cos_pitch * sin_yaw, sin_pitch)
    }

    /// How far the camera must stand for the model's sphere to fit both ways, times the zoom.
    /// How far the camera stands, times the zoom: the nearest distance from which the model fits
    /// the picture (its `FILL_SHARE`) seen from any side at the home pitch, so the model is as
    /// large as it can be and keeps its size while it turns.
    fn distance(&self, aspect: f32) -> f32 {
        let fit = match self.fit.get() {
            Some((bits, fit)) if bits == aspect.to_bits() => fit,
            _ => {
                let fit = self.fit_distance(aspect);
                self.fit.set(Some((aspect.to_bits(), fit)));
                fit
            }
        };
        fit * self.pose.zoom
    }

    fn fit_distance(&self, aspect: f32) -> f32 {
        let tan_y = (FOV_Y / 2.0).tan() * FILL_SHARE;
        let tan_x = tan_y * aspect;
        let (sin_pitch, cos_pitch) = HOME.pitch.sin_cos();
        let mut fit: f32 = 0.0;
        for turn in 0..FRAMING_TURNS {
            let yaw = turn as f32 * TAU / FRAMING_TURNS as f32;
            let (sin_yaw, cos_yaw) = yaw.sin_cos();
            let toward = Vec3::new(cos_pitch * cos_yaw, cos_pitch * sin_yaw, sin_pitch);
            let right = toward.cross(Vec3::Z).normalize_or(Vec3::X);
            let up = right.cross(toward);
            for p in &self.points {
                // A point at depth (d - p·toward) must have its sides within the view's tangents.
                let depth = p.dot(toward);
                fit = fit
                    .max(depth + p.dot(right).abs() / tan_x)
                    .max(depth + p.dot(up).abs() / tan_y);
            }
        }
        fit.max(self.radius * 0.5)
    }
}

/// The same angle within (−π, π].
fn wrap_angle(angle: f32) -> f32 {
    let wrapped = (angle + PI).rem_euclid(TAU) - PI;
    if wrapped <= -PI {
        wrapped + TAU
    } else {
        wrapped
    }
}

#[cfg(test)]
mod tests;
