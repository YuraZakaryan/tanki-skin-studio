// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Backups and the pages around them: the Settings row and the Backups page, deleting old backups
//! (never the ones that take a skin off), the limit, a full disk (nothing is written without a
//! backup; backups of skins on now go only with consent), a skin a game update removed (the
//! Garage says so and offers it again), Details and Back, the About page, narrow windows and a
//! seeded random walk over all of it.

use std::path::PathBuf;

use egui_kittest::kittest::Queryable;
use tanki_core::backups::{Kept, SPARE, Usage, survey};
use tanki_studio::{
    app::{BackupsFilter, DeleteWhat, Dialog, Page, StateFilter, Studio, Worn},
    backups::LIMITS,
};

use crate::common::{Env, contents, env, harness, harness_with_pack};

type H = egui_kittest::Harness<'static, Studio>;

fn backups_root(h: &H) -> PathBuf {
    h.state().data.join("backups")
}

/// Pick the fixture's hull skin and put it on through the Garage, as a user does.
fn apply_wasp_red(h: &mut H) {
    h.state_mut().dialog = None;
    h.state_mut().garage_tab = "hull".into();
    h.state_mut().go(Page::Garage);
    h.run_steps(3);
    if h.query_by_label("Wasp Red").is_none() {
        h.get_by_label("Wasp (Hull)").click();
        h.run_steps(3);
    }
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    h.get_by_label("Apply to Wasp M3").click();
    h.run_steps(4);
    if matches!(h.state().dialog, Some(Dialog::Confirm(_))) {
        h.get_all_by_label("Apply")
            .last()
            .expect("the dialog's Apply")
            .click();
        h.run_steps(5);
    }
    if matches!(h.state().dialog, Some(Dialog::Done { .. })) {
        h.state_mut().dialog = None;
    }
    h.run_steps(2);
}

/// Take every skin off (the Garage's Restore, confirmed).
fn restore_all(h: &mut H) {
    h.state_mut().dialog = None;
    h.state_mut().ask_to_restore();
    h.run_steps(2);
    if matches!(h.state().dialog, Some(Dialog::RestoreAll { .. })) {
        h.state_mut().restore_original();
    }
    h.state_mut().dialog = None;
    h.run_steps(3);
}

/// The game downloads the item again: its files are new bytes, the skin is gone.
fn game_update(env: &Env, h: &mut H) {
    for file in &env.cache.hull_files {
        std::fs::write(file, b"game update").expect("rewrite a cache file");
    }
    h.state_mut().rescan();
    h.state_mut().reload_backups();
    h.run_steps(3);
}

fn kinds(h: &H) -> Vec<Kept> {
    survey(&backups_root(h))
        .expect("survey")
        .into_iter()
        .map(|s| s.kept)
        .collect()
}

fn needed_dirs(h: &H) -> Vec<PathBuf> {
    survey(&backups_root(h))
        .expect("survey")
        .into_iter()
        .filter(|s| !s.kept.is_old())
        .map(|s| s.entry.dir)
        .collect()
}

fn need(env: &Env) -> u64 {
    env.cache
        .hull_files
        .iter()
        .map(|f| std::fs::metadata(f).map_or(0, |m| m.len()))
        .sum()
}

#[test]
fn details_opens_the_item_page_and_back_returns_to_the_garage() {
    let env = env("details");
    let mut h = harness_with_pack(&env);
    h.get_by_label("Garage").click();
    h.run_steps(3);
    h.get_by_label("Wasp Red").click();
    h.run_steps(3);
    h.get_by_label("Details").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Info);
    h.get_by_label("Back to Garage").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Garage);
}

#[test]
fn the_settings_backups_row_opens_the_backups_page_and_back_returns() {
    let env = env("backups-row");
    let mut h = harness(&env);
    h.get_by_label("Settings").click();
    h.run_steps(3);
    h.get_by_label("Backups").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Backups);
    assert!(
        h.state().page.section() == Page::Settings,
        "Settings stays lit in the sidebar"
    );
    h.get_by_label("Back to Settings").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Settings);
}

#[test]
fn settings_switch_opens_the_same_cache_list_as_the_sidebar_card() {
    let env = env("settings-switch");
    let mut h = harness(&env);
    h.get_by_label("Settings").click();
    h.run_steps(3);
    h.get_by_label("Switch").click();
    h.run_steps(3);
    assert!(
        matches!(
            h.state().dialog,
            Some(Dialog::ChooseCache {
                switching: true,
                ..
            })
        ),
        "one cache or several, Switch opens the list, got {:?}",
        h.state().dialog
    );
}

#[test]
fn a_skin_a_game_update_removed_is_lost_in_the_garage_and_its_backup_is_old() {
    let env = env("lost");
    let mut h = harness_with_pack(&env);
    apply_wasp_red(&mut h);
    assert_eq!(kinds(&h), vec![Kept::Needed]);
    game_update(&env, &mut h);
    assert_eq!(kinds(&h), vec![Kept::Replaced]);
    let wasp = h
        .state()
        .items
        .iter()
        .find(|i| i.id == "wasp")
        .cloned()
        .expect("wasp");
    assert_eq!(h.state().item_state(&wasp), StateFilter::Lost);
    let selection = h.state().selection.clone().expect("a selection");
    assert!(
        h.state()
            .lost_skin(&selection.kind, &selection.id, selection.grade)
            .is_some(),
        "My tank says a game update removed it"
    );
    // The skin it lost is picked again: one click on the yellow Apply puts it back.
    assert!(h.state().lost_variant(&selection).is_some());
    assert_eq!(
        h.state().chosen_variant(&selection),
        h.state().lost_variant(&selection)
    );
    h.state_mut().go(Page::Garage);
    h.run_steps(3);
    assert!(h.query_by_label("Lost").is_some(), "the filter offers Lost");
    h.get_by_label("Lost").click();
    h.run_steps(3);
    assert_eq!(h.state().garage_state, StateFilter::Lost);
    // Deleting old backups takes it; the game's new files stay.
    h.state_mut().go(Page::Backups);
    h.run_steps(3);
    h.get_by_label("Delete old").click();
    h.run_steps(3);
    assert!(matches!(
        h.state().dialog,
        Some(Dialog::DeleteOld { backups: 1, .. })
    ));
    h.get_by_label("Delete old backups").click();
    h.run_steps(3);
    assert!(kinds(&h).is_empty());
    assert!(
        contents(&env.cache.hull_files)
            .iter()
            .all(|bytes| bytes == b"game update")
    );
    assert_eq!(h.state().item_state(&wasp), StateFilter::Original);
}

#[test]
fn deleting_old_backups_keeps_the_ones_that_take_skins_off_and_undo_still_works() {
    let env = env("delete-old");
    let originals = contents(&env.cache.hull_files);
    let mut h = harness_with_pack(&env);
    apply_wasp_red(&mut h);
    restore_all(&mut h);
    apply_wasp_red(&mut h);
    assert_eq!(kinds(&h), vec![Kept::Needed, Kept::TakenOff]);
    h.state_mut().go(Page::Backups);
    h.run_steps(3);
    h.get_by_label("Delete old").click();
    h.run_steps(3);
    h.get_by_label("Delete old backups").click();
    h.run_steps(3);
    assert_eq!(kinds(&h), vec![Kept::Needed]);
    assert_ne!(contents(&env.cache.hull_files), originals, "the skin is on");
    h.state_mut().toasts.clear();
    h.run_steps(2);
    h.get_by_label("Undo").click();
    h.run_steps(4);
    assert_eq!(
        contents(&env.cache.hull_files),
        originals,
        "Undo on the Backups page took it off"
    );
}

#[test]
fn the_limit_deletes_old_backups_keeps_needed_ones_and_is_kept_across_starts() {
    let env = env("limit");
    let mut h = harness_with_pack(&env);
    for _ in 0..3 {
        apply_wasp_red(&mut h);
        restore_all(&mut h);
    }
    apply_wasp_red(&mut h);
    assert_eq!(kinds(&h).iter().filter(|k| k.is_old()).count(), 3);
    h.state_mut().set_backup_limit(Some(1));
    h.run_steps(2);
    assert_eq!(
        kinds(&h),
        vec![Kept::Needed],
        "old ones go, the needed one stays"
    );
    drop(h);
    let h = harness(&env);
    assert_eq!(h.state().backup_limit, Some(1));
}

#[test]
fn a_full_disk_writes_nothing_without_a_backup_and_frees_old_backups_first() {
    let env = env("full-disk");
    let originals = contents(&env.cache.hull_files);
    let mut h = harness_with_pack(&env);
    // Nothing to delete and no room: the dialog says so and nothing is written.
    h.state_mut().free_space_override = Some(0);
    apply_wasp_red(&mut h);
    let Some(Dialog::LowSpace { plan, .. }) = h.state().dialog.clone() else {
        panic!("expected the low-space dialog, got {:?}", h.state().dialog);
    };
    assert!(!plan.enough && plan.old.is_empty() && plan.needed.is_empty());
    assert!(h.query_by_label("Delete and put on").is_none());
    assert_eq!(
        contents(&env.cache.hull_files),
        originals,
        "nothing written"
    );
    h.state_mut().dialog = None;

    // An old backup frees enough: only it goes, and the skin goes on.
    h.state_mut().free_space_override = None;
    apply_wasp_red(&mut h);
    restore_all(&mut h);
    let old = Usage::of(&survey(&backups_root(&h)).expect("survey")).old_bytes;
    assert!(old > 0);
    h.state_mut().free_space_override = Some(SPARE + need(&env) - old / 2);
    apply_wasp_red(&mut h);
    let Some(Dialog::LowSpace { plan, .. }) = h.state().dialog.clone() else {
        panic!("expected the low-space dialog, got {:?}", h.state().dialog);
    };
    assert!(plan.enough && plan.needed.is_empty() && plan.old.len() == 1);
    h.get_by_label("Delete and put on").click();
    h.run_steps(5);
    h.state_mut().dialog = None;
    assert_eq!(kinds(&h), vec![Kept::Needed]);
    assert_ne!(contents(&env.cache.hull_files), originals, "the skin is on");

    // Only a backup of a skin on now could make room: the dialog asks, Cancel deletes nothing.
    // The turret's skin goes on, the hull's comes off and its backup is deleted as old.
    restore_all(&mut h);
    h.state_mut().free_space_override = None;
    h.state_mut().delete_old_confirmed();
    h.state_mut().garage_tab = "turret".into();
    h.state_mut().go(Page::Garage);
    h.run_steps(3);
    h.get_by_label("Railgun (Turret)").click();
    h.run_steps(3);
    h.get_by_label("Railgun Blue").click();
    h.run_steps(3);
    h.state_mut().begin_apply_selected();
    h.run_steps(2);
    let Some(Dialog::Confirm(pending)) = h.state().dialog.clone() else {
        panic!("expected the confirmation, got {:?}", h.state().dialog);
    };
    h.state_mut().confirm_apply(pending);
    h.state_mut().dialog = None;
    h.run_steps(3);
    assert_eq!(kinds(&h), vec![Kept::Needed]);
    let needed = Usage::of(&survey(&backups_root(&h)).expect("survey")).needed_bytes;
    h.state_mut().free_space_override = Some(SPARE + need(&env) - needed / 2);
    h.state_mut().garage_tab = "hull".into();
    apply_wasp_red(&mut h);
    let Some(Dialog::LowSpace { plan, .. }) = h.state().dialog.clone() else {
        panic!("expected the low-space dialog, got {:?}", h.state().dialog);
    };
    assert!(plan.enough && plan.old.is_empty() && plan.needed.len() == 1);
    let dirs = needed_dirs(&h);
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().dialog.is_none());
    assert_eq!(needed_dirs(&h), dirs, "Cancel deletes nothing");
    assert_eq!(
        contents(&env.cache.hull_files),
        originals,
        "nothing written"
    );
}

#[test]
fn every_delete_asks_first_and_names_the_skins_that_lose_their_way_back() {
    let env = env("delete-asks");
    let originals = contents(&env.cache.hull_files);
    let mut h = harness_with_pack(&env);
    apply_wasp_red(&mut h);
    restore_all(&mut h);
    apply_wasp_red(&mut h);
    h.state_mut().toasts.clear();
    h.state_mut().go(Page::Backups);
    h.run_steps(3);
    // A line's trash (the old one) asks; Cancel keeps it.
    h.get_by_label("Delete").click();
    h.run_steps(3);
    let Some(Dialog::DeleteBackups { what, worn, .. }) = h.state().dialog.clone() else {
        panic!("expected the question, got {:?}", h.state().dialog);
    };
    assert!(matches!(what, DeleteWhat::One { old: true, .. }));
    assert!(worn.is_empty(), "an old backup takes no skin's way back");
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert_eq!(kinds(&h).len(), 2, "Cancel deleted nothing");
    // Delete day: the day holds the needed one too, so the question names its skin.
    h.get_by_label("Delete day").click();
    h.run_steps(3);
    let Some(Dialog::DeleteBackups { what, worn, .. }) = h.state().dialog.clone() else {
        panic!("expected the question, got {:?}", h.state().dialog);
    };
    assert_eq!(what, DeleteWhat::Day(tanki_studio::model::Day::Today));
    assert_eq!(worn, vec![Worn::Item("Wasp M3".into())]);
    h.get_all_by_label("Delete")
        .last()
        .expect("the dialog's button")
        .click();
    h.run_steps(3);
    assert!(kinds(&h).is_empty(), "the day's backups are gone");
    assert_ne!(
        contents(&env.cache.hull_files),
        originals,
        "deleting backups never touches the game: the skin stays on"
    );
    // Delete all asks too, and with nothing left the button is off.
    apply_wasp_red(&mut h);
    restore_all(&mut h);
    h.state_mut().go(Page::Backups);
    h.run_steps(3);
    h.get_by_label("Delete all").click();
    h.run_steps(3);
    assert!(matches!(
        h.state().dialog,
        Some(Dialog::DeleteBackups {
            what: DeleteWhat::All,
            ..
        })
    ));
    h.get_all_by_label("Delete all")
        .last()
        .expect("the dialog's button")
        .click();
    h.run_steps(3);
    assert!(kinds(&h).is_empty());
}

#[test]
fn about_lists_the_releases_and_opens_one_at_a_time() {
    let env = env("about");
    let mut h = harness(&env);
    h.get_by_label("About").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::About);
    assert_eq!(
        h.state().about_open.as_deref(),
        Some(env!("CARGO_PKG_VERSION"))
    );
    for label in [
        "Telegram channel",
        "Telegram chat",
        "YouTube",
        "Source code",
    ] {
        assert!(h.query_by_label(label).is_some(), "{label}");
    }
    // Each community row hands its link to the system to open in the browser.
    for (label, url) in [
        ("Telegram channel", tanki_studio::about::TELEGRAM_CHANNEL),
        ("Telegram chat", tanki_studio::about::TELEGRAM_CHAT),
        ("YouTube", tanki_studio::about::YOUTUBE),
    ] {
        h.get_by_label(label).click();
        h.step();
        assert_eq!(
            h.output()
                .platform_output
                .commands
                .iter()
                .find_map(|c| match c {
                    egui::OutputCommand::OpenUrl(open) => Some(open.url.as_str()),
                    _ => None,
                }),
            Some(url),
            "{label} opens its link"
        );
        h.run_steps(2);
    }
    h.get_by_label("0.4.0").click();
    h.run_steps(3);
    assert_eq!(h.state().about_open.as_deref(), Some("0.4.0"));
    h.get_by_label("0.4.0").click();
    h.run_steps(3);
    assert_eq!(h.state().about_open, None);
    // Check for updates says it is up to date (tests never reach GitHub).
    h.get_by_label("Check for updates").click();
    h.run_steps(6);
    assert_eq!(
        h.state().updates.latest,
        Some(tanki_studio::updates::Latest::UpToDate)
    );
}

#[test]
fn the_new_pages_hold_together_in_narrow_and_wide_windows() {
    let env = env("pages-widths");
    let mut h = harness_with_pack(&env);
    apply_wasp_red(&mut h);
    restore_all(&mut h);
    apply_wasp_red(&mut h);
    for width in [420.0, 640.0, 900.0, 1200.0, 1920.0] {
        h.set_size(egui::vec2(width, 820.0));
        for page in [
            Page::Settings,
            Page::Backups,
            Page::About,
            Page::Info,
            Page::Garage,
        ] {
            h.state_mut().go(page);
            h.run_steps(3);
            assert_eq!(h.state().page, page);
        }
        h.state_mut().go(Page::Backups);
        h.run_steps(3);
        assert!(
            h.query_by_label("Back to Settings").is_some(),
            "Back stays at {width}"
        );
        assert!(h.query_by_label("Delete old").is_some(), "at {width}");
        h.state_mut().go(Page::Settings);
        h.run_steps(3);
        assert!(h.query_by_label("Switch").is_some(), "at {width}");
        assert!(h.query_by_label("Rescan now").is_some(), "at {width}");
    }
}

/// Answer an open delete question at random. Returns whether a backup of a skin on now was
/// agreed to go.
fn answer_delete(h: &mut H, rng: &mut Rng) -> bool {
    let Some(Dialog::DeleteBackups { dirs, worn, .. }) = h.state().dialog.clone() else {
        return false;
    };
    if rng.below(2) == 0 {
        h.state_mut().delete_backups_confirmed(&dirs);
        !worn.is_empty()
    } else {
        h.state_mut().dialog = None;
        false
    }
}

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// Random use of everything added around backups: pages, filters, the limit, applying and
/// taking off, game updates, a full disk, deleting, undoing. Whatever the order: no panic, the
/// numbers shown are the files on disk, the limit holds, and a backup of a skin on now is never
/// deleted unless the user agreed in the low-space dialog. `TANKI_PAGES_MONKEYS` sets the seeds.
#[test]
fn random_use_of_backups_settings_about_and_the_garage_states_never_breaks() {
    let seeds: u64 = std::env::var("TANKI_PAGES_MONKEYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    for seed in 0..seeds {
        walk(seed, 160);
    }
}

fn walk(seed: u64, steps: usize) {
    let env = env(&format!("pages-walk-{seed}"));
    let mut h = harness_with_pack(&env);
    h.state_mut().toasts.clear();
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ (seed + 1).wrapping_mul(0x2545_F491_4F6C_DD1D));
    let mut applied = 0;
    for step in 0..steps {
        let needed_before = needed_dirs(&h);
        let mut consented = false;
        match rng.below(16) {
            0 => {
                let page = Page::ALL[rng.below(Page::ALL.len())];
                h.state_mut().go(page);
            }
            1 => h.state_mut().garage_state = StateFilter::ALL[rng.below(5)],
            2 => h.state_mut().backups_filter = BackupsFilter::ALL[rng.below(3)],
            3 => h.state_mut().zoom(if rng.below(2) == 0 { 1 } else { -1 }),
            4 => {
                apply_wasp_red(&mut h);
                applied += 1;
            }
            5 => restore_all(&mut h),
            6 => game_update(&env, &mut h),
            7 => {
                let limit = LIMITS[rng.below(LIMITS.len())].map(|l| l >> rng.below(31));
                h.state_mut().set_backup_limit(limit);
                let usage = Usage::of(&survey(&backups_root(&h)).expect("survey"));
                if let Some(limit) = limit {
                    assert!(
                        usage.total() <= limit || usage.old == 0,
                        "seed {seed} step {step}: over the limit with old backups left"
                    );
                }
            }
            8 => {
                h.state_mut().ask_to_delete_old();
                h.run_steps(2);
                if matches!(h.state().dialog, Some(Dialog::DeleteOld { .. })) {
                    h.state_mut().delete_old_confirmed();
                }
            }
            9 => {
                h.state_mut().free_space_override = match rng.below(3) {
                    0 => None,
                    1 => Some(rng.below(1 << 20) as u64),
                    _ => Some(SPARE + rng.below(1 << 16) as u64),
                };
            }
            10 => match h.state().dialog.clone() {
                Some(Dialog::LowSpace { retry, plan, .. }) if plan.enough && rng.below(2) == 0 => {
                    consented = !plan.needed.is_empty();
                    h.state_mut().make_room_and_retry(retry, plan);
                }
                _ => h.state_mut().dialog = None,
            },
            15 => {
                h.state_mut().dialog = None;
                h.state_mut().ask_to_delete_all();
                consented = answer_delete(&mut h, &mut rng);
            }
            11 => {
                h.set_size(egui::vec2(
                    400.0 + rng.below(1600) as f32,
                    600.0 + rng.below(500) as f32,
                ));
            }
            12 => {
                let releases = tanki_studio::changelog::releases(h.state().lang);
                let pick = releases[rng.below(releases.len())].version.clone();
                h.state_mut().about_open = Some(pick);
            }
            _ => {
                h.state_mut().dialog = None;
                h.state_mut().go(Page::Backups);
                h.run_steps(2);
                let lines = tanki_studio::backups::lines(&h.state().stored, |id| id.to_owned());
                if !lines.is_empty() {
                    let line = lines[rng.below(lines.len())].clone();
                    if line.kept.is_old() || rng.below(4) == 0 {
                        // Every delete asks first; the answer is random.
                        let what = DeleteWhat::One {
                            title: line.title.clone(),
                            old: line.kept.is_old(),
                        };
                        h.state_mut()
                            .ask_to_delete(what, std::slice::from_ref(&line));
                        consented = answer_delete(&mut h, &mut rng);
                    } else {
                        h.state_mut().undo_line(&line);
                        if let Some(Dialog::RestoreBatch { batch, name, .. }) =
                            h.state().dialog.clone()
                        {
                            h.state_mut().restore_batch_confirmed(&batch, &name);
                        }
                    }
                }
            }
        }
        h.run_steps(2);
        if matches!(h.state().dialog, Some(Dialog::Done { .. })) {
            h.state_mut().dialog = None;
        }
        // The numbers on screen are the files on disk.
        h.state_mut().reload_backups();
        let on_disk = Usage::of(&survey(&backups_root(&h)).expect("survey"));
        assert_eq!(h.state().usage, on_disk, "seed {seed} step {step}");
        // A backup of a skin on now is only ever deleted with consent.
        let gone = needed_before.iter().filter(|dir| !dir.exists()).count();
        assert!(
            gone == 0 || consented,
            "seed {seed} step {step}: {gone} needed backup(s) went without consent"
        );
    }
    assert!(applied > 0, "seed {seed}: the walk applied something");
}
