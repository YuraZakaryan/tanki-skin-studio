// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use tanki_model::read;
use tanki_testkit::three_ds::{hull_3ds, turret_3ds};

use super::*;

#[test]
fn images_must_fill_their_size_within_limits() {
    assert!(Image::new(2, 2, vec![0; 16]).is_some());
    assert!(Image::new(2, 2, vec![0; 15]).is_none());
    assert!(Image::new(0, 2, Vec::new()).is_none());
    assert!(
        Image::new(
            MAX_IMAGE_SIDE + 1,
            1,
            vec![0; (MAX_IMAGE_SIDE as usize + 1) * 4]
        )
        .is_none()
    );
}

#[test]
fn the_mip_chain_halves_down_to_one_pixel_and_averages() {
    // 4 × 2: left half black, right half white.
    let mut rgba = Vec::new();
    for _ in 0..2 {
        rgba.extend([
            0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        ]);
    }
    let chain = Image::new(4, 2, rgba).unwrap().mip_chain();
    let sizes: Vec<_> = chain.iter().map(|i| (i.width(), i.height())).collect();
    assert_eq!(sizes, [(4, 2), (2, 1), (1, 1)]);
    assert_eq!(chain[1].rgba(), [0, 0, 0, 255, 255, 255, 255, 255]);
    assert_eq!(chain[2].rgba(), [128, 128, 128, 255]);
}

#[test]
fn odd_sizes_keep_their_last_row_and_column() {
    let chain = Image::new(3, 1, vec![9; 12]).unwrap().mip_chain();
    let sizes: Vec<_> = chain.iter().map(|i| (i.width(), i.height())).collect();
    assert_eq!(sizes, [(3, 1), (1, 1)]);
    assert_eq!(chain[1].rgba(), [9, 9, 9, 9]);
}

#[test]
fn scene_bounds_follow_each_parts_offset() {
    let hull = Geometry::from_meshes(read(&hull_3ds()).unwrap().shown());
    let turret = Geometry::from_meshes(read(&turret_3ds()).unwrap().shown());
    let scene = Scene {
        parts: vec![
            Part {
                geometry: Arc::new(hull),
                material: Material::default(),
                offset: [0.0; 3],
            },
            Part {
                geometry: Arc::new(turret),
                material: Material::default(),
                offset: [0.0, 0.0, 110.0],
            },
        ],
    };
    let bounds = scene.bounds();
    assert_eq!(bounds.min, [-150.0, -250.0, 0.0]);
    assert_eq!(bounds.max, [150.0, 330.0, 170.0]);
    assert!(!scene.is_empty());
    assert!(Scene::default().is_empty());
}
