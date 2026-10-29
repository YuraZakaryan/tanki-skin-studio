// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{CornerRadius, Pos2, Rect, Response, Role, Sense, Ui, Vec2};

use super::{alloc, describe};
use crate::{draw, fonts, fonts::Text, theme::Tokens};

/// A sidebar entry: icon, label and an optional count on the right. 42 high, full width.
pub fn nav_item(
    ui: &mut Ui,
    icon: char,
    label: &str,
    count: Option<&str>,
    active: bool,
) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = alloc(ui, Vec2::new(ui.available_width(), 42.0), Sense::click());
    describe(&response, Role::Button, true, Some(active), label);
    let painter = ui.painter();
    if active {
        draw::rounded(
            painter,
            rect,
            CornerRadius::same(10),
            tokens.primary_soft,
            None,
        );
    } else if response.hovered() {
        draw::rounded(
            painter,
            rect,
            CornerRadius::same(10),
            tokens.surface_3,
            None,
        );
    }
    let (ink, count_ink) = if active {
        (tokens.primary, tokens.primary)
    } else {
        (tokens.text_2, tokens.text_3)
    };
    let weight = if active { 600 } else { 500 };
    fonts::icon(
        painter,
        Pos2::new(rect.min.x + 14.0 + 9.0, rect.center().y),
        icon,
        18.0,
        ink,
    );
    fonts::draw_left(
        painter,
        rect.min.x + 44.0,
        rect.center().y,
        label,
        Text::body(14.0, weight, ink),
    );
    if let Some(count) = count {
        fonts::draw_right(
            painter,
            rect.max.x - 14.0,
            rect.center().y,
            count,
            Text::mono(12.0, 400, count_ink),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A switch of two or three segments (language, theme): 36 high, the active segment raised.
/// With `icons_only` the segments show only their icon (the label is the tooltip and the
/// accessible name). Returns the index of the segment that was clicked, if any.
pub fn segmented(
    ui: &mut Ui,
    options: &[(char, &str)],
    active: usize,
    icons_only: bool,
) -> Option<usize> {
    let tokens = Tokens::get(ui.ctx());
    let (rect, _) = alloc(ui, Vec2::new(ui.available_width(), 36.0), Sense::hover());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(10),
        tokens.surface_3,
        None,
    );
    let inner = rect.shrink(3.0);
    let count = options.len().max(1) as f32;
    let width = (inner.width() - 3.0 * (count - 1.0)) / count;
    let mut clicked = None;
    for (index, &(glyph, label)) in options.iter().enumerate() {
        let segment = Rect::from_min_size(
            Pos2::new(inner.min.x + index as f32 * (width + 3.0), inner.min.y),
            Vec2::new(width, inner.height()),
        );
        let response = ui.interact(
            segment,
            ui.scope_id().with(("segment", label)),
            Sense::click(),
        );
        describe(&response, Role::Button, true, Some(index == active), label);
        let on = index == active;
        if on {
            draw::rounded(
                ui.painter(),
                segment,
                CornerRadius::same(8),
                tokens.surface_2,
                None,
            );
        } else if response.hovered() {
            draw::rounded(
                ui.painter(),
                segment,
                CornerRadius::same(8),
                tokens.border,
                None,
            );
        }
        let ink = if on { tokens.primary } else { tokens.text_3 };
        if icons_only {
            fonts::icon(ui.painter(), segment.center(), glyph, 15.0, ink);
        } else {
            let style = Text::body(12.0, 600, if on { tokens.text } else { tokens.text_3 });
            let text_width = fonts::width(ui.painter(), label, style);
            let content = 14.0 + 6.0 + text_width;
            let start = segment.center().x - content / 2.0;
            fonts::icon(
                ui.painter(),
                Pos2::new(start + 7.0, segment.center().y),
                glyph,
                14.0,
                ink,
            );
            fonts::draw_left(ui.painter(), start + 20.0, segment.center().y, label, style);
        }
        if response.clicked() {
            clicked = Some(index);
        }
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if icons_only {
            response.on_hover_text(label);
        }
    }
    clicked
}

/// The page header: title and subtitle on the left (clipped when the actions need the room), the
/// closure's actions on the right. `rect` is 72 high; the rule under it is part of it.
pub fn top_bar(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    subtitle: &str,
    actions: impl FnOnce(&mut Ui),
) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(ui.painter(), rect, 0.0, tokens.bg, None);
    draw::hrule(
        ui.painter(),
        rect.min.x,
        rect.max.x,
        rect.max.y - 1.0,
        tokens.border,
    );
    let area = Rect::from_min_max(
        Pos2::new(rect.min.x + 32.0, rect.min.y + 16.0),
        Pos2::new(rect.max.x - 32.0, rect.min.y + 56.0),
    );
    let used = ui
        .scope_builder(
            egui::UiBuilder::new()
                .max_rect(area)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                actions(ui);
            },
        )
        .response
        .rect;
    // Actions are right-aligned: the room for the titles ends where the leftmost action starts.
    let right = if used.width() > 0.0 {
        used.min.x - 16.0
    } else {
        area.max.x
    };
    let painter = ui.painter().with_clip_rect(Rect::from_min_max(
        Pos2::new(rect.min.x, rect.min.y),
        Pos2::new(right.max(rect.min.x + 64.0), rect.max.y),
    ));
    fonts::draw(
        &painter,
        Pos2::new(rect.min.x + 32.0, rect.min.y + 12.0),
        title,
        Text::display(24.0, 700, tokens.text),
    );
    fonts::draw(
        &painter,
        Pos2::new(rect.min.x + 32.0, rect.min.y + 45.0),
        subtitle,
        Text::body(13.0, 400, tokens.text_3),
    );
}
