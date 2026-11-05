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
    // Back home (under a second) well before it starts turning by itself (five).
    h.run_steps(12);
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

    assert!(h.state().preview.auto_rotating(), "on by default");
    h.get_by_label("Turn by itself").click();
    h.run_steps(2);
    assert!(!h.state().preview.auto_rotating());
    h.get_by_label("Turn by itself").click();
    h.run_steps(2);
    assert!(h.state().preview.auto_rotating());

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
fn quick_settings_turn_3d_off_and_on_and_have_no_buttons_but_close() {
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
    // The settings apply as they change: no Done, no way elsewhere, only the ×.
    assert!(h.query_by_label("Done").is_none());
    assert!(h.query_by_label("All settings").is_none());
    h.get_by_label("Close").click();
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
    assert!(
        h.query_by_label("Economy").is_some(),
        "the quality is there"
    );
    h.key_press(egui::Key::Escape);
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert!(h.query_by_label(STAGE).is_some(), "3D is back");
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

/// A paint picture in the cache at `/0/0/30/<n>/`, labelled by hand as the paint `id`.
fn add_paint(env: &Env, id: &str, n: u32, rgb: [u8; 3]) {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let url = format!("http://cache.test/0/0/30/{n}/1/image.jpg");
    let file = env.cache.root.join(STANDARD.encode(url));
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).expect("cache folder");
    }
    std::fs::write(&file, tanki_testkit::jpeg(64, 64, rgb)).expect("paint picture");
    let store = tanki_core::state::StateStore::open(&env.work.join("data/state.sqlite"))
        .expect("state store");
    store
        .save_user_target("air-cache", &format!("/0/0/30/{n}/"), "paint", id, "m0")
        .expect("label the paint");
}

#[test]
fn a_paint_is_shown_on_the_hull_last_looked_at() {
    let (env, _) = models("3d-paint");
    add_paint(&env, "ember", 2, [230, 120, 20]);
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
    add_paint(&env, "ember", 2, [230, 120, 20]);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "paint", "ember", 0);
    assert!(h.query_by_label(STAGE).is_none());
    assert!(h.query_by_label("3D settings").is_some());
}

#[test]
fn hulls_and_turrets_wear_the_green_of_the_cache_until_a_paint_is_chosen() {
    let (env, _) = models("3d-green");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let plain = h
        .state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned)
        .unwrap_or_default();
    assert!(
        plain.starts_with("cache:"),
        "no green paint in this cache: {plain}"
    );
    drop(h);
    add_paint(&env, "green", 4, [70, 110, 40]);
    let mut h = gpu_harness(&env).expect("the GPU is still there");
    for (kind, id) in [("hull", "wasp"), ("turret", "railgun")] {
        select(&mut h, kind, id, 3);
        let key = h
            .state()
            .preview
            .stage
            .key(Preview3d::MAIN)
            .map(str::to_owned)
            .unwrap_or_default();
        let green = {
            use base64::{Engine, engine::general_purpose::STANDARD};
            STANDARD.encode("http://cache.test/0/0/30/4/1/image.jpg")
        };
        assert!(
            key.starts_with("painted:") && key.contains(&green),
            "{kind} wears the green: {key}"
        );
    }
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

/// Put the starter tank in the test cache: copies of its hull's and turret's files under groups
/// of their own, labelled as the starter parts (M3).
fn starter_tank(env: &Env) {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let store = tanki_core::state::StateStore::open(&env.work.join("data/state.sqlite"))
        .expect("state store");
    for (files, from, to, kind, id) in [
        (
            &env.cache.hull_files,
            "0/0/10/14/",
            "0/0/31/1/",
            "hull",
            "hunter",
        ),
        (
            &env.cache.turret_files,
            "0/0/10/64/",
            "0/0/31/2/",
            "turret",
            "smoky",
        ),
    ] {
        for file in files {
            let url = file
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| STANDARD.decode(name).ok())
                .map(|bytes| String::from_utf8_lossy(&bytes).replace(from, to))
                .expect("a cache file's name is its URL");
            let bytes = std::fs::read(file).expect("cache file");
            std::fs::write(env.cache.root.join(STANDARD.encode(url)), bytes).expect("copy");
        }
        store
            .save_user_target("air-cache", &format!("/{to}"), kind, id, "m3")
            .expect("label the part");
    }
}

fn main_key(h: &Harness<'static, Studio>) -> String {
    h.state()
        .preview
        .stage
        .key(Preview3d::MAIN)
        .map(str::to_owned)
        .unwrap_or_default()
}

#[test]
fn a_new_user_sees_the_starter_tank_whole_in_the_cache_s_green() {
    let (env, _) = models("3d-starter");
    starter_tank(&env);
    add_paint(&env, "green", 4, [70, 110, 40]);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    h.state_mut().go(Page::Garage);
    h.run_steps(4);
    let selection = h.state().selection.clone().expect("a selection");
    assert_eq!(
        (
            selection.kind.as_str(),
            selection.id.as_str(),
            selection.grade
        ),
        ("hull", "hunter", 3),
        "the starter hull is selected first"
    );
    let whole = main_key(&h);
    assert_eq!(whole.matches(" & ").count(), 1, "hull and turret: {whole}");
    assert_eq!(
        whole.matches("painted:").count(),
        2,
        "both painted: {whole}"
    );
    let rect = stage_rect(&h);
    if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
        let image = h.render().expect("render");
        let _ = image.save(PathBuf::from(dir).join("garage-tank.png"));
    }
    assert!(drawn_pixels(&mut h, rect) > 500, "drawn");
    let whole_picture = h.render().expect("render");
    h.state_mut().set_whole_tank(false);
    h.run_steps(4);
    let alone = main_key(&h);
    assert!(!alone.contains(" & "), "off: the hull alone: {alone}");
    let hull_picture = h.render().expect("render");
    // The turret (and the framing it changes) makes a picture of its own.
    let mut differ = 0;
    let mut y = rect.min.y;
    while y < rect.max.y {
        let mut x = rect.min.x;
        while x < rect.max.x {
            let at = Pos2::new(x, y);
            if distance(colour(&whole_picture, at), colour(&hull_picture, at)) > 12 {
                differ += 1;
            }
            x += 2.0;
        }
        y += 2.0;
    }
    assert!(differ > 500, "the turret is drawn on the hull: {differ}");
}

#[test]
fn the_tank_keeps_the_parts_chosen_last_across_a_restart_and_the_switch_shows_one_alone() {
    let (env, _) = models("3d-tank");
    starter_tank(&env);
    add_paint(&env, "green", 4, [70, 110, 40]);
    add_paint(&env, "ember", 2, [230, 120, 20]);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "hunter", 3);
    let green = main_key(&h);
    select(&mut h, "paint", "ember", 0);
    let ember = main_key(&h);
    assert_eq!(ember.matches("painted:").count(), 2, "{ember}");
    assert_ne!(ember, green, "the paint chosen is on both parts");
    // The view stays as it is while the tank's parts change.
    select(&mut h, "turret", "smoky", 3);
    assert_eq!(main_key(&h), ember, "the same tank, the same scene");
    drop(h);
    let mut h = gpu_harness(&env).expect("the GPU is still there");
    let tank = h.state().preview.tank.clone();
    assert_eq!(tank.hull, ("hunter".to_owned(), 3));
    assert_eq!(tank.turret, ("smoky".to_owned(), 3));
    assert_eq!(tank.paint, "ember", "kept across a restart");
    select(&mut h, "hull", "hunter", 3);
    assert_eq!(main_key(&h), ember, "in the paint chosen last");
    // The quick settings turn it off: the item alone, and it stays off.
    h.get_by_label("3D settings").click();
    h.run_steps(3);
    h.get_by_label("Show the whole tank").click();
    h.run_steps(4);
    assert!(!h.state().preview.whole_tank);
    assert!(!main_key(&h).contains(" & "), "the hull alone");
    drop(h);
    let mut h = gpu_harness(&env).expect("the GPU is still there");
    assert!(!h.state().preview.whole_tank, "off for good");
    select(&mut h, "hull", "hunter", 3);
    h.state_mut().set_whole_tank(true);
    h.run_steps(4);
    assert_eq!(main_key(&h), ember, "on again: the whole tank");
}

#[test]
fn without_the_tank_s_other_part_in_the_cache_the_item_is_shown_alone() {
    // The cache has no starter turret: nothing is put together.
    let (env, _) = models("3d-alone");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let key = main_key(&h);
    assert!(!key.is_empty() && !key.contains(" & "), "alone: {key}");
    assert_eq!(h.state().preview.tank.hull, ("wasp".to_owned(), 3));
    select(&mut h, "turret", "railgun", 3);
    assert!(
        main_key(&h).contains(" & "),
        "with a turret chosen, the tank is whole"
    );
}

#[test]
fn the_zoom_turn_and_auto_turn_come_back_after_a_restart() {
    let (env, _) = models("3d-view");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let rect = stage_rect(&h);
    h.hover_at(rect.center());
    h.run_steps(1);
    wheel(&h, 120.0);
    h.drag_at(rect.center());
    h.hover_at(rect.center() + vec2(80.0, -30.0));
    h.drop_at(rect.center() + vec2(80.0, -30.0));
    // Turned off, so the angle stays where it was left.
    h.state_mut().preview.toggle_auto_rotate();
    h.run_steps(60);
    let left = h.state().preview.stage.camera().pose();
    assert!(left.zoom < 1.0, "zoomed in: {left:?}");
    h.state_mut().persist(true);
    drop(h);

    let mut h = gpu_harness(&env).expect("the GPU is still there");
    h.state_mut().go(Page::Garage);
    h.run_steps(2);
    let back = h.state().preview.stage.camera().pose();
    assert!(!h.state().preview.auto_rotating(), "still off");
    assert!((back.zoom - left.zoom).abs() < 1e-3, "{left:?} → {back:?}");
    assert!(
        (back.pitch - left.pitch).abs() < 1e-3,
        "{left:?} → {back:?}"
    );
    assert!((back.yaw - left.yaw).abs() < 1e-3, "{left:?} → {back:?}");
    // Another item keeps the view: the same angle and zoom.
    select(&mut h, "turret", "railgun", 3);
    h.run_steps(10);
    let other = h.state().preview.stage.camera().pose();
    assert!((other.zoom - left.zoom).abs() < 1e-3, "{other:?}");
    assert!((other.yaw - left.yaw).abs() < 1e-3, "{other:?}");
}

#[test]
fn switching_the_hull_turret_or_paint_keeps_the_tank_turning_from_where_it_was() {
    let (env, _) = models("3d-keep-turning");
    starter_tank(&env);
    add_paint(&env, "green", 4, [70, 110, 40]);
    add_paint(&env, "ember", 2, [230, 120, 20]);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "hunter", 3);
    assert!(h.state().preview.auto_rotating(), "on by default");
    let turning =
        |h: &Harness<'static, Studio>| h.state().preview.stage.camera().turning_by_itself();
    for _ in 0..400 {
        if turning(&h) {
            break;
        }
        h.run_steps(1);
    }
    assert!(turning(&h), "turning by itself after the wait");
    h.run_steps(10);
    for (kind, id, grade) in [
        ("paint", "ember", 0),
        ("hull", "wasp", 3),
        ("turret", "smoky", 3),
    ] {
        let before = h.state().preview.stage.camera().pose();
        select(&mut h, kind, id, grade);
        let after = h.state().preview.stage.camera().pose();
        assert!(turning(&h), "{kind}: it goes on turning, no new wait");
        let moved = (after.yaw - before.yaw).rem_euclid(std::f32::consts::TAU);
        assert!(
            moved > 0.0 && moved < 0.5,
            "{kind}: on from where it was, not back home: {before:?} → {after:?}"
        );
        assert_eq!(after.zoom, before.zoom);
    }
}

#[test]
fn while_the_next_model_loads_the_one_before_stays_under_a_loading_pill() {
    let (env, _) = models("3d-loading");
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "wasp", 3);
    let before = main_key(&h);
    let rect = stage_rect(&h);
    let drawn = h.render().expect("render");
    // Built on their own thread, as in the app.
    h.state_mut().preview.background = true;
    h.state_mut().selection = Some(Selection {
        kind: "turret".into(),
        id: "railgun".into(),
        grade: 3,
    });
    h.run_steps(1);
    let mut saw_loading = false;
    for _ in 0..400 {
        if main_key(&h) != before {
            break;
        }
        if h.state().preview.loading() {
            saw_loading = true;
            // The stage is still there, the model before on it (faded), never a blank frame.
            assert!(h.query_by_label(STAGE).is_some(), "the stage stays");
            let faded = h.render().expect("render");
            if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
                let _ = faded.save(PathBuf::from(dir).join("garage-loading.png"));
            }
            let mut kept = 0;
            let mut y = rect.min.y;
            while y < rect.max.y {
                let mut x = rect.min.x;
                while x < rect.max.x {
                    let at = Pos2::new(x, y);
                    if distance(colour(&faded, at), colour(&drawn, at)) < 200
                        && colour(&drawn, at) != colour(&faded, at)
                    {
                        kept += 1;
                    }
                    x += 4.0;
                }
                y += 4.0;
            }
            assert!(kept > 50, "the model before is still drawn, faded: {kept}");
        }
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(saw_loading, "it loaded on its own thread");
    assert_ne!(main_key(&h), before, "the next model came");
    assert!(!h.state().preview.loading());
}

fn open_details(h: &mut Harness<'static, Studio>) {
    h.get_by_label("Details").click();
    h.run_steps(4);
    assert_eq!(h.state().page, Page::Info);
}

#[test]
fn details_of_a_whole_tank_show_it_in_3d_with_its_parts_and_a_part_picked_below() {
    let (env, _) = models("3d-details-tank");
    starter_tank(&env);
    add_paint(&env, "green", 4, [70, 110, 40]);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    select(&mut h, "hull", "hunter", 3);
    let garage_key = main_key(&h);
    let pose = h.state().preview.stage.camera().pose();
    open_details(&mut h);
    // The same stage, the same tank on it, the same view going on.
    assert!(h.query_by_label(STAGE).is_some(), "the tank in 3D");
    assert_eq!(main_key(&h), garage_key, "the whole tank, as in the Garage");
    let now = h.state().preview.stage.camera().pose();
    assert_eq!(now.zoom, pose.zoom);
    assert!((now.pitch - pose.pitch).abs() < 1e-3);
    for part in ["Hull: Hunter", "Turret: Smoky", "Paint: Green"] {
        assert!(h.query_by_label(part).is_some(), "{part}");
    }
    if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
        let image = h.render().expect("render");
        let _ = image.save(PathBuf::from(dir).join("details-tank.png"));
    }
    // Picking the turret shows it in the panels below; the tank stays the same.
    h.get_by_label("Turret: Smoky").click();
    h.run_steps(4);
    let selection = h.state().selection.clone().expect("a selection");
    assert_eq!(
        (
            selection.kind.as_str(),
            selection.id.as_str(),
            selection.grade
        ),
        ("turret", "smoky", 3)
    );
    assert!(
        h.query_by_label("Smoky M0").is_some(),
        "its modifications below"
    );
    assert_eq!(main_key(&h), garage_key, "the same tank");
    // The panels and the model never overlap, wide or narrow.
    for (width, height) in [
        (820.0, 520.0),
        (1100.0, 800.0),
        (1440.0, 900.0),
        (1920.0, 1080.0),
    ] {
        h.set_size(vec2(width, height));
        h.run_steps(4);
        let stage = stage_rect(&h);
        let hull = h.get_by_label("Hull: Hunter").rect();
        let turret = h.get_by_label("Turret: Smoky").rect();
        assert!(
            stage.width() > 100.0 && stage.height() > 100.0,
            "{width}: {stage:?}"
        );
        assert!(
            !stage.intersects(hull) && !stage.intersects(turret) && !hull.intersects(turret),
            "{width}: {stage:?} {hull:?} {turret:?}"
        );
    }
}

#[test]
fn details_of_an_item_alone_show_it_in_3d_and_with_3d_off_its_picture() {
    let (env, _) = models("3d-details-item");
    starter_tank(&env);
    let Some(mut h) = gpu_harness(&env) else {
        return;
    };
    h.state_mut().set_whole_tank(false);
    select(&mut h, "turret", "smoky", 3);
    open_details(&mut h);
    assert!(h.query_by_label(STAGE).is_some(), "the turret in 3D");
    assert!(!main_key(&h).contains(" & "), "alone");
    assert!(h.query_by_label("Hull: Hunter").is_none(), "no tank parts");
    assert!(
        h.query_by_label("Smoky").is_some(),
        "its name beside the model"
    );
    if let Some(dir) = std::env::var_os("TANKI_VISUAL_DIR") {
        let image = h.render().expect("render");
        let _ = image.save(PathBuf::from(dir).join("details-item.png"));
    }
    // 3D off: the flat picture, the parts still listed when the whole tank is on.
    h.state_mut().set_preview_3d(false);
    h.state_mut().set_whole_tank(true);
    h.run_steps(4);
    assert!(
        h.query_by_label(STAGE).is_none(),
        "no stage while 3D is off"
    );
    assert!(h.query_by_label("Hull: Hunter").is_some());
    assert!(h.query_by_label("Turret: Smoky").is_some());
    // Without the tank's other part in the cache, the item alone even with the whole tank on.
    h.state_mut().set_preview_3d(true);
    select(&mut h, "hull", "wasp", 3);
    h.state_mut().preview.tank.turret = ("nothing".into(), 3);
    h.state_mut().go(Page::Info);
    h.run_steps(4);
    assert!(h.query_by_label("Turret: Smoky").is_none());
    assert!(h.query_by_label(STAGE).is_some());
}
