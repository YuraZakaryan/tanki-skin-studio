// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn pictures_keep_their_shape_within_the_longest_side() {
    assert_eq!(picture_size(Vec2::new(800.0, 400.0), 2400), [800, 400]);
    assert_eq!(picture_size(Vec2::new(4800.0, 2400.0), 2400), [2400, 1200]);
    assert_eq!(picture_size(Vec2::new(300.0, 6000.0), 1600), [80, 1600]);
    assert_eq!(picture_size(Vec2::new(0.0, -5.0), 1600), [1, 1]);
    assert_eq!(picture_size(Vec2::new(f32::NAN, 10.0), 1600), [1, 10]);
}

#[test]
fn a_stage_without_a_gpu_is_unavailable_and_harmless() {
    let mut stage = Stage::new(None);
    assert!(!stage.is_available());
    stage.load(MAIN, "hull", &Scene::default());
    assert_eq!(stage.key(MAIN), None);
    assert!(stage.snapshot([64, 64]).is_none());
    let ctx = egui::Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(200.0, 100.0));
        stage.show(ui, rect, StageView::Compare { split: 0.5 }, "3D preview");
    });
    output.textures_delta.clear();
}
