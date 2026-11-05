// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use tanki_cache::cache::scan_cache;
use tanki_pack::pack::import_pack;
use tanki_testkit::{HULL_MODEL, PackOptions, build_cache, build_pack_dir, scratch};

use super::*;

fn wasp_m3() -> PackTarget {
    PackTarget {
        kind: "hull".into(),
        id: "wasp".into(),
        grade: "m3".into(),
    }
}

#[test]
fn the_cache_look_is_the_newest_revision_with_every_file() {
    let fixture = build_cache(&scratch("look-cache")).unwrap();
    let scan = scan_cache(&fixture.root, Some("air-cache")).unwrap();
    let Some(LookSource::Cache(files)) = cache_look(&scan, &wasp_m3()) else {
        panic!("the fixture has the hull");
    };
    let roles: Vec<&str> = files.iter().map(|(role, ..)| role.as_str()).collect();
    assert_eq!(files.len(), 4, "{roles:?}");
    let source = LookSource::Cache(files);
    let look = source.read().unwrap();
    assert_eq!(look.model, b"original-0/0/10/14-222-object.3ds");
    assert!(look.details.is_some() && look.details_alpha.is_some() && look.lightmap.is_some());
}

#[test]
fn an_item_without_a_model_in_the_cache_has_no_look() {
    let fixture = build_cache(&scratch("look-none")).unwrap();
    let scan = scan_cache(&fixture.root, Some("air-cache")).unwrap();
    let missing = PackTarget {
        grade: "m0".into(),
        ..wasp_m3()
    };
    assert!(cache_look(&scan, &missing).is_none());
}

#[test]
fn a_replaced_cache_file_changes_the_key() {
    let fixture = build_cache(&scratch("look-key")).unwrap();
    let scan = scan_cache(&fixture.root, Some("air-cache")).unwrap();
    let before = cache_look(&scan, &wasp_m3()).unwrap().key();
    let Some(LookSource::Cache(files)) = cache_look(&scan, &wasp_m3()) else {
        panic!("the fixture has the hull");
    };
    let (_, model, _) = files
        .iter()
        .find(|(role, ..)| role == "object.3ds")
        .unwrap();
    std::fs::write(model, b"a longer replacement model").unwrap();
    let scan = scan_cache(&fixture.root, Some("air-cache")).unwrap();
    assert_ne!(cache_look(&scan, &wasp_m3()).unwrap().key(), before);
}

#[test]
fn a_pack_variant_look_reads_its_resources() {
    let root = scratch("look-pack");
    let source = build_pack_dir(&root.join("src"), &PackOptions::default()).unwrap();
    let pack = import_pack(&source, &root.join("store")).unwrap();
    let item = pack
        .manifest
        .items
        .iter()
        .find(|i| i.target.kind == "hull")
        .unwrap();
    let look = variant_look(&pack, &item.variants[0]).unwrap();
    assert!(look.key().starts_with("pack:"));
    assert_eq!(look.read().unwrap().model, HULL_MODEL);
}

#[test]
fn a_variant_without_a_model_has_no_look() {
    let root = scratch("look-no-model");
    let options = PackOptions {
        hull_without_model: true,
        ..PackOptions::default()
    };
    let source = build_pack_dir(&root.join("src"), &options).unwrap();
    let pack = import_pack(&source, &root.join("store")).unwrap();
    let item = pack
        .manifest
        .items
        .iter()
        .find(|i| i.target.kind == "hull")
        .unwrap();
    assert!(variant_look(&pack, &item.variants[0]).is_none());
}

#[test]
fn a_painted_body_is_the_body_s_files_with_the_paint_s_picture() {
    let dir = scratch("look-painted");
    let file = |name: &str, bytes: &[u8]| {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        (name.to_owned(), path, bytes.len() as u64)
    };
    let body = LookSource::Cache(vec![file("object.3ds", b"hull model")]);
    let paint = LookSource::Cache(vec![file("image.jpg", b"paint picture")]);
    let painted = LookSource::Painted {
        body: Box::new(body.clone()),
        paint: Box::new(paint.clone()),
    };
    let look = painted.read().unwrap();
    assert_eq!(look.model, b"hull model");
    assert_eq!(look.paint.as_deref(), Some(&b"paint picture"[..]));
    assert_ne!(painted.key(), body.key());
    assert!(paint.read().is_ok(), "a paint alone has a picture");
    let nothing = LookSource::Cache(vec![file("details.jpg", b"x")]);
    assert!(nothing.read().is_err(), "neither a model nor a paint");
}

#[test]
fn revisions_are_read_from_the_end_of_the_group() {
    assert_eq!(revision("/0/0/10/14/222/"), 222);
    assert_eq!(revision("/0/0/10/14/abc/"), 0);
    assert_eq!(revision(""), 0);
}
