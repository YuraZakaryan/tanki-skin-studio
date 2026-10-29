use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{build_model_pack, is_model_archive, layout::interpret};
use crate::pack::{PackManifest, load_pack};

fn scratch(label: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "tanki-archive-{label}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn write(dir: &Path, relative: &str, bytes: &[u8]) {
    let path = dir.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
    fs::write(path, bytes).expect("file");
}

fn build(archive: &Path) -> (super::ArchiveSummary, PackManifest) {
    let out = scratch("pack").join("pack");
    let summary = build_model_pack(archive, &out).expect("builds");
    (summary, load_pack(&out).expect("valid pack"))
}

/// `kind:id:grade` of every item, sorted.
fn targets(manifest: &PackManifest) -> Vec<String> {
    manifest
        .items
        .iter()
        .map(|i| format!("{}:{}:{}", i.target.kind, i.target.id, i.target.grade))
        .collect()
}

fn item<'a>(manifest: &'a PackManifest, target: &str) -> &'a crate::pack::PackItem {
    manifest
        .items
        .iter()
        .find(|i| format!("{}:{}:{}", i.target.kind, i.target.id, i.target.grade) == target)
        .unwrap_or_else(|| panic!("{target} in {:?}", targets(manifest)))
}

fn parts(path: &str) -> Vec<String> {
    path.split('/').map(str::to_owned).collect()
}

#[test]
fn folder_names_are_read_as_words_whatever_the_layout() {
    let read = |path: &str| {
        let identity = interpret(&parts(path)).expect(path);
        (
            identity.kind.code(),
            identity.id,
            identity.grade,
            identity.variant,
            identity.strong,
        )
    };
    let own = |kind, id: &str, grade: &str, variant: &str, strong| {
        (
            kind,
            id.to_owned(),
            grade.to_owned(),
            variant.to_owned(),
            strong,
        )
    };
    assert_eq!(
        read("archive/Flash/Hulls/New/Wasp/M3"),
        own("hull", "wasp", "m3", "new", true)
    );
    assert_eq!(
        read("archive/2.0/hulls/wasp/m0"),
        own("hull", "wasp", "m0", "default", true)
    );
    assert_eq!(
        read("archive/Weapons/Railgun/XT/XT M2"),
        own("turret", "railgun", "xt-m2", "default", true)
    );
    assert_eq!(
        read("archive/Turrets/Old/Freeze/M3 ALT"),
        own("turret", "freeze", "m3-alt", "old", true)
    );
    assert_eq!(
        read("archive/tanks/Neon/Hunter/M1/textures"),
        own("hull", "hunter", "m1", "neon", true),
        "a skin name between the kind folder and the item; filing folders say nothing"
    );
    assert_eq!(
        read("my skins/FirebirdXT"),
        own("turret", "firebird", "xt", "default", false),
        "no kind folder: the name alone (weak), camel case split"
    );
    let typo = interpret(&parts("TO-Assets/Hulls/mamoth")).expect("mamoth");
    assert_eq!(
        (typo.id.as_str(), typo.name.as_str()),
        ("mammoth", "Mammoth")
    );
    assert!(typo.note.is_some(), "a misspelt name is reported");
    let unknown = interpret(&parts("TO-Assets/Hulls/ufo")).expect("an unknown hull");
    assert_eq!((unknown.kind.code(), unknown.id.as_str()), ("hull", "ufo"));
    assert!(unknown.note.is_some());
    assert!(interpret(&parts("archive/Flash/Misc/Rugby")).is_none());
}

#[test]
fn the_flash_archive_layout_with_editions_and_groups() {
    let archive = scratch("flash");
    for style in ["Old", "New"] {
        for file in ["object.3ds", "details.png", "lightmap.jpg", "Preview.png"] {
            write(
                &archive,
                &format!("Flash/Hulls/{style}/Wasp/M3/{file}"),
                file.as_bytes(),
            );
        }
    }
    write(&archive, "Flash/Turrets/Old/Railgun/M2/object.3ds", b"gun");
    write(
        &archive,
        "Flash/Turrets/Old/Railgun/XT/object.3ds",
        b"edition",
    );
    // A group of modifications must not collide with the plain ones.
    write(
        &archive,
        "Flash/Turrets/Old/Railgun/ALT/M2/object.3ds",
        b"alt",
    );
    // A folder that groups editions, and a preview not called preview.png.
    write(
        &archive,
        "Flash/Hulls/New/Hornet/XT/XT M0/object.3ds",
        b"grouped",
    );
    write(
        &archive,
        "Flash/Hulls/New/Hornet/XT/XT M0/Hull_hornet_xt_m0.png",
        b"picture",
    );
    write(&archive, "Flash/Misc/Rugby/ball.3ds", b"not a hull");
    write(
        &archive,
        "Flash/Misc/Overdrives/Wasp/object.3ds",
        b"an effect",
    );
    assert!(is_model_archive(&archive));

    let (summary, manifest) = build(&archive);
    assert_eq!(
        targets(&manifest),
        [
            "hull:hornet:xt-m0",
            "hull:wasp:m3",
            "turret:railgun:alt-m2",
            "turret:railgun:m2",
            "turret:railgun:xt"
        ]
    );
    let wasp = item(&manifest, "hull:wasp:m3");
    let names: Vec<&str> = wasp.variants.iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, ["Wasp New", "Wasp Old"]);
    assert_eq!(wasp.variants[0].resources.len(), 3);
    assert_eq!(
        wasp.variants[0].preview.as_deref(),
        Some("assets/hull/wasp/m3/new/preview.png")
    );
    assert_eq!(
        item(&manifest, "hull:hornet:xt-m0").variants[0]
            .preview
            .as_deref(),
        Some("assets/hull/hornet/xt-m0/new/preview.png")
    );
    assert_eq!((summary.items, summary.variants), (5, 6));
    assert_eq!(summary.without_preview, 3, "the three Railgun variants");
    assert!(summary.unused.is_empty(), "{:?}", summary.unused);
    assert_eq!(summary.unrecognised, ["Flash/Misc/Rugby"]);
    assert_eq!(
        summary.skipped,
        ["Flash/Misc/Overdrives/Wasp"],
        "a name alone is not a hull when the archive has real hull folders"
    );
    assert!(
        summary
            .notes
            .iter()
            .any(|n| n.contains("Hull_hornet_xt_m0.png")),
        "{:?}",
        summary.notes
    );
}

#[test]
fn layouts_without_old_new_or_known_names() {
    let archive = scratch("layouts");
    write(&archive, "Weapons/Railgun/M3/object.3ds", b"gun");
    write(&archive, "tanks/wasp/object.3ds", b"no modification folder");
    write(&archive, "Hulls/Mamoth/M1/object.3ds", b"typo");
    write(&archive, "Hulls/Ufo/object.3ds", b"unknown hull");
    // One skin split into a model folder and a texture folder.
    write(&archive, "Hulls/Neon/Hunter/M3/model/object.3ds", b"model");
    write(
        &archive,
        "Hulls/Neon/Hunter/M3/textures/details.png",
        b"png",
    );
    write(&archive, "html5/hulls/viking/xt/object.a3d", b"html5 model");
    write(
        &archive,
        "html5/hulls/viking/xt/lightmap.webp",
        b"html5 texture",
    );

    let (summary, manifest) = build(&archive);
    assert_eq!(
        targets(&manifest),
        [
            "hull:hunter:m3",
            "hull:mammoth:m1",
            "hull:ufo:default",
            "hull:wasp:default",
            "turret:railgun:m3"
        ]
    );
    let hunter = &item(&manifest, "hull:hunter:m3").variants[0];
    assert_eq!(
        (hunter.id.as_str(), hunter.name.as_str()),
        ("neon", "Hunter Neon")
    );
    let roles: Vec<&str> = hunter.resources.iter().map(|r| r.role.as_str()).collect();
    assert_eq!(roles, ["model", "details"], "the two folders are one skin");
    assert_eq!(
        summary.unsupported.get("a3d").copied(),
        Some(1),
        "{:?}",
        summary.unsupported
    );
    assert_eq!(summary.unsupported.get("webp").copied(), Some(1));
    assert!(summary.notes.iter().any(|n| n.contains("mamoth")));
    assert!(summary.notes.iter().any(|n| n.contains("ufo")));
}

#[test]
fn an_archive_of_names_only_is_imported() {
    let archive = scratch("names");
    write(&archive, "Smoky XT/object.3ds", b"gun");
    write(&archive, "Railgun/M2/lightmap.jpg", b"jpg");
    let (summary, manifest) = build(&archive);
    assert_eq!(targets(&manifest), ["turret:railgun:m2", "turret:smoky:xt"]);
    assert!(summary.skipped.is_empty());
}

#[test]
fn files_are_told_by_their_content_and_what_is_not_understood_is_reported() {
    let archive = scratch("content");
    // A model under another name: a 3DS by its signature (main chunk length = file size).
    let mut model = vec![0x4D, 0x4D, 10, 0, 0, 0];
    model.extend([0u8; 4]);
    write(&archive, "Flash/Turrets/New/Magnum/XT/body.dat", &model);
    write(
        &archive,
        "Flash/Turrets/New/Magnum/XT/lightmaps.jpg",
        b"jpg",
    );
    write(&archive, "Flash/Turrets/New/Magnum/XT/notes.txt", b"hello");
    write(
        &archive,
        "Flash/Turrets/New/Magnum/XT/object.a3d",
        b"other client",
    );
    // Two pictures: only one has the size of a garage preview (a PNG header of 164 x 106).
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13];
    png.extend(b"IHDR");
    png.extend(164u32.to_be_bytes());
    png.extend(106u32.to_be_bytes());
    write(&archive, "Flash/Turrets/New/Magnum/Prime/object.3ds", b"m");
    write(&archive, "Flash/Turrets/New/Magnum/Prime/shot.png", &png);
    write(
        &archive,
        "Flash/Turrets/New/Magnum/Prime/other.png",
        b"not 164x106",
    );
    write(
        &archive,
        "Flash/Turrets/New/Magnum/Prime/.DS_Store",
        b"junk",
    );
    // Textures by a word in their names.
    write(&archive, "Flash/Turrets/New/Magnum/M3/object.3ds", b"m");
    write(
        &archive,
        "Flash/Turrets/New/Magnum/M3/Magnum_diffuse.png",
        b"png",
    );
    write(
        &archive,
        "Flash/Turrets/New/Magnum/M3/details_alpha.jpg",
        b"jpg",
    );

    let (summary, manifest) = build(&archive);
    let roles = |target: &str| -> Vec<(String, String)> {
        item(&manifest, target).variants[0]
            .resources
            .iter()
            .map(|r| (r.role.clone(), r.path.clone()))
            .collect()
    };
    let xt = roles("turret:magnum:xt");
    assert_eq!(
        xt,
        [
            (
                "model".to_owned(),
                "assets/turret/magnum/xt/new/object.3ds".to_owned()
            ),
            (
                "lightmap".to_owned(),
                "assets/turret/magnum/xt/new/lightmap.jpg".to_owned()
            ),
        ],
        "found by content and by name, stored under the usual names"
    );
    let m3: Vec<String> = roles("turret:magnum:m3").into_iter().map(|r| r.0).collect();
    assert_eq!(m3, ["model", "details", "details_alpha"]);
    assert_eq!(
        item(&manifest, "turret:magnum:prime").variants[0]
            .preview
            .as_deref(),
        Some("assets/turret/magnum/prime/new/preview.png"),
        "the picture of the preview size wins over another picture"
    );
    assert_eq!(summary.without_preview, 2, "the XT and the M3");
    assert_eq!(
        summary.unused,
        [
            "Flash/Turrets/New/Magnum/Prime/other.png",
            "Flash/Turrets/New/Magnum/XT/notes.txt"
        ],
        "junk is not worth a report, unknown files are"
    );
    assert_eq!(summary.unsupported.get("a3d").copied(), Some(1));
}

#[test]
fn a_pack_folder_is_not_an_archive_and_an_empty_one_is_refused() {
    let pack = scratch("manifest");
    write(&pack, "Flash/Hulls/Old/Wasp/M3/object.3ds", b"x");
    write(&pack, "manifest.json", b"{}");
    assert!(
        !is_model_archive(&pack),
        "a folder with a manifest is a pack"
    );
    let empty = scratch("empty");
    write(&empty, "Flash/Hulls/Old/Wasp/readme.txt", b"no models");
    assert!(!is_model_archive(&empty));
    assert!(build_model_pack(&empty, &scratch("out").join("p")).is_err());
    assert!(build_model_pack(&scratch("plain"), &scratch("out2").join("p")).is_err());
}

#[test]
fn the_same_archive_has_the_same_version_and_a_changed_one_a_new_one() {
    let archive = scratch("version");
    write(&archive, "Hulls/Wasp/M3/object.3ds", b"one");
    let (_, first) = build(&archive);
    let (_, again) = build(&archive);
    assert_eq!(first.version, again.version);
    write(&archive, "Hulls/Wasp/M3/object.3ds", b"two");
    let (_, changed) = build(&archive);
    assert_ne!(first.version, changed.version);
}
