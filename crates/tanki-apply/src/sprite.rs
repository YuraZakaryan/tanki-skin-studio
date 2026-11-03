// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Tell an animated paint's picture (a sheet of frames) from a still one, and find its frame.
//!
//! An animated paint is one picture: its frames, all of one size, laid in a grid left to right,
//! then top to bottom, the cells after the last frame left empty (or holding a note). The client
//! learns the frame size from the server, never from the cache, so a paint put in place of a still
//! one is shown as one still picture: a sheet would show all its frames at once. One frame shows
//! the paint as it is meant to look; not the first, which can be blank (an animation that fades
//! in), but the most typical: the medoid, the frame that differs least from all the others.
//!
//! Sprite-sheet detectors look for the gaps or the transparency between sprites; a paint's frames
//! have neither (opaque, square, edge to edge, each tiling seamlessly). They are found by what
//! makes an animation instead: the frames are one picture, a little changed, so the picture
//! correlates with itself shifted by exactly one frame, far more than shifted by half of one. A
//! still texture does the opposite: the smaller the shift, the more alike (it is smooth). For each
//! square cell that divides the picture, every cell is averaged down to a small grid of samples
//! (exactly, with a summed-area table, so no detail aliases) and its Pearson correlation is taken
//! with the next cell (the shift by one cell) and with the window half a cell to its right (the
//! control). Empty cells (one flat colour) take no part; the medians are the cut's scores, so a
//! stray cell (a note after the last frame) does not sway them. The smallest cell that correlates strongly with the next, and clearly more than
//! with the control, is the frame: a cut of several frames per cell passes too, part of a frame
//! does not.
//!
//! A still picture of one motif repeated (a tiled pattern: badges on a background) can pass both
//! tests at the motif's size, and is kept whole: one cell would show the motif larger on the tank.
//! Time tells it apart, twice. Frames are read left to right, then on the next row, so a row's last
//! frame and the next row's first are neighbours in time, as alike as any two frames side by side;
//! in a still picture they are far apart and no more alike than any two distant places. And a frame
//! is more like the next one than like the frame half the animation on, which has drifted (or, in
//! a loop, is the farthest away); copies of a motif are equally alike near and far.
//!
//! The frames fill the cells in reading order, one after the other; empty cells (and a note)
//! come only after the last. A picture whose cells that are not empty lie scattered (stars on a
//! dark sky) is no sheet, however alike its patches.
//!
//! No single picture can tell an animation whose background alone flickers from a pattern; the
//! tests lean to keeping a picture whole, the milder mistake. Measured on 102 paints of a public
//! asset pack and 438 pictures of a client cache: every sheet among them found at its frame size,
//! no pattern, landscape or still texture taken for one.

use image::{DynamicImage, GenericImageView};

/// The frame layout of a sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sheet {
    pub frame_width: u32,
    pub frame_height: u32,
    pub columns: u32,
    pub rows: u32,
    /// The frames: the run of cells that are not empty, from the first.
    pub frames: u32,
    /// The cell the first frame is in, counted left to right, top to bottom (cells before it are
    /// empty).
    pub first: u32,
    /// The column and row of the most typical frame (see the module docs).
    pub typical: (u32, u32),
}

impl Sheet {
    /// The top-left corner of frame `index` (counted from the first frame).
    pub fn frame_origin(&self, index: u32) -> (u32, u32) {
        let cell = self.first + index;
        (
            (cell % self.columns) * self.frame_width,
            (cell / self.columns) * self.frame_height,
        )
    }

    /// Frame `index` of `image`, the sheet this layout was found in.
    pub fn frame(&self, image: &DynamicImage, index: u32) -> DynamicImage {
        let (x, y) = self.frame_origin(index);
        image.crop_imm(x, y, self.frame_width, self.frame_height)
    }
}

/// What one way of cutting scored (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub frame_width: u32,
    pub frame_height: u32,
    /// The frames: the run of cells that are not empty, from the first.
    pub frames: u32,
    /// The cells that are not empty, the frames among them.
    pub filled: u32,
    /// The median correlation of a cell with the next.
    pub correlation: f32,
    /// The median correlation of a cell with the window half a cell to its right.
    pub control: f32,
    /// The median correlation of a row's last cell with the next row's first: neighbours in time
    /// if the cells are frames, far apart if they are parts of one picture. `None` with one row.
    pub wrap: Option<f32>,
    /// The median correlation of a frame with the one half the frames on: the farthest in time
    /// (and in a loop).
    pub far: f32,
}

/// The fewest frames a sheet has: two or three cells alike can be a still picture's parts.
const MIN_FRAMES: u32 = 4;
/// The most: 16×16 frames of 64 in a 1024 picture. More, smaller cells are a smooth picture's
/// patches, alike because they are close, not frames.
const MAX_FRAMES: u32 = 256;
/// The smallest frame side, in pixels.
const MIN_SIDE: u32 = 16;
/// Samples per cell side: enough for the correlation to mean something, few enough to be quick.
const SAMPLES: u32 = 24;
/// The median correlation from which the cells are frames of one animation.
const ALIKE: f32 = 0.6;
/// How much more a cell correlates with the next than with the window half a cell on: a frame
/// period is a peak of the picture's correlation with itself, not the slope of a smooth texture.
const PEAK: f32 = 0.2;
/// How much less alike a row's last cell and the next row's first may be than two cells side by
/// side, and still be frames in a row of time.
const WRAP: f32 = 0.3;
/// How much more alike a frame must be with the next than with the frame half the animation on.
const DRIFT: f32 = 0.1;
/// The share of the cells that are not empty the run of frames must hold: the rest is a note.
const RUN: f32 = 0.8;
/// A cell whose samples spread less than this (standard deviation, levels of 255) is empty.
const EMPTY: f32 = 1.5;
/// The frames each frame is compared with to find the most typical one: evenly spread, so a sheet
/// of a thousand frames is weighed in a moment.
const MEDOID_SAMPLE: usize = 64;

/// The sheet `image` is, if it is one; `None` for a still picture.
pub fn sheet(image: &DynamicImage) -> Option<Sheet> {
    let cut = cuts(image).into_iter().find(|c| {
        (MIN_FRAMES..=MAX_FRAMES).contains(&c.frames)
            && c.frames as f32 >= RUN * c.filled as f32
            && c.correlation >= ALIKE
            && c.correlation - c.control >= PEAK
            && c.wrap.is_none_or(|wrap| wrap >= c.correlation - WRAP)
            && c.correlation - c.far >= DRIFT
    })?;
    let (width, height) = image.dimensions();
    let (w, h) = (cut.frame_width, cut.frame_height);
    let columns = width / w;
    let table = SummedArea::new(image);
    let frames: Vec<(usize, Vec<f32>)> = cells(&table, w, h)
        .into_iter()
        .enumerate()
        .skip_while(|(_, cell)| spread(cell) < EMPTY)
        .take(cut.frames as usize)
        .collect();
    let first = frames.first().map_or(0, |(index, _)| *index as u32);
    let step = frames.len().div_ceil(MEDOID_SAMPLE).max(1);
    let typical = frames
        .iter()
        .min_by(|(_, a), (_, b)| {
            let distance = |cell: &Vec<f32>| -> f32 {
                frames
                    .iter()
                    .step_by(step)
                    .map(|(_, other)| mean_difference(cell, other))
                    .sum()
            };
            distance(a).total_cmp(&distance(b))
        })
        .map_or(0, |(index, _)| *index as u32);
    Some(Sheet {
        frame_width: w,
        frame_height: h,
        columns,
        rows: height / h,
        frames: cut.frames,
        first,
        typical: (typical % columns, typical / columns),
    })
}

/// The most typical frame of `image` when it is a sheet, else `None`.
pub fn still_frame(image: &DynamicImage) -> Option<DynamicImage> {
    let sheet = sheet(image)?;
    let (column, row) = sheet.typical;
    Some(image.crop_imm(
        column * sheet.frame_width,
        row * sheet.frame_height,
        sheet.frame_width,
        sheet.frame_height,
    ))
}

/// Every way to cut `image` into at least two equal square cells no smaller than [`MIN_SIDE`],
/// smallest first, with its scores.
pub fn cuts(image: &DynamicImage) -> Vec<Cut> {
    let (width, height) = image.dimensions();
    let table = SummedArea::new(image);
    (MIN_SIDE..=width.min(height))
        .filter(|s| width % s == 0 && height % s == 0 && (width / s) * (height / s) >= 2)
        .map(|s| score(&table, s, s))
        .collect()
}

/// The cells of a `w`×`h` cut, left to right, top to bottom, each as its samples.
fn cells(table: &SummedArea, w: u32, h: u32) -> Vec<Vec<f32>> {
    let (columns, rows) = (table.width / w, table.height / h);
    let samples = SAMPLES.min(w).min(h);
    (0..rows)
        .flat_map(|r| (0..columns).map(move |c| (c, r)))
        .map(|(c, r)| table.samples(c * w, r * h, w, h, samples))
        .collect()
}

/// The scores of cutting into `w`×`h` cells.
fn score(table: &SummedArea, w: u32, h: u32) -> Cut {
    let columns = table.width / w;
    let samples = SAMPLES.min(w).min(h);
    let cells = cells(table, w, h);
    let full: Vec<bool> = cells.iter().map(|cell| spread(cell) >= EMPTY).collect();
    let mut correlations = Vec::new();
    let mut wraps = Vec::new();
    for (index, pair) in cells.windows(2).enumerate() {
        if full[index] && full[index + 1] {
            let correlation = pearson(&pair[0], &pair[1]);
            if (index as u32 + 1).is_multiple_of(columns) {
                wraps.push(correlation);
            } else {
                correlations.push(correlation);
            }
        }
    }
    let indices: Vec<usize> = (0..cells.len()).filter(|i| full[*i]).collect();
    let half = indices.len() / 2;
    let mut fars: Vec<f32> = (0..indices.len().saturating_sub(half))
        .filter(|_| half > 0)
        .map(|k| pearson(&cells[indices[k]], &cells[indices[k + half]]))
        .collect();

    // The control: each cell against the window half a cell to its right, where it fits.
    let mut controls = Vec::new();
    for (index, cell) in cells.iter().enumerate().filter(|(i, _)| full[*i]) {
        let (c, r) = (index as u32 % columns, index as u32 / columns);
        let x = c * w + w / 2;
        if x + w <= table.width {
            controls.push(pearson(cell, &table.samples(x, r * h, w, h, samples)));
        }
    }
    let frames = full.iter().skip_while(|f| !**f).take_while(|f| **f).count();
    Cut {
        frame_width: w,
        frame_height: h,
        frames: frames as u32,
        filled: full.iter().filter(|f| **f).count() as u32,
        correlation: median(&mut correlations),
        control: median(&mut controls),
        far: median(&mut fars),
        wrap: (!wraps.is_empty()).then(|| median(&mut wraps)),
    }
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len().max(1) as f32
}

fn spread(values: &[f32]) -> f32 {
    let m = mean(values);
    (values.iter().map(|v| (v - m).powi(2)).sum::<f32>() / values.len().max(1) as f32).sqrt()
}

/// The Pearson correlation of two series of one length; 0 when either is flat.
fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let (ma, mb) = (mean(a), mean(b));
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        let (dx, dy) = (x - ma, y - mb);
        ab += dx * dy;
        aa += dx * dx;
        bb += dy * dy;
    }
    if aa <= f32::EPSILON || bb <= f32::EPSILON {
        0.0
    } else {
        ab / (aa * bb).sqrt()
    }
}

fn mean_difference(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() / a.len().max(1) as f32
}

/// The median; 0 for none.
fn median(values: &mut [f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// Sums of brightness over any rectangle in constant time (a summed-area table, in f64 so a large
/// picture keeps its precision).
struct SummedArea {
    width: u32,
    height: u32,
    sums: Vec<f64>,
}

impl SummedArea {
    fn new(image: &DynamicImage) -> Self {
        let gray = image.to_luma8();
        let (width, height) = gray.dimensions();
        let stride = width as usize + 1;
        let mut sums = vec![0.0; stride * (height as usize + 1)];
        for (y, line) in gray.rows().enumerate() {
            let mut row = 0.0;
            for (x, pixel) in line.enumerate() {
                row += f64::from(pixel.0[0]);
                sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
            }
        }
        Self {
            width,
            height,
            sums,
        }
    }

    /// The mean brightness of the pixels `x0..x1`, `y0..y1`.
    fn mean(&self, x0: u32, y0: u32, x1: u32, y1: u32) -> f32 {
        let stride = self.width as usize + 1;
        let at = |x: u32, y: u32| self.sums[y as usize * stride + x as usize];
        let sum = at(x1, y1) - at(x0, y1) - at(x1, y0) + at(x0, y0);
        let area = f64::from((x1 - x0) * (y1 - y0)).max(1.0);
        (sum / area) as f32
    }

    /// The cell at (`x`, `y`), `w`×`h`, averaged down to `n`×`n` samples.
    fn samples(&self, x: u32, y: u32, w: u32, h: u32, n: u32) -> Vec<f32> {
        let edge = |i: u32, size: u32| i * size / n;
        (0..n)
            .flat_map(|j| (0..n).map(move |i| (i, j)))
            .map(|(i, j)| {
                self.mean(
                    x + edge(i, w),
                    y + edge(j, h),
                    x + edge(i + 1, w),
                    y + edge(j + 1, h),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};

    /// A smooth, irregular value at (`x`, `y`): a few waves, so no small cut repeats it.
    fn texture(x: f32, y: f32) -> f32 {
        128.0
            + 40.0 * (x / 5.3).sin() * (y / 7.1).cos()
            + 30.0 * ((x + 2.0 * y) / 11.7).sin()
            + 20.0 * ((3.0 * x - y) / 4.1).cos()
    }

    /// A sheet of `frames` frames of `side` pixels, `columns` across, `rows` down: the texture
    /// drifting a little each frame, the cells after the last left a flat grey.
    fn animation(side: u32, columns: u32, rows: u32, frames: u32) -> DynamicImage {
        DynamicImage::ImageLuma8(GrayImage::from_fn(side * columns, side * rows, |x, y| {
            let index = (y / side) * columns + x / side;
            if index >= frames {
                return Luma([90]);
            }
            let t = index as f32;
            let (fx, fy) = ((x % side) as f32, (y % side) as f32);
            Luma([texture(fx + 1.5 * t, fy + 0.7 * t).clamp(0.0, 255.0) as u8])
        }))
    }

    #[test]
    fn a_sheet_is_found_at_its_frame_size() {
        let found = sheet(&animation(32, 4, 4, 16)).expect("a sheet");
        assert_eq!(
            (
                found.frame_width,
                found.frame_height,
                found.columns,
                found.rows,
                found.frames
            ),
            (32, 32, 4, 4, 16)
        );
        let still = still_frame(&animation(32, 4, 4, 16)).expect("a frame");
        assert_eq!((still.width(), still.height()), (32, 32));
    }

    #[test]
    fn empty_cells_after_the_last_frame_are_no_frames() {
        let found = sheet(&animation(32, 4, 4, 13)).expect("a sheet");
        assert_eq!((found.frame_width, found.frames), (32, 13));
        let (column, row) = found.typical;
        assert!(row * 4 + column < 13, "the typical frame is a frame");
    }

    #[test]
    fn a_still_texture_is_kept_whole() {
        let still = DynamicImage::ImageLuma8(GrayImage::from_fn(128, 128, |x, y| {
            Luma([texture(x as f32, y as f32).clamp(0.0, 255.0) as u8])
        }));
        assert_eq!(sheet(&still), None);
        assert_eq!(still_frame(&still), None);
    }

    #[test]
    fn a_repeated_motif_on_a_changing_background_is_kept_whole() {
        // One motif every 32 pixels over a background that changes across the picture: every cell
        // alike near and far, as no animation is.
        let pattern = DynamicImage::ImageLuma8(GrayImage::from_fn(128, 128, |x, y| {
            let motif = texture((x % 32) as f32, (y % 32) as f32);
            let background = 25.0 * (x as f32 / 40.0).sin() + 25.0 * (y as f32 / 55.0).cos();
            Luma([(0.8 * motif + background).clamp(0.0, 255.0) as u8])
        }));
        assert_eq!(sheet(&pattern), None);
    }

    #[test]
    fn a_picture_too_small_to_cut_is_no_sheet() {
        let dot = DynamicImage::ImageLuma8(GrayImage::from_pixel(1, 1, Luma([30])));
        assert_eq!(sheet(&dot), None);
        assert!(cuts(&dot).is_empty());
    }
}
