// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::{fs, path::Path};

use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha256};
use tanki_core::state::StateStore;
use tanki_testkit::scratch;

use crate::common::{env, harness};

fn cache_file(root: &Path, marker: &str, role: &str, bytes: &[u8]) {
    let url = format!("http://cache.test/{marker}/1/{role}");
    fs::write(root.join(STANDARD.encode(url)), bytes).unwrap();
}

/// A pack that ships the model and the lightmap of Isida M3 (the manifest is written by hand).
fn isida_pack(model: &[u8]) -> std::path::PathBuf {
    let dir = scratch("isida-pack");
    fs::write(dir.join("isida.3ds"), model).unwrap();
    fs::write(dir.join("isida.jpg"), b"isida-light-3").unwrap();
    let manifest = format!(
        r#"{{"format_version":1,"id":"isida-pack","name":"Isida","version":"1","games":["air-cache"],
        "items":[{{"target":{{"kind":"turret","id":"isida","grade":"m3"}},
        "variants":[{{"id":"a","name":"Isida A","resources":[{{"role":"model","path":"isida.3ds","sha256":"{:x}"}},{{"role":"lightmap","path":"isida.jpg","sha256":"{:x}"}}]}}]}}]}}"#,
        Sha256::digest(model),
        Sha256::digest(b"isida-light-3")
    );
    fs::write(dir.join("manifest.json"), manifest).unwrap();
    dir
}

#[test]
fn importing_a_pack_recognises_the_items_whose_files_it_ships_with_no_labelling() {
    let env = env("recognise");
    // Four consecutive ids with the same model: an item's four modifications, new to the app. The
    // last one has the pack's lightmap of M3, which anchors the order of the others.
    for marker in ["0/0/7/40", "0/0/7/42", "0/0/7/44", "0/0/7/46"] {
        cache_file(&env.cache.root, marker, "object.3ds", b"isida-model");
    }
    cache_file(
        &env.cache.root,
        "0/0/7/46",
        "lightmap.jpg",
        b"isida-light-3",
    );
    let mut h = harness(&env);
    assert!(
        h.state().items.iter().all(|i| i.id != "isida"),
        "unknown before any pack"
    );
    h.state_mut().import_path(&isida_pack(b"isida-model"));
    h.run_steps(4);
    let item = h
        .state()
        .items
        .iter()
        .find(|i| i.id == "isida")
        .expect("isida is recognised after the import");
    assert_eq!(
        item.present, [true; 4],
        "all four modifications were found, by their order"
    );
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("4 resources")),
        "{:?}",
        h.state().toasts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    // The labels are stored: a restart has them without looking at the files again.
    let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
    assert_eq!(store.load_user_targets().unwrap(), 4);
    drop(h);
    assert!(harness(&env).state().items.iter().any(|i| i.id == "isida"));
}

#[test]
fn files_no_pack_ships_stay_unrecognised() {
    let env = env("recognise-none");
    for marker in ["0/0/7/40", "0/0/7/42", "0/0/7/44", "0/0/7/46"] {
        cache_file(&env.cache.root, marker, "object.3ds", b"some-other-model");
    }
    let mut h = harness(&env);
    h.state_mut().import_path(&isida_pack(b"isida-model"));
    h.run_steps(4);
    assert!(h.state().items.iter().all(|i| i.id != "isida"));
}

#[test]
fn an_answer_for_a_cache_switched_away_from_is_dropped_and_asked_again() {
    let env = crate::common::env("stale-search");
    let mut h = crate::common::harness_with_pack(&env);
    let other = tanki_testkit::build_cache(&env.work.join("other-cache")).expect("second cache");
    h.state_mut().background_recognition = true;
    h.state_mut().recognition = None;
    h.state_mut().auto_recognise(false);
    let first = h
        .state()
        .recognition_root
        .clone()
        .expect("the search started");
    // The user picks another cache before the answer is in.
    h.state_mut().use_cache_folder(&other.root);
    let mut asked_again = false;
    for _ in 0..500 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.state_mut().finish_recognition();
        if h.state()
            .recognition_root
            .as_ref()
            .is_some_and(|r| *r != first)
        {
            asked_again = true;
            break;
        }
        if h.state().recognition.is_none() {
            break;
        }
    }
    assert!(
        asked_again,
        "the answer for the old cache was dropped and the new cache searched"
    );
}
