// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The sidebar and the frame every page shares: top bar, content padding.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Ui, Vec2};
use tanki_i18n::{Lang, fmt, tl};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::{ThemePreference, Tokens},
    widgets::{self, ChipKind},
};

use crate::{
    app::{Page, Studio},
    model::thousands,
};

pub const SIDEBAR_WIDTH: f32 = 240.0;
/// The sidebar shrinks to an icon rail when the window is narrower than this.
pub const COMPACT_BELOW: f32 = 1100.0;
pub const RAIL_WIDTH: f32 = 72.0;
pub const TOP_BAR_HEIGHT: f32 = 72.0;

/// The client name shown for a game id.
pub fn game_name(game: &str) -> String {
    match game {
        "air-cache" => tl!("Game cache").to_owned(),
        "custom" => tl!("Custom folder").to_owned(),
        "unknown" => tl!("Unknown client").to_owned(),
        other => crate::model::target_title(other),
    }
}

fn os_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "Linux"
    }
}

pub fn sidebar(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    if rect.width() < SIDEBAR_WIDTH {
        return rail(app, ui, rect);
    }
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(ui.painter(), rect, 0.0, tokens.surface, None);
    draw::vrule(
        ui.painter(),
        rect.max.x - 1.0,
        rect.min.y,
        rect.max.y,
        tokens.border,
    );
    fonts::draw(
        ui.painter(),
        Pos2::new(rect.min.x + 24.0, rect.min.y + 24.0),
        "TANKI",
        Text::display(17.0, 700, tokens.text).spaced(3.0),
    );
    fonts::draw(
        ui.painter(),
        Pos2::new(rect.min.x + 24.0, rect.min.y + 46.0),
        tl!("Skin Studio"),
        Text::body(12.0, 400, tokens.text_3),
    );

    let counts = [
        None,
        (!app.items.is_empty()).then(|| thousands(app.items.len())),
        app.scan.as_ref().map(|s| thousands(s.resources.len())),
        Some(thousands(app.packs.len())),
        None,
    ];
    let nav = Rect::from_min_size(
        Pos2::new(rect.min.x + 12.0, rect.min.y + 85.0 + 8.0),
        Vec2::new(rect.width() - 24.0 - 1.0, 5.0 * 42.0 + 4.0 * 4.0),
    );
    in_rect(ui, nav, |ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        for (page, count) in Page::ALL.into_iter().zip(counts) {
            let (icon, label) = page_entry(page);
            if widgets::nav_item(ui, icon, label, count.as_deref(), app.page == page).clicked() {
                app.go(page);
            }
        }
    });

    // Footer, anchored to the bottom: cache card, language switch, theme switch.
    let width = rect.width() - 32.0 - 1.0;
    let x = rect.min.x + 16.0;
    let theme_y = rect.max.y - 16.0 - 36.0;
    let lang_y = theme_y - 12.0 - 36.0;
    let card_y = lang_y - 12.0 - 99.0;
    cache_card(
        app,
        ui,
        Rect::from_min_size(Pos2::new(x, card_y), Vec2::new(width, 99.0)),
    );
    in_rect(
        ui,
        Rect::from_min_size(Pos2::new(x, lang_y), Vec2::new(width, 36.0)),
        |ui| {
            let active = usize::from(app.lang != Lang::EN);
            if let Some(index) = widgets::segmented(
                ui,
                &[(icons::GLOBE, "English"), (icons::GLOBE, "Русский")],
                active,
                false,
            ) && let Some(lang) = Lang::all().nth(index)
            {
                app.set_language(lang);
            }
        },
    );
    in_rect(
        ui,
        Rect::from_min_size(Pos2::new(x, theme_y), Vec2::new(width, 36.0)),
        |ui| {
            // System, Light, Dark: icons only, so three fit in the sidebar's width.
            let options = [
                (icons::MONITOR, tl!("System")),
                (icons::SUN, tl!("Light")),
                (icons::MOON, tl!("Dark")),
            ];
            let active = ThemePreference::ALL
                .iter()
                .position(|p| *p == app.theme_pref)
                .unwrap_or(0);
            if let Some(index) = widgets::segmented(ui, &options, active, true) {
                app.set_theme_preference(ThemePreference::ALL[index]);
            }
        },
    );
}

/// The icon rail of narrow windows: the same navigation and switches, icons only.
fn rail(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    use eframe::egui::Sense;
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(ui.painter(), rect, 0.0, tokens.surface, None);
    draw::vrule(
        ui.painter(),
        rect.max.x - 1.0,
        rect.min.y,
        rect.max.y,
        tokens.border,
    );
    let cx = rect.center().x - 0.5;
    fonts::draw_centered(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(cx, rect.min.y + 36.0),
            Vec2::new(rect.width(), 28.0),
        ),
        "T",
        Text::display(22.0, 700, tokens.text),
    );
    let button = |ui: &mut Ui, rect: Rect, salt: &str, glyph: char, active: bool, tip: &str| {
        let response = ui.interact(rect, ui.scope_id().with(("rail", salt)), Sense::click());
        if active {
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(10),
                tokens.primary_soft,
                None,
            );
        } else if response.hovered() {
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(10),
                tokens.surface_3,
                None,
            );
        }
        let ink = if active {
            tokens.primary
        } else {
            tokens.text_2
        };
        fonts::icon(ui.painter(), rect.center(), glyph, 18.0, ink);
        response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tip)
    };
    for (index, page) in Page::ALL.into_iter().enumerate() {
        let (icon, label) = page_entry(page);
        let slot = Rect::from_center_size(
            Pos2::new(cx, rect.min.y + 85.0 + 8.0 + 21.0 + index as f32 * 46.0),
            Vec2::new(48.0, 42.0),
        );
        if button(ui, slot, label, icon, app.page == page, label).clicked() {
            app.go(page);
        }
    }
    let theme_rect = Rect::from_center_size(
        Pos2::new(cx, rect.max.y - 16.0 - 18.0),
        Vec2::new(48.0, 36.0),
    );
    // One button that steps System -> Light -> Dark.
    let (glyph, tip) = match app.theme_pref {
        ThemePreference::System => (icons::MONITOR, tl!("System")),
        ThemePreference::Light => (icons::SUN, tl!("Light")),
        ThemePreference::Dark => (icons::MOON, tl!("Dark")),
    };
    if button(ui, theme_rect, "theme", glyph, false, tip).clicked() {
        let all = ThemePreference::ALL;
        let at = all.iter().position(|p| *p == app.theme_pref).unwrap_or(0);
        app.set_theme_preference(all[(at + 1) % all.len()]);
    }
    let lang_rect = Rect::from_center_size(
        Pos2::new(cx, theme_rect.min.y - 12.0 - 18.0),
        Vec2::new(48.0, 36.0),
    );
    let response = ui.interact(lang_rect, ui.scope_id().with("rail-lang"), Sense::click());
    draw::rounded(
        ui.painter(),
        lang_rect,
        CornerRadius::same(10),
        if response.hovered() {
            tokens.border
        } else {
            tokens.surface_3
        },
        None,
    );
    fonts::draw_centered(
        ui.painter(),
        lang_rect,
        &app.lang.code().to_uppercase(),
        Text::body(12.0, 600, tokens.text),
    );
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        let next = Lang::all()
            .nth((Lang::all().position(|l| l == app.lang).unwrap_or(0) + 1) % Lang::all().count());
        if let Some(next) = next {
            app.set_language(next);
        }
    }
    let cache_rect = Rect::from_center_size(
        Pos2::new(cx, lang_rect.min.y - 12.0 - 24.0),
        Vec2::splat(48.0),
    );
    let response =
        cache_picker(ui.interact(cache_rect, ui.scope_id().with("rail-cache"), Sense::click()));
    draw::rounded(
        ui.painter(),
        cache_rect,
        CornerRadius::same(12),
        if response.hovered() {
            tokens.surface_3
        } else {
            tokens.surface_2
        },
        Some((1.0, tokens.border)),
    );
    fonts::icon(
        ui.painter(),
        cache_rect.center(),
        icons::HARD_DRIVE,
        18.0,
        tokens.text_2,
    );
    let dot = if app.scan.is_some() {
        tokens.primary
    } else {
        tokens.text_3
    };
    draw::dot(
        ui.painter(),
        cache_rect.right_top() + Vec2::new(-9.0, 9.0),
        8.0,
        dot,
    );
    if response.clicked() {
        app.open_cache_switcher();
    }
}

fn page_entry(page: Page) -> (char, &'static str) {
    match page {
        Page::Overview => (icons::LAYOUT_DASHBOARD, tl!("Overview")),
        Page::Garage => (icons::WAREHOUSE, tl!("Garage")),
        Page::Resources => (icons::DATABASE, tl!("Resources")),
        Page::Packs => (icons::PACKAGE, tl!("Packs")),
        Page::Info => (icons::INFO, tl!("Info")),
    }
}

/// The card with the connected game and cache state. It is a picker: the chevron, the hover state,
/// the hand cursor and the tooltip say so, and clicking it opens the cache switcher.
fn cache_card(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let response = ui.interact(rect, ui.scope_id().with("cache-card"), egui::Sense::click());
    let response = cache_picker(response);
    let (fill, border, chevron) = if response.hovered() {
        (tokens.surface_3, tokens.border_strong, tokens.text)
    } else {
        (tokens.surface_2, tokens.border, tokens.text_3)
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        fill,
        Some((1.0, border)),
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.max.x - 14.0 - 8.0, rect.min.y + 22.0),
        icons::CHEVRONS_UP_DOWN,
        16.0,
        chevron,
    );
    let (title, meta, connected) = match &app.scan {
        Some(scan) => (
            game_name(&scan.game),
            fmt(
                tl!("{os} · {n} files"),
                &[("os", os_name()), ("n", &thousands(scan.resources.len()))],
            ),
            true,
        ),
        None => (
            tl!("Cache not connected").to_owned(),
            tl!("Game cache not found").to_owned(),
            false,
        ),
    };
    let x = rect.min.x + 14.0;
    fonts::icon(
        ui.painter(),
        Pos2::new(x + 8.0, rect.min.y + 22.0),
        icons::HARD_DRIVE,
        16.0,
        tokens.text_2,
    );
    // The texts give way to the chevron.
    let room = rect.max.x - 14.0 - 16.0 - 8.0;
    let title_style = Text::body(13.0, 600, tokens.text);
    let title = fonts::ellipsize(ui.painter(), &title, title_style, room - (x + 24.0));
    fonts::draw_left(
        ui.painter(),
        x + 24.0,
        rect.min.y + 22.0,
        &title,
        title_style,
    );
    let meta_style = Text::body(12.0, 400, tokens.text_3);
    let meta = fonts::ellipsize(ui.painter(), &meta, meta_style, room - x);
    fonts::draw(
        ui.painter(),
        Pos2::new(x, rect.min.y + 38.0),
        &meta,
        meta_style,
    );
    if connected {
        let label = tl!("Cache found");
        let chip = Rect::from_min_size(
            Pos2::new(x, rect.min.y + 61.0),
            Vec2::new(widgets::chip_width(ui, label), 24.0),
        );
        widgets::paint_chip(ui, chip, ChipKind::Success, label);
    }
    if response.clicked() {
        app.open_cache_switcher();
    }
}

/// The cache card and the rail's cache button open the cache switcher
/// ([`Studio::open_cache_switcher`]): a hand cursor, a tooltip and an accessible name say so.
fn cache_picker(response: egui::Response) -> egui::Response {
    let label = tl!("Switch game cache");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label));
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label)
}

/// Draw the current page into `main` (everything right of the sidebar).
pub fn page(app: &mut Studio, ui: &mut Ui, main: Rect) {
    if app.scan.is_none() {
        super::empty::draw(app, ui, main);
        return;
    }
    match app.page {
        Page::Overview => super::overview::draw(app, ui, main),
        Page::Garage => super::garage::draw(app, ui, main),
        Page::Resources => super::resources::draw(app, ui, main),
        Page::Packs => super::packs::draw(app, ui, main),
        Page::Info => super::info::draw(app, ui, main),
    }
}

pub fn top_bar_rect(main: Rect) -> Rect {
    Rect::from_min_size(main.min, Vec2::new(main.width(), TOP_BAR_HEIGHT))
}

/// "today, 21:40", "yesterday, 18:12" or "2026-10-20, 09:05" in the local time zone.
pub fn describe_time(unix: u64) -> String {
    use crate::model::{Day, local_time, now_and_offset};
    let (now, offset) = now_and_offset();
    let (day, clock) = local_time(unix, now, offset);
    let day = match day {
        Day::Today => tl!("today").to_owned(),
        Day::Yesterday => tl!("yesterday").to_owned(),
        Day::Date(date) => date,
    };
    format!("{day}, {clock}")
}

/// How long ago something happened: "just now", "5 min ago", "2 h ago".
pub fn describe_age(age: std::time::Duration) -> String {
    let minutes = age.as_secs() / 60;
    match minutes {
        0 => tl!("just now").to_owned(),
        1..=59 => fmt(tl!("{n} min ago"), &[("n", &minutes.to_string())]),
        _ => fmt(tl!("{n} h ago"), &[("n", &(minutes / 60).to_string())]),
    }
}

/// The short client name used on chips.
pub fn game_short_name(game: &str) -> String {
    match game {
        "air-cache" => tl!("Game").to_owned(),
        other => crate::model::target_title(other),
    }
}
