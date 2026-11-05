// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const FRAME: f32 = 1.0 / 60.0;

fn camera() -> OrbitCamera {
    let mut camera = OrbitCamera::default();
    let mut bounds = Bounds::default();
    bounds.add([-150.0, -250.0, 0.0]);
    bounds.add([150.0, 330.0, 170.0]);
    camera.frame(&bounds);
    camera
}

/// Run until the camera stops, at most `seconds`; returns how long that took.
fn settle(camera: &mut OrbitCamera, seconds: f32) -> f32 {
    let mut time = 0.0;
    while camera.update(FRAME) {
        time += FRAME;
        assert!(time < seconds, "still moving after {seconds} s");
    }
    time
}

#[test]
fn dragging_left_turns_the_camera_and_a_flick_slows_to_a_stop() {
    let mut camera = camera();
    camera.drag(Vec2::new(-40.0, 0.0), FRAME);
    let after_drag = camera.pose().yaw;
    assert!(after_drag > HOME.yaw);
    let time = settle(&mut camera, 3.0);
    assert!(time > 0.1, "a flick keeps turning for a moment");
    assert!(camera.pose().yaw > after_drag);
    assert!(!camera.update(FRAME));
}

#[test]
fn holding_the_model_stops_a_flick() {
    let mut camera = camera();
    camera.drag(Vec2::new(-40.0, 0.0), FRAME);
    camera.hold();
    assert!(!camera.update(FRAME));
}

#[test]
fn pitch_stays_above_the_ground_and_below_the_top() {
    let mut camera = camera();
    camera.drag(Vec2::new(0.0, 10_000.0), FRAME);
    assert_eq!(camera.pose().pitch, PITCH_MAX);
    camera.drag(Vec2::new(0.0, -20_000.0), FRAME);
    assert_eq!(camera.pose().pitch, PITCH_MIN);
}

#[test]
fn zoom_eases_to_its_target_within_limits() {
    let mut camera = camera();
    camera.zoom_by(100.0);
    settle(&mut camera, 2.0);
    assert!((camera.pose().zoom - ZOOM_MIN).abs() < 1e-3);
    camera.zoom_by(0.0001);
    settle(&mut camera, 2.0);
    assert!((camera.pose().zoom - ZOOM_MAX).abs() < 1e-3);
    // Nonsense factors are ignored.
    camera.zoom_by(f32::NAN);
    camera.zoom_by(-2.0);
    assert!(!camera.update(FRAME));
}

#[test]
fn home_takes_the_short_way_back_to_the_starting_view() {
    let mut camera = camera();
    camera.nudge(3.0, 0.5);
    camera.zoom_by(2.0);
    camera.update(FRAME);
    camera.go_home();
    settle(&mut camera, 2.0);
    assert_eq!(camera.pose().yaw, HOME.yaw);
    assert_eq!(camera.pose().pitch, HOME.pitch);
    assert!((camera.pose().zoom - HOME.zoom).abs() < 1e-3);
}

#[test]
fn auto_rotation_waits_for_the_user_to_let_go() {
    let mut camera = camera();
    camera.auto_rotate = true;
    let mut time = 0.0;
    while time < AUTO_DELAY - 0.1 {
        assert!(!camera.update(FRAME));
        time += FRAME;
    }
    for _ in 0..20 {
        camera.update(FRAME);
    }
    assert!(camera.update(FRAME), "turning by itself");
    assert_ne!(camera.pose().yaw, HOME.yaw);
    camera.hold();
    assert!(!camera.update(FRAME), "a touch pauses it");
}

#[test]
fn the_model_centre_projects_to_the_middle_and_fits_the_picture() {
    let camera = camera();
    for aspect in [0.5, 1.0, 2.5] {
        let clip = camera.view_projection(aspect) * camera.centre.extend(1.0);
        let ndc = clip.truncate() / clip.w;
        assert!(ndc.x.abs() < 1e-4 && ndc.y.abs() < 1e-4, "{ndc:?}");
        assert!((0.0..1.0).contains(&ndc.z));
    }
    assert!(camera.eye().distance(camera.centre) > camera.radius);
    let lights = camera.lights();
    assert!(lights.key.z > 0.0 && lights.fill.z > 0.0 && lights.rim.z > 0.0);
    let toward = camera.eye() - camera.centre;
    assert!(lights.key.dot(toward) > 0.0, "the key lights the side seen");
    assert!(lights.rim.dot(toward) < 0.0, "the rim comes from behind");
    let flat = |v: Vec3| Vec3::new(v.x, v.y, 0.0);
    let side = |v: Vec3| flat(v).cross(flat(toward)).z.signum();
    assert_ne!(
        side(lights.key),
        side(lights.fill),
        "key and fill on either side"
    );
}

#[test]
fn the_whole_model_fits_the_stage_from_any_side() {
    // A long, low hull, on a stage twice as wide as it is tall (as in the Garage) and a square one.
    let mut camera = camera();
    for (aspect, turn) in [(2.0, 0.0), (1.4, 0.8), (1.0, 1.9), (2.0, 3.0), (1.6, 4.4)] {
        camera.nudge(turn, 0.0);
        let view_projection = camera.view_projection(aspect);
        for corner in 0..8 {
            let point = Vec3::new(
                if corner & 1 == 0 { -150.0 } else { 150.0 },
                if corner & 2 == 0 { -250.0 } else { 330.0 },
                if corner & 4 == 0 { 0.0 } else { 170.0 },
            );
            let clip = view_projection * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(
                ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0,
                "aspect {aspect}: corner {point} at {ndc}"
            );
        }
    }
}

#[test]
fn a_model_framed_on_its_points_comes_closer_than_its_box_and_still_fits() {
    // A wedge: its box's upper back corners are empty.
    let points = [
        [-150.0, -250.0, 0.0],
        [150.0, -250.0, 0.0],
        [-150.0, 330.0, 0.0],
        [150.0, 330.0, 0.0],
        [-150.0, 330.0, 170.0],
        [150.0, 330.0, 170.0],
    ];
    let mut bounds = Bounds::default();
    for p in points {
        bounds.add(p);
    }
    let by_box = camera();
    let mut by_points = OrbitCamera::default();
    by_points.frame_points(&bounds, &points);
    let distance = |c: &OrbitCamera| c.eye().distance(c.centre);
    assert!(distance(&by_points) < distance(&by_box));
    for turn in [0.0, 1.0, 2.5, 4.0] {
        by_points.nudge(turn, 0.0);
        let view_projection = by_points.view_projection(2.0);
        for p in points {
            let clip = view_projection * Vec3::from_array(p).extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0, "{p:?} at {ndc}");
        }
    }
}

#[test]
fn angles_wrap_into_one_turn() {
    assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-5);
    assert!((wrap_angle(-PI / 2.0) + PI / 2.0).abs() < 1e-6);
    assert!((wrap_angle(TAU + 0.5) - 0.5).abs() < 1e-5);
}
