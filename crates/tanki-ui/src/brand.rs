// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Other brands' own marks, for links to their services, used as their owners allow: unchanged
//! (no recolouring, cropping or stretching), at their own proportions and with their clear space.
//! Telegram's logo is the one Telegram offers for such buttons (telegram.org/tour/screenshots);
//! YouTube's icon is the one its brand guidelines give for linking to a channel beside other
//! social icons (brand.youtube). They are trademarks of their owners; the app is not affiliated
//! with them, and says so where it shows them. `docs/brand-marks.md` says where each file came from.

use egui::Vec2;

/// Telegram's logo, rendered from the official `Logo.svg` at 112 px.
pub const TELEGRAM: &[u8] = include_bytes!("../assets/brand/telegram.png");
/// YouTube's full-colour icon, the official digital PNG scaled down; it carries its clear space.
pub const YOUTUBE: &[u8] = include_bytes!("../assets/brand/youtube.png");

/// How large Telegram's logo is drawn beside a row's text.
pub const TELEGRAM_SIZE: Vec2 = Vec2::splat(28.0);
/// How large YouTube's icon is drawn: the official file's proportions, 36 points wide.
pub const YOUTUBE_SIZE: Vec2 = Vec2::new(36.0, 36.0 * 144.0 / 168.0);

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG's width and height, from its header.
    fn png_size(bytes: &[u8]) -> (u32, u32) {
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
        (word(16), word(20))
    }

    #[test]
    fn the_marks_are_drawn_at_their_own_proportions_and_sharp() {
        for (bytes, size) in [(TELEGRAM, TELEGRAM_SIZE), (YOUTUBE, YOUTUBE_SIZE)] {
            let (width, height) = png_size(bytes);
            let ratio = width as f32 / height as f32;
            assert!((ratio - size.x / size.y).abs() < 0.001, "not stretched");
            assert!(
                width as f32 >= size.x * 3.0,
                "sharp on a high-density screen"
            );
        }
    }
}
