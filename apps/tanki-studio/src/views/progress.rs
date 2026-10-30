// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The card of a running import in the bottom-right corner, above the toasts: what is being
//! imported, the stage, a determinate bar with the percent and the time left once it can be told,
//! and Cancel. Non-modal: the rest of the app stays usable while a large pack is checked and copied.

use std::time::Duration;

use eframe::egui::{self, Area, Context, CornerRadius, Id, Order, Pos2, Rect, Sense, Vec2};
use tanki_core::pack::ImportStage;
use tanki_i18n::{fmt, tl};
use tanki_ui::{draw, fonts, fonts::Text, icons, theme::Tokens};

use crate::app::{ImportJob, Studio};

pub const WIDTH: f32 = 440.0;
pub const HEIGHT: f32 = 92.0;

pub fn draw(app: &mut Studio, ctx: &Context) {
    let Some(job) = &app.import_job else {
        return;
    };
    let tokens = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let pos = Pos2::new(screen.max.x - 24.0 - WIDTH, screen.max.y - 24.0 - HEIGHT);
    let fraction = job.progress.fraction();
    let title = fmt(tl!("Importing “{name}”"), &[("name", &job.name)]);
    let mut cancel = false;
    Area::new(Id::unique("import-progress"))
        .order(Order::Tooltip)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(Vec2::new(WIDTH, HEIGHT), Sense::hover());
            response.widget_info(|| {
                let mut info =
                    egui::WidgetInfo::labeled(egui::Role::ProgressIndicator, true, &title);
                info.value = fraction.map(f64::from);
                info
            });
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
            let left = rect.min.x + 14.0;
            let right = rect.max.x - 14.0;
            let top = rect.min.y + 22.0;
            fonts::icon(
                ui.painter(),
                Pos2::new(left + 9.0, top),
                icons::PACKAGE,
                18.0,
                tokens.primary,
            );
            let label = tl!("Cancel");
            let used = fonts::draw_right(
                ui.painter(),
                right,
                top,
                label,
                Text::body(13.0, 600, tokens.primary),
            );
            let hit = Rect::from_min_max(
                Pos2::new(used.min.x - 8.0, rect.min.y),
                Pos2::new(rect.max.x, rect.min.y + 44.0),
            );
            let button = ui.interact(hit, ui.scope_id().with("cancel-import"), Sense::click());
            button.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, tl!("Cancel import"))
            });
            if button.clicked() {
                cancel = true;
            }
            let style = Text::body(13.0, 600, tokens.text);
            let text_x = left + 18.0 + 12.0;
            let shown = fonts::ellipsize(ui.painter(), &title, style, used.min.x - 12.0 - text_x);
            fonts::draw_left(ui.painter(), text_x, top, &shown, style);

            let row = rect.min.y + 46.0;
            fonts::draw_left(
                ui.painter(),
                left,
                row,
                stage_text(job),
                Text::body(12.0, 400, tokens.text_2),
            );
            if let Some(fraction) = fraction {
                fonts::draw_right(
                    ui.painter(),
                    right,
                    row,
                    &format!("{}%", (fraction * 100.0).floor() as u32),
                    Text::mono(12.0, 400, tokens.text),
                );
            }
            let bar = Rect::from_min_max(
                Pos2::new(left, rect.min.y + 58.0),
                Pos2::new(right, rect.min.y + 64.0),
            );
            // The track in the border colour: the card is the colour of the usual track.
            draw::rounded(
                ui.painter(),
                bar,
                CornerRadius::same(3),
                tokens.border,
                None,
            );
            let filled = match fraction {
                Some(fraction) => Rect::from_min_size(
                    bar.min,
                    Vec2::new(bar.width() * fraction.clamp(0.0, 1.0), bar.height()),
                ),
                None => {
                    // The amount of work is not known yet: a segment sweeps the track.
                    let time = ui.input(|i| i.time) as f32;
                    let width = bar.width() * 0.3;
                    let x = bar.min.x + (time * 0.8).fract() * (bar.width() + width) - width;
                    ctx.request_repaint_after(Duration::from_millis(16));
                    Rect::from_min_max(
                        Pos2::new(x.max(bar.min.x), bar.min.y),
                        Pos2::new((x + width).min(bar.max.x), bar.max.y),
                    )
                }
            };
            if filled.width() > 0.0 {
                draw::rounded(
                    ui.painter(),
                    filled,
                    CornerRadius::same(3),
                    tokens.primary,
                    None,
                );
            }
            fonts::draw_left(
                ui.painter(),
                left,
                rect.min.y + 78.0,
                &time_left(job, fraction),
                Text::body(12.0, 400, tokens.text_3),
            );
        });
    if cancel {
        app.cancel_import();
    }
}

/// What the import is doing now.
fn stage_text(job: &ImportJob) -> &'static str {
    if job.progress.is_cancelled() {
        return tl!("Cancelling…");
    }
    match job.progress.stage() {
        ImportStage::Reading if job.archive => tl!("Building a pack from the archive…"),
        ImportStage::Reading => tl!("Reading the pack…"),
        ImportStage::Checking => tl!("Checking files…"),
        ImportStage::Copying => tl!("Copying into the library…"),
        ImportStage::Verifying => tl!("Checking the copy…"),
    }
}

/// The time left, once enough of the import is behind to tell it (5% and two seconds), so the
/// first wild guesses are not shown. A long import is told in minutes.
fn time_left(job: &ImportJob, fraction: Option<f32>) -> String {
    let elapsed = job.started.elapsed().as_secs_f32();
    let Some(fraction) = fraction.filter(|f| *f >= 0.05 && elapsed >= 2.0) else {
        return tl!("Estimating the time left…").to_owned();
    };
    let left = (elapsed / fraction * (1.0 - fraction)).ceil() as u64;
    if left < 60 {
        fmt(tl!("About {n} s left"), &[("n", &left.max(1).to_string())])
    } else {
        fmt(
            tl!("About {n} min left"),
            &[("n", &left.div_ceil(60).to_string())],
        )
    }
}
