use egui::{Color32, CornerRadius, Pos2, Rect, Response, Role, Sense, TextureId, Ui, Vec2};

use super::{alloc, chips::*, describe, interact};
use crate::{draw, fonts, fonts::Text, icons, layout, theme::Tokens};

/// A summary number: spaced caption, large value, one line of detail.
pub fn stat_card(ui: &Ui, rect: Rect, label: &str, value: &str, sub: &str) {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    draw::rounded(
        painter,
        rect,
        CornerRadius::same(14),
        tokens.surface_2,
        Some((1.0, tokens.border)),
    );
    let x = rect.min.x + 18.0;
    fonts::draw(
        painter,
        Pos2::new(x, rect.min.y + 18.0),
        &label.to_uppercase(),
        Text::display(11.0, 700, tokens.text_3).spaced(1.5),
    );
    fonts::draw(
        painter,
        Pos2::new(x, rect.min.y + 38.0),
        value,
        Text::display(30.0, 700, tokens.text),
    );
    fonts::draw(
        painter,
        Pos2::new(x, rect.min.y + 83.0),
        sub,
        Text::body(12.0, 400, tokens.text_2),
    );
}

/// A drop target that is also clickable (opens the file picker). Highlights while files hover
/// over the window.
pub fn dropzone(
    ui: &mut Ui,
    rect: Rect,
    title: &str,
    hint: &str,
    files_hovering: bool,
) -> Response {
    let tokens = Tokens::get(ui.ctx());
    let response = interact(ui, rect, ("dropzone", title), Sense::click());
    describe(&response, Role::Button, true, None, title);
    let lit = files_hovering || response.hovered();
    let (fill, border) = if lit {
        (tokens.primary_soft, tokens.primary)
    } else {
        (tokens.surface_2, tokens.border_strong)
    };
    let painter = ui.painter();
    draw::rounded(
        painter,
        rect,
        CornerRadius::same(14),
        fill,
        Some((1.5, border)),
    );
    let stack = 28.0 + 10.0 + 20.0 + 10.0 + 15.0;
    let top = rect.center().y - stack / 2.0;
    let cx = rect.center().x;
    fonts::icon(
        painter,
        Pos2::new(cx, top + 14.0),
        icons::UPLOAD,
        28.0,
        tokens.primary,
    );
    fonts::draw_centered(
        painter,
        Rect::from_center_size(
            Pos2::new(cx, top + 38.0 + 10.0),
            Vec2::new(rect.width(), 20.0),
        ),
        title,
        Text::display(15.0, 600, tokens.text),
    );
    fonts::draw_centered(
        painter,
        Rect::from_center_size(
            Pos2::new(cx, top + 68.0 + 7.5),
            Vec2::new(rect.width(), 15.0),
        ),
        hint,
        Text::body(12.0, 400, tokens.text_3),
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// What the top part of a card shows.
#[derive(Clone, Copy)]
pub enum Cover {
    /// A flat stage with an icon.
    Icon(char),
    /// A flat stage with a texture preview.
    /// A texture and its size in pixels; it is fitted into the stage with its proportions kept.
    Image(TextureId, Vec2),
}

fn paint_cover(
    ui: &Ui,
    rect: Rect,
    cover: Cover,
    icon_size: f32,
    icon_color: Color32,
    stage: Color32,
) {
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius {
            nw: 14,
            ne: 14,
            sw: 0,
            se: 0,
        },
        stage,
    );
    match cover {
        Cover::Icon(glyph) => fonts::icon(painter, rect.center(), glyph, icon_size, icon_color),
        Cover::Image(texture, size) => {
            // Fit, never stretch: scale to the largest size that fits with a little air around it.
            let area = rect.shrink2(Vec2::new(8.0, 4.0));
            let scale = (area.width() / size.x.max(1.0)).min(area.height() / size.y.max(1.0));
            let image = Rect::from_center_size(rect.center(), size * scale);
            painter.image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
}

/// Outcome of interacting with an [`item_card`].
pub struct ItemCardResponse {
    pub card: Response,
}

/// What a garage card shows.
#[derive(Clone, Copy)]
pub struct ItemCard<'a> {
    pub name: &'a str,
    pub kind: &'a str,
    pub cover: Cover,
    pub selected: bool,
}

/// A garage card: stage, item name and kind.
pub fn item_card(ui: &mut Ui, rect: Rect, card: &ItemCard<'_>) -> ItemCardResponse {
    let ItemCard {
        name,
        kind,
        cover,
        selected,
    } = *card;
    let tokens = Tokens::get(ui.ctx());
    let card = interact(ui, rect, ("item-card", name, kind), Sense::click());
    describe(
        &card,
        Role::Button,
        true,
        Some(selected),
        &format!("{name} ({kind})"),
    );
    let border = if selected {
        (2.0, tokens.primary)
    } else {
        (1.0, tokens.border)
    };
    let hovered = card.hovered() && !selected;
    let fill = if hovered {
        tokens.surface_3
    } else {
        tokens.surface_2
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(14),
        fill,
        Some(border),
    );
    let stage = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 84.0));
    paint_cover(ui, stage, cover, 44.0, tokens.text_3, tokens.stage);
    // The cover covers the top of the border: draw the border again on top of it.
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(14),
        egui::Stroke::new(border.0, border.1),
        egui::StrokeKind::Inside,
    );
    let x = rect.min.x + 14.0;
    let y = rect.min.y + 84.0 + 14.0;
    let name_style = Text::display(17.0, 700, tokens.text);
    let name_text = fonts::ellipsize(ui.painter(), name, name_style, rect.width() - 28.0);
    fonts::draw(ui.painter(), Pos2::new(x, y), &name_text, name_style);
    fonts::draw(
        ui.painter(),
        Pos2::new(x, y + 24.0),
        kind,
        Text::body(12.0, 400, tokens.text_3),
    );
    ItemCardResponse {
        card: card.on_hover_cursor(egui::CursorIcon::PointingHand),
    }
}

/// Paint one M0..M3 chip at `rect`.
pub fn grade_chip_paint(
    ui: &Ui,
    rect: Rect,
    label: &str,
    active: bool,
    present: bool,
    hovered: bool,
) {
    let tokens = Tokens::get(ui.ctx());
    let (fill, ink) = if active {
        (tokens.primary, tokens.on_primary)
    } else if hovered {
        (tokens.border, tokens.text)
    } else {
        (tokens.surface_3, tokens.text_2)
    };
    let (fill, ink) = if present || active {
        (fill, ink)
    } else {
        (draw::fade(fill, 0.55), draw::fade(ink, 0.55))
    };
    draw::rounded(ui.painter(), rect, CornerRadius::same(6), fill, None);
    fonts::draw_centered(ui.painter(), rect, label, Text::mono(12.0, 700, ink));
}

/// Outcome of a [`pack_card`].
pub struct PackCardResponse {
    pub card: Response,
    pub select: bool,
    pub delete: bool,
}

/// What a pack card shows.
#[derive(Clone, Copy)]
pub struct PackCard<'a> {
    pub name: &'a str,
    pub meta: &'a str,
    pub active: bool,
    pub status: (ChipKind, &'a str),
    /// Accessible and visible names of the two actions.
    pub select_label: &'a str,
    pub delete_label: &'a str,
}

/// A pack in the library grid: gradient stage, name, version line, status chip, actions.
pub fn pack_card(ui: &mut Ui, rect: Rect, card: &PackCard<'_>) -> PackCardResponse {
    let PackCard {
        name,
        meta,
        active,
        status,
        select_label,
        delete_label,
    } = *card;
    let tokens = Tokens::get(ui.ctx());
    let card = interact(ui, rect, ("pack-card", name, meta), Sense::hover());
    let border = if active {
        tokens.primary
    } else {
        tokens.border
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(14),
        tokens.surface_2,
        Some((1.0, border)),
    );
    let stage = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 96.0));
    draw::radial(
        ui.painter(),
        stage,
        0.0,
        tokens.surface_3,
        tokens.stage,
        tokens.surface_2,
    );
    // Round the top corners with the colour behind the card (the panel), then redraw the border.
    draw::cut_corners(ui.painter(), stage, 14.0, tokens.surface, true);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(14),
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    fonts::icon(
        ui.painter(),
        stage.center(),
        icons::PACKAGE,
        36.0,
        tokens.primary,
    );
    let x = rect.min.x + 16.0;
    let y = rect.min.y + 96.0 + 16.0;
    // Names and author lines of real packs are long: shorten them to the card.
    let room = rect.width() - 32.0;
    let name_style = Text::display(18.0, 700, tokens.text);
    let meta_style = Text::body(12.0, 400, tokens.text_3);
    let name_text = fonts::ellipsize(ui.painter(), name, name_style, room);
    let meta_text = fonts::ellipsize(ui.painter(), meta, meta_style, room);
    fonts::draw(ui.painter(), Pos2::new(x, y), &name_text, name_style);
    fonts::draw(ui.painter(), Pos2::new(x, y + 25.0), &meta_text, meta_style);
    let chip = Rect::from_min_size(
        Pos2::new(x, y + 52.0),
        Vec2::new(chip_width(ui, status.1), 24.0),
    );
    paint_chip(ui, chip, status.0, status.1);
    let actions = Rect::from_min_size(Pos2::new(x, y + 88.0), Vec2::new(rect.width() - 32.0, 40.0));
    let mut out = PackCardResponse {
        card,
        select: false,
        delete: false,
    };
    let parts = if active {
        layout::columns(actions, 8.0, &[layout::Size::Fixed(38.0)])
    } else {
        layout::columns(
            actions,
            8.0,
            &[layout::Size::Fill(1.0), layout::Size::Fixed(38.0)],
        )
    };
    let delete_rect = *parts.last().expect("delete part");
    if !active {
        let select = parts[0];
        let response = layout::in_rect(ui, select, |ui| {
            super::Button::new(super::ButtonKind::Secondary, select_label)
                .icon(icons::CHECK)
                .width(super::Width::Fill)
                .show(ui)
        });
        out.select = response.clicked();
    }
    let delete = layout::in_rect(
        ui,
        Rect::from_min_size(delete_rect.min, Vec2::splat(38.0)),
        |ui| super::icon_button(ui, icons::TRASH_2, delete_label),
    );
    out.delete = delete.clicked();
    out
}

/// A skin variant of the selected item. With `apply` the row is the chosen one and shows the
/// Apply button; returns whether the row was clicked and whether Apply was.
pub fn skin_row(
    ui: &mut Ui,
    width: f32,
    name: &str,
    meta: &str,
    selected: bool,
    apply_label: Option<&str>,
    badge: Option<&str>,
) -> (Response, bool) {
    let tokens = Tokens::get(ui.ctx());
    let (rect, response) = alloc(ui, Vec2::new(width, 80.0), Sense::click());
    describe(&response, Role::Button, true, Some(selected), name);
    let border = if selected {
        tokens.primary
    } else {
        tokens.border
    };
    let fill = if response.hovered() && !selected {
        tokens.surface_3
    } else {
        tokens.surface_2
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        fill,
        Some((1.0, border)),
    );
    let thumb = Rect::from_min_size(rect.min + Vec2::new(12.0, 12.0), Vec2::splat(56.0));
    draw::rounded(
        ui.painter(),
        thumb,
        CornerRadius::same(8),
        tokens.stage,
        None,
    );
    fonts::icon(
        ui.painter(),
        thumb.center(),
        icons::PAINTBRUSH,
        22.0,
        tokens.text_3,
    );
    let x = rect.min.x + 12.0 + 56.0 + 14.0;
    // The texts end where the Apply button starts (or at the card's padding without one).
    let apply = apply_label
        .map(|label| super::Button::new(super::ButtonKind::Primary, label).icon(icons::PLAY));
    let button_width = apply.as_ref().map_or(0.0, |button| button.measure(ui));
    // A badge (`Incomplete`) sits where a button would, in the danger colour.
    let badge_width = badge.map_or(0.0, |label| super::chip_width(ui, label));
    let text_right = if apply.is_some() {
        rect.max.x - 12.0 - button_width - 12.0
    } else if badge.is_some() {
        rect.max.x - 12.0 - badge_width - 12.0
    } else {
        rect.max.x - 12.0
    };
    if let Some(label) = badge {
        let chip = Rect::from_min_size(
            Pos2::new(rect.max.x - 12.0 - badge_width, rect.center().y - 12.0),
            Vec2::new(badge_width, 24.0),
        );
        super::paint_chip(ui, chip, super::ChipKind::Danger, label);
        // Readers of the screen (and tests) learn the state from the badge as well.
        let badge = ui.interact(chip, response.id.with("badge"), Sense::hover());
        describe(&badge, Role::Label, true, None, label);
    }
    let room = text_right - x;
    let name_style = Text::body(14.0, 600, tokens.text);
    let meta_style = Text::body(12.0, 400, tokens.text_3);
    let name_text = fonts::ellipsize(ui.painter(), name, name_style, room);
    let meta_text = fonts::ellipsize(ui.painter(), meta, meta_style, room);
    fonts::draw(
        ui.painter(),
        Pos2::new(x, rect.min.y + 23.0),
        &name_text,
        name_style,
    );
    fonts::draw(
        ui.painter(),
        Pos2::new(x, rect.min.y + 42.0),
        &meta_text,
        meta_style,
    );
    let mut applied = false;
    if let Some(apply) = apply {
        let button = Rect::from_min_size(
            Pos2::new(rect.max.x - 12.0 - button_width, rect.min.y + 20.0),
            Vec2::new(button_width, 40.0),
        );
        let clicked = layout::in_rect(ui, button, |ui| apply.show(ui).clicked());
        applied = clicked;
    }
    (
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        applied,
    )
}
