// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Opt-in tests against a REAL game cache and a real pack, for the person who owns both. They are
//! ignored by default and need
//!
//! - `TANKI_REAL_CACHE`: the game's cache folder (it is written to and put back byte for byte),
//! - `TANKI_REAL_PACK`: a pack folder (for example the one the app imported),
//! - `TANKI_REAL_SAFETY`: an untouched copy of the cache, used to repair it if a test dies midway.
//!
//! ```text
//! TANKI_REAL_CACHE=... TANKI_REAL_PACK=... TANKI_REAL_SAFETY=... \
//!   cargo test -p tanki-studio --test real -- --ignored --test-threads=1 --nocapture
//! ```
//!
//! Every test ends by proving that the cache is identical to how it started.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use egui_kittest::{Harness, kittest::Queryable};
use sha2::{Digest, Sha256};
use tanki_core::{
    apply::{apply_variant, latest_transaction, restore_transaction},
    cache::{CacheScan, known_target_group, scan_cache},
    pack::{InstalledPack, load_pack, read_pack_resource},
    session::SessionState,
    state::StateStore,
};
use tanki_studio::app::{Dialog, Page, Startup, Studio};
use tanki_testkit::scratch;

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

type Tree = BTreeMap<String, String>;

fn tree(root: &Path) -> Tree {
    fn walk(dir: &Path, root: &Path, out: &mut Tree) {
        for entry in fs::read_dir(dir).expect("read dir").flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = fs::read(&path) {
                out.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    format!("{:x}", Sha256::digest(bytes)),
                );
            }
        }
    }
    let mut out = Tree::new();
    walk(root, root, &mut out);
    out
}

/// Puts the cache back from the safety copy if a test dies with the cache changed.
struct Guard {
    live: PathBuf,
    safety: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let safety = tree(&self.safety);
        let live = tree(&self.live);
        let mut repaired = 0;
        for (name, hash) in &safety {
            if live.get(name) != Some(hash) {
                let _ = fs::copy(self.safety.join(name), self.live.join(name));
                repaired += 1;
            }
        }
        if repaired > 0 {
            eprintln!("GUARD: repaired {repaired} file(s) from the safety copy");
        }
    }
}

struct Real {
    cache: PathBuf,
    pack: InstalledPack,
    before: Tree,
    _guard: Guard,
}

fn real() -> Real {
    let cache = env_path("TANKI_REAL_CACHE");
    let safety = env_path("TANKI_REAL_SAFETY");
    let pack_path = env_path("TANKI_REAL_PACK");
    let manifest = load_pack(&pack_path).expect("the real pack validates");
    let before = tree(&cache);
    assert_eq!(
        before,
        tree(&safety),
        "the safety copy must be identical before the test starts"
    );
    Real {
        pack: InstalledPack {
            path: pack_path,
            manifest,
        },
        _guard: Guard {
            live: cache.clone(),
            safety,
        },
        cache,
        before,
    }
}

fn scan(real: &Real) -> CacheScan {
    scan_cache(&real.cache, Some("air-cache")).expect("scan the real cache")
}

fn changed(before: &Tree, after: &Tree) -> Vec<String> {
    let mut names: Vec<String> = after
        .iter()
        .filter(|(name, hash)| before.get(*name) != Some(*hash))
        .map(|(name, _)| name.clone())
        .chain(before.keys().filter(|n| !after.contains_key(*n)).cloned())
        .collect();
    names.sort();
    names
}

#[test]
#[ignore = "needs a real cache, see the file header"]
fn the_real_cache_is_scanned_and_its_originals_are_compared_with_the_archive() {
    let real = real();
    let scan = scan(&real);
    let recognised: Vec<_> = scan
        .resources
        .iter()
        .filter_map(|r| {
            known_target_group(&scan.game, &r.group).map(|t| (t, r.role.clone(), r.size))
        })
        .collect();
    eprintln!(
        "files: {}, recognised files: {}",
        scan.resources.len(),
        recognised.len()
    );
    for target in [("hull", "wasp", "m3"), ("turret", "railgun", "m3")] {
        let files: Vec<_> = recognised.iter().filter(|(t, _, _)| *t == target).collect();
        assert!(
            !files.is_empty(),
            "{target:?} must be recognised in the real cache"
        );
        let mut roles: Vec<&str> = files.iter().map(|(_, role, _)| role.as_str()).collect();
        roles.sort_unstable();
        eprintln!("{target:?}: {} files, roles {roles:?}", files.len());
    }
    // Which archive style do the cache's current originals match, byte for byte?
    for item in &real.pack.manifest.items {
        let target = (
            item.target.kind.as_str(),
            item.target.id.as_str(),
            item.target.grade.as_str(),
        );
        if !recognised.iter().any(|(t, _, _)| *t == target) {
            continue;
        }
        for variant in &item.variants {
            for resource in &variant.resources {
                let bytes = read_pack_resource(&real.pack, &resource.path).unwrap();
                let hash = format!("{:x}", Sha256::digest(&bytes));
                let cache_role = match resource.role.as_str() {
                    "model" => "object.3ds",
                    "lightmap" => "lightmap.jpg",
                    _ => "details.jpg",
                };
                let same = scan
                    .resources
                    .iter()
                    .filter(|r| {
                        known_target_group(&scan.game, &r.group) == Some(target)
                            && r.role == cache_role
                    })
                    .filter(|r| {
                        fs::read(&r.path).is_ok_and(|b| format!("{:x}", Sha256::digest(b)) == hash)
                    })
                    .count();
                eprintln!(
                    "{target:?} {:<5} {:<9} identical to cache originals: {same}",
                    variant.id, resource.role
                );
            }
        }
    }
    assert_eq!(
        tree(&real.cache),
        real.before,
        "scanning must not change anything"
    );
}

fn decoded(path: &Path) -> image::DynamicImage {
    image::load_from_memory(&fs::read(path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
#[ignore = "needs a real cache, see the file header"]
fn every_recognised_variant_applies_exactly_and_restores_byte_for_byte() {
    let real = real();
    let scan = scan(&real);
    let backups = scratch("real-backups");
    let mut applied = 0;
    for item in &real.pack.manifest.items {
        let target = (
            item.target.kind.as_str(),
            item.target.id.as_str(),
            item.target.grade.as_str(),
        );
        let target_files: Vec<_> = scan
            .resources
            .iter()
            .filter(|r| known_target_group(&scan.game, &r.group) == Some(target))
            .collect();
        for variant in &item.variants {
            if target_files.is_empty() {
                // Not in this cache: the apply is refused and nothing at all changes.
                let error =
                    apply_variant(&scan, &real.pack, &item.target, variant, &backups).unwrap_err();
                assert!(error.to_string().contains("not recognized"), "{error}");
                assert_eq!(
                    tree(&real.cache),
                    real.before,
                    "a refused apply changed something"
                );
                continue;
            }
            let before = tree(&real.cache);
            let tx = apply_variant(&scan, &real.pack, &item.target, variant, &backups)
                .unwrap_or_else(|e| panic!("{target:?} {}: {e:#}", variant.id));
            let after = tree(&real.cache);
            let touched = changed(&before, &after);
            let expected: Vec<String> = {
                let mut names: Vec<String> = tx
                    .changes
                    .iter()
                    .map(|c| {
                        c.target
                            .strip_prefix(&real.cache)
                            .unwrap()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect();
                names.sort();
                names
            };
            assert_eq!(
                touched, expected,
                "{target:?} {}: only the transaction's files may change",
                variant.id
            );
            assert!(
                touched
                    .iter()
                    .all(|name| target_files.iter().any(|r| r.path.ends_with(name))),
                "a file outside the target changed"
            );
            // The contents are what the pack ships.
            for resource in &variant.resources {
                let bytes = read_pack_resource(&real.pack, &resource.path).unwrap();
                match resource.role.as_str() {
                    "model" => {
                        for r in target_files.iter().filter(|r| r.role == "object.3ds") {
                            assert_eq!(fs::read(&r.path).unwrap(), bytes, "model bytes");
                        }
                    }
                    "lightmap" => {
                        for r in target_files.iter().filter(|r| r.role == "lightmap.jpg") {
                            assert_eq!(fs::read(&r.path).unwrap(), bytes, "lightmap bytes");
                        }
                    }
                    "details" => {
                        let source = image::load_from_memory(&bytes).unwrap();
                        for r in target_files
                            .iter()
                            .filter(|r| r.role == "details.jpg" || r.role == "details_alpha.jpg")
                        {
                            let written = decoded(&r.path);
                            assert_eq!(
                                (written.width(), written.height()),
                                (source.width(), source.height()),
                                "{}",
                                r.role
                            );
                        }
                    }
                    other => panic!("unexpected role {other}"),
                }
            }
            eprintln!(
                "{target:?} {:<4} applied to {} files, contents verified",
                variant.id,
                expected.len()
            );
            applied += 1;

            let (dir, latest) = latest_transaction(&backups)
                .unwrap()
                .expect("a backup exists");
            assert_eq!(latest.id, tx.id);
            restore_transaction(&dir, &latest).unwrap();
            assert_eq!(
                tree(&real.cache),
                before,
                "{target:?} {}: restore must be exact",
                variant.id
            );
            fs::remove_dir_all(&dir).unwrap();
        }
    }
    assert!(
        applied >= 2,
        "at least Wasp M3 and Railgun M3 variants must have been applied, got {applied}"
    );

    // Two different skins in a row, then both undone one after the other.
    let wasp = real
        .pack
        .manifest
        .items
        .iter()
        .find(|i| i.target.id == "wasp" && i.target.grade == "m3")
        .unwrap();
    if wasp.variants.len() >= 2 {
        apply_variant(&scan, &real.pack, &wasp.target, &wasp.variants[0], &backups).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        apply_variant(&scan, &real.pack, &wasp.target, &wasp.variants[1], &backups).unwrap();
        for _ in 0..2 {
            let (dir, tx) = latest_transaction(&backups).unwrap().expect("backup");
            restore_transaction(&dir, &tx).unwrap();
            fs::remove_dir_all(dir).unwrap();
        }
        assert_eq!(
            tree(&real.cache),
            real.before,
            "two applies undone in turn must give the original cache"
        );
        eprintln!("two skins in a row, restored in turn: exact");
    }
    assert_eq!(tree(&real.cache), real.before);
}

fn open(real: &Real, data: &Path) -> Harness<'static, Studio> {
    let data = data.to_path_buf();
    let root = real.cache.clone();
    let pack = real.pack.path.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(move |cc| {
            let store = StateStore::open(&data.join("state.sqlite")).unwrap();
            let session: SessionState = store.session();
            let mut app = Studio::new(
                cc,
                Startup {
                    store,
                    session,
                    data,
                    roots: Some(vec![("air-cache".into(), root)]),
                    skip_splash: true,
                },
            )
            .unwrap();
            app.game_processes_override = Some(Vec::new());
            if app.active_pack.is_none() {
                app.import_path(&pack);
            }
            app
        });
    harness.run_steps(5);
    harness
}

#[test]
#[ignore = "needs a real cache, see the file header"]
fn the_whole_journey_through_the_real_interface_on_the_real_cache() {
    let real = real();
    let data = scratch("real-ui-data");
    let mut h = open(&real, &data);
    assert!(
        h.state().active_pack.is_some(),
        "the pack is imported: {:?}",
        h.state().toasts.iter().map(|t| &t.text).collect::<Vec<_>>()
    );
    assert!(
        h.state().scan.as_ref().unwrap().resources.len() > 3000,
        "the real cache is scanned"
    );
    let items: Vec<String> = h
        .state()
        .items
        .iter()
        .map(|i| format!("{}:{}", i.kind, i.id))
        .collect();
    eprintln!("garage items in the real cache: {items:?}");

    for (tab, card, id, label) in [
        ("hull", "Wasp (Hull)", "wasp", "Wasp M3"),
        ("turret", "Railgun (Turret)", "railgun", "Railgun M3"),
    ] {
        h.get_by_label("Garage").click();
        h.run_steps(3);
        h.state_mut().garage_tab = tab.into();
        h.run_steps(3);
        h.get_by_label(card).click();
        h.run_steps(3);
        let variants = h.state().pack_variants(tab, id, "m3");
        assert!(
            variants.len() >= 2,
            "{label}: old and new variants, got {variants:?}"
        );
        for (index, (item, variant)) in variants.iter().enumerate() {
            let name = h
                .state()
                .pack_variant(*item, *variant)
                .unwrap()
                .1
                .name
                .clone();
            let before = tree(&real.cache);
            // Pick this variant in the list, then apply through the buttons.
            h.get_by_label(name.as_str()).click();
            h.run_steps(3);
            h.get_by_label(format!("Apply to {label}").as_str()).click();
            h.run_steps(4);
            let Some(Dialog::Confirm(pending)) = h.state().dialog.clone() else {
                panic!(
                    "{name}: expected the confirmation, got {:?}",
                    h.state().dialog
                );
            };
            assert_eq!(pending.target, label);
            assert!(!pending.files.is_empty());
            // Cancelling changes nothing at all.
            h.get_by_label("Cancel").click();
            h.run_steps(3);
            assert_eq!(tree(&real.cache), before, "{name}: cancel must not write");
            h.get_by_label(format!("Apply to {label}").as_str()).click();
            h.run_steps(4);
            h.get_all_by_label("Apply")
                .last()
                .expect("dialog Apply")
                .click();
            h.run_steps(6);
            let Some(Dialog::Done { files, .. }) = h.state().dialog.clone() else {
                panic!(
                    "{name}: expected the result, got {:?} / {:?}",
                    h.state().dialog,
                    h.state().toasts.iter().map(|t| &t.text).collect::<Vec<_>>()
                );
            };
            let after = tree(&real.cache);
            assert_eq!(
                changed(&before, &after).len(),
                files,
                "{name}: the dialog reports what was written"
            );
            eprintln!("UI: {name}: {files} files replaced");
            h.get_all_by_label("Restore")
                .last()
                .expect("dialog Restore")
                .click();
            h.run_steps(6);
            assert!(h.state().dialog.is_none());
            assert_eq!(
                tree(&real.cache),
                before,
                "UI: {name}: restore through the dialog must be exact (variant {index})"
            );
        }
    }
    assert_eq!(tree(&real.cache), real.before);
    assert_eq!(h.state().page, Page::Garage);
    h.state_mut().persist(true);
    // A restart keeps state and cache intact.
    drop(h);
    let again = open(&real, &data);
    assert!(again.state().active_pack.is_some());
    assert_eq!(tree(&real.cache), real.before);
}
