// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A large dialog for a task of several steps: a header with the title, a subtitle and the steps
//! (done ones ticked, the current one raised), a body the caller lays out itself, and a footer
//! with a hint on the left and the buttons on the right.

use egui::{
    Area, Context, CornerRadius, Id, Layout, Order, Pos2, Rect, Sense, Ui, UiBuilder, Vec2,
};

use super::{ModalEvent, icon_button};
use crate::{draw, fonts, fonts::Text, icons, theme::Tokens};

/// What a [`wizard`] shows around its body.
pub struct WizardSpec<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub subtitle: Option<&'a str>,
    /// The steps' names; empty for none.
    pub steps: &'a [&'a str],
    pub current: usize,
    /// The accessible name of the × button.
    pub close_label: &'a str,
    /// Whether the × button, Esc and a click outside close it (not while work is under way).
    pub closable: bool,
    /// The size it wants; a smaller window gets a smaller dialog, 16 points from its edges.
    pub size: Vec2,
    /// An icon and a line on the left of the footer.
    pub hint: Option<(char, &'a str)>,
}

/// The header's height with a subtitle (the title and subtitle, 18 above and below), and with
/// the title alone.
pub const WIZARD_HEADER: f32 = 80.0;
pub const WIZARD_HEADER_PLAIN: f32 = 74.0;
/// The footer's height.
pub const WIZARD_FOOTER: f32 = 72.0;

/// The rectangle the wizard takes on this screen.
pub fn wizard_rect(ctx: &Context, size: Vec2) -> Rect {
    let screen = ctx.content_rect();
    let size = Vec2::new(
        size.x.min(screen.width() - 32.0).max(320.0),
        size.y.min(screen.height() - 32.0).max(260.0),
    );
    Rect::from_center_size(screen.center(), size)
}

/// Width of the steps, drawn right to left from `right`.
fn steps_width(painter: &egui::Painter, steps: &[&str]) -> f32 {
    steps
        .iter()
        .map(|s| {
            6.0 + 22.0
                + 8.0
                + fonts::width(painter, s, Text::body(13.0, 600, egui::Color32::WHITE))
                + 12.0
        })
        .sum::<f32>()
        + steps.len().saturating_sub(1) as f32 * (6.0 + 14.0 + 6.0)
}

fn paint_steps(ui: &Ui, left: f32, center_y: f32, steps: &[&str], current: usize) {
    let tokens = Tokens::get(ui.ctx());
    let mut x = left;
    for (index, step) in steps.iter().enumerate() {
        let (done, now) = (index < current, index == current);
        let style = Text::body(
            13.0,
            600,
            if now {
                tokens.primary
            } else if done {
                tokens.text_2
            } else {
                tokens.text_3
            },
        );
        let width = 6.0 + 22.0 + 8.0 + fonts::width(ui.painter(), step, style) + 12.0;
        let pill = Rect::from_min_size(Pos2::new(x, center_y - 16.0), Vec2::new(width, 32.0));
        if now {
            draw::rounded(
                ui.painter(),
                pill,
                CornerRadius::same(16),
                tokens.primary_soft,
                None,
            );
        }
        let dot = Rect::from_center_size(Pos2::new(x + 6.0 + 11.0, center_y), Vec2::splat(22.0));
        let fill = if done || now {
            tokens.primary
        } else {
            tokens.surface_3
        };
        draw::rounded(ui.painter(), dot, CornerRadius::same(11), fill, None);
        if done {
            fonts::icon(
                ui.painter(),
                dot.center(),
                icons::CHECK,
                13.0,
                tokens.on_primary,
            );
        } else {
            let ink = if now {
                tokens.on_primary
            } else {
                tokens.text_3
            };
            fonts::draw_centered(
                ui.painter(),
                dot,
                &(index + 1).to_string(),
                Text::mono(12.0, 700, ink),
            );
        }
        fonts::draw_left(ui.painter(), x + 6.0 + 22.0 + 8.0, center_y, step, style);
        x += width;
        if index + 1 < steps.len() {
            fonts::icon(
                ui.painter(),
                Pos2::new(x + 6.0 + 7.0, center_y),
                icons::CHEVRON_RIGHT,
                14.0,
                tokens.text_3,
            );
            x += 6.0 + 14.0 + 6.0;
        }
    }
}

/// Draw the scrim and the wizard. `body` gets a `Ui` and the body's rectangle (the dialog less its
/// header and footer, no padding); `footer` lays its buttons out right to left (the primary one
/// first). Returns [`ModalEvent::Dismissed`] when a closable wizard was closed.
pub fn wizard(
    ctx: &Context,
    spec: &WizardSpec<'_>,
    body: impl FnOnce(&mut Ui, Rect),
    footer: impl FnOnce(&mut Ui),
) -> ModalEvent {
    let tokens = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let rect = wizard_rect(ctx, spec.size);
    let mut event = ModalEvent::None;
    Area::new(Id::unique(("wizard", spec.id)))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let scrim = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(screen, 0.0, tokens.scrim);
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
            let inside = ui.interact(rect, ui.scope_id().with("inside"), Sense::click());
            let title = spec.title.to_owned();
            inside.widget_info(move || {
                egui::WidgetInfo::labeled(egui::Role::Dialog, true, title.clone())
            });
            if spec.closable
                && scrim.clicked()
                && !scrim.double_clicked()
                && !inside.hovered()
                && !rect.contains(scrim.interact_pointer_pos().unwrap_or_default())
            {
                event = ModalEvent::Dismissed;
            }
            // Header: the title and subtitle, the steps, the × button.
            let header_height = if spec.subtitle.is_some() {
                WIZARD_HEADER
            } else {
                WIZARD_HEADER_PLAIN
            };
            let header = Rect::from_min_size(rect.min, Vec2::new(rect.width(), header_height));
            draw::hrule(
                ui.painter(),
                rect.min.x + 1.0,
                rect.max.x - 1.0,
                header.max.y,
                tokens.border,
            );
            let mut right = rect.max.x - 24.0;
            if spec.closable {
                let close = Rect::from_min_size(
                    Pos2::new(right - 38.0, header.center().y - 19.0),
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
                right = close.min.x - 16.0;
            }
            let left = rect.min.x + 24.0;
            if !spec.steps.is_empty() {
                let width = steps_width(ui.painter(), spec.steps);
                // The steps give way to the title in a narrow window.
                if right - width - left >= 220.0 {
                    paint_steps(
                        ui,
                        right - width,
                        header.center().y,
                        spec.steps,
                        spec.current,
                    );
                    right -= width + 16.0;
                }
            }
            let room = (right - left).max(0.0);
            let title_style = Text::display(20.0, 700, tokens.text);
            let title = fonts::ellipsize(ui.painter(), spec.title, title_style, room);
            match spec.subtitle {
                Some(subtitle) => {
                    fonts::draw_left(
                        ui.painter(),
                        left,
                        header.min.y + 18.0 + 13.0,
                        &title,
                        title_style,
                    );
                    let style = Text::body(13.0, 400, tokens.text_3);
                    let subtitle = fonts::ellipsize(ui.painter(), subtitle, style, room);
                    fonts::draw_left(
                        ui.painter(),
                        left,
                        header.min.y + 18.0 + 26.0 + 3.0 + 8.0,
                        &subtitle,
                        style,
                    );
                }
                None => {
                    fonts::draw_left(ui.painter(), left, header.center().y, &title, title_style);
                }
            }
            // Body.
            let footer_top = rect.max.y - WIZARD_FOOTER;
            let body_rect = Rect::from_min_max(
                Pos2::new(rect.min.x + 1.0, header.max.y + 1.0),
                Pos2::new(rect.max.x - 1.0, footer_top),
            );
            ui.scope_builder(UiBuilder::new().max_rect(body_rect), |ui| {
                ui.set_clip_rect(body_rect);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                body(ui, body_rect);
            });
            // Footer: buttons right to left, then the hint in the room they leave.
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
            let used = ui
                .scope_builder(
                    UiBuilder::new()
                        .max_rect(buttons)
                        .layout(Layout::right_to_left(egui::Align::Center)),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 12.0;
                        footer(ui);
                        ui.min_rect().width()
                    },
                )
                .inner;
            if let Some((icon, hint)) = spec.hint {
                // The icon, 8 to the text, and 12 to the buttons.
                let room = buttons.width() - used - 12.0 - 15.0 - 8.0;
                let y = footer_top + WIZARD_FOOTER / 2.0;
                fonts::icon(
                    ui.painter(),
                    Pos2::new(buttons.min.x + 7.5, y),
                    icon,
                    15.0,
                    tokens.text_3,
                );
                let style = Text::body(13.0, 400, tokens.text_2);
                let galley =
                    fonts::paragraph(ui.painter(), hint, style, room.max(40.0), Some(17.0));
                let top = y - galley.size().y / 2.0;
                ui.painter()
                    .galley(Pos2::new(buttons.min.x + 23.0, top), galley, tokens.text_2);
            }
        });
    // Taken, so a dialog that closing opens (a question) does not see the same Esc and close too.
    if spec.closable
        && !egui::Popup::is_any_open(ctx)
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        event = ModalEvent::Dismissed;
    }
    event
}
