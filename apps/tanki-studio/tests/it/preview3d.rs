// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The Garage's 3D preview with a real GPU: what it draws, its toolbar, its quick settings, what
//! happens when a look cannot be built, every window width, and a monkey over all of it. On a
//! machine without a GPU adapter these skip with a note.

use std::path::PathBuf;

use egui::{Pos2, Rect, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use tanki_studio::{
    app::{Dialog, Page, Selection, Studio},
    preview3d::Preview3d,
};
use tanki_testkit::{PackOptions, build_pack_dir, give_models};

use crate::common::{Env, env, gpu_harness};

const STAGE: &str = "3D view of the item";

/// A cache whose hull and turret have drawable models, and a pack whose skins have them too.
fn models(label: &str) -> (Env, PathBuf) {
    let env = env(label);
    give_models(&env.cache).expect("models in the cache");
    let pack = build_pack_dir(
        &env.work.join("models-pack"),
        &PackOptions {
            id: "models",
            real_models: true,
            ..PackOptions::default()
        },
    )
    .expect("pack with models");
    (env, pack)
}

fn select(h: &mut Harness<'static, Studio>, kind: &str, id: &str, grade: usize) {
    let app = h.state_mut();
    app.selection = Some(Selection {
        kind: kind.into(),
        id: id.into(),
        grade,
    });
    app.go(Page::Garage);
    h.run_steps(4);
}

fn stage_rect(h: &Harness<'static, Studio>) -> Rect {
    h.get_by_label(STAGE).rect()
}

/// The colour of the rendered window at `pos` (one pixel per point in tests).
fn colour(image: &image::RgbaImage, pos: Pos2) -> [u8; 4] {
    image
        .get_pixel_checked(pos.x as u32, pos.y as u32)
        .map_or([0; 4], |p| p.0)
}

fn distance(a: [u8; 4], b: [u8; 4]) -> u32 {
    (0..3).map(|i| u32::from(a[i].abs_diff(b[i]))).sum()
}

/// The mean colour of a column of the stage, `share` across its width.
fn column(image: &image::RgbaImage, rect: Rect, share: f32) -> [u32; 3] {
    let x = rect.min.x + rect.width() * share;
    let mut sum = [0u32; 3];
    let mut count = 0;
    let mut y = rect.min.y;
    while y < rect.max.y {
        let p = colour(image, Pos2::new(x, y));
        for i in 0..3 {
            sum[i] += u32::from(p[i]);
        }
        count += 1;
        y += 2.0;
    }
    sum.map(|s| s / count.max(1))
}

/// How many pixels of the stage the 3D model changes: the window rendered with 3D, then with 3D
/// off (the same page, the flat picture), compared inside the stage.
fn drawn_pixels(h: &mut Harness<'static, Studio>, rect: Rect) -> usize {
    let with = h.render().expect("render");
    h.state_mut().preview.enabled = false;
    h.run_steps(2);
    let without = h.render().expect("render");
    h.state_mut().preview.enabled = true;
    h.run_steps(2);
    let mut changed = 0;
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            let at = Pos2::new(x, y);
            if distance(colour(&with, at), colour(&without, at)) > 12 {
                changed += 1;
            }
            x += 2.0;
        }
        y += 2.0;
    }
    changed
}

fn wheel(h: &Harness<'static, Studio>, delta: f32) {
    h.event(egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: vec2(0.0, delta),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: egui::Modifiers::NONE,
    });
}

#[test]
fn the_garage_draws_the_item_in_3d_where_the_flat_picture_was() {
    let (env, _) = models("3d-draws");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    assert!(h.state().preview.active(), "on by default, with a GPU");
    assert!(
        h.state().preview.stage.key(Preview3d::MAIN).is_some(),
        "the cache's model is on the stage"
    );
    let rect = stage_rect(&h);
    if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
        let image = h.render().expect("render");
        let _ = image.save(PathBuf::from(dir).join("garage-3d.png"));
    }
    // Sampled every other pixel: a model on a tenth of the stage at least.
    let drawn = drawn_pixels(&mut h, rect);
    let room = (rect.width() * rect.height() / 4.0) as usize;
    assert!(drawn > room / 10, "the model is drawn: {drawn} of {room}");
    for label in [
        "Compare with the game",
        "Turn by itself",
        "Reset the view",
        "Save a picture",
        "3D settings",
    ] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    let details = h.get_by_label("Details").rect();
    let compare = h.get_by_label("Compare with the game").rect();
    assert!(details.max.x <= compare.min.x, "{details:?} {compare:?}");
}

#[test]
fn dragging_turns_the_model_and_the_home_key_brings_it_back() {
    let (env, _) = models("3d-drag");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let rect = stage_rect(&h);
    let home = h.state().preview.stage.camera().pose();
    let before = h.render().expect("render");
    h.drag_at(rect.center());
    h.run_steps(1);
    for step in 1..=6 {
        h.hover_at(rect.center() + vec2(step as f32 * 20.0, 0.0));
        h.run_steps(1);
    }
    h.drop_at(rect.center() + vec2(120.0, 0.0));
    h.run_steps(30);
    let turned = h.state().preview.stage.camera().pose();
    assert_ne!(turned.yaw, home.yaw, "a drag turns the camera");
    assert!(
        h.state().preview.used,
        "the drag hint gives way to the caption"
    );
    let after = h.render().expect("render");
    assert_ne!(before.as_raw(), after.as_raw(), "the picture follows");
    // Focused by the drag, the stage takes the keyboard: Home brings the first view back.
    h.key_press(egui::Key::Home);
    h.run_steps(90);
    let back = h.state().preview.stage.camera().pose();
    assert!(
        (back.yaw - home.yaw).abs() < 1e-3 && (back.pitch - home.pitch).abs() < 1e-3,
        "{back:?} against {home:?}"
    );
}

#[test]
fn the_wheel_zooms_the_stage_and_never_scrolls_the_page_behind_it() {
    let (env, _) = models("3d-wheel");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let rect = stage_rect(&h);
    h.hover_at(rect.center());
    h.run_steps(1);
    let zoom = h.state().preview.stage.camera().pose().zoom;
    wheel(&h, 120.0);
    h.run_steps(60);
    let zoomed = h.state().preview.stage.camera().pose().zoom;
    assert!(zoomed < zoom, "scrolling up zooms in: {zoom} → {zoomed}");
    assert_eq!(stage_rect(&h), rect, "the page did not scroll");
}

#[test]
fn a_picked_skin_compares_with_the_game_and_the_toolbar_does_what_it_says() {
    let (env, pack) = models("3d-toolbar");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    h.state_mut().import_path(&pack);
    h.run_steps(4);
    select(&mut h, "hull", "wasp", 3);
    h.get_by_label("Wasp Red").click();
    h.run_steps(4);
    assert!(
        h.state().preview.stage.key(Preview3d::MAIN).is_some()
            && h.state().preview.stage.key(Preview3d::COMPARED).is_some(),
        "the skin and the game's look are both on the stage"
    );

    h.get_by_label("Compare with the game").click();
    h.run_steps(4);
    assert_eq!(h.state().preview.compare, Some(0.5));
    let rect = stage_rect(&h);
    let image = h.render().expect("render");
    // The pack's skin is red; the game's look is not: the two halves differ.
    assert_ne!(
        column(&image, rect, 0.42),
        column(&image, rect, 0.58),
        "the halves show two looks"
    );

    // The divider follows a drag.
    let divider = h.get_by_label("Before and after divider").rect();
    h.drag_at(divider.center());
    h.run_steps(1);
    h.hover_at(divider.center() + vec2(-80.0, 0.0));
    h.run_steps(1);
    h.drop_at(divider.center() + vec2(-80.0, 0.0));
    h.run_steps(2);
    let split = h.state().preview.compare.unwrap_or(0.5);
    assert!(split < 0.45, "moved left: {split}");

    h.get_by_label("Turn by itself").click();
    h.run_steps(2);
    assert!(h.state().preview.auto_rotating());
    h.get_by_label("Turn by itself").click();
    h.run_steps(2);
    assert!(!h.state().preview.auto_rotating());

    h.get_by_label("Save a picture").click();
    h.run_steps(2);
    let picture = image::open(env.work.join("data/tank.png"))
        .expect("a PNG is saved")
        .to_rgba8();
    assert!(
        picture.pixels().any(|p| p.0[3] == 255),
        "the model is in it"
    );
    assert!(
        picture.pixels().any(|p| p.0[3] == 0),
        "on a clear background"
    );

    h.get_by_label("Compare with the game").click();
    h.run_steps(2);
    assert_eq!(h.state().preview.compare, None);
}

#[test]
fn without_a_picked_skin_there_is_nothing_to_compare() {
    let (env, _) = models("3d-no-compare");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let compare = h.get_by_label("Compare with the game");
    assert!(compare.accesskit_node().is_disabled());
    compare.click();
    h.run_steps(2);
    assert_eq!(h.state().preview.compare, None);
}

#[test]
fn quick_settings_turn_3d_off_and_on_and_lead_to_settings() {
    let (env, _) = models("3d-quick");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    h.get_by_label("3D settings").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(Dialog::Preview3d)));
    h.get_by_label("Show the tank in 3D").click();
    h.run_steps(3);
    assert!(!h.state().preview.enabled);
    h.get_by_label("Done").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    // Off: the flat picture, and only the settings button to turn it back on.
    assert!(h.query_by_label(STAGE).is_none());
    assert!(h.query_by_label("Compare with the game").is_none());
    h.get_by_label("3D settings").click();
    h.run_steps(3);
    h.get_by_label("Show the tank in 3D").click();
    h.run_steps(3);
    assert!(h.state().preview.enabled);
    h.get_by_label("All settings").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().page, Page::Settings);
    assert!(
        h.query_by_label("Economy").is_some(),
        "the quality is there"
    );
}

#[test]
fn escape_and_the_close_button_close_the_quick_settings() {
    let (env, _) = models("3d-escape");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    for close in [None, Some("Close")] {
        h.get_by_label("3D settings").click();
        h.run_steps(3);
        assert!(h.state().dialog.is_some());
        match close {
            None => h.key_press(egui::Key::Escape),
            Some(label) => h.get_by_label(label).click(),
        }
        h.run_steps(3);
        assert!(h.state().dialog.is_none(), "{close:?}");
        assert!(h.state().preview.enabled, "closing changes nothing");
    }
}

#[test]
fn a_model_that_cannot_be_read_shows_the_flat_picture_and_is_not_read_again() {
    // The default fixture's models are placeholder bytes: not drawable.
    let env = env("3d-broken");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    assert!(h.state().preview.active());
    assert!(
        h.query_by_label(STAGE).is_none(),
        "the flat picture stands in"
    );
    assert!(
        h.query_by_label("3D settings").is_some(),
        "3D can still be turned off"
    );
    for _ in 0..20 {
        h.run_steps(1);
        assert!(!h.state_mut().preview.poll(), "nothing is being read again");
    }
}

#[test]
fn every_modification_and_the_turret_get_their_own_scene() {
    let (env, _) = models("3d-items");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let hull = h
        .state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned);
    select(&mut h, "turret", "railgun", 3);
    let turret = h
        .state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned);
    assert!(hull.is_some() && turret.is_some());
    assert_ne!(hull, turret);
    // A modification the cache does not have: no model, the flat picture's note.
    select(&mut h, "hull", "wasp", 0);
    assert!(h.query_by_label(STAGE).is_none());
}

#[test]
fn the_toolbar_details_and_stage_never_overlap_at_any_window_size() {
    let (env, _) = models("3d-sizes");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    for (width, height) in [
        (820.0, 520.0),
        (960.0, 700.0),
        (1100.0, 800.0),
        (1440.0, 900.0),
        (1920.0, 1080.0),
    ] {
        h.set_size(vec2(width, height));
        h.run_steps(4);
        let stage = stage_rect(&h);
        let settings = h.get_by_label("3D settings").rect();
        let details = h.get_by_label("Details").rect();
        let compare = h.get_by_label("Compare with the game").rect();
        assert!(
            stage.width() > 100.0 && stage.height() > 100.0,
            "{width}: room for the model: {stage:?}"
        );
        assert!(
            settings.max.x <= width && compare.min.x >= details.max.x,
            "{width}: {details:?} {compare:?} {settings:?}"
        );
        assert!(
            !stage.intersects(settings),
            "{width}: the toolbar sits above the model's room"
        );
    }
}

#[test]
fn quality_choices_redraw_the_stage_and_survive_a_restart() {
    let (env, _) = models("3d-quality");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    for (label, quality) in [
        ("Economy", tanki_viewer::QualityPreference::Economy),
        ("High", tanki_viewer::QualityPreference::High),
        ("Balanced", tanki_viewer::QualityPreference::Balanced),
    ] {
        h.get_by_label("3D settings").click();
        h.run_steps(3);
        h.get_by_label(label).click();
        h.run_steps(3);
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        assert_eq!(h.state().preview.quality, quality);
        let rect = stage_rect(&h);
        let drawn = drawn_pixels(&mut h, rect);
        assert!(drawn > 500, "{label}: still drawn ({drawn})");
    }
    drop(h);
    let again = gpu_harness(&env).expect("the GPU is still there");
    assert_eq!(
        again.state().preview.quality,
        tanki_viewer::QualityPreference::Balanced
    );
}

/// A paint picture in the cache at `/0/0/30/2/`, labelled by hand as the paint "ember".
fn add_paint(env: &Env) {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let url = "http://cache.test/0/0/30/2/1/image.jpg";
    let file = env.cache.root.join(STANDARD.encode(url));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).expect("cache folder");
    }
    std::fs::write(&file, tanki_testkit::jpeg(64, 64, [230, 120, 20])).expect("paint picture");
    let store = tanki_core::state::StateStore::open(&env.work.join("data/state.sqlite"))
        .expect("state store");
    store
        .save_user_target("air-cache", "/0/0/30/2/", "paint", "ember", "m0")
        .expect("label the paint");
}

#[test]
fn a_paint_is_shown_on_the_hull_last_looked_at() {
    let (env, _) = models("3d-paint");
    add_paint(&env);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let hull = h
        .state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned);
    select(&mut h, "paint", "ember", 0);
    let painted = h
        .state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned)
        .unwrap_or_default();
    assert!(
        painted.starts_with("painted:"),
        "the paint is on a hull: {painted}"
    );
    assert!(
        hull.is_some_and(|hull| painted.contains(&hull)),
        "on the hull last shown"
    );
    let rect = stage_rect(&h);
    if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
        let image = h.render().expect("render");
        let _ = image.save(PathBuf::from(dir).join("garage-paint.png"));
    }
    assert!(drawn_pixels(&mut h, rect) > 500, "drawn");
    // A paint has no modifications: no selector under it.
    assert!(h.query_by_label("M3").is_none());
}

#[test]
fn a_paint_without_any_hull_model_shows_its_flat_picture() {
    // Placeholder models: no hull to put the paint on.
    let env = env("3d-paint-flat");
    add_paint(&env);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "paint", "ember", 0);
    assert!(h.query_by_label(STAGE).is_none());
    assert!(h.query_by_label("3D settings").is_some());
}

/// A fixed pseudo-random walk, so a failure is reproducible.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        ((self.0 >> 33) as usize) % below.max(1)
    }
}

#[test]
fn a_monkey_on_the_3d_preview_breaks_nothing() {
    let (env, pack) = models("3d-monkey");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    h.state_mut().import_path(&pack);
    h.run_steps(4);
    select(&mut h, "hull", "wasp", 3);
    let mut rng = Lcg(0x005E_ED3D);
    let labels = [
        "Compare with the game",
        "Turn by itself",
        "Reset the view",
        "Save a picture",
        "3D settings",
        "Show the tank in 3D",
        "Auto",
        "Economy",
        "Balanced",
        "High",
        "Done",
        "All settings",
        "Wasp Red",
        "Before and after divider",
        "Garage",
    ];
    let sizes = [
        (820.0, 520.0),
        (1100.0, 760.0),
        (1440.0, 900.0),
        (1920.0, 1080.0),
    ];
    for step in 0..300 {
        match rng.next(9) {
            0..=3 => {
                let label = labels[rng.next(labels.len())];
                if let Some(node) = h.query_by_label(label) {
                    node.click();
                }
            }
            4 => {
                if let Some(stage) = h.query_by_label(STAGE).map(|n| n.rect()) {
                    let from = stage.center();
                    let to =
                        from + vec2(rng.next(400) as f32 - 200.0, rng.next(200) as f32 - 100.0);
                    h.drag_at(from);
                    h.run_steps(1);
                    h.hover_at(to);
                    h.run_steps(1);
                    h.drop_at(to);
                }
            }
            5 => {
                if let Some(stage) = h.query_by_label(STAGE).map(|n| n.rect()) {
                    h.hover_at(stage.center());
                    wheel(&h, rng.next(600) as f32 - 300.0);
                }
            }
            6 => {
                let (kind, id) = [("hull", "wasp"), ("turret", "railgun")][rng.next(2)];
                select(&mut h, kind, id, rng.next(4));
            }
            7 => {
                let (w, height) = sizes[rng.next(sizes.len())];
                h.set_size(vec2(w, height));
            }
            _ => {
                let key = [
                    egui::Key::Escape,
                    egui::Key::ArrowLeft,
                    egui::Key::ArrowUp,
                    egui::Key::Home,
                ][rng.next(4)];
                h.key_press(key);
            }
        }
        h.run_steps(2);
        let app = h.state();
        assert!(
            matches!(app.dialog, None | Some(Dialog::Preview3d)),
            "step {step}: an unexpected dialog: {:?}",
            app.dialog
        );
        if let Some(split) = app.preview.compare {
            assert!((0.0..=1.0).contains(&split), "step {step}: split {split}");
        }
        let pose = app.preview.stage.camera().pose();
        assert!(
            pose.yaw.is_finite() && pose.pitch.is_finite() && pose.zoom.is_finite(),
            "step {step}: {pose:?}"
        );
        if step % 50 == 0 {
            h.render().expect("every state renders");
        }
    }
}
