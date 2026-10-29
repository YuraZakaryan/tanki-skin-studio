use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};

use super::alloc;
use crate::{draw, fonts, fonts::Text, theme::Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipKind {
    Success,
    Info,
    Warning,
    Danger,
    Neutral,
}

fn colors(tokens: &Tokens, kind: ChipKind) -> (Color32, Color32) {
    match kind {
        ChipKind::Success => (tokens.primary_soft, tokens.primary),
        ChipKind::Info => (tokens.info_soft, tokens.info),
        ChipKind::Warning => (tokens.accent_soft, tokens.accent),
        ChipKind::Danger => (tokens.danger_soft, tokens.danger),
        ChipKind::Neutral => (tokens.surface_3, tokens.text_2),
    }
}

/// Width of a chip with this label.
pub fn chip_width(ui: &Ui, label: &str) -> f32 {
    let width = fonts::width(ui.painter(), label, Text::body(12.0, 600, Color32::WHITE));
    10.0 + 6.0 + 6.0 + width + 10.0
}

/// Paint a chip (pill with a dot and a label) at `rect`'s left edge, `rect` being 24 high.
pub fn paint_chip(ui: &Ui, rect: Rect, kind: ChipKind, label: &str) {
    let tokens = Tokens::get(ui.ctx());
    let (fill, ink) = colors(&tokens, kind);
    draw::rounded(ui.painter(), rect, CornerRadius::same(12), fill, None);
    draw::dot(
        ui.painter(),
        Pos2::new(rect.min.x + 13.0, rect.center().y),
        6.0,
        ink,
    );
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 22.0,
        rect.center().y,
        label,
        Text::body(12.0, 600, ink),
    );
}

/// A chip in the layout flow.
pub fn chip(ui: &mut Ui, kind: ChipKind, label: &str) {
    let width = chip_width(ui, label);
    let (rect, _) = alloc(ui, Vec2::new(width, 24.0), Sense::hover());
    paint_chip(ui, rect, kind, label);
}

/// A chip whose right edge is at `right`, vertically centred on `center_y`. Returns its width.
pub fn chip_at_right(ui: &Ui, right: f32, center_y: f32, kind: ChipKind, label: &str) -> f32 {
    let width = chip_width(ui, label);
    let rect = Rect::from_min_size(
        Pos2::new(right - width, center_y - 12.0),
        Vec2::new(width, 24.0),
    );
    paint_chip(ui, rect, kind, label);
    width
}

/// Key caps use the mono face for plain text (`esc`, `K`); symbols such as `↵` and `⌘` are not in
/// the mono font, so they are drawn with the body face.
fn kbd_style(label: &str, ink: Color32) -> Text {
    if label.is_ascii() {
        Text::mono(11.0, 400, ink)
    } else {
        Text::body(11.0, 500, ink)
    }
}

/// The small keyboard-key cap (`⌘K`, `esc`, `↵`).
pub fn kbd_width(ui: &Ui, label: &str) -> f32 {
    fonts::width(ui.painter(), label, kbd_style(label, Color32::WHITE)).ceil() + 12.0
}

/// Paint a key cap with its right edge at `right`; returns its width.
pub fn paint_kbd(ui: &Ui, right: f32, center_y: f32, label: &str, ink: Color32) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    let width = kbd_width(ui, label);
    let rect = Rect::from_min_size(
        Pos2::new(right - width, center_y - 9.5),
        Vec2::new(width, 19.0),
    );
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(4),
        tokens.surface_3,
        None,
    );
    fonts::draw_centered(ui.painter(), rect, label, kbd_style(label, ink));
    width
}
