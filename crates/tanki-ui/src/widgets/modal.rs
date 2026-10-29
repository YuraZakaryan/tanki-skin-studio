// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use egui::{
    Area, Context, CornerRadius, Id, Layout, Order, Pos2, Rect, Sense, Ui, UiBuilder, Vec2,
};

use crate::{draw, fonts, fonts::Text, icons, theme::Tokens, widgets::icon_button};

/// Size and content description of a dialog.
pub struct ModalSpec<'a> {
    pub id: &'a str,
    pub title: &'a str,
    /// The accessible name of the × button (translated by the caller).
    pub close_label: &'a str,
    pub width: f32,
    /// Height of the body area between header (78) and footer (72), bottom padding included.
    pub body_height: f32,
}

/// What the user did with the dialog this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalEvent {
    None,
    /// Closed with the × button, Esc, or a click outside.
    Dismissed,
}

/// Draw the scrim, then a centred dialog: header with title and close button, the body closure
/// (given a `Ui` inset by 24 on the sides), a footer rule and the footer closure, which lays its
/// buttons out right to left (put the primary action first).
pub fn modal(
    ctx: &Context,
    spec: &ModalSpec<'_>,
    body: impl FnOnce(&mut Ui),
    footer: impl FnOnce(&mut Ui),
) -> ModalEvent {
    let tokens = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let mut event = ModalEvent::None;
    Area::new(Id::unique(("modal", spec.id)))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let scrim = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(screen, 0.0, tokens.scrim);
            let height = 78.0 + spec.body_height + 72.0;
            let rect = Rect::from_center_size(screen.center(), Vec2::new(spec.width, height));
            draw::shadow(
                ui.painter(),
                rect,
                18.0,
                24,
                48,
                egui::Color32::from_black_alpha(0x80),
            );
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(18),
                tokens.surface_2,
                Some((1.0, tokens.border_strong)),
            );
            // The dialog itself swallows clicks so only the scrim outside it dismisses.
            let inside = ui.interact(rect, ui.scope_id().with("inside"), Sense::click());
            if scrim.clicked()
                && !inside.hovered()
                && !rect.contains(scrim.interact_pointer_pos().unwrap_or_default())
            {
                event = ModalEvent::Dismissed;
            }
            fonts::draw_left(
                ui.painter(),
                rect.min.x + 24.0,
                rect.min.y + 39.0,
                spec.title,
                Text::display(20.0, 700, tokens.text),
            );
            let close = Rect::from_min_size(
                Pos2::new(rect.max.x - 24.0 - 38.0, rect.min.y + 20.0),
                Vec2::splat(38.0),
            );
            let closed = ui
                .scope_builder(UiBuilder::new().max_rect(close), |ui| {
                    icon_button(ui, icons::X, spec.close_label)
                })
                .inner;
            if closed.clicked() {
                event = ModalEvent::Dismissed;
            }
            let body_rect = Rect::from_min_size(
                Pos2::new(rect.min.x + 24.0, rect.min.y + 78.0),
                Vec2::new(spec.width - 48.0, spec.body_height - 20.0),
            );
            ui.scope_builder(UiBuilder::new().max_rect(body_rect), |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                body(ui);
            });
            let footer_top = rect.max.y - 72.0;
            draw::hrule(
                ui.painter(),
                rect.min.x + 1.0,
                rect.max.x - 1.0,
                footer_top,
                tokens.border,
            );
            let buttons = Rect::from_min_max(
                Pos2::new(rect.min.x + 24.0, footer_top + 16.0),
                Pos2::new(rect.max.x - 24.0, footer_top + 56.0),
            );
            ui.scope_builder(
                UiBuilder::new()
                    .max_rect(buttons)
                    .layout(Layout::right_to_left(egui::Align::Center)),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    footer(ui);
                },
            );
        });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        event = ModalEvent::Dismissed;
    }
    event
}
