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
    h.state_mut().restore_last();
    h.state_mut().restore_last();
    h.run_steps(2);
    let same = h
        .state()
        .toasts
        .iter()
        .filter(|t| t.text == "No backup to restore")
        .count();
    assert_eq!(same, 1, "one toast, not two");
}
