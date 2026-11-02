// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::apply::{FileChange, Transaction};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Lab {
    root: PathBuf,
    backups: PathBuf,
    cache: PathBuf,
}

impl Lab {
    fn new(tag: &str) -> Lab {
        let root = std::env::temp_dir().join(format!(
            "tanki-backups-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let backups = root.join("backups");
        let cache = root.join("cache");
        fs::create_dir_all(&backups).unwrap();
        fs::create_dir_all(&cache).unwrap();
        Lab {
            root,
            backups,
            cache,
        }
    }

    /// A replacement of `file` from `before` to `after`, as apply writes it.
    fn backup(&self, id: u64, file: &str, before: &[u8], after: &[u8], restored: bool) {
        let dir = self.backups.join(id.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0000.bin"), before).unwrap();
        let tx = Transaction {
            id: id.to_string(),
            pack_id: "p".into(),
            variant_id: "v".into(),
            changes: vec![FileChange {
                target: self.cache.join(file),
                backup_name: "0000.bin".into(),
                before_sha256: sha256(before),
                after_sha256: sha256(after),
            }],
            target: None,
            variant_name: "V".into(),
            batch: None,
            skin_grade: None,
        };
        fs::write(
            dir.join("transaction.json"),
            serde_json::to_vec(&tx).unwrap(),
        )
        .unwrap();
        if restored {
            fs::write(dir.join("restored"), id.to_string()).unwrap();
        }
    }

    fn kinds(&self) -> Vec<(String, Kept)> {
        survey(&self.backups)
            .unwrap()
            .into_iter()
            .map(|s| (s.entry.tx.id.clone(), s.kept))
            .collect()
    }
}

impl Drop for Lab {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn a_backup_is_needed_while_its_skin_is_on_and_old_otherwise() {
    let lab = Lab::new("kinds");
    fs::write(lab.cache.join("a"), b"skin-a2").unwrap();
    fs::write(lab.cache.join("b"), b"game update").unwrap();
    fs::write(lab.cache.join("c"), b"orig-c").unwrap();
    // a: two skins in a row; the older one leads back to the original.
    lab.backup(1, "a", b"orig-a", b"skin-a1", false);
    lab.backup(2, "a", b"skin-a1", b"skin-a2", false);
    // b: the game wrote the file since.
    lab.backup(3, "b", b"orig-b", b"skin-b", false);
    // c: taken off.
    lab.backup(4, "c", b"orig-c", b"skin-c", true);
    assert_eq!(
        lab.kinds(),
        vec![
            ("4".into(), Kept::TakenOff),
            ("3".into(), Kept::Replaced),
            ("2".into(), Kept::Needed),
            ("1".into(), Kept::Needed),
        ]
    );
}

#[test]
fn deleting_old_backups_keeps_every_needed_one() {
    let lab = Lab::new("old");
    fs::write(lab.cache.join("a"), b"skin-a").unwrap();
    fs::write(lab.cache.join("b"), b"orig-b").unwrap();
    lab.backup(1, "a", b"orig-a", b"skin-a", false);
    lab.backup(2, "b", b"orig-b", b"skin-b", true);
    let freed = delete_old(&lab.backups).unwrap();
    assert_eq!(freed.backups, 1);
    assert!(freed.bytes > 0);
    assert_eq!(lab.kinds(), vec![("1".into(), Kept::Needed)]);
}

#[test]
fn the_limit_deletes_the_oldest_old_backups_and_never_a_needed_one() {
    let lab = Lab::new("limit");
    fs::write(lab.cache.join("a"), b"skin-a").unwrap();
    lab.backup(1, "x", &[0; 1000], b"gone", true);
    lab.backup(2, "a", &[0; 1000], b"skin-a", false);
    lab.backup(3, "y", &[0; 1000], b"gone", true);
    // Room for two of the three: the oldest old one goes.
    let freed = keep_under(&lab.backups, 2600).unwrap();
    assert_eq!(freed.backups, 1);
    let ids: Vec<_> = lab.kinds().into_iter().map(|(id, _)| id).collect();
    assert_eq!(ids, vec!["3", "2"]);
    // A limit below the needed one alone deletes every old one and keeps the needed one.
    keep_under(&lab.backups, 10).unwrap();
    assert_eq!(lab.kinds(), vec![("2".into(), Kept::Needed)]);
}

#[test]
fn making_room_takes_old_backups_first_and_needed_ones_only_when_short() {
    let lab = Lab::new("room");
    fs::write(lab.cache.join("a"), b"skin-a").unwrap();
    fs::write(lab.cache.join("b"), b"skin-b").unwrap();
    lab.backup(1, "a", &[0; 500], b"skin-a", false);
    lab.backup(2, "b", &[0; 500], b"skin-b", false);
    lab.backup(3, "x", &[0; 300], b"gone", true);
    let stored = survey(&lab.backups).unwrap();
    let small = plan_room(&stored, 200);
    assert!(small.enough);
    assert_eq!((small.old.len(), small.needed.len()), (1, 0));
    let big = plan_room(&stored, 700);
    assert!(big.enough);
    assert_eq!((big.old.len(), big.needed.len()), (1, 1));
    assert_eq!(
        big.needed[0].tx.id, "1",
        "the oldest needed backup goes first"
    );
    let too_big = plan_room(&stored, 10_000);
    assert!(!too_big.enough);
    let freed = make_room(&lab.backups, &big).unwrap();
    assert_eq!(freed.backups, 2);
    assert_eq!(lab.kinds(), vec![("2".into(), Kept::Needed)]);
}

#[test]
fn undoing_one_backup_also_undoes_what_sits_on_top_of_it() {
    let lab = Lab::new("undo");
    fs::write(lab.cache.join("a"), b"skin-a2").unwrap();
    lab.backup(1, "a", b"orig-a", b"skin-a1", false);
    lab.backup(2, "a", b"skin-a1", b"skin-a2", false);
    crate::apply::restore_backup(&lab.backups, "2").unwrap();
    assert_eq!(fs::read(lab.cache.join("a")).unwrap(), b"skin-a1");
    lab.backup(3, "a", b"skin-a1", b"skin-a3", false);
    fs::write(lab.cache.join("a"), b"skin-a3").unwrap();
    let summary = crate::apply::restore_backup(&lab.backups, "1").unwrap();
    assert_eq!(
        summary.replacements, 2,
        "the one on top first, then the one asked for"
    );
    assert_eq!(fs::read(lab.cache.join("a")).unwrap(), b"orig-a");
    assert!(lab.kinds().iter().all(|(_, kept)| *kept == Kept::TakenOff));
}

#[test]
fn only_a_backup_folder_can_be_deleted() {
    let lab = Lab::new("guard");
    let outside = lab.root.join("keep");
    fs::create_dir_all(&outside).unwrap();
    assert!(delete(&lab.backups, &outside).is_err());
    let named = lab.backups.join("notes");
    fs::create_dir_all(&named).unwrap();
    assert!(delete(&lab.backups, &named).is_err());
    assert!(outside.is_dir() && named.is_dir());
}

#[test]
fn shortfall_keeps_a_spare() {
    assert_eq!(shortfall(100, SPARE + 100), 0);
    assert_eq!(shortfall(100, 50), SPARE + 50);
    assert!(free_space(&std::env::temp_dir()).is_some_and(|b| b > 0));
}
