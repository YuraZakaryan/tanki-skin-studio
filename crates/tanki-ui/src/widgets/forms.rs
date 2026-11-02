// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{Color32, CornerRadius, Pos2, Rect, Response, Role, Sense, TextEdit, Ui, Vec2};

use super::{alloc, describe, interact};
use crate::{draw, fonts, fonts::Text, icons, layout, theme::Tokens};

/// A text field in a bordered box with a leading icon and an optional key hint, 40 high. Pass the
/// rectangle (usually from [`layout`]); returns the text edit's response.
pub fn text_field(
    ui: &mut Ui,
    rect: Rect,
    icon: char,
    hint: &str,
    text: &mut String,
    key_hint: Option<&str>,
) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let id = ui.scope_id().with(("text-field", hint));
    let focused = ui.ctx().memory(|m| m.has_focus(id));
    let border = if focused {
        tokens.primary
    } else {
        tokens.border
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(8),
        tokens.surface_2,
        Some((1.0, border)),
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 20.0, rect.center().y),
        icon,
        16.0,
        tokens.text_3,
    );
    let mut right = rect.max.x - 12.0;
    if let Some(key) = key_hint {
        right -= draw_kbd_hint(ui, right, rect.center().y, key) + 10.0;
    }
    let edit = Rect::from_min_max(
        Pos2::new(rect.min.x + 38.0, rect.min.y + 2.0),
        Pos2::new(right, rect.max.y - 2.0),
    );
    layout::in_rect(ui, edit, |ui| {
        ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
        ui.add_sized(
            edit.size(),
            TextEdit::singleline(text)
                .id(id)
                .frame(egui::Frame::NONE)
                .hint_text(egui::RichText::new(hint).color(tokens.text_3).size(13.0))
                .font(egui::FontId::new(
                    13.0,
                    egui::FontFamily::Name("inter".into()),
                ))
                .text_color(tokens.text)
                .align(egui::Align2::LEFT_CENTER),
        )
    })
}

fn draw_kbd_hint(ui: &Ui, right: f32, center_y: f32, key: &str) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    super::paint_kbd(ui, right, center_y, key, tokens.text_3)
}

/// The 40 × 22 on/off switch. Returns the response; `value` flips on click.
pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let (rect, mut response) = alloc(ui, Vec2::new(40.0, 22.0), Sense::click());
    describe(&response, Role::CheckBox, true, Some(*value), label);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let (fill, knob) = if *value {
        (tokens.primary, tokens.on_primary)
    } else {
        // The knob keeps its colour, as the design's switch does: only the track changes.
        (tokens.surface_3, tokens.on_primary)
    };
    draw::rounded(ui.painter(), rect, CornerRadius::same(11), fill, None);
    let x = if *value {
        rect.max.x - 3.0 - 8.0
    } else {
        rect.min.x + 3.0 + 8.0
    };
    ui.painter()
        .circle_filled(Pos2::new(x, rect.center().y), 8.0, knob);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A label on the left and its value on the right of `rect` (one 16-high line). Returns whether
/// the value had to be shortened to fit, so the caller can offer the whole of it.
pub fn key_value(ui: &Ui, rect: Rect, key: &str, value: &str, mono: bool) -> bool {
    let tokens = Tokens::get(ui.ctx());
    fonts::draw_left(
        ui.painter(),
        rect.min.x,
        rect.center().y,
        key,
        Text::body(13.0, 400, tokens.text_3),
    );
    let style = if mono {
        Text::mono(12.0, 400, tokens.text)
    } else {
        Text::body(13.0, 600, tokens.text)
    };
    // A long value gives way: it is shortened to the room left of the key.
    let key_width = fonts::width(ui.painter(), key, Text::body(13.0, 400, tokens.text_3));
    let room = rect.width() - key_width - 16.0;
    let shown = fonts::ellipsize(ui.painter(), value, style, room);
    fonts::draw_right(ui.painter(), rect.max.x, rect.center().y, &shown, style);
    shown.as_ref() != value
}

/// A horizontal progress bar: 6 high track with a lime fill (`fraction` in 0..=1).
pub fn progress_bar(ui: &Ui, rect: Rect, fraction: f32) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(3),
        tokens.surface_3,
        None,
    );
    let fill = Rect::from_min_size(
        rect.min,
        Vec2::new(rect.width() * fraction.clamp(0.0, 1.0), rect.height()),
    );
    if fill.width() > 0.0 {
        draw::rounded(
            ui.painter(),
            fill,
            CornerRadius::same(3),
            tokens.primary,
            None,
        );
    }
}

/// The width a [`tab`] with this label takes, so a row of tabs can be fitted before drawing.
pub fn tab_width(ui: &Ui, label: &str) -> f32 {
    let style = Text::body(13.0, 600, Tokens::get(ui.ctx()).text);
    fonts::width(ui.painter(), label, style) + 28.0
}

/// A tab (category filter): 34 high, text-sized. Returns the response.
pub fn tab(ui: &mut Ui, label: &str, active: bool) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let ink = if active { tokens.text } else { tokens.text_2 };
    let style = Text::body(13.0, 600, ink);
    let width = tab_width(ui, label);
    let (rect, response) = alloc(ui, Vec2::new(width, 34.0), Sense::click());
    describe(&response, Role::Button, true, Some(active), label);
    if active {
        draw::rounded(
            ui.painter(),
            rect,
            CornerRadius::same(8),
            tokens.surface_3,
            Some((1.0, tokens.border_strong)),
        );
    } else if response.hovered() {
        draw::rounded(
            ui.painter(),
            rect,
            CornerRadius::same(8),
            tokens.surface_2,
            None,
        );
    }
    fonts::draw_centered(ui.painter(), rect, label, style);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A modification chip (M0..M3) of the given width and 30 high.
pub fn grade_chip(ui: &mut Ui, label: &str, active: bool, present: bool, width: f32) -> Response {
    let (rect, response) = alloc(ui, Vec2::new(width, 30.0), Sense::click());
    describe(&response, Role::Button, true, Some(active), label);
    super::grade_chip_paint(ui, rect, label, active, present, response.hovered());
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A bordered "label: value" box such as the checked locations in the empty state.
pub fn info_box(ui: &Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
}

/// Make a rectangle clickable without painting anything (hit area for custom-drawn rows).
pub fn hit(ui: &mut Ui, rect: Rect, salt: impl std::hash::Hash + std::fmt::Debug) -> Response {
    interact(ui, rect, salt, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// An icon-only decoration helper used by headers: the lime leading icon.
pub fn lead_icon(ui: &Ui, center: Pos2, glyph: char) {
    let tokens = Tokens::get(ui.ctx());
    fonts::icon(ui.painter(), center, glyph, 18.0, tokens.primary);
}

/// The magnifier used by search boxes.
pub const SEARCH: char = icons::SEARCH;
