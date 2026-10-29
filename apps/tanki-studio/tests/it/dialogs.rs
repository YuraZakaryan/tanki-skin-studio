// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui_kittest::kittest::Queryable;
use tanki_studio::app::{Dialog, Page};
use tanki_testkit::{HULL_MODEL, PackOptions, build_pack_dir, scratch};

use crate::common::{contents, env, harness, harness_with_pack};

fn open_garage_with_hull(h: &mut egui_kittest::Harness<'static, tanki_studio::app::Studio>) {
    h.get_by_label("Garage").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Garage);
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

    // "Restore" is also in the top bar; the dialog is drawn last.
    h.get_all_by_label("Restore")
        .last()
        .expect("the dialog's Restore")
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
    assert!(h.state().packs.is_empty());
    assert!(h.state().active_pack.is_none());
}
