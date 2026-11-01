// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! "Apply to all items": a look picked once goes on every item that has it, each modification as
//! chosen, every write through a transaction of one batch, undone in one go.

use std::{path::PathBuf, sync::mpsc, time::Duration};

use egui::{Key, Role};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use tanki_core::{pack::import_pack, state::StateStore};
use tanki_studio::{
    app::{BulkStep, BulkWizard, Dialog, ItemState, Page, RowFilter, Selection, Studio},
    bulk::{Cell, Choice, Failure, Pick, Source},
};
use tanki_testkit::{LooksFixture, build_looks, looks_model, scratch};

use crate::common::{contents, env, harness_in};

/// The looks fixture with both packs imported ("Looks B" last, so active and first) and its
/// items labelled.
struct Looks {
    work: PathBuf,
    fixture: LooksFixture,
}

fn looks(label: &str) -> Looks {
    let work = scratch(label);
    let fixture = build_looks(&work.join("looks")).expect("looks fixture");
    let data = work.join("data");
    let store = StateStore::open(&data.join("state.sqlite")).expect("state store");
    for (marker, kind, id, grade) in &fixture.labels {
        store
            .save_user_target("air-cache", marker, kind, id, grade)
            .expect("label");
    }
    for pack in &fixture.packs {
        let pack = import_pack(pack, &data.join("packs")).expect("import");
        store.save_pack(&pack).expect("save pack");
    }
    Looks { work, fixture }
}

fn harness(looks: &Looks) -> Harness<'static, Studio> {
    harness_in(looks.work.join("data"), vec![looks.fixture.cache.clone()])
}

impl Looks {
    fn all(&self) -> Vec<PathBuf> {
        self.fixture
            .items
            .iter()
            .flat_map(|i| i.all_files())
            .collect()
    }

    fn files(&self, id: &str, k: usize) -> Vec<PathBuf> {
        self.fixture.item(id).files[k].clone()
    }

    /// The model files of an item's modification, as they are now.
    fn models(&self, id: &str, k: usize) -> Vec<Vec<u8>> {
        contents(&self.files(id, k))
            .into_iter()
            .filter(|bytes| bytes.ends_with(b"model") || bytes.ends_with(b"object.3ds"))
            .collect()
    }

    fn wears(&self, id: &str, k: usize, pack: &str, grade: &str) -> bool {
        let models = self.models(id, k);
        models.len() == 2 && models.iter().all(|m| *m == looks_model(pack, id, grade))
    }
}

fn wizard<'a>(h: &'a Harness<'static, Studio>) -> &'a BulkWizard {
    h.state().bulk.as_ref().expect("the wizard is open")
}

/// The Garage's "Apply to all items".
fn open(h: &mut Harness<'static, Studio>) {
    h.state_mut().go(Page::Garage);
    h.run_steps(3);
    h.get_by_label("Apply to all items").click();
    h.run_steps(3);
}

/// Open the wizard and go to the last step with the defaults (XT, every modification its own).
fn to_check(h: &mut Harness<'static, Studio>) {
    open(h);
    h.get_by_label("Next: where").click();
    h.run_steps(3);
    h.get_by_label("Check and apply").click();
    h.run_steps(3);
    assert_eq!(wizard(h).step, BulkStep::Check);
}

fn cell<'a>(h: &'a Harness<'static, Studio>, id: &str, k: usize) -> &'a Option<Cell> {
    &wizard(h)
        .plan
        .as_ref()
        .expect("a plan")
        .rows
        .iter()
        .find(|r| r.id == id)
        .expect("the item's row")
        .cells[k]
}

#[test]
fn opens_from_the_garage_and_the_palette_and_esc_closes_it() {
    let looks = looks("bulk-open");
    let mut h = harness(&looks);
    open(&mut h);
    let names: Vec<&str> = wizard(&h).looks.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["XT", "Legacy", "Prime", "Desert Camo"]);
    assert_eq!(wizard(&h).look, Some(0), "the first look is picked");
    assert!(
        h.query_by_role_and_label(Role::Dialog, "Apply a look to all items")
            .is_some(),
        "a dialog, as screen readers know it"
    );
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(
        h.state().bulk.is_none(),
        "Esc closes it before the last step"
    );

    h.state_mut().open_palette();
    if let Some(palette) = h.state_mut().palette.as_mut() {
        palette.query = "Apply to all".into();
    }
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().palette.is_none());
    assert_eq!(
        wizard(&h).step,
        BulkStep::Look,
        "the palette's Enter opens it and goes no further"
    );
}

#[test]
fn with_no_look_it_offers_to_add_a_pack() {
    let env = env("bulk-empty");
    let mut h = crate::common::harness(&env);
    open(&mut h);
    assert!(wizard(&h).looks.is_empty());
    assert!(h.query_by_label("No looks to apply yet").is_some());
    h.get_by_label("Add a pack").click();
    h.run_steps(3);
    assert!(h.state().bulk.is_none());
    assert_eq!(h.state().page, Page::Packs, "the import, where packs are");
}

#[test]
fn every_cell_of_the_table_says_what_happens_and_every_choice_changes_it() {
    let looks = looks("bulk-where");
    let mut h = harness(&looks);
    open(&mut h);
    h.get_by_role_and_label(Role::RadioButton, "Legacy").click();
    h.run_steps(2);
    assert_eq!(wizard(&h).look, Some(1));
    h.get_by_role_and_label(Role::RadioButton, "XT").click();
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(
        wizard(&h).step,
        BulkStep::Where,
        "Enter is the primary button"
    );

    let plan = wizard(&h).plan.clone().expect("a plan");
    assert_eq!(plan.columns, [true; 4]);
    assert_eq!(
        (plan.changing_items(), plan.skipped_items(), plan.changes()),
        (4, 0, 12),
        "every item changes something, so none is skipped as a whole"
    );
    assert!(matches!(
        cell(&h, "dictator", 2),
        Some(Cell::Incomplete { .. })
    ));
    assert_eq!(cell(&h, "hunter", 0), &Some(Cell::NotInGame));
    assert_eq!(
        cell(&h, "smoky", 1),
        &Some(Cell::Missing { source: Some(1) })
    );
    assert!(
        matches!(cell(&h, "viking", 3), Some(Cell::Change { skin, .. }) if skin.pack_id == "looks-b"),
        "the same look of the same item in two packs: the first pack's (the active one)"
    );
    for label in ["Skin incomplete", "No M0 in game", "No XT M1", "XT M3"] {
        assert!(
            h.query_all_by_label(label).next().is_some(),
            "a cell says “{label}”"
        );
    }

    // A modification turned off is not touched.
    h.get_by_label("Put skins on M3").click();
    h.run_steps(3);
    assert!(!wizard(&h).settings.as_ref().expect("settings").choices[3].on);
    assert_eq!(cell(&h, "viking", 3), &Some(Cell::Off));
    assert_eq!(wizard(&h).plan.as_ref().map(|p| p.changes()), Some(8));

    // M1 from M3: the Smoky, with no XT made for M1, gets one now.
    h.get_by_label("What M1 gets").click();
    h.run_steps(3);
    h.get_by_label("XT · from M3").click();
    h.run_steps(3);
    assert_eq!(
        wizard(&h).settings.as_ref().expect("settings").choices[1],
        Choice {
            on: true,
            source: Source::From(3)
        }
    );
    assert!(cell(&h, "smoky", 1).as_ref().is_some_and(Cell::changes));

    // One item's own pick, and back to the common choice.
    h.get_by_label("Own choices for Viking").click();
    h.run_steps(3);
    assert_eq!(
        wizard(&h).expanded,
        Some(("hull".to_owned(), "viking".to_owned()))
    );
    assert!(
        h.query_by_label("What M0 of Viking gets").is_some(),
        "one line, for its first modification"
    );
    h.get_by_label("Modification of Viking").click();
    h.run_steps(3);
    h.get_by_label("M2").click();
    h.run_steps(3);
    assert_eq!(wizard(&h).editing, Some(2));
    h.get_by_label("What M2 of Viking gets").click();
    h.run_steps(3);
    h.get_by_label("XT · M0").click();
    h.run_steps(3);
    let key = ("hull".to_owned(), "viking".to_owned(), 2);
    assert_eq!(
        wizard(&h)
            .settings
            .as_ref()
            .expect("settings")
            .overrides
            .get(&key),
        Some(&Pick::Skin(Some(0)))
    );
    assert!(h.query_by_label("XT M0 · own pick").is_some());
    assert!(
        cell(&h, "dictator", 2) != &Some(Cell::Off),
        "the others keep the common choice"
    );
    h.get_by_label("Use the common choice for M2").click();
    h.run_steps(3);
    assert!(
        wizard(&h)
            .settings
            .as_ref()
            .expect("settings")
            .overrides
            .is_empty()
    );

    // The kinds: turrets left out of the run; the filter only shows.
    h.get_by_role_and_label(Role::CheckBox, "Turrets").click();
    h.run_steps(3);
    let plan = wizard(&h).plan.clone().expect("a plan");
    assert!(plan.rows.iter().all(|r| r.kind == "hull"));
    h.get_by_label("Skipped").click();
    h.run_steps(3);
    assert_eq!(wizard(&h).filter, RowFilter::Skipped);
    assert!(
        h.query_by_label("Own choices for Viking").is_none(),
        "every item changes something: none is skipped as a whole"
    );
    h.get_by_label("Will change").click();
    h.run_steps(3);
    assert_eq!(wizard(&h).filter, RowFilter::Change);
    assert!(h.query_by_label("Own choices for Dictator").is_some());
    assert_eq!(
        wizard(&h).plan.as_ref(),
        Some(&plan),
        "a filter changes no plan"
    );
}

#[test]
fn a_running_game_stops_it_then_every_item_gets_its_skin_in_one_batch() {
    let looks = looks("bulk-apply");
    let all = looks.all();
    let before = contents(&all);
    let mut h = harness(&looks);
    to_check(&mut h);
    assert!(
        h.query_by_role_and_label(Role::CheckBox, "Make a backup first")
            .is_some_and(|b| b.accesskit_node().is_disabled()),
        "the backup is always on"
    );
    h.state_mut().game_processes_override = Some(vec!["Game.exe".into()]);
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(Dialog::BulkGameRunning(_))));
    assert_eq!(
        contents(&all),
        before,
        "nothing written while the game runs"
    );
    assert!(!looks.work.join("data/backups").exists());

    h.state_mut().game_processes_override = Some(Vec::new());
    h.get_by_label("Check again").click();
    h.run_steps(4);
    assert_eq!(wizard(&h).step, BulkStep::Done);
    assert!(looks.wears("viking", 0, "looks-a", "xt-m0"));
    assert!(looks.wears("viking", 3, "looks-b", "xt-m3"));
    assert!(looks.wears("hunter", 1, "looks-a", "xt-m1"));
    assert!(looks.wears("smoky", 0, "looks-b", "xt-m0"));
    assert!(looks.wears("smoky", 3, "looks-a", "xt-m3"));
    let unchanged: Vec<PathBuf> = [("smoky", 1), ("dictator", 2), ("thunder", 0)]
        .iter()
        .flat_map(|(id, k)| looks.files(id, *k))
        .collect();
    let index = |p: &PathBuf| all.iter().position(|a| a == p).expect("a fixture file");
    for path in &unchanged {
        assert_eq!(
            std::fs::read(path).ok(),
            before.get(index(path)).cloned(),
            "skipped: {}",
            path.display()
        );
    }
    let backups = &h.state().backups;
    assert_eq!(
        backups.len(),
        12,
        "one transaction per item and modification"
    );
    let batch = backups[0].tx.batch.clone().expect("a batch");
    assert_eq!(batch.name, "XT");
    assert!(backups.iter().all(|b| b.tx.batch.as_ref() == Some(&batch)));
    assert!(h.query_by_label("4 applied").is_some());
    assert!(h.query_by_label("0 skipped").is_some());

    // The Garage shows what the items wear now.
    h.get_by_label("Done").click();
    h.run_steps(3);
    assert!(h.state().bulk.is_none());
    let selection = |id: &str, grade: usize| Selection {
        kind: "hull".into(),
        id: id.into(),
        grade,
    };
    assert_eq!(
        h.state().in_game_name(&selection("viking", 3)).as_deref(),
        Some("Viking")
    );
    assert_eq!(
        h.state().in_game_name(&selection("viking", 0)).as_deref(),
        Some("Viking · Looks A"),
        "a skin of another pack than the active one is named with its pack"
    );
    assert!(
        h.state().chosen_variant(&selection("viking", 3)).is_some(),
        "it is the item's pick"
    );
    // Applied again: everything that changed is on already.
    open(&mut h);
    h.get_by_label("Next: where").click();
    h.run_steps(3);
    assert_eq!(cell(&h, "viking", 0), &Some(Cell::Already));
    assert_eq!(
        wizard(&h).plan.as_ref().map(|p| p.changes()),
        Some(0),
        "nothing left to change"
    );
    assert!(
        h.get_by_label("Check and apply")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a_failing_item_is_left_as_it_was_and_the_others_are_applied() {
    let looks = looks("bulk-fail");
    let mut h = harness(&looks);
    to_check(&mut h);
    // The Viking's M1 loses its model after the plan was made: its write fails mid-item.
    for path in looks.files("viking", 1) {
        if std::fs::read(&path).is_ok_and(|b| b.ends_with(b"object.3ds")) {
            std::fs::remove_file(&path).expect("remove");
        }
    }
    let viking_m0 = contents(&looks.files("viking", 0));
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(4);
    assert_eq!(wizard(&h).step, BulkStep::Done);
    let run = wizard(&h).run.as_ref().expect("the run");
    let viking = run
        .tasks
        .iter()
        .position(|t| t.id == "viking")
        .expect("the Viking");
    assert!(
        matches!(&run.states[viking], ItemState::Failed { modification: 1, failure: Failure::Error(error), .. } if error.contains("required cache resource missing")),
        "{:?}",
        run.states[viking]
    );
    assert_eq!(
        contents(&looks.files("viking", 0)),
        viking_m0,
        "its M0, written before, is put back"
    );
    assert_eq!(run.applied(), 3);
    assert!(looks.wears("hunter", 3, "looks-a", "xt-m3"));
    assert!(h.query_by_label("1 not applied").is_some());
    assert!(
        h.query_all_by_label_contains("Viking · M1:")
            .next()
            .is_some(),
        "the reason is said for the item"
    );
    // Said in the language of the moment, not the one the run ended in.
    h.state_mut()
        .set_language(tanki_i18n::Lang::from_code("ru").expect("Russian"));
    h.run_steps(3);
    assert!(
        h.query_all_by_label_contains("Viking · M1: в кэше нет файла")
            .next()
            .is_some(),
        "the reason follows the language"
    );
}

#[test]
fn stop_after_this_item_finishes_it_and_writes_no_other() {
    let looks = looks("bulk-stop");
    let all = looks.all();
    let before = contents(&all);
    let mut h = harness(&looks);
    let (permit, gate) = mpsc::channel();
    h.state_mut().bulk_gate = Some(gate);
    h.state_mut().background_bulk = true;
    to_check(&mut h);
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(3);
    assert_eq!(wizard(&h).step, BulkStep::Running);
    let settle = |h: &mut Harness<'static, Studio>, done: &dyn Fn(&Studio) -> bool| {
        for _ in 0..500 {
            h.run_steps(1);
            if done(h.state()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the run did not get there");
    };
    permit.send(()).expect("the first item");
    settle(&mut h, &|app| {
        app.bulk
            .as_ref()
            .and_then(|w| w.run.as_ref())
            .is_some_and(|run| matches!(run.states[0], ItemState::Applied { .. }))
    });
    assert!(h.query_by_label("1 of 4 items").is_some());
    // While it writes: no Esc, no import, no other cache.
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(wizard(&h).step, BulkStep::Running);
    h.state_mut().import_path(&looks.fixture.packs[0]);
    assert!(h.state().import_job.is_none());
    h.state_mut().switch_cache(0);
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("Wait until it is done"))
    );
    h.get_by_label("Stop after this item").click();
    h.run_steps(2);
    permit.send(()).expect("the next item, which stops");
    settle(&mut h, &|app| {
        app.bulk.as_ref().is_some_and(|w| w.step == BulkStep::Done)
    });
    let run = wizard(&h).run.as_ref().expect("the run");
    assert!(matches!(run.states[0], ItemState::Applied { .. }));
    assert!(run.states[1..].iter().all(|s| *s == ItemState::NotRun));
    assert!(h.query_by_label("Stopped: 3 items not changed.").is_some());
    let first = &run.tasks[0].id;
    let rest: Vec<PathBuf> = looks
        .fixture
        .items
        .iter()
        .filter(|i| i.id != first)
        .flat_map(|i| i.all_files())
        .collect();
    let index = |p: &PathBuf| all.iter().position(|a| a == p).expect("a fixture file");
    for path in &rest {
        assert_eq!(std::fs::read(path).ok(), before.get(index(path)).cloned());
    }
}

#[test]
fn undo_all_puts_back_what_every_item_wore() {
    let looks = looks("bulk-undo");
    let all = looks.all();
    let before = contents(&all);
    let mut h = harness(&looks);
    to_check(&mut h);
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(4);
    assert_ne!(contents(&all), before);
    h.get_by_label("Undo all").click();
    h.run_steps(3);
    assert!(h.state().bulk.is_none());
    assert_eq!(contents(&all), before, "every file is the original again");
    assert!(h.state().backups.iter().all(|b| b.restored));
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("Undid “XT”"))
    );
    assert!(
        h.state()
            .in_game_name(&Selection {
                kind: "hull".into(),
                id: "viking".into(),
                grade: 0
            })
            .is_none()
    );
}

#[test]
fn the_batch_is_one_entry_of_the_backups_which_undoes_it() {
    let looks = looks("bulk-backups");
    let all = looks.all();
    let before = contents(&all);
    let mut h = harness(&looks);
    to_check(&mut h);
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(4);
    h.get_by_label("Done").click();
    h.run_steps(2);
    h.state_mut().go(Page::Packs);
    h.run_steps(3);
    assert_eq!(
        tanki_studio::model::backup_rows(&h.state().backups).len(),
        1,
        "twelve replacements, one entry"
    );
    h.get_by_label("XT · 4 items").click();
    h.run_steps(3);
    assert!(matches!(
        h.state().dialog,
        Some(Dialog::RestoreBatch { .. })
    ));
    h.get_by_label("Undo all").click();
    h.run_steps(3);
    assert_eq!(contents(&all), before);

    // Restore original files undoes a batch too.
    open(&mut h);
    h.get_by_label("Next: where").click();
    h.run_steps(3);
    h.get_by_label("Check and apply").click();
    h.run_steps(3);
    h.get_by_label("Apply to 4 items").click();
    h.run_steps(4);
    h.get_by_label("Done").click();
    h.run_steps(2);
    h.state_mut().restore_original();
    h.run_steps(2);
    assert_eq!(contents(&all), before);
}

#[test]
fn esc_at_the_last_step_asks_first() {
    let looks = looks("bulk-esc");
    let mut h = harness(&looks);
    to_check(&mut h);
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(matches!(h.state().dialog, Some(Dialog::CloseBulk)));
    h.get_by_label("Keep choosing").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(wizard(&h).step, BulkStep::Check);
    h.key_press(Key::Escape);
    h.run_steps(3);
    h.get_by_label("Close without applying").click();
    h.run_steps(3);
    assert!(h.state().bulk.is_none());
}

#[test]
fn the_russian_steps_fit_their_dialog_even_in_the_smallest_window() {
    let looks = looks("bulk-ru");
    let mut h = harness(&looks);
    h.state_mut()
        .set_language(tanki_i18n::Lang::from_code("ru").expect("Russian"));
    h.run_steps(3);
    let fits = |h: &Harness<'static, Studio>, labels: &[&str]| {
        let dialog = h
            .get_by_role_and_label(Role::Dialog, "Надеть образ на все предметы")
            .rect();
        for label in labels {
            let button = h.get_by_label(label).rect();
            assert!(
                dialog.contains_rect(button),
                "“{label}” ({button:?}) stays inside the dialog ({dialog:?})"
            );
        }
    };
    for size in [egui::vec2(1440.0, 900.0), egui::vec2(820.0, 520.0)] {
        h.set_size(size);
        h.state_mut().bulk = None;
        h.state_mut().go(Page::Garage);
        h.run_steps(3);
        h.state_mut().open_bulk();
        h.run_steps(3);
        fits(&h, &["Далее: куда", "Отмена"]);
        h.get_by_label("Далее: куда").click();
        h.run_steps(3);
        fits(&h, &["Проверить и надеть", "Назад", "Что получит M0"]);
        h.get_by_label("Проверить и надеть").click();
        h.run_steps(3);
        fits(&h, &["Надеть на 4 предмета", "Назад"]);
    }
}

/// The design's own sample (the e2e `design-*` scenes) gives the numbers its frames show.
#[test]
fn the_design_sample_shows_the_designs_numbers() {
    let work = scratch("bulk-design");
    let sample = tanki_testkit::build_design_sample(&work.join("design")).expect("sample");
    let data = work.join("data");
    let store = StateStore::open(&data.join("state.sqlite")).expect("state store");
    for (marker, kind, id, grade) in &sample.labels {
        store
            .save_user_target("air-cache", marker, kind, id, grade)
            .expect("label");
    }
    let mut installed = Vec::new();
    for pack in sample.packs.iter().rev() {
        let pack = import_pack(pack, &data.join("packs")).expect("import");
        store.save_pack(&pack).expect("save pack");
        installed.push(pack);
    }
    let scan = tanki_core::cache::scan_cache(&sample.cache, Some("air-cache")).expect("scan");
    for (pack_id, item, grade, k) in tanki_testkit::design_sample_worn() {
        let pack = installed
            .iter()
            .find(|p| p.manifest.id == pack_id)
            .expect("pack");
        let pack_item = pack
            .manifest
            .items
            .iter()
            .find(|i| i.target.id == item && i.target.grade == grade)
            .expect("skin");
        let target = tanki_core::pack::PackTarget {
            kind: pack_item.target.kind.clone(),
            id: item.into(),
            grade: format!("m{k}"),
        };
        let note = tanki_core::apply::ApplyNote {
            batch: None,
            skin_grade: Some(grade.into()),
        };
        tanki_core::apply::apply_variant_in(
            &scan,
            pack,
            &target,
            &pack_item.variants[0],
            &data.join("backups"),
            &note,
        )
        .expect("worn");
    }
    drop(store);
    let mut h = harness_in(data, vec![sample.cache.clone()]);
    h.state_mut().open_bulk();
    h.run_steps(2);
    let looks: Vec<(String, usize, usize, usize)> = wizard(&h)
        .looks
        .iter()
        .map(|l| {
            (
                l.name.clone(),
                l.items.len(),
                l.count("hull"),
                l.packs.len(),
            )
        })
        .collect();
    let expected = [
        ("XT", 14, 8, 2),
        ("Legacy", 12, 7, 1),
        ("Prime", 9, 5, 1),
        ("Neon", 6, 4, 1),
        ("Old", 5, 5, 1),
        ("Winter", 3, 2, 2),
    ];
    assert_eq!(looks.len(), expected.len());
    for ((name, items, hulls, packs), (n, i, hu, p)) in looks.iter().zip(expected) {
        assert_eq!((name.as_str(), *items, *hulls, *packs), (n, i, hu, p));
    }
    let app = h.state_mut();
    app.bulk_next();
    app.bulk_set_on(3, false);
    app.bulk_set_source(0, Source::From(3));
    app.bulk_set_pick("hull", "viking", 0, Some(Pick::Skin(Some(0))));
    let plan = wizard(&h).plan.clone().expect("plan");
    assert_eq!(
        (
            plan.rows.len(),
            plan.changing_items(),
            plan.skipped_items(),
            plan.changes()
        ),
        (14, 12, 2, 24)
    );
    let first: Vec<String> = plan.rows.iter().take(5).map(|r| r.name()).collect();
    assert_eq!(first, ["Wasp", "Hornet", "Viking", "Dictator", "Railgun"]);
    assert_eq!(cell(&h, "viking", 1), &Some(Cell::Already));
    assert_eq!(
        cell(&h, "hornet", 0),
        &Some(Cell::Missing { source: Some(3) })
    );
    assert_eq!(cell(&h, "dictator", 0), &Some(Cell::NotInGame));
}
