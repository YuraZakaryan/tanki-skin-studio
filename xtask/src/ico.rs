// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo xtask ico <out.ico> <in.png>...`: pack square PNGs into a Windows icon, as WordCraft does.
//! Every size is stored as PNG data inside the ICO (Windows Vista and later read that), so the file
//! is the images as rendered, with no conversion. packaging/icons.sh calls this.

use anyhow::{Context, Result, bail, ensure};

/// The size and bytes of one PNG.
pub struct Image {
    pub size: u32,
    pub png: Vec<u8>,
}

/// The square size of a PNG, read from its IHDR chunk.
pub fn png_size(png: &[u8]) -> Result<u32> {
    ensure!(
        png.len() >= 24 && png.starts_with(b"\x89PNG\r\n\x1a\n") && &png[12..16] == b"IHDR",
        "not a PNG"
    );
    let width = u32::from_be_bytes([png[16], png[17], png[18], png[19]]);
    let height = u32::from_be_bytes([png[20], png[21], png[22], png[23]]);
    ensure!(width == height, "not square: {width}x{height}");
    ensure!(
        (1..=256).contains(&width),
        "an ICO image is 1 to 256 px, not {width}"
    );
    Ok(width)
}

/// An ICO file holding `images`, smallest first.
pub fn encode(images: &[Image]) -> Result<Vec<u8>> {
    ensure!(!images.is_empty(), "no images");
    let count = u16::try_from(images.len()).context("too many images")?;
    let mut sorted: Vec<&Image> = images.iter().collect();
    sorted.sort_by_key(|image| image.size);
    // ICONDIR, then one 16-byte ICONDIRENTRY per image, then the image data.
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    let mut offset = 6 + 16 * u32::from(count);
    for image in &sorted {
        // 256 is written as 0.
        let side = u8::try_from(image.size % 256).context("size")?;
        let len = u32::try_from(image.png.len()).context("image too large")?;
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset = offset.checked_add(len).context("ICO too large")?;
    }
    for image in sorted {
        out.extend_from_slice(&image.png);
    }
    Ok(out)
}

pub fn run(args: &[String]) -> Result<()> {
    let [out, inputs @ ..] = args else {
        bail!("usage: ico <out.ico> <in.png>...");
    };
    let mut images = Vec::new();
    for path in inputs {
        let png = std::fs::read(path).with_context(|| path.clone())?;
        let size = png_size(&png).with_context(|| path.clone())?;
        images.push(Image { size, png });
    }
    std::fs::write(out, encode(&images)?).with_context(|| out.clone())?;
    println!("wrote {out} ({} sizes)", images.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(size: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(&size.to_be_bytes());
        bytes.extend_from_slice(b"rest of the image");
        bytes
    }

    #[test]
    fn sizes_are_read_from_the_png_header() {
        assert_eq!(png_size(&png(48)).unwrap(), 48);
        assert!(png_size(b"GIF89a not a png at all").is_err());
        assert!(
            png_size(&png(512)).is_err(),
            "an ICO image is at most 256 px"
        );
    }

    #[test]
    fn the_directory_lists_every_size_smallest_first_with_256_as_zero() {
        let images = [256, 16, 32].map(|size| Image {
            size,
            png: png(size),
        });
        let ico = encode(&images).unwrap();
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 3, 0]);
        let widths: Vec<u8> = (0..3).map(|i| ico[6 + 16 * i]).collect();
        assert_eq!(widths, [16, 32, 0]);
        // The first image starts right after the directory, and the data follows in order.
        let first = u32::from_le_bytes(ico[18..22].try_into().unwrap()) as usize;
        assert_eq!(first, 6 + 16 * 3);
        assert_eq!(&ico[first..first + png(16).len()], png(16).as_slice());
    }
}
