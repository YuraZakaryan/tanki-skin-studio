// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The three type families of the design (Chakra Petch for titles, Inter for body text, JetBrains
//! Mono for numbers and file names; all SIL OFL) and the Lucide icon font (ISC), bundled so the
//! app looks the same on every machine.
//!
//! Inter and JetBrains Mono are variable fonts: the weight is a variation coordinate on each text
//! run. Chakra Petch ships as static weights, so each weight is its own family.

use std::sync::Arc;

use egui::epaint::text::VariationCoords;
use egui::{
    Align2, Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Galley, Painter, Pos2,
    Rect, Vec2,
    text::{LayoutJob, TextFormat},
};

const CHAKRA_600: &[u8] = include_bytes!("../assets/ChakraPetch-SemiBold.ttf");
const CHAKRA_700: &[u8] = include_bytes!("../assets/ChakraPetch-Bold.ttf");
const INTER: &[u8] = include_bytes!("../assets/Inter.ttf");
const MONO: &[u8] = include_bytes!("../assets/JetBrainsMono.ttf");
const LUCIDE: &[u8] = include_bytes!("../assets/lucide.ttf");

fn named(name: &str) -> FontFamily {
    FontFamily::Name(name.into())
}

/// Register the bundled fonts. Takes effect from the next frame.
pub fn install(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        ("chakra-600", CHAKRA_600),
        ("chakra-700", CHAKRA_700),
        ("inter", INTER),
        ("mono", MONO),
        ("lucide", LUCIDE),
    ] {
        fonts
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
        // Each family falls back to egui's built-in fonts for glyphs the file lacks (arrows, ✓).
        // Chakra Petch has no Cyrillic: Inter, drawn at the same weight, stands in for it.
        let mut chain = vec![name.to_owned()];
        if name.starts_with("chakra") {
            chain.push("inter".to_owned());
        }
        chain.extend(
            fonts
                .families
                .get(&FontFamily::Proportional)
                .cloned()
                .unwrap_or_default(),
        );
        fonts.families.insert(named(name), chain);
    }
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    /// Chakra Petch: titles, buttons, section headings.
    Display,
    /// Inter: body text.
    Body,
    /// JetBrains Mono: numbers, sizes, file names, key caps.
    Mono,
}

/// One text style: face, size in points, weight, colour and letter spacing.
#[derive(Clone, Copy, Debug)]
pub struct Text {
    pub face: Face,
    pub size: f32,
    pub weight: u16,
    pub color: Color32,
    pub spacing: f32,
}

impl Text {
    pub fn new(face: Face, size: f32, weight: u16, color: Color32) -> Self {
        Self {
            face,
            size,
            weight,
            color,
            spacing: 0.0,
        }
    }
    pub fn display(size: f32, weight: u16, color: Color32) -> Self {
        Self::new(Face::Display, size, weight, color)
    }
    pub fn body(size: f32, weight: u16, color: Color32) -> Self {
        Self::new(Face::Body, size, weight, color)
    }
    pub fn mono(size: f32, weight: u16, color: Color32) -> Self {
        Self::new(Face::Mono, size, weight, color)
    }
    pub fn spaced(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }

    fn format(&self) -> TextFormat {
        let (family, coords) = match self.face {
            // The weight coordinate only reaches the variable Inter fallback (Cyrillic text).
            Face::Display => (
                named(if self.weight >= 700 {
                    "chakra-700"
                } else {
                    "chakra-600"
                }),
                VariationCoords::new([
                    (b"wght", f32::from(self.weight)),
                    (b"opsz", self.size.clamp(14.0, 32.0)),
                ]),
            ),
            // Inter's optical size follows the font size within the axis range, like browsers.
            Face::Body => (
                named("inter"),
                VariationCoords::new([
                    (b"wght", f32::from(self.weight)),
                    (b"opsz", self.size.clamp(14.0, 32.0)),
                ]),
            ),
            Face::Mono => (
                named("mono"),
                VariationCoords::new([(b"wght", f32::from(self.weight))]),
            ),
        };
        TextFormat {
            font_id: FontId::new(self.size, family),
            color: self.color,
            extra_letter_spacing: self.spacing,
            coords,
            ..Default::default()
        }
    }
}

/// Lay out one line of text.
pub fn line(painter: &Painter, text: &str, style: Text) -> Arc<Galley> {
    painter.layout_job(LayoutJob::single_section(text.to_owned(), style.format()))
}

/// Lay out wrapped text no wider than `width`, with an optional line height.
pub fn paragraph(
    painter: &Painter,
    text: &str,
    style: Text,
    width: f32,
    line_height: Option<f32>,
) -> Arc<Galley> {
    let mut format = style.format();
    format.line_height = line_height;
    let mut job = LayoutJob::single_section(text.to_owned(), format);
    job.wrap.max_width = width;
    painter.layout_job(job)
}

/// Like [`paragraph`], with every line centred on the x of the position it is drawn at.
pub fn paragraph_centered(
    painter: &Painter,
    text: &str,
    style: Text,
    width: f32,
    line_height: Option<f32>,
) -> Arc<Galley> {
    let mut format = style.format();
    format.line_height = line_height;
    let mut job = LayoutJob::single_section(text.to_owned(), format);
    job.wrap.max_width = width;
    job.halign = egui::Align::Center;
    painter.layout_job(job)
}

/// Width of one line of text.
pub fn width(painter: &Painter, text: &str, style: Text) -> f32 {
    line(painter, text, style).size().x
}

/// `text` shortened with "…" so that it is no wider than `max_width` (as much of it as fits).
pub fn ellipsize<'a>(
    painter: &Painter,
    text: &'a str,
    style: Text,
    max_width: f32,
) -> std::borrow::Cow<'a, str> {
    if max_width <= 0.0 {
        return std::borrow::Cow::Borrowed("");
    }
    if width(painter, text, style) <= max_width {
        return std::borrow::Cow::Borrowed(text);
    }
    // Binary search for the longest prefix that fits together with the ellipsis.
    let chars: Vec<char> = text.chars().collect();
    let (mut low, mut high) = (0, chars.len());
    while low < high {
        let middle = (low + high).div_ceil(2);
        let candidate: String = chars[..middle].iter().collect::<String>() + "…";
        if width(painter, &candidate, style) <= max_width {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    let mut shortened: String = chars[..low].iter().collect();
    while shortened.ends_with(' ') {
        shortened.pop();
    }
    std::borrow::Cow::Owned(shortened + "…")
}

/// Draw one line with its top-left corner at `pos`; returns the rectangle it occupies.
pub fn draw(painter: &Painter, pos: Pos2, text: &str, style: Text) -> Rect {
    let galley = line(painter, text, style);
    let rect = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, style.color);
    rect
}

/// Draw one line vertically centred on `center_y`, starting at `x`.
pub fn draw_left(painter: &Painter, x: f32, center_y: f32, text: &str, style: Text) -> Rect {
    let galley = line(painter, text, style);
    let pos = Pos2::new(x, center_y - galley.size().y / 2.0);
    let rect = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, style.color);
    rect
}

/// Draw one line ending at `right`, vertically centred on `center_y`.
pub fn draw_right(painter: &Painter, right: f32, center_y: f32, text: &str, style: Text) -> Rect {
    let galley = line(painter, text, style);
    let pos = Pos2::new(right - galley.size().x, center_y - galley.size().y / 2.0);
    let rect = Rect::from_min_size(pos, galley.size());
    painter.galley(pos, galley, style.color);
    rect
}

/// Draw one line centred in `rect`.
pub fn draw_centered(painter: &Painter, rect: Rect, text: &str, style: Text) {
    let galley = line(painter, text, style);
    let pos = Align2::CENTER_CENTER
        .align_size_within_rect(galley.size(), rect)
        .min;
    painter.galley(pos, galley, style.color);
}

/// Draw a Lucide icon centred in a square of side `size` whose centre is `center`.
pub fn icon(painter: &Painter, center: Pos2, glyph: char, size: f32, color: Color32) {
    let format = TextFormat {
        font_id: FontId::new(size, named("lucide")),
        color,
        ..Default::default()
    };
    let galley = painter.layout_job(LayoutJob::single_section(glyph.to_string(), format));
    let offset = Vec2::new(galley.size().x / 2.0, galley.size().y / 2.0);
    painter.galley(center - offset, galley, color);
}

/// Like [`icon`], turned clockwise by `angle` radians around its centre (a spinner).
pub fn icon_rotated(
    painter: &Painter,
    center: Pos2,
    glyph: char,
    size: f32,
    color: Color32,
    angle: f32,
) {
    let format = TextFormat {
        font_id: FontId::new(size, named("lucide")),
        color,
        ..Default::default()
    };
    let galley = painter.layout_job(LayoutJob::single_section(glyph.to_string(), format));
    // Text shapes turn around their top-left corner: move it so the glyph's centre stays put.
    let half = Vec2::new(galley.size().x / 2.0, galley.size().y / 2.0);
    let turned = egui::emath::Rot2::from_angle(angle) * half;
    let shape = egui::epaint::TextShape::new(center - turned, galley, color).with_angle(angle);
    painter.add(shape);
}

/// The spinner's angle after `seconds`: one turn per 0.9 s.
pub fn spinner_angle(seconds: f32) -> f32 {
    seconds * std::f32::consts::TAU / 0.9
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_shortened_to_the_room_and_short_text_is_left_alone() {
        let ctx = egui::Context::default();
        let mut checked = false;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter();
            let style = Text::body(13.0, 400, Color32::WHITE);
            let long = "Private use; check upstream asset rights before redistribution";
            let full = width(painter, long, style);
            assert_eq!(ellipsize(painter, "short", style, full), "short");
            let shortened = ellipsize(painter, long, style, full / 2.0);
            assert!(shortened.ends_with('…'), "{shortened}");
            assert!(width(painter, &shortened, style) <= full / 2.0 + 0.01);
            assert!(shortened.len() > 8, "as much as fits is kept: {shortened}");
            assert_eq!(ellipsize(painter, long, style, 0.0), "");
            checked = true;
        });
        // A bare context uploads the font atlas; nothing paints it here.
        output.textures_delta.clear();
        assert!(checked);
    }

    #[test]
    fn the_spinner_turns_once_per_period_and_keeps_going() {
        assert_eq!(spinner_angle(0.0), 0.0);
        assert!((spinner_angle(0.9) - std::f32::consts::TAU).abs() < 1e-5);
        assert!(spinner_angle(2.0) > spinner_angle(1.0));
    }
}
