// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic 3DS models built from boxes, so model tests never need a real export.

const MAIN: u16 = 0x4D4D;
const EDITOR: u16 = 0x3D3D;
const OBJECT: u16 = 0x4000;
const TRIMESH: u16 = 0x4100;
const VERTICES: u16 = 0x4110;
const FACES: u16 = 0x4120;
const SMOOTHING: u16 = 0x4150;
const UVS: u16 = 0x4140;

/// One object of a synthetic 3DS model.
#[derive(Debug, Clone, Default)]
pub struct Mesh3ds {
    pub name: String,
    pub positions: Vec<[f32; 3]>,
    /// Empty, or one per position.
    pub uvs: Vec<[f32; 2]>,
    pub faces: Vec<[u16; 3]>,
    /// Empty, or one smoothing-group mask per face.
    pub smoothing: Vec<u32>,
}

impl Mesh3ds {
    /// An axis-aligned box from `min` to `max` (Z up, like the client's models): four vertices and
    /// two triangles per side, so every side keeps its own flat normal.
    pub fn cuboid(name: &str, min: [f32; 3], max: [f32; 3]) -> Self {
        let [x0, y0, z0] = min;
        let [x1, y1, z1] = max;
        let sides: [[[f32; 3]; 4]; 6] = [
            [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], // top
            [[x0, y1, z0], [x1, y1, z0], [x1, y0, z0], [x0, y0, z0]], // bottom
            [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], // front
            [[x1, y1, z0], [x0, y1, z0], [x0, y1, z1], [x1, y1, z1]], // back
            [[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]], // right
            [[x0, y1, z0], [x0, y0, z0], [x0, y0, z1], [x0, y1, z1]], // left
        ];
        let mut mesh = Mesh3ds {
            name: name.to_owned(),
            ..Default::default()
        };
        for (side, corners) in sides.iter().enumerate() {
            let first = (side * 4) as u16;
            mesh.positions.extend(corners);
            mesh.uvs
                .extend([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
            mesh.faces.push([first, first + 1, first + 2]);
            mesh.faces.push([first, first + 2, first + 3]);
            // Sides of a box never share a smoothing group, so their edges stay sharp.
            mesh.smoothing.extend([1 << side, 1 << side]);
        }
        mesh
    }
}

fn chunk(id: u16, body: &[u8]) -> Vec<u8> {
    let mut out = id.to_le_bytes().to_vec();
    out.extend(((body.len() + 6) as u32).to_le_bytes());
    out.extend(body);
    out
}

fn count(n: usize) -> [u8; 2] {
    (n as u16).to_le_bytes()
}

/// A 3DS file with the given objects.
pub fn model_3ds(meshes: &[Mesh3ds]) -> Vec<u8> {
    let mut editor = Vec::new();
    for mesh in meshes {
        let mut vertices = count(mesh.positions.len()).to_vec();
        for p in &mesh.positions {
            vertices.extend(p.iter().flat_map(|v| v.to_le_bytes()));
        }
        let mut faces = count(mesh.faces.len()).to_vec();
        for f in &mesh.faces {
            faces.extend(f.iter().flat_map(|v| v.to_le_bytes()));
            faces.extend(0u16.to_le_bytes());
        }
        if !mesh.smoothing.is_empty() {
            let groups: Vec<u8> = mesh
                .smoothing
                .iter()
                .flat_map(|g| g.to_le_bytes())
                .collect();
            faces.extend(chunk(SMOOTHING, &groups));
        }
        let mut trimesh = chunk(VERTICES, &vertices);
        if !mesh.uvs.is_empty() {
            let mut uvs = count(mesh.uvs.len()).to_vec();
            for uv in &mesh.uvs {
                uvs.extend(uv.iter().flat_map(|v| v.to_le_bytes()));
            }
            trimesh.extend(chunk(UVS, &uvs));
        }
        trimesh.extend(chunk(FACES, &faces));
        let mut object = format!("{}\0", mesh.name).into_bytes();
        object.extend(chunk(TRIMESH, &trimesh));
        editor.extend(chunk(OBJECT, &object));
    }
    chunk(MAIN, &chunk(EDITOR, &editor))
}

/// A 3DS file whose objects only state their (vertices, faces) counts, with no data after them:
/// enough for a geometry fingerprint, nothing to draw.
pub fn model_counts(meshes: &[(u16, u16)]) -> Vec<u8> {
    let mut editor = Vec::new();
    for (index, (vertices, faces)) in meshes.iter().enumerate() {
        let mut mesh = chunk(VERTICES, &vertices.to_le_bytes());
        mesh.extend(chunk(FACES, &faces.to_le_bytes()));
        let mut object = format!("obj{index}\0").into_bytes();
        object.extend(chunk(TRIMESH, &mesh));
        editor.extend(chunk(OBJECT, &object));
    }
    chunk(MAIN, &chunk(EDITOR, &editor))
}

/// A small synthetic hull: a body with a deck on top. Flat boxes only, no game shapes.
pub fn hull_3ds() -> Vec<u8> {
    model_3ds(&[
        Mesh3ds::cuboid("hull", [-150.0, -250.0, 0.0], [150.0, 250.0, 80.0]),
        Mesh3ds::cuboid("deck", [-110.0, -180.0, 80.0], [110.0, 160.0, 110.0]),
    ])
}

/// A small synthetic turret: a block with a barrel along +Y, its pivot at the origin.
pub fn turret_3ds() -> Vec<u8> {
    model_3ds(&[
        Mesh3ds::cuboid("turret", [-80.0, -90.0, 0.0], [80.0, 90.0, 60.0]),
        Mesh3ds::cuboid("barrel", [-12.0, 90.0, 18.0], [12.0, 330.0, 42.0]),
    ])
}
