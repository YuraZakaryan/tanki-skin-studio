// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A group of setting rows, the way Windows 11 and GNOME lay out settings: one rounded card, a
//! row per setting with its icon, title and one line of description on the left and its control
//! on the right, a hairline between rows. On a narrow page the control goes under the text.

use egui::{Color32, CornerRadius, Pos2, Rect, Sense, TextureId, Ui, Vec2};

use super::describe;
use crate::{draw, fonts, fonts::Text, theme::Tokens};

/// The colour of a row's icon box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    Plain,
    Info,
    Success,
    Danger,
}

/// A brand's own mark, drawn as it is (no tinted box) in place of a row's icon: an image
/// `size` points large, centred in the icon's square.
#[derive(Clone, Copy, Debug)]
pub struct Mark {
    pub texture: TextureId,
    pub size: Vec2,
}

/// One row of a [`group`].
pub struct GroupRow<'a> {
    pub icon: char,
    /// Drawn instead of `icon` when set.
    pub mark: Option<Mark>,
    pub tint: Tint,
    pub title: &'a str,
    pub desc: Option<&'a str>,
    /// The size of the control the caller draws in [`RowPlace::control`].
    pub control: Vec2,
    /// Room under the text for more (a usage bar), the text's width; 0 for none.
    pub extra: f32,
    /// The whole row is a button (it opens a page).
    pub click: bool,
}

/// Where a row of a [`group`] went.
pub struct RowPlace {
    pub rect: Rect,
    pub control: Rect,
    pub extra: Rect,
    pub clicked: bool,
}

const PAD_X: f32 = 18.0;
const PAD_Y: f32 = 14.0;
const ICON: f32 = 36.0;
const TEXT_X: f32 = PAD_X + ICON + 14.0;
/// Narrower than this, a control goes under its row's text.
pub const STACK_BELOW: f32 = 600.0;

/// Whether a row's control goes under its text: on a narrow page, unless it is a small mark
/// (an icon) that always fits beside it.
fn stacked(width: f32, row: &GroupRow<'_>) -> bool {
    width < STACK_BELOW && row.control.x > 40.0
}

fn text_width(width: f32, row: &GroupRow<'_>) -> f32 {
    let control = if stacked(width, row) || row.control.x <= 0.0 {
        0.0
    } else {
        row.control.x + 14.0
    };
    (width - TEXT_X - PAD_X - control).max(80.0)
}

fn desc_height(ui: &Ui, width: f32, row: &GroupRow<'_>) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    row.desc.map_or(0.0, |desc| {
        2.0 + fonts::paragraph(
            ui.painter(),
            desc,
            Text::body(12.0, 400, tokens.text_3),
            text_width(width, row),
            None,
        )
        .size()
        .y
    })
}

fn heights(ui: &Ui, width: f32, row: &GroupRow<'_>) -> (f32, f32) {
    let head = (19.0 + desc_height(ui, width, row)).max(ICON);
    let mut total = PAD_Y + head;
    if stacked(width, row) && row.control.y > 0.0 {
        total += 10.0 + row.control.y;
    }
    if row.extra > 0.0 {
        total += 12.0 + row.extra;
    }
    (head, total + PAD_Y)
}

/// The height of a group of `rows` `width` wide.
pub fn group_height(ui: &Ui, width: f32, rows: &[GroupRow<'_>]) -> f32 {
    rows.iter().map(|row| heights(ui, width, row).1).sum()
}

/// A section heading above a group: 14 px, semibold. Returns the y under it.
pub fn group_heading(ui: &Ui, origin: Pos2, text: &str) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    fonts::draw(
        ui.painter(),
        origin,
        text,
        Text::body(14.0, 600, tokens.text),
    );
    origin.y + 19.0 + 10.0
}

fn tint(tokens: &Tokens, tint: Tint) -> (Color32, Color32) {
    match tint {
        Tint::Plain => (tokens.surface_3, tokens.text_2),
        Tint::Info => (tokens.info_soft, tokens.info),
        Tint::Success => (tokens.primary_soft, tokens.primary),
        Tint::Danger => (tokens.danger_soft, tokens.danger),
    }
}

/// Draw a group of `rows` from `origin`, `width` wide: the card, each row's icon and text and the
/// hairlines. The caller draws each control in its [`RowPlace::control`].
pub fn group(
    ui: &mut Ui,
    origin: Pos2,
    width: f32,
    rows: &[GroupRow<'_>],
    salt: &str,
) -> Vec<RowPlace> {
    let tokens = Tokens::get(ui.ctx());
    let total = group_height(ui, width, rows);
    let card = Rect::from_min_size(origin, Vec2::new(width, total));
    draw::rounded(
        ui.painter(),
        card,
        CornerRadius::same(12),
        tokens.surface,
        None,
    );
    let mut places = Vec::new();
    let mut y = origin.y;
    for (index, row) in rows.iter().enumerate() {
        let (head, height) = heights(ui, width, row);
        let rect = Rect::from_min_size(Pos2::new(origin.x, y), Vec2::new(width, height));
        let mut clicked = false;
        if row.click {
            let response = ui.interact(rect, ui.make_persistent_id((salt, index)), Sense::click());
            describe(&response, egui::Role::Button, true, None, row.title);
            if response.hovered() {
                let round = |on: bool| if on { 12 } else { 0 };
                let first = index == 0;
                let last = index + 1 == rows.len();
                ui.painter().rect_filled(
                    rect,
                    CornerRadius {
                        nw: round(first),
                        ne: round(first),
                        sw: round(last),
                        se: round(last),
                    },
                    tokens.surface_2,
                );
            }
            clicked = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
        }
        if index > 0 {
            draw::hrule(
                ui.painter(),
                rect.min.x,
                rect.max.x,
                rect.min.y,
                tokens.border,
            );
        }
        let (fill, ink) = tint(&tokens, row.tint);
        let icon_box = Rect::from_min_size(
            Pos2::new(rect.min.x + PAD_X, rect.min.y + PAD_Y + (head - ICON) / 2.0),
            Vec2::splat(ICON),
        );
        if let Some(mark) = row.mark {
            ui.painter().image(
                mark.texture,
                Rect::from_center_size(icon_box.center(), mark.size),
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            draw::rounded(ui.painter(), icon_box, CornerRadius::same(8), fill, None);
            fonts::icon(ui.painter(), icon_box.center(), row.icon, 16.0, ink);
        }
        let text_height = 19.0 + desc_height(ui, width, row);
        let text_top = rect.min.y + PAD_Y + (head - text_height) / 2.0;
        let x = rect.min.x + TEXT_X;
        fonts::draw(
            ui.painter(),
            Pos2::new(x, text_top),
            row.title,
            Text::body(14.0, 600, tokens.text),
        );
        if let Some(desc) = row.desc {
            let galley = fonts::paragraph(
                ui.painter(),
                desc,
                Text::body(12.0, 400, tokens.text_3),
                text_width(width, row),
                None,
            );
            ui.painter()
                .galley(Pos2::new(x, text_top + 21.0), galley, tokens.text_3);
        }
        let mut below = rect.min.y + PAD_Y + head;
        let control = if stacked(width, row) {
            let place = Rect::from_min_size(Pos2::new(x, below + 10.0), row.control);
            if row.control.y > 0.0 {
                below += 10.0 + row.control.y;
            }
            place
        } else {
            Rect::from_min_size(
                Pos2::new(
                    rect.max.x - PAD_X - row.control.x,
                    rect.min.y + PAD_Y + (head - row.control.y) / 2.0,
                ),
                row.control,
            )
        };
        let extra = Rect::from_min_size(
            Pos2::new(x, below + 12.0),
            Vec2::new(rect.max.x - PAD_X - x, row.extra),
        );
        places.push(RowPlace {
            rect,
            control,
            extra,
            clicked,
        });
        y += height;
    }
    ui.painter().rect_stroke(
        card,
        CornerRadius::same(12),
        egui::Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
    places
}
