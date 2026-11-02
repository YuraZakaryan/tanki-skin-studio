// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Screens. Each file draws one page or overlay from the design file into rectangles cut with
//! `tanki_ui::layout`, using the widgets of `tanki_ui`.

pub mod bulk;
pub mod dialogs;
pub mod empty;
pub mod garage;
pub mod info;
pub mod overview;
pub mod packs;
pub mod palette;
pub mod progress;
pub mod resources;
pub mod shell;
pub mod splash;
pub mod toast;
pub mod update;

use eframe::egui::{self, Rect, ScrollArea, Sense, Ui, UiBuilder, Vec2};
use tanki_ui::theme::Tokens;

use crate::app::Studio;

/// Draw the whole window: splash, or sidebar + page, then the overlays.
pub fn draw(app: &mut Studio, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let tokens = Tokens::get(&ctx);
    let full = ui.max_rect();
    ui.painter().rect_filled(full, 0.0, tokens.bg);
    if !app.splash_done {
        splash::draw(app, ui, full);
        return;
    }
    let side = if full.width() < shell::COMPACT_BELOW {
        shell::RAIL_WIDTH
    } else {
        shell::SIDEBAR_WIDTH
    };
    let sidebar = Rect::from_min_size(full.min, Vec2::new(side, full.height()));
    let main = Rect::from_min_max(egui::pos2(sidebar.max.x, full.min.y), full.max);
    shell::sidebar(app, ui, sidebar);
    shell::page(app, ui, main);
    progress::draw(app, &ctx);
    toast::draw(app, &ctx);
    bulk::draw(app, &ctx);
    dialogs::draw(app, &ctx);
    update::draw(app, &ctx);
    palette::draw(app, &ctx);
}

/// A page body that scrolls when the window is shorter than `min_height`. `content` receives the
/// full content rectangle (at least `min_height` high) to lay out.
pub fn scroll_page(ui: &mut Ui, rect: Rect, min_height: f32, content: impl FnOnce(&mut Ui, Rect)) {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let size = Vec2::new(rect.width(), rect.height().max(min_height));
                let (content_rect, _) = ui.allocate_exact_size(size, Sense::hover());
                content(ui, content_rect);
            });
    });
}

/// Whether a page's content area is wide enough for two columns; otherwise columns are stacked.
pub fn is_wide(content: Rect) -> bool {
    content.width() >= 860.0
}
