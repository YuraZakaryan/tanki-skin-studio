// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The window, Dock and taskbar icon: the app mark from assets/app-icon (drawn in design.pen,
//! rendered by packaging/icons.sh), as WordCraft does. macOS gets the version on Apple's icon grid,
//! with its transparent margin, so it sits in the Dock like the icons beside it.

use eframe::egui::IconData;

#[cfg(target_os = "macos")]
const PNG: &[u8] = include_bytes!("../../../assets/app-icon/tanki-skin-studio-macos-512.png");
#[cfg(not(target_os = "macos"))]
const PNG: &[u8] = include_bytes!("../../../assets/app-icon/tanki-skin-studio-256.png");

/// The icon as RGBA pixels; `None` (and a warning) only if the bundled PNG cannot be decoded.
pub fn app_icon() -> Option<IconData> {
    eframe::icon_data::from_png_bytes(PNG)
        .map_err(|error| log::warn!("app icon: {error}"))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_icon_decodes_square_with_a_transparent_corner_and_a_solid_centre() {
        let icon = app_icon().expect("the bundled PNG decodes");
        assert_eq!(icon.width, icon.height);
        assert!(icon.width >= 256);
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert_eq!(
            icon.rgba[3], 0,
            "the corner outside the tile is transparent"
        );
        let centre = ((icon.height / 2 * icon.width + icon.width / 2) * 4) as usize;
        assert_eq!(icon.rgba[centre + 3], 255, "the tile is opaque");
    }
}
