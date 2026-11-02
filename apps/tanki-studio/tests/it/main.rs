// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! UI tests that drive the real app in-process with `egui_kittest`: look a
//! widget up by its accessible label, click it, run a few frames, and check the app's state and the
//! files on disk. No window is opened, so these run on every machine in `cargo test`.

mod archive;
mod bulk;
mod caches;
mod chaos;
mod common;
mod dialogs;
mod garage;
mod import;
mod monkey;
mod navigation;
mod preferences;
mod recognise;
mod resources;
mod updates;
