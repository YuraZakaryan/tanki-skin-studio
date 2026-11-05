// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Modal dialogs: confirm a replacement, the game-is-running warning, the result of a
//! replacement, the list of problems of a refused pack, and which game cache to use.

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
    app::{CacheChoice, DeleteWhat, Dialog, PendingApply, Retry, Studio},
    model::{format_bytes, home_path, item_name},
};
use tanki_core::backups::RoomPlan;

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
        Dialog::GameRunning(pending, processes) => running(app, ctx, Some(pending), &processes),
        Dialog::BulkGameRunning(processes) => running(app, ctx, None, &processes),
        Dialog::CloseBulk => close_bulk(app, ctx),
        Dialog::RestoreBatch {
            batch,
            name,
            items,
            files,
        } => restore_batch(app, ctx, &batch, &name, &items, files),
        Dialog::Done { target, files } => done(app, ctx, &target, files),
        Dialog::ImportError {
            name,
            issues,
            message,
        } => import_error(app, ctx, &name, &issues, message.as_deref()),
        Dialog::DeletePack { id, version, name } => delete_pack(app, ctx, &id, &version, &name),
        Dialog::RestoreAll {
            items,
            replacements,
            files,
        } => restore_all(app, ctx, &items, replacements, files),
        Dialog::ChooseCache {
            choices,
            selected,
            switching,
        } => choose_cache(app, ctx, &choices, selected, switching),
        Dialog::LowSpace {
            retry,
            need,
            free,
            plan,
        } => low_space(app, ctx, retry, need, free, plan),
        Dialog::DeleteOld { backups, bytes } => delete_old(app, ctx, backups, bytes),
        Dialog::DeleteBackups {
            what,
            dirs,
            backups,
            bytes,
            worn,
        } => delete_backups(app, ctx, &what, &dirs, backups, bytes, &worn),
        Dialog::Preview3d => super::preview3d::dialog(app, ctx),
    }
}

/// Wrapped body text; returns its height.
pub(crate) fn paragraph(
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

/// The inner width of a 520-wide dialog's body.
const BODY_WIDTH: f32 = 520.0 - 48.0;

/// The height `text` takes wrapped to `width`: a dialog is sized to its text, so a longer
/// translation adds a line instead of running into the buttons.
pub(crate) fn text_height(
    ctx: &eframe::egui::Context,
    text: &str,
    size: f32,
    line_height: f32,
    width: f32,
) -> f32 {
    let painter = ctx.layer_painter(eframe::egui::LayerId::background());
    let galley = fonts::paragraph(
        &painter,
        text,
        Text::body(size, 400, eframe::egui::Color32::WHITE),
        width,
        Some(line_height),
    );
    galley.size().y.max(line_height)
}

/// The text room of a [`warning`] box `width` wide: the icon and the paddings take the rest.
fn warning_text_width(width: f32) -> f32 {
    width - 14.0 - 18.0 - 10.0 - 14.0
}

/// The height of a [`warning`] box: 38 for one line, more when the text wraps.
fn warning_height(ctx: &eframe::egui::Context, text: &str, width: f32) -> f32 {
    text_height(ctx, text, 13.0, 19.0, warning_text_width(width)) + 19.0
}

/// An amber box with a warning sign and `text`, wrapped to the box.
fn warning(ui: &mut Ui, text: &str) {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width();
    let height = warning_height(ui.ctx(), text, width);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(10),
        tokens.accent_soft,
        None,
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 14.0 + 9.0, rect.min.y + 19.0),
        icons::TRIANGLE_ALERT,
        18.0,
        tokens.accent,
    );
    let galley = fonts::paragraph(
        ui.painter(),
        text,
        Text::body(13.0, 400, tokens.accent),
        warning_text_width(width),
        Some(19.0),
    );
    ui.painter().galley(
        Pos2::new(rect.min.x + 14.0 + 18.0 + 10.0, rect.min.y + 9.5),
        galley,
        tokens.accent,
    );
}

fn confirm(app: &mut Studio, ctx: &eframe::egui::Context, pending: PendingApply) {
    let tokens = Tokens::get(ctx);
    let title = fmt(
        tl!("Apply {pack} to {target}?"),
        &[("pack", &pending.pack_name), ("target", &pending.target)],
    );
    let files_height = 37.0 * pending.files.len() as f32;
    let intro = tl!(
        "Files in the game cache will be replaced. Originals are saved in a backup and can be restored."
    );
    let note = tl!("Close the game before replacing and restart it afterwards.");
    let spec = ModalSpec {
        id: "confirm",
        title: &title,
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, intro, 14.0, 21.0, BODY_WIDTH)
            + 14.0
            + files_height
            + 14.0
            + warning_height(ctx, note, BODY_WIDTH)
            + 20.0,
    };
    let mut apply = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, intro, tokens.text_2, 14.0, 21.0);
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
            warning(ui, note);
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

/// The game is running: no replacement until it is closed. `pending` is the single replacement
/// asked for; `None` is "Apply to all items".
fn running(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    pending: Option<PendingApply>,
    processes: &[String],
) {
    let tokens = Tokens::get(ctx);
    let text = if processes.is_empty() {
        tl!("The game is running. Replacing files while it runs can corrupt the cache.").to_owned()
    } else {
        fmt(
            tl!(
                "The game is running ({names}). Replacing files while it runs can corrupt the cache."
            ),
            &[("names", &processes.join(", "))],
        )
    };
    let advice =
        tl!("Close the game and press “Check again”. Restart the game after the replacement.");
    // The box is 74 high, or as high as its wrapped text with the same margins.
    let box_text = BODY_WIDTH - 16.0 - 28.0 - 14.0 - 16.0;
    let box_height = (text_height(ctx, &text, 14.0, 21.0, box_text) + 32.0).max(74.0);
    let spec = ModalSpec {
        id: "running",
        title: tl!("Game is running"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: box_height + 16.0 + text_height(ctx, advice, 13.0, 20.0, BODY_WIDTH) + 20.0,
    };
    let mut recheck = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            let (rect, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), box_height), Sense::hover());
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
            let galley = fonts::paragraph(
                ui.painter(),
                &text,
                Text::body(14.0, 400, tokens.text),
                box_text,
                Some(21.0),
            );
            let y = rect.center().y - galley.size().y / 2.0;
            ui.painter().galley(
                Pos2::new(rect.min.x + 16.0 + 28.0 + 14.0, y),
                galley,
                tokens.text,
            );
            ui.add_space(16.0);
            paragraph(ui, advice, tokens.text_2, 13.0, 20.0);
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
        match pending {
            Some(pending) => app.ask_to_confirm(pending),
            None => app.bulk_recheck(),
        }
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

/// "Apply to all items" is closed at its last step: everything chosen is dropped, so it asks.
fn close_bulk(app: &mut Studio, ctx: &eframe::egui::Context) {
    let tokens = Tokens::get(ctx);
    let text = tl!("Nothing has been replaced yet. The choices made for this look are not kept.");
    let spec = ModalSpec {
        id: "close-bulk",
        title: tl!("Close without applying?"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, text, 14.0, 21.0, BODY_WIDTH) + 20.0,
    };
    let mut leave = false;
    let mut stay = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, text, tokens.text_2, 14.0, 21.0);
        },
        |ui| {
            stay = Button::new(ButtonKind::Primary, tl!("Keep choosing"))
                .show(ui)
                .clicked();
            leave = Button::new(ButtonKind::Ghost, tl!("Close without applying"))
                .show(ui)
                .clicked();
        },
    );
    if leave {
        app.confirm_close_bulk();
    } else if stay || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

/// Undo one run of "Apply to all items" from its entry in the backups.
fn restore_batch(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    batch: &str,
    name: &str,
    items: &[String],
    files: usize,
) {
    let tokens = Tokens::get(ctx);
    let title = fmt(tl!("Undo “{look}” on every item?"), &[("look", name)]);
    let intro = fmt(
        tl!(
            "{items} get back what they wore before, {files} in all. A skin put on one of them later is undone too."
        ),
        &[
            ("items", &tn(items.len() as u64, "{n} item", "{n} items")),
            ("files", &tn(files as u64, "{n} file", "{n} files")),
        ],
    );
    let list = items.join(", ");
    let note = tl!("Close the game before restoring.");
    let spec = ModalSpec {
        id: "restore-batch",
        title: &title,
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, &intro, 14.0, 21.0, BODY_WIDTH)
            + 10.0
            + text_height(ctx, &list, 13.0, 19.0, BODY_WIDTH)
            + 14.0
            + warning_height(ctx, note, BODY_WIDTH)
            + 20.0,
    };
    let mut restore = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, &intro, tokens.text_2, 14.0, 21.0);
            ui.add_space(10.0);
            paragraph(ui, &list, tokens.text, 13.0, 19.0);
            ui.add_space(14.0);
            warning(ui, note);
        },
        |ui| {
            restore = Button::new(ButtonKind::Danger, tl!("Undo all"))
                .icon(icons::UNDO_2)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if restore {
        app.restore_batch_confirmed(batch, name);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

fn done(app: &mut Studio, ctx: &eframe::egui::Context, target: &str, files: usize) {
    let tokens = Tokens::get(ctx);
    let text = fmt(
        tl!("{files} replaced for {target}. Restart the game to see the skin."),
        &[
            ("files", &tn(files as u64, "{n} file", "{n} files")),
            ("target", target),
        ],
    );
    let text_height = text_height(ctx, &text, 14.0, 21.0, BODY_WIDTH);
    let spec = ModalSpec {
        id: "done",
        title: tl!("Replacement complete"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: 72.0 + 16.0 + text_height + 20.0,
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
            let galley = fonts::paragraph_centered(
                ui.painter(),
                &text,
                Text::body(14.0, 400, tokens.text_2),
                width,
                Some(21.0),
            );
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, text_height), Sense::hover());
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
        app.ask_to_restore();
    } else if close || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

fn delete_pack(app: &mut Studio, ctx: &eframe::egui::Context, id: &str, version: &str, name: &str) {
    let tokens = Tokens::get(ctx);
    let title = fmt(tl!("Delete pack “{name}”?"), &[("name", name)]);
    let text = tl!(
        "The pack and its files are removed from the library. Skins already applied stay in the game cache and can still be restored."
    );
    let spec = ModalSpec {
        id: "delete-pack",
        title: &title,
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, text, 14.0, 21.0, BODY_WIDTH) + 20.0,
    };
    let mut delete = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, text, tokens.text_2, 14.0, 21.0);
        },
        |ui| {
            delete = Button::new(ButtonKind::Danger, tl!("Delete"))
                .icon(icons::TRASH_2)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if delete {
        app.dialog = None;
        app.delete_pack(id, version);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

/// The most items listed by name; the rest are counted, so the dialog fits the smallest window.
const RESTORE_ROWS: usize = 4;

fn restore_all(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    items: &[String],
    replacements: usize,
    files: usize,
) {
    let tokens = Tokens::get(ctx);
    let shown = items.len().min(RESTORE_ROWS);
    let more = items.len() - shown;
    let rows = shown + usize::from(more > 0);
    let list_height = 37.0 * rows as f32;
    let intro = tl!(
        "Every replacement not restored yet is undone, newest first, and the game cache goes back to how it was."
    );
    let note = tl!("Close the game before restoring.");
    let spec = ModalSpec {
        id: "restore-all",
        title: tl!("Restore the original files?"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, intro, 14.0, 21.0, BODY_WIDTH)
            + 14.0
            + 24.0
            + 14.0
            + list_height
            + 14.0
            + warning_height(ctx, note, BODY_WIDTH)
            + 20.0,
    };
    let mut restore = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, intro, tokens.text_2, 14.0, 21.0);
            ui.add_space(14.0);
            let count = fmt(
                tl!("{replacements} · {files}"),
                &[
                    (
                        "replacements",
                        &tn(replacements as u64, "{n} replacement", "{n} replacements"),
                    ),
                    ("files", &tn(files as u64, "{n} file", "{n} files")),
                ],
            );
            let width = widgets::chip_width(ui, &count);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 24.0), Sense::hover());
            widgets::paint_chip(ui, rect, widgets::ChipKind::Neutral, &count);
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
            for (index, item) in items.iter().take(shown).enumerate() {
                let line = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.min.y + index as f32 * 37.0),
                    Vec2::new(rect.width(), 37.0),
                );
                widgets::file_line(ui, line, icons::ROTATE_CCW, tokens.text_2, item, "", false);
            }
            if more > 0 {
                let line = Rect::from_min_size(
                    Pos2::new(rect.min.x, rect.min.y + shown as f32 * 37.0),
                    Vec2::new(rect.width(), 37.0),
                );
                let text = fmt(tl!("and {n} more"), &[("n", &more.to_string())]);
                widgets::file_line(ui, line, icons::LAYERS, tokens.text_3, &text, "", false);
            }
            ui.add_space(14.0);
            warning(ui, note);
        },
        |ui| {
            restore = Button::new(ButtonKind::Danger, tl!("Restore"))
                .icon(icons::ROTATE_CCW)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if restore {
        app.restore_original();
    } else if cancel || event == ModalEvent::Dismissed {
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
    let text = fmt(
        tl!(
            "Pack “{name}” failed validation. Nothing was written to the library and the game cache is unchanged."
        ),
        &[("name", name)],
    );
    let spec = ModalSpec {
        id: "import-error",
        title: tl!("Could not import pack"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, &text, 14.0, 21.0, BODY_WIDTH)
            + 14.0
            + 24.0
            + 14.0
            + list_height
            + 20.0,
    };
    let mut other = false;
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
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

/// The most caches shown at once; more scroll.
const CACHE_ROWS: usize = 4;

/// Which game cache the app works with. Asked when several were found (`switching` false): the
/// current one is selected at first, and closing the question keeps it. Opened from the cache card
/// (`switching`): a deliberate switch, with Choose folder… for a folder that is not listed, and
/// Cancel. The arrow keys move the selection, Enter takes it, Esc closes.
fn choose_cache(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    choices: &[CacheChoice],
    selected: usize,
    switching: bool,
) {
    let tokens = Tokens::get(ctx);
    let gap = 8.0;
    let shown = choices.len().clamp(1, CACHE_ROWS) as f32;
    let list_height = shown * widgets::CHOICE_ROW_HEIGHT + (shown - 1.0) * gap;
    let (title, intro) = if switching {
        (
            tl!("Switch game cache"),
            tl!(
                "Choose the cache the studio works with. A folder that is not listed can be chosen by hand."
            ),
        )
    } else {
        (
            tl!("Choose the game cache"),
            tl!(
                "Several game clients are installed on this computer, each with its own cache. Choose the one the studio works with."
            ),
        )
    };
    let hint =
        (!switching).then(|| tl!("You can switch later from the cache card in the sidebar."));
    let body = 600.0 - 48.0;
    let hint_height = hint.map_or(0.0, |hint| {
        14.0 + text_height(ctx, hint, 12.0, 18.0, body - 24.0)
    });
    let spec = ModalSpec {
        id: "choose-cache",
        title,
        close_label: tl!("Close"),
        width: 600.0,
        body_height: text_height(ctx, intro, 14.0, 21.0, body)
            + 16.0
            + list_height
            + hint_height
            + 20.0,
    };
    let mut picked = selected;
    let (down, up, enter) = ctx.input(|i| {
        (
            i.key_pressed(eframe::egui::Key::ArrowDown),
            i.key_pressed(eframe::egui::Key::ArrowUp),
            i.key_pressed(eframe::egui::Key::Enter),
        )
    });
    if down {
        picked = (picked + 1).min(choices.len().saturating_sub(1));
    }
    if up {
        picked = picked.saturating_sub(1);
    }
    let mut chosen_twice = false;
    let mut confirm = enter && !choices.is_empty();
    let mut folder = false;
    let mut close = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, intro, tokens.text_2, 14.0, 21.0);
            ui.add_space(16.0);
            if choices.is_empty() {
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), list_height),
                    Sense::hover(),
                );
                draw::rounded(
                    ui.painter(),
                    rect,
                    CornerRadius::same(12),
                    tokens.surface,
                    Some((1.0, tokens.border)),
                );
                fonts::draw_centered(
                    ui.painter(),
                    rect,
                    tl!("No game cache was found on this computer."),
                    Text::body(13.0, 400, tokens.text_3),
                );
            }
            eframe::egui::ScrollArea::vertical()
                .max_height(list_height)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = gap;
                    for (index, choice) in choices.iter().enumerate() {
                        let mut chips = Vec::new();
                        if choice.new {
                            chips.push((widgets::ChipKind::Info, tl!("New")));
                        }
                        if choice.current {
                            chips.push((widgets::ChipKind::Success, tl!("In use")));
                        }
                        let note = choice.modified.map_or_else(String::new, |unix| {
                            fmt(
                                tl!("changed {time}"),
                                &[("time", &super::shell::describe_time(unix))],
                            )
                        });
                        let path = home_path(&choice.path);
                        let full = choice.path.display().to_string();
                        let row = widgets::ChoiceRow {
                            name: &choice.name,
                            detail: &path,
                            tooltip: &full,
                            note: &note,
                            chips: &chips,
                            selected: index == picked,
                        };
                        let response = row.show(ui);
                        if response.clicked() {
                            picked = index;
                        }
                        if response.double_clicked() {
                            chosen_twice = true;
                        }
                    }
                });
            if let Some(hint) = hint {
                ui.add_space(14.0);
                let width = ui.available_width();
                let galley = fonts::paragraph(
                    ui.painter(),
                    hint,
                    Text::body(12.0, 400, tokens.text_3),
                    width - 24.0,
                    Some(18.0),
                );
                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(width, galley.size().y.max(18.0)),
                    Sense::hover(),
                );
                fonts::icon(
                    ui.painter(),
                    Pos2::new(rect.min.x + 8.0, rect.min.y + 9.0),
                    icons::INFO,
                    14.0,
                    tokens.text_3,
                );
                ui.painter().galley(
                    Pos2::new(rect.min.x + 24.0, rect.min.y),
                    galley,
                    tokens.text_3,
                );
            }
        },
        |ui| {
            if !choices.is_empty() {
                confirm |= Button::new(ButtonKind::Primary, tl!("Use this cache"))
                    .icon(icons::CHECK)
                    .show(ui)
                    .clicked();
            }
            if switching {
                folder = Button::new(ButtonKind::Secondary, tl!("Choose folder…"))
                    .icon(icons::FOLDER_OPEN)
                    .show(ui)
                    .clicked();
                close = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                    .show(ui)
                    .clicked();
            } else {
                close = Button::new(ButtonKind::Ghost, tl!("Keep current"))
                    .show(ui)
                    .clicked();
            }
        },
    );
    if picked != selected
        && let Some(Dialog::ChooseCache { selected, .. }) = &mut app.dialog
    {
        *selected = picked;
    }
    if (confirm || chosen_twice)
        && let Some(choice) = choices.get(picked)
    {
        app.pick_cache(choice);
    } else if folder {
        app.choose_cache_folder_instead();
    } else if close || event == ModalEvent::Dismissed {
        app.keep_cache();
    }
}

/// A box of soft colour with an icon and wrapped text (what stays, in the delete dialogs).
fn note_box(ui: &mut Ui, icon: char, text: &str) {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width();
    let height = warning_height(ui.ctx(), text, width);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(10),
        tokens.primary_soft,
        None,
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 14.0 + 9.0, rect.min.y + 19.0),
        icon,
        16.0,
        tokens.primary,
    );
    let galley = fonts::paragraph(
        ui.painter(),
        text,
        Text::body(13.0, 400, tokens.text),
        warning_text_width(width),
        Some(19.0),
    );
    ui.painter().galley(
        Pos2::new(rect.min.x + 14.0 + 18.0 + 10.0, rect.min.y + 9.5),
        galley,
        tokens.text,
    );
}

fn delete_old(app: &mut Studio, ctx: &eframe::egui::Context, backups: usize, bytes: u64) {
    let tokens = Tokens::get(ctx);
    let text = fmt(
        tl!(
            "{backups} · {size}. They are copies of skins you already took off or that a game update removed. Nothing changes in the game."
        ),
        &[
            ("size", &format_bytes(bytes)),
            ("backups", &tn(backups as u64, "{n} backup", "{n} backups")),
        ],
    );
    let keep = tl!("Skins on now keep their backups and can still be taken off.");
    let spec = ModalSpec {
        id: "delete-old-backups",
        title: tl!("Delete old backups?"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, &text, 14.0, 21.0, BODY_WIDTH)
            + 14.0
            + warning_height(ctx, keep, BODY_WIDTH)
            + 20.0,
    };
    let mut delete = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, &text, tokens.text_2, 14.0, 21.0);
            ui.add_space(14.0);
            note_box(ui, icons::SHIELD_CHECK, keep);
        },
        |ui| {
            delete = Button::new(ButtonKind::Danger, tl!("Delete old backups"))
                .icon(icons::TRASH_2)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if delete {
        app.delete_old_confirmed();
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

/// The skins a plan's needed backups belong to, a few by name.
fn plan_skins(plan: &RoomPlan) -> String {
    let mut names: Vec<String> = Vec::new();
    for entry in &plan.needed {
        let name = entry.tx.target.as_ref().map_or_else(
            || entry.tx.variant_name.clone(),
            |t| item_name(&t.kind, &t.id, &t.grade),
        );
        if !names.contains(&name) {
            names.push(name);
        }
    }
    let shown = names.len().min(3);
    let mut text = names[..shown].join(", ");
    if names.len() > shown {
        text = fmt(
            tl!("{names} and {n} more"),
            &[("names", &text), ("n", &(names.len() - shown).to_string())],
        );
    }
    text
}

/// One thing that would go to make room: an icon in its box, a title, a line, the size.
fn room_line(ui: &mut Ui, safe: bool, title: &str, line: &str, bytes: u64) {
    let tokens = Tokens::get(ui.ctx());
    let width = ui.available_width();
    let text_width = width - 14.0 - 32.0 - 12.0 - 70.0 - 14.0;
    let galley = fonts::paragraph(
        ui.painter(),
        line,
        Text::body(12.0, 400, if safe { tokens.text_3 } else { tokens.accent }),
        text_width,
        Some(17.0),
    );
    let height = (12.0 + 19.0 + galley.size().y + 12.0).max(56.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let (fill, ink, icon) = if safe {
        (tokens.primary_soft, tokens.primary, icons::ARCHIVE_X)
    } else {
        (tokens.accent_soft, tokens.accent, icons::TRIANGLE_ALERT)
    };
    let icon_box = Rect::from_min_size(
        Pos2::new(rect.min.x + 14.0, rect.center().y - 16.0),
        Vec2::splat(32.0),
    );
    draw::rounded(ui.painter(), icon_box, CornerRadius::same(8), fill, None);
    fonts::icon(ui.painter(), icon_box.center(), icon, 15.0, ink);
    let x = icon_box.max.x + 12.0;
    let top = rect.center().y - (19.0 + galley.size().y) / 2.0;
    fonts::draw(
        ui.painter(),
        Pos2::new(x, top),
        title,
        Text::body(13.0, 600, tokens.text),
    );
    let color = if safe { tokens.text_3 } else { tokens.accent };
    ui.painter().galley(Pos2::new(x, top + 19.0), galley, color);
    fonts::draw_right(
        ui.painter(),
        rect.max.x - 14.0,
        rect.center().y,
        &format_bytes(bytes),
        Text::mono(13.0, 400, tokens.text_2),
    );
}

fn low_space(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    retry: Retry,
    need: u64,
    free: u64,
    plan: RoomPlan,
) {
    let tokens = Tokens::get(ctx);
    let text = fmt(
        tl!("The backup needs {need}, and the disk has {free} free."),
        &[("need", &format_bytes(need)), ("free", &format_bytes(free))],
    );
    let skins = plan_skins(&plan);
    let old_title = fmt(
        tl!("Old backups · {n}"),
        &[("n", &plan.old.len().to_string())],
    );
    let needed_title = fmt(
        tl!("Oldest backups of skins on now · {n}"),
        &[("n", &plan.needed.len().to_string())],
    );
    let needed_line = fmt(
        tl!("{skins}: can no longer be taken off"),
        &[("skins", &skins)],
    );
    let hint = if plan.enough {
        tl!("Or free up disk space and try again.")
    } else {
        tl!("Even deleting every backup is not enough. Free up disk space and try again.")
    };
    let lines = usize::from(!plan.old.is_empty()) + usize::from(!plan.needed.is_empty());
    let list_height = if lines == 0 {
        0.0
    } else {
        let needed = if plan.needed.is_empty() {
            0.0
        } else {
            (12.0 + 19.0 + text_height(ctx, &needed_line, 12.0, 17.0, BODY_WIDTH - 142.0) + 12.0)
                .max(56.0)
        };
        let old = if plan.old.is_empty() { 0.0 } else { 56.0 };
        old + needed + 14.0
    };
    let spec = ModalSpec {
        id: "low-space",
        title: tl!("Not enough disk space"),
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, &text, 14.0, 21.0, BODY_WIDTH)
            + 14.0
            + list_height
            + text_height(ctx, hint, 12.0, 17.0, BODY_WIDTH)
            + 20.0,
    };
    let mut go = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, &text, tokens.text_2, 14.0, 21.0);
            ui.add_space(14.0);
            if lines > 0 {
                let top = ui.cursor().min;
                if !plan.old.is_empty() {
                    room_line(ui, true, &old_title, tl!("Safe"), plan.old_bytes);
                }
                if !plan.needed.is_empty() {
                    room_line(ui, false, &needed_title, &needed_line, plan.needed_bytes);
                }
                let frame =
                    Rect::from_min_max(top, Pos2::new(top.x + BODY_WIDTH, ui.cursor().min.y));
                ui.painter().rect_stroke(
                    frame,
                    CornerRadius::same(10),
                    eframe::egui::Stroke::new(1.0, tokens.border),
                    eframe::egui::StrokeKind::Inside,
                );
                ui.add_space(14.0);
            }
            paragraph(ui, hint, tokens.text_3, 12.0, 17.0);
        },
        |ui| {
            if plan.enough {
                let kind = if plan.needed.is_empty() {
                    ButtonKind::Primary
                } else {
                    ButtonKind::Danger
                };
                go = Button::new(kind, tl!("Delete and put on"))
                    .icon(icons::TRASH_2)
                    .show(ui)
                    .clicked();
            }
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if go {
        app.make_room_and_retry(retry, plan);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

/// "A, B and C", or "A, B, C and 4 more" past three.
fn names(list: &[String]) -> String {
    match list {
        [] => String::new(),
        [one] => one.clone(),
        _ if list.len() <= 3 => fmt(
            tl!("{names} and {last}"),
            &[
                ("names", &list[..list.len() - 1].join(", ")),
                ("last", &list[list.len() - 1]),
            ],
        ),
        _ => fmt(
            tl!("{names} and {n} more"),
            &[
                ("names", &list[..3].join(", ")),
                ("n", &(list.len() - 3).to_string()),
            ],
        ),
    }
}

/// Every delete of the Backups page asks first: one backup, a day's or all of them.
fn delete_backups(
    app: &mut Studio,
    ctx: &eframe::egui::Context,
    what: &DeleteWhat,
    dirs: &[std::path::PathBuf],
    backups: usize,
    bytes: u64,
    worn: &[crate::app::Worn],
) {
    let tokens = Tokens::get(ctx);
    let (title, button, note) = match what {
        DeleteWhat::One { old, .. } => (
            tl!("Delete this backup?").to_owned(),
            tl!("Delete"),
            if *old {
                tl!("It is an old backup: nothing changes in the game.")
            } else {
                ""
            },
        ),
        DeleteWhat::Day(day) => (
            match day {
                crate::model::Day::Today => tl!("Delete today's backups?").to_owned(),
                crate::model::Day::Yesterday => tl!("Delete yesterday's backups?").to_owned(),
                crate::model::Day::Date(date) => {
                    fmt(tl!("Delete the backups of {date}?"), &[("date", date)])
                }
            },
            tl!("Delete"),
            tl!("Old backups among them change nothing in the game."),
        ),
        DeleteWhat::All => (
            tl!("Delete all backups?").to_owned(),
            tl!("Delete all"),
            tl!("To keep a way back, use Restore original files first."),
        ),
    };
    let body = match what {
        DeleteWhat::One { title, .. } => format!("{title} · {}.", format_bytes(bytes)),
        _ => format!(
            "{} · {}.",
            tn(backups as u64, "{n} backup", "{n} backups"),
            format_bytes(bytes)
        ),
    };
    let warn = (!worn.is_empty()).then(|| {
        fmt(
            tl!("{skins} are on now: they can no longer be taken off."),
            &[(
                "skins",
                &names(&worn.iter().map(|w| w.label()).collect::<Vec<_>>()),
            )],
        )
    });
    let note = if warn.is_none() && matches!(what, DeleteWhat::All) {
        tl!("Old backups change nothing in the game.")
    } else {
        note
    };
    let spec = ModalSpec {
        id: "delete-backups",
        title: &title,
        close_label: tl!("Close"),
        width: 520.0,
        body_height: text_height(ctx, &body, 14.0, 21.0, BODY_WIDTH)
            + warn
                .as_ref()
                .map_or(0.0, |w| 14.0 + warning_height(ctx, w, BODY_WIDTH))
            + if note.is_empty() {
                0.0
            } else {
                14.0 + text_height(ctx, note, 12.0, 17.0, BODY_WIDTH)
            }
            + 20.0,
    };
    let mut delete = false;
    let mut cancel = false;
    let event = widgets::modal(
        ctx,
        &spec,
        |ui| {
            paragraph(ui, &body, tokens.text_2, 14.0, 21.0);
            if let Some(warn) = &warn {
                ui.add_space(14.0);
                warning(ui, warn);
            }
            if !note.is_empty() {
                ui.add_space(14.0);
                paragraph(ui, note, tokens.text_3, 12.0, 17.0);
            }
        },
        |ui| {
            delete = Button::new(ButtonKind::Danger, button)
                .icon(icons::TRASH_2)
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if delete {
        app.delete_backups_confirmed(dirs);
    } else if cancel || event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}
