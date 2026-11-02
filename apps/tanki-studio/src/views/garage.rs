// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Garage: the selected item with its modifications and skin variants, and every item of the cache.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use tanki_core::{
    apply::{cache_skin_files, missing_from},
    pack::PackTarget,
};
use tanki_i18n::{fmt, tl, tl_ctx, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::{Size, columns, in_rect, rows},
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ChipKind, Cover, Width},
};

use super::{scroll_page, shell};
use crate::{
    app::{Page, Selection, StateFilter, Studio, TABS},
    model::{grade_code, target_name, target_title},
};

pub fn kind_label(kind: &str) -> &'static str {
    match kind {
        "hull" => tl!("Hull"),
        "turret" => tl!("Turret"),
        "paint" => tl!("Paint"),
        _ => tl!("Item"),
    }
}

/// The icon of a kind of item: a tank for hulls, a crosshair for turrets, a brush for paints.
pub fn kind_icon(kind: &str) -> char {
    match kind {
        "turret" => icons::CROSSHAIR,
        "paint" => icons::PAINTBRUSH,
        _ => icons::TANK,
    }
}

fn tab_label(kind: &str) -> &'static str {
    match kind {
        "hull" => tl!("Hulls"),
        "turret" => tl!("Turrets"),
        _ => tl!("Paints"),
    }
}

/// An item card: the stage, then the name and the kind.
const CARD_HEIGHT: f32 = 152.0;

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Garage"),
        tl!("Client items and skins from the active pack"),
        |ui| {
            if Button::new(ButtonKind::Ghost, tl!("Restore"))
                .icon(icons::ROTATE_CCW)
                .enabled(app.can_restore())
                .show(ui)
                .clicked()
            {
                app.ask_to_restore();
            }
        },
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let wide = super::is_wide(body.shrink2(Vec2::new(32.0, 0.0)));
    let done = app.later_done.clone();
    let bar = if done.is_some() { 48.0 + 16.0 } else { 0.0 };
    let min_height = bar
        + if wide {
            828.0
        } else {
            32.0 + 420.0 + 24.0 + 440.0 + 24.0 + 320.0 + 32.0
        };
    scroll_page(ui, body, min_height, |ui, rect| {
        let mut content = rect.shrink(32.0);
        if let Some(done) = &done {
            let strip = Rect::from_min_size(content.min, Vec2::new(content.width(), 48.0));
            later_bar(app, ui, strip, done);
            content.min.y += bar;
        }
        if wide {
            let parts = rows(content, 24.0, &[Size::Fixed(420.0), Size::Fill(1.0)]);
            let top = columns(parts[0], 24.0, &[Size::Fill(1.0), Size::Fixed(440.0)]);
            my_tank(app, ui, top[0]);
            variants(app, ui, top[1]);
            items(app, ui, parts[1]);
        } else {
            let all = rows(
                content,
                24.0,
                &[Size::Fixed(420.0), Size::Fixed(440.0), Size::Fill(1.0)],
            );
            my_tank(app, ui, all[0]);
            variants(app, ui, all[1]);
            items(app, ui, all[2]);
        }
    });
}

/// The skins that went on by themselves: what went on where, with Undo and ×.
fn later_bar(app: &mut Studio, ui: &mut Ui, rect: Rect, done: &crate::app::LaterDone) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        tokens.primary_soft,
        Some((1.0, tokens.primary)),
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 16.0 + 9.0, rect.center().y),
        icons::CIRCLE_CHECK,
        18.0,
        tokens.primary,
    );
    let items = match done.items.as_slice() {
        [one] => one.clone(),
        [first, rest @ ..] => fmt(
            tl!("{item} and {n} more"),
            &[("item", first), ("n", &rest.len().to_string())],
        ),
        [] => String::new(),
    };
    let title = fmt(
        tl!("{look} is on {items}"),
        &[("look", &done.look), ("items", &items)],
    );
    let message = match done.items.as_slice() {
        [one] => fmt(
            tl!(
                "{modification} appeared in the cache and the Studio put the waiting skin on. Backup saved."
            ),
            &[("modification", one.rsplit(' ').next().unwrap_or_default())],
        ),
        _ => {
            tl!("They appeared in the cache and the Studio put the waiting skins on. Backup saved.")
                .to_owned()
        }
    };
    // Right to left: ×, then Undo.
    let close = Rect::from_min_size(
        Pos2::new(rect.max.x - 6.0 - 38.0, rect.center().y - 19.0),
        Vec2::splat(38.0),
    );
    let close_clicked = in_rect(ui, close, |ui| {
        widgets::icon_button(ui, icons::X, tl!("Close")).clicked()
    });
    let undo = Button::new(ButtonKind::Secondary, tl!("Undo")).icon(icons::UNDO_2);
    let undo_width = undo.measure(ui);
    let undo_rect = Rect::from_min_size(
        Pos2::new(close.min.x - 12.0 - undo_width, rect.center().y - 20.0),
        Vec2::new(undo_width, 40.0),
    );
    let undo_clicked = in_rect(ui, undo_rect, |ui| undo.show(ui).clicked());
    let title_style = Text::body(14.0, 700, tokens.text);
    let x = rect.min.x + 16.0 + 18.0 + 12.0;
    let used = fonts::draw_left(ui.painter(), x, rect.center().y, &title, title_style);
    super::label_node(ui, used, "later-bar", &title);
    let message_style = Text::body(13.0, 400, tokens.text_2);
    let room = undo_rect.min.x - 12.0 - (used.max.x + 10.0);
    let shown = fonts::ellipsize(ui.painter(), &message, message_style, room.max(0.0));
    fonts::draw_left(
        ui.painter(),
        used.max.x + 10.0,
        rect.center().y,
        &shown,
        message_style,
    );
    if undo_clicked {
        app.undo_later();
    } else if close_clicked {
        app.dismiss_later_done();
    }
}

fn my_tank(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let Some(selection) = app.selection.clone() else {
        let body = widgets::panel(ui, rect, tl!("My tank"), None);
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width(), 18.0)),
            tl!("Pick an item below to see its modifications."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let meta = format!(
        "{} · {}",
        target_title(&selection.id),
        grade_code(selection.grade).to_uppercase()
    );
    let body = widgets::panel(ui, rect, tl!("My tank"), Some(&meta));
    let stage = body.shrink(14.0);
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
    let marker = app
        .items
        .iter()
        .find(|i| i.kind == selection.kind && i.id == selection.id)
        .and_then(|i| i.markers[selection.grade].clone());
    let row = marker
        .as_ref()
        .and_then(|m| app.rows.iter().find(|r| &r.key == m))
        .cloned();
    // Chips: recognised + revision.
    let mut x = stage.min.x + 18.0;
    let y = stage.min.y + 18.0;
    let recognised = fmt(
        tl!("{name} recognised"),
        &[(
            "name",
            &target_name(&selection.id, grade_code(selection.grade)),
        )],
    );
    let present = row.is_some();
    let (kind, label) = if present {
        (ChipKind::Success, recognised)
    } else {
        (
            ChipKind::Neutral,
            fmt(
                tl!("{name} not in cache"),
                &[(
                    "name",
                    &target_name(&selection.id, grade_code(selection.grade)),
                )],
            ),
        )
    };
    let width = widgets::chip_width(ui, &label);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
        kind,
        &label,
    );
    x += width + 8.0;
    // Details opens the item's page; the chips give way to it.
    let details = Button::new(ButtonKind::Ghost, tl!("Details"))
        .icon(icons::SCAN_SEARCH)
        .height(28.0);
    let details_width = details.measure(ui);
    let details_rect = Rect::from_min_size(
        Pos2::new(stage.max.x - 18.0 - details_width, y - 2.0),
        Vec2::new(details_width, 28.0),
    );
    let room = details_rect.min.x - 8.0;
    if let Some(row) = &row {
        let label = fmt(tl!("revision {r}"), &[("r", &row.revision)]);
        let width = widgets::chip_width(ui, &label);
        if x + width <= room {
            widgets::paint_chip(
                ui,
                Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
                ChipKind::Neutral,
                &label,
            );
            x += width + 8.0;
        }
    }
    if app
        .lost_skin(&selection.kind, &selection.id, selection.grade)
        .is_some()
    {
        let label = tl!("Removed by a game update");
        let width = widgets::chip_width(ui, label);
        if x + width <= room {
            let chip = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0));
            widgets::paint_chip(ui, chip, ChipKind::Warning, label);
        }
    }
    if in_rect(ui, details_rect, |ui| details.show(ui).clicked()) {
        app.go(Page::Info);
    }
    // Preview: the chosen variant's picture, else the cache texture, else a placeholder icon.
    let center = Rect::from_min_max(
        Pos2::new(stage.min.x + 18.0, stage.min.y + 54.0),
        Pos2::new(stage.max.x - 18.0, stage.max.y - 18.0 - 30.0 - 12.0),
    );
    let ctx = ui.ctx().clone();
    let variant_texture = app.chosen_variant(&selection).and_then(|(i, v)| {
        let pack = app.active_pack.clone()?;
        let variant = pack.manifest.items.get(i)?.variants.get(v)?.clone();
        app.variant_preview(&ctx, &pack, &variant)
    });
    // The picked skin's own picture when the pack has one, else the item as the game shows it.
    // The caption says whether that skin is in the game: a pick is only a pick until applied.
    let chosen = app.chosen_variant(&selection);
    let has_variant = chosen.is_some();
    let in_game = chosen.is_some() && chosen == app.applied_variant(&selection);
    let worn = app.in_game_name(&selection);
    let (texture, caption) = match (
        variant_texture,
        has_variant,
        row.as_ref().and_then(|r| r.picture_group.clone()),
    ) {
        (Some(texture), _, _) if in_game => (Some(texture), tl!("In the game now").to_owned()),
        (Some(texture), _, _) => (Some(texture), tl!("Skin preview · not applied").to_owned()),
        // A skin of the pack that came without a picture: say so instead of showing another picture.
        (None, true, _) => (None, tl!("This skin has no preview in the pack").to_owned()),
        (None, false, Some(group)) => (
            app.cache_preview(&ctx, &group),
            worn.map_or_else(
                || tl!("Current look in the game").to_owned(),
                |name| fmt(tl!("In the game: {name}"), &[("name", &name)]),
            ),
        ),
        (None, false, None) => (None, tl!("No preview available").to_owned()),
    };
    // The game downloads a modification's garage picture only when its own garage shows it: say
    // so, and how to get it, when the cache has none.
    let no_picture =
        row.is_some() && !has_variant && row.as_ref().is_some_and(|r| r.picture_group.is_none());
    match texture {
        Some(texture) => {
            let area = Rect::from_min_max(center.min, Pos2::new(center.max.x, center.max.y - 24.0));
            let size = texture.size_vec2();
            let scale = (area.width() / size.x).min(area.height() / size.y).min(2.0);
            let image = Rect::from_center_size(area.center(), size * scale);
            ui.painter().image(
                texture.id(),
                image,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            fonts::draw_centered(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(center.center().x, center.max.y - 8.0),
                    Vec2::new(center.width(), 16.0),
                ),
                &caption,
                Text::mono(12.0, 400, tokens.text_3),
            );
        }
        None => {
            let c = center.center();
            fonts::icon(
                ui.painter(),
                Pos2::new(c.x, c.y - 14.0),
                icons::IMAGE,
                72.0,
                tokens.text_3,
            );
            fonts::draw_centered(
                ui.painter(),
                Rect::from_center_size(Pos2::new(c.x, c.y + 44.0), Vec2::new(center.width(), 16.0)),
                &caption,
                Text::mono(12.0, 400, tokens.text_3),
            );
            if no_picture {
                let note = fmt(
                    tl!(
                        "The game has not cached a picture of {name} yet. Open it in the game's garage, then rescan the cache."
                    ),
                    &[(
                        "name",
                        &target_name(&selection.id, grade_code(selection.grade)),
                    )],
                );
                let width = (center.width() - 48.0).clamp(120.0, 420.0);
                let galley = fonts::paragraph_centered(
                    ui.painter(),
                    &note,
                    Text::body(12.0, 400, tokens.text_3),
                    width,
                    Some(17.0),
                );
                ui.painter()
                    .galley(Pos2::new(c.x, c.y + 62.0), galley, tokens.text_3);
            }
        }
    }
    // Modification selector, centred at the bottom.
    let label = tl!("Modification").to_uppercase();
    let label_style = Text::display(11.0, 700, tokens.text_3).spaced(1.5);
    let label_width = fonts::width(ui.painter(), &label, label_style);
    let total = label_width + 12.0 + 4.0 * 52.0 + 3.0 * 12.0;
    let start = stage.center().x - total / 2.0;
    let selector_y = stage.max.y - 18.0 - 30.0;
    fonts::draw_left(ui.painter(), start, selector_y + 15.0, &label, label_style);
    let present_grades = app
        .items
        .iter()
        .find(|i| i.kind == selection.kind && i.id == selection.id)
        .map_or([false; 4], |i| i.present);
    for (grade, &is_present) in present_grades.iter().enumerate() {
        let chip = Rect::from_min_size(
            Pos2::new(start + label_width + 12.0 + grade as f32 * 64.0, selector_y),
            Vec2::new(52.0, 30.0),
        );
        let response = ui.interact(
            chip,
            ui.scope_id().with(("garage-grade", grade)),
            Sense::click(),
        );
        widgets::grade_chip_paint(
            ui,
            chip,
            &format!("M{grade}"),
            grade == selection.grade,
            is_present,
            response.hovered(),
        );
        if response.clicked() {
            app.selection = Some(Selection {
                grade,
                ..selection.clone()
            });
        }
    }
}

fn variants(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let pack_name = app.active_pack.as_ref().map(|p| p.manifest.name.clone());
    let body = widgets::panel(ui, rect, tl!("Skin variants"), pack_name.as_deref());
    let inner = body.shrink(18.0);
    let Some(selection) = app.selection.clone() else {
        return;
    };
    let options = app.pack_variants(&selection.kind, &selection.id);
    let chosen = app.chosen_variant(&selection);
    let applied_now = app.applied_variant(&selection);
    let in_game = chosen.is_some() && chosen == applied_now;
    let target = target_name(&selection.id, grade_code(selection.grade));
    // The model and texture files the selected item has in the cache: a skin that does not bring
    // every one of them cannot be applied (the preview is optional).
    let cache_files = app
        .scan
        .as_ref()
        .map(|scan| {
            cache_skin_files(
                scan,
                &PackTarget {
                    kind: selection.kind.clone(),
                    id: selection.id.clone(),
                    grade: grade_code(selection.grade).to_owned(),
                },
            )
        })
        .unwrap_or_default();
    let missing = |app: &Studio, (item, variant): (usize, usize)| {
        app.pack_variant(item, variant)
            .map(|(_, v)| missing_from(&cache_files, v))
            .unwrap_or_default()
    };
    let chosen_missing = chosen.map(|c| missing(app, c)).unwrap_or_default();
    let blocked = (!chosen_missing.is_empty()).then(|| {
        fmt(
            tl!("Can't apply: this skin has no {files} in the pack. Every file except the preview is needed."),
            &[("files", &chosen_missing.join(", "))],
        )
    });
    // Bottom stack: wide Apply button and a hint (two lines when it says why it cannot apply).
    let hint_y = inner.max.y - 15.0 - if blocked.is_some() { 17.0 } else { 0.0 };
    let apply = Rect::from_min_size(
        Pos2::new(inner.min.x, hint_y - 10.0 - 46.0),
        Vec2::new(inner.width(), 46.0),
    );
    let list = Rect::from_min_max(inner.min, Pos2::new(inner.max.x, apply.min.y - 11.0));
    if options.is_empty() {
        let text = if app.active_pack.is_some() {
            fmt(
                tl!("The active pack has no skins for {name}."),
                &[("name", &target)],
            )
        } else {
            tl!("Import a pack to see skins here.").to_owned()
        };
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(list.center(), Vec2::new(list.width(), 18.0)),
            &text,
            Text::body(13.0, 400, tokens.text_3),
        );
    }
    let mut picked = None;
    let mut apply_clicked = false;
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("garage-1")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 10.0;
                for (item, variant) in &options {
                    let Some((pack_item, v)) = app.pack_variant(*item, *variant) else {
                        continue;
                    };
                    let resources: Vec<&str> =
                        v.resources.iter().map(|r| r.role.as_str()).collect();
                    let meta = format!(
                        "{} · {}",
                        pack_item.target.grade.to_uppercase(),
                        resources.join(" + ")
                    );
                    let selected = chosen == Some((*item, *variant));
                    let incomplete = !missing(app, (*item, *variant)).is_empty();
                    // The skin in the game now says so; a skin that cannot be applied says why.
                    let badge = if applied_now == Some((*item, *variant)) {
                        Some((ChipKind::Success, tl!("Applied")))
                    } else {
                        incomplete.then(|| (ChipKind::Danger, tl!("Incomplete")))
                    };
                    let (response, applied) =
                        widgets::skin_row(ui, list.width(), &v.name, &meta, selected, None, badge);
                    // Bring the chosen variant into view when the choice changes (another item, a
                    // saved choice further down the list), never while the user scrolls.
                    if selected {
                        let shown = egui::Id::unique("garage-variant-in-view");
                        let key = format!(
                            "{}/{}",
                            selection.key(),
                            app.variant_key(*item, *variant).unwrap_or_default()
                        );
                        if ui.ctx().data(|d| d.get_temp::<String>(shown)) != Some(key.clone()) {
                            response.scroll_to_me(Some(egui::Align::Center));
                            ui.ctx().data_mut(|d| d.insert_temp(shown, key));
                        }
                    }
                    if response.clicked() {
                        picked = app.variant_key(*item, *variant);
                    }
                    apply_clicked |= applied;
                }
            });
    });
    in_rect(ui, apply, |ui| {
        let label = fmt(tl!("Apply to {name}"), &[("name", &target)]);
        let response = Button::new(ButtonKind::Accent, &label)
            .icon(icons::ZAP)
            .width(Width::Fill)
            .height(46.0)
            .enabled(chosen.is_some() && !in_game && blocked.is_none())
            .show(ui);
        apply_clicked |= response.clicked();
    });
    match &blocked {
        Some(reason) => {
            fonts::icon(
                ui.painter(),
                Pos2::new(inner.min.x + 7.0, hint_y + 8.0),
                icons::TRIANGLE_ALERT,
                14.0,
                tokens.danger,
            );
            let text = fonts::paragraph(
                ui.painter(),
                reason,
                Text::body(12.0, 400, tokens.danger),
                inner.width() - 22.0,
                Some(17.0),
            );
            ui.painter()
                .galley(Pos2::new(inner.min.x + 22.0, hint_y), text, tokens.danger);
        }
        None => {
            let lost = chosen.is_some() && chosen == app.lost_variant(&selection);
            let hint = if chosen.is_none() && !options.is_empty() {
                tl!("Pick a skin above to apply it.")
            } else if lost {
                tl!("A game update removed this skin. Apply it again.")
            } else if in_game {
                tl!("This skin is in the game now.")
            } else {
                tl!("Close the game before replacing.")
            };
            fonts::draw(
                ui.painter(),
                Pos2::new(inner.min.x, hint_y),
                hint,
                Text::body(12.0, 400, tokens.text_3),
            );
        }
    }
    if let Some(key) = picked {
        app.remember_variant(&selection, &key);
    }
    if apply_clicked {
        app.begin_apply_selected();
    }
}

fn items(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let tab = app.garage_tab.clone();
    let in_tab: Vec<_> = app
        .items
        .iter()
        .filter(|i| i.kind == tab)
        .cloned()
        .collect();
    let states: Vec<StateFilter> = in_tab.iter().map(|i| app.item_state(i)).collect();
    let count = |state: StateFilter| states.iter().filter(|s| **s == state).count();
    // The filter shows the states that occur, and only when something is not original.
    let shown_states: Vec<StateFilter> = StateFilter::ALL
        .into_iter()
        .filter(|s| *s == StateFilter::All || count(*s) > 0)
        .collect();
    let filtering = states.iter().any(|s| *s != StateFilter::Original);
    let active = if filtering && shown_states.contains(&app.garage_state) {
        app.garage_state
    } else {
        StateFilter::All
    };
    let list: Vec<_> = in_tab
        .iter()
        .zip(&states)
        .filter(|(_, state)| active == StateFilter::All || **state == active)
        .map(|(item, state)| (item.clone(), *state))
        .collect();
    let meta = tn(app.items.len() as u64, "{n} item", "{n} items");
    let body = widgets::panel(ui, rect, tl!("Items"), Some(&meta));
    let inner = body.shrink(18.0);
    let tabs = Rect::from_min_size(inner.min, Vec2::new(inner.width(), 34.0));
    in_rect(ui, tabs, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for kind in TABS {
                if widgets::tab(ui, tab_label(kind), app.garage_tab == kind).clicked() {
                    app.garage_tab = kind.to_owned();
                }
            }
        });
    });
    // One look on every item at once, on the right of the tabs; the hint gives way first.
    let tabs_width: f32 = TABS
        .iter()
        .map(|kind| widgets::tab_width(ui, tab_label(kind)) + 6.0)
        .sum();
    let button = Button::new(ButtonKind::Secondary, tl!("Apply to all items"))
        .icon(icons::LAYERS)
        .height(34.0);
    let button_width = button.measure(ui);
    let segment_items: Vec<(&str, Option<String>)> = shown_states
        .iter()
        .map(|s| (state_label(*s), Some(count_of(&states, *s).to_string())))
        .collect();
    let dots: Vec<Option<egui::Color32>> = shown_states
        .iter()
        .map(|s| state_color(&tokens, *s))
        .collect();
    let active_index = shown_states.iter().position(|s| *s == active).unwrap_or(0);
    let segments_width = widgets::dotted_segments_width(
        ui,
        &segment_items,
        dots.iter().flatten().count(),
        active_index,
    );
    let after_tabs = tabs.min.x + tabs_width + 12.0;
    // The filter sits after the tabs when there is room, else on a line of its own.
    let filter_inline =
        !filtering || after_tabs + segments_width + 12.0 + button_width <= tabs.max.x;
    let mut head_bottom = tabs.max.y;
    if filtering {
        let origin = if filter_inline {
            Pos2::new(after_tabs, tabs.center().y - 16.0)
        } else {
            head_bottom += 10.0 + 32.0;
            Pos2::new(tabs.min.x, tabs.max.y + 10.0)
        };
        let (_, clicked) = widgets::dotted_segments(
            ui,
            origin,
            32.0,
            &segment_items,
            &dots,
            active_index,
            "garage-states",
        );
        if let Some(state) = clicked.and_then(|i| shown_states.get(i)) {
            app.garage_state = *state;
        }
    }
    let button_rect = Rect::from_min_size(
        Pos2::new(tabs.max.x - button_width, tabs.min.y),
        Vec2::new(button_width, 34.0),
    );
    let left_of_button = if filtering && filter_inline {
        after_tabs + segments_width + 12.0
    } else {
        after_tabs
    };
    if button_rect.min.x > left_of_button {
        if in_rect(ui, button_rect, |ui| button.show(ui).clicked()) {
            app.open_bulk();
        }
        let hint = tl!("One look on every hull and turret at once");
        let style = Text::body(13.0, 400, tokens.text_2);
        if button_rect.min.x - 12.0 - fonts::width(ui.painter(), hint, style) > left_of_button {
            fonts::draw_right(
                ui.painter(),
                button_rect.min.x - 12.0,
                tabs.center().y,
                hint,
                style,
            );
        }
    }
    let cards = Rect::from_min_max(Pos2::new(inner.min.x, head_bottom + 14.0), inner.max);
    if list.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(cards.center(), Vec2::new(cards.width(), 18.0)),
            tl!("Nothing recognised in this category yet. Label a resource to add it."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    let per_row = (((cards.width() + 16.0) / (190.0 + 16.0)).floor() as usize).max(1);
    let card_width = (cards.width() - (per_row - 1) as f32 * 16.0) / per_row as f32;
    let ctx = ui.ctx().clone();
    let covers: Vec<_> = list
        .iter()
        .map(|(i, _)| app.item_cover(&ctx, &i.kind, &i.id))
        .collect();
    // A card of an item that lost its skin opens the modification that lost it.
    let lost_grades: Vec<Option<usize>> = list
        .iter()
        .map(|(i, state)| {
            (*state == StateFilter::Lost)
                .then(|| (0..4).find(|g| app.lost_skin(&i.kind, &i.id, *g).is_some()))
                .flatten()
        })
        .collect();
    let mut chosen = None;
    in_rect(ui, cards, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("garage-2")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (row_index, chunk) in list.chunks(per_row).enumerate() {
                    let top = ui.cursor().min;
                    let (_, _) = ui.allocate_exact_size(
                        Vec2::new(cards.width(), CARD_HEIGHT + 16.0),
                        Sense::hover(),
                    );
                    for (col, (item, state)) in chunk.iter().enumerate() {
                        let rect = Rect::from_min_size(
                            Pos2::new(top.x + col as f32 * (card_width + 16.0), top.y),
                            Vec2::new(card_width, CARD_HEIGHT),
                        );
                        let index = row_index * per_row + col;
                        let selected = app
                            .selection
                            .as_ref()
                            .is_some_and(|s| s.kind == item.kind && s.id == item.id);
                        // A card picks the item; its modification is chosen in the "My tank" panel.
                        let grade = app.selection.as_ref().filter(|_| selected).map_or_else(
                            || {
                                lost_grades[index].unwrap_or_else(|| {
                                    (0..4).rev().find(|g| item.present[*g]).unwrap_or(3)
                                })
                            },
                            |s| s.grade,
                        );
                        let cover = covers[index].as_ref().map_or(Cover::Icon(icons::BOX), |t| {
                            Cover::Image(t.id(), t.size_vec2())
                        });
                        let name = item.name();
                        let response = widgets::item_card(
                            ui,
                            rect,
                            &widgets::ItemCard {
                                name: &name,
                                kind: kind_label(&item.kind),
                                cover,
                                selected,
                                state: card_state(*state),
                            },
                        );
                        if response.card.clicked() {
                            chosen = Some(Selection {
                                kind: item.kind.clone(),
                                id: item.id.clone(),
                                grade,
                            });
                        }
                    }
                }
            });
    });
    if let Some(selection) = chosen {
        app.selection = Some(selection);
    }
}

/// A filter segment names a group of items ("Waiting" are the items that wait), a card one item.
fn state_label(state: StateFilter) -> &'static str {
    match state {
        StateFilter::All => tl!("All"),
        StateFilter::Skin => tl!("Skin on"),
        StateFilter::Original => tl!("Original"),
        StateFilter::Waiting => tl_ctx!("filter", "Waiting"),
        StateFilter::Lost => tl_ctx!("filter", "Lost"),
    }
}

fn state_color(tokens: &Tokens, state: StateFilter) -> Option<egui::Color32> {
    match state {
        StateFilter::Skin => Some(tokens.primary),
        StateFilter::Waiting => Some(tokens.info),
        StateFilter::Lost => Some(tokens.accent),
        StateFilter::All | StateFilter::Original => None,
    }
}

fn count_of(states: &[StateFilter], state: StateFilter) -> usize {
    if state == StateFilter::All {
        states.len()
    } else {
        states.iter().filter(|s| **s == state).count()
    }
}

/// The chip of a card: what the item wears, in a word ("Skin on", "Waiting", "Lost").
fn card_state(state: StateFilter) -> Option<(ChipKind, &'static str, Option<char>)> {
    match state {
        StateFilter::Skin => Some((ChipKind::Success, tl!("Skin on"), None)),
        StateFilter::Waiting => Some((ChipKind::Info, tl!("Waiting"), Some(icons::CLOCK_3))),
        StateFilter::Lost => Some((ChipKind::Warning, tl!("Lost"), Some(icons::TRIANGLE_ALERT))),
        StateFilter::All | StateFilter::Original => None,
    }
}
