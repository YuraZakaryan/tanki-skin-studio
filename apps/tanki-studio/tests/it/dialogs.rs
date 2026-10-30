// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui_kittest::kittest::Queryable;
use tanki_studio::app::{Dialog, Page};
use tanki_testkit::{HULL_MODEL, PackOptions, build_pack_dir, scratch};

use crate::common::{contents, env, harness, harness_with_pack};

/// The Garage with the fixture's hull and its skin picked (nothing is picked until a click).
fn open_garage_with_hull(h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>) {
    h.get_by_label("Garage").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Garage);
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
}

#[test]
fn applying_asks_first_and_cancel_changes_nothing() {
    let env = env("apply-cancel");
    let mut h = harness_with_pack(&env);
    open_garage_with_hull(&mut h);
    let before = contents(&env.cache.hull_files);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    let Some(Dialog::Confirm(pending)) = h.state().dialog.clone() else {
        panic!("expected the confirmation, got {:?}", h.state().dialog);
    };
    assert_eq!(pending.target, "Wasp M3");
    let names: Vec<&str> = pending.files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "details.jpg",
            "details_alpha.jpg",
            "lightmap.jpg",
            "object.3ds"
        ],
        "the dialog lists the files that will change"
    );
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(
        contents(&env.cache.hull_files),
        before,
        "nothing was written"
    );
    assert!(
        !env.work.join("data/backups").exists(),
        "and nothing was backed up"
    );
}

#[test]
fn confirming_replaces_the_files_shows_the_result_and_restore_puts_them_back() {
    let env = env("apply-confirm");
    let mut h = harness_with_pack(&env);
    open_garage_with_hull(&mut h);
    let before = contents(&env.cache.hull_files);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    // The only "Apply" button is the dialog's.
    h.get_all_by_label("Apply")
        .last()
        .expect("the dialog's Apply")
        .click();
    h.run_steps(5);
    let Some(Dialog::Done { target, files }) = h.state().dialog.clone() else {
        panic!("expected the result dialog, got {:?}", h.state().dialog);
    };
    assert_eq!(
        (target.as_str(), files),
        ("Wasp M3", 8),
        "2 revisions x 4 files"
    );
    let replaced = env
        .cache
        .hull_files
        .iter()
        .filter(|p| std::fs::read(p).is_ok_and(|b| b == HULL_MODEL))
        .count();
    assert_eq!(replaced, 2, "the model reached both revisions");
    assert_eq!(h.state().backups.len(), 1);

    // "Restore" is also in the top bar; the dialog is drawn last. It asks first.
    h.get_all_by_label("Restore")
        .last()
        .expect("the dialog's Restore")
        .click();
    h.run_steps(5);
    assert!(
        matches!(h.state().dialog, Some(Dialog::RestoreAll { .. })),
        "{:?}",
        h.state().dialog
    );
    h.get_all_by_label("Restore")
        .last()
        .expect("the question's Restore")
        .click();
    h.run_steps(5);
    assert!(h.state().dialog.is_none());
    assert_eq!(
        contents(&env.cache.hull_files),
        before,
        "every original byte is back"
    );
}

#[test]
fn a_running_game_blocks_the_replacement_until_it_is_closed() {
    let env = env("running");
    let mut h = harness_with_pack(&env);
    open_garage_with_hull(&mut h);
    let before = contents(&env.cache.hull_files);
    h.state_mut().game_processes_override = Some(vec!["GameClient.exe".into()]);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    assert!(
        matches!(h.state().dialog, Some(Dialog::GameRunning(..))),
        "{:?}",
        h.state().dialog
    );
    assert_eq!(contents(&env.cache.hull_files), before);
    // The player closes the game and checks again: now the normal confirmation follows.
    h.state_mut().game_processes_override = Some(Vec::new());
    h.get_by_label("Check again").click();
    h.run_steps(4);
    assert!(
        matches!(h.state().dialog, Some(Dialog::Confirm(..))),
        "{:?}",
        h.state().dialog
    );
}

#[test]
fn a_refused_pack_lists_its_problems_and_changes_nothing() {
    let env = env("bad-pack");
    let mut h = harness(&env);
    let bad = build_pack_dir(
        &scratch("bad-pack-src"),
        &PackOptions {
            corrupt_checksum: true,
            ..PackOptions::default()
        },
    )
    .unwrap();
    h.state_mut().import_path(&bad);
    h.run_steps(4);
    let Some(Dialog::ImportError { issues, .. }) = h.state().dialog.clone() else {
        panic!("expected the import error, got {:?}", h.state().dialog);
    };
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].file, "hull/model.3ds");
    assert!(h.state().packs.is_empty(), "the library stays empty");
    h.get_all_by_label("Close")
        .next()
        .expect("a Close button")
        .click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
}

#[test]
fn importing_shows_an_undo_that_removes_the_pack_again() {
    let env = env("undo");
    let mut h = harness(&env);
    h.state_mut().import_path(&env.zip);
    h.run_steps(4);
    assert_eq!(h.state().packs.len(), 1);
    assert_eq!(h.state().toasts.len(), 1);
    h.get_by_label("Undo").click();
    h.run_steps(4);
    assert!(
        h.state().packs.is_empty(),
        "Undo removed the pack that was just imported"
    );
    assert!(h.state().active_pack.is_none());
}

#[test]
fn deleting_a_pack_from_the_library_removes_it() {
    let env = env("delete");
    let mut h = harness_with_pack(&env);
    h.get_by_label("Packs").click();
    h.run_steps(3);
    h.get_by_label("Delete pack").click();
    h.run_steps(4);
    assert!(
        matches!(h.state().dialog, Some(Dialog::DeletePack { .. })),
        "asked first: {:?}",
        h.state().dialog
    );
    assert_eq!(
        h.state().packs.len(),
        1,
        "nothing deleted before the answer"
    );
    h.get_by_label("Delete").click();
    h.run_steps(4);
    assert!(h.state().packs.is_empty());
    assert!(h.state().active_pack.is_none());
}

#[test]
fn cancelling_the_deletion_keeps_the_pack() {
    let env = env("delete-cancel");
    let mut h = harness_with_pack(&env);
    h.get_by_label("Packs").click();
    h.run_steps(3);
    h.get_by_label("Delete pack").click();
    h.run_steps(4);
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().packs.len(), 1);
}

/// Two presses at the same point, as fast as a person double-clicks. A harness step moves egui's
/// clock by more than the 0.3 s of a double click, so the clock is held still between the presses.
fn double_click_at(
    h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>,
    pos: egui::Pos2,
) {
    let now = h.ctx.input(|i| i.time);
    h.input_mut().time = Some(now);
    for _ in 0..2 {
        h.event(egui::Event::PointerMoved(pos));
        for pressed in [true, false] {
            h.event(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        h.run_steps(1);
    }
    h.input_mut().time = None;
    h.run_steps(3);
}

#[test]
fn a_double_click_on_the_bin_deletes_no_other_pack() {
    let env = env("delete-double");
    let mut h = harness_with_pack(&env);
    for id in ["second", "third"] {
        let pack = build_pack_dir(
            &scratch(&format!("delete-double-{id}")),
            &PackOptions {
                id,
                ..PackOptions::default()
            },
        )
        .unwrap();
        h.state_mut().import_path(&pack);
        h.run_steps(2);
    }
    h.state_mut().toasts.clear();
    h.get_by_label("Packs").click();
    h.run_steps(3);
    assert_eq!(h.state().packs.len(), 3);
    let first = h.state().packs[0].manifest.id.clone();
    let bin = h
        .get_all_by_label("Delete pack")
        .next()
        .expect("a bin")
        .rect()
        .center();
    double_click_at(&mut h, bin);
    assert_eq!(h.state().packs.len(), 3, "the double click deleted nothing");
    let Some(Dialog::DeletePack { id, .. }) = h.state().dialog.clone() else {
        panic!("expected the question, got {:?}", h.state().dialog);
    };
    assert_eq!(id, first, "about the pack whose bin was clicked");
}

#[test]
fn a_double_clicked_apply_keeps_its_confirmation_open() {
    let env = env("apply-double");
    let mut h = harness_with_pack(&env);
    open_garage_with_hull(&mut h);
    let apply = h.get_by_label("Apply to Wasp M3").rect().center();
    double_click_at(&mut h, apply);
    assert!(
        matches!(h.state().dialog, Some(Dialog::Confirm(..))),
        "{:?}",
        h.state().dialog
    );
}

fn open_confirmation(h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>) {
    open_garage_with_hull(h);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    assert!(
        matches!(h.state().dialog, Some(Dialog::Confirm(..))),
        "{:?}",
        h.state().dialog
    );
}

fn press_dialog_apply(h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>) {
    h.get_all_by_label("Apply")
        .last()
        .expect("the dialog's Apply")
        .click();
    h.run_steps(5);
}

#[test]
fn a_pack_dropped_while_the_confirmation_is_open_is_not_imported() {
    let env = env("drop-under-confirm");
    let mut h = harness_with_pack(&env);
    open_confirmation(&mut h);
    let other = build_pack_dir(
        &scratch("drop-under-confirm-src"),
        &PackOptions {
            id: "other",
            ..PackOptions::default()
        },
    )
    .unwrap();
    h.state_mut().handle_dropped(vec![other]);
    h.run_steps(3);
    assert_eq!(
        h.state().packs.len(),
        1,
        "nothing imported under the dialog"
    );
    assert!(matches!(h.state().dialog, Some(Dialog::Confirm(..))));
}

#[test]
fn the_confirmation_applies_the_pack_it_names_when_another_became_active() {
    let env = env("swap-under-confirm");
    let mut h = harness_with_pack(&env);
    open_confirmation(&mut h);
    // Another pack becomes active while the dialog is open; its first skin has no model.
    let other = build_pack_dir(
        &scratch("swap-under-confirm-src"),
        &PackOptions {
            id: "other",
            hull_without_model: true,
            ..PackOptions::default()
        },
    )
    .unwrap();
    h.state_mut().import_path(&other);
    h.run_steps(3);
    assert_eq!(
        h.state()
            .active_pack
            .as_ref()
            .map(|p| p.manifest.id.as_str()),
        Some("other")
    );
    press_dialog_apply(&mut h);
    let models = env
        .cache
        .hull_files
        .iter()
        .filter(|p| std::fs::read(p).is_ok_and(|b| b == HULL_MODEL))
        .count();
    assert_eq!(
        models, 2,
        "the confirmed pack's skin reached both revisions"
    );
}

#[test]
fn a_pack_removed_under_the_confirmation_replaces_nothing() {
    let env = env("removed-under-confirm");
    let mut h = harness_with_pack(&env);
    open_confirmation(&mut h);
    let before = contents(&env.cache.hull_files);
    h.state_mut().delete_pack("fixture-pack", "1.0.0");
    h.run_steps(2);
    press_dialog_apply(&mut h);
    assert_eq!(contents(&env.cache.hull_files), before);
    assert!(h.state().dialog.is_none());
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("no longer in the library")),
        "the player is told why nothing happened"
    );
}

#[test]
fn a_game_started_while_the_confirmation_is_open_stops_the_replacement() {
    let env = env("started-under-confirm");
    let mut h = harness_with_pack(&env);
    open_confirmation(&mut h);
    let before = contents(&env.cache.hull_files);
    h.state_mut().game_processes_override = Some(vec!["GameClient.exe".into()]);
    press_dialog_apply(&mut h);
    assert!(
        matches!(h.state().dialog, Some(Dialog::GameRunning(..))),
        "{:?}",
        h.state().dialog
    );
    assert_eq!(
        contents(&env.cache.hull_files),
        before,
        "nothing was written"
    );
}

#[test]
fn restoring_while_the_game_runs_changes_nothing() {
    let env = env("restore-running");
    let mut h = harness_with_pack(&env);
    open_confirmation(&mut h);
    press_dialog_apply(&mut h);
    let applied = contents(&env.cache.hull_files);
    h.state_mut().dialog = None;
    h.state_mut().game_processes_override = Some(vec!["GameClient.exe".into()]);
    h.state_mut().ask_to_restore();
    h.run_steps(2);
    assert!(h.state().dialog.is_none(), "not even asked while it runs");
    h.state_mut().restore_original();
    h.run_steps(2);
    assert_eq!(contents(&env.cache.hull_files), applied, "the skin stays");
    assert!(
        h.state().can_restore(),
        "the backup is still there to restore"
    );
}

#[test]
fn restore_asks_first_then_undoes_every_replacement_back_to_the_original() {
    let env = env("restore-all");
    let mut h = harness_with_pack(&env);
    let original = contents(&env.cache.hull_files);
    open_confirmation(&mut h);
    press_dialog_apply(&mut h);
    h.state_mut().dialog = None;
    // The same skin again, from a second pack, and the turret: three replacements in all.
    let second = build_pack_dir(
        &scratch("restore-all-src"),
        &PackOptions {
            id: "second",
            ..PackOptions::default()
        },
    )
    .unwrap();
    h.state_mut().import_path(&second);
    h.run_steps(3);
    open_confirmation(&mut h);
    press_dialog_apply(&mut h);
    h.state_mut().dialog = None;
    let turrets = contents(&env.cache.turret_files);
    h.state_mut().begin_apply(1, 0);
    h.run_steps(2);
    press_dialog_apply(&mut h);
    h.state_mut().dialog = None;
    assert_eq!(h.state().backups.len(), 3);
    assert_ne!(contents(&env.cache.turret_files), turrets);

    // The top bar's Restore asks; Cancel changes nothing.
    h.run_steps(3);
    h.get_by_label("Restore").click();
    h.run_steps(4);
    let Some(Dialog::RestoreAll {
        items,
        replacements,
        files,
    }) = h.state().dialog.clone()
    else {
        panic!("expected the question, got {:?}", h.state().dialog);
    };
    assert_eq!(
        items,
        ["Railgun M3", "Wasp M3"],
        "each item once, newest first"
    );
    assert_eq!((replacements, files), (3, 24));
    h.get_by_label("Cancel").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(h.state().backups.iter().filter(|b| !b.restored).count(), 3);

    h.run_steps(3);
    h.get_by_label("Restore").click();
    h.run_steps(4);
    h.get_all_by_label("Restore")
        .last()
        .expect("the question's Restore")
        .click();
    h.run_steps(5);
    assert_eq!(
        contents(&env.cache.hull_files),
        original,
        "the hull is back to the original"
    );
    assert_eq!(contents(&env.cache.turret_files), turrets, "and the turret");
    assert!(!h.state().can_restore(), "nothing left to restore");
}
