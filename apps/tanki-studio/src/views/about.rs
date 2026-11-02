// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! About: the app (version, whether it is up to date, its source), what changed in each version
//! (the changelog built in, newest first, one version open at a time) and the community.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use tanki_i18n::{fmt, tl};
use tanki_ui::{
    brand, draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, GroupRow, Mark, Tint},
};

use super::{scroll_page, shell};
use crate::{about, app::Studio, changelog, updates::Latest};

/// Side by side from this width; one column under it.
const TWO_COLUMNS: f32 = 900.0;
const RIGHT: f32 = 440.0;
const HERO: f32 = 100.0;
const RELEASE_HEAD: f32 = 46.0;

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("About"),
        tl!("Version, what's new and community"),
        |_| {},
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let releases = changelog::releases(app.lang);
    let content_width = body.width() - 64.0;
    let wide = content_width >= TWO_COLUMNS;
    let community = community_rows(app, ui.ctx());
    let side_width = if wide { RIGHT } else { content_width };
    let community_height = 29.0
        + widgets::group_height(ui, side_width, &community)
        + 10.0
        + trademark_note(ui, side_width).size().y;
    // One column: the notes get room for the open release and every header, then the page
    // scrolls.
    let notes_min = 29.0 + notes_height(app, ui, &releases, content_width) + 48.0;
    let min_height = if wide {
        32.0 + HERO + 24.0 + community_height.max(360.0) + 32.0
    } else {
        32.0 + HERO + 24.0 + notes_min + 24.0 + community_height + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let inner = rect.shrink(32.0);
        let hero = Rect::from_min_size(inner.min, Vec2::new(inner.width(), HERO));
        draw_hero(app, ui, hero, &releases);
        let below = Rect::from_min_max(Pos2::new(inner.min.x, hero.max.y + 24.0), inner.max);
        let (notes, side) = if wide {
            (
                Rect::from_min_max(
                    below.min,
                    Pos2::new(below.max.x - RIGHT - 24.0, below.max.y),
                ),
                Rect::from_min_max(Pos2::new(below.max.x - RIGHT, below.min.y), below.max),
            )
        } else {
            let notes = Rect::from_min_size(below.min, Vec2::new(below.width(), notes_min));
            (
                notes,
                Rect::from_min_max(Pos2::new(below.min.x, notes.max.y + 24.0), below.max),
            )
        };
        draw_notes(app, ui, notes, &releases);
        let y = widgets::group_heading(ui, side.min, tl!("Community"));
        let places = widgets::group(
            ui,
            Pos2::new(side.min.x, y),
            side.width(),
            &community,
            "community",
        );
        let tokens = Tokens::get(ui.ctx());
        if let Some(last) = places.last() {
            let note = trademark_note(ui, side.width());
            ui.painter().galley(
                Pos2::new(side.min.x, last.rect.max.y + 10.0),
                note,
                tokens.text_3,
            );
        }
        for (place, url) in places.iter().zip(LINKS) {
            fonts::icon(
                ui.painter(),
                place.control.center(),
                icons::EXTERNAL_LINK,
                16.0,
                tokens.text_3,
            );
            if place.clicked {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
    });
}

const LINKS: [&str; 3] = [
    about::TELEGRAM_CHANNEL,
    about::TELEGRAM_CHAT,
    about::YOUTUBE,
];

/// The links show their services' own marks, so the page says whose they are.
fn trademark_note(ui: &Ui, width: f32) -> std::sync::Arc<egui::Galley> {
    let tokens = Tokens::get(ui.ctx());
    fonts::paragraph(
        ui.painter(),
        tl!(
            "Telegram and YouTube are trademarks of their respective owners. Tanki Skin Studio is not affiliated with or endorsed by them."
        ),
        Text::body(11.0, 400, tokens.text_3),
        width,
        Some(15.0),
    )
}

fn community_rows(app: &mut Studio, ctx: &egui::Context) -> [GroupRow<'static>; 3] {
    let mut mark = |key: &str, bytes: &'static [u8], size| {
        app.texture(ctx, key, || Some(bytes.to_vec()))
            .map(|texture| Mark {
                texture: texture.id(),
                size,
            })
    };
    let telegram = mark("brand/telegram", brand::TELEGRAM, brand::TELEGRAM_SIZE);
    let youtube = mark("brand/youtube", brand::YOUTUBE, brand::YOUTUBE_SIZE);
    let row = |mark, tint, title, desc| GroupRow {
        icon: icons::EXTERNAL_LINK,
        mark,
        tint,
        title,
        desc: Some(desc),
        control: Vec2::splat(16.0),
        extra: 0.0,
        click: true,
    };
    [
        row(
            telegram,
            Tint::Info,
            tl!("Telegram channel"),
            tl!("News and new skin packs"),
        ),
        row(
            telegram,
            Tint::Info,
            tl!("Telegram chat"),
            tl!("Questions and help from players"),
        ),
        row(youtube, Tint::Danger, "YouTube", tl!("Video guides")),
    ]
}

fn draw_hero(app: &mut Studio, ui: &mut Ui, rect: Rect, releases: &[changelog::Release]) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(16),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let check = Button::new(ButtonKind::Secondary, tl!("Check for updates"))
        .icon(icons::REFRESH_CW)
        .enabled(!app.updates.checking());
    let source = Button::new(ButtonKind::Ghost, tl!("Source code")).icon(icons::CODE);
    let check_width = check.measure(ui);
    let source_width = source.measure(ui);
    // A narrow page drops the source button first; the check stays.
    let show_source = rect.width() - 24.0 - 60.0 - 18.0 - 260.0 > check_width + 10.0 + source_width;
    let place = Rect::from_min_size(
        Pos2::new(rect.max.x - 24.0 - check_width, rect.center().y - 20.0),
        Vec2::new(check_width, 40.0),
    );
    if in_rect(ui, place, |ui| check.show(ui).clicked()) {
        app.check_for_updates();
    }
    let mut right = place.min.x - 16.0;
    if show_source {
        let place = Rect::from_min_size(
            Pos2::new(place.min.x - 10.0 - source_width, rect.center().y - 20.0),
            Vec2::new(source_width, 40.0),
        );
        if in_rect(ui, place, |ui| source.show(ui).clicked()) {
            ui.ctx()
                .open_url(egui::OpenUrl::new_tab(format!("https://{}", about::SOURCE)));
        }
        right = place.min.x - 16.0;
    }
    let logo = Rect::from_min_size(
        Pos2::new(rect.min.x + 24.0, rect.center().y - 30.0),
        Vec2::splat(60.0),
    );
    draw::rounded(
        ui.painter(),
        logo,
        CornerRadius::same(16),
        tokens.primary_soft,
        None,
    );
    fonts::icon(
        ui.painter(),
        logo.center(),
        icons::PAINTBRUSH,
        28.0,
        tokens.primary,
    );
    let x = logo.max.x + 18.0;
    let painter = ui
        .painter()
        .with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(right, rect.max.y)));
    fonts::draw(
        &painter,
        Pos2::new(x, rect.center().y - 26.0),
        "Tanki Skin Studio",
        Text::display(22.0, 700, tokens.text),
    );
    let version = env!("CARGO_PKG_VERSION");
    let mut line = fmt(tl!("Version {version}"), &[("version", version)]);
    if let Some(release) = releases.iter().find(|r| r.version == version) {
        line = format!("{line} · {}", release.date);
    }
    line = format!("{line} · {}", about::LICENSE);
    let used = fonts::draw_left(
        &painter,
        x,
        rect.center().y + 14.0,
        &line,
        Text::body(13.0, 400, tokens.text_3),
    );
    let status = if app.updates.checking() {
        Some((ChipKind::Neutral, tl!("Checking…").to_owned()))
    } else {
        match &app.updates.latest {
            Some(Latest::UpToDate) => Some((ChipKind::Success, tl!("Up to date").to_owned())),
            Some(Latest::Newer(tag)) => Some((
                ChipKind::Warning,
                fmt(tl!("{version} is out"), &[("version", tag)]),
            )),
            None => None,
        }
    };
    if let Some((kind, label)) = status {
        let chip = Rect::from_min_size(
            Pos2::new(used.max.x + 10.0, rect.center().y + 2.0),
            Vec2::new(widgets::chip_width(ui, &label), 24.0),
        );
        if chip.max.x < right {
            widgets::paint_chip(ui, chip, kind, &label);
        }
    }
}

/// The height of the notes list: every release's header and the open one's notes.
fn notes_height(app: &Studio, ui: &Ui, releases: &[changelog::Release], width: f32) -> f32 {
    releases
        .iter()
        .map(|r| {
            RELEASE_HEAD
                + 1.0
                + if app.about_open.as_deref() == Some(r.version.as_str()) {
                    open_notes_height(ui, r, width)
                } else {
                    0.0
                }
        })
        .sum()
}

fn note_width(width: f32) -> f32 {
    width - 18.0 - 15.0 - 18.0
}

fn open_notes_height(ui: &Ui, release: &changelog::Release, width: f32) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    release
        .notes
        .iter()
        .map(|note| {
            fonts::paragraph(
                ui.painter(),
                note,
                Text::body(13.0, 400, tokens.text_2),
                note_width(width),
                Some(19.0),
            )
            .size()
            .y + 8.0
        })
        .sum::<f32>()
        + 8.0
}

/// What's new: a card filling `rect`, the releases scrolling inside it and the link to every
/// release on GitHub at its foot.
fn draw_notes(app: &mut Studio, ui: &mut Ui, rect: Rect, releases: &[changelog::Release]) {
    let tokens = Tokens::get(ui.ctx());
    let top = widgets::group_heading(ui, rect.min, tl!("What's new"));
    let card = Rect::from_min_max(Pos2::new(rect.min.x, top), rect.max);
    draw::rounded(
        ui.painter(),
        card,
        CornerRadius::same(12),
        tokens.surface,
        None,
    );
    let foot = Rect::from_min_max(Pos2::new(card.min.x, card.max.y - 48.0), card.max);
    draw::hrule(
        ui.painter(),
        foot.min.x,
        foot.max.x,
        foot.min.y,
        tokens.border,
    );
    let link = foot.shrink2(Vec2::new(18.0, 0.0));
    let label = tl!("All releases on GitHub");
    let label_style = Text::body(13.0, 600, tokens.primary);
    let label_width = fonts::width(ui.painter(), label, label_style);
    let response = ui.interact(
        Rect::from_min_size(link.min, Vec2::new(label_width + 28.0, link.height())),
        ui.make_persistent_id("all-releases"),
        Sense::click(),
    );
    let used = fonts::draw_left(
        ui.painter(),
        link.min.x,
        link.center().y,
        label,
        label_style,
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(used.max.x + 14.0, link.center().y),
        icons::EXTERNAL_LINK,
        14.0,
        tokens.primary,
    );
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        ui.ctx().open_url(egui::OpenUrl::new_tab(format!(
            "https://{}/releases",
            about::SOURCE
        )));
    }
    let list = Rect::from_min_max(card.min, Pos2::new(card.max.x, foot.min.y));
    let current = env!("CARGO_PKG_VERSION");
    let mut toggle = None;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("about-notes")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let width = list.width();
                for release in releases {
                    let open = app.about_open.as_deref() == Some(release.version.as_str());
                    let (head, response) =
                        ui.allocate_exact_size(Vec2::new(width, RELEASE_HEAD), Sense::click());
                    let name = release.version.clone();
                    response.widget_info(move || {
                        egui::WidgetInfo::selected(egui::Role::Button, true, open, name.clone())
                    });
                    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.hovered() {
                        ui.painter().rect_filled(head, 0.0, tokens.surface_2);
                    }
                    let y = head.center().y;
                    let used = fonts::draw_left(
                        ui.painter(),
                        head.min.x + 18.0,
                        y,
                        &release.version,
                        Text::mono(14.0, 700, if open { tokens.text } else { tokens.text_2 }),
                    );
                    if release.version == current {
                        let label = tl!("Current");
                        let chip = Rect::from_min_size(
                            Pos2::new(used.max.x + 10.0, y - 12.0),
                            Vec2::new(widgets::chip_width(ui, label), 24.0),
                        );
                        widgets::paint_chip(ui, chip, ChipKind::Success, label);
                    }
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(head.max.x - 18.0 - 8.0, y),
                        if open {
                            icons::CHEVRON_UP
                        } else {
                            icons::CHEVRON_DOWN
                        },
                        16.0,
                        tokens.text_3,
                    );
                    fonts::draw_right(
                        ui.painter(),
                        head.max.x - 18.0 - 16.0 - 12.0,
                        y,
                        &release.date,
                        Text::body(12.0, 400, tokens.text_3),
                    );
                    if response.clicked() {
                        toggle = Some(release.version.clone());
                    }
                    if open {
                        for note in &release.notes {
                            let galley = fonts::paragraph(
                                ui.painter(),
                                note,
                                Text::body(13.0, 400, tokens.text_2),
                                note_width(width),
                                Some(19.0),
                            );
                            let (line, _) = ui.allocate_exact_size(
                                Vec2::new(width, galley.size().y + 8.0),
                                Sense::hover(),
                            );
                            draw::dot(
                                ui.painter(),
                                Pos2::new(line.min.x + 18.0 + 2.5, line.min.y + 9.5),
                                5.0,
                                tokens.primary,
                            );
                            ui.painter().galley(
                                Pos2::new(line.min.x + 18.0 + 15.0, line.min.y),
                                galley,
                                tokens.text_2,
                            );
                        }
                        ui.allocate_exact_size(Vec2::new(width, 8.0), Sense::hover());
                    }
                    let (rule, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
                    draw::hrule(
                        ui.painter(),
                        rule.min.x,
                        rule.max.x,
                        rule.min.y,
                        tokens.border,
                    );
                }
            });
    });
    if let Some(version) = toggle {
        app.about_open = if app.about_open.as_deref() == Some(version.as_str()) {
            None
        } else {
            Some(version)
        };
    }
    ui.painter().rect_stroke(
        card,
        CornerRadius::same(12),
        egui::Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
}
