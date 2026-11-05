// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Geometry ready to draw: indexed triangles with a normal and texture coordinates per vertex.
//!
//! Normals follow the model's smoothing groups: faces that share a vertex and a group are shaded
//! as one smooth surface, any other edge stays sharp. A model without smoothing groups is smooth
//! wherever its faces share a vertex, which is how the format's exporters meant it.

use std::collections::HashMap;

use crate::three_ds::Mesh;

/// One vertex as the renderer reads it. Positions keep the model's axes (Z up) and units.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Vertex {
    pub position: [f32; 3],
    /// Unit length.
    pub normal: [f32; 3],
    /// Texture coordinates with V down from the top of the picture, as GPUs read pictures (the
    /// file counts V up from the bottom).
    pub uv: [f32; 2],
}

/// An axis-aligned box around geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Default for Bounds {
    /// The empty box: adding any point makes it that point.
    fn default() -> Self {
        Bounds {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }
}

impl Bounds {
    pub fn is_empty(&self) -> bool {
        (0..3).any(|axis| self.min[axis] > self.max[axis])
    }

    pub fn add(&mut self, point: [f32; 3]) {
        for (axis, value) in point.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value);
            self.max[axis] = self.max[axis].max(value);
        }
    }

    pub fn union(mut self, other: Bounds) -> Bounds {
        if !other.is_empty() {
            self.add(other.min);
            self.add(other.max);
        }
        self
    }

    pub fn center(&self) -> [f32; 3] {
        if self.is_empty() {
            return [0.0; 3];
        }
        std::array::from_fn(|axis| (self.min[axis] + self.max[axis]) / 2.0)
    }

    pub fn size(&self) -> [f32; 3] {
        if self.is_empty() {
            return [0.0; 3];
        }
        std::array::from_fn(|axis| self.max[axis] - self.min[axis])
    }

    /// The radius of the sphere around the box, from its centre.
    pub fn radius(&self) -> f32 {
        length(self.size()) / 2.0
    }
}

/// Indexed triangles of one or more meshes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geometry {
    pub vertices: Vec<Vertex>,
    /// Three per triangle, each below `vertices.len()`.
    pub indices: Vec<u32>,
    pub bounds: Bounds,
}

impl Geometry {
    /// The geometry of the meshes, as one piece.
    pub fn from_meshes<'a>(meshes: impl IntoIterator<Item = &'a Mesh>) -> Geometry {
        let mut out = Geometry::default();
        for mesh in meshes {
            out.append(mesh);
        }
        out
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    fn append(&mut self, mesh: &Mesh) {
        let normals: Vec<[f32; 3]> = mesh
            .faces
            .iter()
            .map(|face| face_normal(mesh, *face))
            .collect();
        // Which faces use each vertex.
        let mut users: Vec<Vec<usize>> = vec![Vec::new(); mesh.positions.len()];
        for (index, face) in mesh.faces.iter().enumerate() {
            for corner in face {
                if let Some(list) = users.get_mut(usize::from(*corner)) {
                    list.push(index);
                }
            }
        }
        let group = |face: usize| -> u32 {
            if mesh.smoothing.is_empty() {
                1
            } else {
                mesh.smoothing.get(face).copied().unwrap_or(0)
            }
        };
        // A vertex shared by corners with the same normal is written once.
        let mut written: HashMap<(u16, [u32; 3]), u32> = HashMap::new();
        for (index, face) in mesh.faces.iter().enumerate() {
            let own = group(index);
            for corner in face {
                let Some(position) = mesh.positions.get(usize::from(*corner)).copied() else {
                    continue;
                };
                let summed = if own == 0 {
                    normals.get(index).copied().unwrap_or_default()
                } else {
                    users
                        .get(usize::from(*corner))
                        .into_iter()
                        .flatten()
                        .filter(|other| group(**other) & own != 0)
                        .filter_map(|other| normals.get(*other))
                        .fold([0.0; 3], |sum, n| add(sum, *n))
                };
                let normal = normalize(summed)
                    .or_else(|| normals.get(index).copied().and_then(normalize))
                    .unwrap_or([0.0, 0.0, 1.0]);
                let key = (*corner, normal.map(f32::to_bits));
                let next = self.vertices.len() as u32;
                let at = *written.entry(key).or_insert_with(|| next);
                if at == next {
                    let uv = mesh
                        .uvs
                        .get(usize::from(*corner))
                        .map_or([0.0; 2], |[u, v]| [*u, 1.0 - v]);
                    self.vertices.push(Vertex {
                        position,
                        normal,
                        uv,
                    });
                    self.bounds.add(position);
                }
                self.indices.push(at);
            }
        }
    }
}

/// The face's normal scaled by its area, so larger faces weigh more in a smooth vertex normal.
fn face_normal(mesh: &Mesh, [a, b, c]: [u16; 3]) -> [f32; 3] {
    let point = |i: u16| {
        mesh.positions
            .get(usize::from(i))
            .copied()
            .unwrap_or_default()
    };
    let (a, b, c) = (point(a), point(b), point(c));
    cross(sub(b, a), sub(c, a))
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn length(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
}

fn normalize(v: [f32; 3]) -> Option<[f32; 3]> {
    let len = length(v);
    (len.is_finite() && len > f32::EPSILON).then(|| v.map(|c| c / len))
}

#[cfg(test)]
mod tests;
