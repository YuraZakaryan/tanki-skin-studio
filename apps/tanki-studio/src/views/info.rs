// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Info: the item in focus, its modifications in the cache, matching skin variants, the pack the
//! skins come from, and the form to label an unrecognised resource by hand.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Ui, Vec2};
use tanki_core::targets::modification_markers;
use tanki_i18n::{fmt, tl, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect, rows},
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, ListRow, Width},
};

use super::{scroll_page, shell};
use crate::{
    app::Studio,
    model::{grade_code, marker_id, target_name, target_title},
};

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Info"),
        tl!("Item, labeling and pack source"),
        |ui| {
            if Button::new(ButtonKind::Primary, tl!("Apply"))
                .icon(icons::CHECK)
                .enabled(app.chosen_apply_available() && app.label_resource.is_none())
                .show(ui)
                .clicked()
            {
                app.begin_apply_selected();
            }
        },
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let wide = super::is_wide(body.shrink2(Vec2::new(32.0, 0.0)));
    let min_height = if wide {
        764.0 + 64.0
    } else {
        32.0 + 193.0 + 24.0 + 291.0 + 24.0 + 232.0 + 24.0 + 209.0 + 24.0 + 420.0 + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        let (item, mods, skins, source_rect, label) = if wide {
            let cols = columns(content, 24.0, &[Size::Fill(1.0), Size::Fixed(440.0)]);
            let left = rows(
                cols[0],
                24.0,
                &[Size::Fixed(193.0), Size::Fixed(291.0), Size::Fill(1.0)],
            );
            let right = rows(cols[1], 24.0, &[Size::Fixed(209.0), Size::Fill(1.0)]);
            (left[0], left[1], left[2], right[0], right[1])
        } else {
            let all = rows(
                content,
                24.0,
                &[
                    Size::Fixed(193.0),
                    Size::Fixed(291.0),
                    Size::Fixed(232.0),
                    Size::Fixed(209.0),
                    Size::Fixed(420.0),
                ],
            );
            (all[0], all[1], all[2], all[3], all[4])
        };
        item_panel(app, ui, item);
        modifications(app, ui, mods);
        variants(app, ui, skins);
        source(app, ui, source_rect);
        labeling(app, ui, label);
    });
}

/// The resource being labelled, if the Info page was opened for one.
fn labelled_row(app: &Studio) -> Option<crate::model::ResourceRow> {
    let key = app.label_resource.as_ref()?;
    app.rows.iter().find(|r| &r.key == key).cloned()
}

fn item_panel(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let label_row = labelled_row(app);
    let (meta, title, kind_chip, state_chip, note) = if let Some(row) = &label_row {
        (
            format!("#{}", row.id),
            format!("#{}", row.id),
            (ChipKind::Neutral, row.kind.clone()),
            (ChipKind::Warning, tl!("Not recognised").to_owned()),
            tl!("Give this resource an item id below to use skins for it.").to_owned(),
        )
    } else if let Some(selection) = &app.selection {
        (
            format!("{} · {}", selection.kind, selection.id),
            target_title(&selection.id),
            (
                ChipKind::Info,
                super::garage::kind_label(&selection.kind).to_owned(),
            ),
            (ChipKind::Success, tl!("Recognised in cache").to_owned()),
            tl!(
                "Modifications of one item sit side by side and usually share the same model size."
            )
            .to_owned(),
        )
    } else {
        let body = widgets::panel(ui, rect, tl!("Item"), None);
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width(), 18.0)),
            tl!("Pick an item in the Garage or a resource in Resources."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let body = widgets::panel(ui, rect, tl!("Item"), Some(&meta));
    let inner = body.shrink(18.0);
    let thumb = Rect::from_min_size(inner.min, Vec2::splat(80.0));
    draw::rounded(
        ui.painter(),
        thumb,
        CornerRadius::same(12),
        tokens.stage,
        None,
    );
    fonts::icon(
        ui.painter(),
        thumb.center(),
        icons::TRUCK,
        34.0,
        tokens.primary,
    );
    let x = inner.min.x + 80.0 + 16.0;
    fonts::draw(
        ui.painter(),
        Pos2::new(x, inner.min.y + 9.0),
        &title,
        Text::display(24.0, 700, tokens.text),
    );
    let chips_y = inner.min.y + 48.0;
    let first = widgets::chip_width(ui, &kind_chip.1);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(Pos2::new(x, chips_y), Vec2::new(first, 24.0)),
        kind_chip.0,
        &kind_chip.1,
    );
    let second = widgets::chip_width(ui, &state_chip.1);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(Pos2::new(x + first + 8.0, chips_y), Vec2::new(second, 24.0)),
        state_chip.0,
        &state_chip.1,
    );
    let galley = fonts::paragraph(
        ui.painter(),
        &note,
        Text::body(13.0, 400, tokens.text_2),
        inner.width(),
        Some(20.0),
    );
    ui.painter().galley(
        Pos2::new(inner.min.x, inner.min.y + 92.0),
        galley,
        tokens.text_2,
    );
}

fn modifications(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let label_row = labelled_row(app);
    // The four slots: for a recognised item the item's own markers, for a resource being labelled
    // the ids that would be labelled with it (so adjacency can be confirmed by eye).
    let slots: Vec<(String, Option<String>)> = if let Some(row) = &label_row {
        let block = modification_markers(&row.key, grade_code(app.label_grade)).unwrap_or_default();
        (0..4)
            .map(|i| {
                let marker = block.get(i).map(|(m, _)| m.clone());
                (format!("M{i}"), marker)
            })
            .collect()
    } else if let Some(selection) = &app.selection {
        let item = app
            .items
            .iter()
            .find(|i| i.kind == selection.kind && i.id == selection.id);
        (0..4)
            .map(|i| {
                (
                    target_name(&selection.id, grade_code(i)),
                    item.and_then(|it| it.markers[i].clone()),
                )
            })
            .collect()
    } else {
        Vec::new()
    };
    let found = slots
        .iter()
        .filter(|(_, m)| {
            m.as_ref()
                .is_some_and(|m| app.rows.iter().any(|r| &r.key == m))
        })
        .count();
    let meta = fmt(tl!("{found} of 4 found"), &[("found", &found.to_string())]);
    let body = widgets::panel(ui, rect, tl!("Modifications"), Some(&meta));
    let list = body.shrink2(Vec2::new(8.0, 8.0));
    let mut open = None;
    in_rect(ui, list, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for (index, (name, marker)) in slots.iter().enumerate() {
            let row = marker
                .as_ref()
                .and_then(|m| app.rows.iter().find(|r| &r.key == m));
            let meta = marker
                .as_ref()
                .map_or_else(|| "—".to_owned(), |m| format!("#{}", marker_id(m)));
            let mut entry = ListRow::new(icons::LAYERS, name, &meta);
            let size_text;
            match row {
                Some(row) => {
                    size_text = tn(row.files as u64, "{n} file", "{n} files");
                    entry = entry.chip(ChipKind::Success, tl!("Found")).size(&size_text);
                }
                None => {
                    entry = entry.chip(ChipKind::Neutral, tl!("Not in cache")).size("—");
                }
            }
            if entry.show(ui).clicked()
                && let Some(row) = row
            {
                open = Some(row.key.clone());
            }
            let _ = index;
        }
    });
    if let Some(key) = open {
        app.open_row(&key);
    }
}

fn variants(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let selection = app
        .selection
        .clone()
        .filter(|_| app.label_resource.is_none());
    let meta = match (&app.active_pack, &selection) {
        (Some(pack), Some(s)) => Some(format!(
            "{} · {}",
            pack.manifest.name,
            target_name(&s.id, grade_code(s.grade))
        )),
        _ => None,
    };
    let body = widgets::panel(ui, rect, tl!("Skin variants"), meta.as_deref());
    let inner = body.shrink(18.0);
    let Some(selection) = selection else {
        return;
    };
    let options = app.pack_variants(&selection.kind, &selection.id, grade_code(selection.grade));
    if options.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(inner.center(), Vec2::new(inner.width(), 18.0)),
            tl!("The active pack has no skins for this item."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    let chosen = app.chosen_variant(&selection);
    let mut picked = None;
    let mut apply = false;
    in_rect(ui, inner, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("info-1")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                for (item, variant) in &options {
                    let Some((pack_item, v)) = app.pack_variant(*item, *variant) else {
                        continue;
                    };
                    let meta = format!(
                        "{} · {}",
                        pack_item.target.grade.to_uppercase(),
                        v.resources
                            .iter()
                            .map(|r| r.role.as_str())
                            .collect::<Vec<_>>()
                            .join(" + ")
                    );
                    let selected = chosen == Some((*item, *variant));
                    let (response, applied) = widgets::skin_row(
                        ui,
                        inner.width(),
                        &v.name,
                        &meta,
                        selected,
                        selected.then(|| tl!("Apply")),
                        None,
                    );
                    if response.clicked() {
                        picked = app.variant_key(*item, *variant);
                    }
                    apply |= applied;
                }
            });
    });
    if let Some(id) = picked {
        app.remember_variant(&selection, &id);
    }
    if apply {
        app.begin_apply_selected();
    }
}

fn source(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let body = widgets::panel(ui, rect, tl!("Source and compatibility"), None);
    let Some(pack) = app.active_pack.clone() else {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width(), 18.0)),
            tl!("No active pack"),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let inner = body.shrink(18.0);
    let facts = [
        (tl!("Pack"), pack.manifest.name.clone()),
        (tl!("Version"), pack.manifest.version.clone()),
        (tl!("Author"), crate::model::or_dash(&pack.manifest.author)),
        (
            tl!("License"),
            crate::model::or_dash(&pack.manifest.license),
        ),
    ];
    for (index, (key, value)) in facts.iter().enumerate() {
        let line = Rect::from_min_size(
            Pos2::new(inner.min.x, inner.min.y + index as f32 * 26.0),
            Vec2::new(inner.width(), 16.0),
        );
        widgets::key_value(ui, line, key, value, false);
    }
    let y = inner.min.y + 4.0 * 26.0;
    fonts::draw_left(
        ui.painter(),
        inner.min.x,
        y + 12.0,
        tl!("Clients"),
        Text::body(13.0, 400, tokens.text_3),
    );
    let mut x = inner.min.x
        + fonts::width(
            ui.painter(),
            tl!("Clients"),
            Text::body(13.0, 400, tokens.text_3),
        )
        + 8.0;
    for game in &pack.manifest.games {
        let label = shell::game_short_name(game);
        let width = widgets::chip_width(ui, &label);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
            ChipKind::Info,
            &label,
        );
        x += width + 8.0;
    }
}

fn labeling(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let body = widgets::panel(ui, rect, tl!("Manual labeling"), None);
    let inner = body.shrink(18.0);
    let active = app.label_resource.is_some();
    let save = Rect::from_min_size(
        Pos2::new(inner.min.x, inner.max.y - 44.0),
        Vec2::new(inner.width(), 44.0),
    );
    let mut y = inner.min.y;
    if !active {
        let galley = fonts::paragraph(
            ui.painter(),
            tl!("Open an unrecognised resource from Resources with “Label manually” to name it."),
            Text::body(13.0, 400, tokens.text_3),
            inner.width(),
            Some(20.0),
        );
        ui.painter()
            .galley(Pos2::new(inner.min.x, y), galley, tokens.text_3);
        y += 60.0;
    }
    let id_rect = Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), 40.0));
    let hint = tl!("Item id, e.g. hornet");
    ui.add_enabled_ui(active, |ui| {
        widgets::text_field(ui, id_rect, icons::TAG, hint, &mut app.label_id, None);
    });
    y += 40.0 + 14.0;
    // Kind and modification of the resource being labelled: one row when it fits, else the
    // modifications go to a row of their own (Russian kind names are longer).
    let kinds = ["hull", "turret", "paint"];
    let row_width = kinds
        .iter()
        .map(|k| widgets::tab_width(ui, super::garage::kind_label(k)))
        .sum::<f32>()
        + 6.0 * 2.0
        + 8.0
        + 6.0
        + 4.0 * 44.0
        + 6.0 * 3.0;
    let stacked = row_width > inner.width();
    let grade_chips = |app: &mut Studio, ui: &mut Ui| {
        for grade in 0..4 {
            if widgets::grade_chip(
                ui,
                &format!("M{grade}"),
                app.label_grade == grade,
                true,
                44.0,
            )
            .clicked()
            {
                app.label_grade = grade;
            }
        }
    };
    let kinds_rect = Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), 34.0));
    ui.add_enabled_ui(active, |ui| {
        in_rect(ui, kinds_rect, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                for (index, kind) in kinds.into_iter().enumerate() {
                    if widgets::tab(ui, super::garage::kind_label(kind), app.label_kind == index)
                        .clicked()
                    {
                        app.label_kind = index;
                    }
                }
                if !stacked {
                    ui.add_space(8.0);
                    grade_chips(app, ui);
                }
            });
        });
    });
    y += 34.0 + 14.0;
    if stacked {
        let grades_rect = Rect::from_min_size(
            Pos2::new(inner.min.x, y - 6.0),
            Vec2::new(inner.width(), 34.0),
        );
        ui.add_enabled_ui(active, |ui| {
            in_rect(ui, grades_rect, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    grade_chips(app, ui);
                });
            });
        });
        y += 34.0 + 8.0;
    }
    let toggle_rect =
        Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), 22.0));
    ui.add_enabled_ui(active, |ui| {
        in_rect(ui, toggle_rect, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                widgets::toggle(
                    ui,
                    &mut app.label_whole,
                    tl!("Label all modifications M0–M3 (adjacent resources)"),
                );
                ui.label(
                    egui::RichText::new(tl!("Label all modifications M0–M3 (adjacent resources)"))
                        .color(tokens.text_2)
                        .size(13.0),
                );
            });
        });
    });
    let valid = !app.label_id.trim().is_empty()
        && app
            .label_id
            .trim()
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    in_rect(ui, save, |ui| {
        if Button::new(ButtonKind::Secondary, tl!("Save labeling"))
            .icon(icons::TAG)
            .width(Width::Fill)
            .height(44.0)
            .enabled(active && valid)
            .show(ui)
            .clicked()
        {
            app.save_label();
        }
    });
}
