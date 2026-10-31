// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The Resources page's inspector.

use base64::{Engine, engine::general_purpose::STANDARD};
use egui::Role;
use egui_kittest::kittest::Queryable;

use crate::common::{env, harness};

#[test]
fn a_very_long_resource_name_stays_inside_the_inspector_and_shows_whole_in_a_tooltip() {
    let env = env("long-name");
    let name = format!("{}.png", "a_very_long_texture_file_name".repeat(5));
    let url = format!("http://cache.test/9/8/7/65/1/{name}");
    std::fs::write(env.cache.root.join(STANDARD.encode(url)), b"not an image").unwrap();
    let mut h = harness(&env);
    let key = h
        .state()
        .rows
        .iter()
        .find(|r| r.title() == name)
        .map(|r| r.key.clone())
        .expect("the long-named resource is listed");
    h.state_mut().go(tanki_studio::app::Page::Resources);
    h.state_mut().selected_resource = Some(key);
    h.run_steps(4);

    let title = h.get_by_role_and_label(Role::Label, &name).rect();
    // The buttons at the bottom span the inspector's inner width.
    let button = h
        .get_all_by_label("Label manually")
        .map(|b| b.rect())
        .next()
        .expect("the inspector's buttons");
    assert!(title.width() > 0.0);
    assert!(
        title.min.x >= button.min.x - 0.5 && title.max.x <= button.max.x + 0.5,
        "the name ({title:?}) stays within the inspector ({button:?})"
    );

    h.get_by_role_and_label(Role::Label, &name).hover();
    h.run_steps(4);
    assert!(
        h.query_all_by_role_and_label(Role::Label, &name).count() >= 2,
        "hovering the shortened name shows the whole of it"
    );
}
