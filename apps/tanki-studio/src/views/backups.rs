// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Backups, a page of Settings: what the backups take against the limit (with the limit, Restore
//! original files and Delete old), a filter, and every backup by day, newest first, each with
//! what it is still good for and the one action that fits it.

use eframe::egui::{CornerRadius, Pos2, Rect, Ui, Vec2};
use tanki_core::backups::Kept;
use tanki_i18n::{fmt, tl, tl_ctx, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, GroupRow, Tint},
};

use super::{scroll_page, settings::usage_bar, shell};
use crate::{
    app::{BackupsFilter, DeleteWhat, Page, Studio},
    backups::{LIMITS, Line, LineKey, limit_label, lines},
    model::{Day, format_bytes, local_time, now_and_offset},
};

const SUMMARY: f32 = 18.0 + 26.0 + 14.0 + 36.0 + 14.0 + 34.0 + 18.0;
const SEGMENTS: f32 = 32.0;

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    if widgets::top_bar_back(
        ui,
        shell::top_bar_rect(main),
        tl!("Backups"),
        tl!("Original files of every skin put on, newest first"),
        tl!("Back to Settings"),
        |_| {},
    ) {
        app.go(Page::Settings);
    }
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let packs = app.packs.clone();
    let all = lines(&app.stored, |id| {
        packs
            .iter()
            .find(|p| p.manifest.id == id)
            .map_or_else(|| id.to_owned(), |p| p.manifest.name.clone())
    });
    let shown: Vec<&Line> = all
        .iter()
        .filter(|line| match app.backups_filter {
            BackupsFilter::All => true,
            BackupsFilter::Needed => !line.kept.is_old(),
            BackupsFilter::Old => line.kept.is_old(),
        })
        .collect();
    let width = (body.width() - 64.0).max(240.0);
    let days = by_day(&shown);
    let details: Vec<Vec<String>> = days
        .iter()
        .map(|(_, lines)| lines.iter().map(|l| detail(l)).collect())
        .collect();
    let list_height: f32 = days
        .iter()
        .zip(&details)
        .map(|((_, lines), details)| {
            29.0 + widgets::group_height(ui, width, &rows(ui, lines, details)) + 18.0
        })
        .sum();
    let summary_height = SUMMARY
        + if summary_stacked(app, ui, width) {
            46.0
        } else {
            0.0
        };
    let height = 32.0 + summary_height + 20.0 + SEGMENTS + 20.0 + list_height.max(120.0) + 32.0;
    scroll_page(ui, body, height, |ui, rect| {
        let x = rect.min.x + 32.0;
        let summary = Rect::from_min_size(
            Pos2::new(x, rect.min.y + 32.0),
            Vec2::new(width, summary_height),
        );
        draw_summary(app, ui, summary);
        let counts = [
            all.len(),
            all.iter().filter(|l| !l.kept.is_old()).count(),
            all.iter().filter(|l| l.kept.is_old()).count(),
        ];
        let items: Vec<(&str, Option<String>)> = [tl!("All"), tl!("Needed to undo"), tl!("Old")]
            .into_iter()
            .zip(counts)
            .map(|(label, n)| (label, Some(n.to_string())))
            .collect();
        let active = BackupsFilter::ALL
            .iter()
            .position(|f| *f == app.backups_filter)
            .unwrap_or(0);
        let (_, clicked) = widgets::choice_segments(
            ui,
            Pos2::new(x, summary.max.y + 20.0),
            SEGMENTS,
            &items,
            active,
            "backups-filter",
        );
        if let Some(filter) = clicked.and_then(|i| BackupsFilter::ALL.get(i)) {
            app.backups_filter = *filter;
        }
        let mut y = summary.max.y + 20.0 + SEGMENTS + 20.0;
        if days.is_empty() {
            let tokens = Tokens::get(ui.ctx());
            fonts::draw_centered(
                ui.painter(),
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 80.0)),
                if all.is_empty() {
                    tl!("No backups yet. Each skin you put on keeps its original files here.")
                } else {
                    tl!("Nothing here with this filter.")
                },
                Text::body(13.0, 400, tokens.text_3),
            );
            return;
        }
        let mut action = None;
        for ((day, lines), details) in days.iter().zip(&details) {
            // A day's heading ends with Delete day, which asks first.
            let day_button = Button::new(ButtonKind::Ghost, tl!("Delete day"))
                .icon(icons::TRASH_2)
                .height(28.0);
            let day_width = day_button.measure(ui);
            let place = Rect::from_min_size(
                Pos2::new(x + width - day_width, y - 5.0),
                Vec2::new(day_width, 28.0),
            );
            if in_rect(ui, place, |ui| day_button.show(ui).clicked()) {
                action = Some(Action::DeleteDay(
                    lines.iter().map(|l| (*l).clone()).collect(),
                ));
            }
            y = widgets::group_heading(ui, Pos2::new(x, y), day);
            let rows = rows(ui, lines, details);
            let group_height = widgets::group_height(ui, width, &rows);
            let places = widgets::group(ui, Pos2::new(x, y), width, &rows, day);
            for (place, line) in places.iter().zip(lines.iter()) {
                match draw_control(ui, place.control, line) {
                    Some(Click::Undo) => action = Some(Action::Undo((*line).clone())),
                    Some(Click::Delete) => action = Some(Action::Delete((*line).clone())),
                    None => {}
                }
            }
            y += group_height + 18.0;
        }
        match action {
            Some(Action::Undo(line)) => app.undo_line(&line),
            Some(Action::Delete(line)) => app.ask_to_delete(
                DeleteWhat::One {
                    title: line.title.clone(),
                    old: line.kept.is_old(),
                },
                &[line],
            ),
            Some(Action::DeleteDay(lines)) => {
                let (now, offset) = now_and_offset();
                if let Some(first) = lines.first() {
                    let (day, _) = local_time(first.created, now, offset);
                    app.ask_to_delete(DeleteWhat::Day(day), &lines);
                }
            }
            None => {}
        }
    });
}

/// A click on a line's control.
enum Click {
    Undo,
    Delete,
}

enum Action {
    Undo(Line),
    Delete(Line),
    DeleteDay(Vec<Line>),
}

fn summary_total(app: &Studio) -> String {
    let usage = app.usage;
    match app.backup_limit {
        Some(limit) => fmt(
            tl!("{used} of {limit}"),
            &[
                ("used", &format_bytes(usage.total())),
                ("limit", &crate::backups::limit_label(Some(limit))),
            ],
        ),
        None => format_bytes(usage.total()),
    }
}

fn delete_old_button(app: &Studio) -> Button<'static> {
    Button::new(ButtonKind::Secondary, tl!("Delete old"))
        .icon(icons::TRASH_2)
        .enabled(app.usage.old > 0)
}

fn delete_all_button(app: &Studio) -> Button<'static> {
    Button::new(ButtonKind::Ghost, tl!("Delete all"))
        .icon(icons::TRASH_2)
        .enabled(!app.stored.is_empty())
}

/// A narrow page puts the summary's buttons on a line under the total.
fn summary_stacked(app: &Studio, ui: &Ui, width: f32) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let title = fonts::width(
        ui.painter(),
        &summary_total(app),
        Text::display(20.0, 700, tokens.text),
    );
    width - 40.0
        < title
            + 20.0
            + delete_old_button(app).measure(ui)
            + 10.0
            + delete_all_button(app).measure(ui)
}

fn draw_summary(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let inner = rect.shrink2(Vec2::new(20.0, 18.0));
    let stacked = summary_stacked(app, ui, rect.width());
    fonts::draw(
        ui.painter(),
        inner.min,
        &summary_total(app),
        Text::display(20.0, 700, tokens.text),
    );
    let delete = delete_old_button(app);
    let delete_all = delete_all_button(app);
    let restore = Button::new(ButtonKind::Ghost, tl!("Restore original files"))
        .icon(icons::ROTATE_CCW)
        .enabled(app.can_restore());
    let delete_width = delete.measure(ui);
    let all_width = delete_all.measure(ui);
    let restore_width = restore.measure(ui);
    // Right-aligned beside the total, or on a line of their own under it on a narrow page.
    let buttons_y = if stacked {
        inner.min.y + 34.0
    } else {
        inner.min.y - 6.0
    };
    let place = Rect::from_min_size(
        Pos2::new(inner.max.x - delete_width, buttons_y),
        Vec2::new(delete_width, 40.0),
    );
    if in_rect(ui, place, |ui| delete.show(ui).clicked()) {
        app.ask_to_delete_old();
    }
    let place = Rect::from_min_size(
        Pos2::new(place.min.x - 10.0 - all_width, buttons_y),
        Vec2::new(all_width, 40.0),
    );
    if in_rect(ui, place, |ui| delete_all.show(ui).clicked()) {
        app.ask_to_delete_all();
    }
    // The restore button gives way on a narrow page; the Garage keeps one too.
    if !stacked && inner.width() > delete_width + all_width + restore_width + 20.0 + 220.0 {
        let place = Rect::from_min_size(
            Pos2::new(place.min.x - 10.0 - restore_width, buttons_y),
            Vec2::new(restore_width, 40.0),
        );
        if in_rect(ui, place, |ui| restore.show(ui).clicked()) {
            app.ask_to_restore();
        }
    }
    let top = if stacked {
        inner.min.y + 46.0
    } else {
        inner.min.y
    };
    let bar = Rect::from_min_size(
        Pos2::new(inner.min.x, top + 26.0 + 14.0),
        Vec2::new(inner.width(), 36.0),
    );
    usage_bar(app, ui, bar);
    // The limit, under the bar: old backups beyond it go by themselves.
    let row = Rect::from_min_size(
        Pos2::new(inner.min.x, bar.max.y + 14.0),
        Vec2::new(inner.width(), 34.0),
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(row.min.x + 8.0, row.center().y),
        icons::GAUGE,
        16.0,
        tokens.text_3,
    );
    let used = fonts::draw_left(
        ui.painter(),
        row.min.x + 26.0,
        row.center().y,
        tl!("Space for backups"),
        Text::body(13.0, 600, tokens.text),
    );
    let select_width = 140.0;
    let note = tl!("Over it, the oldest old backups go first");
    let note_style = Text::body(12.0, 400, tokens.text_3);
    if used.max.x + 12.0 + fonts::width(ui.painter(), note, note_style)
        < row.max.x - select_width - 16.0
    {
        fonts::draw_left(
            ui.painter(),
            used.max.x + 12.0,
            row.center().y,
            note,
            note_style,
        );
    }
    let options: Vec<String> = LIMITS.iter().map(|l| limit_label(*l)).collect();
    let select = Rect::from_min_size(
        Pos2::new(row.max.x - select_width, row.min.y),
        Vec2::new(select_width, 34.0),
    );
    if let Some(index) = widgets::select(
        ui,
        select,
        tl!("Space for backups"),
        &limit_label(app.backup_limit),
        &options,
        true,
        false,
    ) && let Some(limit) = LIMITS.get(index)
    {
        app.set_backup_limit(*limit);
    }
}

/// The lines by day ("Today", "Yesterday", or the date), newest first.
fn by_day<'a>(lines: &[&'a Line]) -> Vec<(String, Vec<&'a Line>)> {
    let (now, offset) = now_and_offset();
    let mut days: Vec<(String, Vec<&Line>)> = Vec::new();
    for line in lines {
        let (day, _) = local_time(line.created, now, offset);
        let label = match day {
            Day::Today => tl!("Today").to_owned(),
            Day::Yesterday => tl!("Yesterday").to_owned(),
            Day::Date(date) => date,
        };
        match days.last_mut() {
            Some((last, group)) if *last == label => group.push(line),
            _ => days.push((label, vec![line])),
        }
    }
    days
}

fn kept_chip(kept: Kept) -> (ChipKind, &'static str) {
    match kept {
        Kept::Needed => (ChipKind::Success, tl_ctx!("backup", "Needed to undo")),
        Kept::TakenOff => (ChipKind::Neutral, tl!("Old · taken off")),
        Kept::Replaced => (ChipKind::Neutral, tl!("Old · removed by a game update")),
    }
}

/// "17:15 · 8 files · Desert Camo".
fn detail(line: &Line) -> String {
    let (now, offset) = now_and_offset();
    let (_, clock) = local_time(line.created, now, offset);
    format!(
        "{clock} · {} · {}",
        tn(line.files as u64, "{n} file", "{n} files"),
        line.pack
    )
}

fn undo_button() -> Button<'static> {
    Button::new(ButtonKind::Secondary, tl!("Undo")).icon(icons::UNDO_2)
}

fn control_width(ui: &Ui, line: &Line) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    let (_, label) = kept_chip(line.kept);
    let size = fonts::width(
        ui.painter(),
        &format_bytes(line.bytes),
        Text::mono(13.0, 400, tokens.text_2),
    );
    let action = if line.kept.is_old() {
        38.0
    } else {
        undo_button().measure(ui)
    };
    widgets::chip_width(ui, label) + 12.0 + size + 12.0 + action
}

fn rows<'a>(ui: &Ui, lines: &'a [&'a Line], details: &'a [String]) -> Vec<GroupRow<'a>> {
    lines
        .iter()
        .zip(details)
        .map(|(line, detail)| GroupRow {
            icon: if matches!(line.what, LineKey::Run(_)) {
                icons::LAYERS
            } else {
                icons::HISTORY
            },
            mark: None,
            tint: if line.kept.is_old() {
                Tint::Plain
            } else {
                Tint::Success
            },
            title: &line.title,
            desc: Some(detail),
            control: Vec2::new(control_width(ui, line), 40.0),
            extra: 0.0,
            click: false,
        })
        .collect()
}

fn draw_control(ui: &mut Ui, rect: Rect, line: &Line) -> Option<Click> {
    let tokens = Tokens::get(ui.ctx());
    let (kind, label) = kept_chip(line.kept);
    let chip = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.center().y - 12.0),
        Vec2::new(widgets::chip_width(ui, label), 24.0),
    );
    widgets::paint_chip(ui, chip, kind, label);
    let size = fonts::draw_left(
        ui.painter(),
        chip.max.x + 12.0,
        rect.center().y,
        &format_bytes(line.bytes),
        Text::mono(13.0, 400, tokens.text_2),
    );
    let action = Rect::from_min_max(Pos2::new(size.max.x + 12.0, rect.min.y), rect.max);
    if line.kept.is_old() {
        let place = Rect::from_min_size(
            Pos2::new(action.max.x - 38.0, rect.center().y - 19.0),
            Vec2::splat(38.0),
        );
        in_rect(ui, place, |ui| {
            widgets::icon_button(ui, icons::TRASH_2, tl!("Delete")).clicked()
        })
        .then_some(Click::Delete)
    } else {
        in_rect(ui, action, |ui| undo_button().show(ui).clicked()).then_some(Click::Undo)
    }
}
