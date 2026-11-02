// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Settings, one column of grouped rows (as Windows 11 and GNOME lay them out): the game cache
//! (which one, when it was last read, how often it is read again, what the backups take) and the
//! appearance (theme, language, interface size). The backups have their own page.

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use tanki_i18n::{Lang, fmt, tl, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::{ThemePreference, Tokens},
    widgets::{self, Button, ButtonKind, GroupRow, Tint},
};

use super::{scroll_page, shell};
use crate::{
    app::{Page, Studio},
    later::RescanMode,
    model::format_bytes,
};

/// The column is at most this wide; a wider window leaves the rest empty (as settings pages do).
const COLUMN: f32 = 800.0;
const SEGMENTS: f32 = 32.0;

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Settings"),
        tl!("Game cache, backups and appearance"),
        |_| {},
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let width = (body.width() - 64.0).clamp(240.0, COLUMN);
    let cache = cache_rows(app, ui);
    let look = look_rows(app, ui);
    let height = 32.0
        + 29.0
        + widgets::group_height(ui, width, &cache.rows())
        + 28.0
        + 29.0
        + widgets::group_height(ui, width, &look.rows())
        + 32.0;
    scroll_page(ui, body, height, |ui, rect| {
        let x = rect.min.x + 32.0;
        let mut y = widgets::group_heading(ui, Pos2::new(x, rect.min.y + 32.0), tl!("Game cache"));
        y = cache.draw(app, ui, Pos2::new(x, y), width) + 28.0;
        y = widgets::group_heading(ui, Pos2::new(x, y), tl!("Appearance"));
        look.draw(app, ui, Pos2::new(x, y), width);
    });
}

/// "Today, 11:04" for a moment of the last two days, else the date.
fn when(unix: u64) -> String {
    let text = shell::describe_time(unix);
    let mut chars = text.chars();
    chars.next().map_or(text.clone(), |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}

struct CacheRows {
    cache_desc: String,
    last_desc: String,
    scan_desc: &'static str,
    usage_text: String,
    switch: Vec2,
    rescan: Vec2,
    modes: Vec2,
    usage: Vec2,
}

const USAGE_EXTRA: f32 = 8.0 + 10.0 + 18.0;

fn cache_rows(app: &Studio, ui: &Ui) -> CacheRows {
    let tokens = Tokens::get(ui.ctx());
    let caches = app.roots.len();
    let cache_desc = match &app.scan {
        Some(scan) => {
            let mut text = fmt(
                tl!("{os} · {n} files"),
                &[
                    ("os", shell::os_name()),
                    ("n", &crate::model::thousands(scan.resources.len())),
                ],
            );
            if caches > 1 {
                text = format!(
                    "{text} · {}",
                    tn(caches as u64, "{n} cache found", "{n} caches found")
                );
            }
            text
        }
        None => tl!("No game cache found").to_owned(),
    };
    let last_desc = match app.last_check {
        Some(check) => fmt(
            tl!("{when} · {what}"),
            &[
                ("when", &when(check.at)),
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
    let scan_desc = match app.rescan_mode {
        RescanMode::Auto => {
            tl!("In the cache in use: at launch, on return to the window and every 5 minutes")
        }
        RescanMode::Hourly => tl!("In the cache in use: at launch and every hour"),
        RescanMode::Manual => tl!("In the cache in use: only with Rescan now"),
    };
    let usage = app.usage;
    let usage_text = match app.backup_limit {
        Some(limit) => fmt(
            tl!("{used} of {limit}"),
            &[
                ("used", &format_bytes(usage.total())),
                ("limit", &crate::backups::limit_label(Some(limit))),
            ],
        ),
        None => format_bytes(usage.total()),
    };
    let usage_width = fonts::width(
        ui.painter(),
        &usage_text,
        Text::mono(13.0, 600, tokens.text_2),
    ) + 8.0
        + 16.0;
    let mode_items = mode_items();
    CacheRows {
        cache_desc,
        last_desc,
        scan_desc,
        switch: Vec2::new(switch_button().measure(ui), 40.0),
        rescan: Vec2::new(rescan_button().measure(ui), 40.0),
        modes: Vec2::new(
            widgets::choice_segments_width(ui, &mode_items, mode_index(app)),
            SEGMENTS,
        ),
        usage: Vec2::new(usage_width, 20.0),
        usage_text,
    }
}

fn switch_button() -> Button<'static> {
    Button::new(ButtonKind::Secondary, tl!("Switch")).icon(icons::ARROW_LEFT_RIGHT)
}

fn rescan_button() -> Button<'static> {
    Button::new(ButtonKind::Secondary, tl!("Rescan now")).icon(icons::REFRESH_CW)
}

fn mode_items() -> [(&'static str, Option<String>); 3] {
    [
        (tl!("Automatic"), None),
        (tl!("Every hour"), None),
        (tl!("Manual"), None),
    ]
}

fn mode_index(app: &Studio) -> usize {
    RescanMode::ALL
        .iter()
        .position(|m| *m == app.rescan_mode)
        .unwrap_or(0)
}

impl CacheRows {
    fn rows(&self) -> [GroupRow<'_>; 4] {
        [
            GroupRow {
                icon: icons::HARD_DRIVE,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Cache in use"),
                desc: Some(&self.cache_desc),
                control: self.switch,
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::HISTORY,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Last check"),
                desc: Some(&self.last_desc),
                control: self.rescan,
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::REFRESH_CW,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Look for new game files"),
                desc: Some(self.scan_desc),
                control: self.modes,
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::ARCHIVE,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Backups"),
                desc: Some(tl!("Original files, so any skin can be taken off")),
                control: self.usage,
                extra: USAGE_EXTRA,
                click: true,
            },
        ]
    }

    fn draw(&self, app: &mut Studio, ui: &mut Ui, origin: Pos2, width: f32) -> f32 {
        let rows = self.rows();
        let height = widgets::group_height(ui, width, &rows);
        let places = widgets::group(ui, origin, width, &rows, "settings-cache");
        // The same list of caches as the sidebar's cache card, with Choose folder… in it.
        if in_rect(ui, places[0].control, |ui| {
            switch_button().show(ui).clicked()
        }) {
            app.open_cache_switcher();
        }
        if in_rect(ui, places[1].control, |ui| {
            rescan_button().show(ui).clicked()
        }) {
            app.rescan();
        }
        let (_, clicked) = widgets::choice_segments(
            ui,
            places[2].control.min,
            SEGMENTS,
            &mode_items(),
            mode_index(app),
            "rescan-mode",
        );
        if let Some(mode) = clicked.and_then(|i| RescanMode::ALL.get(i)) {
            app.set_rescan_mode(*mode);
        }
        let tokens = Tokens::get(ui.ctx());
        let usage = places[3].control;
        fonts::draw_left(
            ui.painter(),
            usage.min.x,
            usage.center().y,
            &self.usage_text,
            Text::mono(13.0, 600, tokens.text_2),
        );
        fonts::icon(
            ui.painter(),
            Pos2::new(usage.max.x - 8.0, usage.center().y),
            icons::CHEVRON_RIGHT,
            16.0,
            tokens.text_3,
        );
        usage_bar(app, ui, places[3].extra);
        if places[3].clicked {
            app.go(Page::Backups);
        }
        origin.y + height
    }
}

/// The backups against the limit: needed ones in green, old ones in grey, and a legend.
pub(crate) fn usage_bar(app: &Studio, ui: &Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let usage = app.usage;
    let scale = app.backup_limit.unwrap_or(0).max(usage.total()).max(1) as f32;
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 8.0));
    draw::rounded(
        ui.painter(),
        bar,
        CornerRadius::same(4),
        tokens.surface_3,
        None,
    );
    let part = |bytes: u64| bar.width() * (bytes as f32 / scale).min(1.0);
    let old = part(usage.total());
    if old > 0.0 {
        draw::rounded(
            ui.painter(),
            Rect::from_min_size(bar.min, Vec2::new(old.max(8.0), 8.0)),
            CornerRadius::same(4),
            tokens.text_3,
            None,
        );
    }
    let needed = part(usage.needed_bytes);
    if needed > 0.0 {
        draw::rounded(
            ui.painter(),
            Rect::from_min_size(bar.min, Vec2::new(needed.max(8.0), 8.0)),
            CornerRadius::same(4),
            tokens.primary,
            None,
        );
    }
    let y = bar.max.y + 10.0 + 9.0;
    let mut x = rect.min.x;
    for (color, label, bytes) in [
        (tokens.primary, tl!("Needed to undo"), usage.needed_bytes),
        (tokens.text_3, tl!("Old"), usage.old_bytes),
    ] {
        draw::dot(ui.painter(), Pos2::new(x + 4.0, y), 8.0, color);
        let used = fonts::draw_left(
            ui.painter(),
            x + 16.0,
            y,
            label,
            Text::body(13.0, 400, tokens.text_2),
        );
        let size = fonts::draw_left(
            ui.painter(),
            used.max.x + 8.0,
            y,
            &format_bytes(bytes),
            Text::mono(12.0, 400, tokens.text_3),
        );
        x = size.max.x + 20.0;
    }
}

struct LookRows {
    themes: Vec2,
    langs: Vec2,
    size: Vec2,
}

fn theme_items() -> [(&'static str, Option<String>); 3] {
    [
        (tl!("System"), None),
        (tl!("Light"), None),
        (tl!("Dark"), None),
    ]
}

const LANGS: [(&str, Option<String>); 2] = [("English", None), ("Русский", None)];
const STEPPER: Vec2 = Vec2::new(3.0 + 30.0 + 2.0 + 56.0 + 2.0 + 30.0 + 3.0, 34.0);

fn look_rows(app: &Studio, ui: &Ui) -> LookRows {
    LookRows {
        themes: Vec2::new(
            widgets::choice_segments_width(ui, &theme_items(), theme_index(app)),
            SEGMENTS,
        ),
        langs: Vec2::new(
            widgets::choice_segments_width(ui, &LANGS, usize::from(app.lang != Lang::EN)),
            SEGMENTS,
        ),
        size: STEPPER,
    }
}

fn theme_index(app: &Studio) -> usize {
    ThemePreference::ALL
        .iter()
        .position(|p| *p == app.theme_pref)
        .unwrap_or(0)
}

impl LookRows {
    fn rows(&self) -> [GroupRow<'_>; 3] {
        [
            GroupRow {
                icon: icons::SUN_MOON,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Theme"),
                desc: Some(tl!("Like the system, or always light or dark")),
                control: self.themes,
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::LANGUAGES,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Language"),
                desc: None,
                control: self.langs,
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::ZOOM_IN,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Interface size"),
                desc: Some(if cfg!(target_os = "macos") {
                    tl!("Also ⌘ and + or −")
                } else {
                    tl!("Also Ctrl and + or −")
                }),
                control: self.size,
                extra: 0.0,
                click: false,
            },
        ]
    }

    fn draw(&self, app: &mut Studio, ui: &mut Ui, origin: Pos2, width: f32) {
        let rows = self.rows();
        let places = widgets::group(ui, origin, width, &rows, "settings-look");
        let (_, clicked) = widgets::choice_segments(
            ui,
            places[0].control.min,
            SEGMENTS,
            &theme_items(),
            theme_index(app),
            "theme",
        );
        if let Some(pref) = clicked.and_then(|i| ThemePreference::ALL.get(i)) {
            app.set_theme_preference(*pref);
        }
        let (_, clicked) = widgets::choice_segments(
            ui,
            places[1].control.min,
            SEGMENTS,
            &LANGS,
            usize::from(app.lang != Lang::EN),
            "language",
        );
        if let Some(lang) = clicked.and_then(|i| Lang::all().nth(i)) {
            app.set_language(lang);
        }
        if let Some(step) = stepper(ui, places[2].control, app.ui_scale) {
            app.zoom(step);
        }
    }
}

/// − 100% +: the interface size, a step down or up per click. Returns the step clicked.
fn stepper(ui: &mut Ui, rect: Rect, scale: f32) -> Option<i32> {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(9),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let mut step = None;
    for (offset, icon, direction, label) in [
        (3.0, icons::MINUS, -1, tl!("Smaller")),
        (rect.width() - 3.0 - 30.0, icons::PLUS, 1, tl!("Larger")),
    ] {
        let button = Rect::from_min_size(
            Pos2::new(rect.min.x + offset, rect.min.y + 3.0),
            Vec2::new(30.0, rect.height() - 6.0),
        );
        let response = ui.interact(
            button,
            ui.make_persistent_id(("zoom", direction)),
            Sense::click(),
        );
        response.widget_info(|| {
            eframe::egui::WidgetInfo::labeled(eframe::egui::Role::Button, true, label)
        });
        if response.hovered() {
            draw::rounded(
                ui.painter(),
                button,
                CornerRadius::same(7),
                tokens.surface_2,
                None,
            );
        }
        fonts::icon(ui.painter(), button.center(), icon, 14.0, tokens.text_2);
        if response
            .on_hover_cursor(eframe::egui::CursorIcon::PointingHand)
            .clicked()
        {
            step = Some(direction);
        }
    }
    fonts::draw_centered(
        ui.painter(),
        rect,
        &format!("{}%", (scale * 100.0).round() as u32),
        Text::mono(13.0, 600, tokens.text),
    );
    step
}
