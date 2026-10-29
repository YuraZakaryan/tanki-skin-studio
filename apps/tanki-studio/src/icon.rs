// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The application icon (dock, taskbar, Alt-Tab), drawn at start-up so the repository carries no
//! binary artwork: a rounded dark tile with a lime "T" in the style of the interface.

use eframe::egui::IconData;

const SIZE: u32 = 256;

fn inside_rounded(
    x: f32,
    y: f32,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    radius: f32,
) -> bool {
    if x < left || x > right || y < top || y > bottom {
        return false;
    }
    let cx = x.clamp(left + radius, right - radius);
    let cy = y.clamp(top + radius, bottom - radius);
    (x - cx).powi(2) + (y - cy).powi(2) <= radius * radius
}

/// The icon as RGBA pixels. Edges are anti-aliased by 3 × 3 supersampling.
pub fn app_icon() -> IconData {
    let tile = [0x10_u8, 0x17, 0x12];
    let rim = [0x3A_u8, 0x52, 0x40];
    let lime = [0x8C_u8, 0xFF, 0x4D];
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for py in 0..SIZE {
        for px in 0..SIZE {
            let (mut sum, mut covered) = ([0.0_f32; 3], 0.0_f32);
            for sy in 0..3 {
                for sx in 0..3 {
                    let x = px as f32 + (sx as f32 + 0.5) / 3.0;
                    let y = py as f32 + (sy as f32 + 0.5) / 3.0;
                    let size = SIZE as f32;
                    if !inside_rounded(x, y, 8.0, 8.0, size - 8.0, size - 8.0, 56.0) {
                        continue;
                    }
                    let inner = inside_rounded(x, y, 14.0, 14.0, size - 14.0, size - 14.0, 50.0);
                    let bar = inside_rounded(x, y, 64.0, 70.0, 192.0, 106.0, 8.0);
                    let stem = inside_rounded(x, y, 110.0, 70.0, 146.0, 192.0, 8.0);
                    let colour = if !inner {
                        rim
                    } else if bar || stem {
                        lime
                    } else {
                        tile
                    };
                    for (total, channel) in sum.iter_mut().zip(colour) {
                        *total += f32::from(channel);
                    }
                    covered += 1.0;
                }
            }
            if covered == 0.0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let [r, g, b] = sum.map(|c| (c / covered).round() as u8);
                rgba.extend_from_slice(&[r, g, b, (covered / 9.0 * 255.0).round() as u8]);
            }
        }
    }
    IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_has_the_right_size_and_a_transparent_corner() {
        let icon = app_icon();
        assert_eq!(icon.rgba.len(), (icon.width * icon.height * 4) as usize);
        assert_eq!(
            icon.rgba[3], 0,
            "the corner outside the tile is transparent"
        );
        let centre = ((icon.height / 2 * icon.width + icon.width / 2) * 4) as usize;
        assert_eq!(icon.rgba[centre + 3], 255);
        assert_eq!(
            &icon.rgba[centre..centre + 3],
            &[0x8C, 0xFF, 0x4D],
            "the stem of the T is lime"
        );
    }
}
