// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::path::PathBuf;

/// `TANKI_DATA_DIR` overrides the data folder so tests never touch real user state.
pub fn app_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("TANKI_DATA_DIR") {
        return PathBuf::from(dir);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/TankiSkinStudio");
    }
    #[cfg(target_os = "windows")]
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata).join("TankiSkinStudio");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tanki-skin-studio")
}
