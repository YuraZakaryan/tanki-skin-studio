// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The state without a game cache: explain it and offer the two ways out.

use eframe::egui::{Pos2, Rect, Ui, Vec2};
use tanki_i18n::tl;
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, Width},
};

use super::shell;

pub fn draw(app: &mut crate::app::Studio, ui: &mut Ui, main: Rect) {
    let tokens = Tokens::get(ui.ctx());
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Overview"),
        tl!("Game cache not connected"),
        |_| {},
    );
    let area = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let stack = 96.0 + 22.0 + 110.0 + 22.0 + 46.0 + 22.0 + 124.0;
    let top = area.center().y - stack / 2.0;
    let cx = area.center().x;
    let icon_box = Rect::from_center_size(Pos2::new(cx, top + 48.0), Vec2::splat(96.0));
    draw::rounded(
        ui.painter(),
        icon_box,
        24.0,
        tokens.surface_2,
        Some((1.0, tokens.border)),
    );
    fonts::icon(
        ui.painter(),
        icon_box.center(),
        icons::FOLDER_SEARCH,
        44.0,
        tokens.primary,
    );
    let text_top = top + 96.0 + 22.0;
    fonts::draw_centered(
        ui.painter(),
        Rect::from_min_size(Pos2::new(cx - 230.0, text_top), Vec2::new(460.0, 36.0)),
        tl!("Game cache not found"),
        Text::display(28.0, 700, tokens.text),
    );
    let hint = fonts::paragraph_centered(
        ui.painter(),
        tl!(
            "Choose the game cache folder manually, or launch the game once and search again. Your real cache is not modified until you apply a pack."
        ),
        Text::body(14.0, 400, tokens.text_2),
        460.0,
        Some(22.0),
    );
    ui.painter()
        .galley(Pos2::new(cx, text_top + 44.0), hint, tokens.text_2);
    let actions_top = text_top + 110.0 + 22.0;
    let pick = Button::new(ButtonKind::Primary, tl!("Choose cache folder"))
        .icon(icons::FOLDER_OPEN)
        .height(46.0);
    let retry = Button::new(ButtonKind::Secondary, tl!("Search again"))
        .icon(icons::REFRESH_CW)
        .height(46.0);
    let (w1, w2) = (pick.measure(ui), retry.measure(ui));
    let row_left = cx - (w1 + 12.0 + w2) / 2.0;
    in_rect(
        ui,
        Rect::from_min_size(Pos2::new(row_left, actions_top), Vec2::new(w1, 46.0)),
        |ui| {
            if pick.width(Width::Exact(w1)).show(ui).clicked() {
                app.choose_cache();
            }
        },
    );
    in_rect(
        ui,
        Rect::from_min_size(
            Pos2::new(row_left + w1 + 12.0, actions_top),
            Vec2::new(w2, 46.0),
        ),
        |ui| {
            if retry.width(Width::Exact(w2)).show(ui).clicked() {
                app.rescan();
            }
        },
    );
    let checked = Rect::from_min_size(
        Pos2::new(cx - 230.0, actions_top + 46.0 + 22.0),
        Vec2::new(460.0, 124.0),
    );
    widgets::info_box(ui, checked);
    widgets::caption(
        ui,
        checked.min + Vec2::new(16.0, 16.0),
        tl!("Checked automatically"),
    );
    let on_mac = cfg!(target_os = "macos");
    let rows = [
        (
            "macOS",
            if on_mac {
                tl!("not found")
            } else {
                tl!("not applicable")
            },
        ),
        (
            "Windows",
            if on_mac {
                tl!("not applicable")
            } else {
                tl!("not found")
            },
        ),
        (tl!("Custom folder"), tl!("not selected")),
    ];
    for (index, (key, value)) in rows.iter().enumerate() {
        let cy = checked.min.y + 16.0 + 14.0 + 10.0 + index as f32 * 26.0 + 8.0;
        fonts::draw_left(
            ui.painter(),
            checked.min.x + 16.0,
            cy,
            key,
            Text::body(13.0, 400, tokens.text),
        );
        fonts::draw_right(
            ui.painter(),
            checked.max.x - 16.0,
            cy,
            value,
            Text::body(13.0, 400, tokens.text_3),
        );
    }
}
