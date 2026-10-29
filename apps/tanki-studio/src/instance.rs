// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! One Tanki Skin Studio per data folder. A second window on the same data would write the same
//! database and the same game cache at once (two applies, two restores racing).
//!
//! As PdfCraft does for its launch election, the guard is an exclusive OS lock on a file: the OS
//! drops it when the process ends, however it ends, so a crash never leaves a stale lock, and a
//! failure to take it lets the app run rather than refusing to start. Different data folders (the
//! tests, `TANKI_DATA_DIR`) never block each other.

use std::{
    fs::{File, OpenOptions, TryLockError},
    path::Path,
};

/// Held for the life of the process; dropping it frees the data folder.
#[derive(Debug)]
pub struct InstanceLock(#[allow(dead_code)] File);

/// Take the data folder for this process. `Ok(None)`: another Tanki Skin Studio holds it.
pub fn claim(data: &Path) -> std::io::Result<Option<InstanceLock>> {
    std::fs::create_dir_all(data)?;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(data.join("instance.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(InstanceLock(file))),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> std::path::PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "tanki-instance-{label}-{}-{stamp}",
            std::process::id()
        ))
    }

    #[test]
    fn a_second_window_on_the_same_data_is_refused_until_the_first_ends() {
        let data = scratch("same");
        let first = claim(&data).unwrap();
        assert!(first.is_some(), "the first launch takes the folder");
        assert!(
            claim(&data).unwrap().is_none(),
            "a second one on the same data is refused"
        );
        drop(first);
        assert!(
            claim(&data).unwrap().is_some(),
            "free again once the first ends"
        );
    }

    #[test]
    fn different_data_folders_never_block_each_other() {
        let a = claim(&scratch("a")).unwrap();
        let b = claim(&scratch("b")).unwrap();
        assert!(a.is_some() && b.is_some());
    }

    #[test]
    fn a_lock_file_left_behind_is_no_lock() {
        // A crashed run leaves the file but not the OS lock.
        let data = scratch("left");
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(data.join("instance.lock"), b"").unwrap();
        assert!(claim(&data).unwrap().is_some());
    }
}
