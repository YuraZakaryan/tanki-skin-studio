// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Layer 1: a 3D stage for egui.
//!
//! A [`Scene`] (meshes with their textures, built on the CPU) is drawn by a [`Stage`] into a texture
//! of its own with wgpu, and egui shows that texture like any picture. The stage draws a new frame
//! only when the view changes: an untouched stage costs the GPU nothing. While the model turns it
//! draws at a lower resolution on weaker graphics ([`Quality`]), and the full picture the moment
//! it stops. Nothing here knows about caches or packs.

pub mod camera;
mod gpu;
pub mod quality;
pub mod scene;
pub mod stage;

pub use camera::OrbitCamera;
pub use quality::{Quality, QualityPreference, Tier};
pub use scene::{Image, Material, Part, Scene};
pub use stage::{Stage, StageView};
