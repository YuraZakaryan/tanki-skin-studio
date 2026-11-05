// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Item: the item in focus (in 3D as in the Garage, or the whole tank with its parts), its
//! modifications in the cache, matching skin variants, the pack the skins come from, and the form
//! to label an unrecognised resource by hand.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Role, Sense, Ui, Vec2};
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
    app::{Page, Selection, Studio},
    model::{ResourceRow, grade_code, item_name, marker_id, target_title},
    preview3d::TankParts,
};

/// The overview's model and facts stand side by side from this width; narrower, one above the other.
const SIDE_BY_SIDE: f32 = 640.0;
/// The facts' column beside the model.
const FACTS_WIDTH: f32 = 300.0;
/// The model's height when the facts are under it.
const STACKED_STAGE: f32 = 240.0;
/// The overview's height: a header, the model (and the facts, beside or under it: the tank's
/// three part cards, or an item's name, chips and note), margins.
fn overview_height(width: f32, tank: bool) -> f32 {
    let facts = if tank { 196.0 } else { 124.0 };
    let body = if width - 36.0 >= SIDE_BY_SIDE {
        280.0
    } else {
        STACKED_STAGE + 16.0 + facts
    };
    widgets::HEADER_HEIGHT + 18.0 + body + 18.0
}

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    // A page of the Garage (its Details), or of Resources when a resource is being labelled:
    // Back goes where it came from, and the title names the item.
    let from = if app.label_resource.is_some() {
        Page::Resources
    } else {
        Page::Garage
    };
    let parts = app
        .selection
        .as_ref()
        .filter(|_| app.label_resource.is_none())
        .and_then(|selection| app.tank_parts(selection));
    let (title, subtitle) = match (&app.selection, &parts) {
        (Some(_), Some(parts)) => (
            format!(
                "{} + {}",
                target_title(&parts.hull.id),
                target_title(&parts.turret.id)
            ),
            tl!("Tank · pick a part for its modifications and skin variants").to_owned(),
        ),
        (Some(selection), None) if app.label_resource.is_none() => (
            crate::model::target_title(&selection.id),
            fmt(
                tl!("{kind} · modifications, skin variants, pack and labeling"),
                &[("kind", super::garage::kind_label(&selection.kind))],
            ),
        ),
        _ => (
            tl!("Item").to_owned(),
            tl!("Modifications, skin variants, pack and manual labeling").to_owned(),
        ),
    };
    let back = if from == Page::Resources {
        tl!("Back to Resources")
    } else {
        tl!("Back to Garage")
    };
    let go_back = widgets::top_bar_back(
        ui,
        shell::top_bar_rect(main),
        &title,
        &subtitle,
        back,
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
    if go_back {
        app.go(from);
    }
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let wide = super::is_wide(body.shrink2(Vec2::new(32.0, 0.0)));
    // The item's panel: its 3D overview for a selection, the short facts while labelling.
    let content_width = body.width() - 64.0;
    let item_height = if app.selection.is_some() && app.label_resource.is_none() {
        overview_height(
            if wide {
                content_width - 440.0 - 24.0
            } else {
                content_width
            },
            parts.is_some(),
        )
    } else {
        193.0
    };
    let min_height = if wide {
        764.0 - 193.0 + item_height + 64.0
    } else {
        32.0 + item_height + 24.0 + 291.0 + 24.0 + 232.0 + 24.0 + 209.0 + 24.0 + 420.0 + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        let (item, mods, skins, source_rect, label) = if wide {
            let cols = columns(content, 24.0, &[Size::Fill(1.0), Size::Fixed(440.0)]);
            let left = rows(
                cols[0],
                24.0,
                &[
                    Size::Fixed(item_height),
                    Size::Fixed(291.0),
                    Size::Fill(1.0),
                ],
            );
            let right = rows(cols[1], 24.0, &[Size::Fixed(209.0), Size::Fill(1.0)]);
            (left[0], left[1], left[2], right[0], right[1])
        } else {
            let all = rows(
                content,
                24.0,
                &[
                    Size::Fixed(item_height),
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
    if app.label_resource.is_none()
        && let Some(selection) = app.selection.clone()
    {
        return overview(app, ui, rect, &selection);
    }
    let tokens = Tokens::get(ui.ctx());
    let label_row = labelled_row(app);
    let (icon, meta, title, kind_chip, state_chip, note) = if let Some(row) = &label_row {
        (
            icons::BOX,
            format!("#{}", row.id),
            format!("#{}", row.id),
            (ChipKind::Neutral, row.kind.clone()),
            (ChipKind::Warning, tl!("Not recognised").to_owned()),
            tl!("Give this resource an item id below to use skins for it.").to_owned(),
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
    fonts::icon(ui.painter(), thumb.center(), icon, 34.0, tokens.primary);
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

/// The item in focus above its modifications: in 3D as in the Garage (the same stage, view and
/// turning; its flat picture while 3D is off), and beside it what the item is. With the whole tank
/// on, the tank in 3D and its parts, the one picked shown in the panels below.
fn overview(app: &mut Studio, ui: &mut Ui, rect: Rect, selection: &Selection) {
    app.remember_tank_part(selection);
    let parts = app.tank_parts(selection);
    let (title, meta) = match &parts {
        Some(parts) => (
            tl!("My tank"),
            format!(
                "{} · {}",
                item_name("hull", &parts.hull.id, grade_code(parts.hull.grade)),
                item_name("turret", &parts.turret.id, grade_code(parts.turret.grade)),
            ),
        ),
        None => (
            tl!("Item"),
            format!("{} · {}", selection.kind, selection.id),
        ),
    };
    let body = widgets::panel(ui, rect, title, Some(&meta));
    let inner = body.shrink(18.0);
    let beside = inner.width() >= SIDE_BY_SIDE;
    let (stage, facts) = if beside {
        let cols = columns(inner, 24.0, &[Size::Fill(1.0), Size::Fixed(FACTS_WIDTH)]);
        (cols[0], cols[1])
    } else {
        let parts = rows(inner, 16.0, &[Size::Fixed(STACKED_STAGE), Size::Fill(1.0)]);
        (parts[0], parts[1])
    };
    model(app, ui, stage, selection);
    match parts {
        Some(parts) => part_cards(app, ui, facts, &parts, selection, beside),
        None => item_facts(app, ui, facts, selection, beside),
    }
}

/// The cache resource of a selection's modification, when the cache has it.
fn selection_row(app: &Studio, selection: &Selection) -> Option<ResourceRow> {
    let marker = app
        .items
        .iter()
        .find(|i| i.kind == selection.kind && i.id == selection.id)
        .and_then(|i| i.markers[selection.grade.min(3)].clone())?;
    app.rows.iter().find(|r| r.key == marker).cloned()
}

/// The model on a stage of its own, as in the Garage.
fn model(app: &mut Studio, ui: &mut Ui, stage: Rect, selection: &Selection) {
    let tokens = Tokens::get(ui.ctx());
    draw::radial(
        ui.painter(),
        stage,
        12.0,
        tokens.surface_3,
        tokens.stage,
        tokens.surface,
    );
    ui.painter().rect_stroke(
        stage,
        CornerRadius::same(12),
        egui::Stroke::new(1.0, tokens.border),
        egui::StrokeKind::Inside,
    );
    let backdrop = ui.painter().add(egui::Shape::Noop);
    let under = ui.painter().add(egui::Shape::Noop);
    let drawn = app.preview.active() && {
        let mut looks = super::preview3d::looks(app, selection);
        // Comparing with the game is the Garage's: here the look alone.
        looks.compared = None;
        super::preview3d::stage(
            app,
            ui,
            stage.shrink(16.0),
            stage.shrink(2.0),
            [backdrop, under],
            looks,
        )
    };
    if !drawn {
        let row = selection_row(app, selection);
        super::garage::flat_picture(app, ui, stage.shrink(16.0), selection, row.as_ref());
    }
}

/// What a single item is: its name, kind, whether the cache has it.
fn item_facts(app: &Studio, ui: &mut Ui, area: Rect, selection: &Selection, centred: bool) {
    let tokens = Tokens::get(ui.ctx());
    let found = selection_row(app, selection).is_some();
    let title = target_title(&selection.id);
    let title_style = Text::display(24.0, 700, tokens.text);
    let chips_height = 24.0;
    let note =
        tl!("Modifications of one item sit side by side and usually share the same model size.");
    let galley = fonts::paragraph(
        ui.painter(),
        note,
        Text::body(13.0, 400, tokens.text_2),
        area.width(),
        Some(20.0),
    );
    // Centred beside the model; under it, from the top.
    let height = 32.0 + 12.0 + chips_height + 16.0 + galley.size().y;
    let top = area.min.y
        + if centred {
            ((area.height() - height) / 2.0).max(0.0)
        } else {
            0.0
        };
    let used = fonts::draw(
        ui.painter(),
        Pos2::new(area.min.x, top),
        &title,
        title_style,
    );
    super::label_node(ui, used, "item-title", &title);
    let chips_y = top + 32.0 + 12.0;
    let kind = super::garage::kind_label(&selection.kind);
    let first = widgets::chip_width(ui, kind);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(
            Pos2::new(area.min.x, chips_y),
            Vec2::new(first, chips_height),
        ),
        ChipKind::Info,
        kind,
    );
    let (chip, state) = if found {
        (ChipKind::Success, tl!("Recognised in cache"))
    } else {
        (ChipKind::Neutral, tl!("Not in cache"))
    };
    let second = widgets::chip_width(ui, state);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(
            Pos2::new(area.min.x + first + 8.0, chips_y),
            Vec2::new(second, chips_height),
        ),
        chip,
        state,
    );
    ui.painter().galley(
        Pos2::new(area.min.x, chips_y + chips_height + 16.0),
        galley,
        tokens.text_2,
    );
}

/// The tank's parts, one card each, the picked one marked: picking another shows it in the panels
/// below.
fn part_cards(
    app: &mut Studio,
    ui: &mut Ui,
    area: Rect,
    parts: &TankParts,
    selection: &Selection,
    centred: bool,
) {
    let cards: Vec<&Selection> = [Some(&parts.hull), Some(&parts.turret), parts.paint.as_ref()]
        .into_iter()
        .flatten()
        .collect();
    let height = |part: &Selection| if part.kind == "paint" { 52.0 } else { 64.0 };
    let total: f32 = cards.iter().map(|p| height(p)).sum::<f32>() + 8.0 * (cards.len() - 1) as f32;
    let mut y = area.min.y
        + if centred {
            ((area.height() - total) / 2.0).max(0.0)
        } else {
            0.0
        };
    let mut picked = None;
    for part in cards {
        let card = Rect::from_min_size(
            Pos2::new(area.min.x, y),
            Vec2::new(area.width(), height(part)),
        );
        if part_card(app, ui, card, part, part == selection) {
            picked = Some(part.clone());
        }
        y += height(part) + 8.0;
    }
    if let Some(part) = picked {
        app.selection = Some(part);
    }
}

/// One part of the tank: its kind's icon (a paint's own swatch), name, kind and modification, and
/// whether the cache has it. Returns whether it was clicked.
fn part_card(app: &mut Studio, ui: &mut Ui, rect: Rect, part: &Selection, picked: bool) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let paint = part.kind == "paint";
    let name = target_title(&part.id);
    let kind = super::garage::kind_label(&part.kind);
    let label = format!("{kind}: {name}");
    let response = ui.interact(
        rect,
        ui.scope_id().with(("tank-part", &part.kind)),
        Sense::click(),
    );
    let accessible = label.clone();
    response.widget_info(move || {
        egui::WidgetInfo::selected(Role::Button, true, picked, accessible.clone())
    });
    let hovered = response.hovered();
    let (fill, stroke) = if picked {
        (tokens.surface_2, (1.5, tokens.primary))
    } else if hovered {
        (tokens.surface_2, (1.0, tokens.border_strong))
    } else {
        (tokens.surface, (1.0, tokens.border))
    };
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        fill,
        Some(stroke),
    );
    if hovered && !picked {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let side = rect.height() - 24.0;
    let icon = Rect::from_min_size(
        Pos2::new(rect.min.x + 12.0, rect.center().y - side / 2.0),
        Vec2::splat(side),
    );
    let row = selection_row(app, part);
    let swatch = paint
        .then(|| row.as_ref().and_then(|r| r.preview_group.clone()))
        .flatten()
        .and_then(|group| app.cache_preview(&ui.ctx().clone(), &group));
    match swatch {
        Some(texture) => {
            egui::Image::new(&texture)
                .corner_radius(CornerRadius::same(8))
                .paint_at(ui, icon);
        }
        None => {
            draw::rounded(
                ui.painter(),
                icon,
                CornerRadius::same(10),
                tokens.stage,
                None,
            );
            fonts::icon(
                ui.painter(),
                icon.center(),
                super::garage::kind_icon(&part.kind),
                side * 0.5,
                if picked {
                    tokens.primary
                } else {
                    tokens.text_2
                },
            );
        }
    }
    let x = icon.max.x + 12.0;
    let (chip, state) = if row.is_some() {
        (ChipKind::Success, tl!("Found"))
    } else {
        (ChipKind::Neutral, tl!("Not in cache"))
    };
    let chip_width = widgets::chip_width(ui, state);
    let chip_rect = Rect::from_min_size(
        Pos2::new(rect.max.x - 12.0 - chip_width, rect.center().y - 12.0),
        Vec2::new(chip_width, 24.0),
    );
    widgets::paint_chip(ui, chip_rect, chip, state);
    let room = chip_rect.min.x - 8.0 - x;
    let title_style = Text::body(15.0, 600, tokens.text);
    let sub_style = Text::body(12.0, 400, tokens.text_3);
    let sub = if paint {
        kind.to_owned()
    } else {
        format!("{kind} · {}", grade_code(part.grade).to_uppercase())
    };
    let (title_y, sub_y) = (rect.center().y - 9.0, rect.center().y + 10.0);
    fonts::draw_left(
        ui.painter(),
        x,
        title_y,
        &fonts::ellipsize(ui.painter(), &name, title_style, room),
        title_style,
    );
    fonts::draw_left(
        ui.painter(),
        x,
        sub_y,
        &fonts::ellipsize(ui.painter(), &sub, sub_style, room),
        sub_style,
    );
    response.clicked()
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
        // A paint is one picture, with no modifications.
        let count = if selection.kind == "paint" { 1 } else { 4 };
        (0..count)
            .map(|i| {
                (
                    item_name(&selection.kind, &selection.id, grade_code(i)),
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
    // A modification not in the cache may wait for a skin (see `crate::later`).
    let root = app
        .scan
        .as_ref()
        .map(|s| s.root.to_string_lossy().into_owned())
        .unwrap_or_default();
    let waiting: Vec<Option<tanki_core::later::LaterSkin>> = (0..4u8)
        .map(|k| {
            let selection = app.selection.as_ref().filter(|_| label_row.is_none());
            selection.and_then(|selection| {
                crate::later::waiting(&app.later, &root, &selection.kind, &selection.id, k).cloned()
            })
        })
        .collect();
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
            let later_text;
            match row {
                Some(row) => {
                    size_text = tn(row.files as u64, "{n} file", "{n} files");
                    entry = entry.chip(ChipKind::Success, tl!("Found")).size(&size_text);
                }
                None => {
                    entry = entry.chip(ChipKind::Neutral, tl!("Not in cache")).size("—");
                    if let Some(skin) = waiting.get(index).and_then(Option::as_ref) {
                        later_text = fmt(tl!("Later: {look}"), &[("look", &skin.look)]);
                        entry = entry.note(ChipKind::Info, icons::CLOCK_3, &later_text);
                    }
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
            item_name(&s.kind, &s.id, grade_code(s.grade))
        )),
        _ => None,
    };
    let body = widgets::panel(ui, rect, tl!("Skin variants"), meta.as_deref());
    let inner = body.shrink(18.0);
    let Some(selection) = selection else {
        return;
    };
    let options = app.pack_variants(&selection.kind, &selection.id);
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
    let applied_now = app.applied_variant(&selection);
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
                    let in_game = applied_now == Some((*item, *variant));
                    let (response, applied) = widgets::skin_row(
                        ui,
                        inner.width(),
                        &v.name,
                        &meta,
                        selected,
                        (selected && !in_game).then(|| tl!("Apply")),
                        in_game.then(|| (ChipKind::Success, tl!("Applied"))),
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
    let whole = tl!("Label all modifications M0–M3 (adjacent resources)");
    ui.add_enabled_ui(active, |ui| {
        in_rect(ui, toggle_rect, |ui| {
            ui.horizontal(|ui| {
                widgets::toggle(ui, &mut app.label_whole, whole).on_hover_text(whole);
            });
        });
    });
    // The switch's caption, shortened with "…" in a narrow panel (the switch keeps the full name).
    let style = Text::body(13.0, 400, tokens.text_2);
    let caption = fonts::ellipsize(ui.painter(), whole, style, toggle_rect.width() - 52.0);
    let caption_painter = ui.painter().with_clip_rect(toggle_rect);
    fonts::draw_left(
        &caption_painter,
        toggle_rect.min.x + 52.0,
        toggle_rect.center().y,
        &caption,
        style,
    );
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
