// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Short messages in the bottom-right corner, with an optional action (Undo).

use eframe::egui::{self, Area, Context, CornerRadius, Id, Order, Pos2, Rect, Sense, Vec2};
use tanki_i18n::tl;
use tanki_ui::{draw, fonts, fonts::Text, icons, theme::Tokens};

use crate::app::{Studio, ToastAction};

const WIDTH: f32 = 440.0;
const HEIGHT: f32 = 48.0;

pub fn draw(app: &mut Studio, ctx: &Context) {
    let tokens = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let mut run = None;
    let mut dismissed = None;
    // The card of a running import sits in the corner; the toasts stack above it.
    let mut lift = if app.import_job.is_some() {
        super::progress::HEIGHT + 8.0
    } else {
        0.0
    };
    // Over "Apply to all items", above its buttons, which reach the corner in a small window.
    if let Some(wizard) = super::bulk::rect(app, ctx) {
        let footer = wizard.max.y - tanki_ui::widgets::WIZARD_FOOTER - 8.0;
        lift = lift.max(screen.max.y - 24.0 - footer);
    }
    for (index, toast) in app.toasts.iter().enumerate() {
        let from_bottom = (app.toasts.len() - 1 - index) as f32;
        let pos = Pos2::new(
            screen.max.x - 24.0 - WIDTH,
            screen.max.y - 24.0 - lift - HEIGHT - from_bottom * (HEIGHT + 8.0),
        );
        Area::new(Id::unique(("toast", index)))
            .order(Order::Tooltip)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let (rect, response) =
                    ui.allocate_exact_size(Vec2::new(WIDTH, HEIGHT), Sense::click());
                draw::shadow(
                    ui.painter(),
                    rect,
                    10.0,
                    8,
                    24,
                    egui::Color32::from_black_alpha(0x60),
                );
                draw::rounded(
                    ui.painter(),
                    rect,
                    CornerRadius::same(10),
                    tokens.surface_3,
                    Some((1.0, tokens.border_strong)),
                );
                fonts::icon(
                    ui.painter(),
                    Pos2::new(rect.min.x + 14.0 + 9.0, rect.center().y),
                    icons::CIRCLE_CHECK,
                    18.0,
                    tokens.primary,
                );
                let mut right = rect.max.x - 14.0;
                if let Some(action) = &toast.action {
                    let label = match action {
                        ToastAction::UndoImport { .. } => tl!("Undo"),
                    };
                    let used = fonts::draw_right(
                        ui.painter(),
                        right,
                        rect.center().y,
                        label,
                        Text::body(13.0, 600, tokens.primary),
                    );
                    let hit = Rect::from_min_max(Pos2::new(used.min.x - 8.0, rect.min.y), rect.max);
                    let response =
                        ui.interact(hit, ui.scope_id().with("toast-action"), Sense::click());
                    response
                        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
                    if response.clicked() {
                        run = Some((index, action.clone()));
                    }
                    right = used.min.x - 12.0;
                }
                let clip = Rect::from_min_max(
                    Pos2::new(rect.min.x + 14.0 + 18.0 + 12.0, rect.min.y),
                    Pos2::new(right, rect.max.y),
                );
                let painter = ui.painter().with_clip_rect(clip);
                fonts::draw_left(
                    &painter,
                    clip.min.x,
                    rect.center().y,
                    &toast.text,
                    Text::body(13.0, 400, tokens.text),
                );
                if response.clicked() {
                    dismissed = Some(index);
                }
            });
    }
    if let Some((index, action)) = run {
        app.toasts.remove(index);
        app.run_toast_action(action);
    } else if let Some(index) = dismissed {
        app.toasts.remove(index);
    }
}
