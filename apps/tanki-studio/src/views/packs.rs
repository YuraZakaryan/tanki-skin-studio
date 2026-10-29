// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Packs: the library of imported packs, the active pack's details and the backup history.

use eframe::egui::{self, Pos2, Rect, Ui, Vec2};
use tanki_i18n::{fmt, tl, tn};
use tanki_ui::{
    fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect, rows},
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, ListRow, Width},
};

use super::{scroll_page, shell};
use crate::{
    app::Studio,
    model::{format_bytes, target_name},
};

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Packs"),
        tl!(".tankipack library · active pack"),
        |ui| {
            if Button::new(ButtonKind::Primary, tl!("Import ZIP"))
                .icon(icons::UPLOAD)
                .show(ui)
                .clicked()
            {
                app.import(false);
            }
            if Button::new(ButtonKind::Secondary, tl!("From folder"))
                .icon(icons::FOLDER_OPEN)
                .show(ui)
                .clicked()
            {
                app.import(true);
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
        32.0 + 560.0 + 24.0 + 201.0 + 24.0 + 175.0 + 24.0 + 340.0 + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        let (mine, details, items, backup) = if wide {
            let cols = columns(content, 24.0, &[Size::Fill(1.0), Size::Fixed(420.0)]);
            let right = rows(
                cols[1],
                24.0,
                &[Size::Fixed(201.0), Size::Fixed(175.0), Size::Fill(1.0)],
            );
            (cols[0], right[0], right[1], right[2])
        } else {
            let all = rows(
                content,
                24.0,
                &[
                    Size::Fixed(560.0),
                    Size::Fixed(201.0),
                    Size::Fixed(175.0),
                    Size::Fixed(340.0),
                ],
            );
            (all[0], all[1], all[2], all[3])
        };
        my_packs(app, ui, mine);
        active_details(app, ui, details);
        pack_items(app, ui, items);
        backups(app, ui, backup);
    });
}

fn my_packs(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let meta = tn(app.packs.len() as u64, "{n} pack", "{n} packs");
    let body = widgets::panel(ui, rect, tl!("My packs"), Some(&meta));
    let inner = body.shrink(18.0);
    let per_row = (((inner.width() + 16.0) / (200.0 + 16.0)).floor() as usize).max(1);
    let card_width = (inner.width() - (per_row - 1) as f32 * 16.0) / per_row as f32;
    let packs = app.packs.clone();
    let active = app
        .active_pack
        .as_ref()
        .map(|p| (p.manifest.id.clone(), p.manifest.version.clone()));
    let grid_rows = packs.len().div_ceil(per_row);
    let grid_height = grid_rows as f32 * 256.0 + grid_rows.saturating_sub(1) as f32 * 16.0;
    let grid = Rect::from_min_size(
        inner.min,
        Vec2::new(inner.width(), grid_height.min(inner.height())),
    );
    let mut select = None;
    let mut delete = None;
    in_rect(ui, grid, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("packs-1")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let top = ui.cursor().min;
                ui.allocate_exact_size(Vec2::new(grid.width(), grid_height), egui::Sense::hover());
                for (index, pack) in packs.iter().enumerate() {
                    let (row, col) = (index / per_row, index % per_row);
                    let card = Rect::from_min_size(
                        Pos2::new(
                            top.x + col as f32 * (card_width + 16.0),
                            top.y + row as f32 * (256.0 + 16.0),
                        ),
                        Vec2::new(card_width, 254.0),
                    );
                    let is_active = active.as_ref()
                        == Some(&(pack.manifest.id.clone(), pack.manifest.version.clone()));
                    let count: usize = pack.manifest.items.iter().map(|i| i.variants.len()).sum();
                    let version = format!("v{}", pack.manifest.version);
                    let meta = crate::model::dotted([
                        version.as_str(),
                        pack.manifest.author.as_str(),
                        pack.manifest.license.as_str(),
                    ]);
                    let status = if is_active {
                        (ChipKind::Success, tl!("Active").to_owned())
                    } else {
                        (
                            ChipKind::Neutral,
                            tn(count as u64, "{n} variant", "{n} variants"),
                        )
                    };
                    let response = widgets::pack_card(
                        ui,
                        card,
                        &widgets::PackCard {
                            name: &pack.manifest.name,
                            meta: &meta,
                            active: is_active,
                            status: (status.0, &status.1),
                            select_label: tl!("Select"),
                            delete_label: tl!("Delete pack"),
                        },
                    );
                    if response.select {
                        select = Some(index);
                    }
                    if response.delete {
                        delete = Some((pack.manifest.id.clone(), pack.manifest.version.clone()));
                    }
                }
            });
    });
    let zone_top = grid.max.y + 16.0;
    let zone = Rect::from_min_max(Pos2::new(inner.min.x, zone_top), inner.max);
    if zone.height() > 40.0 {
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        if widgets::dropzone(
            ui,
            zone,
            tl!("Drop a .tankipack here"),
            tl!(".tankipack, .zip or a folder with manifest.json"),
            hovering,
        )
        .clicked()
        {
            app.import(false);
        }
    } else if packs.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(inner.center(), Vec2::new(inner.width(), 18.0)),
            tl!("Add your first pack"),
            Text::body(13.0, 400, tokens.text_3),
        );
    }
    if let Some(index) = select {
        app.select_pack(index);
    }
    if let Some((id, version)) = delete {
        app.delete_pack(&id, &version);
    }
}

fn active_details(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let pack = app.active_pack.clone();
    let name = pack.as_ref().map(|p| p.manifest.name.clone());
    let body = widgets::panel(ui, rect, tl!("Active pack"), name.as_deref());
    let Some(pack) = pack else {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width(), 18.0)),
            tl!("No active pack"),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let inner = body.shrink(18.0);
    let games: Vec<String> = pack
        .manifest
        .games
        .iter()
        .map(|g| shell::game_short_name(g))
        .collect();
    let facts = [
        (tl!("Version"), pack.manifest.version.clone()),
        (tl!("Author"), crate::model::or_dash(&pack.manifest.author)),
        (
            tl!("License"),
            crate::model::or_dash(&pack.manifest.license),
        ),
        (tl!("Clients"), games.join(", ")),
        (tl!("Checksums"), "SHA-256 ✓".to_owned()),
    ];
    for (index, (key, value)) in facts.iter().enumerate() {
        let line = Rect::from_min_size(
            Pos2::new(inner.min.x, inner.min.y + index as f32 * 26.0),
            Vec2::new(inner.width(), 16.0),
        );
        widgets::key_value(ui, line, key, value, false);
    }
}

fn pack_items(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let pack = app.active_pack.clone();
    let variants: usize = pack.as_ref().map_or(0, |p| {
        p.manifest.items.iter().map(|i| i.variants.len()).sum()
    });
    let count = pack.as_ref().map_or(0, |p| p.manifest.items.len());
    let meta = fmt(
        tl!("{items} · {variants}"),
        &[
            ("items", &tn(count as u64, "{n} item", "{n} items")),
            (
                "variants",
                &tn(variants as u64, "{n} variant", "{n} variants"),
            ),
        ],
    );
    let body = widgets::panel(ui, rect, tl!("Pack items"), Some(&meta));
    let Some(pack) = pack else {
        return;
    };
    let list = body.shrink(8.0);
    let mut open = None;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("packs-2")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for item in &pack.manifest.items {
                    let name = target_name(&item.target.id, &item.target.grade);
                    let meta = tn(item.variants.len() as u64, "{n} variant", "{n} variants");
                    let icon = match item.target.kind.as_str() {
                        "turret" => icons::CROSSHAIR,
                        "paint" => icons::PAINTBRUSH,
                        _ => icons::TRUCK,
                    };
                    let kind = super::garage::kind_label(&item.target.kind);
                    if ListRow::new(icon, &name, &meta)
                        .chip(ChipKind::Neutral, kind)
                        .show(ui)
                        .clicked()
                    {
                        open = Some((
                            item.target.kind.clone(),
                            item.target.id.clone(),
                            crate::model::grade_index(&item.target.grade).unwrap_or(3),
                        ));
                    }
                }
            });
    });
    if let Some((kind, id, grade)) = open {
        app.open_in_garage(&kind, &id, grade);
    }
}

fn backups(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let meta = tn(app.backups.len() as u64, "{n} backup", "{n} backups");
    let body = widgets::panel(ui, rect, tl!("Backups"), Some(&meta));
    let inner = body.shrink(8.0);
    let button = Rect::from_min_size(
        Pos2::new(inner.min.x, inner.max.y - 44.0),
        Vec2::new(inner.width(), 44.0),
    );
    let list = Rect::from_min_max(inner.min, Pos2::new(inner.max.x, button.min.y - 8.0));
    if app.backups.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(list.center(), Vec2::new(list.width(), 18.0)),
            tl!("Backups appear after the first replacement."),
            Text::body(13.0, 400, tokens.text_3),
        );
    }
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("packs-3")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for backup in &app.backups {
                    let target = backup.tx.target.as_ref().map_or_else(
                        || backup.tx.pack_id.clone(),
                        |t| target_name(&t.id, &t.grade),
                    );
                    let name = if backup.tx.variant_name.is_empty() {
                        target
                    } else {
                        format!("{target} · {}", backup.tx.variant_name)
                    };
                    let meta = fmt(
                        tl!("{when} · {files}"),
                        &[
                            ("when", &shell::describe_time(backup.created_unix)),
                            (
                                "files",
                                &tn(backup.tx.changes.len() as u64, "{n} file", "{n} files"),
                            ),
                        ],
                    );
                    ListRow::new(icons::HISTORY, &name, &meta)
                        .size(&format_bytes(backup.bytes))
                        .clickable(false)
                        .show(ui);
                }
            });
    });
    in_rect(ui, button, |ui| {
        if Button::new(ButtonKind::Danger, tl!("Restore last replacement"))
            .icon(icons::ROTATE_CCW)
            .width(Width::Fill)
            .height(44.0)
            .enabled(app.can_restore())
            .show(ui)
            .clicked()
        {
            app.restore_last();
        }
    });
}
