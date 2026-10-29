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
    fonts::draw_left(painter, rect.min.x + 40.0, cy, name, name_style);
    fonts::draw_right(
        painter,
        rect.max.x - 14.0,
        cy,
        right,
        Text::mono(12.0, 400, tokens.text_3),
    );
}
