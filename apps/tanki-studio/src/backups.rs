// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The backups as the app shows and keeps them: the size limit, the lines of the Backups page (a
//! run of "Apply to all items" is one line), deleting old ones, undoing one, and making room when
//! the disk is too full for the next backup. The files themselves are `tanki_apply::backups`.

use tanki_core::{
    apply::{Batch, restore_backup},
    backups::{self as store, Kept, RoomPlan, Stored, Usage},
    cache::CacheScan,
    pack::PackTarget,
};
use tanki_i18n::{fmt, tl, tn};

use crate::{
    actions::bulk_busy,
    app::{DeleteWhat, Dialog, Retry, Studio, Worn},
    bulk::Task,
    model::{format_bytes, grade_code, item_name},
};

/// Settings key of the limit: bytes, or `none`.
pub const LIMIT_KEY: &str = "backups.limit";
const GIB: u64 = 1024 * 1024 * 1024;
/// The limit a new install starts with: old backups beyond it go by themselves, and the ones
/// that take skins off are never touched by it.
pub const DEFAULT_LIMIT: u64 = 2 * GIB;
/// The limits offered, smallest first; `None` is no limit.
pub const LIMITS: [Option<u64>; 5] = [
    Some(GIB),
    Some(2 * GIB),
    Some(5 * GIB),
    Some(10 * GIB),
    None,
];

pub fn limit_code(limit: Option<u64>) -> String {
    limit.map_or_else(|| "none".to_owned(), |bytes| bytes.to_string())
}

/// The stored limit; anything unreadable gives the default.
pub fn limit_from_code(code: Option<&str>) -> Option<u64> {
    match code {
        Some("none") => None,
        Some(bytes) => Some(bytes.parse().unwrap_or(DEFAULT_LIMIT)),
        None => Some(DEFAULT_LIMIT),
    }
}

/// "2 GB", or "No limit": a limit is a whole number of gigabytes.
pub fn limit_label(limit: Option<u64>) -> String {
    match limit {
        None => tl!("No limit").to_owned(),
        Some(bytes) if bytes >= GIB && bytes % GIB == 0 => format!("{} GB", bytes / GIB),
        Some(bytes) => format_bytes(bytes),
    }
}

/// One line of the Backups page: a single replacement, or a whole run of "Apply to all items".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub what: LineKey,
    pub title: String,
    /// When it was made (Unix seconds).
    pub created: u64,
    pub files: usize,
    pub pack: String,
    pub bytes: u64,
    pub kept: Kept,
    /// How many items a run changed (1 for a single replacement).
    pub items: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LineKey {
    /// One replacement, by its transaction id.
    One(String),
    /// A run, by its batch.
    Run(Batch),
}

/// The lines of `stored`, newest first. A run is needed while one of its replacements is; it is
/// "taken off" only when every replacement is.
pub fn lines(stored: &[Stored], pack_name: impl Fn(&str) -> String) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for s in stored {
        let tx = &s.entry.tx;
        if let Some(batch) = &tx.batch
            && let Some(line) = lines
                .iter_mut()
                .find(|l| matches!(&l.what, LineKey::Run(b) if b.id == batch.id))
        {
            line.files += tx.changes.len();
            line.bytes += s.entry.bytes;
            line.items += 1;
            line.created = line.created.min(s.entry.created_unix);
            line.kept = match (line.kept, s.kept) {
                (Kept::Needed, _) | (_, Kept::Needed) => Kept::Needed,
                (Kept::TakenOff, Kept::TakenOff) => Kept::TakenOff,
                _ => Kept::Replaced,
            };
            continue;
        }
        let target = tx
            .target
            .as_ref()
            .map(|t| item_name(&t.kind, &t.id, &t.grade))
            .unwrap_or_default();
        let (what, title) = match &tx.batch {
            Some(batch) => (LineKey::Run(batch.clone()), batch.name.clone()),
            None if target.is_empty() => (LineKey::One(tx.id.clone()), tx.variant_name.clone()),
            None => (
                LineKey::One(tx.id.clone()),
                format!("{} · {target}", tx.variant_name),
            ),
        };
        lines.push(Line {
            what,
            title,
            created: s.entry.created_unix,
            files: tx.changes.len(),
            pack: pack_name(&tx.pack_id),
            bytes: s.entry.bytes,
            kept: s.kept,
            items: 1,
        });
    }
    for line in &mut lines {
        if let LineKey::Run(batch) = &line.what {
            // A run counts items, not the modifications it changed.
            let mut items: Vec<(&str, &str)> = Vec::new();
            for s in stored {
                if s.entry.tx.batch.as_ref().is_some_and(|b| b.id == batch.id)
                    && let Some(t) = &s.entry.tx.target
                    && !items.contains(&(t.kind.as_str(), t.id.as_str()))
                {
                    items.push((t.kind.as_str(), t.id.as_str()));
                }
            }
            line.items = items.len().max(1);
            line.title = format!(
                "{} · {}",
                batch.name,
                tn(line.items as u64, "{n} item", "{n} items")
            );
        }
    }
    lines
}

/// What the backups of `tasks` take at most.
pub fn tasks_need(scan: &CacheScan, tasks: &[Task]) -> u64 {
    tasks
        .iter()
        .flat_map(|task| {
            task.writes.iter().map(move |write| PackTarget {
                kind: task.kind.clone(),
                id: task.id.clone(),
                grade: grade_code(usize::from(write.modification)).to_owned(),
            })
        })
        .map(|target| store::backup_need(scan, &target))
        .sum()
}

/// The skin on now a line's backup takes off: an item's modification, or a run.
fn worn(line: &Line) -> Worn {
    match &line.what {
        LineKey::Run(batch) => Worn::Run {
            look: batch.name.clone(),
            items: line.items,
        },
        LineKey::One(_) => Worn::Item(
            line.title
                .rsplit(" · ")
                .next()
                .unwrap_or(&line.title)
                .to_owned(),
        ),
    }
}

fn line_has(line: &Line, stored: &Stored) -> bool {
    match &line.what {
        LineKey::One(id) => stored.entry.tx.id == *id,
        LineKey::Run(batch) => stored
            .entry
            .tx
            .batch
            .as_ref()
            .is_some_and(|b| b.id == batch.id),
    }
}

impl Studio {
    fn backup_root(&self) -> std::path::PathBuf {
        self.data.join("backups")
    }

    /// Free space where the backups go.
    pub fn free_space(&self) -> Option<u64> {
        self.free_space_override
            .or_else(|| store::free_space(&self.backup_root()))
    }

    /// The question to ask before writing `need` bytes of backups, when the disk is too full for
    /// them; `None` when there is room (or the system does not say how much there is).
    pub fn room_for(&self, need: u64, retry: Retry) -> Option<Dialog> {
        let free = self.free_space()?;
        let short = store::shortfall(need, free);
        if short == 0 {
            return None;
        }
        Some(Dialog::LowSpace {
            retry,
            need,
            free,
            plan: store::plan_room(&self.stored, short),
        })
    }

    /// The low-space dialog's confirm: delete what it named, then go on with what waited.
    pub fn make_room_and_retry(&mut self, retry: Retry, plan: RoomPlan) {
        self.dialog = None;
        match store::make_room(&self.backup_root(), &plan) {
            Ok(freed) => {
                if let Some(free) = self.free_space_override.as_mut() {
                    *free += freed.bytes;
                }
                self.reload_backups();
                match retry {
                    Retry::Apply(pending) => self.confirm_apply(*pending),
                    Retry::Bulk => self.bulk_apply(),
                }
            }
            Err(error) => {
                self.reload_backups();
                self.toast(crate::errors::user_message(&error), None);
            }
        }
    }

    /// Choose the limit; old backups over it go at once.
    pub fn set_backup_limit(&mut self, limit: Option<u64>) {
        self.backup_limit = limit;
        if let Err(error) = self.store.set(LIMIT_KEY, &limit_code(limit)) {
            log::warn!("could not save the backup limit: {error:#}");
        }
        self.keep_backups_under_limit();
    }

    /// Delete old backups over the limit, oldest first (after a replacement and when the limit
    /// changes). Backups that take skins off stay.
    pub fn keep_backups_under_limit(&mut self) {
        let Some(limit) = self.backup_limit else {
            return;
        };
        match store::keep_under(&self.backup_root(), limit) {
            Ok(freed) if freed.backups > 0 => self.reload_backups(),
            Ok(_) => {}
            Err(error) => log::warn!("could not keep the backups under the limit: {error:#}"),
        }
    }

    /// Delete old backups: say how many and how much first.
    pub fn ask_to_delete_old(&mut self) {
        self.reload_backups();
        let usage = Usage::of(&self.stored);
        if usage.old == 0 {
            return self.toast(tl!("No old backups"), None);
        }
        self.dialog = Some(Dialog::DeleteOld {
            backups: usage.old,
            bytes: usage.old_bytes,
        });
    }

    pub fn delete_old_confirmed(&mut self) {
        self.dialog = None;
        match store::delete_old(&self.backup_root()) {
            Ok(freed) => self.toast(
                fmt(tl!("Freed {size}"), &[("size", &format_bytes(freed.bytes))]),
                None,
            ),
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
        self.reload_backups();
    }

    /// The Backups page's deletes (a line's trash, a day's Delete day, Delete all): say what goes
    /// and, when backups of skins on now are among it, which skins, then ask.
    pub fn ask_to_delete(&mut self, what: DeleteWhat, lines: &[Line]) {
        let stored: Vec<&Stored> = self
            .stored
            .iter()
            .filter(|s| lines.iter().any(|line| line_has(line, s)))
            .collect();
        if stored.is_empty() {
            return;
        }
        let mut names: Vec<Worn> = Vec::new();
        for line in lines.iter().filter(|line| !line.kept.is_old()) {
            let name = worn(line);
            if !names.contains(&name) {
                names.push(name);
            }
        }
        self.dialog = Some(Dialog::DeleteBackups {
            what,
            dirs: stored.iter().map(|s| s.entry.dir.clone()).collect(),
            backups: lines.len(),
            bytes: stored.iter().map(|s| s.entry.bytes).sum(),
            worn: names,
        });
    }

    /// Delete all: every line of the page.
    pub fn ask_to_delete_all(&mut self) {
        let all = lines(&self.stored, |id| id.to_owned());
        self.ask_to_delete(DeleteWhat::All, &all);
    }

    /// The confirmed [`Dialog::DeleteBackups`].
    pub fn delete_backups_confirmed(&mut self, dirs: &[std::path::PathBuf]) {
        self.dialog = None;
        let root = self.backup_root();
        let mut freed = 0;
        for dir in dirs {
            match store::delete(&root, dir) {
                Ok(bytes) => freed += bytes,
                Err(error) => {
                    self.toast(crate::errors::user_message(&error), None);
                    break;
                }
            }
        }
        if freed > 0 {
            self.toast(
                fmt(tl!("Freed {size}"), &[("size", &format_bytes(freed))]),
                None,
            );
        }
        self.reload_backups();
    }

    /// A line's Undo: a run asks first (it takes many skins off), one replacement goes at once.
    pub fn undo_line(&mut self, line: &Line) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        match &line.what {
            LineKey::Run(batch) => self.ask_to_restore_batch(&batch.id),
            LineKey::One(id) => self.undo_backup(id),
        }
    }

    fn undo_backup(&mut self, id: &str) {
        if !self.running_game().is_empty() {
            return self.toast(
                tl!("Close the game before restoring the original files."),
                None,
            );
        }
        match restore_backup(&self.backup_root(), id) {
            Ok(summary) => {
                self.toast(
                    fmt(
                        tl!("Restored {n} files. Restart the game."),
                        &[("n", &summary.files.to_string())],
                    ),
                    None,
                );
                self.rescan_quietly();
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
        self.reload_backups();
    }
}

#[cfg(test)]
mod tests;
