// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Skins that go on later. "Apply to all items" puts a look on a modification the game has not
//! downloaded yet as soon as it appears in the cache: the choice is queued (see
//! `tanki_core::later`), the cache is read again on its own (see [`RescanMode`]), and the queued
//! skins whose modification is now there go on through the same transactions as a run.
//!
//! Cache file names carry a revision only the server knows, so nothing is written ahead: the skin
//! waits for the game's own files.

use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

use tanki_core::later::LaterSkin;

use crate::{
    bulk::{LookSkin, Task, Write, split_grade},
    model::{BaseItem, target_title},
};

/// How often the cache is read again while the window is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RescanMode {
    /// At launch, when the window comes back to the front, and every 5 minutes.
    Auto,
    /// Every hour.
    Hourly,
    /// Only with Rescan.
    Manual,
}

impl RescanMode {
    pub const ALL: [RescanMode; 3] = [RescanMode::Auto, RescanMode::Hourly, RescanMode::Manual];

    pub fn code(self) -> &'static str {
        match self {
            RescanMode::Auto => "auto",
            RescanMode::Hourly => "hourly",
            RescanMode::Manual => "manual",
        }
    }

    /// The stored choice; nothing (or anything unknown) is Automatic.
    pub fn from_code(code: Option<&str>) -> Self {
        RescanMode::ALL
            .into_iter()
            .find(|m| Some(m.code()) == code)
            .unwrap_or(RescanMode::Auto)
    }

    /// How long between two looks at the cache folder; `None`: never on its own.
    pub fn every(self) -> Option<Duration> {
        match self {
            RescanMode::Auto => Some(Duration::from_secs(5 * 60)),
            RescanMode::Hourly => Some(Duration::from_secs(60 * 60)),
            RescanMode::Manual => None,
        }
    }

    /// Whether coming back to the window looks at the cache.
    pub fn on_focus(self) -> bool {
        self == RescanMode::Auto
    }
}

/// What the cache folder looks like, cheaply: how many entries it holds and when the newest one
/// changed (Unix seconds). Two equal stamps mean nothing to read again. Looks as deep as a scan.
pub fn stamp(root: &Path) -> Option<(u64, u64)> {
    fn walk(dir: &Path, depth: usize, count: &mut u64, newest: &mut u64) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            *count += 1;
            let changed = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            *newest = (*newest).max(changed);
            if meta.is_dir() && depth < 8 {
                walk(&entry.path(), depth + 1, count, newest);
            }
        }
    }
    if !root.is_dir() {
        return None;
    }
    let (mut count, mut newest) = (0, 0);
    walk(root, 0, &mut count, &mut newest);
    Some((count, newest))
}

/// The queued skins of the cache at `root` whose modification is in the cache now.
pub fn due<'a>(queue: &'a [LaterSkin], root: &str, items: &[BaseItem]) -> Vec<&'a LaterSkin> {
    queue
        .iter()
        .filter(|skin| skin.root == root)
        .filter(|skin| {
            items.iter().any(|item| {
                item.kind == skin.kind
                    && item.id == skin.id
                    && item
                        .present
                        .get(usize::from(skin.modification))
                        .copied()
                        .unwrap_or(false)
            })
        })
        .collect()
}

/// The queued skins of the cache at `root` (for the Packs list and the Item page).
pub fn of_cache<'a>(queue: &'a [LaterSkin], root: &str) -> Vec<&'a LaterSkin> {
    queue.iter().filter(|skin| skin.root == root).collect()
}

/// The queued skin of one item's modification, if any.
pub fn waiting<'a>(
    queue: &'a [LaterSkin],
    root: &str,
    kind: &str,
    id: &str,
    modification: u8,
) -> Option<&'a LaterSkin> {
    queue.iter().find(|skin| {
        skin.root == root && skin.kind == kind && skin.id == id && skin.modification == modification
    })
}

/// The skins as a run's tasks: one per item, in the queue's order.
pub fn tasks(due: &[&LaterSkin]) -> Vec<Task> {
    let mut tasks: Vec<Task> = Vec::new();
    for skin in due {
        let write = Write {
            modification: skin.modification,
            skin: LookSkin {
                pack_id: skin.pack_id.clone(),
                pack_version: skin.pack_version.clone(),
                pack_name: skin.pack_name.clone(),
                item: skin.item,
                variant: skin.variant,
                variant_id: skin.variant_id.clone(),
                grade: skin.grade.clone(),
                source: split_grade(&skin.grade).0,
            },
        };
        match tasks
            .iter_mut()
            .find(|t| t.kind == skin.kind && t.id == skin.id)
        {
            Some(task) => task.writes.push(write),
            None => tasks.push(Task {
                kind: skin.kind.clone(),
                id: skin.id.clone(),
                writes: vec![write],
            }),
        }
    }
    tasks
}

/// A queue entry for one cell of a plan that goes on later.
pub fn entry(root: &str, kind: &str, id: &str, write: &Write, look: &str, now: u64) -> LaterSkin {
    LaterSkin {
        root: root.to_owned(),
        kind: kind.to_owned(),
        id: id.to_owned(),
        modification: write.modification,
        pack_id: write.skin.pack_id.clone(),
        pack_version: write.skin.pack_version.clone(),
        pack_name: write.skin.pack_name.clone(),
        item: write.skin.item,
        variant: write.skin.variant,
        variant_id: write.skin.variant_id.clone(),
        grade: write.skin.grade.clone(),
        look: look.to_owned(),
        added: now,
    }
}

/// "Dictator M0".
pub fn label(skin: &LaterSkin) -> String {
    format!("{} M{}", target_title(&skin.id), skin.modification)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, present: [bool; 4]) -> BaseItem {
        BaseItem {
            kind: "hull".into(),
            id: id.into(),
            present,
            markers: [None, None, None, None],
        }
    }

    fn skin(root: &str, id: &str, modification: u8) -> LaterSkin {
        LaterSkin {
            root: root.into(),
            kind: "hull".into(),
            id: id.into(),
            modification,
            pack_id: "desert".into(),
            grade: "xt-m3".into(),
            look: "XT".into(),
            ..LaterSkin::default()
        }
    }

    #[test]
    fn a_skin_is_due_once_its_modification_is_in_this_cache() {
        let queue = vec![
            skin("/a", "dictator", 0),
            skin("/a", "titan", 1),
            skin("/b", "dictator", 0),
        ];
        let items = [
            item("dictator", [true, true, true, true]),
            item("titan", [true, false, false, true]),
        ];
        let due = due(&queue, "/a", &items);
        assert_eq!(
            due.len(),
            1,
            "Titan M1 is not there yet, the other cache does not count"
        );
        assert_eq!(label(due[0]), "Dictator M0");
    }

    #[test]
    fn skins_of_one_item_are_one_task() {
        let queue = [
            skin("/a", "titan", 1),
            skin("/a", "dictator", 0),
            skin("/a", "titan", 2),
        ];
        let refs: Vec<&LaterSkin> = queue.iter().collect();
        let tasks = tasks(&refs);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].id, "titan");
        assert_eq!(
            tasks[0]
                .writes
                .iter()
                .map(|w| w.modification)
                .collect::<Vec<_>>(),
            [1, 2]
        );
        assert_eq!(tasks[0].writes[0].skin.source, Some(3), "made for M3");
    }

    #[test]
    fn the_rescan_choice_reads_back_and_defaults_to_automatic() {
        for mode in RescanMode::ALL {
            assert_eq!(RescanMode::from_code(Some(mode.code())), mode);
        }
        assert_eq!(RescanMode::from_code(None), RescanMode::Auto);
        assert_eq!(RescanMode::from_code(Some("weekly")), RescanMode::Auto);
        assert!(RescanMode::Manual.every().is_none());
        assert!(RescanMode::Auto.on_focus() && !RescanMode::Hourly.on_focus());
    }

    #[test]
    fn the_stamp_changes_when_the_game_adds_a_file() {
        let dir = std::env::temp_dir().join(format!("tanki-later-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let before = stamp(&dir).unwrap();
        std::fs::write(dir.join("sub").join("new"), b"x").unwrap();
        assert_ne!(stamp(&dir), Some(before));
        assert_eq!(stamp(&dir.join("missing")), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
