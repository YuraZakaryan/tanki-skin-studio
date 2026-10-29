// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Runs the real `tanki-cli` binary against a fixture cache and an isolated data folder.

use std::{fs, path::Path, process::Command};
use tanki_testkit::{HULL_MODEL, PackOptions, build_cache, build_pack_dir, scratch};

fn cli(data: &Path, cache: &Path, args: &[&str]) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_tanki-cli"))
        .args(args)
        .env("TANKI_DATA_DIR", data)
        .env("TANKI_CACHE_DIR", cache)
        .output()
        .expect("spawn tanki-cli");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

#[test]
fn scan_import_apply_restore_through_the_binary() {
    let root = scratch("cli");
    let cache = build_cache(&root.join("cache")).unwrap();
    let pack = build_pack_dir(&root.join("pack"), &PackOptions::default()).unwrap();
    let data = root.join("data");
    let cache_arg = cache.root.to_str().unwrap();
    let pack_arg = pack.to_str().unwrap();

    let (ok, out) = cli(&data, &cache.root, &["scan", cache_arg]);
    assert!(ok, "{out}");
    assert!(out.contains("air-cache: 26 resources"), "{out}");

    let (ok, out) = cli(&data, &cache.root, &["pack-validate", pack_arg]);
    assert!(
        ok && out.contains("Valid pack: Fixture Pack 1.0.0 (2 items)"),
        "{out}"
    );

    let (ok, out) = cli(&data, &cache.root, &["import", pack_arg]);
    assert!(ok && out.contains("Imported Fixture Pack 1.0.0"), "{out}");
    let (ok, out) = cli(&data, &cache.root, &["import", pack_arg]);
    assert!(
        !ok && out.contains("already imported"),
        "re-import must fail: {out}"
    );

    let (ok, out) = cli(&data, &cache.root, &["packs"]);
    assert!(ok && out.contains("fixture-pack@1.0.0"), "{out}");

    let before = cache.snapshot(&cache.hull_files);
    let (ok, out) = cli(
        &data,
        &cache.root,
        &["apply", "fixture-pack", "red", cache_arg],
    );
    assert!(ok && out.contains("Applied Wasp Red to 8 files"), "{out}");
    assert!(
        cache
            .hull_files
            .iter()
            .any(|p| fs::read(p).unwrap() == HULL_MODEL)
    );

    let (ok, out) = cli(
        &data,
        &cache.root,
        &["apply", "fixture-pack", "nope", cache_arg],
    );
    assert!(!ok && out.contains("variant not found"), "{out}");

    let (ok, out) = cli(&data, &cache.root, &["restore"]);
    assert!(ok && out.contains("Restored 8 files"), "{out}");
    for (path, original) in &before {
        assert_eq!(&fs::read(path).unwrap(), original);
    }
    fs::remove_dir_all(root).unwrap();
}
