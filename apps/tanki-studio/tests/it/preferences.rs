// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use tanki_core::state::StateStore;
use tanki_i18n::Lang;
use tanki_studio::app::Page;
use tanki_ui::theme::{ThemeKind, ThemePreference};

use crate::common::{env, harness};

#[test]
fn english_is_the_default_and_the_language_choice_survives_a_restart() {
    let env = env("language");
    let mut h = harness(&env);
    assert_eq!(h.state().lang, Lang::EN, "a fresh install is English");
    h.get_by_label("Русский").click();
    h.run_steps(4);
    assert_eq!(h.state().lang.code(), "ru");
    // The interface is now Russian: the Garage entry has its Russian name.
    assert!(
        h.query_by_label("Гараж").is_some(),
        "the sidebar is translated"
    );
    assert!(h.query_by_label("Garage").is_none());
    let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
    assert_eq!(store.language().unwrap().as_deref(), Some("ru"));
    drop(h);

    // Restart: the choice is back without anyone asking for it.
    let mut again = harness(&env);
    assert_eq!(again.state().lang.code(), "ru");
    assert!(again.query_by_label("Гараж").is_some());
    again.get_by_label("English").click();
    again.run_steps(4);
    assert_eq!(again.state().lang, Lang::EN);
    assert!(again.query_by_label("Garage").is_some());
}

#[test]
fn the_theme_follows_the_system_until_one_is_chosen_and_the_choice_survives_a_restart() {
    let env = env("theme");
    let mut h = harness(&env);
    assert_eq!(
        h.state().theme_pref,
        ThemePreference::System,
        "a fresh install follows the system"
    );
    assert_eq!(
        h.state().theme,
        ThemeKind::Dark,
        "dark when the system does not say"
    );
    h.get_by_label("Light").click();
    h.run_steps(4);
    assert_eq!(
        (h.state().theme_pref, h.state().theme),
        (ThemePreference::Light, ThemeKind::Light)
    );
    drop(h);
    let mut again = harness(&env);
    assert_eq!(
        again.state().theme_pref,
        ThemePreference::Light,
        "the choice is remembered"
    );
    again.get_by_label("System").click();
    again.run_steps(4);
    assert_eq!(again.state().theme_pref, ThemePreference::System);
    let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
    assert_eq!(store.theme().unwrap().as_deref(), Some("system"));
}

#[test]
fn a_theme_stored_by_an_older_version_still_loads() {
    let env = env("theme-old");
    {
        let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
        store.set_theme("light").unwrap();
    }
    let h = harness(&env);
    assert_eq!(h.state().theme_pref, ThemePreference::Light);
}

#[test]
fn the_interface_size_steps_with_the_zoom_shortcuts_and_is_remembered() {
    let env = env("zoom");
    let mut h = harness(&env);
    assert_eq!(h.state().ui_scale, 1.0);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Equals);
    h.run_steps(3);
    assert_eq!(h.state().ui_scale, 1.1);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Minus);
    h.run_steps(3);
    assert_eq!(h.state().ui_scale, 0.9);
    let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
    assert_eq!(store.ui_scale().unwrap().as_deref(), Some("90"));
    drop(h);
    assert_eq!(
        harness(&env).state().ui_scale,
        0.9,
        "restored after a restart"
    );
    let mut again = harness(&env);
    again.key_press_modifiers(Modifiers::COMMAND, Key::Num0);
    again.run_steps(3);
    assert_eq!(again.state().ui_scale, 1.0, "Cmd+0 is the actual size");
}

#[test]
fn the_page_and_the_selected_item_come_back_after_a_restart() {
    let env = env("session");
    let mut h = harness(&env);
    h.get_by_label("Packs").click();
    h.run_steps(3);
    h.state_mut().persist(true);
    drop(h);
    let again = harness(&env);
    assert_eq!(again.state().page, Page::Packs);
    let selection = again.state().selection.clone().expect("the default item");
    assert_eq!(selection.id, "wasp");
}

#[test]
fn a_session_pointing_at_things_that_no_longer_exist_is_repaired() {
    let env = env("session-stale");
    {
        let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
        store
            .set(
                "ui.session.v1",
                r#"{"page":"a-page-from-the-future","garage_tab":"submarine","caches":[{"root":"/gone","item":{"kind":"hull","id":"nobody"}}]}"#,
            )
            .unwrap();
    }
    let h = harness(&env);
    assert_eq!(
        h.state().page,
        Page::Overview,
        "an unknown page falls back to Overview"
    );
    assert_eq!(h.state().garage_tab, "hull");
    assert!(
        h.state().selection.is_some(),
        "and an item that is gone is replaced by one that exists"
    );
}
