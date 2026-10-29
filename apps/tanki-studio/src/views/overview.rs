//! Overview: cache numbers, the active pack, the pack library and recent resources.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Ui, Vec2};
use tanki_i18n::{fmt, tl};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect, rows},
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, ListRow},
};

use super::{scroll_page, shell};
use crate::{
    app::Studio,
    model::{format_bytes, target_name, thousands},
};

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    let subtitle = tl!("Game cache · active pack · restore");
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Overview"),
        subtitle,
        |ui| {
            if Button::new(ButtonKind::Primary, tl!("Import ZIP"))
                .icon(icons::UPLOAD)
                .show(ui)
                .clicked()
            {
                app.import(false);
            }
            if Button::new(ButtonKind::Secondary, tl!("Choose folder"))
                .icon(icons::FOLDER_OPEN)
                .show(ui)
                .clicked()
            {
                app.choose_cache();
            }
        },
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let inner = body.shrink2(Vec2::new(32.0, 0.0));
    let wide = super::is_wide(inner);
    let two_by_two = inner.width() < 700.0;
    let stats_height = if two_by_two {
        116.0 * 2.0 + 16.0
    } else {
        116.0
    };
    let min_height = if wide {
        828.0
    } else {
        32.0 + stats_height + 24.0 + 214.0 + 24.0 + 170.0 + 24.0 + 320.0 + 24.0 + 320.0 + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        let parts = rows(content, 24.0, &[Size::Fixed(stats_height), Size::Fill(1.0)]);
        stats(app, ui, parts[0], two_by_two);
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        let (pack, list, drop, recent_rect) = if wide {
            let cols = columns(parts[1], 24.0, &[Size::Fill(1.0), Size::Fixed(400.0)]);
            let left = rows(cols[0], 24.0, &[Size::Fixed(214.0), Size::Fill(1.0)]);
            let right = rows(cols[1], 24.0, &[Size::Fixed(170.0), Size::Fill(1.0)]);
            (left[0], left[1], right[0], right[1])
        } else {
            let all = rows(
                parts[1],
                24.0,
                &[
                    Size::Fixed(214.0),
                    Size::Fixed(170.0),
                    Size::Fixed(320.0),
                    Size::Fixed(320.0),
                ],
            );
            (all[0], all[2], all[1], all[3])
        };
        active_pack(app, ui, pack);
        pack_list(app, ui, list);
        if widgets::dropzone(
            ui,
            drop,
            tl!("Drop a .tankipack here"),
            tl!(".tankipack, .zip or a folder with manifest.json"),
            hovering,
        )
        .clicked()
        {
            app.import(false);
        }
        recent(app, ui, recent_rect);
    });
}

fn stats(app: &Studio, ui: &mut Ui, rect: Rect, two_by_two: bool) {
    let cells: Vec<Rect> = if two_by_two {
        rows(rect, 16.0, &[Size::Fixed(116.0); 2])
            .into_iter()
            .flat_map(|row| columns(row, 16.0, &[Size::Fill(1.0); 2]))
            .collect()
    } else {
        columns(rect, 16.0, &[Size::Fill(1.0); 4])
    };
    let scan = app.scan.as_ref();
    let files = scan.map_or(0, |s| s.resources.len());
    let recognised = app.rows.iter().filter(|r| r.target.is_some()).count();
    widgets::stat_card(
        ui,
        cells[0],
        tl!("Files in cache"),
        &thousands(files),
        &fmt(
            tl!("index updated {when}"),
            &[("when", &shell::describe_age(app.scan_age()))],
        ),
    );
    widgets::stat_card(
        ui,
        cells[1],
        tl!("Resource groups"),
        &thousands(app.rows.len()),
        &fmt(
            tl!("{n} items recognised"),
            &[("n", &thousands(recognised))],
        ),
    );
    let active = app.active_pack.as_ref().map_or_else(
        || tl!("no active pack").to_owned(),
        |p| fmt(tl!("active: {name}"), &[("name", &p.manifest.name)]),
    );
    widgets::stat_card(
        ui,
        cells[2],
        tl!("Packs"),
        &thousands(app.packs.len()),
        &active,
    );
    let latest = app.backups.first().map_or_else(
        || tl!("none yet").to_owned(),
        |b| {
            fmt(
                tl!("latest: {when}"),
                &[("when", &shell::describe_time(b.created_unix))],
            )
        },
    );
    widgets::stat_card(
        ui,
        cells[3],
        tl!("Backups"),
        &thousands(app.backups.len()),
        &latest,
    );
}

fn active_pack(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let Some(pack) = app.active_pack.clone() else {
        let body = widgets::panel(ui, rect, tl!("Active pack"), None);
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(
                body.center() - Vec2::new(0.0, 22.0),
                Vec2::new(body.width(), 22.0),
            ),
            tl!("Add your first pack"),
            Text::display(18.0, 700, tokens.text),
        );
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(
                body.center() + Vec2::new(0.0, 4.0),
                Vec2::new(body.width(), 16.0),
            ),
            tl!("The last imported pack becomes active automatically."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let version = fmt(tl!("version {v}"), &[("v", &pack.manifest.version)]);
    let body = widgets::panel(ui, rect, tl!("Active pack"), Some(&version));
    let inner = body.shrink(18.0);
    let summary = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 81.0));
    let thumb = Rect::from_min_size(summary.min + Vec2::new(0.0, 3.0), Vec2::splat(76.0));
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
        icons::PACKAGE,
        30.0,
        tokens.primary,
    );
    let x = summary.min.x + 76.0 + 16.0;
    // Long names and author lines are shortened to the panel instead of running out of it.
    let room = summary.max.x - x;
    let name_style = Text::display(22.0, 700, tokens.text);
    let name = fonts::ellipsize(ui.painter(), &pack.manifest.name, name_style, room);
    fonts::draw(ui.painter(), Pos2::new(x, summary.min.y), &name, name_style);
    let items: Vec<String> = pack
        .manifest
        .items
        .iter()
        .map(|i| target_name(&i.target.id, &i.target.grade))
        .collect();
    let author = if pack.manifest.author.trim().is_empty() {
        String::new()
    } else {
        fmt(
            tl!("Author: {author}"),
            &[("author", &pack.manifest.author)],
        )
    };
    let license = fmt(
        tl!("License {license}"),
        &[("license", &pack.manifest.license)],
    );
    let items = items.join(", ");
    let meta = crate::model::dotted([author.as_str(), license.as_str(), items.as_str()]);
    let meta_style = Text::body(13.0, 400, tokens.text_2);
    let meta = fonts::ellipsize(ui.painter(), &meta, meta_style, room);
    fonts::draw(
        ui.painter(),
        Pos2::new(x, summary.min.y + 35.0),
        &meta,
        meta_style,
    );
    let chips_y = summary.min.y + 57.0;
    let mut cx = x;
    let active_label = tl!("Active");
    let width = widgets::chip_width(ui, active_label);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(Pos2::new(cx, chips_y), Vec2::new(width, 24.0)),
        ChipKind::Success,
        active_label,
    );
    cx += width + 8.0;
    for game in &pack.manifest.games {
        let label = shell::game_short_name(game);
        let width = widgets::chip_width(ui, &label);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(cx, chips_y), Vec2::new(width, 24.0)),
            ChipKind::Info,
            &label,
        );
        cx += width + 8.0;
    }
    let actions = Rect::from_min_size(
        Pos2::new(inner.min.x, summary.max.y + 12.0),
        Vec2::new(inner.width(), 40.0),
    );
    in_rect(ui, actions, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let can_apply = app.chosen_apply_available();
            let apply = Button::new(ButtonKind::Accent, tl!("Apply to cache"))
                .icon(icons::ZAP)
                .enabled(can_apply)
                .show(ui);
            if apply.clicked() {
                app.begin_apply_selected();
            }
            let restore = Button::new(ButtonKind::Danger, tl!("Restore original"))
                .icon(icons::ROTATE_CCW)
                .enabled(app.can_restore())
                .show(ui);
            if restore.clicked() {
                app.restore_last();
            }
        });
    });
}

fn pack_list(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let variants: usize = app
        .packs
        .iter()
        .map(|p| {
            p.manifest
                .items
                .iter()
                .map(|i| i.variants.len())
                .sum::<usize>()
        })
        .sum();
    let meta = fmt(
        tl!("{packs} · {variants}"),
        &[
            (
                "packs",
                &tanki_i18n::tn(app.packs.len() as u64, "{n} pack", "{n} packs"),
            ),
            (
                "variants",
                &tanki_i18n::tn(variants as u64, "{n} variant", "{n} variants"),
            ),
        ],
    );
    let body = widgets::panel(ui, rect, tl!("Packs"), Some(&meta));
    let list = body.shrink2(Vec2::new(8.0, 8.0));
    let active = app
        .active_pack
        .as_ref()
        .map(|p| (p.manifest.id.clone(), p.manifest.version.clone()));
    let packs = app.packs.clone();
    let mut chosen = None;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("overview-1")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (index, pack) in packs.iter().enumerate() {
                    let count: usize = pack.manifest.items.iter().map(|i| i.variants.len()).sum();
                    let meta = fmt(
                        tl!("{variants} · v{v}"),
                        &[
                            (
                                "variants",
                                &tanki_i18n::tn(count as u64, "{n} variant", "{n} variants"),
                            ),
                            ("v", &pack.manifest.version),
                        ],
                    );
                    let is_active = active.as_ref()
                        == Some(&(pack.manifest.id.clone(), pack.manifest.version.clone()));
                    let row = ListRow::new(icons::PACKAGE, &pack.manifest.name, &meta)
                        .selected(is_active)
                        .chip(
                            if is_active {
                                ChipKind::Success
                            } else {
                                ChipKind::Neutral
                            },
                            if is_active {
                                tl!("Active")
                            } else {
                                tl!("In library")
                            },
                        );
                    if row.show(ui).clicked() {
                        chosen = Some(index);
                    }
                }
            });
    });
    if let Some(index) = chosen {
        app.select_pack(index);
    }
}

fn recent(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let body = widgets::panel(ui, rect, tl!("Recent resources"), None);
    let recent_keys = app.store.recent_assets(12).unwrap_or_default();
    let shown: Vec<_> = recent_keys
        .iter()
        .filter_map(|key| {
            let marker = tanki_core::targets::marker_for_group(key).unwrap_or_else(|| key.clone());
            app.rows.iter().find(|r| r.key == marker).cloned()
        })
        .take(8)
        .collect();
    if shown.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width() - 32.0, 16.0)),
            tl!("Resources you open appear here."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    let list = body.shrink2(Vec2::new(8.0, 8.0));
    let mut open = None;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("overview-2")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for row in &shown {
                    let meta = tanki_i18n::tn(row.files as u64, "{n} file", "{n} files");
                    let title = row.title();
                    if ListRow::new(icons::IMAGE, &title, &meta)
                        .size(&format_bytes(row.bytes))
                        .show(ui)
                        .clicked()
                    {
                        open = Some(row.key.clone());
                    }
                }
            });
    });
    if let Some(key) = open {
        app.open_row(&key);
    }
}
