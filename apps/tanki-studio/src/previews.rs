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

    /// The preview of a cache group for an item card: lit up a little, so the cards read as bright
    /// as the 3D stage above them (the game's garage pictures are darker than its garage).
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
                Some(gain) => brighter(&picture, gain),
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

/// A picture with its colours `gain` times as bright (its transparency kept), as PNG bytes.
fn brighter(picture: &[u8], gain: f32) -> Option<Vec<u8>> {
    let mut image = image::load_from_memory(picture).ok()?.to_rgba8();
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
    fn a_brighter_cover_keeps_its_transparency_and_never_overflows() {
        let picture = png(image::DynamicImage::ImageRgba8(
            image::RgbaImage::from_pixel(2, 1, image::Rgba([100, 240, 0, 77])),
        ));
        let lit = image::load_from_memory(&brighter(&picture, 1.18).unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(lit.get_pixel(0, 0).0, [118, 255, 0, 77]);
        assert!(brighter(b"not a picture", 1.18).is_none());
    }
}
