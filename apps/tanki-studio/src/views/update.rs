// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The update dialog: a newer release with what is new in it (Update and restart, Later, Skip
//! this version), the download and installation with its progress, and the outcome of Check for
//! updates (checking, up to date, or why the check failed).

use std::time::Duration;

use eframe::egui::{Context, CornerRadius, Pos2, Rect, ScrollArea, Sense, Ui, Vec2};
use tanki_core::update::{
    NoteLine, Release, UpdateProgress, UpdateStage,
    install::{Install, Manual},
};
use tanki_i18n::{fmt, tl};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ModalEvent, ModalSpec},
};

use crate::{
    app::Studio,
    model::format_bytes,
    updates::{UpdateView, running_version},
    views::dialogs::paragraph,
};

/// Wide enough for the three buttons of an offer in Russian.
const WIDTH: f32 = 560.0;
const NOTES_HEIGHT: f32 = 176.0;
/// The text width inside a message box: the dialog's, less the box's icon and padding.
const NOTE_TEXT: f32 = WIDTH - 48.0 - (14.0 + 18.0 + 10.0) - 14.0;

pub fn draw(app: &mut Studio, ctx: &Context) {
    // A dialog already open (a replacement being confirmed) goes first; the offer waits for it.
    if app.dialog.is_some() || !app.splash_done {
        return;
    }
    let Some(view) = app.updates.view.clone() else {
        return;
    };
    match view {
        UpdateView::Checking => checking(app, ctx),
        UpdateView::UpToDate => up_to_date(app, ctx),
        UpdateView::Failed(error) => failed(app, ctx, &error),
        UpdateView::Offer { release, error } => offer(app, ctx, &release, error.as_deref()),
        UpdateView::Installing { release, progress } => installing(app, ctx, &release, &progress),
    }
}

/// The reason this copy is updated from the release page, or `None` when it replaces itself.
fn manual_reason(install: &Install) -> Option<&'static str> {
    match install {
        Install::MacApp(_) | Install::Msi(_) => None,
        Install::Manual(Manual::Portable) => Some(tl!(
            "This copy was unzipped by hand. Download the new version and unzip it over this one."
        )),
        Install::Manual(Manual::NotWritable) => Some(tl!(
            "Move Tanki Skin Studio to Applications to update it from here. Until then, download the new version."
        )),
        Install::Manual(Manual::Development) => Some(tl!(
            "This copy is built from source. Download the new version or build it from the new source."
        )),
        Install::Manual(Manual::Unsupported) => {
            Some(tl!("Download the new version from its page."))
        }
    }
}

/// The height of a message box holding `text`: its wrapped lines and the padding.
fn note_height(ctx: &Context, text: &str) -> f32 {
    let painter = ctx.layer_painter(eframe::egui::LayerId::background());
    let galley = fonts::paragraph(
        &painter,
        text,
        Text::body(13.0, 400, eframe::egui::Color32::WHITE),
        NOTE_TEXT,
        Some(19.0),
    );
    galley.size().y.max(19.0) + 24.0
}

/// A rounded box with an icon and the wrapped `text`, as high as [`note_height`] says.
fn note_box(
    ui: &mut Ui,
    icon: char,
    text: &str,
    ink: eframe::egui::Color32,
    fill: eframe::egui::Color32,
) {
    let height = note_height(ui.ctx(), text);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    draw::rounded(ui.painter(), rect, CornerRadius::same(10), fill, None);
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 14.0 + 9.0, rect.min.y + 12.0 + 9.0),
        icon,
        18.0,
        ink,
    );
    let left = rect.min.x + 14.0 + 18.0 + 10.0;
    let galley = fonts::paragraph(
        ui.painter(),
        text,
        Text::body(13.0, 400, ink),
        rect.max.x - 14.0 - left,
        Some(19.0),
    );
    ui.painter()
        .galley(Pos2::new(left, rect.min.y + 12.0), galley, ink);
}

/// A round badge with an icon, centred (the outcome of a check).
fn badge(ui: &mut Ui, icon: char, ink: eframe::egui::Color32, fill: eframe::egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 64.0), Sense::hover());
    let circle = Rect::from_center_size(rect.center(), Vec2::splat(64.0));
    draw::rounded(ui.painter(), circle, CornerRadius::same(32), fill, None);
    fonts::icon(ui.painter(), circle.center(), icon, 32.0, ink);
}

/// Text centred on one line.
fn centred(ui: &mut Ui, text: &str, style: Text) {
    let width = ui.available_width();
    let galley = fonts::paragraph_centered(ui.painter(), text, style, width, Some(21.0));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 21.0), Sense::hover());
    ui.painter()
        .galley(Pos2::new(rect.center().x, rect.min.y), galley, style.color);
}

/// "Check for updates when the app starts" with its switch.
fn check_at_start_switch(app: &mut Studio, ui: &mut Ui) {
    let tokens = Tokens::get(ui.ctx());
    let label = tl!("Check for updates when the app starts");
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    fonts::draw_left(
        ui.painter(),
        row.min.x,
        row.center().y,
        label,
        Text::body(13.0, 400, tokens.text_2),
    );
    let switch = Rect::from_min_size(
        Pos2::new(row.max.x - 40.0, row.min.y),
        Vec2::new(40.0, 22.0),
    );
    let mut on = app.updates.check_at_start;
    let changed = ui
        .scope_builder(eframe::egui::UiBuilder::new().max_rect(switch), |ui| {
            widgets::toggle(ui, &mut on, label).changed()
        })
        .inner;
    if changed {
        app.set_update_check_at_start(on);
    }
}

/// A bar of `fraction`, or a segment sweeping the track while the amount is not known.
fn bar(ui: &mut Ui, fraction: Option<f32>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 6.0), Sense::hover());
    match fraction {
        Some(fraction) => widgets::progress_bar(ui, rect, fraction),
        None => {
            let tokens = Tokens::get(ui.ctx());
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(3),
                tokens.surface_3,
                None,
            );
            let time = ui.input(|i| i.time) as f32;
            let width = rect.width() * 0.3;
            let x = rect.min.x + (time * 0.8).fract() * (rect.width() + width) - width;
            let segment = Rect::from_min_max(
                Pos2::new(x.max(rect.min.x), rect.min.y),
                Pos2::new((x + width).min(rect.max.x), rect.max.y),
            );
            if segment.width() > 0.0 {
                draw::rounded(
                    ui.painter(),
                    segment,
                    CornerRadius::same(3),
                    tokens.primary,
                    None,
                );
            }
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

fn checking(app: &mut Studio, ctx: &Context) {
    let tokens = Tokens::get(ctx);
    let spec = ModalSpec {
        id: "update-checking",
        title: tl!("Check for updates"),
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: 21.0 + 14.0 + 6.0 + 20.0,
    };
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(
                ui,
                tl!("Asking GitHub for the newest version…"),
                tokens.text_2,
                14.0,
                21.0,
            );
            ui.add_space(14.0);
            bar(ui, None);
        },
        |ui| {
            close = Button::new(ButtonKind::Ghost, tl!("Close"))
                .show(ui)
                .clicked();
        },
    );
    if close || event == ModalEvent::Dismissed {
        app.later_update();
    }
}

fn up_to_date(app: &mut Studio, ctx: &Context) {
    let tokens = Tokens::get(ctx);
    let spec = ModalSpec {
        id: "update-current",
        title: tl!("You have the newest version"),
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: 64.0 + 16.0 + 21.0 + 20.0 + 22.0 + 20.0,
    };
    let mut done = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            badge(ui, icons::CIRCLE_CHECK, tokens.primary, tokens.primary_soft);
            ui.add_space(16.0);
            let text = fmt(
                tl!("Tanki Skin Studio {version} is the newest version."),
                &[("version", &running_version())],
            );
            centred(ui, &text, Text::body(14.0, 400, tokens.text_2));
            ui.add_space(20.0);
            check_at_start_switch(app, ui);
        },
        |ui| {
            done = Button::new(ButtonKind::Primary, tl!("Done"))
                .icon(icons::CHECK)
                .show(ui)
                .clicked();
        },
    );
    if done || event == ModalEvent::Dismissed {
        app.later_update();
    }
}

fn failed(app: &mut Studio, ctx: &Context, error: &str) {
    let tokens = Tokens::get(ctx);
    let spec = ModalSpec {
        id: "update-failed",
        title: tl!("Could not check for updates"),
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: 21.0 + 14.0 + note_height(ctx, error) + 20.0 + 22.0 + 20.0,
    };
    let mut retry = false;
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(
                ui,
                tl!("Check the connection and try again."),
                tokens.text_2,
                14.0,
                21.0,
            );
            ui.add_space(14.0);
            note_box(
                ui,
                icons::CIRCLE_X,
                error,
                tokens.danger,
                tokens.danger_soft,
            );
            ui.add_space(20.0);
            check_at_start_switch(app, ui);
        },
        |ui| {
            retry = Button::new(ButtonKind::Primary, tl!("Try again"))
                .icon(icons::REFRESH_CW)
                .show(ui)
                .clicked();
            close = Button::new(ButtonKind::Ghost, tl!("Close"))
                .show(ui)
                .clicked();
        },
    );
    if retry {
        app.check_for_updates();
    } else if close || event == ModalEvent::Dismissed {
        app.later_update();
    }
}

/// The notes' "What's new", in a box that scrolls when it is long.
fn notes(ui: &mut Ui, release: &Release) {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width();
    let height = if release.notes.is_empty() {
        52.0
    } else {
        NOTES_HEIGHT
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(10),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    if release.notes.is_empty() {
        fonts::draw_left(
            ui.painter(),
            rect.min.x + 16.0,
            rect.center().y,
            tl!("What is new is listed on the release page."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 16.0,
        rect.min.y + 20.0,
        &tl!("What's new").to_uppercase(),
        Text::body(11.0, 600, tokens.text_3).spaced(0.8),
    );
    let list = Rect::from_min_max(
        Pos2::new(rect.min.x + 16.0, rect.min.y + 36.0),
        Pos2::new(rect.max.x - 8.0, rect.max.y - 10.0),
    );
    ui.scope_builder(eframe::egui::UiBuilder::new().max_rect(list), |ui| {
        ScrollArea::vertical()
            .id_salt("update-notes")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
                let text_width = ui.available_width() - 12.0;
                for line in &release.notes {
                    match line {
                        NoteLine::Heading(title) => {
                            ui.add_space(2.0);
                            let galley = fonts::paragraph(
                                ui.painter(),
                                title,
                                Text::body(13.0, 600, tokens.text),
                                text_width,
                                Some(19.0),
                            );
                            let (row, _) = ui.allocate_exact_size(galley.size(), Sense::hover());
                            ui.painter().galley(row.min, galley, tokens.text);
                        }
                        NoteLine::Item(text) => {
                            let galley = fonts::paragraph(
                                ui.painter(),
                                text,
                                Text::body(13.0, 400, tokens.text_2),
                                text_width - 14.0,
                                Some(19.0),
                            );
                            let size = Vec2::new(text_width, galley.size().y);
                            let (row, _) = ui.allocate_exact_size(size, Sense::hover());
                            ui.painter().circle_filled(
                                Pos2::new(row.min.x + 3.0, row.min.y + 9.5),
                                2.0,
                                tokens.text_3,
                            );
                            ui.painter().galley(
                                Pos2::new(row.min.x + 14.0, row.min.y),
                                galley,
                                tokens.text_2,
                            );
                        }
                    }
                }
            });
    });
}

fn offer(app: &mut Studio, ctx: &Context, release: &Release, error: Option<&str>) {
    let tokens = Tokens::get(ctx);
    let manual = manual_reason(&app.updates.install);
    let notes_height = if release.notes.is_empty() {
        52.0
    } else {
        NOTES_HEIGHT
    };
    let error = error.map(|error| fmt(tl!("The update failed: {error}"), &[("error", error)]));
    let boxes: f32 = manual
        .iter()
        .copied()
        .chain(error.as_deref())
        .map(|text| 14.0 + note_height(ctx, text))
        .sum();
    let spec = ModalSpec {
        id: "update-offer",
        title: tl!("Update available"),
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: 21.0 + 14.0 + notes_height + boxes + 20.0,
    };
    let mut update = false;
    let mut download = false;
    let mut later = false;
    let mut skip = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            let headline = fmt(
                tl!("Tanki Skin Studio {new} is out. You have {current}."),
                &[
                    ("new", &release.version.to_string()),
                    ("current", &running_version()),
                ],
            );
            paragraph(ui, &headline, tokens.text_2, 14.0, 21.0);
            ui.add_space(14.0);
            notes(ui, release);
            if let Some(reason) = manual {
                ui.add_space(14.0);
                note_box(ui, icons::INFO, reason, tokens.info, tokens.info_soft);
            }
            if let Some(error) = &error {
                ui.add_space(14.0);
                note_box(
                    ui,
                    icons::TRIANGLE_ALERT,
                    error,
                    tokens.danger,
                    tokens.danger_soft,
                );
            }
        },
        |ui| {
            if manual.is_some() {
                download = Button::new(ButtonKind::Primary, tl!("Download"))
                    .icon(icons::EXTERNAL_LINK)
                    .show(ui)
                    .clicked();
            } else if error.is_some() {
                update = Button::new(ButtonKind::Primary, tl!("Try again"))
                    .icon(icons::REFRESH_CW)
                    .show(ui)
                    .clicked();
                download = Button::new(ButtonKind::Secondary, tl!("Download"))
                    .icon(icons::EXTERNAL_LINK)
                    .show(ui)
                    .clicked();
            } else {
                update = Button::new(ButtonKind::Primary, tl!("Update and restart"))
                    .icon(icons::DOWNLOAD)
                    .show(ui)
                    .clicked();
            }
            later = Button::new(ButtonKind::Ghost, tl!("Later"))
                .show(ui)
                .clicked();
            if error.is_none() {
                skip = Button::new(ButtonKind::Ghost, tl!("Skip this version"))
                    .show(ui)
                    .clicked();
            }
        },
    );
    if update {
        app.start_update();
    } else if download {
        app.download_update();
    } else if skip {
        app.skip_update();
    } else if later || event == ModalEvent::Dismissed {
        app.later_update();
    }
}

/// What the update is doing now.
fn stage_text(progress: &UpdateProgress) -> &'static str {
    if progress.is_cancelled() {
        return tl!("Cancelling…");
    }
    match progress.stage() {
        UpdateStage::Downloading => tl!("Downloading…"),
        UpdateStage::Verifying => tl!("Checking the download…"),
        UpdateStage::Installing => tl!("Installing…"),
    }
}

fn installing(app: &mut Studio, ctx: &Context, release: &Release, progress: &UpdateProgress) {
    let tokens = Tokens::get(ctx);
    let title = fmt(
        tl!("Updating to {version}"),
        &[("version", &release.version.to_string())],
    );
    let note = tl!("Tanki Skin Studio closes and opens the new version by itself.");
    let spec = ModalSpec {
        id: "update-installing",
        title: &title,
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: 21.0 + 10.0 + 6.0 + 10.0 + 18.0 + 14.0 + note_height(ctx, note) + 20.0,
    };
    let downloading = progress.stage() == UpdateStage::Downloading;
    let fraction = progress.fraction().filter(|_| downloading);
    let mut cancel = false;
    // Esc, × and a click outside do nothing: the update finishes or is cancelled.
    widgets::modal(
        ctx,
        &spec,
        |ui| {
            let (row, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 21.0), Sense::hover());
            fonts::draw_left(
                ui.painter(),
                row.min.x,
                row.center().y,
                stage_text(progress),
                Text::body(14.0, 400, tokens.text_2),
            );
            if let Some(fraction) = fraction {
                fonts::draw_right(
                    ui.painter(),
                    row.max.x,
                    row.center().y,
                    &format!("{}%", (fraction * 100.0).floor() as u32),
                    Text::mono(13.0, 400, tokens.text),
                );
            }
            ui.add_space(10.0);
            bar(ui, fraction);
            ui.add_space(10.0);
            let (row, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
            let (done, total) = progress.bytes();
            if downloading && total > 0 {
                let text = fmt(
                    tl!("{done} of {total}"),
                    &[
                        ("done", &format_bytes(done)),
                        ("total", &format_bytes(total)),
                    ],
                );
                fonts::draw_left(
                    ui.painter(),
                    row.min.x,
                    row.center().y,
                    &text,
                    Text::body(12.0, 400, tokens.text_3),
                );
            }
            ui.add_space(14.0);
            note_box(ui, icons::INFO, note, tokens.text_2, tokens.surface);
        },
        |ui| {
            let can_cancel =
                progress.stage() != UpdateStage::Installing && !progress.is_cancelled();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .enabled(can_cancel)
                .show(ui)
                .clicked();
        },
    );
    if cancel {
        app.cancel_update();
    }
}
