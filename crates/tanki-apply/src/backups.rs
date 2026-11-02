// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Housekeeping of the backups [`crate::apply`] writes: which ones are still needed to take a skin
//! off and which are only history, deleting the old ones, a size limit, and making room on a full
//! disk. A backup that is needed is only ever deleted through a [`RoomPlan`] the user agreed to.

use anyhow::{Context, Result};
use std::{collections::HashMap, fs, path::Path};
use tanki_cache::cache::{CacheScan, known_target_group};
use tanki_pack::pack::PackTarget;

use crate::apply::{BackupEntry, list_backups, sha256};

/// What a stored backup is still good for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// Its skin is in the cache: the backup is what takes it off.
    Needed,
    /// The skin was taken off; the backup is history.
    TakenOff,
    /// The game (or another tool) wrote the files since: the skin is gone and putting these
    /// originals back would bring back older game files.
    Replaced,
}

impl Kept {
    pub fn is_old(self) -> bool {
        self != Kept::Needed
    }
}

/// A backup and what it is good for.
#[derive(Clone, Debug)]
pub struct Stored {
    pub entry: BackupEntry,
    pub kept: Kept,
}

/// Every readable backup, newest first, each with what it is still good for. A backup is needed
/// while one of its files is still what the newest replacement of that file wrote: older
/// replacements of the same file are needed too, they lead back to the original.
pub fn survey(backup_root: &Path) -> Result<Vec<Stored>> {
    let entries = list_backups(backup_root)?;
    let mut top: HashMap<&Path, &str> = HashMap::new();
    for entry in entries.iter().filter(|e| !e.restored) {
        for change in &entry.tx.changes {
            top.entry(change.target.as_path())
                .or_insert(change.after_sha256.as_str());
        }
    }
    let intact: HashMap<&Path, bool> = top
        .iter()
        .map(|(path, after)| {
            let same = fs::read(path).is_ok_and(|bytes| sha256(&bytes) == *after);
            (*path, same)
        })
        .collect();
    Ok(entries
        .iter()
        .map(|entry| {
            let kept = if entry.restored {
                Kept::TakenOff
            } else if entry
                .tx
                .changes
                .iter()
                .any(|c| intact.get(c.target.as_path()).copied().unwrap_or(false))
            {
                Kept::Needed
            } else {
                Kept::Replaced
            };
            Stored {
                entry: entry.clone(),
                kept,
            }
        })
        .collect())
}

/// How much the backups take, needed and old.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub needed_bytes: u64,
    pub needed: usize,
    pub old_bytes: u64,
    pub old: usize,
}

impl Usage {
    pub fn of(stored: &[Stored]) -> Usage {
        let mut usage = Usage::default();
        for s in stored {
            if s.kept.is_old() {
                usage.old += 1;
                usage.old_bytes += s.entry.bytes;
            } else {
                usage.needed += 1;
                usage.needed_bytes += s.entry.bytes;
            }
        }
        usage
    }

    pub fn total(&self) -> u64 {
        self.needed_bytes + self.old_bytes
    }
}

/// What deleting freed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Freed {
    pub backups: usize,
    pub bytes: u64,
}

/// Delete one backup folder. Only a folder directly in `backup_root` with a numeric name (the
/// shape [`crate::apply`] gives them) is deleted, so a stray path can never take anything else.
pub fn delete(backup_root: &Path, dir: &Path) -> Result<u64> {
    let root = backup_root
        .canonicalize()
        .context("the backups folder is not there")?;
    let dir = dir.canonicalize().context("the backup is not there")?;
    anyhow::ensure!(
        dir.parent() == Some(root.as_path())
            && dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
        "not a backup folder: {}",
        dir.display()
    );
    let bytes = folder_bytes(&dir);
    fs::remove_dir_all(&dir).with_context(|| format!("could not delete {}", dir.display()))?;
    Ok(bytes)
}

fn folder_bytes(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

/// Delete every old backup (taken off, or replaced by the game). Nothing changes in the game.
pub fn delete_old(backup_root: &Path) -> Result<Freed> {
    let mut freed = Freed::default();
    for s in survey(backup_root)?.into_iter().filter(|s| s.kept.is_old()) {
        freed.bytes += delete(backup_root, &s.entry.dir)?;
        freed.backups += 1;
    }
    Ok(freed)
}

/// Keep the backups under `limit` bytes by deleting old ones, the oldest first. Backups that are
/// needed are never deleted here: when they alone are over the limit, the limit stays exceeded.
pub fn keep_under(backup_root: &Path, limit: u64) -> Result<Freed> {
    let stored = survey(backup_root)?;
    let mut total = Usage::of(&stored).total();
    let mut freed = Freed::default();
    // Newest first: the oldest is at the end.
    for s in stored.iter().rev().filter(|s| s.kept.is_old()) {
        if total <= limit {
            break;
        }
        freed.bytes += delete(backup_root, &s.entry.dir)?;
        freed.backups += 1;
        total = total.saturating_sub(s.entry.bytes);
    }
    Ok(freed)
}

/// What to delete to free `shortfall` bytes: old backups first, oldest first; then, only if that
/// is not enough, the oldest needed backups, whose skins can then no longer be taken off.
#[derive(Clone, Debug, Default)]
pub struct RoomPlan {
    pub old: Vec<BackupEntry>,
    pub old_bytes: u64,
    pub needed: Vec<BackupEntry>,
    pub needed_bytes: u64,
    /// Whether the plan frees the shortfall.
    pub enough: bool,
}

impl RoomPlan {
    pub fn bytes(&self) -> u64 {
        self.old_bytes + self.needed_bytes
    }
}

pub fn plan_room(stored: &[Stored], shortfall: u64) -> RoomPlan {
    let mut plan = RoomPlan::default();
    for s in stored.iter().rev().filter(|s| s.kept.is_old()) {
        if plan.bytes() >= shortfall {
            break;
        }
        plan.old_bytes += s.entry.bytes;
        plan.old.push(s.entry.clone());
    }
    for s in stored.iter().rev().filter(|s| !s.kept.is_old()) {
        if plan.bytes() >= shortfall {
            break;
        }
        plan.needed_bytes += s.entry.bytes;
        plan.needed.push(s.entry.clone());
    }
    plan.enough = plan.bytes() >= shortfall;
    plan
}

/// Carry out a plan the user agreed to.
pub fn make_room(backup_root: &Path, plan: &RoomPlan) -> Result<Freed> {
    let mut freed = Freed::default();
    for entry in plan.old.iter().chain(&plan.needed) {
        freed.bytes += delete(backup_root, &entry.dir)?;
        freed.backups += 1;
    }
    Ok(freed)
}

/// Free space on the disk that holds `path` (the nearest existing folder up from it), or `None`
/// when the system does not say.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut at = path;
    while !at.exists() {
        at = at.parent()?;
    }
    let at = at.canonicalize().ok()?;
    let disks = sysinfo::Disks::new_with_refreshed_list();
    disks
        .list()
        .iter()
        .filter(|d| at.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// What a backup of `target` takes at most: the cache files of the item, whose originals are kept.
pub fn backup_need(scan: &CacheScan, target: &PackTarget) -> u64 {
    scan.resources
        .iter()
        .filter(|r| {
            known_target_group(&scan.game, &r.group)
                == Some((
                    target.kind.as_str(),
                    target.id.as_str(),
                    target.grade.as_str(),
                ))
        })
        .filter_map(|r| fs::metadata(&r.path).ok())
        .map(|m| m.len())
        .sum()
}

/// A little room is kept free on top of a backup: the system and the game need some too.
pub const SPARE: u64 = 64 * 1024 * 1024;

/// How many bytes are missing to write a backup of `need` bytes with `free` available.
pub fn shortfall(need: u64, free: u64) -> u64 {
    (need + SPARE).saturating_sub(free)
}

#[cfg(test)]
mod tests;
