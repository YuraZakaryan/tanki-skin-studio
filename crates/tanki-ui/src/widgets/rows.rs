// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{CornerRadius, Pos2, Rect, Response, Role, Sense, Ui, Vec2};

use super::{alloc, chips::*, describe};
use crate::{draw, fonts, fonts::Text, theme::Tokens};

/// One entry of a list (pack, resource, backup, modification): thumbnail, two lines, optional
/// chip and size on the right.
pub struct ListRow<'a> {
    pub icon: char,
    pub name: &'a str,
    pub meta: &'a str,
    pub selected: bool,
    pub chip: Option<(ChipKind, &'a str)>,
    pub size: Option<&'a str>,
    pub clickable: bool,
}

impl<'a> ListRow<'a> {
    pub fn new(icon: char, name: &'a str, meta: &'a str) -> Self {
        Self {
            icon,
            name,
            meta,
            selected: false,
            chip: None,
            size: None,
            clickable: true,
        }
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    pub fn chip(mut self, kind: ChipKind, label: &'a str) -> Self {
        self.chip = Some((kind, label));
        self
    }
    pub fn size(mut self, size: &'a str) -> Self {
        self.size = Some(size);
        self
    }
    pub fn clickable(mut self, clickable: bool) -> Self {
        self.clickable = clickable;
        self
    }

    /// Show a full-width, 56-high row in the layout flow.
    pub fn show(self, ui: &mut Ui) -> Response {
        let width = ui.available_width();
        let sense = if self.clickable {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = alloc(ui, Vec2::new(width, 56.0), sense);
        describe(
            &response,
            Role::Button,
            self.clickable,
            Some(self.selected),
            self.name,
        );
        self.paint(ui, rect, &response);
        if self.clickable {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response) {
        let tokens = Tokens::get(ui.ctx());
        let painter = ui.painter();
        if self.selected {
            draw::rounded(
                painter,
                rect,
                CornerRadius::same(10),
                tokens.primary_soft,
                None,
            );
        } else if self.clickable && response.hovered() {
            draw::rounded(
                painter,
                rect,
                CornerRadius::same(10),
                tokens.surface_3,
                None,
            );
        }
        let cy = rect.center().y;
        let thumb =
            Rect::from_center_size(Pos2::new(rect.min.x + 14.0 + 18.0, cy), Vec2::splat(36.0));
        draw::rounded(
            painter,
            thumb,
            CornerRadius::same(8),
            tokens.surface_3,
            None,
        );
        fonts::icon(painter, thumb.center(), self.icon, 16.0, tokens.text_2);
        let text_x = rect.min.x + 14.0 + 36.0 + 14.0;
        // Right side first, so the texts can be clipped to the room that is left.
        let mut right = rect.max.x - 14.0;
        if let Some(size) = self.size {
            let used = fonts::draw_right(
                painter,
                right,
                cy,
                size,
                Text::mono(12.0, 400, tokens.text_3),
            );
            right = used.min.x - 14.0;
        }
        if let Some((kind, label)) = self.chip {
            let width = chip_at_right(ui, right, cy, kind, label);
            right -= width + 14.0;
        }
        let room = (right - text_x).max(0.0);
        let name_style = Text::body(14.0, 600, tokens.text);
        let meta_style = Text::body(12.0, 400, tokens.text_3);
        let name = fonts::ellipsize(painter, self.name, name_style, room);
        let meta = fonts::ellipsize(painter, self.meta, meta_style, room);
        fonts::draw(painter, Pos2::new(text_x, cy - 17.0), &name, name_style);
        fonts::draw(painter, Pos2::new(text_x, cy + 2.0), &meta, meta_style);
    }
}

/// One option of a single choice, drawn as a bordered card: a radio mark, the name with chips on
/// the right, and a mono detail line (a path) with a short note on the right. The detail is
/// shortened in its middle to fit; `tooltip` (the whole path) shows on hover.
pub struct ChoiceRow<'a> {
    pub name: &'a str,
    pub detail: &'a str,
    pub tooltip: &'a str,
    pub note: &'a str,
    pub chips: &'a [(ChipKind, &'a str)],
    pub selected: bool,
}

/// The height of a [`ChoiceRow`].
pub const CHOICE_ROW_HEIGHT: f32 = 64.0;

impl ChoiceRow<'_> {
    /// Show a full-width row in the layout flow.
    pub fn show(&self, ui: &mut Ui) -> Response {
        let width = ui.available_width();
        let (rect, response) = alloc(ui, Vec2::new(width, CHOICE_ROW_HEIGHT), Sense::click());
        describe(
            &response,
            Role::RadioButton,
            true,
            Some(self.selected),
            self.name,
        );
        self.paint(ui, rect, &response);
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if self.tooltip.is_empty() {
            response
        } else {
            response.on_hover_text(self.tooltip)
        }
    }

    fn paint(&self, ui: &Ui, rect: Rect, response: &Response) {
        let tokens = Tokens::get(ui.ctx());
        let painter = ui.painter();
        let (fill, border) = if self.selected {
            (tokens.primary_soft, tokens.primary)
        } else if response.hovered() {
            (tokens.surface_3, tokens.border_strong)
        } else {
            (tokens.surface, tokens.border)
        };
        draw::rounded(
            painter,
            rect,
            CornerRadius::same(12),
            fill,
            Some((1.0, border)),
        );
        let top = rect.min.y + 22.0;
        let bottom = rect.min.y + 44.0;
        let radio = Pos2::new(rect.min.x + 16.0 + 9.0, top);
        let ring = if self.selected {
            tokens.primary
        } else {
            tokens.text_3
        };
        painter.circle_stroke(radio, 8.0, egui::Stroke::new(1.5, ring));
        if self.selected {
            draw::dot(painter, radio, 9.0, tokens.primary);
        }
        let text_x = rect.min.x + 16.0 + 18.0 + 12.0;
        let mut right = rect.max.x - 16.0;
        for (kind, label) in self.chips.iter().rev() {
            let width = chip_at_right(ui, right, top, *kind, label);
            right -= width + 8.0;
        }
        let name_style = Text::body(14.0, 600, tokens.text);
        let name = fonts::ellipsize(painter, self.name, name_style, (right - text_x).max(0.0));
        fonts::draw_left(painter, text_x, top, &name, name_style);
        let note_style = Text::body(12.0, 400, tokens.text_3);
        let used = fonts::draw_right(painter, rect.max.x - 16.0, bottom, self.note, note_style);
        let detail_style = Text::mono(12.0, 400, tokens.text_3);
        let room = (used.min.x - 16.0 - text_x).max(0.0);
        let detail = fonts::ellipsize_middle(painter, self.detail, detail_style, room);
        fonts::draw_left(painter, text_x, bottom, &detail, detail_style);
    }
}

/// One file of a list inside a bordered box (the files of a replacement, validation errors).
pub fn file_line(
    ui: &Ui,
    rect: Rect,
    icon: char,
    icon_color: egui::Color32,
    name: &str,
    right: &str,
    name_mono: bool,
) {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    let cy = rect.center().y;
    fonts::icon(
        painter,
        Pos2::new(rect.min.x + 14.0 + 8.0, cy),
        icon,
        16.0,
        icon_color,
    );
    let name_style = if name_mono {
        Text::mono(13.0, 400, tokens.text)
    } else {
        Text::body(13.0, 400, tokens.text)
    };
    // The right text first: the name gives way to it, shortened with an ellipsis.
    let used = fonts::draw_right(
        painter,
        rect.max.x - 14.0,
        cy,
        right,
        Text::mono(12.0, 400, tokens.text_3),
    );
    let room = (used.min.x - 14.0 - (rect.min.x + 40.0)).max(0.0);
    let name = fonts::ellipsize(painter, name, name_style, room);
    fonts::draw_left(painter, rect.min.x + 40.0, cy, &name, name_style);
}
