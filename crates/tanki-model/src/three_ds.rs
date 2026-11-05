// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Autodesk 3DS: the model format of the client's cache and of most skin packs.
//!
//! A 3DS file is a tree of chunks. The parts read here: `MAIN` > `EDITOR` > `OBJECT` (a name, then)
//! `TRIMESH` > vertices, texture coordinates and faces, with each face's smoothing groups and
//! material. Vertices are stored in the scene's space, so meshes need no transform to be drawn.

use std::fmt;

use crate::bytes::{Chunks, c_string, f32_at, u16_at, u32_at};

const MAIN: u16 = 0x4D4D;
const EDITOR: u16 = 0x3D3D;
const OBJECT: u16 = 0x4000;
const TRIMESH: u16 = 0x4100;
const VERTICES: u16 = 0x4110;
const FACES: u16 = 0x4120;
const FACE_MATERIAL: u16 = 0x4130;
const UVS: u16 = 0x4140;
const SMOOTHING: u16 = 0x4150;
const LOCAL_AXES: u16 = 0x4160;
const MATERIAL: u16 = 0xAFFF;
const MATERIAL_NAME: u16 = 0xA000;
const TEXTURE_MAP: u16 = 0xA200;
const MAP_FILE: u16 = 0xA300;

/// Larger files are refused before anything is read: no item model comes near it.
pub const MAX_BYTES: usize = 64 << 20;
/// Objects past this many are ignored.
const MAX_MESHES: usize = 1024;
/// Names longer than this are cut (the format allows ten characters).
const MAX_NAME: usize = 64;
/// How deep the fingerprint walk follows nested chunks.
const MAX_DEPTH: u8 = 8;
/// Models smaller than this (cubes, planes, shell stubs) say nothing about an item.
const MIN_FINGERPRINT_VERTICES: u32 = 20;

/// Mesh (vertices, faces) pairs of a model, sorted.
pub type Fingerprint = Vec<(u16, u16)>;

/// One object of a model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// One per position, or empty when the file has none (or not one for every vertex).
    pub uvs: Vec<[f32; 2]>,
    /// Triangles; each index is below `positions.len()` and no triangle repeats a vertex.
    pub faces: Vec<[u16; 3]>,
    /// One smoothing-group mask per face, or empty when the file has none.
    pub smoothing: Vec<u32>,
    /// The name of the first material the faces use.
    pub material: Option<String>,
    /// The object's own origin, its pivot (the vertices are in the scene's space already).
    pub origin: Option<[f32; 3]>,
}

/// Name prefixes of the helper objects a model carries besides what it shows: where a turret is
/// mounted or a flag hangs, where a shot leaves the muzzle, and the boxes the game collides with.
const HELPERS: [&str; 4] = ["box", "mount", "fmnt", "muzzle"];

impl Mesh {
    /// Whether the object is a helper, not part of what the model looks like.
    pub fn is_helper(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        HELPERS.iter().any(|prefix| name.starts_with(prefix))
    }
}

/// A material of the model: its name and the file name of its texture, as the file states them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Material {
    pub name: String,
    pub texture: Option<String>,
}

/// Every object and material of a 3DS file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    pub meshes: Vec<Mesh>,
    pub materials: Vec<Material>,
}

impl Model {
    /// The object called `name`, ignoring case.
    pub fn mesh(&self, name: &str) -> Option<&Mesh> {
        self.meshes
            .iter()
            .find(|mesh| mesh.name.eq_ignore_ascii_case(name))
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(|mesh| mesh.faces.len()).sum()
    }

    /// The objects that show the model, without its helpers.
    pub fn shown(&self) -> impl Iterator<Item = &Mesh> {
        self.meshes.iter().filter(|mesh| !mesh.is_helper())
    }

    /// Where a hull carries its turret: the origin of its mount helper.
    pub fn mount(&self) -> Option<[f32; 3]> {
        self.meshes
            .iter()
            .find(|mesh| mesh.name.to_ascii_lowercase().starts_with("mount"))?
            .origin
    }

    /// The point a turret turns around and is mounted by: the origin of its main object (named
    /// after it, else the first shown one).
    pub fn pivot(&self) -> Option<[f32; 3]> {
        self.mesh("turret").or_else(|| self.shown().next())?.origin
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// The bytes do not start like a 3DS file.
    NotA3ds,
    /// The file is larger than [`MAX_BYTES`].
    TooLarge(usize),
    /// The file has no object with a face to draw.
    NoMeshes,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelError::NotA3ds => f.write_str("not a 3DS model"),
            ModelError::TooLarge(size) => {
                write!(
                    f,
                    "the model is {size} bytes, over the {MAX_BYTES}-byte limit"
                )
            }
            ModelError::NoMeshes => f.write_str("the model has nothing to draw"),
        }
    }
}

impl std::error::Error for ModelError {}

/// Read a 3DS file. Damaged parts are skipped: an object whose data is cut short keeps what is
/// whole, an object with a non-finite vertex is dropped, and faces naming a missing vertex are too.
pub fn read(bytes: &[u8]) -> Result<Model, ModelError> {
    if bytes.len() > MAX_BYTES {
        return Err(ModelError::TooLarge(bytes.len()));
    }
    let Some(main) = Chunks::new(bytes).next().filter(|c| c.id == MAIN) else {
        return Err(ModelError::NotA3ds);
    };
    let mut model = Model::default();
    for editor in Chunks::new(main.body).filter(|c| c.id == EDITOR) {
        for chunk in Chunks::new(editor.body) {
            match chunk.id {
                OBJECT if model.meshes.len() < MAX_MESHES => {
                    model.meshes.extend(object(chunk.body));
                }
                MATERIAL => model.materials.push(material(chunk.body)),
                _ => {}
            }
        }
    }
    if model.meshes.iter().all(|mesh| mesh.faces.is_empty()) {
        return Err(ModelError::NoMeshes);
    }
    Ok(model)
}

fn name(raw: String) -> String {
    raw.chars().take(MAX_NAME).collect()
}

/// An `OBJECT` chunk: its name, then its triangle mesh (lights and cameras have none).
fn object(body: &[u8]) -> Option<Mesh> {
    let (raw, rest) = c_string(body);
    let trimesh = Chunks::new(rest).find(|c| c.id == TRIMESH)?;
    let mut mesh = Mesh {
        name: name(raw),
        ..Mesh::default()
    };
    for chunk in Chunks::new(trimesh.body) {
        match chunk.id {
            VERTICES => {
                mesh.positions = vectors(chunk.body, |b, at| {
                    Some([f32_at(b, at)?, f32_at(b, at + 4)?, f32_at(b, at + 8)?])
                })
            }
            UVS => {
                mesh.uvs = vectors(chunk.body, |b, at| {
                    Some([f32_at(b, at)?, f32_at(b, at + 4)?])
                })
            }
            FACES => faces(chunk.body, &mut mesh),
            // Three axes, then the origin: 12 floats.
            LOCAL_AXES => {
                mesh.origin = Some([
                    f32_at(chunk.body, 36),
                    f32_at(chunk.body, 40),
                    f32_at(chunk.body, 44),
                ])
                .and_then(|[x, y, z]| Some([x?, y?, z?]))
                .filter(|origin| origin.iter().all(|v| v.is_finite()));
            }
            _ => {}
        }
    }
    clean(mesh)
}

/// A counted list: a `u16` count, then that many fixed-size records (as many as the data holds).
fn vectors<T, const N: usize>(
    body: &[u8],
    read: impl Fn(&[u8], usize) -> Option<[T; N]>,
) -> Vec<[T; N]> {
    let count = u16_at(body, 0).unwrap_or(0) as usize;
    let size = N * 4;
    (0..count)
        .map_while(|index| read(body, 2 + index * size))
        .collect()
}

/// The `FACES` chunk: a counted list of (a, b, c, flags), then the faces' sub-chunks.
fn faces(body: &[u8], mesh: &mut Mesh) {
    let count = u16_at(body, 0).unwrap_or(0) as usize;
    mesh.faces = (0..count)
        .map_while(|index| {
            let at = 2 + index * 8;
            Some([
                u16_at(body, at)?,
                u16_at(body, at + 2)?,
                u16_at(body, at + 4)?,
            ])
        })
        .collect();
    let after = 2 + count * 8;
    for chunk in Chunks::new(body.get(after..).unwrap_or_default()) {
        match chunk.id {
            SMOOTHING => {
                mesh.smoothing = (0..mesh.faces.len())
                    .map_while(|index| u32_at(chunk.body, index * 4))
                    .collect();
            }
            FACE_MATERIAL if mesh.material.is_none() => {
                mesh.material = Some(name(c_string(chunk.body).0));
            }
            _ => {}
        }
    }
}

/// Keep what can be drawn: drop the mesh when a vertex is not a finite number, drop faces that
/// name a missing vertex or repeat one, and drop texture coordinates or smoothing groups that do
/// not cover every vertex or face.
fn clean(mut mesh: Mesh) -> Option<Mesh> {
    if mesh.positions.iter().flatten().any(|v| !v.is_finite()) {
        return None;
    }
    if mesh.uvs.len() != mesh.positions.len() || mesh.uvs.iter().flatten().any(|v| !v.is_finite()) {
        mesh.uvs.clear();
    }
    if mesh.smoothing.len() != mesh.faces.len() {
        mesh.smoothing.clear();
    }
    let vertices = mesh.positions.len();
    let usable = |[a, b, c]: [u16; 3]| {
        [a, b, c].iter().all(|i| usize::from(*i) < vertices) && a != b && b != c && a != c
    };
    if mesh.smoothing.is_empty() {
        mesh.faces.retain(|face| usable(*face));
    } else {
        let (faces, smoothing) = mesh
            .faces
            .iter()
            .zip(&mesh.smoothing)
            .filter(|(face, _)| usable(**face))
            .map(|(face, group)| (*face, *group))
            .unzip();
        mesh.faces = faces;
        mesh.smoothing = smoothing;
    }
    Some(mesh)
}

/// A `MATERIAL` chunk: its name and the file of its texture map.
fn material(body: &[u8]) -> Material {
    let mut material = Material::default();
    for chunk in Chunks::new(body) {
        match chunk.id {
            MATERIAL_NAME => material.name = name(c_string(chunk.body).0),
            TEXTURE_MAP => {
                material.texture = Chunks::new(chunk.body)
                    .find(|c| c.id == MAP_FILE)
                    .map(|file| name(c_string(file.body).0));
            }
            _ => {}
        }
    }
    material
}

/// A geometry fingerprint: the vertex and face counts of the model's meshes, or `None` when the
/// bytes are not a 3DS file or too small a model to name an item.
///
/// The cache holds the current version of a model and a pack often ships an older export of the
/// same item, so the files differ byte for byte while the mesh is the same. The counts survive
/// that, and a model of a tank or a weapon has too many vertices to collide with another item by
/// chance. Only the counts are read, so a file cut short after them still has a fingerprint.
pub fn fingerprint(bytes: &[u8]) -> Option<Fingerprint> {
    if u16_at(bytes, 0) != Some(MAIN) {
        return None;
    }
    let mut meshes = Vec::new();
    walk(bytes, None, &mut meshes, 0);
    meshes.retain(|(vertices, _)| *vertices > 0);
    let total: u32 = meshes.iter().map(|(v, _)| u32::from(*v)).sum();
    if total < MIN_FINGERPRINT_VERTICES {
        return None;
    }
    meshes.sort_unstable();
    Some(meshes)
}

fn walk(bytes: &[u8], mesh: Option<usize>, meshes: &mut Vec<(u16, u16)>, depth: u8) {
    if depth > MAX_DEPTH {
        return;
    }
    for chunk in Chunks::new(bytes) {
        match chunk.id {
            MAIN | EDITOR | TRIMESH => walk(chunk.body, mesh, meshes, depth + 1),
            // The object's name is a zero-terminated string before its sub-chunks.
            OBJECT => {
                if let Some(name) = chunk.body.iter().position(|c| *c == 0) {
                    meshes.push((0, 0));
                    let index = meshes.len() - 1;
                    let rest = chunk.body.get(name + 1..).unwrap_or_default();
                    walk(rest, Some(index), meshes, depth + 1);
                }
            }
            VERTICES | FACES => {
                if let Some(counts) = mesh.and_then(|index| meshes.get_mut(index)) {
                    let count = u16_at(chunk.body, 0).unwrap_or(0);
                    if chunk.id == VERTICES {
                        counts.0 = count;
                    } else {
                        counts.1 = count;
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests;
