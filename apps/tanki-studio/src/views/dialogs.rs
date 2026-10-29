//! Modal dialogs: confirm a replacement, the game-is-running warning, the result of a
//! replacement, and the list of problems of a refused pack.

use eframe::egui::{CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use tanki_core::pack::PackIssueKind;
use tanki_i18n::{fmt, tl, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ModalEvent, ModalSpec},
};

use crate::{
    app::{Dialog, PendingApply, Studio},
    model::format_bytes,
};

fn issue_text(kind: PackIssueKind) -> &'static str {
    match kind {
        PackIssueKind::Missing => tl!("Missing file"),
        PackIssueKind::InvalidPath => tl!("Invalid path"),
        PackIssueKind::DuplicateEntry => tl!("Duplicate entry"),
        PackIssueKind::TooLarge => tl!("File too large"),
        PackIssueKind::ChecksumMismatch => tl!("Checksum mismatch"),
        PackIssueKind::InvalidManifest => tl!("Invalid manifest"),
    }
}

pub fn draw(app: &mut Studio, ctx: &eframe::egui::Context) {
    let Some(dialog) = app.dialog.clone() else {
        return;
    };
    match dialog {
        Dialog::Confirm(pending) => confirm(app, ctx, pending),
        Dialog::GameRunning(pending, processes) => running(app, ctx, pending, &processes),
        Dialog::Done { target, files } => done(app, ctx, &target, files),
        Dialog::ImportError {
            name,
            issues,
            message,
        } => import_error(app, ctx, &name, &issues, message.as_deref()),
    }
}

/// Wrapped body text; returns its height.
fn paragraph(
    ui: &mut Ui,
    text: &str,
    color: eframe::egui::Color32,
    size: f32,
    line_height: f32,
) -> f32 {
    let width = ui.available_width();
    let galley = fonts::paragraph(
        ui.painter(),
        text,
        Text::body(size, 400, color),
        width,
        Some(line_height),
    );
    let height = galley.size().y;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter().galley(rect.min, galley, color);
    height
}

fn package_name(app: &Studio) -> String {
    app.active_pack
        .as_ref()
        .map_or_else(String::new, |p| p.manifest.name.clone())
}

fn confirm(app: &mut Studio, ctx: &eframe::egui::Context, pending: PendingApply) {
    let tokens = Tokens::get(ctx);
    let title = fmt(
        tl!("Apply {pack} to {target}?"),
        &[("pack", &package_name(app)), ("target", &pending.target)],
    );
    let files_height = 37.0 * pending.files.len() as f32;
    let spec = ModalSpec {
        id: "confirm",
        title: &title,
        close_label: tl!("Close"),
        width: 520.0,
        body_height: 42.0 + 14.0 + files_height + 14.0 + 38.0 + 20.0,
    };
    let mut apply = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(
                ui,
                tl!(
                    "Files in the game cache will be replaced. Originals are saved in a backup and can be restored."
                ),
                tokens.text_2,
                14.0,
                21.0,
            );
            ui.add_space(14.0);
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), files_height),
                Sense::hover(),
            );
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(10),
                tokens.surface,
                Some((1.0, tokens.border)),
            );
            for (index, (name, size)) in pending.files.iter().enumerate() {
                let line = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.min.y + index as f32 * 37.0),
                    Vec2::new(rect.width(), 37.0),
                );
                widgets::file_line(
                    ui,
                    line,
                    icons::FILE_IMAGE,
                    tokens.text_2,
                    name,
                    &format_bytes(*size),
                    true,
                );
            }
            ui.add_space(14.0);
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(10),
                tokens.accent_soft,
                None,
            );
            fonts::icon(
                ui.painter(),
                Pos2::new(rect.min.x + 14.0 + 9.0, rect.center().y),
                icons::TRIANGLE_ALERT,
                18.0,
                tokens.accent,
            );
            fonts::draw_left(
                ui.painter(),
                rect.min.x + 14.0 + 18.0 + 10.0,
                rect.center().y,
                tl!("Close the game before replacing and restart it afterwards."),
                Text::body(13.0, 400, tokens.accent),
            );
        },
        |ui| {
            apply = Button::new(ButtonKind::Accent, tl!("Apply"))
                .icon(icons::ZAP)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if apply {
        app.confirm_apply(pending);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

fn running(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    pending: PendingApply,
    processes: &[String],
) {
    let tokens = Tokens::get(ctx);
    let spec = ModalSpec {
        id: "running",
        title: tl!("Game is running"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: 74.0 + 16.0 + 40.0 + 20.0,
    };
    let mut recheck = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 74.0), Sense::hover());
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(12),
                tokens.accent_soft,
                None,
            );
            fonts::icon(
                ui.painter(),
                Pos2::new(rect.min.x + 16.0 + 14.0, rect.center().y),
                icons::TRIANGLE_ALERT,
                28.0,
                tokens.accent,
            );
            let text = if processes.is_empty() {
                tl!("The game is running. Replacing files while it runs can corrupt the cache.")
                    .to_owned()
            } else {
                fmt(
                    tl!(
                        "The game is running ({names}). Replacing files while it runs can corrupt the cache."
                    ),
                    &[("names", &processes.join(", "))],
                )
            };
            let galley = fonts::paragraph(
                ui.painter(),
                &text,
                Text::body(14.0, 400, tokens.text),
                rect.width() - 16.0 - 28.0 - 14.0 - 16.0,
                Some(21.0),
            );
            let y = rect.center().y - galley.size().y / 2.0;
            ui.painter().galley(
                Pos2::new(rect.min.x + 16.0 + 28.0 + 14.0, y),
                galley,
                tokens.text,
            );
            ui.add_space(16.0);
            paragraph(
                ui,
                tl!(
                    "Close the game and press “Check again”. Restart the game after the replacement."
                ),
                tokens.text_2,
                13.0,
                20.0,
            );
        },
        |ui| {
            recheck = Button::new(ButtonKind::Accent, tl!("Check again"))
                .icon(icons::REFRESH_CW)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if recheck {
        app.dialog = None;
        app.begin_apply(pending.item, pending.variant);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

fn done(app: &mut Studio, ctx: &eframe::egui::Context, target: &str, files: usize) {
    let tokens = Tokens::get(ctx);
    let spec = ModalSpec {
        id: "done",
        title: tl!("Replacement complete"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: 72.0 + 16.0 + 21.0 + 20.0,
    };
    let mut restore = false;
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            let width = ui.available_width();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 72.0), Sense::hover());
            let badge = Rect::from_center_size(rect.center(), Vec2::splat(72.0));
            draw::rounded(
                ui.painter(),
                badge,
                CornerRadius::same(36),
                tokens.primary_soft,
                None,
            );
            fonts::icon(
                ui.painter(),
                badge.center(),
                icons::CIRCLE_CHECK,
                38.0,
                tokens.primary,
            );
            ui.add_space(16.0);
            let text = fmt(
                tl!("{files} replaced for {target}. Restart the game to see the skin."),
                &[
                    ("files", &tn(files as u64, "{n} file", "{n} files")),
                    ("target", target),
                ],
            );
            let galley = fonts::paragraph_centered(
                ui.painter(),
                &text,
                Text::body(14.0, 400, tokens.text_2),
                width,
                Some(21.0),
            );
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 21.0), Sense::hover());
            ui.painter().galley(
                Pos2::new(rect.center().x, rect.min.y),
                galley,
                tokens.text_2,
            );
        },
        |ui| {
            close = Button::new(ButtonKind::Primary, tl!("Done"))
                .icon(icons::CHECK)
                .show(ui)
                .clicked();
            restore = Button::new(ButtonKind::Ghost, tl!("Restore"))
                .icon(icons::ROTATE_CCW)
                .show(ui)
                .clicked();
        },
    );
    if restore {
        app.restore_last();
    } else if close || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

fn import_error(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    name: &str,
    issues: &[tanki_core::pack::PackIssue],
    message: Option<&str>,
) {
    let tokens = Tokens::get(ctx);
    let lines = issues.len().max(1);
    let list_height = 36.0 * lines as f32;
    let spec = ModalSpec {
        id: "import-error",
        title: tl!("Could not import pack"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: 42.0 + 14.0 + 24.0 + 14.0 + list_height + 20.0,
    };
    let mut other = false;
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            let text = fmt(
                tl!(
                    "Pack “{name}” failed validation. Nothing was written to the library and the game cache is unchanged."
                ),
                &[("name", name)],
            );
            paragraph(ui, &text, tokens.text_2, 14.0, 21.0);
            ui.add_space(14.0);
            let count = if issues.is_empty() {
                tl!("Validation failed").to_owned()
            } else {
                tn(
                    issues.len() as u64,
                    "{n} validation error",
                    "{n} validation errors",
                )
            };
            let width = widgets::chip_width(ui, &count);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
            widgets::paint_chip(ui, rect, widgets::ChipKind::Danger, &count);
            ui.add_space(14.0);
            let (rect, _) = ui
                .allocate_exact_size(Vec2::new(ui.available_width(), list_height), Sense::hover());
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(10),
                tokens.surface,
                Some((1.0, tokens.border)),
            );
            if issues.is_empty() {
                let line = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 36.0));
                widgets::file_line(
                    ui,
                    line,
                    icons::CIRCLE_X,
                    tokens.danger,
                    message.unwrap_or(""),
                    "",
                    false,
                );
            }
            for (index, issue) in issues.iter().enumerate() {
                let line = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.min.y + index as f32 * 36.0),
                    Vec2::new(rect.width(), 36.0),
                );
                widgets::file_line(
                    ui,
                    line,
                    icons::CIRCLE_X,
                    tokens.danger,
                    issue_text(issue.kind),
                    &issue.file,
                    false,
                );
            }
        },
        |ui| {
            other = Button::new(ButtonKind::Secondary, tl!("Choose another file"))
                .icon(icons::FOLDER_OPEN)
                .show(ui)
                .clicked();
            close = Button::new(ButtonKind::Ghost, tl!("Close"))
                .show(ui)
                .clicked();
        },
    );
    if other {
        app.dialog = None;
        app.import(false);
    } else if close || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}
