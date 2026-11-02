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
    app::{CacheChoice, Dialog, PendingApply, Studio},
    model::{format_bytes, home_path},
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
