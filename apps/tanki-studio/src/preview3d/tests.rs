// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use tanki_testkit::{
    jpeg, png,
    three_ds::{HULL_MOUNT, Mesh3ds, hull_3ds, model_3ds, turret_3ds},
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
    let scene = build_scene(&[look(Some(jpeg(16, 16, [10, 120, 40])), None)]).unwrap();
    assert_eq!(scene.parts.len(), 1);
    let part = &scene.parts[0];
    assert_eq!(part.geometry.triangle_count(), 24);
    let details = part.material.details.as_ref().unwrap();
    assert_eq!((details.width(), details.height()), (16, 16));
    assert!(part.material.lightmap.is_some());
}

#[test]
fn the_separate_alpha_picture_becomes_the_details_alpha_even_at_another_size() {
    let scene = build_scene(&[look(
        Some(jpeg(16, 16, [10, 120, 40])),
        Some(jpeg(4, 4, [0, 0, 0])),
    )])
    .unwrap();
    let details = scene.parts[0].material.details.as_ref().unwrap();
    assert!(details.rgba().as_chunks::<4>().0.iter().all(|p| p[3] < 8));
}

#[test]
fn a_painted_look_lays_the_paint_s_picture_under_the_details() {
    let mut files = look(Some(png(8, 8, [1, 2, 3, 0])), None);
    files.paint = Some(jpeg(32, 32, [180, 30, 30]));
    let scene = build_scene(&[files]).unwrap();
    let paint = scene.parts[0].material.paint_picture.as_ref().unwrap();
    assert_eq!((paint.width(), paint.height()), (32, 32));
    assert!(paint.rgba()[0] > 150, "red paint: {:?}", &paint.rgba()[..4]);
}

#[test]
fn a_png_keeps_its_own_alpha() {
    let scene = build_scene(&[look(Some(png(8, 8, [1, 2, 3, 77])), None)]).unwrap();
    let details = scene.parts[0].material.details.as_ref().unwrap();
    assert!(details.rgba().as_chunks::<4>().0.iter().all(|p| p[3] == 77));
}

#[test]
fn a_look_without_a_drawable_model_or_with_a_broken_texture_is_an_error() {
    let mut files = look(None, None);
    files.model = b"not a model".to_vec();
    assert!(build_scene(&[files]).is_err());
    let mut files = look(Some(b"not a picture".to_vec()), None);
    files.model = model_3ds(&[]);
    assert!(build_scene(&[files]).is_err());
    // Helpers alone leave nothing to draw.
    let mut files = look(None, None);
    files.model = model_3ds(&[Mesh3ds::cuboid("Box01", [0.0; 3], [1.0; 3])]);
    assert!(build_scene(&[files]).is_err());
}

#[test]
fn a_tank_sets_its_turret_s_pivot_on_the_hull_s_mount_and_leaves_the_helpers_out() {
    let mut turret = look(None, None);
    turret.model = turret_3ds();
    let scene = build_scene(&[look(None, None), turret]).unwrap();
    assert_eq!(scene.parts.len(), 2);
    assert_eq!(scene.parts[0].offset, [0.0; 3]);
    assert_eq!(
        scene.parts[1].offset, HULL_MOUNT,
        "the turret's pivot is its origin"
    );
    // The hull's two blocks and the turret's own meshes: no mount or collision box.
    assert_eq!(scene.parts[0].geometry.triangle_count(), 24);
    let turret = &scene.parts[1].geometry;
    assert!(
        turret.bounds.min[2] >= 0.0,
        "the collision box is not drawn"
    );
}

#[test]
fn without_a_mount_or_pivot_the_turret_sits_on_the_middle_of_the_hull_s_top() {
    let mut hull = look(None, None);
    hull.model = model_3ds(&[Mesh3ds::cuboid(
        "hull",
        [-100.0, -200.0, 0.0],
        [100.0, 0.0, 50.0],
    )]);
    let mut turret = look(None, None);
    turret.model = model_3ds(&[Mesh3ds::cuboid(
        "turret",
        [10.0, 10.0, 5.0],
        [30.0, 50.0, 25.0],
    )]);
    let scene = build_scene(&[hull, turret]).unwrap();
    assert_eq!(
        scene.parts[1].offset,
        [0.0 - 20.0, -100.0 - 30.0, 50.0 - 5.0]
    );
}

#[test]
fn an_assembled_look_is_keyed_by_all_its_parts() {
    let part = |name: &str| LookSource::Cache(vec![("object.3ds".into(), PathBuf::from(name), 1)]);
    let one = Keyed::assembled(vec![part("/a"), part("/b")]);
    let other = Keyed::assembled(vec![part("/a"), part("/c")]);
    assert_eq!(one.parts.len(), 2);
    assert_ne!(one.key, other.key);
    assert_ne!(one.key, Keyed::new(part("/a")).key);
}

#[test]
fn the_tank_starts_as_the_starter_one_and_remembers_what_is_chosen() {
    use crate::app::Selection;
    let tank = Tank::restored(|_| None);
    assert_eq!(tank.hull, ("hunter".into(), 3));
    assert_eq!(tank.turret, ("smoky".into(), 3));
    assert_eq!(tank.paint, DEFAULT_PAINT);
    let mut tank = tank;
    let pick = |kind: &str, id: &str, grade| Selection {
        kind: kind.into(),
        id: id.into(),
        grade,
    };
    assert_eq!(
        tank.remember(&pick("hull", "viking", 1)),
        Some(("hull", "viking:m1".to_owned()))
    );
    assert_eq!(tank.remember(&pick("hull", "viking", 1)), None, "unchanged");
    assert_eq!(
        tank.remember(&pick("turret", "twins", 2)),
        Some(("turret", "twins:m2".to_owned()))
    );
    assert_eq!(
        tank.remember(&pick("paint", "red", 0)),
        Some(("paint", "red".to_owned()))
    );
    assert_eq!(tank.remember(&pick("sound", "shot", 0)), None);
    let saved: Vec<(&str, String)> = ["hull", "turret", "paint"]
        .into_iter()
        .map(|part| (part, tank.saved(part)))
        .collect();
    let again = Tank::restored(|part| {
        saved
            .iter()
            .find(|(p, _)| *p == part)
            .map(|(_, v)| v.clone())
    });
    assert_eq!(again, tank, "kept across a restart");
    let broken = Tank::restored(|part| (part != "paint").then(|| "no-grade".to_owned()));
    assert_eq!(
        broken,
        Tank::default(),
        "a broken value gives the starter part"
    );
}

#[test]
fn a_look_that_cannot_be_read_is_not_tried_again() {
    let mut preview = Preview3d::new(
        true,
        QualityPreference::Auto,
        Stage::new(None),
        true,
        Tank::default(),
    );
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
fn another_item_stops_comparing_and_keeps_the_view() {
    let mut preview = Preview3d::new(
        true,
        QualityPreference::Economy,
        Stage::new(None),
        true,
        Tank::default(),
    );
    preview.frame_item("hull:wasp");
    preview.toggle_compare();
    assert_eq!(preview.compare, Some(0.5));
    preview.frame_item("hull:wasp");
    assert_eq!(preview.compare, Some(0.5), "same item: kept");
    preview.frame_item("hull:hornet");
    assert_eq!(preview.compare, None);
    assert!(preview.auto_rotating(), "turns by itself unless turned off");
    preview.toggle_auto_rotate();
    assert!(!preview.auto_rotating());
}
