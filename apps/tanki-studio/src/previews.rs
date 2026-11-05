// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Preview textures: the cached `details.jpg` of a resource and the preview of a pack variant.
//! Decoded once and kept in the texture cache of the app.

use eframe::egui::{Context, TextureHandle};
use tanki_core::pack::{InstalledPack, PackVariant, read_pack_resource};

use crate::{app::Studio, model::preview_path};

impl Studio {
    /// The preview of a cache group (URL minus file name), as the game saved it.
    pub fn cache_preview(&mut self, ctx: &Context, group: &str) -> Option<TextureHandle> {
        self.cache_picture(ctx, group, None)
    }

    /// The preview of a cache group for an item card: cut to what it shows (the game's pictures
    /// leave uneven transparent margins, more below than above, so a card would sit its item off
    /// centre) and lit up a little, so the cards read as bright as the 3D stage above them.
    pub fn cache_cover(&mut self, ctx: &Context, group: &str) -> Option<TextureHandle> {
        self.cache_picture(ctx, group, Some(COVER_GAIN))
    }

    fn cache_picture(
        &mut self,
        ctx: &Context,
        group: &str,
        gain: Option<f32>,
    ) -> Option<TextureHandle> {
        let scan = self.scan.as_ref()?;
        let path = preview_path(scan, group)?;
        // The client keeps the transparency of a picture in a file of its own next to the colour.
        let alpha = scan
            .resources
            .iter()
            .find(|r| {
                r.group == group && matches!(r.role.as_str(), "alpha.jpg" | "details_alpha.jpg")
            })
            .map(|r| r.path.clone());
        let key = match gain {
            Some(_) => format!("cache:cover:{group}"),
            None => format!("cache:{group}"),
        };
        self.texture(ctx, &key, || {
            let colour = std::fs::read(path).ok()?;
            let merged = alpha
                .and_then(|alpha| std::fs::read(alpha).ok())
                .and_then(|alpha| with_alpha(&colour, &alpha));
            let picture = merged.or(Some(colour))?;
            match gain {
                Some(gain) => cover(&picture, gain),
                None => Some(picture),
            }
        })
    }

    /// The picture that stands for a variant: its preview. A texture atlas is not a picture of the
    /// item, so a variant without a preview has none.
    pub fn variant_preview(
        &mut self,
        ctx: &Context,
        pack: &InstalledPack,
        variant: &PackVariant,
    ) -> Option<TextureHandle> {
        let path = variant.preview.as_deref()?;
        let key = format!("pack:{}:{path}", pack.path.display());
        let pack = pack.clone();
        let relative = path.to_owned();
        self.texture(ctx, &key, move || read_pack_resource(&pack, &relative).ok())
    }

    /// A cover for an item card: the item's picture from the cache, else the first pack variant's
    /// picture, else nothing (the card shows an icon).
    pub fn item_cover(&mut self, ctx: &Context, kind: &str, id: &str) -> Option<TextureHandle> {
        let group = self
            .items
            .iter()
            .find(|i| i.kind == kind && i.id == id)
            .and_then(|item| {
                item.markers.iter().rev().flatten().find_map(|marker| {
                    self.rows
                        .iter()
                        .find(|r| &r.key == marker)?
                        .picture_group
                        .clone()
                })
            });
        if let Some(texture) = group.and_then(|g| self.cache_cover(ctx, &g)) {
            return Some(texture);
        }
        let pack = self.active_pack.clone()?;
        let variant = pack
            .manifest
            .items
            .iter()
            .filter(|i| i.target.kind == kind && i.target.id == id)
            .flat_map(|i| i.variants.iter())
            .find(|v| v.preview.is_some())
            .cloned()?;
        self.variant_preview(ctx, &pack, &variant)
    }

    /// Whether the Apply buttons can start a replacement for the current selection: a skin is
    /// picked and it is not the one in the game already.
    pub fn chosen_apply_available(&self) -> bool {
        self.selection.as_ref().is_some_and(|s| {
            let chosen = self.chosen_variant(s);
            chosen.is_some() && chosen != self.applied_variant(s)
        })
    }
}

/// A PNG of the colour picture with the alpha picture as its transparency, or `None` when either
/// does not decode or they differ in size.
/// How much brighter an item card shows the game's picture.
const COVER_GAIN: f32 = 1.18;

/// Below this opacity a pixel is margin, not picture.
const VISIBLE: u8 = 8;
/// From this opacity a pixel is the item itself, not its soft shadow.
const SOLID: u8 = 128;

/// The box (x0, y0, x1, y1, inclusive) of the pixels at least `opacity` opaque.
fn opaque_box(image: &image::RgbaImage, opacity: u8) -> Option<(u32, u32, u32, u32)> {
    image
        .enumerate_pixels()
        .filter(|(_, _, p)| p.0[3] >= opacity)
        .fold(None, |bounds, (x, y, _)| {
            Some(
                bounds.map_or((x, y, x, y), |(x0, y0, x1, y1): (u32, u32, u32, u32)| {
                    (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
                }),
            )
        })
}

/// A picture cut to its visible part with the item itself (not its soft shadow) at the centre, its
/// colours `gain` times as bright (its transparency kept), as PNG bytes. A picture with nothing
/// visible is kept whole.
fn cover(picture: &[u8], gain: f32) -> Option<Vec<u8>> {
    let mut image = image::load_from_memory(picture).ok()?.to_rgba8();
    if let Some((x0, y0, x1, y1)) = opaque_box(&image, VISIBLE) {
        let (s0x, s0y, s1x, s1y) = opaque_box(&image, SOLID).unwrap_or((x0, y0, x1, y1));
        // Twice the item's centre, so the halves stay whole numbers.
        let (cx, cy) = (i64::from(s0x + s1x), i64::from(s0y + s1y));
        let reach = |lo: u32, hi: u32, c: i64| (c - 2 * i64::from(lo)).max(2 * i64::from(hi) - c);
        let (rx, ry) = (reach(x0, x1, cx), reach(y0, y1, cy));
        // A transparent canvas around the item's centre, as wide each way as the picture reaches.
        let width = u32::try_from(rx + 1).ok()?;
        let height = u32::try_from(ry + 1).ok()?;
        let mut canvas = image::RgbaImage::new(width, height);
        let (left, top) = ((cx - rx) / 2, (cy - ry) / 2);
        for (x, y, pixel) in image.enumerate_pixels() {
            let (cx, cy) = (i64::from(x) - left, i64::from(y) - top);
            if let (Ok(cx), Ok(cy)) = (u32::try_from(cx), u32::try_from(cy))
                && cx < width
                && cy < height
            {
                canvas.put_pixel(cx, cy, *pixel);
            }
        }
        image = canvas;
    }
    for pixel in image.pixels_mut() {
        for channel in &mut pixel.0[..3] {
            *channel = (f32::from(*channel) * gain).round().min(255.0) as u8;
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

fn with_alpha(colour: &[u8], alpha: &[u8]) -> Option<Vec<u8>> {
    let colour = image::load_from_memory(colour).ok()?.to_rgba8();
    let alpha = image::load_from_memory(alpha).ok()?.to_luma8();
    if colour.dimensions() != alpha.dimensions() {
        return None;
    }
    let mut merged = colour;
    for (pixel, opacity) in merged.pixels_mut().zip(alpha.pixels()) {
        pixel.0[3] = opacity.0[0];
    }
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(merged)
        .write_to(&mut out, image::ImageFormat::Png)
        .ok()?;
    Some(out.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(image: image::DynamicImage) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn the_alpha_file_becomes_the_transparency_of_the_picture() {
        let colour = png(image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            2,
            1,
            image::Rgb([200, 100, 50]),
        )));
        let alpha = png(image::DynamicImage::ImageLuma8(
            image::GrayImage::from_vec(2, 1, vec![0, 255]).unwrap(),
        ));
        let merged = image::load_from_memory(&with_alpha(&colour, &alpha).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(merged.get_pixel(0, 0).0, [200, 100, 50, 0]);
        assert_eq!(merged.get_pixel(1, 0).0, [200, 100, 50, 255]);
        let other = png(image::DynamicImage::ImageLuma8(image::GrayImage::new(3, 3)));
        assert!(with_alpha(&colour, &other).is_none(), "sizes differ");
    }

    #[test]
    fn a_cover_is_cut_to_what_it_shows_and_lit_up_without_overflowing() {
        // A 10 × 10 picture: the item a 4 × 2 block, more margin below than above.
        let mut image = image::RgbaImage::new(10, 10);
        for (x, y) in (3..7).flat_map(|x| (1..3).map(move |y| (x, y))) {
            image.put_pixel(x, y, image::Rgba([100, 240, 0, 200]));
        }
        let picture = png(image::DynamicImage::ImageRgba8(image));
        let lit = image::load_from_memory(&cover(&picture, 1.18).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(lit.dimensions(), (4, 2), "the margins are gone");
        assert_eq!(lit.get_pixel(0, 0).0, [118, 255, 0, 200]);
        // A soft shadow under the item: the picture is centred on the item, not on the shadow.
        let mut image = image::RgbaImage::new(10, 10);
        for (x, y) in (3..7).flat_map(|x| (1..3).map(move |y| (x, y))) {
            image.put_pixel(x, y, image::Rgba([100, 240, 0, 255]));
        }
        for (x, y) in (3..7).flat_map(|x| (3..7).map(move |y| (x, y))) {
            image.put_pixel(x, y, image::Rgba([0, 0, 0, 40]));
        }
        let picture = png(image::DynamicImage::ImageRgba8(image));
        let lit = image::load_from_memory(&cover(&picture, 1.0).unwrap())
            .unwrap()
            .to_rgba8();
        let solid: Vec<u32> = lit
            .enumerate_pixels()
            .filter(|(_, _, p)| p.0[3] == 255)
            .map(|(_, y, _)| y)
            .collect();
        let (top, bottom) = (solid[0], *solid.last().unwrap());
        assert_eq!(
            top,
            lit.height() - 1 - bottom,
            "as much air above the item as below"
        );
        let empty = png(image::DynamicImage::ImageRgba8(image::RgbaImage::new(3, 3)));
        let kept = image::load_from_memory(&cover(&empty, 1.18).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(kept.dimensions(), (3, 3), "nothing visible: kept whole");
        assert!(cover(b"not a picture", 1.18).is_none());
    }
}
