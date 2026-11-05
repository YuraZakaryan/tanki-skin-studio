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
    assert!(camera.light().z > 0.0);
}

#[test]
fn angles_wrap_into_one_turn() {
    assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-5);
    assert!((wrap_angle(-PI / 2.0) + PI / 2.0).abs() < 1e-6);
    assert!((wrap_angle(TAU + 0.5) - 0.5).abs() < 1e-5);
}
