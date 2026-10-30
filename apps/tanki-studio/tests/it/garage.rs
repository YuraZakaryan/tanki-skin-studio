// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The Garage: a skin that does not bring every model and texture file of the item cannot be
//! applied, through the button or any other way, and the cache is left as it was.

use egui_kittest::kittest::{NodeT, Queryable};
use tanki_studio::app::Page;
use tanki_testkit::{PackOptions, build_pack_dir};

use crate::common::{contents, env, harness};

#[test]
fn a_skin_without_a_model_is_marked_and_cannot_be_applied() {
    let env = env("incomplete-skin");
    let pack = build_pack_dir(
        &env.work.join("no-model"),
        &PackOptions {
            id: "no-model",
            hull_without_model: true,
            ..PackOptions::default()
        },
    )
    .expect("pack fixture");
    let mut h = harness(&env);
    h.state_mut().import_path(&pack);
    h.run_steps(4);
    let before = contents(&env.cache.hull_files);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    assert_eq!(h.state().page, Page::Garage);
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);

    assert!(
        h.query_by_label("Incomplete").is_some(),
        "the variant row says the skin is incomplete"
    );
    let apply = h.get_by_label("Apply to Wasp M3");
    assert!(
        apply.accesskit_node().is_disabled(),
        "the one Apply button is off"
    );
    apply.click();
    h.run_steps(4);
    assert!(
        h.state().dialog.is_none(),
        "no confirmation for an incomplete skin"
    );

    // The palette and scripted runs go through the same refusal, and say why.
    h.state_mut().begin_apply_selected();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("object.3ds")),
        "the reason names the missing file"
    );
    assert_eq!(
        contents(&env.cache.hull_files),
        before,
        "nothing was written"
    );
}

#[test]
fn a_complete_skin_keeps_the_apply_button_on() {
    let env = env("complete-skin");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.run_steps(4);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    assert!(
        h.get_by_label("Apply to Wasp M3")
            .accesskit_node()
            .is_disabled(),
        "off until a skin is picked"
    );
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    assert!(h.query_by_label("Incomplete").is_none());
    assert!(
        !h.get_by_label("Apply to Wasp M3")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_skin_of_another_item_goes_to_its_own_item_never_to_the_selected_one() {
    let env = env("other-item");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.run_steps(4);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    assert_eq!(
        h.state().selection.as_ref().map(|s| s.id.as_str()),
        Some("wasp")
    );
    // The palette or a script names the Railgun skin (pack item 1) while the Wasp is selected.
    h.state_mut().begin_apply(1, 0);
    h.run_steps(2);
    let Some(tanki_studio::app::Dialog::Confirm(pending)) = h.state().dialog.clone() else {
        panic!("expected the confirmation, got {:?}", h.state().dialog);
    };
    assert_eq!(pending.cache_target.id, "railgun", "not onto the Wasp");
    assert_eq!(pending.target, "Railgun M3");
}

#[test]
fn a_repeated_message_refreshes_instead_of_stacking() {
    let env = env("toasts");
    let mut h = harness(&env);
    h.state_mut().ask_to_restore();
    h.state_mut().ask_to_restore();
    h.run_steps(2);
    let same = h
        .state()
        .toasts
        .iter()
        .filter(|t| t.text == "No backup to restore")
        .count();
    assert_eq!(same, 1, "one toast, not two");
}

fn apply_wasp_red(h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>) {
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    h.get_all_by_label("Apply")
        .last()
        .expect("the dialog's Apply")
        .click();
    h.run_steps(5);
    h.state_mut().dialog = None;
    h.run_steps(3);
}

fn picked(h: &egui_kittest::Harness<'static, tanki_studio::app::Studio>) -> Option<(usize, usize)> {
    let selection = h.state().selection.clone().expect("a selection");
    h.state().chosen_variant(&selection)
}

#[test]
fn the_garage_shows_the_skin_in_the_game_and_restore_resets_the_pick() {
    let env = env("applied-state");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.run_steps(4);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    assert_eq!(picked(&h), None, "nothing is picked before a click");
    assert!(h.query_by_label("Applied").is_none());

    apply_wasp_red(&mut h);
    assert!(
        h.query_by_label("Applied").is_some(),
        "the skin in the game says so"
    );
    assert!(
        h.get_by_label("Apply to Wasp M3")
            .accesskit_node()
            .is_disabled(),
        "the skin already in the game is not applied again"
    );

    // A restart: the cache still wears the skin, and the Garage says so without a saved pick.
    drop(h);
    let mut h = harness(&env);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    assert!(
        h.query_by_label("Applied").is_some(),
        "known after a restart"
    );
    // The player picks the skin by hand as well: Restore must forget that pick too.
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    assert!(picked(&h).is_some());

    h.get_by_label("Restore").click();
    h.run_steps(4);
    h.get_all_by_label("Restore")
        .last()
        .expect("the question's Restore")
        .click();
    h.run_steps(5);
    assert!(
        h.query_by_label("Applied").is_none(),
        "nothing is in the game"
    );
    assert_eq!(picked(&h), None, "Restore resets the pick as well");
    assert!(
        h.get_by_label("Apply to Wasp M3")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_cache_file_changed_since_is_no_longer_called_applied() {
    let env = env("applied-changed");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.run_steps(4);
    h.get_by_label("Garage").click();
    h.run_steps(4);
    apply_wasp_red(&mut h);
    assert!(h.query_by_label("Applied").is_some());
    // The game (or another tool) rewrites one of the files.
    std::fs::write(&env.cache.hull_files[0], b"updated by the game").unwrap();
    h.state_mut().reload_backups();
    h.run_steps(3);
    assert!(h.query_by_label("Applied").is_none());
}

#[test]
fn skin_variants_list_m0_to_m3_first_then_the_editions() {
    let env = env("variant-order");
    let dir = tanki_testkit::scratch("variant-order-pack");
    build_pack_dir(&dir, &PackOptions::default()).expect("pack fixture");
    // The same Wasp skin filed under several modifications and editions, out of order, the way
    // packs from different authors name them.
    let path = dir.join("manifest.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let hull = manifest["items"][0].clone();
    let mut items = Vec::new();
    for (grade, name) in [
        ("m3", "Wasp Old"),
        ("prime", "Wasp"),
        ("m3", "Wasp New"),
        ("xt-m0", "Wasp"),
        ("m0", "Wasp Old"),
        ("m1", "Wasp Camo 10"),
        ("m1", "Wasp Camo 2"),
        ("m0", "Wasp New"),
    ] {
        let mut item = hull.clone();
        item["target"]["grade"] = grade.into();
        item["variants"][0]["name"] = name.into();
        item["variants"][0]["id"] = name.to_lowercase().replace(' ', "-").into();
        items.push(item);
    }
    manifest["items"] = items.into();
    std::fs::write(&path, serde_json::to_vec_pretty(&manifest).unwrap()).unwrap();

    let mut h = harness(&env);
    h.state_mut().import_path(&dir);
    h.run_steps(4);
    // The cache's Wasp is an M3: its own skins used to come first.
    let order: Vec<String> = h
        .state()
        .pack_variants("hull", "wasp")
        .into_iter()
        .filter_map(|(i, v)| {
            let (item, variant) = h.state().pack_variant(i, v)?;
            Some(format!("{} {}", item.target.grade, variant.name))
        })
        .collect();
    assert_eq!(
        order,
        [
            "m0 Wasp New",
            "m0 Wasp Old",
            "m1 Wasp Camo 2",
            "m1 Wasp Camo 10",
            "m3 Wasp New",
            "m3 Wasp Old",
            "xt-m0 Wasp",
            "prime Wasp",
        ]
    );
}

/// The library index of the pack with this id.
fn pack_index(h: &egui_kittest::Harness<'static, tanki_studio::app::Studio>, id: &str) -> usize {
    h.state()
        .packs
        .iter()
        .position(|p| p.manifest.id == id)
        .expect("the pack is in the library")
}

#[test]
fn a_pick_belongs_to_its_pack_and_comes_back_with_it() {
    // Two packs whose skins share ids ("red" for the Wasp), as packs built from archives do.
    let env = env("pick-per-pack");
    let second = build_pack_dir(
        &env.work.join("second"),
        &PackOptions {
            id: "second-pack",
            ..PackOptions::default()
        },
    )
    .expect("pack fixture");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.state_mut().import_path(&second);
    let first = pack_index(&h, "fixture-pack");
    h.state_mut().select_pack(first);
    h.get_by_label("Garage").click();
    h.run_steps(3);
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    let selection = h.state().selection.clone().expect("the Wasp is selected");
    assert!(
        h.state().chosen_variant(&selection).is_some(),
        "picked in the first pack"
    );

    let other = pack_index(&h, "second-pack");
    h.state_mut().select_pack(other);
    h.run_steps(3);
    assert_eq!(
        h.state().chosen_variant(&selection),
        None,
        "the other pack's skin of the same id is not picked: nothing was clicked in it"
    );
    assert!(
        h.get_by_label("Apply to Wasp M3")
            .accesskit_node()
            .is_disabled(),
        "so Apply stays off"
    );

    let first = pack_index(&h, "fixture-pack");
    h.state_mut().select_pack(first);
    h.run_steps(3);
    assert!(
        h.state().chosen_variant(&selection).is_some(),
        "back in the first pack, its pick is there again"
    );
}

#[test]
fn a_skin_in_the_game_from_another_pack_is_named_with_its_pack() {
    let env = env("worn-other-pack");
    let second = build_pack_dir(
        &env.work.join("second"),
        &PackOptions {
            id: "second-pack",
            ..PackOptions::default()
        },
    )
    .expect("pack fixture");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.state_mut().import_path(&second);
    let first = pack_index(&h, "fixture-pack");
    h.state_mut().select_pack(first);
    h.get_by_label("Garage").click();
    h.run_steps(3);
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    h.state_mut().begin_apply_selected();
    h.run_steps(2);
    let Some(tanki_studio::app::Dialog::Confirm(pending)) = h.state().dialog.clone() else {
        panic!("the confirmation is open");
    };
    h.state_mut().confirm_apply(pending);
    h.state_mut().dialog = None;
    h.run_steps(2);
    let selection = h.state().selection.clone().expect("the Wasp is selected");
    assert_eq!(
        h.state().in_game_name(&selection).as_deref(),
        Some("Wasp Red"),
        "the active pack's own skin needs no pack name"
    );

    let other = pack_index(&h, "second-pack");
    h.state_mut().select_pack(other);
    h.run_steps(3);
    let name = h.state().packs[first].manifest.name.clone();
    assert_eq!(
        h.state().in_game_name(&selection),
        Some(format!("Wasp Red · {name}")),
        "the other pack has a Wasp Red too: the pack says which one is in the game"
    );
}
