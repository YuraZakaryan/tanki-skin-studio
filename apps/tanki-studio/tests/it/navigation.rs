// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{Key, Modifiers};
use egui_kittest::kittest::Queryable;
use tanki_studio::app::Page;

use crate::common::{env, harness, harness_with_pack};

#[test]
fn the_sidebar_switches_pages() {
    let env = env("nav");
    let mut h = harness(&env);
    assert_eq!(h.state().page, Page::Overview);
    for (label, page) in [
        ("Garage", Page::Garage),
        ("Resources", Page::Resources),
        ("Packs", Page::Packs),
        ("Info", Page::Info),
        ("Overview", Page::Overview),
    ] {
        h.get_by_label(label).click();
        h.run_steps(3);
        assert_eq!(h.state().page, page, "after clicking {label}");
    }
}

#[test]
fn a_narrow_window_keeps_every_page_reachable_through_the_icon_rail() {
    let env = env("rail");
    let mut h = harness(&env);
    h.set_size(egui::vec2(900.0, 600.0));
    h.run_steps(3);
    // The rail has no text, but the buttons keep their names.
    h.get_by_label("Packs").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Packs);
    // And back to a wide window: the full sidebar is there again.
    h.set_size(egui::vec2(1440.0, 900.0));
    h.run_steps(3);
    h.get_by_label("Garage").click();
    h.run_steps(3);
    assert_eq!(h.state().page, Page::Garage);
}

#[test]
fn the_command_palette_finds_an_item_and_opens_it_in_the_garage() {
    let env = env("palette");
    let mut h = harness_with_pack(&env);
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run_steps(4);
    assert!(h.state().palette.is_some(), "Cmd+K opens the palette");
    h.state_mut().palette.as_mut().expect("palette").query = "wasp".into();
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(
        h.state().palette.is_none(),
        "choosing a result closes the palette"
    );
    assert_eq!(h.state().page, Page::Garage);
    let selection = h.state().selection.clone().expect("an item is selected");
    assert_eq!((selection.id.as_str(), selection.grade), ("wasp", 3));
}

#[test]
fn escape_closes_the_palette_without_doing_anything() {
    let env = env("palette-esc");
    let mut h = harness(&env);
    h.key_press_modifiers(Modifiers::COMMAND, Key::K);
    h.run_steps(3);
    assert!(h.state().palette.is_some());
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert!(h.state().palette.is_none());
    assert_eq!(h.state().page, Page::Overview);
}
