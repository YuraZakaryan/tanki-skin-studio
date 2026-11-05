// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use tanki_testkit::{
    jpeg, png,
    three_ds::{hull_3ds, model_3ds},
};

use super::*;

fn look(details: Option<Vec<u8>>, alpha: Option<Vec<u8>>) -> LookFiles {
    LookFiles {
        model: hull_3ds(),
        details,
        details_alpha: alpha,
        lightmap: Some(jpeg(8, 8, [200, 200, 200])),
        paint: None,
    }
}

#[test]
fn a_look_becomes_one_textured_part() {
    let scene = build_scene(&look(Some(jpeg(16, 16, [10, 120, 40])), None)).unwrap();
    assert_eq!(scene.parts.len(), 1);
    let part = &scene.parts[0];
    assert_eq!(part.geometry.triangle_count(), 24);
    let details = part.material.details.as_ref().unwrap();
    assert_eq!((details.width(), details.height()), (16, 16));
    assert!(part.material.lightmap.is_some());
}

#[test]
fn the_separate_alpha_picture_becomes_the_details_alpha_even_at_another_size() {
    let scene = build_scene(&look(
        Some(jpeg(16, 16, [10, 120, 40])),
        Some(jpeg(4, 4, [0, 0, 0])),
    ))
    .unwrap();
    let details = scene.parts[0].material.details.as_ref().unwrap();
    assert!(details.rgba().as_chunks::<4>().0.iter().all(|p| p[3] < 8));
}

#[test]
fn a_painted_look_lays_the_paint_s_picture_under_the_details() {
    let mut files = look(Some(png(8, 8, [1, 2, 3, 0])), None);
    files.paint = Some(jpeg(32, 32, [180, 30, 30]));
    let scene = build_scene(&files).unwrap();
    let paint = scene.parts[0].material.paint_picture.as_ref().unwrap();
    assert_eq!((paint.width(), paint.height()), (32, 32));
    assert!(paint.rgba()[0] > 150, "red paint: {:?}", &paint.rgba()[..4]);
}

#[test]
fn a_png_keeps_its_own_alpha() {
    let scene = build_scene(&look(Some(png(8, 8, [1, 2, 3, 77])), None)).unwrap();
    let details = scene.parts[0].material.details.as_ref().unwrap();
    assert!(details.rgba().as_chunks::<4>().0.iter().all(|p| p[3] == 77));
}

#[test]
fn a_look_without_a_drawable_model_or_with_a_broken_texture_is_an_error() {
    let mut files = look(None, None);
    files.model = b"not a model".to_vec();
    assert!(build_scene(&files).is_err());
    let mut files = look(Some(b"not a picture".to_vec()), None);
    files.model = model_3ds(&[]);
    assert!(build_scene(&files).is_err());
}

#[test]
fn a_look_that_cannot_be_read_is_not_tried_again() {
    let mut preview = Preview3d::new(true, QualityPreference::Auto, Stage::new(None));
    preview.background = false;
    assert!(!preview.active(), "no GPU in a test");
    let missing = Keyed::new(LookSource::Cache(vec![(
        "object.3ds".into(),
        PathBuf::from("/nowhere/object.3ds"),
        1,
    )]));
    assert_eq!(
        preview.want(Preview3d::MAIN, Some(&missing)),
        SlotState::Unavailable
    );
    assert!(!preview.poll());
    assert_eq!(
        preview.want(Preview3d::MAIN, Some(&missing)),
        SlotState::Unavailable
    );
    assert_eq!(preview.want(Preview3d::MAIN, None), SlotState::Unavailable);
}

#[test]
fn another_item_starts_from_home_and_stops_comparing() {
    let mut preview = Preview3d::new(true, QualityPreference::Economy, Stage::new(None));
    preview.frame_item("hull:wasp");
    preview.toggle_compare();
    assert_eq!(preview.compare, Some(0.5));
    preview.frame_item("hull:wasp");
    assert_eq!(preview.compare, Some(0.5), "same item: kept");
    preview.frame_item("hull:hornet");
    assert_eq!(preview.compare, None);
    preview.toggle_auto_rotate();
    assert!(preview.auto_rotating());
}
