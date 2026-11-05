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
    // 30 degrees, as the game's own garage pictures look down on an item.
    pitch: 0.52,
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
/// Turning by itself starts after this many seconds without input, gathers speed over
/// `AUTO_RAMP` seconds and goes on very slowly (radians per second: some 40 s a turn).
const AUTO_DELAY: f32 = 5.0;
const AUTO_RAMP: f32 = 2.0;
const AUTO_SPEED: f32 = 0.15;
/// Seconds the way back to the home view takes: it starts and ends at rest, never a jump.
const HOME_SECONDS: f32 = 0.9;
/// The model fills this share of the picture at most, seen from any side at the home pitch. The
/// Garage's picture reaches under the chips and the selector, so this is about two thirds of the
/// clear room between them: some 17% of air above and below, as product viewers and product
/// photography leave around an object.
pub const FILL_SHARE: f32 = 0.6;
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
    /// The share of the picture the model fills at most ([`FILL_SHARE`] unless set).
    fill: f32,
    /// The last framing distance worked out, and the picture's aspect it is for.
    fit: Cell<Option<(u32, f32)>>,
    /// The way back to the home view under way: where it started and how far it is (0 to 1).
    homing: Option<(Pose, f32)>,
    idle: f32,
    /// Grabbed while it was turning by itself: let go, it goes on at once rather than waiting.
    grabbed_turning: bool,
    /// Let go after that: it goes on turning at once, the flick dying down over it.
    go_on: bool,
    /// Which way it turns by itself (1 or -1): the way the hand last flicked it, so a flick slows
    /// down into turning by itself and never stops or turns back in between.
    auto_direction: f32,
    /// Turn slowly by itself after a few seconds without input: on unless turned off.
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
            fill: FILL_SHARE,
            fit: Cell::new(None),
            homing: None,
            idle: 0.0,
            grabbed_turning: false,
            go_on: false,
            auto_direction: 1.0,
            auto_rotate: true,
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
        self.grabbed_turning = self.turning_by_itself() || self.go_on;
        self.go_on = false;
        self.velocity = Vec2::ZERO;
        self.touched();
    }

    /// The model is let go: if it was turning by itself when grabbed, it goes on turning at once,
    /// at its usual speed, without the wait; a flick dies down over it.
    pub fn release(&mut self) {
        self.go_on = std::mem::take(&mut self.grabbed_turning);
        if self.velocity.x.abs() > STOP_SPEED {
            self.auto_direction = self.velocity.x.signum();
        }
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
            // Zooming while it turns by itself does not stop it.
            if !self.turning_by_itself() {
                self.touched();
            }
        }
    }

    /// How much of the picture the model fills at most (0.3 to 0.9): more for a model whose
    /// framing from every side leaves it small from the usual one.
    pub fn set_fill(&mut self, share: f32) {
        let share = if share.is_finite() {
            share.clamp(0.3, 0.9)
        } else {
            FILL_SHARE
        };
        if share != self.fill {
            self.fill = share;
            self.fit.set(None);
        }
    }

    /// Stand at `pose` at once (a view kept from before), the pitch and zoom within their limits.
    /// A pose with a number that is none is ignored.
    pub fn set_pose(&mut self, pose: Pose) {
        if ![pose.yaw, pose.pitch, pose.zoom]
            .iter()
            .all(|v| v.is_finite())
        {
            return;
        }
        self.pose = Pose {
            yaw: wrap_angle(pose.yaw),
            pitch: pose.pitch.clamp(PITCH_MIN, PITCH_MAX),
            zoom: pose.zoom.clamp(ZOOM_MIN, ZOOM_MAX),
        };
        self.target_zoom = self.pose.zoom;
        self.velocity = Vec2::ZERO;
        self.homing = None;
        self.touched();
    }

    /// The pose the camera comes to rest at (the zoom it eases to), or `None` while a flick or
    /// the way home still turns it.
    pub fn resting_pose(&self) -> Option<Pose> {
        (self.homing.is_none() && self.velocity == Vec2::ZERO).then_some(Pose {
            zoom: self.target_zoom,
            ..self.pose
        })
    }

    /// Seconds until it starts turning by itself, while it waits to.
    pub fn turns_by_itself_in(&self) -> Option<f32> {
        (self.auto_rotate && self.idle <= AUTO_DELAY).then_some(AUTO_DELAY - self.idle)
    }

    /// Whether it is turning by itself now (not just allowed to).
    pub fn turning_by_itself(&self) -> bool {
        self.auto_rotate && self.idle > AUTO_DELAY
    }

    /// Ease back to the starting view.
    pub fn go_home(&mut self) {
        self.turn_home();
        self.target_zoom = HOME.zoom;
    }

    /// Ease back to the starting view's angle, keeping the zoom (another item on the stage), and
    /// wait again before turning by itself.
    pub fn turn_home(&mut self) {
        self.homing = Some((self.pose, 0.0));
        self.velocity = Vec2::ZERO;
        self.touched();
    }

    /// Advance `dt` seconds (the time since the last frame, however long). Returns whether the
    /// camera is still moving (draw again next frame).
    pub fn update(&mut self, dt: f32) -> bool {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        // All of a long gap counts as waiting; movement steps at most a tenth of a second.
        self.idle += dt;
        let dt = dt.min(0.1);
        if std::mem::take(&mut self.go_on) {
            // Let go while it was turning by itself: on at once, at full speed.
            self.idle = AUTO_DELAY + AUTO_RAMP;
        }
        let ease = 1.0 - (-EASE * dt).exp();
        let mut moving = false;
        if let Some((from, done)) = self.homing {
            // Smoothstep: no jump in speed at either end, the short way round.
            let done = (done + dt / HOME_SECONDS).min(1.0);
            let s = done * done * (3.0 - 2.0 * done);
            self.pose.yaw = wrap_angle(from.yaw + wrap_angle(HOME.yaw - from.yaw) * s);
            self.pose.pitch = from.pitch + (HOME.pitch - from.pitch) * s;
            if done >= 1.0 {
                self.pose.yaw = HOME.yaw;
                self.pose.pitch = HOME.pitch;
                self.homing = None;
            } else {
                self.homing = Some((from, done));
                moving = true;
            }
        } else if self.velocity.length() > STOP_SPEED {
            self.turn_by(self.velocity * dt);
            self.velocity *= (-DAMPING * dt).exp();
            moving = true;
        } else {
            self.velocity = Vec2::ZERO;
        }
        // Turning by itself waits only for the way home: a flick dies down over it, and the zoom
        // settles while it turns.
        let turning = self.homing.is_some();
        let zoom = self.target_zoom - self.pose.zoom;
        if zoom.abs() > 1e-4 {
            self.pose.zoom += zoom * ease;
            moving = true;
        } else {
            self.pose.zoom = self.target_zoom;
        }
        if self.auto_rotate && self.idle > AUTO_DELAY && !turning {
            let ramp = ((self.idle - AUTO_DELAY) / AUTO_RAMP).min(1.0);
            let speed = AUTO_SPEED * ramp * ramp * (3.0 - 2.0 * ramp);
            self.turn_by(Vec2::new(self.auto_direction * speed * dt, 0.0));
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
        self.homing = None;
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
    /// the picture (its fill share) seen from any side at the home pitch, so the model is as
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
        let tan_y = (FOV_Y / 2.0).tan() * self.fill;
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
