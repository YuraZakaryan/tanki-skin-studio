// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The progress of one update, shared between the thread that downloads and installs it and the
//! one that shows it.

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

use anyhow::Result;

/// What the update is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum UpdateStage {
    /// Downloading the release's file.
    Downloading,
    /// Checking the download against the release's checksums.
    Verifying,
    /// Putting the new version in place.
    Installing,
}

/// The error of an update stopped by [`UpdateProgress::cancel`], told apart from a real failure
/// with `error.is::<Cancelled>()`.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("update cancelled")
    }
}

impl std::error::Error for Cancelled {}

/// Bytes downloaded of the file's size, the stage, and a request to stop.
#[derive(Debug, Default)]
pub struct UpdateProgress {
    done: AtomicU64,
    total: AtomicU64,
    stage: AtomicU8,
    cancel: AtomicBool,
}

impl UpdateProgress {
    /// Progress frozen at a point, for a screenshot of the update dialog and for tests.
    pub fn at(stage: UpdateStage, done: u64, total: u64) -> Self {
        let progress = Self::default();
        progress.set_stage(stage);
        progress.set_total(total);
        progress.advance(done);
        progress
    }

    /// The part downloaded, 0..=1, or `None` while the size is not known yet.
    pub fn fraction(&self) -> Option<f32> {
        let (done, total) = self.bytes();
        (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0) as f32)
    }

    /// Bytes downloaded and the file's size.
    pub fn bytes(&self) -> (u64, u64) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    pub fn stage(&self) -> UpdateStage {
        match self.stage.load(Ordering::Relaxed) {
            1 => UpdateStage::Verifying,
            2 => UpdateStage::Installing,
            _ => UpdateStage::Downloading,
        }
    }

    /// Ask the update to stop. The download stops at its next block and leaves nothing behind;
    /// once the new version is being put in place, the update finishes.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub(crate) fn set_stage(&self, stage: UpdateStage) {
        self.stage.store(stage as u8, Ordering::Relaxed);
    }

    pub(crate) fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::Relaxed);
    }

    pub(crate) fn advance(&self, bytes: u64) {
        self.done.fetch_add(bytes, Ordering::Relaxed);
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(Cancelled.into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reports_the_part_done_and_the_stage() {
        let progress = UpdateProgress::at(UpdateStage::Downloading, 25, 100);
        assert_eq!(progress.fraction(), Some(0.25));
        assert_eq!(progress.stage(), UpdateStage::Downloading);
        progress.set_stage(UpdateStage::Installing);
        assert_eq!(progress.stage(), UpdateStage::Installing);
        assert_eq!(UpdateProgress::default().fraction(), None);
    }

    #[test]
    fn a_cancelled_update_stops_at_its_next_check() {
        let progress = UpdateProgress::default();
        assert!(progress.check().is_ok());
        progress.cancel();
        assert!(progress.check().unwrap_err().is::<Cancelled>());
    }
}
