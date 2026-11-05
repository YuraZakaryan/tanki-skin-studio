// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use tanki_testkit::three_ds::{Mesh3ds, hull_3ds, model_3ds};

use super::*;
use crate::three_ds::read;

/// Two triangles of a roof folded along the X axis, sharing vertices 0 and 1.
fn roof(smoothing: Vec<u32>) -> Mesh {
    Mesh {
        name: "roof".into(),
        positions: vec![
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
        ],
        faces: vec![[0, 1, 2], [1, 0, 3]],
        smoothing,
        ..Mesh::default()
    }
}

#[test]
fn a_box_keeps_sharp_sides_with_axis_normals() {
    let model = read(&model_3ds(&[Mesh3ds::cuboid(
        "box",
        [0.0; 3],
        [2.0, 4.0, 6.0],
    )]))
    .unwrap();
    let geometry = Geometry::from_meshes(&model.meshes);
    assert_eq!(geometry.vertices.len(), 24);
    assert_eq!(geometry.triangle_count(), 12);
    assert!(geometry.indices.iter().all(|i| (*i as usize) < 24));
    for vertex in &geometry.vertices {
        assert_eq!(vertex.normal.iter().filter(|c| c.abs() == 1.0).count(), 1);
    }
    // The top side faces up.
    assert!(
        geometry
            .vertices
            .iter()
            .any(|v| v.position[2] == 6.0 && v.normal == [0.0, 0.0, 1.0])
    );
    assert_eq!(geometry.bounds.min, [0.0; 3]);
    assert_eq!(geometry.bounds.max, [2.0, 4.0, 6.0]);
    assert_eq!(geometry.bounds.center(), [1.0, 2.0, 3.0]);
    // The file's V counts up from the bottom of the picture; the GPU's counts down from the top.
    let corners: Vec<[f32; 2]> = geometry.vertices.iter().map(|v| v.uv).collect();
    assert!(corners.contains(&[0.0, 1.0]) && corners.contains(&[1.0, 0.0]));
    assert_eq!(
        model.meshes[0].uvs[0],
        [0.0, 0.0],
        "the mesh keeps the file's values"
    );
    assert_eq!(geometry.vertices[0].uv, [0.0, 1.0]);
}

#[test]
fn faces_in_one_smoothing_group_share_their_vertices() {
    let geometry = Geometry::from_meshes([&roof(vec![1, 1])]);
    assert_eq!(geometry.vertices.len(), 4);
    let ridge = geometry
        .vertices
        .iter()
        .find(|v| v.position == [0.0, 0.0, 1.0])
        .unwrap();
    // Averaged between the two slopes: straight up.
    assert!(ridge.normal[2] > 0.99, "{:?}", ridge.normal);
}

#[test]
fn faces_in_different_groups_or_none_split_their_vertices() {
    assert_eq!(Geometry::from_meshes([&roof(vec![1, 2])]).vertices.len(), 6);
    assert_eq!(Geometry::from_meshes([&roof(vec![0, 0])]).vertices.len(), 6);
    // No smoothing at all: smooth across shared vertices.
    assert_eq!(Geometry::from_meshes([&roof(Vec::new())]).vertices.len(), 4);
}

#[test]
fn several_meshes_become_one_piece_with_offset_indices() {
    let model = read(&hull_3ds()).unwrap();
    let geometry = Geometry::from_meshes(model.shown());
    assert_eq!(geometry.vertices.len(), 48);
    assert_eq!(geometry.triangle_count(), 24);
    assert!(geometry.indices.iter().any(|i| *i >= 24));
    assert_eq!(geometry.bounds.max[2], 110.0);
}

#[test]
fn empty_bounds_are_harmless() {
    let bounds = Bounds::default();
    assert!(bounds.is_empty());
    assert_eq!(bounds.center(), [0.0; 3]);
    assert_eq!(bounds.radius(), 0.0);
    let mut one = Bounds::default();
    one.add([1.0, 2.0, 3.0]);
    assert_eq!(bounds.union(one), one);
}
