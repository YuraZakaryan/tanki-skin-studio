// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Several game clients on one machine, each with its own cache: the app asks which one to use
//! on the first start and when a new one appears, and keeps the answer.

use std::path::PathBuf;

use egui_kittest::{Harness, kittest::Queryable};
use tanki_studio::app::{Dialog, Studio};
use tanki_testkit::build_cache;

use crate::common::{Env, env, harness_with_caches};

/// A cache where an AIR application keeps it, so it is named by the application's id.
fn client_cache(env: &Env, id: &str) -> PathBuf {
    build_cache(&env.work.join(id).join("Local Store").join("cache"))
        .expect("cache fixture")
        .root
}

fn question(h: &Harness<'static, Studio>) -> Option<Vec<(String, bool, bool)>> {
    match &h.state().dialog {
        Some(Dialog::ChooseCache { choices, .. }) => Some(
            choices
                .iter()
                .map(|c| (c.name.clone(), c.new, c.current))
                .collect(),
        ),
        _ => None,
    }
}

fn open_cache(h: &Harness<'static, Studio>) -> Option<PathBuf> {
    h.state().scan.as_ref().map(|s| s.root.clone())
}

#[test]
fn several_caches_are_asked_about_on_the_first_start_and_the_choice_is_kept() {
    let env = env("caches-first");
    let first = client_cache(&env, "org.example.first");
    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first.clone(), second.clone()]);
    assert_eq!(
        question(&h),
        Some(vec![
            ("org.example.first".to_owned(), false, true),
            ("org.example.second".to_owned(), false, false),
        ]),
        "both caches are offered, the open one marked, none called new on the first start"
    );

    h.get_by_label("org.example.second").click();
    h.run_steps(2);
    h.get_by_label("Use this cache").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(second.clone()));
    drop(h);

    let h = harness_with_caches(&env, vec![first, second.clone()]);
    assert!(question(&h).is_none(), "the answer is not asked for again");
    assert_eq!(open_cache(&h), Some(second), "the chosen cache opens");
}

#[test]
fn closing_the_question_keeps_the_open_cache_and_does_not_ask_again() {
    let env = env("caches-keep");
    let first = client_cache(&env, "org.example.first");
    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first.clone(), second.clone()]);
    assert!(question(&h).is_some());
    h.get_by_label("Keep current").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(first.clone()));
    drop(h);

    let h = harness_with_caches(&env, vec![first.clone(), second]);
    assert!(question(&h).is_none());
    assert_eq!(open_cache(&h), Some(first));
}

#[test]
fn a_cache_that_appears_after_a_start_is_asked_about_and_marked_new() {
    let env = env("caches-new");
    let first = client_cache(&env, "org.example.first");
    let h = harness_with_caches(&env, vec![first.clone()]);
    assert!(question(&h).is_none(), "one cache leaves nothing to choose");
    drop(h);

    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first.clone(), second.clone()]);
    assert_eq!(
        question(&h),
        Some(vec![
            ("org.example.first".to_owned(), false, true),
            ("org.example.second".to_owned(), true, false),
        ]),
        "the new client's cache is offered and marked new"
    );
    h.get_by_label("org.example.second").click();
    h.run_steps(2);
    h.get_by_label("Use this cache").click();
    h.run_steps(4);
    assert_eq!(open_cache(&h), Some(second.clone()));
    drop(h);

    let h = harness_with_caches(&env, vec![first, second.clone()]);
    assert!(question(&h).is_none());
    assert_eq!(open_cache(&h), Some(second));
}

#[test]
fn a_cache_found_by_a_rescan_while_running_is_asked_about() {
    let env = env("caches-rescan");
    let first = client_cache(&env, "org.example.first");
    let mut h = harness_with_caches(&env, vec![first.clone()]);
    assert!(question(&h).is_none());

    let second = client_cache(&env, "org.example.second");
    h.state_mut().roots_override = Some(vec![
        ("air-cache".into(), first.clone()),
        ("air-cache".into(), second),
    ]);
    h.state_mut().rescan();
    h.run_steps(2);
    assert_eq!(
        question(&h).map(|q| q.into_iter().map(|(_, new, _)| new).collect::<Vec<_>>()),
        Some(vec![false, true])
    );
    // Closing with Escape is an answer too: the open cache stays.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(first));
    h.state_mut().rescan();
    h.run_steps(2);
    assert!(
        question(&h).is_none(),
        "a cache asked about once is not asked about again"
    );
}

fn switching(h: &Harness<'static, Studio>) -> bool {
    matches!(
        h.state().dialog,
        Some(Dialog::ChooseCache {
            switching: true,
            ..
        })
    )
}

#[test]
fn the_cache_card_opens_the_switcher_and_switches_the_cache() {
    let env = env("caches-card");
    let first = client_cache(&env, "org.example.first");
    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first, second.clone()]);
    h.get_by_label("Keep current").click();
    h.run_steps(4);
    h.get_by_label("Switch game cache").click();
    h.run_steps(2);
    assert!(switching(&h), "the card opens the switcher, not a menu");
    assert_eq!(
        question(&h),
        Some(vec![
            ("org.example.first".to_owned(), false, true),
            ("org.example.second".to_owned(), false, false),
        ])
    );
    h.get_by_label("Choose folder…");
    h.get_by_label("org.example.second").click();
    h.run_steps(2);
    h.get_by_label("Use this cache").click();
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(second.clone()));
    drop(h);

    let h = harness_with_caches(
        &env,
        vec![client_cache(&env, "org.example.first"), second.clone()],
    );
    assert_eq!(open_cache(&h), Some(second), "the switch is kept");
}

#[test]
fn cancel_and_escape_leave_the_switcher_without_switching() {
    let env = env("caches-cancel");
    let first = client_cache(&env, "org.example.first");
    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first.clone(), second]);
    h.get_by_label("Keep current").click();
    h.run_steps(4);
    for close in [Some("Cancel"), None] {
        h.state_mut().open_cache_switcher();
        h.run_steps(2);
        h.get_by_label("org.example.second").click();
        h.run_steps(2);
        match close {
            Some(label) => h.get_by_label(label).click(),
            None => h.key_press(egui::Key::Escape),
        }
        h.run_steps(2);
        assert!(h.state().dialog.is_none());
        assert_eq!(open_cache(&h), Some(first.clone()));
    }
}

#[test]
fn the_arrow_keys_and_enter_switch_from_the_keyboard() {
    let env = env("caches-keys");
    let first = client_cache(&env, "org.example.first");
    let second = client_cache(&env, "org.example.second");
    let mut h = harness_with_caches(&env, vec![first, second.clone()]);
    h.get_by_label("Keep current").click();
    h.run_steps(4);
    h.state_mut().open_cache_switcher();
    h.run_steps(2);
    h.key_press(egui::Key::ArrowDown);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(4);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(second));
}

#[test]
fn the_switcher_lists_a_single_cache_too() {
    let env = env("caches-single");
    let only = client_cache(&env, "org.example.only");
    let mut h = harness_with_caches(&env, vec![only.clone()]);
    assert!(
        question(&h).is_none(),
        "one cache is not asked about at start"
    );
    h.get_by_label("Switch game cache").click();
    h.run_steps(2);
    assert!(switching(&h));
    assert_eq!(
        question(&h),
        Some(vec![("org.example.only".to_owned(), false, true)])
    );
    h.get_by_label("Use this cache").click();
    h.run_steps(2);
    assert!(h.state().dialog.is_none());
    assert_eq!(open_cache(&h), Some(only));
}
