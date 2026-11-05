// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! What a stage shows, built on the CPU (in a background thread if need be) and handed to the
//! GPU once: parts of geometry with the textures that colour them.

use std::sync::Arc;

use tanki_model::{Bounds, Geometry};

/// Textures larger than this on a side are refused; no item texture comes near it.
pub const MAX_IMAGE_SIDE: u32 = 4096;

/// An RGBA picture, 8 bits per channel in sRGB, alpha not premultiplied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Image {
    /// `None` unless the pixels fill exactly `width × height` and the sides are within limits.
    pub fn new(width: u32, height: u32, rgba: Vec<u8>) -> Option<Image> {
        let sides = 1..=MAX_IMAGE_SIDE;
        let expected = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(4)?;
        (sides.contains(&width) && sides.contains(&height) && rgba.len() == expected).then_some(
            Image {
                width,
                height,
                rgba,
            },
        )
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }

    /// The picture and its smaller copies down to one pixel, each half the size of the one
    /// before (box-filtered), so a texture far from the camera does not shimmer.
    pub fn mip_chain(&self) -> Vec<Image> {
        let mut chain = vec![self.clone()];
        while let Some(last) = chain.last()
            && (last.width > 1 || last.height > 1)
        {
            let next = last.half();
            chain.push(next);
        }
        chain
    }

    fn half(&self) -> Image {
        let width = (self.width / 2).max(1);
        let height = (self.height / 2).max(1);
        let pixel = |x: u32, y: u32| -> [u8; 4] {
            let x = x.min(self.width - 1) as usize;
            let y = y.min(self.height - 1) as usize;
            let at = (y * self.width as usize + x) * 4;
            self.rgba
                .get(at..at + 4)
                .and_then(|p| p.try_into().ok())
                .unwrap_or_default()
        };
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for y in 0..height {
            for x in 0..width {
                let quad = [
                    pixel(x * 2, y * 2),
                    pixel(x * 2 + 1, y * 2),
                    pixel(x * 2, y * 2 + 1),
                    pixel(x * 2 + 1, y * 2 + 1),
                ];
                for channel in 0..4 {
                    let sum: u32 = quad.iter().map(|p| u32::from(p[channel])).sum();
                    rgba.push(((sum + 2) / 4) as u8);
                }
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }
}

/// How a part is coloured: its paint (a picture, else a colour), the details texture over it
/// (where the details' alpha is opaque), all of it darkened by the baked lightmap.
#[derive(Debug, Clone, PartialEq)]
pub struct Material {
    /// sRGB colour under the details when there is no paint picture.
    pub paint: [u8; 3],
    /// A paint's picture, repeated over the model's texture coordinates.
    pub paint_picture: Option<Arc<Image>>,
    pub details: Option<Arc<Image>>,
    pub lightmap: Option<Arc<Image>>,
}

impl Default for Material {
    fn default() -> Self {
        Material {
            paint: [150, 152, 140],
            paint_picture: None,
            details: None,
            lightmap: None,
        }
    }
}

/// One piece of the scene, moved by `offset` (in model units, Z up).
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub geometry: Arc<Geometry>,
    pub material: Material,
    pub offset: [f32; 3],
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub parts: Vec<Part>,
}

impl Scene {
    pub fn is_empty(&self) -> bool {
        self.parts
            .iter()
            .all(|part| part.geometry.indices.is_empty())
    }

    /// The box around every part where it stands.
    pub fn bounds(&self) -> Bounds {
        self.parts.iter().fold(Bounds::default(), |all, part| {
            let mut moved = Bounds::default();
            if !part.geometry.bounds.is_empty() {
                let shift = |p: [f32; 3]| std::array::from_fn(|axis| p[axis] + part.offset[axis]);
                moved.add(shift(part.geometry.bounds.min));
                moved.add(shift(part.geometry.bounds.max));
            }
            all.union(moved)
        })
    }
}

#[cfg(test)]
mod tests;
