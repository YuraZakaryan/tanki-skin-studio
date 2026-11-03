// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Packs: the library of imported packs, the active pack's details and the backup history.

use eframe::egui::{self, Pos2, Rect, Ui, Vec2};
use tanki_core::later::LaterSkin;
use tanki_i18n::{fmt, tl, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect, rows},
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, ListRow},
};

use super::{scroll_page, shell};
use crate::{app::Studio, model::item_name};

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
    let root = app
        .scan
        .as_ref()
        .map(|s| s.root.to_string_lossy().into_owned())
        .unwrap_or_default();
    let waiting: Vec<LaterSkin> = crate::later::of_cache(&app.later, &root)
        .into_iter()
        .cloned()
        .collect();
    let later_height = if waiting.is_empty() { 0.0 } else { 210.0 };
    let min_height = if wide {
        764.0 + 64.0
    } else {
        32.0 + 560.0 + 24.0 + 201.0 + 24.0 + 340.0 + 32.0 + later_height + 24.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        // The backups have their own page (Settings, Backups): the pack's items take the room.
        let (mine, details, items) = if wide {
            let cols = columns(content, 24.0, &[Size::Fill(1.0), Size::Fixed(420.0)]);
            let right = rows(cols[1], 24.0, &[Size::Fixed(201.0), Size::Fill(1.0)]);
            (cols[0], right[0], right[1])
        } else {
            let all = rows(
                content,
                24.0,
                &[
                    Size::Fixed(560.0 + later_height + 24.0),
                    Size::Fixed(201.0),
                    Size::Fixed(340.0),
                ],
            );
            (all[0], all[1], all[2])
        };
        // The skins that go on later sit under the library, which gives up some of its drop zone.
        let mine = if waiting.is_empty() {
            mine
        } else {
            let parts = rows(mine, 24.0, &[Size::Fill(1.0), Size::Fixed(later_height)]);
            later_list(app, ui, parts[1], &waiting);
            parts[0]
        };
        my_packs(app, ui, mine);
        active_details(app, ui, details);
        pack_items(app, ui, items);
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
        app.ask_to_delete_pack(&id, &version);
    }
}

/// The modifications waiting for the game to download them, each with Remove.
fn later_list(app: &mut Studio, ui: &mut Ui, rect: Rect, waiting: &[LaterSkin]) {
    let tokens = Tokens::get(ui.ctx());
    let meta = tn(
        waiting.len() as u64,
        "{n} modification",
        "{n} modifications",
    );
    let body = widgets::panel(ui, rect, tl!("Go on later"), Some(&meta));
    let inner = body.shrink(18.0);
    let note_text = tl!(
        "Open these modifications in the game's garage. Once the game is closed, the Studio puts them on by itself."
    );
    let note = fonts::paragraph(
        ui.painter(),
        note_text,
        Text::body(12.0, 400, tokens.text_2),
        inner.width() - 24.0 - 14.0 - 10.0,
        None,
    );
    let note_height = note.size().y + 20.0;
    // The note follows the rows; many rows scroll above it.
    let rows_height = (waiting.len() as f32 * 48.0 - 12.0)
        .min(inner.height() - note_height - 12.0)
        .max(36.0);
    let list = Rect::from_min_size(inner.min, Vec2::new(inner.width(), rows_height));
    let mut remove = None;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("later-list")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let top = ui.cursor().min;
                let total = waiting.len() as f32 * 48.0 - 12.0;
                ui.allocate_exact_size(Vec2::new(list.width(), total), egui::Sense::hover());
                for (index, skin) in waiting.iter().enumerate() {
                    let row = Rect::from_min_size(
                        Pos2::new(top.x, top.y + index as f32 * 48.0),
                        Vec2::new(list.width(), 36.0),
                    );
                    let icon_box = Rect::from_min_size(row.min, Vec2::splat(36.0));
                    draw::rounded(
                        ui.painter(),
                        icon_box,
                        egui::CornerRadius::same(8),
                        tokens.info_soft,
                        None,
                    );
                    fonts::icon(
                        ui.painter(),
                        icon_box.center(),
                        icons::CLOCK_3,
                        16.0,
                        tokens.info,
                    );
                    let button = Button::new(ButtonKind::Ghost, tl!("Remove")).icon(icons::X);
                    let width = button.measure(ui);
                    let place = Rect::from_min_size(
                        Pos2::new(row.max.x - width, row.center().y - 20.0),
                        Vec2::new(width, 40.0),
                    );
                    if in_rect(ui, place, |ui| button.show(ui).clicked()) {
                        remove = Some(skin.clone());
                    }
                    let x = row.min.x + 36.0 + 14.0;
                    let room = place.min.x - 12.0 - x;
                    let title = format!("{} · {}", crate::later::label(skin), skin.look);
                    let style = Text::body(14.0, 600, tokens.text);
                    let shown = fonts::ellipsize(ui.painter(), &title, style, room);
                    fonts::draw(ui.painter(), Pos2::new(x, row.min.y), &shown, style);
                    super::label_node(
                        ui,
                        Rect::from_min_max(
                            Pos2::new(x, row.min.y),
                            Pos2::new(place.min.x, row.max.y),
                        ),
                        ("later", index),
                        &title,
                    );
                    let sub = fmt(
                        tl!("Goes on by itself once the game downloads M{n} · from {pack}"),
                        &[
                            ("n", &skin.modification.to_string()),
                            ("pack", &skin.pack_name),
                        ],
                    );
                    let style = Text::body(12.0, 400, tokens.text_3);
                    let shown = fonts::ellipsize(ui.painter(), &sub, style, room);
                    fonts::draw(ui.painter(), Pos2::new(x, row.min.y + 21.0), &shown, style);
                }
            });
    });
    let note_rect = Rect::from_min_size(
        Pos2::new(inner.min.x, list.max.y + 12.0),
        Vec2::new(inner.width(), note_height),
    );
    draw::rounded(
        ui.painter(),
        note_rect,
        egui::CornerRadius::same(10),
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
    if let Some(skin) = remove {
        app.remove_later(&skin);
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
                    let name = item_name(&item.target.kind, &item.target.id, &item.target.grade);
                    let meta = tn(item.variants.len() as u64, "{n} variant", "{n} variants");
                    let icon = super::garage::kind_icon(&item.target.kind);
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
