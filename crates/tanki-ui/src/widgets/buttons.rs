// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{CornerRadius, Pos2, Rect, Response, Role, Sense, Ui, Vec2};

use super::{alloc, describe, dim};
use crate::{draw, fonts, fonts::Text, theme::Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// Lime: the main action of a screen.
    Primary,
    /// Amber: replaces files in the game.
    Accent,
    /// Bordered neutral button.
    Secondary,
    /// Text only.
    Ghost,
    /// Red tint: destructive or undoing.
    Danger,
}

/// How wide a button is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Width {
    /// Fits its content with 18-point side padding.
    Content,
    /// Exactly this wide (content centred).
    Exact(f32),
    /// The full width of the parent.
    Fill,
}

pub struct Button<'a> {
    pub kind: ButtonKind,
    pub icon: Option<char>,
    pub label: &'a str,
    pub width: Width,
    pub height: f32,
    pub enabled: bool,
}

impl<'a> Button<'a> {
    pub fn new(kind: ButtonKind, label: &'a str) -> Self {
        Self {
            kind,
            icon: None,
            label,
            width: Width::Content,
            height: 40.0,
            enabled: true,
        }
    }
    pub fn icon(mut self, icon: char) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn width(mut self, width: Width) -> Self {
        self.width = width;
        self
    }
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// The width the button takes when it fits its content.
    pub fn measure(&self, ui: &Ui) -> f32 {
        let style = Text::display(14.0, 600, egui::Color32::WHITE);
        let label = fonts::width(ui.painter(), self.label, style);
        self.icon.map_or(0.0, |_| 16.0 + 8.0) + label + 36.0
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let tokens = Tokens::get(ui.ctx());
        let (fill, border, ink) = match self.kind {
            ButtonKind::Primary => (tokens.primary, None, tokens.on_primary),
            ButtonKind::Accent => (tokens.accent, None, tokens.on_accent),
            ButtonKind::Secondary => (
                tokens.surface_3,
                Some((1.0, tokens.border_strong)),
                tokens.text,
            ),
            ButtonKind::Ghost => (egui::Color32::TRANSPARENT, None, tokens.text_2),
            ButtonKind::Danger => (tokens.danger_soft, None, tokens.danger),
        };
        let style = Text::display(14.0, 600, dim(ink, self.enabled));
        let label_width = fonts::width(ui.painter(), self.label, style);
        let content = self.icon.map_or(0.0, |_| 16.0 + 8.0) + label_width;
        let width = match self.width {
            Width::Content => content + 36.0,
            Width::Exact(w) => w,
            Width::Fill => ui.available_width(),
        };
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, response) = alloc(ui, Vec2::new(width, self.height), sense);
        describe(&response, Role::Button, self.enabled, None, self.label);
        paint(ui, rect, &response, self.kind, fill, border, self.enabled);
        let start = rect.center().x - content / 2.0;
        let mut x = start;
        if let Some(icon) = self.icon {
            fonts::icon(
                ui.painter(),
                Pos2::new(x + 8.0, rect.center().y),
                icon,
                16.0,
                style.color,
            );
            x += 24.0;
        }
        fonts::draw_left(ui.painter(), x, rect.center().y, self.label, style);
        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

fn paint(
    ui: &Ui,
    rect: Rect,
    response: &Response,
    kind: ButtonKind,
    fill: egui::Color32,
    border: Option<(f32, egui::Color32)>,
    enabled: bool,
) {
    let tokens = Tokens::get(ui.ctx());
    let mut fill = fill;
    if enabled && response.hovered() {
        fill = match kind {
            ButtonKind::Ghost => tokens.surface_3,
            ButtonKind::Danger => tokens.danger_soft.gamma_multiply(1.8),
            _ if tokens.is_dark() => draw::lighten(fill, 0.1),
            _ => draw::fade(fill, 0.88),
        };
    }
    if enabled && response.is_pointer_button_down_on() {
        fill = draw::fade(fill, 0.85);
    }
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(8),
        dim(fill, enabled),
        border,
    );
}

/// The 38-point square icon button (close, delete, settings); `label` is its accessible name.
pub fn icon_button(ui: &mut Ui, icon: char, label: &str) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = alloc(ui, Vec2::splat(38.0), Sense::click());
    describe(&response, Role::Button, true, None, label);
    let fill = if response.hovered() {
        tokens.border
    } else {
        tokens.surface_3
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(8),
        fill,
        Some((1.0, tokens.border)),
    );
    fonts::icon(ui.painter(), rect.center(), icon, 18.0, tokens.text_2);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
