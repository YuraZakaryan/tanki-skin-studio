// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::Path;

use egui_kittest::kittest::Queryable;
use tanki_core::state::StateStore;
use tanki_studio::app::Page;
use tanki_testkit::scratch;

use crate::common::{env, harness};

fn write(dir: &Path, relative: &str, bytes: &[u8]) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

/// A tiny model archive in the layout of the public `tanki-model-archive`.
fn archive() -> std::path::PathBuf {
    let dir = scratch("model-archive");
    for style in ["Old", "New"] {
        for file in ["object.3ds", "lightmap.jpg", "details.png", "preview.png"] {
            write(
                &dir,
                &format!("Flash/Hulls/{style}/Wasp/M3/{file}"),
                file.as_bytes(),
            );
        }
    }
    write(&dir, "Flash/Turrets/Old/Railgun/M3/object.3ds", b"gun");
    dir
}

#[test]
fn choosing_a_model_archive_as_the_cache_builds_a_pack_and_keeps_the_real_cache() {
    let env = env("archive-as-cache");
    let mut h = harness(&env);
    let cache_before = h.state().scan.as_ref().map(|s| s.root.clone());
    h.state_mut().use_cache_folder(&archive());
    h.run_steps(4);
    assert_eq!(h.state().packs.len(), 1, "the archive became a pack");
    let pack = h
        .state()
        .active_pack
        .clone()
        .expect("and it is the active one");
    assert_eq!(pack.manifest.items.len(), 2, "Wasp M3 and Railgun M3");
    assert_eq!(
        h.state().page,
        Page::Packs,
        "the user is taken to the Packs page"
    );
    assert_eq!(
        h.state().scan.as_ref().map(|s| s.root.clone()),
        cache_before,
        "the game cache stays connected"
    );
    let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
    assert_eq!(
        store.get("custom_cache").unwrap(),
        None,
        "the archive is not remembered as a cache"
    );
    assert!(
        h.query_by_label("Undo").is_some(),
        "and the import can be undone"
    );
}

#[test]
fn an_ordinary_folder_is_refused_as_a_cache_with_an_explanation() {
    let env = env("plain-folder");
    let mut h = harness(&env);
    let folder = scratch("plain-folder-src");
    write(&folder, "notes.txt", b"hello");
    h.state_mut().use_cache_folder(&folder);
    h.run_steps(3);
    assert!(h.state().packs.is_empty());
    assert!(
        h.state()
            .toasts
            .iter()
            .any(|t| t.text.contains("not a game cache")),
        "{:?}",
        h.state().toasts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    assert_eq!(
        h.state().scan.as_ref().map(|s| s.root.clone()),
        Some(env.cache.root.clone())
    );
}

#[test]
fn a_folder_that_was_wrongly_stored_as_the_cache_is_not_offered_again() {
    let env = env("stale-custom");
    {
        let store = StateStore::open(&env.work.join("data/state.sqlite")).unwrap();
        store
            .set("custom_cache", &archive().to_string_lossy())
            .unwrap();
    }
    let h = harness(&env);
    assert_eq!(
        h.state().roots.len(),
        1,
        "only the real cache is offered: {:?}",
        h.state().roots
    );
    assert_eq!(
        h.state().scan.as_ref().map(|s| s.root.clone()),
        Some(env.cache.root.clone())
    );
}
