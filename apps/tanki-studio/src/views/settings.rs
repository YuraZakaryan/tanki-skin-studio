// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings: how often the cache is read again, the skins that go on later, appearance and what
//! the app is.

use eframe::egui::{self, Align, CornerRadius, Layout, Pos2, Rect, Ui, Vec2};
use tanki_i18n::{Lang, fmt, tl, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect},
    theme::{ThemePreference, Tokens},
    widgets::{self, Button, ButtonKind, Width},
};

use super::{scroll_page, shell};
use crate::{about, app::Page, app::Studio, later::RescanMode};

const ROW: f32 = 36.0;

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Settings"),
        tl!("Cache, skins that go on later, appearance and about"),
        |_| {},
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    // Stacked, the four panels need more room than side by side.
    let wide = super::is_wide(body.shrink2(Vec2::new(32.0, 0.0)));
    let min_height = if wide { 764.0 + 64.0 } else { 1100.0 };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        // Two columns when the page is wide, one under the other when it is not.
        if super::is_wide(content) {
            let cols = columns(content, 24.0, &[Size::Fill(1.0), Size::Fill(1.0)]);
            let (left, right) = (cols[0], cols[1]);
            let cache = cache_panel(app, ui, left);
            later_panel(app, ui, left, cache.max.y + 24.0);
            let look = appearance_panel(app, ui, right);
            about_panel(app, ui, right, look.max.y + 24.0);
        } else {
            let cache = cache_panel(app, ui, content);
            let later = later_panel(app, ui, content, cache.max.y + 24.0);
            let look = appearance_panel(
                app,
                ui,
                Rect::from_min_max(Pos2::new(content.min.x, later.max.y + 24.0), content.max),
            );
            about_panel(app, ui, content, look.max.y + 24.0);
        }
    });
}

/// A panel `content` tall (under its header) at the top of `column` from `top`.
fn panel_rect(column: Rect, top: f32, content: f32) -> Rect {
    Rect::from_min_size(
        Pos2::new(column.min.x, top),
        Vec2::new(column.width(), 45.0 + 18.0 + content + 18.0),
    )
}

/// A row's icon in its 36 px box, the title and a description under it (wrapped to `width`).
/// Returns the row's height.
fn row_text(
    ui: &Ui,
    origin: Pos2,
    icon: char,
    info: bool,
    title: &str,
    desc: Option<&str>,
    width: f32,
) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    let (fill, ink) = if info {
        (tokens.info_soft, tokens.info)
    } else {
        (tokens.surface_3, tokens.text_2)
    };
    let text_x = origin.x + 36.0 + 14.0;
    let desc = desc.map(|desc| {
        fonts::paragraph(
            painter,
            desc,
            Text::body(12.0, 400, tokens.text_3),
            width - 50.0,
            None,
        )
    });
    let title_height = 19.0;
    let text_height = title_height + desc.as_ref().map_or(0.0, |g| 2.0 + g.size().y);
    let height = text_height.max(ROW);
    let icon_box = Rect::from_min_size(
        Pos2::new(origin.x, origin.y + (height - ROW) / 2.0),
        Vec2::splat(ROW),
    );
    draw::rounded(painter, icon_box, CornerRadius::same(8), fill, None);
    fonts::icon(painter, icon_box.center(), icon, 16.0, ink);
    let text_top = origin.y + (height - text_height) / 2.0;
    fonts::draw(
        painter,
        Pos2::new(text_x, text_top),
        title,
        Text::body(14.0, 600, tokens.text),
    );
    if let Some(desc) = desc {
        painter.galley(
            Pos2::new(text_x, text_top + title_height + 2.0),
            desc,
            tokens.text_3,
        );
    }
    height
}

/// A button against the right edge of `row`, vertically centred.
fn right_button(ui: &mut Ui, row: Rect, button: Button<'_>) -> bool {
    let width = button.measure(ui);
    let place = Rect::from_min_size(
        Pos2::new(row.max.x - width, row.center().y - 20.0),
        Vec2::new(width, 40.0),
    );
    in_rect(ui, place, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            button.show(ui).clicked()
        })
        .inner
    })
}

fn cache_panel(app: &mut Studio, ui: &mut Ui, column: Rect) -> Rect {
    let tokens = Tokens::get(ui.ctx());
    let width = column.width() - 36.0;
    let note = fonts::paragraph(
        ui.painter(),
        tl!(
            "Automatic: at launch, when you come back to the window and every 5 minutes. A check barely loads the computer."
        ),
        Text::body(12.0, 400, tokens.text_2),
        width - 24.0 - 14.0 - 10.0,
        None,
    );
    let note_height = note.size().y + 20.0;
    let content = ROW + 12.0 + 32.0 + 12.0 + note_height + 12.0 + 40.0;
    let rect = panel_rect(column, column.min.y, content);
    let meta = app.scan.as_ref().map(|scan| {
        fmt(
            tl!("{os} · {n} files"),
            &[
                ("os", shell::os_name()),
                ("n", &crate::model::thousands(scan.resources.len())),
            ],
        )
    });
    let body = widgets::panel(ui, rect, tl!("Game cache"), meta.as_deref());
    let inner = body.shrink(18.0);
    let mut y = inner.min.y;
    row_text(
        ui,
        Pos2::new(inner.min.x, y),
        icons::REFRESH_CW,
        false,
        tl!("Rescan the cache"),
        Some(tl!("The Studio notices new game files by itself.")),
        width,
    );
    y += ROW + 12.0;
    let modes = [
        (tl!("Automatic"), None),
        (tl!("Every hour"), None),
        (tl!("Manual"), None),
    ];
    let active = RescanMode::ALL
        .iter()
        .position(|m| *m == app.rescan_mode)
        .unwrap_or(0);
    let (_, clicked) = widgets::choice_segments(
        ui,
        Pos2::new(inner.min.x, y),
        32.0,
        &modes,
        active,
        "rescan-mode",
    );
    if let Some(mode) = clicked.and_then(|i| RescanMode::ALL.get(i)) {
        app.set_rescan_mode(*mode);
    }
    y += 32.0 + 12.0;
    let note_rect = Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(width, note_height));
    draw::rounded(
        ui.painter(),
        note_rect,
        CornerRadius::same(10),
        tokens.surface_2,
        None,
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(note_rect.min.x + 12.0 + 7.0, note_rect.center().y),
        icons::INFO,
        14.0,
        tokens.text_3,
    );
    ui.painter().galley(
        Pos2::new(note_rect.min.x + 12.0 + 14.0 + 10.0, note_rect.min.y + 10.0),
        note,
        tokens.text_2,
    );
    y += note_height + 12.0;
    let last = match app.last_check {
        Some(check) => fmt(
            tl!("{when} · {what}"),
            &[
                ("when", &shell::describe_time(check.at)),
                (
                    "what",
                    if check.changed {
                        tl!("new files read")
                    } else {
                        tl!("no new files")
                    },
                ),
            ],
        ),
        None => tl!("Not yet").to_owned(),
    };
    let row = Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(width, 40.0));
    let button = Button::new(ButtonKind::Secondary, tl!("Rescan now")).icon(icons::REFRESH_CW);
    let button_width = button.measure(ui);
    row_text(
        ui,
        Pos2::new(row.min.x, row.min.y + 2.0),
        icons::HISTORY,
        false,
        tl!("Last check"),
        Some(&last),
        width - button_width - 12.0,
    );
    if right_button(ui, row, button) {
        app.rescan();
    }
    rect
}

fn later_panel(app: &mut Studio, ui: &mut Ui, column: Rect, top: f32) -> Rect {
    let root = app
        .scan
        .as_ref()
        .map(|s| s.root.to_string_lossy().into_owned())
        .unwrap_or_default();
    let count = crate::later::of_cache(&app.later, &root).len();
    let width = column.width() - 36.0;
    let button = Button::new(ButtonKind::Ghost, tl!("Open the list")).icon(icons::ARROW_RIGHT);
    let button_width = button.measure(ui);
    let (title, desc) = if count == 0 {
        (
            tl!("Nothing waits").to_owned(),
            tl!(
                "A modification the game has not downloaded yet waits here, then goes on by itself."
            ),
        )
    } else {
        (
            tn(
                count as u64,
                "{n} modification goes on later",
                "{n} modifications go on later",
            ),
            tl!("The Studio puts it on by itself once the game downloads it."),
        )
    };
    // Measure first: the description wraps beside the button.
    let text_width = if count > 0 {
        width - button_width - 12.0
    } else {
        width
    };
    let tokens = Tokens::get(ui.ctx());
    let measured = fonts::paragraph(
        ui.painter(),
        desc,
        Text::body(12.0, 400, tokens.text_3),
        text_width - 50.0,
        None,
    )
    .size()
    .y;
    let row_height = (19.0 + 2.0 + measured).max(ROW);
    let rect = panel_rect(column, top, row_height);
    let meta = count.to_string();
    let body = widgets::panel(ui, rect, tl!("Go on later"), Some(&meta));
    let inner = body.shrink(18.0);
    row_text(
        ui,
        inner.min,
        icons::CLOCK_3,
        true,
        &title,
        Some(desc),
        text_width,
    );
    let row = Rect::from_min_size(inner.min, Vec2::new(width, row_height));
    if count > 0 && right_button(ui, row, button) {
        app.go(Page::Packs);
    }
    rect
}

fn appearance_panel(app: &mut Studio, ui: &mut Ui, column: Rect) -> Rect {
    let tokens = Tokens::get(ui.ctx());
    let width = column.width() - 36.0;
    let themes = [
        (tl!("System"), None),
        (tl!("Light"), None),
        (tl!("Dark"), None),
    ];
    let active = ThemePreference::ALL
        .iter()
        .position(|p| *p == app.theme_pref)
        .unwrap_or(0);
    let seg_width = widgets::choice_segments_width(ui, &themes, active);
    let theme_desc = tl!("Like the system, or always light or dark");
    // The description wraps beside the switch: the row is as tall as it needs.
    let desc_height = fonts::paragraph(
        ui.painter(),
        theme_desc,
        Text::body(12.0, 400, tokens.text_3),
        width - seg_width - 12.0 - 50.0,
        None,
    )
    .size()
    .y;
    let theme_height = (19.0 + 2.0 + desc_height).max(ROW);
    let content = theme_height + 12.0 + ROW;
    let rect = panel_rect(column, column.min.y, content);
    let body = widgets::panel(ui, rect, tl!("Appearance"), None);
    let inner = body.shrink(18.0);
    row_text(
        ui,
        inner.min,
        icons::SUN_MOON,
        false,
        tl!("Theme"),
        Some(theme_desc),
        width - seg_width - 12.0,
    );
    let (_, clicked) = widgets::choice_segments(
        ui,
        Pos2::new(
            inner.max.x - seg_width,
            inner.min.y + (theme_height - 32.0) / 2.0,
        ),
        32.0,
        &themes,
        active,
        "theme",
    );
    if let Some(pref) = clicked.and_then(|i| ThemePreference::ALL.get(i)) {
        app.set_theme_preference(*pref);
    }
    let y = inner.min.y + theme_height + 12.0;
    let langs = [("English", None), ("Русский", None)];
    let active = usize::from(app.lang != Lang::EN);
    let seg_width = widgets::choice_segments_width(ui, &langs, active);
    row_text(
        ui,
        Pos2::new(inner.min.x, y),
        icons::LANGUAGES,
        false,
        tl!("Language"),
        None,
        width - seg_width - 12.0,
    );
    let (_, clicked) = widgets::choice_segments(
        ui,
        Pos2::new(inner.max.x - seg_width, y + 2.0),
        32.0,
        &langs,
        active,
        "language",
    );
    if let Some(lang) = clicked.and_then(|i| Lang::all().nth(i)) {
        app.set_language(lang);
    }
    rect
}

fn about_panel(app: &mut Studio, ui: &mut Ui, column: Rect, top: f32) {
    let tokens = Tokens::get(ui.ctx());
    let links = [
        (icons::CODE, tl!("Source code"), ""),
        (icons::FILE_TEXT, tl!("What's new"), "/releases"),
        (icons::BUG, tl!("Report a problem"), "/issues"),
    ];
    // The links wrap when the column is narrow: each keeps its own width, 8 apart. A link has no
    // fill until hovered, so it is padded less than a button.
    let room = column.width() - 36.0;
    let mut places = Vec::new();
    let (mut x, mut line) = (0.0, 0usize);
    for (icon, label, _) in &links {
        let width = Button::new(ButtonKind::Ghost, label)
            .icon(*icon)
            .measure(ui)
            - 12.0;
        if x > 0.0 && x + width > room {
            x = 0.0;
            line += 1;
        }
        places.push((x, line, width));
        x += width + 8.0;
    }
    let lines = line + 1;
    let content = 48.0 + 12.0 + lines as f32 * 40.0 + line as f32 * 8.0;
    let rect = panel_rect(column, top, content);
    let body = widgets::panel(ui, rect, tl!("About"), None);
    let inner = body.shrink(18.0);
    let logo = Rect::from_min_size(inner.min, Vec2::splat(48.0));
    draw::rounded(
        ui.painter(),
        logo,
        CornerRadius::same(12),
        tokens.primary_soft,
        None,
    );
    fonts::icon(
        ui.painter(),
        logo.center(),
        icons::PAINTBRUSH,
        22.0,
        tokens.primary,
    );
    let text_x = logo.max.x + 14.0;
    fonts::draw(
        ui.painter(),
        Pos2::new(text_x, logo.min.y + 4.0),
        "Tanki Skin Studio",
        Text::display(18.0, 700, tokens.text),
    );
    let version = fmt(
        tl!("Version {version} · {license}"),
        &[
            ("version", env!("CARGO_PKG_VERSION")),
            ("license", about::LICENSE),
        ],
    );
    fonts::draw(
        ui.painter(),
        Pos2::new(text_x, logo.min.y + 28.0),
        &version,
        Text::body(12.0, 400, tokens.text_3),
    );
    let head = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 48.0));
    let check = Button::new(ButtonKind::Secondary, tl!("Check for updates"))
        .icon(icons::REFRESH_CW)
        .enabled(!app.updates.checking());
    if right_button(ui, head, check) {
        app.check_for_updates();
    }
    let source = format!("https://{}", about::SOURCE);
    let mut open = None;
    for ((icon, label, path), (x, line, width)) in links.iter().zip(places) {
        let place = Rect::from_min_size(
            Pos2::new(
                inner.min.x + x,
                head.max.y + 12.0 + line as f32 * (40.0 + 8.0),
            ),
            Vec2::new(width, 40.0),
        );
        if in_rect(ui, place, |ui| {
            Button::new(ButtonKind::Ghost, label)
                .icon(*icon)
                .width(Width::Exact(width))
                .show(ui)
                .clicked()
        }) {
            open = Some(format!("{source}{path}"));
        }
    }
    if let Some(url) = open {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}
