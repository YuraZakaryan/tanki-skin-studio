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
    // A roof: a flat base and a ridge along the middle, so every upper corner of its box is empty.
    let points = [
        [-150.0, -250.0, 0.0],
        [150.0, -250.0, 0.0],
        [-150.0, 330.0, 0.0],
        [150.0, 330.0, 0.0],
        [0.0, -250.0, 170.0],
        [0.0, 330.0, 170.0],
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
fn a_larger_fill_comes_closer_and_stays_within_its_limits() {
    let distance = |fill: f32| {
        let mut c = camera();
        c.set_fill(fill);
        c.eye().distance(c.centre)
    };
    let usual = distance(FILL_SHARE);
    assert!(distance(0.78) < usual, "a whole tank comes closer");
    assert!(distance(0.4) > usual);
    assert_eq!(distance(5.0), distance(0.9), "kept below 0.9");
    assert_eq!(distance(f32::NAN), usual, "nonsense keeps the usual share");
}

#[test]
fn turning_home_starts_and_ends_at_rest_and_keeps_the_zoom() {
    let mut camera = camera();
    camera.set_pose(Pose { yaw: -1.0, ..HOME });
    camera.zoom_by(1.5);
    settle(&mut camera, 1.0);
    let zoom = camera.pose().zoom;
    camera.turn_home();
    let mut steps = Vec::new();
    let mut last = camera.pose().yaw;
    for _ in 0..80 {
        camera.update(FRAME);
        steps.push(wrap_angle(camera.pose().yaw - last).abs());
        last = camera.pose().yaw;
    }
    assert_eq!(camera.pose().yaw, HOME.yaw);
    assert_eq!(camera.pose().zoom, zoom, "the zoom is kept");
    let fastest = steps.iter().copied().fold(0.0, f32::max);
    assert!(steps[0] < fastest / 4.0, "starts slowly: {steps:?}");
    assert!(camera.resting_pose().is_some());
}

#[test]
fn turning_by_itself_waits_five_seconds_then_gathers_speed_very_slowly() {
    let mut camera = camera();
    assert!(camera.auto_rotate, "on unless turned off");
    let mut time = 0.0;
    while time < 4.9 {
        assert!(!camera.update(FRAME), "still at {time}");
        time += FRAME;
    }
    assert!(!camera.turning_by_itself());
    let mut yaws = Vec::new();
    for _ in 0..(6.0 / FRAME) as usize {
        camera.update(FRAME);
        yaws.push(camera.pose().yaw);
    }
    assert!(camera.turning_by_itself());
    let step = |i: usize| wrap_angle(yaws[i + 1] - yaws[i]);
    assert!(step(10) < step(yaws.len() - 2), "gathers speed");
    let speed = step(yaws.len() - 2) / FRAME;
    assert!(speed > 0.0 && speed < 0.2, "very slowly: {speed} rad/s");
}

#[test]
fn angles_wrap_into_one_turn() {
    assert!((wrap_angle(3.0 * PI) - PI).abs() < 1e-5);
    assert!((wrap_angle(-PI / 2.0) + PI / 2.0).abs() < 1e-6);
    assert!((wrap_angle(TAU + 0.5) - 0.5).abs() < 1e-5);
}

#[test]
fn a_still_stage_is_woken_to_turn_by_itself_after_one_long_gap() {
    // Nothing moves, so no frames come between: one update five seconds later must count.
    let mut camera = camera();
    camera.update(FRAME);
    let wait = camera.turns_by_itself_in().expect("waiting to turn");
    assert!(wait > 4.9 && wait <= AUTO_DELAY, "{wait}");
    camera.update(wait + 0.02);
    assert!(camera.turns_by_itself_in().is_none());
    assert!(camera.turning_by_itself());
    let yaw = camera.pose().yaw;
    for _ in 0..30 {
        camera.update(FRAME);
    }
    assert_ne!(camera.pose().yaw, yaw, "it turns");
    camera.auto_rotate = false;
    assert!(camera.turns_by_itself_in().is_none(), "off: never woken");
}

#[test]
fn turning_it_by_hand_pauses_turning_by_itself_which_then_goes_on_from_there() {
    let mut camera = camera();
    camera.update(AUTO_DELAY + AUTO_RAMP);
    camera.update(FRAME);
    assert!(camera.turning_by_itself());
    // Grabbed and turned by hand: it does not fight the hand.
    camera.hold();
    camera.drag(Vec2::new(-60.0, 0.0), FRAME);
    assert!(!camera.turning_by_itself());
    let mut time = 0.0;
    while time < 1.0 {
        camera.update(FRAME);
        time += FRAME;
    }
    let settled = camera.pose().yaw;
    assert!(camera.turns_by_itself_in().is_some(), "waits again");
    // Five seconds later it goes on from where the hand (and its flick) left it, not from home.
    camera.update(AUTO_DELAY);
    for _ in 0..120 {
        camera.update(FRAME);
    }
    assert!(camera.turning_by_itself());
    let now = camera.pose().yaw;
    assert_ne!(now, settled, "turning again");
    assert!(
        wrap_angle(now - settled).abs() < 0.5,
        "from there: {settled} → {now}"
    );
}

#[test]
fn let_go_while_it_turned_by_itself_it_goes_on_at_once_and_zooming_does_not_stop_it() {
    let mut camera = camera();
    camera.update(AUTO_DELAY + AUTO_RAMP);
    camera.update(FRAME);
    assert!(camera.turning_by_itself());
    // Zooming does not stop it.
    camera.zoom_by(1.2);
    let yaw = camera.pose().yaw;
    camera.update(FRAME);
    assert!(camera.turning_by_itself());
    assert_ne!(camera.pose().yaw, yaw, "still turning while zooming");
    // Grabbed: it stops while held, however long.
    camera.hold();
    for _ in 0..(8.0 / FRAME) as usize {
        camera.drag(Vec2::ZERO, FRAME);
        camera.update(FRAME);
    }
    assert!(!camera.turning_by_itself(), "held still");
    // Let go: it goes on the very next frame, at its usual speed, no stop and no new ramp.
    camera.release();
    let yaw = camera.pose().yaw;
    camera.update(FRAME);
    assert!(camera.turning_by_itself(), "goes on at once");
    let step = wrap_angle(camera.pose().yaw - yaw) / FRAME;
    assert!(
        (step - AUTO_SPEED).abs() < 1e-3,
        "at full speed: {step} rad/s"
    );
    // Let go with a flick the other way: it slows down into turning that way, never stopping or
    // turning back in between.
    camera.hold();
    camera.drag(Vec2::new(40.0, 0.0), FRAME);
    camera.release();
    let mut last = camera.pose().yaw;
    for _ in 0..120 {
        camera.update(FRAME);
        let yaw = camera.pose().yaw;
        assert!(wrap_angle(yaw - last) < -1e-4, "never stops or turns back");
        last = yaw;
    }
    assert!(camera.turning_by_itself());
    // Grabbed while it only waited: let go, it waits the usual time.
    let mut waiting = OrbitCamera::default();
    waiting.update(FRAME);
    waiting.hold();
    waiting.release();
    waiting.update(1.0);
    assert!(!waiting.turning_by_itself());
}
