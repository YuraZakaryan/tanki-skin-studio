// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A select box (one choice of a short list, opened below the box) and a checkbox.

use egui::{CornerRadius, Frame, Pos2, Rect, Response, Role, Sense, Ui, Vec2};

use super::{alloc, describe, dim, interact};
use crate::{draw, fonts, fonts::Text, icons, theme::Tokens};

/// A 34-high select box in `rect` showing `shown`; a click opens `options` below it. Returns the
/// index of the option picked this frame. `label` is its accessible name.
pub fn select(
    ui: &mut Ui,
    rect: Rect,
    label: &str,
    shown: &str,
    options: &[String],
    enabled: bool,
) -> Option<usize> {
    let tokens = Tokens::get(ui.ctx());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = interact(ui, rect, ("select", label), sense);
    let value = shown.to_owned();
    let name = label.to_owned();
    response.widget_info(move || {
        let mut info = egui::WidgetInfo::labeled(Role::ComboBox, enabled, name.clone());
        info.current_text_value = Some(value.clone());
        info
    });
    let fill = if enabled && response.hovered() {
        tokens.border
    } else {
        tokens.surface_3
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(8),
        dim(fill, enabled),
        Some((1.0, dim(tokens.border_strong, enabled))),
    );
    let ink = if enabled { tokens.text } else { tokens.text_3 };
    let style = Text::body(13.0, 600, ink);
    let text = fonts::ellipsize(ui.painter(), shown, style, rect.width() - 20.0 - 22.0);
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 10.0,
        rect.center().y,
        &text,
        style,
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.max.x - 10.0 - 7.0, rect.center().y),
        icons::CHEVRON_DOWN,
        14.0,
        tokens.text_3,
    );
    if !enabled {
        return None;
    }
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let popup_id = egui::Popup::default_response_id(&response);
    let mut picked = None;
    egui::Popup::from_toggle_button_response(&response)
        .width(rect.width())
        .gap(4.0)
        .frame(
            Frame::new()
                .fill(tokens.surface_3)
                .stroke(egui::Stroke::new(1.0, tokens.border_strong))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(4.0),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
            for (index, option) in options.iter().enumerate() {
                let (row, hit) = alloc(ui, Vec2::new(rect.width() - 8.0, 30.0), Sense::click());
                describe(&hit, Role::Button, true, Some(option == shown), option);
                if hit.hovered() || option == shown {
                    draw::rounded(
                        ui.painter(),
                        row,
                        CornerRadius::same(6),
                        tokens.border,
                        None,
                    );
                }
                let style = Text::body(13.0, 600, tokens.text);
                let text = fonts::ellipsize(ui.painter(), option, style, row.width() - 16.0);
                fonts::draw_left(ui.painter(), row.min.x + 8.0, row.center().y, &text, style);
                if hit
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    picked = Some(index);
                }
            }
        });
    if picked.is_some() {
        egui::Popup::close_id(ui.ctx(), popup_id);
    }
    picked
}

/// The 20 × 20 box of a checkbox, ticked when `on`; `locked` greys it a little.
pub fn paint_checkbox(ui: &Ui, rect: Rect, on: bool, locked: bool) {
    let tokens = Tokens::get(ui.ctx());
    let alpha = if locked { 0.75 } else { 1.0 };
    let (fill, border) = if on {
        (tokens.primary, tokens.primary)
    } else {
        (egui::Color32::TRANSPARENT, tokens.border_strong)
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(5),
        draw::fade(fill, alpha),
        Some((1.5, draw::fade(border, alpha))),
    );
    if on {
        fonts::icon(
            ui.painter(),
            rect.center(),
            icons::CHECK,
            14.0,
            tokens.on_primary,
        );
    }
}

/// A checkbox with its label to the right, in the layout flow. `value` flips on click.
pub fn checkbox(ui: &mut Ui, value: &mut bool, label: &str) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let style = Text::body(13.0, 600, tokens.text);
    let width = 20.0 + 8.0 + fonts::width(ui.painter(), label, style);
    let (rect, mut response) = alloc(ui, Vec2::new(width, 24.0), Sense::click());
    describe(&response, Role::CheckBox, true, Some(*value), label);
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let boxed = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.center().y - 10.0),
        Vec2::splat(20.0),
    );
    paint_checkbox(ui, boxed, *value, false);
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 28.0,
        rect.center().y,
        label,
        style,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
