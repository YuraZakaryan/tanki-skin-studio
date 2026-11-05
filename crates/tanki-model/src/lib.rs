// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Layer 0: read 3D models into plain meshes and build geometry ready to draw.
//!
//! Model files come from a game cache or a pack and are untrusted: every read is bounds-checked,
//! sizes are capped and a broken file gives an error or fewer meshes, never a panic. Nothing here
//! knows about caches, packs or the GPU.

mod bytes;
pub mod geometry;
pub mod three_ds;

pub use geometry::{Bounds, Geometry, Vertex};
pub use three_ds::{Fingerprint, Mesh, Model, ModelError, fingerprint, read};
