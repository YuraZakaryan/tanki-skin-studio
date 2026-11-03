// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The picture a paint writes in place of another paint's.
//!
//! The client reads a paint's picture as the server describes it, never from the cache: as one
//! still picture, or as a sheet of frames of a given size and count, played at a given speed. So
//! what is written takes the shape of the picture it replaces ([`crate::sprite`] reads both):
//!
//! - In place of a still paint: a still picture. A sheet would show all its frames at once, so an
//!   animated paint gives its most typical frame.
//! - In place of an animated paint: a sheet laid out as the replaced one, its size, frame size and
//!   frame count, so the client cuts it as it expects. An animated paint fills it with its own
//!   frames, retimed to the count (frame `i` of `n` shows its frame `⌊i·m/n⌋` of `m`) and played at
//!   the replaced paint's speed; a still paint fills every frame with itself, and stands still.
//!   The cells after the last frame keep what the replaced picture had there.
//!
//! Pictures are JPEG in the cache; a picture that need not change (a still JPEG in place of a still
//! paint) is written byte for byte.

use anyhow::Result;
use image::{
    DynamicImage, GenericImage, ImageFormat, codecs::jpeg::JpegEncoder, imageops::FilterType,
};

use crate::sprite::{Sheet, sheet, still_frame};

/// JPEG quality of a picture made here: high, so a paint loses nothing to a second encoding.
const QUALITY: u8 = 95;

/// The bytes to write in place of the paint picture `original`, from the paint picture `source`.
pub fn fit(source: &[u8], original: &[u8]) -> Result<Vec<u8>> {
    let picture = image::load_from_memory(source)?;
    let layout = image::load_from_memory(original)
        .ok()
        .and_then(|old| sheet(&old).map(|layout| (old, layout)));
    match layout {
        Some((old, layout)) => jpeg(&into_sheet(&picture, old, &layout)),
        None => match still_frame(&picture) {
            Some(frame) => jpeg(&frame),
            None if image::guess_format(source).ok() == Some(ImageFormat::Jpeg) => {
                Ok(source.to_vec())
            }
            None => jpeg(&picture),
        },
    }
}

/// The paint `picture` laid out as the sheet `layout` of `old` (see the module docs).
fn into_sheet(picture: &DynamicImage, old: DynamicImage, layout: &Sheet) -> DynamicImage {
    let (w, h) = (layout.frame_width, layout.frame_height);
    let own = sheet(picture);
    let mut out = old.to_rgb8();
    let mut still = None;
    for index in 0..layout.frames {
        let frame = match &own {
            Some(own) => {
                let source =
                    (u64::from(index) * u64::from(own.frames) / u64::from(layout.frames)) as u32;
                own.frame(picture, source)
                    .resize_exact(w, h, FilterType::Lanczos3)
            }
            None => still
                .get_or_insert_with(|| picture.resize_exact(w, h, FilterType::Lanczos3))
                .clone(),
        };
        let (x, y) = layout.frame_origin(index);
        // The frame fits by construction: its cell is inside the sheet it was found in.
        let _ = out.copy_from(&frame.to_rgb8(), x, y);
    }
    DynamicImage::ImageRgb8(out)
}

fn jpeg(image: &DynamicImage) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, QUALITY)
        .encode_image(&DynamicImage::ImageRgb8(image.to_rgb8()))?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GenericImageView, GrayImage, Luma};

    fn texture(x: f32, y: f32) -> f32 {
        128.0
            + 40.0 * (x / 5.3).sin() * (y / 7.1).cos()
            + 30.0 * ((x + 2.0 * y) / 11.7).sin()
            + 20.0 * ((3.0 * x - y) / 4.1).cos()
    }

    /// A sheet of `frames` frames of `side`, `columns` × `rows` cells, the rest flat grey.
    fn animation(side: u32, columns: u32, rows: u32, frames: u32, speed: f32) -> DynamicImage {
        DynamicImage::ImageLuma8(GrayImage::from_fn(side * columns, side * rows, |x, y| {
            let index = (y / side) * columns + x / side;
            if index >= frames {
                return Luma([90]);
            }
            let t = index as f32 * speed;
            let (fx, fy) = ((x % side) as f32, (y % side) as f32);
            Luma([texture(fx + 1.5 * t, fy + 0.7 * t).clamp(0.0, 255.0) as u8])
        }))
    }

    fn still(side: u32) -> DynamicImage {
        DynamicImage::ImageLuma8(GrayImage::from_fn(side, side, |x, y| {
            Luma([texture(x as f32 * 1.7, y as f32 * 0.9).clamp(0.0, 255.0) as u8])
        }))
    }

    fn encoded(image: &DynamicImage, format: ImageFormat) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut bytes), format)
            .expect("encode");
        bytes
    }

    fn decoded(bytes: &[u8]) -> DynamicImage {
        image::load_from_memory(bytes).expect("a picture")
    }

    #[test]
    fn a_still_jpeg_over_a_still_paint_is_written_as_it_is() {
        let source = encoded(&still(64), ImageFormat::Jpeg);
        let original = encoded(&still(32), ImageFormat::Jpeg);
        assert_eq!(fit(&source, &original).expect("fit"), source);
    }

    #[test]
    fn a_png_becomes_a_jpeg() {
        let source = encoded(&still(64), ImageFormat::Png);
        let original = encoded(&still(32), ImageFormat::Jpeg);
        let written = fit(&source, &original).expect("fit");
        assert_eq!(image::guess_format(&written).ok(), Some(ImageFormat::Jpeg));
    }

    #[test]
    fn an_animated_paint_over_a_still_one_gives_one_frame() {
        let source = encoded(&animation(32, 4, 4, 16, 1.0), ImageFormat::Png);
        let original = encoded(&still(32), ImageFormat::Jpeg);
        let written = decoded(&fit(&source, &original).expect("fit"));
        assert_eq!(written.dimensions(), (32, 32));
    }

    #[test]
    fn over_an_animated_paint_the_sheet_keeps_its_layout() {
        // The replaced paint: 13 frames of 32 in a 4 × 4 sheet.
        let original = encoded(&animation(32, 4, 4, 13, 1.0), ImageFormat::Png);
        let layout = sheet(&decoded(&original)).expect("a sheet");

        // An animation of another layout is retimed into it, and reads as that sheet.
        let other = encoded(&animation(48, 3, 2, 6, 1.5), ImageFormat::Png);
        let written = decoded(&fit(&other, &original).expect("fit"));
        assert_eq!(written.dimensions(), (128, 128));
        let found = sheet(&written).expect("still a sheet");
        assert_eq!(
            (found.frame_width, found.frames, found.first),
            (layout.frame_width, layout.frames, layout.first)
        );

        // A still paint fills every frame with itself.
        let plain = encoded(&still(64), ImageFormat::Jpeg);
        let written = decoded(&fit(&plain, &original).expect("fit"));
        assert_eq!(written.dimensions(), (128, 128));
        let first = layout.frame(&written, 0).to_luma8();
        let last = layout.frame(&written, layout.frames - 1).to_luma8();
        let difference: u64 = first
            .pixels()
            .zip(last.pixels())
            .map(|(a, b)| u64::from(a.0[0].abs_diff(b.0[0])))
            .sum();
        assert!(
            difference / u64::from(32_u32 * 32) <= 2,
            "every frame is the same still picture"
        );
        // The cells after the last frame keep the replaced picture's grey.
        let after = written.to_rgb8().get_pixel(127, 127).0;
        assert!(after.iter().all(|c| c.abs_diff(90) <= 6), "{after:?}");
    }
}
