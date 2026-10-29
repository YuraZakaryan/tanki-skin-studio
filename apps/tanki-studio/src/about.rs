// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! What the app says about itself: its version, its licence, that it comes with no warranty and
//! where its source is. GPL-3.0 asks an interactive program to show these "Appropriate Legal
//! Notices"; the splash screen and the About command show them.

use tanki_i18n::{fmt, tl};

/// The licence of the app's own code (the bundled fonts and icons keep theirs).
pub const LICENSE: &str = "GPL-3.0-or-later";
/// Where the source of every version is.
pub const SOURCE: &str = "github.com/YuraZakaryan/tanki-skin-studio";

/// The legal notice, in the interface language.
pub fn notice() -> String {
    fmt(
        tl!(
            "Tanki Skin Studio {version} is free software under {license}, with no warranty. Source: {source}"
        ),
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            ("license", LICENSE),
            ("source", SOURCE),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_notice_names_version_licence_warranty_and_source() {
        let notice = notice();
        for part in [env!("CARGO_PKG_VERSION"), LICENSE, SOURCE, "no warranty"] {
            assert!(notice.contains(part), "{notice:?} lacks {part:?}");
        }
    }

    #[test]
    fn the_licence_matches_the_workspace_manifest() {
        assert_eq!(env!("CARGO_PKG_LICENSE"), LICENSE);
    }
}
