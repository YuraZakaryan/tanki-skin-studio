// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! End-to-end journey over the public core API against a synthetic cache and pack:
//! scan → import (zip + folder) → persisted state → apply → restore, plus hostile inputs.

use std::{fs, path::PathBuf};

use anyhow::Result;
use tanki_core::{
    apply::{apply_variant, latest_transaction, list_backups, missing_files, restore_transaction},
    cache::{known_target_group, scan_cache},
    pack::{InstalledPack, import_pack, load_pack},
    state::StateStore,
    targets::KNOWN_TARGETS,
};
use tanki_testkit::{
    HULL_MODEL, PackOptions, TURRET_MODEL, build_cache, build_pack_dir, scratch, zip_dir,
};

fn find_item<'a>(
    pack: &'a InstalledPack,
    id: &str,
) -> (
    &'a tanki_core::pack::PackTarget,
    &'a tanki_core::pack::PackVariant,
) {
    let item = pack
        .manifest
        .items
        .iter()
        .find(|i| i.target.id == id)
        .expect("item in pack");
    (&item.target, &item.variants[0])
}

fn decoded(path: &PathBuf) -> image::RgbImage {
    image::load_from_memory(&fs::read(path).unwrap())
        .expect("valid image written to cache")
        .to_rgb8()
}

#[test]
fn import_apply_restore_journey() -> Result<()> {
    let root = scratch("journey");
    let cache = build_cache(&root.join("cache"))?;
    let data = root.join("data");
    let packs_dir = data.join("packs");
    let backups = data.join("backups");

    // 1. Scan: the two built-in items are recognised, in both revisions. Counted against the
    // built-in table: hand labels live in one registry per process, and a test running beside
    // this one may hold a label for the fixture's third item.
    let scan = scan_cache(&cache.root, Some("air-cache"))?;
    let recognised = scan
        .resources
        .iter()
        .filter(|r| {
            KNOWN_TARGETS
                .iter()
                .any(|t| t.game == scan.game && r.group.contains(t.marker))
        })
        .count();
    assert_eq!(recognised, 16, "2 items x 2 revisions x 4 files");
    assert_eq!(
        scan.resources.len(),
        16 + cache.hornet_files.len() + cache.unrelated_files.len()
    );

    // 2. Import a zip, persist, and reopen the store as after an app restart.
    let pack_dir = build_pack_dir(&root.join("src-pack"), &PackOptions::default())?;
    let zip = zip_dir(&pack_dir, &root.join("fixture.tankipack"), &[])?;
    let first = import_pack(&zip, &packs_dir)?;
    {
        let store = StateStore::open(&data.join("state.sqlite"))?;
        store.save_pack(&first)?;
        store.record_recent_asset("hull/wasp/m3")?;
        store.record_recent_asset("turret/railgun/m3")?;
    }
    let store = StateStore::open(&data.join("state.sqlite"))?;
    let active = store.active_pack()?.expect("active pack survives restart");
    assert_eq!(active.manifest.id, "fixture-pack");
    assert_eq!(
        store.recent_assets(5)?,
        ["turret/railgun/m3", "hull/wasp/m3"],
        "most recent first"
    );

    // 3. A newer version imported from a folder becomes the active, first-listed pack.
    let v2_dir = build_pack_dir(
        &root.join("src-pack-2"),
        &PackOptions {
            version: "1.1.0",
            ..PackOptions::default()
        },
    )?;
    let second = import_pack(&v2_dir, &packs_dir)?;
    store.save_pack(&second)?;
    assert_eq!(store.active_pack()?.unwrap().manifest.version, "1.1.0");
    let listed = store.list_packs()?;
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].manifest.version, "1.1.0");
    assert!(
        import_pack(&v2_dir, &packs_dir).is_err(),
        "the same version is not imported twice"
    );
    store.set_active_pack("fixture-pack", "1.0.0")?;
    assert_eq!(store.active_pack()?.unwrap().manifest.version, "1.0.0");

    // 4. Apply hull then turret; each touches only its own item.
    let originals_all: Vec<_> = cache
        .hull_files
        .iter()
        .chain(&cache.turret_files)
        .chain(&cache.hornet_files)
        .chain(&cache.unrelated_files)
        .cloned()
        .collect();
    let before = cache.snapshot(&originals_all);

    let (hull_target, hull_variant) = find_item(&first, "wasp");
    let hull_tx = apply_variant(&scan, &first, hull_target, hull_variant, &backups)?;
    assert_eq!(hull_tx.changes.len(), 8, "4 files x 2 revisions");
    for path in cache.hull_files.iter().filter(|p| {
        scan.resources
            .iter()
            .any(|r| &r.path == *p && r.role == "object.3ds")
    }) {
        assert_eq!(fs::read(path)?, HULL_MODEL);
    }
    for path in &cache.hull_files {
        let role = &scan
            .resources
            .iter()
            .find(|r| &r.path == path)
            .unwrap()
            .role;
        match role.as_str() {
            "details.jpg" => assert!(decoded(path).get_pixel(3, 3)[0] > 150, "red PNG -> JPEG"),
            "details_alpha.jpg" => assert!(
                image::load_from_memory(&fs::read(path)?)?
                    .to_luma8()
                    .get_pixel(3, 3)[0]
                    > 200,
                "opaque PNG -> bright alpha mask"
            ),
            "lightmap.jpg" => {
                let original = &before.iter().find(|(p, _)| p == path).unwrap().1;
                assert_ne!(
                    &fs::read(path)?,
                    original,
                    "a complete skin brings its lightmap"
                );
            }
            _ => {}
        }
    }
    for (path, original) in before.iter().filter(|(p, _)| {
        cache.turret_files.contains(p)
            || cache.hornet_files.contains(p)
            || cache.unrelated_files.contains(p)
    }) {
        assert_eq!(&fs::read(path)?, original, "other items untouched");
    }

    let (turret_target, turret_variant) = find_item(&first, "railgun");
    let turret_tx = apply_variant(&scan, &first, turret_target, turret_variant, &backups)?;
    for path in &cache.turret_files {
        let role = &scan
            .resources
            .iter()
            .find(|r| &r.path == path)
            .unwrap()
            .role;
        if role == "object.3ds" {
            assert_eq!(fs::read(path)?, TURRET_MODEL);
        }
    }

    // 5. The newest backup is the turret; restoring it leaves the hull skin in place.
    let (dir, latest) = latest_transaction(&backups)?.expect("a backup exists");
    assert_eq!(latest.id, turret_tx.id);
    restore_transaction(&dir, &latest)?;
    for (path, original) in before
        .iter()
        .filter(|(p, _)| cache.turret_files.contains(p))
    {
        assert_eq!(&fs::read(path)?, original, "turret restored byte for byte");
    }
    assert!(
        cache
            .hull_files
            .iter()
            .any(|p| fs::read(p).unwrap() == HULL_MODEL),
        "hull skin still applied"
    );

    // 6. Restoring the hull as well returns the whole cache to its original state.
    restore_transaction(&backups.join(&hull_tx.id), &hull_tx)?;
    for (path, original) in &before {
        assert_eq!(&fs::read(path)?, original);
    }

    // Windows cannot delete a database that is still open.
    drop(store);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn hostile_and_broken_packs_are_rejected_without_leftovers() -> Result<()> {
    let root = scratch("hostile");
    let packs_dir = root.join("packs");
    let good = build_pack_dir(&root.join("good"), &PackOptions::default())?;

    let traversal = zip_dir(
        &good,
        &root.join("traversal.tankipack"),
        &[("../evil.txt", b"x")],
    )?;
    assert!(load_pack(&traversal).is_err(), "path traversal entry");
    assert!(import_pack(&traversal, &packs_dir).is_err());

    let corrupt = build_pack_dir(
        &root.join("corrupt"),
        &PackOptions {
            corrupt_checksum: true,
            ..PackOptions::default()
        },
    )?;
    let error = import_pack(&corrupt, &packs_dir).unwrap_err().to_string();
    assert!(error.contains("checksum"), "{error}");

    let empty = root.join("empty");
    fs::create_dir_all(&empty)?;
    assert!(load_pack(&empty).is_err(), "no manifest");
    assert!(load_pack(&root.join("missing.zip")).is_err());

    let leftovers = fs::read_dir(&packs_dir)
        .map(|d| d.count())
        .unwrap_or_default();
    assert_eq!(leftovers, 0, "failed imports leave no staging files");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn a_skin_missing_a_file_of_the_item_is_refused_and_changes_nothing() -> Result<()> {
    let root = scratch("incomplete");
    let cache = build_cache(&root.join("cache"))?;
    let scan = scan_cache(&cache.root, Some("air-cache"))?;
    let before = cache.snapshot(&cache.hull_files);
    let pack = import_pack(
        &build_pack_dir(&root.join("pack"), &PackOptions::default())?,
        &root.join("packs"),
    )?;
    let (target, variant) = find_item(&pack, "wasp");
    assert!(
        missing_files(&scan, target, variant).is_empty(),
        "the fixture skin is complete"
    );
    // The Viking Prime of the public archive: textures without a model.
    let mut no_model = variant.clone();
    no_model.resources.retain(|r| r.role != "model");
    assert_eq!(missing_files(&scan, target, &no_model), ["object.3ds"]);
    let error = apply_variant(&scan, &pack, target, &no_model, &root.join("backups"))
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("incomplete") && error.contains("object.3ds"),
        "{error}"
    );
    // A JPEG texture brings no alpha of its own.
    let mut jpeg_only = variant.clone();
    for resource in &mut jpeg_only.resources {
        if resource.role == "details" {
            resource.path = resource.path.replace(".png", ".jpg");
        }
    }
    assert_eq!(
        missing_files(&scan, target, &jpeg_only),
        ["details_alpha.jpg"]
    );
    assert_eq!(
        cache.snapshot(&cache.hull_files),
        before,
        "nothing was written"
    );
    assert!(
        list_backups(&root.join("backups"))?.is_empty(),
        "and nothing was backed up"
    );
    Ok(())
}

#[test]
fn apply_refuses_wrong_game_unknown_target_and_tampered_backup() -> Result<()> {
    let root = scratch("refuse");
    let cache = build_cache(&root.join("cache"))?;
    let scan = scan_cache(&cache.root, Some("air-cache"))?;
    let backups = root.join("backups");
    let before = cache.snapshot(&cache.hull_files);

    // Pack for another client.
    let other = build_pack_dir(
        &root.join("other"),
        &PackOptions {
            id: "other-game",
            games: &["other-game"],
            ..PackOptions::default()
        },
    )?;
    let other = import_pack(&other, &root.join("packs"))?;
    let (target, variant) = find_item(&other, "wasp");
    let error = apply_variant(&scan, &other, target, variant, &backups)
        .unwrap_err()
        .to_string();
    assert!(error.contains("does not support"), "{error}");

    // Item the adapter cannot identify must not touch anything.
    let good = import_pack(
        &build_pack_dir(&root.join("good"), &PackOptions::default())?,
        &root.join("packs"),
    )?;
    let (_, variant) = find_item(&good, "wasp");
    let mut unknown = good.manifest.items[0].target.clone();
    // Labels are process-global, so use an id no other test labels.
    unknown.id = "mosquito".into();
    let error = apply_variant(&scan, &good, &unknown, variant, &backups)
        .unwrap_err()
        .to_string();
    assert!(error.contains("not recognized"), "{error}");
    for (path, original) in &before {
        assert_eq!(&fs::read(path)?, original);
    }

    // A corrupted backup is detected and the applied file is left alone.
    let (target, variant) = find_item(&good, "wasp");
    let tx = apply_variant(&scan, &good, target, variant, &backups)?;
    let backup_file = backups.join(&tx.id).join(&tx.changes[0].backup_name);
    fs::write(&backup_file, b"tampered")?;
    let applied = fs::read(&tx.changes[0].target)?;
    let error = restore_transaction(&backups.join(&tx.id), &tx)
        .unwrap_err()
        .to_string();
    assert!(error.contains("backup corrupt"), "{error}");
    assert_eq!(fs::read(&tx.changes[0].target)?, applied);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn hand_labelled_item_becomes_applicable_and_survives_restart() -> Result<()> {
    use tanki_core::targets::{is_user_target, marker_for_group, unregister_user_target};
    use tanki_testkit::HORNET_MODEL;

    let root = scratch("label");
    let cache = build_cache(&root.join("cache"))?;
    let data = root.join("data");
    let scan = scan_cache(&cache.root, Some("air-cache"))?;
    let hornet_group = scan
        .resources
        .iter()
        .find(|r| r.group.contains("/0/0/10/20/"))
        .expect("hornet group in the fixture")
        .group
        .clone();
    let pack = import_pack(
        &build_pack_dir(
            &root.join("pack"),
            &PackOptions {
                with_hornet: true,
                ..PackOptions::default()
            },
        )?,
        &data.join("packs"),
    )?;
    let (target, variant) = find_item(&pack, "hornet");

    // Unknown until labelled: the pack cannot be applied and nothing changes.
    assert!(known_target_group("air-cache", &hornet_group).is_none());
    let before = cache.snapshot(&cache.hornet_files);
    assert!(apply_variant(&scan, &pack, target, variant, &data.join("backups")).is_err());
    for (path, original) in &before {
        assert_eq!(&fs::read(path)?, original);
    }

    // Label it by hand, as the inspector form does.
    let marker = marker_for_group(&hornet_group).expect("marker");
    {
        let store = StateStore::open(&data.join("state.sqlite"))?;
        store.save_user_target("air-cache", &marker, "hull", "hornet", "m3")?;
    }
    assert_eq!(
        known_target_group("air-cache", &hornet_group),
        Some(("hull", "hornet", "m3"))
    );
    assert!(is_user_target("air-cache", &hornet_group));
    let tx = apply_variant(&scan, &pack, target, variant, &data.join("backups"))?;
    assert_eq!(tx.changes.len(), 8, "4 files x 2 revisions");
    assert!(
        cache
            .hornet_files
            .iter()
            .any(|p| fs::read(p).unwrap() == HORNET_MODEL)
    );

    // The label persists: forget it in memory, reopen the store, it is back.
    unregister_user_target("air-cache", &marker);
    assert!(known_target_group("air-cache", &hornet_group).is_none());
    let store = StateStore::open(&data.join("state.sqlite"))?;
    assert_eq!(
        known_target_group("air-cache", &hornet_group),
        Some(("hull", "hornet", "m3"))
    );

    // Removing the label makes the item unknown again, in memory and on disk.
    store.delete_user_target("air-cache", &marker)?;
    assert!(known_target_group("air-cache", &hornet_group).is_none());
    drop(store);
    StateStore::open(&data.join("state.sqlite"))?;
    assert!(known_target_group("air-cache", &hornet_group).is_none());

    // Built-in rows cannot be overridden.
    let store = StateStore::open(&data.join("state.sqlite"))?;
    assert!(
        store
            .save_user_target("air-cache", "/0/0/10/14/", "hull", "fake", "m1")
            .is_err()
    );
    drop(store);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn labelling_one_modification_labels_the_whole_item() -> Result<()> {
    use tanki_core::targets::marker_for_group;
    use tanki_testkit::build_item_block;

    let root = scratch("block");
    let cache_dir = root.join("cache");
    let block = build_item_block(&cache_dir, 3000)?; // ids 3000, 3002, 3004, 3006
    let scan = scan_cache(&cache_dir, Some("air-cache"))?;
    let group_of = |files: &Vec<PathBuf>| {
        scan.resources
            .iter()
            .find(|r| r.path == files[0])
            .expect("file in scan")
            .group
            .clone()
    };
    let groups: Vec<String> = block.iter().map(group_of).collect();
    let store = StateStore::open(&root.join("state.sqlite"))?;
    let present: Vec<String> = groups.iter().filter_map(|g| marker_for_group(g)).collect();
    let exists = |marker: &str| present.iter().any(|m| m == marker);

    // The user only identifies M2; M0, M1 and M3 follow from the consecutive ids.
    let m2_marker = marker_for_group(&groups[2]).unwrap();
    let labelled =
        store.save_user_item("air-cache", &m2_marker, "hull", "mosquito2", "m2", &exists)?;
    assert_eq!(labelled, ["m0", "m1", "m2", "m3"]);
    for (i, group) in groups.iter().enumerate() {
        let (kind, id, grade) = known_target_group("air-cache", group).expect("recognised");
        assert_eq!((kind, id), ("hull", "mosquito2"));
        assert_eq!(grade, format!("m{i}"));
    }

    // A pack skin for M3 now applies to the right modification only.
    let pack = import_pack(
        &build_pack_dir(
            &root.join("pack"),
            &PackOptions {
                with_hornet: true,
                ..PackOptions::default()
            },
        )?,
        &root.join("packs"),
    )?;
    let mut target = find_item(&pack, "hornet").0.clone();
    target.id = "mosquito2".into();
    let variant = find_item(&pack, "hornet").1.clone();
    let tx = apply_variant(&scan, &pack, &target, &variant, &root.join("backups"))?;
    assert_eq!(tx.changes.len(), 8, "4 files x 2 revisions, only for M3");
    for file in &block[3] {
        let _ = fs::read(file)?;
    }
    for earlier in &block[..3] {
        for file in earlier {
            assert!(!fs::read(file)?.starts_with(b"fixture"), "M0..M2 untouched");
        }
    }

    // Siblings already claimed are never overwritten; a missing sibling is skipped.
    let other = store.save_user_item("air-cache", &m2_marker, "hull", "other", "m2", &exists)?;
    assert_eq!(
        other,
        ["m2"],
        "only the explicitly labelled one is re-labelled"
    );
    assert!(
        store
            .save_user_item("air-cache", &m2_marker, "hull", "x", "m9", &exists)
            .is_err()
    );
    for group in &groups {
        let marker = marker_for_group(group).unwrap();
        store.delete_user_target("air-cache", &marker)?;
    }
    drop(store);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn backups_are_listed_newest_first_with_what_was_replaced() -> Result<()> {
    use tanki_core::apply::{list_backups, planned_files};
    let root = scratch("backups");
    let cache = build_cache(&root.join("cache"))?;
    let data = root.join("data");
    let pack_dir = build_pack_dir(&root.join("src-pack"), &PackOptions::default())?;
    let pack = import_pack(&pack_dir, &data.join("packs"))?;
    let scan = scan_cache(&cache.root, Some("air-cache"))?;
    assert!(
        list_backups(&data.join("backups"))?.is_empty(),
        "no backups before the first replacement"
    );

    // The plan names exactly the cache files that will change, with the replacement sizes.
    let (target, variant) = find_item(&pack, "wasp");
    let plan = planned_files(&pack, variant)?;
    let names: Vec<&str> = plan.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "details.jpg",
            "details_alpha.jpg",
            "lightmap.jpg",
            "object.3ds"
        ]
    );
    assert_eq!(
        plan.iter()
            .find(|(n, _)| n == "object.3ds")
            .map(|(_, s)| *s),
        Some(HULL_MODEL.len() as u64)
    );

    apply_variant(&scan, &pack, target, variant, &data.join("backups"))?;
    let (target, variant) = find_item(&pack, "railgun");
    apply_variant(&scan, &pack, target, variant, &data.join("backups"))?;

    let backups = list_backups(&data.join("backups"))?;
    assert_eq!(backups.len(), 2);
    assert!(
        backups[0].created_unix >= backups[1].created_unix,
        "newest first"
    );
    let latest = &backups[0].tx;
    assert_eq!(
        latest.target.as_ref().map(|t| t.id.as_str()),
        Some("railgun")
    );
    assert_eq!(latest.variant_name, "Railgun Blue");
    assert!(
        backups[0].bytes > 0,
        "the size of the saved originals is reported"
    );
    assert!(
        backups[0].created_unix > 1_600_000_000,
        "time is read from the transaction id"
    );
    Ok(())
}

#[test]
fn a_refused_pack_lists_every_problem_and_leaves_nothing_behind() -> Result<()> {
    use tanki_core::pack::{PackIssueKind, PackRejected, pack_issues};
    let root = scratch("issues");
    let packs_dir = root.join("data/packs");
    let corrupt = build_pack_dir(
        &root.join("corrupt"),
        &PackOptions {
            corrupt_checksum: true,
            ..PackOptions::default()
        },
    )?;
    // A second problem in the same pack: a resource the manifest names but the folder lacks.
    fs::remove_file(corrupt.join("turret/model.3ds"))?;
    let issues = pack_issues(&corrupt);
    assert!(
        issues
            .iter()
            .any(|i| i.kind == PackIssueKind::ChecksumMismatch && i.file == "hull/model.3ds"),
        "{issues:?}"
    );
    assert!(
        issues
            .iter()
            .any(|i| i.kind == PackIssueKind::Missing && i.file == "turret/model.3ds"),
        "{issues:?}"
    );

    let error = import_pack(&corrupt, &packs_dir).unwrap_err();
    let rejected = error
        .downcast::<PackRejected>()
        .expect("the typed rejection reaches the UI");
    assert_eq!(rejected.issues, issues);
    assert!(
        !packs_dir.exists() || fs::read_dir(&packs_dir)?.next().is_none(),
        "nothing was written to the library"
    );

    // Hostile archive entries are reported as invalid paths and as duplicates, not just refused.
    let good = build_pack_dir(&root.join("good"), &PackOptions::default())?;
    let hostile = zip_dir(
        &good,
        &root.join("hostile.zip"),
        &[("../outside.png", b"x")],
    )?;
    assert!(
        pack_issues(&hostile)
            .iter()
            .any(|i| i.kind == PackIssueKind::InvalidPath && i.file == "../outside.png")
    );
    // A healthy pack has no issues.
    assert!(pack_issues(&good).is_empty());
    Ok(())
}

#[test]
fn deleting_a_pack_removes_its_files_and_falls_back_to_the_next_one() -> Result<()> {
    let root = scratch("delete-pack");
    let data = root.join("data");
    let store = StateStore::open(&data.join("state.sqlite"))?;
    let v1 = import_pack(
        &build_pack_dir(&root.join("v1"), &PackOptions::default())?,
        &data.join("packs"),
    )?;
    store.save_pack(&v1)?;
    let v2 = import_pack(
        &build_pack_dir(
            &root.join("v2"),
            &PackOptions {
                version: "2.0.0",
                ..PackOptions::default()
            },
        )?,
        &data.join("packs"),
    )?;
    store.save_pack(&v2)?;
    assert_eq!(
        store.active_pack()?.expect("active").manifest.version,
        "2.0.0"
    );

    store.delete_pack("fixture-pack", "2.0.0")?;
    assert!(!v2.path.exists(), "the deleted pack's files are gone");
    let active = store
        .active_pack()?
        .expect("falls back to the remaining pack");
    assert_eq!(active.manifest.version, "1.0.0");
    assert_eq!(store.list_packs()?.len(), 1);

    store.delete_pack("fixture-pack", "1.0.0")?;
    assert!(store.active_pack()?.is_none(), "no pack left, none active");
    // Deleting something that is not there is not an error.
    store.delete_pack("fixture-pack", "9.9.9")?;
    Ok(())
}

#[test]
fn language_theme_size_and_session_survive_a_restart_without_clobbering_each_other() -> Result<()> {
    use tanki_core::session::{ItemRef, SessionState};
    let root = scratch("prefs");
    let db = root.join("data/state.sqlite");
    {
        let store = StateStore::open(&db)?;
        assert_eq!(
            store.language()?,
            None,
            "nothing chosen yet: the app starts in English"
        );
        store.set_language("ru")?;
        store.set_theme("light")?;
        store.set_ui_scale(125)?;
        let mut session = SessionState {
            page: "garage".into(),
            ..SessionState::default()
        };
        let cache = session.cache_mut("/caches/a");
        cache.item = Some(ItemRef {
            kind: "hull".into(),
            id: "wasp".into(),
        });
        cache.grade = Some("m3".into());
        assert!(store.save_session(&session)?, "first save writes");
        assert!(
            !store.save_session(&session)?,
            "an unchanged session is not written again"
        );
    }
    let store = StateStore::open(&db)?;
    assert_eq!(store.language()?.as_deref(), Some("ru"));
    assert_eq!(store.theme()?.as_deref(), Some("light"));
    assert_eq!(store.ui_scale()?.as_deref(), Some("125"));
    let session = store.session();
    assert_eq!(session.page, "garage");
    assert_eq!(
        session
            .cache("/caches/a")
            .and_then(|c| c.grade.clone())
            .as_deref(),
        Some("m3")
    );

    // A damaged session document is dropped without touching the other preferences.
    store.set("ui.session.v1", "{ this is not json")?;
    assert_eq!(store.session(), SessionState::default());
    assert_eq!(store.language()?.as_deref(), Some("ru"));
    assert_eq!(store.theme()?.as_deref(), Some("light"));
    Ok(())
}
