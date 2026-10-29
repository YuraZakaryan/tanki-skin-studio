use egui::{CornerRadius, Pos2, Rect, Ui, Vec2};

use crate::{draw, fonts, fonts::Text, theme::Tokens};

/// Height of a panel header, rule included.
pub const HEADER_HEIGHT: f32 = 45.0;

/// A titled panel: rounded surface with a lime marker, spaced upper-case title, an optional meta
/// text on the right and a rule under the header. Returns the body rectangle below the header.
pub fn panel(ui: &Ui, rect: Rect, title: &str, meta: Option<&str>) -> Rect {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    draw::rounded(
        painter,
        rect,
        CornerRadius::same(16),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let center_y = rect.min.y + 22.5;
    painter.rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.min.x + 18.0, rect.min.y + 16.0),
            Vec2::new(4.0, 14.0),
        ),
        CornerRadius::same(2),
        tokens.primary,
    );
    fonts::draw_left(
        painter,
        rect.min.x + 32.0,
        center_y,
        &title.to_uppercase(),
        Text::display(13.0, 700, tokens.text_2).spaced(1.5),
    );
    if let Some(meta) = meta {
        let title_style = Text::display(13.0, 700, tokens.text_2).spaced(1.5);
        let title_width = fonts::width(painter, &title.to_uppercase(), title_style);
        let style = Text::body(12.0, 400, tokens.text_3);
        let room = rect.width() - 32.0 - title_width - 18.0 - 16.0;
        let meta = fonts::ellipsize(painter, meta, style, room);
        fonts::draw_right(painter, rect.max.x - 18.0, center_y, &meta, style);
    }
    draw::hrule(
        painter,
        rect.min.x + 1.0,
        rect.max.x - 1.0,
        rect.min.y + HEADER_HEIGHT - 1.0,
        tokens.border,
    );
    Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y + HEADER_HEIGHT), rect.max)
}

/// Small spaced upper-case caption (stat labels, group titles in lists).
pub fn caption(ui: &Ui, pos: Pos2, text: &str) -> Rect {
    let tokens = Tokens::get(ui.ctx());
    fonts::draw(
        ui.painter(),
        pos,
        &text.to_uppercase(),
        Text::display(11.0, 700, tokens.text_3).spaced(1.5),
    )
}
