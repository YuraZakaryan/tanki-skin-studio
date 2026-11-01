// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! "Apply to all items": the wizard over the Garage. Pick a look (step 1), choose what each
//! modification gets and see it item by item (step 2), check every change (step 3), then the run
//! with its progress, and the result with Undo all. With no look in the packs, an empty state
//! offers to add a pack.

use eframe::egui::{self, Context, CornerRadius, Pos2, Rect, Role, Sense, Ui, Vec2};
use tanki_i18n::{fmt, tl, tl_ctx, tn};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    layout::in_rect,
    theme::Tokens,
    widgets::{self, Button, ButtonKind, ModalEvent, WizardSpec},
};

use super::scroll_page;
use crate::{
    app::{BulkRun, BulkStep, ItemState, RowFilter, Studio},
    bulk::{self, Cell, How, Look, Pick, Plan, Source, Task},
    model::format_bytes,
};

/// The size of the three choosing steps, of the run, of the result and of the empty state.
const BIG: Vec2 = Vec2::new(1100.0, 840.0);
const RUN: Vec2 = Vec2::new(620.0, 470.0);
const DONE: Vec2 = Vec2::new(640.0, 470.0);
const EMPTY: Vec2 = Vec2::new(560.0, 420.0);

/// What the user did this frame; applied once drawing is over.
enum Act {
    Close,
    Next,
    Back,
    Select(usize),
    Open(usize),
    SetOn(u8, bool),
    SetSource(u8, Source),
    Pick(String, String, u8, Option<Pick>),
    Kind(&'static str, bool),
    Filter(RowFilter),
    Toggle(String, String),
    Edit(u8),
    Stop,
    Undo,
    AddPack,
}

fn steps() -> [&'static str; 3] {
    [tl!("Look"), tl!("Where"), tl!("Check")]
}

fn title() -> &'static str {
    tl!("Apply a look to all items")
}

/// A reason in the middle of a sentence: "The file is in use" is "the file is in use" there.
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_lowercase().chain(chars).collect())
        .unwrap_or_default()
}

/// "M0".."M3".
fn grade(modification: u8) -> String {
    format!("M{modification}")
}

/// Where the wizard is on the screen now, if it is open.
pub fn rect(app: &Studio, ctx: &Context) -> Option<Rect> {
    let wizard = app.bulk.as_ref()?;
    let size = match wizard.step {
        BulkStep::Look if wizard.looks.is_empty() => EMPTY,
        BulkStep::Look | BulkStep::Where | BulkStep::Check => BIG,
        BulkStep::Running => RUN,
        BulkStep::Done => DONE,
    };
    Some(widgets::wizard_rect(ctx, size))
}

pub fn draw(app: &mut Studio, ctx: &Context) {
    let Some(step) = app.bulk.as_ref().map(|w| w.step) else {
        return;
    };
    let empty = app.bulk.as_ref().is_some_and(|w| w.looks.is_empty());
    // A dialog over the wizard (the game is running) takes the keys.
    let keys = app.dialog.is_none() && !egui::Popup::is_any_open(ctx);
    let enter = keys && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
    let mut acts = Vec::new();
    match step {
        BulkStep::Look if empty => empty_state(ctx, keys, &mut acts),
        BulkStep::Look => look_step(app, ctx, keys, &mut acts),
        BulkStep::Where => where_step(app, ctx, keys, &mut acts),
        BulkStep::Check => check_step(app, ctx, keys, &mut acts),
        BulkStep::Running => running_step(app, ctx, &mut acts),
        BulkStep::Done => done_step(app, ctx, keys, &mut acts),
    }
    if enter && step != BulkStep::Running {
        acts.push(if empty { Act::AddPack } else { Act::Next });
    }
    for act in acts {
        match act {
            Act::Close => app.close_bulk(),
            Act::Next => app.bulk_next(),
            Act::Back => app.bulk_back(),
            Act::Select(index) => app.bulk_select_look(index),
            Act::Open(index) => {
                app.bulk_select_look(index);
                app.bulk_next();
            }
            Act::SetOn(modification, on) => app.bulk_set_on(modification, on),
            Act::SetSource(modification, source) => app.bulk_set_source(modification, source),
            Act::Pick(kind, id, modification, pick) => {
                app.bulk_set_pick(&kind, &id, modification, pick)
            }
            Act::Kind(kind, on) => app.bulk_set_kind(kind, on),
            Act::Filter(filter) => app.bulk_set_filter(filter),
            Act::Toggle(kind, id) => app.bulk_toggle_row(&kind, &id),
            Act::Edit(modification) => app.bulk_edit_modification(modification),
            Act::Stop => app.bulk_stop(),
            Act::Undo => app.bulk_undo(),
            Act::AddPack => app.bulk_add_pack(),
        }
        // One act per frame changes the step at most once (Enter on a button's click).
        if app.bulk.as_ref().map(|w| w.step) != Some(step) {
            break;
        }
    }
}

fn spec<'a>(
    title: &'a str,
    subtitle: Option<&'a str>,
    steps: &'a [&'a str],
    current: usize,
    closable: bool,
    size: Vec2,
    hint: Option<(char, &'a str)>,
) -> WizardSpec<'a> {
    WizardSpec {
        id: "bulk",
        title,
        subtitle,
        steps,
        current,
        close_label: tl!("Close"),
        closable,
        size,
        hint,
    }
}

/// A wrapped paragraph at `pos`, `width` wide; returns its height.
fn text_at(ui: &Ui, pos: Pos2, text: &str, style: Text, width: f32, line: f32) -> f32 {
    let galley = fonts::paragraph(ui.painter(), text, style, width.max(20.0), Some(line));
    let height = galley.size().y;
    ui.painter().galley(pos, galley, style.color);
    height
}

/// The height `text` takes wrapped to `width`.
fn text_height(ui: &Ui, text: &str, style: Text, width: f32, line: f32) -> f32 {
    fonts::paragraph(ui.painter(), text, style, width.max(20.0), Some(line))
        .size()
        .y
        .max(line)
}

/// A label that screen readers and tests can find, over a painted rectangle.
fn label_node(ui: &mut Ui, rect: Rect, salt: impl std::hash::Hash + std::fmt::Debug, text: &str) {
    let response = ui.interact(rect, ui.scope_id().with(salt), Sense::hover());
    let text = text.to_owned();
    response.widget_info(move || egui::WidgetInfo::labeled(Role::Label, true, text.clone()));
}

// ---------------------------------------------------------------------------------------------
// Empty state.

fn empty_state(ctx: &Context, keys: bool, acts: &mut Vec<Act>) {
    let spec = spec(title(), None, &[], 0, keys, EMPTY, None);
    let mut add = false;
    let mut close = false;
    let event = widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let inner = rect.shrink2(Vec2::new(40.0, 24.0));
            let heading = tl!("No looks to apply yet");
            let body = tl!(
                "Add a pack with skins. Skins with the same name for different items show up here as one look."
            );
            let body_style = Text::body(14.0, 400, tokens.text_2);
            let total =
                64.0 + 14.0 + 26.0 + 14.0 + text_height(ui, body, body_style, inner.width(), 17.0);
            let top = inner.center().y - total / 2.0;
            let icon =
                Rect::from_min_size(Pos2::new(inner.center().x - 32.0, top), Vec2::splat(64.0));
            draw::rounded(
                ui.painter(),
                icon,
                CornerRadius::same(16),
                tokens.surface_3,
                None,
            );
            fonts::icon(
                ui.painter(),
                icon.center(),
                icons::LAYERS,
                30.0,
                tokens.text_3,
            );
            let heading_rect = Rect::from_min_size(
                Pos2::new(inner.min.x, icon.max.y + 14.0),
                Vec2::new(inner.width(), 26.0),
            );
            fonts::draw_centered(
                ui.painter(),
                heading_rect,
                heading,
                Text::display(20.0, 700, tokens.text),
            );
            label_node(ui, heading_rect, "empty-heading", heading);
            let galley = fonts::paragraph_centered(
                ui.painter(),
                body,
                body_style,
                inner.width(),
                Some(17.0),
            );
            ui.painter().galley(
                Pos2::new(inner.center().x, heading_rect.max.y + 14.0),
                galley,
                tokens.text_2,
            );
        },
        |ui| {
            add = Button::new(ButtonKind::Primary, tl!("Add a pack"))
                .icon(icons::PACKAGE_PLUS)
                .show(ui)
                .clicked();
            close = Button::new(ButtonKind::Ghost, tl!("Close"))
                .show(ui)
                .clicked();
        },
    );
    if add {
        acts.push(Act::AddPack);
    } else if close || event == ModalEvent::Dismissed {
        acts.push(Act::Close);
    }
}

// ---------------------------------------------------------------------------------------------
// Step 1: the look.

/// "14 items · 8 hulls · 6 turrets · 2 packs".
fn look_meta(look: &Look) -> String {
    let (hulls, turrets) = (look.count("hull"), look.count("turret"));
    let mut parts = vec![tn(look.items.len() as u64, "{n} item", "{n} items")];
    if hulls > 0 {
        parts.push(tn(hulls as u64, "{n} hull", "{n} hulls"));
    }
    if turrets > 0 {
        parts.push(tn(turrets as u64, "{n} turret", "{n} turrets"));
    }
    parts.push(tn(look.packs.len() as u64, "{n} pack", "{n} packs"));
    parts.join(" · ")
}

fn look_step(app: &mut Studio, ctx: &Context, keys: bool, acts: &mut Vec<Act>) {
    let Some(wizard) = app.bulk.as_ref() else {
        return;
    };
    let looks = wizard.looks.clone();
    let selected = wizard.look;
    let needle = bulk::normalise(&wizard.search);
    let mut packs: Vec<&str> = Vec::new();
    for look in &looks {
        for pack in &look.packs {
            if !packs.contains(&pack.as_str()) {
                packs.push(pack);
            }
        }
    }
    let hint = fmt(
        tl!("{looks} found {packs}"),
        &[
            ("looks", &tn(looks.len() as u64, "{n} look", "{n} looks")),
            (
                "packs",
                &tn(packs.len() as u64, "in {n} pack", "in {n} packs"),
            ),
        ],
    );
    let steps = steps();
    let spec = spec(
        title(),
        Some(tl!("Pick a look from your packs")),
        &steps,
        0,
        keys,
        BIG,
        Some((icons::LAYERS, &hint)),
    );
    let shown: Vec<usize> = (0..looks.len())
        .filter(|&i| needle.is_empty() || looks.get(i).is_some_and(|l| l.key.contains(&needle)))
        .collect();
    let mut next = false;
    let mut cancel = false;
    let event = widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let inner = rect.shrink2(Vec2::new(24.0, 18.0));
            let search_width = (inner.width() * 0.3).clamp(180.0, 280.0);
            let intro = tl!(
                "Skins with the same name in your packs are one look. Pick it once and it goes on every item that has it."
            );
            let intro_style = Text::body(14.0, 400, tokens.text_2);
            let intro_width = inner.width() - search_width - 16.0;
            let intro_height = text_height(ui, intro, intro_style, intro_width, 17.0);
            let top_height = intro_height.max(38.0);
            text_at(
                ui,
                Pos2::new(inner.min.x, inner.min.y + (top_height - intro_height) / 2.0),
                intro,
                intro_style,
                intro_width,
                17.0,
            );
            let search = Rect::from_min_size(
                Pos2::new(
                    inner.max.x - search_width,
                    inner.min.y + (top_height - 38.0) / 2.0,
                ),
                Vec2::new(search_width, 38.0),
            );
            if let Some(wizard) = app.bulk.as_mut() {
                widgets::text_field(
                    ui,
                    search,
                    icons::SEARCH,
                    tl!("Search looks"),
                    &mut wizard.search,
                    None,
                );
            }
            // "How it works" at the bottom when there is room for it.
            let how_height = 76.0;
            let room_for_how =
                inner.height() - top_height - 16.0 > 196.0 + 16.0 + how_height + 16.0;
            let grid_bottom = if room_for_how {
                inner.max.y - how_height - 16.0
            } else {
                inner.max.y
            };
            let grid = Rect::from_min_max(
                Pos2::new(inner.min.x, inner.min.y + top_height + 16.0),
                Pos2::new(inner.max.x, grid_bottom),
            );
            if room_for_how {
                how_it_works(
                    ui,
                    Rect::from_min_max(Pos2::new(inner.min.x, inner.max.y - how_height), inner.max),
                );
            }
            if shown.is_empty() {
                fonts::draw_centered(
                    ui.painter(),
                    grid,
                    &fmt(
                        tl!("No look matches “{query}”."),
                        &[("query", &wizard_search(app))],
                    ),
                    Text::body(13.0, 400, tokens.text_3),
                );
                return;
            }
            let per_row = if grid.width() >= 760.0 {
                3
            } else if grid.width() >= 480.0 {
                2
            } else {
                1
            };
            let gap = 14.0;
            let card_width = (grid.width() - gap * (per_row - 1) as f32) / per_row as f32;
            let card_height = 196.0;
            let rows = shown.len().div_ceil(per_row);
            let total = rows as f32 * (card_height + gap) - gap;
            in_rect(ui, grid, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("bulk-looks")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let top = ui.cursor().min;
                        ui.allocate_exact_size(Vec2::new(grid.width(), total), Sense::hover());
                        for (slot, &index) in shown.iter().enumerate() {
                            let Some(look) = looks.get(index) else {
                                continue;
                            };
                            let (row, col) = (slot / per_row, slot % per_row);
                            let card = Rect::from_min_size(
                                Pos2::new(
                                    top.x + col as f32 * (card_width + gap),
                                    top.y + row as f32 * (card_height + gap),
                                ),
                                Vec2::new(card_width, card_height),
                            );
                            let response = look_card(app, ui, card, look, selected == Some(index));
                            if response.double_clicked() {
                                acts.push(Act::Open(index));
                            } else if response.clicked() {
                                acts.push(Act::Select(index));
                            }
                        }
                    });
            });
        },
        |ui| {
            next = Button::new(ButtonKind::Primary, tl!("Next: where"))
                .icon(icons::ARROW_RIGHT)
                .enabled(selected.is_some())
                .show(ui)
                .clicked();
            cancel = Button::new(ButtonKind::Ghost, tl!("Cancel"))
                .show(ui)
                .clicked();
        },
    );
    if next {
        acts.push(Act::Next);
    } else if cancel || event == ModalEvent::Dismissed {
        acts.push(Act::Close);
    }
}

fn wizard_search(app: &Studio) -> String {
    app.bulk
        .as_ref()
        .map_or_else(String::new, |w| w.search.clone())
}

fn look_card(
    app: &mut Studio,
    ui: &mut Ui,
    card: Rect,
    look: &Look,
    selected: bool,
) -> egui::Response {
    let tokens = Tokens::get(ui.ctx());
    let response = ui.interact(
        card,
        ui.scope_id().with(("look", &look.key)),
        Sense::click(),
    );
    let name = look.name.clone();
    response.widget_info(move || {
        egui::WidgetInfo::selected(Role::RadioButton, true, selected, name.clone())
    });
    let (fill, stroke) = if selected {
        (tokens.primary_soft, (2.0, tokens.primary))
    } else if response.hovered() {
        (tokens.surface_2, (1.0, tokens.border_strong))
    } else {
        (tokens.surface, (1.0, tokens.border))
    };
    draw::rounded(
        ui.painter(),
        card,
        CornerRadius::same(14),
        fill,
        Some(stroke),
    );
    // Pictures: the first items' skins (their preview from the pack, else the kind's icon).
    let strip = Rect::from_min_size(
        card.min + Vec2::splat(1.0),
        Vec2::new(card.width() - 2.0, 95.0),
    );
    draw::rounded(
        ui.painter(),
        strip,
        CornerRadius {
            nw: 13,
            ne: 13,
            sw: 0,
            se: 0,
        },
        tokens.stage,
        None,
    );
    let pictures = 3;
    let pic_width = (strip.width() - 24.0 - 8.0 * (pictures - 1) as f32) / pictures as f32;
    let ctx = ui.ctx().clone();
    for slot in 0..pictures {
        let pic = Rect::from_min_size(
            Pos2::new(
                strip.min.x + 12.0 + slot as f32 * (pic_width + 8.0),
                strip.min.y + 12.0,
            ),
            Vec2::new(pic_width, strip.height() - 24.0),
        );
        draw::rounded(
            ui.painter(),
            pic,
            CornerRadius::same(8),
            tokens.surface_3,
            None,
        );
        // Fewer items than pictures: the items again, so the strip is never half empty.
        let item = look.items.get(slot % look.items.len().max(1));
        let texture = item.and_then(|item| {
            let skin = item.skins.first()?;
            let pack = app
                .packs
                .iter()
                .find(|p| p.manifest.id == skin.pack_id && p.manifest.version == skin.pack_version)?
                .clone();
            let variant = pack
                .manifest
                .items
                .get(skin.item)?
                .variants
                .get(skin.variant)?
                .clone();
            app.variant_preview(&ctx, &pack, &variant)
        });
        match texture {
            Some(texture) => {
                let size = texture.size_vec2();
                let scale = (pic.width() / size.x).min(pic.height() / size.y);
                ui.painter().image(
                    texture.id(),
                    Rect::from_center_size(pic.center(), size * scale),
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    egui::Color32::WHITE,
                );
            }
            None => {
                // No picture: a hull, a turret and a hull stand for the look.
                let icon = if slot == 1 {
                    icons::CROSSHAIR
                } else {
                    icons::CAR_FRONT
                };
                fonts::icon(ui.painter(), pic.center(), icon, 22.0, tokens.text_3);
            }
        }
    }
    let left = card.min.x + 14.0;
    let right = card.max.x - 14.0;
    let name_y = card.min.y + 96.0 + 12.0 + 12.0;
    let radio = Rect::from_center_size(Pos2::new(right - 10.0, name_y), Vec2::splat(20.0));
    draw::rounded(
        ui.painter(),
        radio,
        CornerRadius::same(10),
        egui::Color32::TRANSPARENT,
        Some((
            2.0,
            if selected {
                tokens.primary
            } else {
                tokens.border_strong
            },
        )),
    );
    if selected {
        ui.painter()
            .circle_filled(radio.center(), 5.0, tokens.primary);
    }
    let name_style = Text::display(18.0, 700, tokens.text);
    let name = fonts::ellipsize(ui.painter(), &look.name, name_style, right - left - 30.0);
    fonts::draw_left(ui.painter(), left, name_y, &name, name_style);
    let meta_style = Text::body(12.0, 400, tokens.text_2);
    let meta = look_meta(look);
    let meta = fonts::ellipsize(ui.painter(), &meta, meta_style, right - left);
    fonts::draw_left(ui.painter(), left, name_y + 26.0, &meta, meta_style);
    // Which modifications the look has skins for.
    let grades_y = name_y + 52.0;
    let label_style = Text::body(12.0, 400, tokens.text_3);
    let used = fonts::draw_left(ui.painter(), left, grades_y, tl!("Skins for:"), label_style);
    let sources = look.sources();
    let any = look.has_edition_skins();
    let mut x = used.max.x + 8.0;
    for (k, has) in sources.iter().enumerate() {
        let on = *has || any;
        let chip = Rect::from_min_size(Pos2::new(x, grades_y - 10.0), Vec2::new(28.0, 20.0));
        let (fill, border, ink) = if on {
            (tokens.surface_3, tokens.border_strong, tokens.text)
        } else {
            (
                egui::Color32::TRANSPARENT,
                draw::fade(tokens.border, 0.5),
                draw::fade(tokens.text_3, 0.5),
            )
        };
        draw::rounded(
            ui.painter(),
            chip,
            CornerRadius::same(5),
            fill,
            Some((1.0, border)),
        );
        let text = grade(k as u8);
        fonts::draw_centered(ui.painter(), chip, &text, Text::mono(11.0, 700, ink));
        if !on {
            ui.painter().line_segment(
                [
                    Pos2::new(chip.min.x + 6.0, chip.center().y),
                    Pos2::new(chip.max.x - 6.0, chip.center().y),
                ],
                egui::Stroke::new(1.0, ink),
            );
        }
        x += 28.0 + 6.0;
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn how_it_works(ui: &Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let cards = [
        (
            icons::LAYERS,
            tl!("Pick a look"),
            tl!("XT, Legacy, Prime or any other name from your packs"),
        ),
        (
            icons::SLIDERS_HORIZONTAL,
            tl!("Choose M0–M3"),
            tl!("Each modification can get its own skin or stay as it is"),
        ),
        (
            icons::SHIELD_CHECK,
            // An instruction here ("Проверьте и наденьте"), not the button's label.
            tl_ctx!("step", "Check and apply"),
            tl!("You see every change first and can undo it all in one click"),
        ),
    ];
    let gap = 12.0;
    let width = (rect.width() - gap * 2.0) / 3.0;
    for (index, (icon, heading, detail)) in cards.iter().enumerate() {
        let card = Rect::from_min_size(
            Pos2::new(rect.min.x + index as f32 * (width + gap), rect.min.y),
            Vec2::new(width, rect.height()),
        );
        draw::rounded(
            ui.painter(),
            card,
            CornerRadius::same(12),
            tokens.surface,
            Some((1.0, tokens.border)),
        );
        let badge = Rect::from_center_size(
            Pos2::new(card.min.x + 14.0 + 17.0, card.center().y),
            Vec2::splat(34.0),
        );
        draw::rounded(
            ui.painter(),
            badge,
            CornerRadius::same(17),
            tokens.primary_soft,
            None,
        );
        fonts::icon(ui.painter(), badge.center(), *icon, 17.0, tokens.primary);
        let left = badge.max.x + 12.0;
        let room = card.max.x - 14.0 - left;
        let heading = format!("{}. {heading}", index + 1);
        let heading_style = Text::body(14.0, 600, tokens.text);
        let detail_style = Text::body(12.0, 400, tokens.text_3);
        let detail_height = text_height(ui, detail, detail_style, room, 15.0).min(30.0);
        let top = card.center().y - (18.0 + 2.0 + detail_height) / 2.0;
        let shown = fonts::ellipsize(ui.painter(), &heading, heading_style, room);
        fonts::draw(ui.painter(), Pos2::new(left, top), &shown, heading_style);
        text_at(
            ui,
            Pos2::new(left, top + 20.0),
            detail,
            detail_style,
            room,
            15.0,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Step 2: where.

/// "XT · own M1", "XT · from M3"; a look of edition skins only is just its name.
fn source_label(look: &Look, modification: u8, source: Source) -> String {
    if look.sources().iter().all(|s| !s) {
        return look.name.clone();
    }
    match source {
        Source::Own => fmt(
            tl!("{look} · own {modification}"),
            &[("look", &look.name), ("modification", &grade(modification))],
        ),
        Source::From(from) => fmt(
            tl!("{look} · from {modification}"),
            &[("look", &look.name), ("modification", &grade(from))],
        ),
    }
}

/// A skin of the look as a pick names it: "XT · M0", or the look's name for an edition's.
fn skin_label(look: &Look, source: Option<u8>) -> String {
    match source {
        Some(source) => fmt(
            tl!("{look} · {modification}"),
            &[("look", &look.name), ("modification", &grade(source))],
        ),
        None => look.name.clone(),
    }
}

/// "XT M3", or "Prime" for an edition's skin.
fn skin_name(look: &Look, source: Option<u8>) -> String {
    match source {
        Some(source) => format!("{} {}", look.name, grade(source)),
        None => look.name.clone(),
    }
}

/// The tone, icon and text of a cell of the table.
fn cell_view(look: &Look, modification: u8, cell: &Cell) -> (Tone, char, String) {
    match cell {
        Cell::Change { skin, how } => match how {
            How::Own => (Tone::Ok, icons::CHECK, skin_name(look, skin.source)),
            How::From(from) => (
                Tone::Ok,
                icons::CHECK,
                fmt(
                    tl!("from {modification}"),
                    &[("modification", &grade(*from))],
                ),
            ),
            How::Edition => (Tone::Ok, icons::CHECK, look.name.clone()),
            How::Picked => (
                Tone::Info,
                icons::PENCIL,
                fmt(
                    tl!("{skin} · own pick"),
                    &[("skin", &skin_name(look, skin.source))],
                ),
            ),
        },
        Cell::Missing { source } => (
            Tone::Missing,
            icons::CIRCLE_MINUS,
            fmt(tl!("No {skin}"), &[("skin", &skin_name(look, *source))]),
        ),
        Cell::NotInGame => (
            Tone::Missing,
            icons::CIRCLE_SLASH,
            fmt(
                tl!("No {modification} in game"),
                &[("modification", &grade(modification))],
            ),
        ),
        Cell::Already => (
            Tone::Neutral,
            icons::CHECK_CHECK,
            tl!("Already on").to_owned(),
        ),
        Cell::Incomplete { .. } => (
            Tone::Missing,
            icons::TRIANGLE_ALERT,
            tl!("Skin incomplete").to_owned(),
        ),
        Cell::Off => (Tone::Off, icons::MINUS, tl!("Not touched").to_owned()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Ok,
    Info,
    Neutral,
    Missing,
    Off,
}

fn paint_cell(ui: &mut Ui, rect: Rect, salt: &str, tone: Tone, icon: char, text: &str) {
    let tokens = Tokens::get(ui.ctx());
    let (fill, ink, border) = match tone {
        Tone::Ok => (tokens.primary_soft, tokens.primary, None),
        Tone::Info => (tokens.info_soft, tokens.info, None),
        Tone::Neutral => (tokens.surface_3, tokens.text_2, None),
        Tone::Missing => (
            egui::Color32::TRANSPARENT,
            tokens.text_3,
            Some((1.0, tokens.border)),
        ),
        Tone::Off => (egui::Color32::TRANSPARENT, tokens.text_3, None),
    };
    draw::rounded(ui.painter(), rect, CornerRadius::same(7), fill, border);
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.min.x + 10.0 + 6.5, rect.center().y),
        icon,
        13.0,
        ink,
    );
    let weight = if tone == Tone::Off { 400 } else { 600 };
    let style = Text::body(12.0, weight, ink);
    let shown = fonts::ellipsize(
        ui.painter(),
        text,
        style,
        rect.width() - 10.0 - 13.0 - 6.0 - 8.0,
    );
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 10.0 + 13.0 + 6.0,
        rect.center().y,
        &shown,
        style,
    );
    label_node(ui, rect, ("cell", salt), text);
}

/// The "M0" badge of a modification, lime when it changes.
fn grade_badge(ui: &Ui, rect: Rect, modification: u8, on: bool) {
    let tokens = Tokens::get(ui.ctx());
    let (fill, ink) = if on {
        (tokens.primary, tokens.on_primary)
    } else {
        (tokens.surface_3, tokens.text_3)
    };
    draw::rounded(ui.painter(), rect, CornerRadius::same(6), fill, None);
    fonts::draw_centered(
        ui.painter(),
        rect,
        &grade(modification),
        Text::mono(13.0, 700, ink),
    );
}

const TILE_HEIGHT: f32 = 160.0;
const ROW_HEIGHT: f32 = 49.0;
const NAME_COLUMN: f32 = 210.0;

fn where_step(app: &mut Studio, ctx: &Context, keys: bool, acts: &mut Vec<Act>) {
    let Some(wizard) = app.bulk.as_ref() else {
        return;
    };
    let (Some(look), Some(settings), Some(plan)) = (
        wizard.chosen().cloned(),
        wizard.settings.clone(),
        wizard.plan.clone(),
    ) else {
        return;
    };
    let filter = wizard.filter;
    let expanded = wizard.expanded.clone();
    let editing = wizard.editing;
    let subtitle = fmt(
        tl!("Look: {look} · {items} {packs}"),
        &[
            ("look", &look.name),
            (
                "items",
                &tn(look.items.len() as u64, "{n} item", "{n} items"),
            ),
            (
                "packs",
                &tn(look.packs.len() as u64, "in {n} pack", "in {n} packs"),
            ),
        ],
    );
    let hint = fmt(
        tl!("{items} will change · {skipped}"),
        &[
            (
                "items",
                &tn(plan.changing_items() as u64, "{n} item", "{n} items"),
            ),
            (
                "skipped",
                &tn(plan.skipped_items() as u64, "{n} skipped", "{n} skipped"),
            ),
        ],
    );
    let steps = steps();
    let spec = spec(
        title(),
        Some(&subtitle),
        &steps,
        1,
        keys,
        BIG,
        Some((icons::LIST_CHECKS, &hint)),
    );
    let mut next = false;
    let mut back = false;
    let event = widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let intro = tl!(
                "Choose what each modification gets. Items that have the chosen skin change; the others are listed with the reason."
            );
            let intro_style = Text::body(14.0, 400, tokens.text_2);
            let width = rect.width() - 48.0;
            let intro_height = text_height(ui, intro, intro_style, width, 17.0);
            let fixed = 18.0 + intro_height + 14.0 + TILE_HEIGHT + 14.0 + 32.0 + 14.0 + 18.0;
            scroll_page(ui, rect, fixed + 200.0, |ui, content| {
                let inner = content.shrink2(Vec2::new(24.0, 18.0));
                let mut y = inner.min.y;
                text_at(
                    ui,
                    Pos2::new(inner.min.x, y),
                    intro,
                    intro_style,
                    width,
                    17.0,
                );
                y += intro_height + 14.0;
                let tiles = Rect::from_min_size(
                    Pos2::new(inner.min.x, y),
                    Vec2::new(inner.width(), TILE_HEIGHT),
                );
                modification_tiles(ui, tiles, &look, &settings, &plan, acts);
                y += TILE_HEIGHT + 14.0;
                let filters =
                    Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), 32.0));
                filter_row(ui, filters, &plan, filter, &settings, acts);
                y += 32.0 + 14.0;
                let table = Rect::from_min_max(Pos2::new(inner.min.x, y), inner.max);
                plan_table(
                    ui,
                    table,
                    &look,
                    &settings,
                    &plan,
                    filter,
                    expanded.as_ref(),
                    editing,
                    acts,
                );
            });
        },
        |ui| {
            next = Button::new(ButtonKind::Primary, tl!("Check and apply"))
                .icon(icons::ARROW_RIGHT)
                .enabled(plan.changes() > 0)
                .show(ui)
                .clicked();
            back = Button::new(ButtonKind::Ghost, tl!("Back"))
                .icon(icons::ARROW_LEFT)
                .show(ui)
                .clicked();
        },
    );
    if next {
        acts.push(Act::Next);
    } else if back {
        acts.push(Act::Back);
    } else if event == ModalEvent::Dismissed {
        acts.push(Act::Close);
    }
}

fn modification_tiles(
    ui: &mut Ui,
    rect: Rect,
    look: &Look,
    settings: &bulk::Settings,
    plan: &Plan,
    acts: &mut Vec<Act>,
) {
    let tokens = Tokens::get(ui.ctx());
    let shown: Vec<u8> = (0..4u8).filter(|k| plan.columns[usize::from(*k)]).collect();
    if shown.is_empty() {
        return;
    }
    let gap = 12.0;
    let width = (rect.width() - gap * (shown.len() - 1) as f32) / shown.len() as f32;
    for (slot, &k) in shown.iter().enumerate() {
        let tile = Rect::from_min_size(
            Pos2::new(rect.min.x + slot as f32 * (width + gap), rect.min.y),
            Vec2::new(width, rect.height()),
        );
        let choice = settings.choices[usize::from(k)];
        let on = choice.on;
        let stroke = if on {
            (1.5, tokens.primary)
        } else {
            (1.0, tokens.border)
        };
        draw::rounded(
            ui.painter(),
            tile,
            CornerRadius::same(12),
            tokens.surface,
            Some(stroke),
        );
        let inner = tile.shrink(14.0);
        let badge = Rect::from_min_size(inner.min, Vec2::new(40.0, 26.0));
        grade_badge(ui, badge, k, on);
        // The switch, with On / Off before it.
        let switch = Rect::from_min_size(
            Pos2::new(inner.max.x - 40.0, inner.min.y + 2.0),
            Vec2::new(40.0, 22.0),
        );
        let state = if on { tl!("On") } else { tl!("Off") };
        fonts::draw_right(
            ui.painter(),
            switch.min.x - 8.0,
            switch.center().y,
            state,
            Text::body(12.0, 600, if on { tokens.text } else { tokens.text_3 }),
        );
        let mut value = on;
        let label = fmt(
            tl!("Put skins on {modification}"),
            &[("modification", &grade(k))],
        );
        let toggled = in_rect(ui, switch, |ui| {
            widgets::toggle(ui, &mut value, &label).clicked()
        });
        if toggled {
            acts.push(Act::SetOn(k, value));
        }
        let caption = if on {
            tl!("Put on")
        } else {
            tl!("Don't touch")
        };
        fonts::draw(
            ui.painter(),
            Pos2::new(inner.min.x, inner.min.y + 26.0 + 10.0),
            caption,
            Text::display(11.0, 700, tokens.text_3).spaced(1.0),
        );
        let options_sources = bulk::source_options(look, k);
        let mut options: Vec<String> = options_sources
            .iter()
            .map(|s| source_label(look, k, *s))
            .collect();
        options.push(tl!("Don't touch").to_owned());
        let shown_text = if on {
            source_label(look, k, choice.source)
        } else {
            tl!("Don't touch").to_owned()
        };
        let select = Rect::from_min_size(
            Pos2::new(inner.min.x, inner.min.y + 26.0 + 10.0 + 14.0 + 8.0),
            Vec2::new(inner.width(), 34.0),
        );
        let select_label = fmt(
            tl!("What {modification} gets"),
            &[("modification", &grade(k))],
        );
        if let Some(index) =
            widgets::select(ui, select, &select_label, &shown_text, &options, true, !on)
        {
            match options_sources.get(index) {
                Some(source) => acts.push(Act::SetSource(k, *source)),
                None => acts.push(Act::SetOn(k, false)),
            }
        }
        // The bar sits on the tile's bottom padding, the note 10 above it.
        let bar_top = tile.max.y - 14.0 - 5.0;
        let note_y = bar_top - 10.0 - 7.5;
        if on {
            let (have, total) = bulk::coverage(look, settings, k);
            let text = fmt(
                tl!("{have} of {total} items have it"),
                &[("have", &have.to_string()), ("total", &total.to_string())],
            );
            fonts::draw_left(
                ui.painter(),
                inner.min.x,
                note_y,
                &text,
                Text::body(12.0, 400, tokens.text_2),
            );
            let bar = Rect::from_min_size(
                Pos2::new(inner.min.x, bar_top),
                Vec2::new(inner.width(), 5.0),
            );
            let fraction = if total == 0 {
                0.0
            } else {
                have as f32 / total as f32
            };
            widgets::progress_bar(ui, bar, fraction);
        } else {
            let text = fmt(
                tl!("{modification} stays as it is"),
                &[("modification", &grade(k))],
            );
            fonts::draw_left(
                ui.painter(),
                inner.min.x,
                note_y,
                &text,
                Text::body(12.0, 400, tokens.text_3),
            );
        }
    }
}

fn filter_row(
    ui: &mut Ui,
    rect: Rect,
    plan: &Plan,
    filter: RowFilter,
    settings: &bulk::Settings,
    acts: &mut Vec<Act>,
) {
    let tokens = Tokens::get(ui.ctx());
    let segments = [
        (RowFilter::All, tl!("All"), plan.rows.len()),
        (RowFilter::Change, tl!("Will change"), plan.changing_items()),
        (RowFilter::Skipped, tl!("Skipped"), plan.skipped_items()),
    ];
    let label_style =
        |on: bool| Text::body(13.0, 600, if on { tokens.text } else { tokens.text_2 });
    let count_style = Text::mono(12.0, 400, tokens.text_3);
    let widths: Vec<f32> = segments
        .iter()
        .map(|(f, label, count)| {
            12.0 + fonts::width(ui.painter(), label, label_style(*f == filter))
                + 6.0
                + fonts::width(ui.painter(), &count.to_string(), count_style)
                + 12.0
        })
        .collect();
    let total: f32 = widths.iter().sum::<f32>() + 4.0 * 2.0 + 6.0;
    let frame = Rect::from_min_size(rect.min, Vec2::new(total, rect.height()));
    draw::rounded(
        ui.painter(),
        frame,
        CornerRadius::same(9),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let mut x = frame.min.x + 3.0;
    for ((value, label, count), width) in segments.iter().zip(&widths) {
        let segment = Rect::from_min_size(
            Pos2::new(x, frame.min.y + 3.0),
            Vec2::new(*width, frame.height() - 6.0),
        );
        let on = *value == filter;
        let response = ui.interact(
            segment,
            ui.scope_id().with(("filter", *label)),
            Sense::click(),
        );
        let name = (*label).to_owned();
        response
            .widget_info(move || egui::WidgetInfo::selected(Role::Button, true, on, name.clone()));
        if on {
            draw::rounded(
                ui.painter(),
                segment,
                CornerRadius::same(7),
                tokens.surface_3,
                Some((1.0, tokens.border_strong)),
            );
        } else if response.hovered() {
            draw::rounded(
                ui.painter(),
                segment,
                CornerRadius::same(7),
                tokens.surface_2,
                None,
            );
        }
        let used = fonts::draw_left(
            ui.painter(),
            segment.min.x + 12.0,
            segment.center().y,
            label,
            label_style(on),
        );
        fonts::draw_left(
            ui.painter(),
            used.max.x + 6.0,
            segment.center().y,
            &count.to_string(),
            count_style,
        );
        if response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            acts.push(Act::Filter(*value));
        }
        x += width + 4.0;
    }
    // The kinds: a kind left out leaves the run.
    let mut right = rect.max.x;
    for (kind, label, on) in [
        ("turret", tl!("Turrets"), settings.turrets),
        ("hull", tl!("Hulls"), settings.hulls),
    ] {
        let width = 28.0 + fonts::width(ui.painter(), label, Text::body(13.0, 600, tokens.text));
        let place = Rect::from_min_size(
            Pos2::new(right - width, rect.center().y - 12.0),
            Vec2::new(width, 24.0),
        );
        let mut value = on;
        if in_rect(ui, place, |ui| {
            widgets::checkbox(ui, &mut value, label).clicked()
        }) {
            acts.push(Act::Kind(kind, value));
        }
        right -= width + 16.0;
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_table(
    ui: &mut Ui,
    rect: Rect,
    look: &Look,
    settings: &bulk::Settings,
    plan: &Plan,
    filter: RowFilter,
    expanded: Option<&(String, String)>,
    editing: Option<u8>,
    acts: &mut Vec<Act>,
) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        egui::Color32::TRANSPARENT,
        Some((1.0, tokens.border)),
    );
    let head = Rect::from_min_size(
        rect.min + Vec2::splat(1.0),
        Vec2::new(rect.width() - 2.0, 29.0),
    );
    draw::rounded(
        ui.painter(),
        head,
        CornerRadius {
            nw: 11,
            ne: 11,
            sw: 0,
            se: 0,
        },
        tokens.surface,
        None,
    );
    draw::hrule(
        ui.painter(),
        rect.min.x + 1.0,
        rect.max.x - 1.0,
        head.max.y,
        tokens.border,
    );
    let columns: Vec<u8> = (0..4u8).filter(|k| plan.columns[usize::from(*k)]).collect();
    let name_width = NAME_COLUMN.min(rect.width() * 0.3);
    let cells_left = rect.min.x + 14.0 + name_width + 10.0;
    let cell_width = if columns.is_empty() {
        0.0
    } else {
        (rect.max.x - 14.0 - cells_left - 10.0 * (columns.len() - 1) as f32) / columns.len() as f32
    };
    let head_style = Text::display(11.0, 700, tokens.text_3).spaced(1.0);
    fonts::draw_left(
        ui.painter(),
        rect.min.x + 14.0,
        head.center().y,
        &tl!("Item").to_uppercase(),
        head_style,
    );
    for (slot, k) in columns.iter().enumerate() {
        fonts::draw_left(
            ui.painter(),
            cells_left + slot as f32 * (cell_width + 10.0),
            head.center().y,
            &grade(*k),
            head_style,
        );
    }
    let rows: Vec<&bulk::Row> = plan
        .rows
        .iter()
        .filter(|row| match filter {
            RowFilter::All => true,
            RowFilter::Change => row.changes(),
            RowFilter::Skipped => row.skipped(),
        })
        .collect();
    let body = Rect::from_min_max(
        Pos2::new(rect.min.x + 1.0, head.max.y + 1.0),
        rect.max - Vec2::splat(1.0),
    );
    if rows.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            body,
            tl!("No items here."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    let height_of = |row: &bulk::Row| {
        let open = expanded.is_some_and(|(kind, id)| *kind == row.kind && *id == row.id);
        ROW_HEIGHT
            + if open {
                picks_height(pick_lines(row, settings, editing).len())
            } else {
                0.0
            }
    };
    let total: f32 = rows.iter().map(|r| height_of(r)).sum();
    in_rect(ui, body, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("bulk-table")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let top = ui.cursor().min;
                ui.allocate_exact_size(Vec2::new(body.width(), total), Sense::hover());
                let mut y = top.y;
                for row in rows {
                    let open =
                        expanded.is_some_and(|(kind, id)| *kind == row.kind && *id == row.id);
                    let line = Rect::from_min_size(
                        Pos2::new(body.min.x, y),
                        Vec2::new(body.width(), ROW_HEIGHT),
                    );
                    item_row(
                        ui, line, look, row, &columns, cells_left, cell_width, name_width, open,
                        acts,
                    );
                    y += ROW_HEIGHT;
                    if open {
                        let panel = Rect::from_min_size(
                            Pos2::new(body.min.x, y),
                            Vec2::new(
                                body.width(),
                                picks_height(pick_lines(row, settings, editing).len()),
                            ),
                        );
                        own_picks(ui, panel, look, settings, row, editing, acts);
                        y += panel.height();
                    }
                }
            });
    });
}

#[allow(clippy::too_many_arguments)]
fn item_row(
    ui: &mut Ui,
    line: Rect,
    look: &Look,
    row: &bulk::Row,
    columns: &[u8],
    cells_left: f32,
    cell_width: f32,
    name_width: f32,
    open: bool,
    acts: &mut Vec<Act>,
) {
    let tokens = Tokens::get(ui.ctx());
    let name = row.name();
    let hit = Rect::from_min_size(line.min, Vec2::new(14.0 + name_width, line.height()));
    let response = ui.interact(
        hit,
        ui.scope_id().with(("row", &row.kind, &row.id)),
        Sense::click(),
    );
    let label = fmt(tl!("Own choices for {item}"), &[("item", &name)]);
    response
        .widget_info(move || egui::WidgetInfo::selected(Role::Button, true, open, label.clone()));
    if open {
        draw::rounded(ui.painter(), line, CornerRadius::ZERO, tokens.surface, None);
    } else if response.hovered() {
        draw::rounded(
            ui.painter(),
            line,
            CornerRadius::ZERO,
            tokens.surface_2,
            None,
        );
    }
    draw::hrule(
        ui.painter(),
        line.min.x,
        line.max.x,
        line.max.y,
        tokens.border,
    );
    let cy = line.center().y;
    let mut x = line.min.x + 13.0;
    fonts::icon(
        ui.painter(),
        Pos2::new(x + 7.0, cy),
        if open {
            icons::CHEVRON_DOWN
        } else {
            icons::CHEVRON_RIGHT
        },
        14.0,
        tokens.text_3,
    );
    x += 14.0 + 10.0;
    let thumb = Rect::from_min_size(Pos2::new(x, cy - 15.0), Vec2::splat(30.0));
    draw::rounded(
        ui.painter(),
        thumb,
        CornerRadius::same(8),
        tokens.stage,
        None,
    );
    let icon = if row.kind == "turret" {
        icons::CROSSHAIR
    } else {
        icons::CAR_FRONT
    };
    fonts::icon(ui.painter(), thumb.center(), icon, 15.0, tokens.text_3);
    x += 30.0 + 10.0;
    let room = line.min.x + 14.0 + name_width - x;
    let name_style = Text::body(14.0, 600, tokens.text);
    let shown = fonts::ellipsize(ui.painter(), &name, name_style, room);
    fonts::draw_left(ui.painter(), x, cy - 8.0, &shown, name_style);
    fonts::draw_left(
        ui.painter(),
        x,
        cy + 9.0,
        super::garage::kind_label(&row.kind),
        Text::body(12.0, 400, tokens.text_3),
    );
    if response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
    {
        acts.push(Act::Toggle(row.kind.clone(), row.id.clone()));
    }
    for (slot, k) in columns.iter().enumerate() {
        let Some(Some(cell)) = row.cells.get(usize::from(*k)) else {
            continue;
        };
        let rect = Rect::from_min_size(
            Pos2::new(cells_left + slot as f32 * (cell_width + 10.0), cy - 15.0),
            Vec2::new(cell_width, 30.0),
        );
        let (tone, icon, text) = cell_view(look, *k, cell);
        paint_cell(
            ui,
            rect,
            &format!("{}:{}:{k}", row.kind, row.id),
            tone,
            icon,
            &text,
        );
    }
}

/// An item's own choices, one line per modification it has in the game.
/// The modifications an open row has a line for: those with the item's own pick, and the one
/// being chosen (the first in the game at first).
fn pick_lines(row: &bulk::Row, settings: &bulk::Settings, editing: Option<u8>) -> Vec<u8> {
    let in_game: Vec<u8> = row
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| matches!(cell, Some(cell) if *cell != Cell::NotInGame))
        .map(|(k, _)| k as u8)
        .collect();
    let mut lines: Vec<u8> = in_game
        .iter()
        .copied()
        .filter(|k| settings.pick(&row.kind, &row.id, *k).is_some())
        .collect();
    let current = editing
        .filter(|k| in_game.contains(k))
        .or_else(|| lines.first().copied())
        .or_else(|| in_game.first().copied());
    if let Some(k) = current
        && !lines.contains(&k)
    {
        lines.push(k);
    }
    lines.sort_unstable();
    lines
}

/// The height of an open row's own choices: 10 above, 34 a line, 10 between, 12 below.
fn picks_height(lines: usize) -> f32 {
    if lines == 0 {
        return 0.0;
    }
    10.0 + 34.0 * lines as f32 + 10.0 * (lines - 1) as f32 + 12.0
}

fn own_picks(
    ui: &mut Ui,
    panel: Rect,
    look: &Look,
    settings: &bulk::Settings,
    row: &bulk::Row,
    editing: Option<u8>,
    acts: &mut Vec<Act>,
) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        panel,
        CornerRadius::ZERO,
        tokens.surface,
        None,
    );
    draw::hrule(
        ui.painter(),
        panel.min.x,
        panel.max.x,
        panel.max.y,
        tokens.border,
    );
    let Some(item) = look.item(&row.kind, &row.id) else {
        return;
    };
    let name = row.name();
    let label = fmt(tl!("Only for {item}:"), &[("item", &name)]);
    let label_style = Text::body(13.0, 600, tokens.text_2);
    let label_width = fonts::width(ui.painter(), &label, label_style);
    let in_game: Vec<u8> = row
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| matches!(cell, Some(cell) if *cell != Cell::NotInGame))
        .map(|(k, _)| k as u8)
        .collect();
    let mut y = panel.min.y + 10.0;
    let mut first = true;
    for modification in pick_lines(row, settings, editing) {
        let k = usize::from(modification);
        let cell = row.cells.get(k).cloned().flatten();
        let line = Rect::from_min_size(Pos2::new(panel.min.x, y), Vec2::new(panel.width(), 34.0));
        let cy = line.center().y;
        let mut x = line.min.x + 58.0;
        if first {
            fonts::draw_left(ui.painter(), x, cy, &label, label_style);
        }
        first = false;
        x += label_width + 12.0;
        let badge = Rect::from_min_size(Pos2::new(x, cy - 13.0), Vec2::new(40.0, 26.0));
        let pick = settings.pick(&row.kind, &row.id, modification);
        let changes = cell.as_ref().is_some_and(Cell::changes);
        grade_badge(ui, badge, modification, changes);
        // The badge picks which modification the line is for.
        let badge_label = fmt(tl!("Modification of {item}"), &[("item", &name)]);
        let response = ui.interact(
            badge,
            ui.scope_id().with(("pick-modification", &row.id, k)),
            Sense::click(),
        );
        let value = grade(modification);
        let badge_name = badge_label.clone();
        response.widget_info(move || {
            let mut info = egui::WidgetInfo::labeled(Role::ComboBox, true, badge_name.clone());
            info.current_text_value = Some(value.clone());
            info
        });
        let names: Vec<String> = in_game.iter().map(|m| grade(*m)).collect();
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if let Some(index) = widgets::menu(ui, &response, 120.0, &names, &grade(modification))
            && let Some(chosen) = in_game.get(index)
        {
            acts.push(Act::Edit(*chosen));
        }
        x += 40.0 + 10.0;
        fonts::icon(
            ui.painter(),
            Pos2::new(x + 7.0, cy),
            icons::ARROW_LEFT,
            14.0,
            tokens.text_3,
        );
        x += 14.0 + 10.0;
        let sources: Vec<Option<u8>> = item.skins.iter().map(|s| s.source).collect();
        let mut options: Vec<String> = sources.iter().map(|s| skin_label(look, *s)).collect();
        options.push(tl!("Don't touch").to_owned());
        let choice = settings.choices[k];
        let shown = match pick {
            Some(Pick::Off) => tl!("Don't touch").to_owned(),
            Some(Pick::Skin(source)) => skin_label(look, source),
            None if !choice.on => tl!("Don't touch").to_owned(),
            None => match bulk::common_skin(item, choice, modification) {
                Ok((skin, _)) => skin_label(look, skin.source),
                Err(source) => fmt(tl!("No {skin}"), &[("skin", &skin_name(look, source))]),
            },
        };
        let select = Rect::from_min_size(Pos2::new(x, cy - 17.0), Vec2::new(200.0, 34.0));
        let select_label = fmt(
            tl!("What {modification} of {item} gets"),
            &[("modification", &grade(modification)), ("item", &name)],
        );
        if let Some(index) =
            widgets::select(ui, select, &select_label, &shown, &options, true, false)
        {
            let chosen = match sources.get(index) {
                Some(source) => Pick::Skin(*source),
                None => Pick::Off,
            };
            acts.push(Act::Pick(
                row.kind.clone(),
                row.id.clone(),
                modification,
                Some(chosen),
            ));
        }
        x += 200.0 + 12.0;
        if pick.is_some() {
            // The "own pick" chip, and the way back to the common choice on the right.
            let chip = tl!("own pick");
            let style = Text::body(12.0, 600, tokens.info);
            let width = 10.0 + 12.0 + 6.0 + fonts::width(ui.painter(), chip, style) + 10.0;
            let rect = Rect::from_min_size(Pos2::new(x, cy - 12.0), Vec2::new(width, 24.0));
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(12),
                tokens.info_soft,
                None,
            );
            fonts::icon(
                ui.painter(),
                Pos2::new(rect.min.x + 10.0 + 6.0, cy),
                icons::PENCIL,
                12.0,
                tokens.info,
            );
            fonts::draw_left(ui.painter(), rect.min.x + 28.0, cy, chip, style);
            let reset = tl!("Use the common choice");
            let style = Text::body(13.0, 600, tokens.primary);
            let width = 13.0 + 6.0 + fonts::width(ui.painter(), reset, style);
            let link = Rect::from_min_size(
                Pos2::new(line.max.x - 14.0 - width, cy - 12.0),
                Vec2::new(width, 24.0),
            );
            let response = ui.interact(
                link,
                ui.scope_id().with(("reset", &row.id, k)),
                Sense::click(),
            );
            let name = fmt(
                tl!("Use the common choice for {modification}"),
                &[("modification", &grade(modification))],
            );
            response
                .widget_info(move || egui::WidgetInfo::labeled(Role::Button, true, name.clone()));
            fonts::icon(
                ui.painter(),
                Pos2::new(link.min.x + 6.5, cy),
                icons::UNDO_2,
                13.0,
                tokens.primary,
            );
            fonts::draw_left(ui.painter(), link.min.x + 19.0, cy, reset, style);
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                acts.push(Act::Pick(
                    row.kind.clone(),
                    row.id.clone(),
                    modification,
                    None,
                ));
            }
        }
        y += 34.0 + 10.0;
    }
}

// ---------------------------------------------------------------------------------------------
// Step 3: check.

/// "M0 gets XT from M3, M1 and M2 get their own XT, M3 is not touched."
fn mapping_sentence(look: &Look, settings: &bulk::Settings, columns: [bool; 4]) -> String {
    let mut groups: Vec<(bulk::Choice, Vec<u8>)> = Vec::new();
    for k in 0..4u8 {
        if !columns[usize::from(k)] {
            continue;
        }
        let mut choice = settings.choices[usize::from(k)];
        if !choice.on {
            choice.source = Source::Own;
        }
        match groups.iter_mut().find(|(c, _)| *c == choice) {
            Some((_, list)) => list.push(k),
            None => groups.push((choice, vec![k])),
        }
    }
    let names = |list: &[u8]| -> String {
        let names: Vec<String> = list.iter().map(|k| grade(*k)).collect();
        match names.split_last() {
            Some((last, rest)) if !rest.is_empty() => fmt(
                tl!("{list} and {last}"),
                &[("list", &rest.join(", ")), ("last", last)],
            ),
            _ => names.concat(),
        }
    };
    let parts: Vec<String> = groups
        .iter()
        .map(|(choice, list)| {
            let many = list.len() > 1;
            let mods = names(list);
            let args = |source: &str| {
                vec![
                    ("modifications".to_owned(), mods.clone()),
                    ("look".to_owned(), look.name.clone()),
                    ("source".to_owned(), source.to_owned()),
                ]
            };
            let (template, source) = match (choice.on, choice.source, many) {
                (false, _, false) => (tl!("{modifications} is not touched"), String::new()),
                (false, _, true) => (tl!("{modifications} are not touched"), String::new()),
                (true, Source::Own, false) => {
                    (tl!("{modifications} gets its own {look}"), String::new())
                }
                (true, Source::Own, true) => {
                    (tl!("{modifications} get their own {look}"), String::new())
                }
                (true, Source::From(from), false) => (
                    tl!("{modifications} gets {look} from {source}"),
                    grade(from),
                ),
                (true, Source::From(from), true) => {
                    (tl!("{modifications} get {look} from {source}"), grade(from))
                }
            };
            let owned = args(&source);
            let pairs: Vec<(&str, &str)> = owned
                .iter()
                .map(|(a, b)| (a.as_str(), b.as_str()))
                .collect();
            fmt(template, &pairs)
        })
        .collect();
    let mut sentence = parts.join(", ");
    if !sentence.is_empty() {
        sentence.push('.');
    }
    sentence
}

/// What an item's change says in the review: "M0 ← XT M3 · M1 · M2".
fn change_detail(look: &Look, row: &bulk::Row) -> String {
    row.cells
        .iter()
        .enumerate()
        .filter_map(|(k, cell)| match cell {
            Some(Cell::Change { skin, how }) => Some(match how {
                How::Own => grade(k as u8),
                How::From(_) | How::Edition => {
                    format!("{} ← {}", grade(k as u8), skin_name(look, skin.source))
                }
                How::Picked => fmt(
                    tl!("{modification} ← {skin} (own pick)"),
                    &[
                        ("modification", &grade(k as u8)),
                        ("skin", &skin_name(look, skin.source)),
                    ],
                ),
            }),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Why a modification is skipped, in the review.
fn skip_reason(look: &Look, modification: u8, cell: &Cell) -> String {
    match cell {
        Cell::Missing { source } => fmt(
            tl!("No {skin} in the packs, {modification} stays as it is"),
            &[
                ("skin", &skin_name(look, *source)),
                ("modification", &grade(modification)),
            ],
        ),
        Cell::NotInGame => fmt(
            tl!("This item has no {modification} in the game"),
            &[("modification", &grade(modification))],
        ),
        Cell::Already => tl!("This skin is on it already").to_owned(),
        Cell::Incomplete { files } => fmt(
            tl!("The skin has no {files}, so it is not put on"),
            &[("files", &files.join(", "))],
        ),
        Cell::Change { .. } | Cell::Off => String::new(),
    }
}

/// Why a modification was skipped, short, for the result: "no XT M3".
fn short_reason(look: &Look, modification: u8, cell: &Cell) -> String {
    match cell {
        Cell::Missing { source } => fmt(tl!("no {skin}"), &[("skin", &skin_name(look, *source))]),
        Cell::NotInGame => fmt(
            tl!("no {modification} in the game"),
            &[("modification", &grade(modification))],
        ),
        Cell::Already => tl!("already on").to_owned(),
        Cell::Incomplete { .. } => tl!("incomplete skin").to_owned(),
        Cell::Change { .. } | Cell::Off => String::new(),
    }
}

/// The size of the cache files the run backs up first.
fn backup_size(app: &Studio, plan: &Plan) -> u64 {
    let Some(wizard) = app.bulk.as_ref() else {
        return 0;
    };
    plan.rows
        .iter()
        .flat_map(|row| {
            row.cells.iter().enumerate().filter_map(move |(k, cell)| {
                cell.as_ref()
                    .filter(|c| c.changes())
                    .map(|_| (row.kind.clone(), row.id.clone(), k as u8))
            })
        })
        .filter_map(|key| wizard.cache_files.get(&key).map(|(_, size)| *size))
        .sum()
}

fn check_step(app: &mut Studio, ctx: &Context, keys: bool, acts: &mut Vec<Act>) {
    let Some(wizard) = app.bulk.as_ref() else {
        return;
    };
    let (Some(look), Some(settings), Some(plan)) = (
        wizard.chosen().cloned(),
        wizard.settings.clone(),
        wizard.plan.clone(),
    ) else {
        return;
    };
    let backup = backup_size(app, &plan);
    let changing = plan.changing_items();
    let subtitle = fmt(
        tl!("Look: {look} · check what will change"),
        &[("look", &look.name)],
    );
    let steps = steps();
    let spec = spec(
        title(),
        Some(&subtitle),
        &steps,
        2,
        keys,
        BIG,
        Some((icons::INFO, tl!("Close the game before applying."))),
    );
    let apply_label = tn(changing as u64, "Apply to {n} item", "Apply to {n} items");
    let mut apply = false;
    let mut back = false;
    let event = widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let width = rect.width() - 48.0;
            let big = fmt(
                tl!("{skins} {items} will be replaced"),
                &[
                    ("skins", &tn(plan.changes() as u64, "{n} skin", "{n} skins")),
                    ("items", &tn(changing as u64, "on {n} item", "on {n} items")),
                ],
            );
            let sentence = mapping_sentence(&look, &settings, plan.columns);
            let sentence_style = Text::body(14.0, 400, tokens.text_2);
            let sentence_height = text_height(ui, &sentence, sentence_style, width, 19.0);
            let warn_title = tl!("This replaces the skins these items wear now");
            let warn_detail = tl!(
                "Including skins you put on earlier, one by one. Nothing else in the game changes."
            );
            let warn_style = Text::body(13.0, 400, tokens.text_2);
            let warn_room = width - 16.0 - 20.0 - 14.0 - 16.0;
            // 16 around, the title's line of 18, 4 between, the detail in lines of 16.
            let warn_height = 16.0
                + 18.0
                + 4.0
                + text_height(ui, warn_detail, warn_style, warn_room, 16.0)
                + 16.0;
            let fixed = 20.0
                + 30.0
                + 4.0
                + sentence_height
                + 16.0
                + 68.0
                + 16.0
                + warn_height
                + 16.0
                + 58.0
                + 16.0
                + 20.0;
            scroll_page(ui, rect, fixed + 160.0, |ui, content| {
                let inner = content.shrink2(Vec2::new(24.0, 20.0));
                let mut y = inner.min.y;
                let big_style = Text::display(24.0, 700, tokens.text);
                let shown = fonts::ellipsize(ui.painter(), &big, big_style, width);
                let used = fonts::draw(ui.painter(), Pos2::new(inner.min.x, y), &shown, big_style);
                label_node(ui, used, "summary", &big);
                y += 30.0 + 4.0;
                text_at(
                    ui,
                    Pos2::new(inner.min.x, y),
                    &sentence,
                    sentence_style,
                    width,
                    19.0,
                );
                y += sentence_height + 16.0;
                // Three numbers: what changes, what is skipped, the backup.
                let stats = [
                    (
                        icons::REFRESH_CW,
                        changing.to_string(),
                        tn(changing as u64, "item changes", "items change"),
                        tn(plan.changes() as u64, "{n} skin", "{n} skins"),
                    ),
                    (
                        icons::CIRCLE_MINUS,
                        plan.skipped_items().to_string(),
                        tn(plan.skipped_items() as u64, "item skipped", "items skipped"),
                        if plan.skipped_items() > 0 {
                            tl!("see the list").to_owned()
                        } else {
                            tl!("nothing skipped").to_owned()
                        },
                    ),
                    (
                        icons::HARD_DRIVE,
                        format_bytes(backup),
                        tl!("backup").to_owned(),
                        tl!("saved before replacing").to_owned(),
                    ),
                ];
                let gap = 12.0;
                let card_width = (inner.width() - gap * 2.0) / 3.0;
                for (index, (icon, value, label, sub)) in stats.iter().enumerate() {
                    let card = Rect::from_min_size(
                        Pos2::new(inner.min.x + index as f32 * (card_width + gap), y),
                        Vec2::new(card_width, 68.0),
                    );
                    stat_card(ui, card, *icon, value, label, sub, index == 0);
                }
                y += 68.0 + 16.0;
                // The warning: what is worn now is replaced.
                let warn = Rect::from_min_size(
                    Pos2::new(inner.min.x, y),
                    Vec2::new(inner.width(), warn_height),
                );
                draw::rounded(
                    ui.painter(),
                    warn,
                    CornerRadius::same(12),
                    tokens.accent_soft,
                    Some((1.0, tokens.accent)),
                );
                fonts::icon(
                    ui.painter(),
                    Pos2::new(warn.min.x + 16.0 + 10.0, warn.min.y + 16.0 + 10.0),
                    icons::TRIANGLE_ALERT,
                    20.0,
                    tokens.accent,
                );
                let text_x = warn.min.x + 16.0 + 20.0 + 14.0;
                let shown = fonts::ellipsize(
                    ui.painter(),
                    warn_title,
                    Text::body(15.0, 700, tokens.accent),
                    warn_room,
                );
                fonts::draw_left(
                    ui.painter(),
                    text_x,
                    warn.min.y + 16.0 + 9.0,
                    &shown,
                    Text::body(15.0, 700, tokens.accent),
                );
                text_at(
                    ui,
                    Pos2::new(text_x, warn.min.y + 16.0 + 18.0 + 4.0),
                    warn_detail,
                    warn_style,
                    warn_room,
                    16.0,
                );
                y += warn_height + 16.0;
                // The backup: always on.
                let boxed =
                    Rect::from_min_size(Pos2::new(inner.min.x, y), Vec2::new(inner.width(), 58.0));
                backup_box(ui, boxed);
                y += 58.0 + 16.0;
                let lists = Rect::from_min_max(Pos2::new(inner.min.x, y), inner.max);
                review_lists(ui, lists, &look, &plan);
            });
        },
        |ui| {
            apply = Button::new(ButtonKind::Accent, &apply_label)
                .icon(icons::LAYERS)
                .enabled(changing > 0)
                .show(ui)
                .clicked();
            back = Button::new(ButtonKind::Ghost, tl!("Back"))
                .icon(icons::ARROW_LEFT)
                .show(ui)
                .clicked();
        },
    );
    if apply {
        acts.push(Act::Next);
    } else if back {
        acts.push(Act::Back);
    } else if event == ModalEvent::Dismissed {
        acts.push(Act::Close);
    }
}

fn stat_card(ui: &mut Ui, card: Rect, icon: char, value: &str, label: &str, sub: &str, lead: bool) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        card,
        CornerRadius::same(12),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let badge = Rect::from_center_size(
        Pos2::new(card.min.x + 14.0 + 19.0, card.center().y),
        Vec2::splat(38.0),
    );
    let (fill, ink) = if lead {
        (tokens.primary_soft, tokens.primary)
    } else {
        (tokens.surface_3, tokens.text_2)
    };
    draw::rounded(ui.painter(), badge, CornerRadius::same(10), fill, None);
    fonts::icon(ui.painter(), badge.center(), icon, 18.0, ink);
    let x = badge.max.x + 12.0;
    let room = card.max.x - 14.0 - x;
    let value_style = Text::display(20.0, 700, tokens.text);
    let used = fonts::draw_left(ui.painter(), x, card.center().y - 8.0, value, value_style);
    let label_style = Text::body(13.0, 400, tokens.text_2);
    let label_room = card.max.x - 14.0 - used.max.x - 6.0;
    let shown = fonts::ellipsize(ui.painter(), label, label_style, label_room);
    fonts::draw_left(
        ui.painter(),
        used.max.x + 6.0,
        card.center().y - 4.0,
        &shown,
        label_style,
    );
    let sub_style = Text::body(12.0, 400, tokens.text_3);
    let shown = fonts::ellipsize(ui.painter(), sub, sub_style, room);
    fonts::draw_left(ui.painter(), x, card.center().y + 13.0, &shown, sub_style);
    label_node(ui, card, ("stat", label), &format!("{value} {label}"));
}

fn backup_box(ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    draw::rounded(
        ui.painter(),
        rect,
        CornerRadius::same(12),
        tokens.surface,
        Some((1.0, tokens.border)),
    );
    let check = Rect::from_min_size(
        Pos2::new(rect.min.x + 16.0, rect.center().y - 10.0),
        Vec2::splat(20.0),
    );
    widgets::paint_checkbox(ui, check, true, true);
    let heading = tl!("Make a backup first");
    let detail = tl!("Needed to undo everything in one click, so it is always on.");
    let x = check.max.x + 12.0;
    let room = rect.max.x - 16.0 - 15.0 - 12.0 - x;
    let style = Text::body(14.0, 600, tokens.text);
    let shown = fonts::ellipsize(ui.painter(), heading, style, room);
    fonts::draw_left(ui.painter(), x, rect.center().y - 9.0, &shown, style);
    let style = Text::body(12.0, 400, tokens.text_3);
    let shown = fonts::ellipsize(ui.painter(), detail, style, room);
    fonts::draw_left(ui.painter(), x, rect.center().y + 10.0, &shown, style);
    fonts::icon(
        ui.painter(),
        Pos2::new(rect.max.x - 16.0 - 7.5, rect.center().y),
        icons::LOCK,
        15.0,
        tokens.text_3,
    );
    // A checkbox that is on and cannot be turned off.
    let response = ui.interact(rect, ui.scope_id().with("backup"), Sense::hover());
    response.widget_info(move || egui::WidgetInfo::selected(Role::CheckBox, false, true, heading));
}

fn review_lists(ui: &mut Ui, rect: Rect, look: &Look, plan: &Plan) {
    let tokens = Tokens::get(ui.ctx());
    let gap = 12.0;
    let half = (rect.width() - gap) / 2.0;
    let left = Rect::from_min_size(rect.min, Vec2::new(half, rect.height()));
    let right = Rect::from_min_size(
        Pos2::new(rect.min.x + half + gap, rect.min.y),
        Vec2::new(half, rect.height()),
    );
    let caption = Text::display(11.0, 700, tokens.text_3).spaced(1.0);
    fonts::draw(
        ui.painter(),
        left.min,
        &tl!("Will change").to_uppercase(),
        caption,
    );
    fonts::draw(
        ui.painter(),
        right.min,
        &tl!("Will be skipped").to_uppercase(),
        caption,
    );
    let changes: Vec<&bulk::Row> = plan.rows.iter().filter(|r| r.changes()).collect();
    let skipped: Vec<(String, u8, &Cell)> = plan
        .rows
        .iter()
        .flat_map(|row| {
            row.cells
                .iter()
                .enumerate()
                .filter_map(move |(k, cell)| match cell {
                    Some(cell) if cell.skipped() => Some((row.name(), k as u8, cell)),
                    _ => None,
                })
        })
        .collect();
    let rows = changes.len().max(skipped.len()).max(1);
    let total = rows as f32 * 40.0;
    let list = Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y + 22.0), rect.max);
    in_rect(ui, list, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("bulk-review")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let top = ui.cursor().min;
                ui.allocate_exact_size(Vec2::new(list.width(), total), Sense::hover());
                for (index, row) in changes.iter().enumerate() {
                    let line = Rect::from_min_size(
                        Pos2::new(left.min.x, top.y + index as f32 * 40.0),
                        Vec2::new(half, 36.0),
                    );
                    draw::rounded(
                        ui.painter(),
                        line,
                        CornerRadius::same(10),
                        tokens.surface,
                        None,
                    );
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(line.min.x + 12.0 + 7.0, line.center().y),
                        icons::CHECK,
                        14.0,
                        tokens.primary,
                    );
                    let name_style = Text::body(13.0, 600, tokens.text);
                    let name = row.name();
                    let shown = fonts::ellipsize(ui.painter(), &name, name_style, 90.0);
                    fonts::draw_left(
                        ui.painter(),
                        line.min.x + 12.0 + 14.0 + 10.0,
                        line.center().y,
                        &shown,
                        name_style,
                    );
                    // The packs the item's skins come from, on the right.
                    let mut packs: Vec<&str> = Vec::new();
                    for cell in row.cells.iter().flatten() {
                        if let Cell::Change { skin, .. } = cell
                            && !packs.contains(&skin.pack_name.as_str())
                        {
                            packs.push(&skin.pack_name);
                        }
                    }
                    let pack_style = Text::body(12.0, 400, tokens.text_3);
                    let packs = packs.join(", ");
                    let pack_width = fonts::width(ui.painter(), &packs, pack_style).min(half * 0.3);
                    let packs_shown =
                        fonts::ellipsize(ui.painter(), &packs, pack_style, pack_width);
                    fonts::draw_right(
                        ui.painter(),
                        line.max.x - 12.0,
                        line.center().y,
                        &packs_shown,
                        pack_style,
                    );
                    let detail_x = line.min.x + 12.0 + 14.0 + 10.0 + 90.0 + 10.0;
                    let detail_style = Text::mono(12.0, 400, tokens.text_2);
                    let detail = change_detail(look, row);
                    let room = line.max.x - 12.0 - pack_width - 10.0 - detail_x;
                    let shown = fonts::ellipsize(ui.painter(), &detail, detail_style, room);
                    fonts::draw_left(
                        ui.painter(),
                        detail_x,
                        line.center().y,
                        &shown,
                        detail_style,
                    );
                    label_node(ui, line, ("change", &row.id), &format!("{name} {detail}"));
                }
                for (index, (name, k, cell)) in skipped.iter().enumerate() {
                    let line = Rect::from_min_size(
                        Pos2::new(right.min.x, top.y + index as f32 * 40.0),
                        Vec2::new(half, 36.0),
                    );
                    draw::rounded(
                        ui.painter(),
                        line,
                        CornerRadius::same(10),
                        egui::Color32::TRANSPARENT,
                        Some((1.0, tokens.border)),
                    );
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(line.min.x + 12.0 + 7.0, line.center().y),
                        icons::CIRCLE_MINUS,
                        14.0,
                        tokens.text_3,
                    );
                    let title = format!("{name} {}", grade(*k));
                    let name_style = Text::body(13.0, 600, tokens.text);
                    let shown = fonts::ellipsize(ui.painter(), &title, name_style, 110.0);
                    fonts::draw_left(
                        ui.painter(),
                        line.min.x + 12.0 + 14.0 + 10.0,
                        line.center().y,
                        &shown,
                        name_style,
                    );
                    let reason = skip_reason(look, *k, cell);
                    let x = line.min.x + 12.0 + 14.0 + 10.0 + 110.0 + 10.0;
                    let style = Text::body(12.0, 400, tokens.text_3);
                    let shown =
                        fonts::ellipsize(ui.painter(), &reason, style, line.max.x - 12.0 - x);
                    fonts::draw_left(ui.painter(), x, line.center().y, &shown, style);
                    label_node(
                        ui,
                        line,
                        ("skipped", name, *k),
                        &format!("{title}: {reason}"),
                    );
                }
                if skipped.is_empty() {
                    let line =
                        Rect::from_min_size(Pos2::new(right.min.x, top.y), Vec2::new(half, 36.0));
                    fonts::draw_left(
                        ui.painter(),
                        line.min.x,
                        line.center().y,
                        tl!("Nothing is skipped."),
                        Text::body(12.0, 400, tokens.text_3),
                    );
                }
            });
    });
}

// ---------------------------------------------------------------------------------------------
// The run.

fn running_step(app: &mut Studio, ctx: &Context, acts: &mut Vec<Act>) {
    let Some(run) = app.bulk.as_ref().and_then(|w| w.run.as_ref()) else {
        return;
    };
    let total = run.tasks.len();
    let done = run.done();
    let stopping = run.stop.load(std::sync::atomic::Ordering::SeqCst);
    let title = fmt(
        tl!("Applying {look} to {items}"),
        &[
            ("look", &run.look),
            ("items", &tn(total as u64, "{n} item", "{n} items")),
        ],
    );
    let spec = spec(
        &title,
        Some(tl!("Each item is backed up, replaced and checked")),
        &[],
        0,
        false,
        RUN,
        Some((
            icons::INFO,
            tl!("Items already done stay done; you can undo them after."),
        )),
    );
    let current = current_line(run);
    let names: Vec<(String, ItemState)> = run
        .tasks
        .iter()
        .map(Task::name)
        .zip(run.states.iter().cloned())
        .collect();
    let mut stop = false;
    widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let inner = rect.shrink(24.0);
            let fraction = if total == 0 {
                0.0
            } else {
                done as f32 / total as f32
            };
            let label = fmt(
                tl!("{done} of {total} items"),
                &[("done", &done.to_string()), ("total", &total.to_string())],
            );
            fonts::draw_left(
                ui.painter(),
                inner.min.x,
                inner.min.y + 8.0,
                &label,
                Text::body(13.0, 400, tokens.text),
            );
            fonts::draw_right(
                ui.painter(),
                inner.max.x,
                inner.min.y + 8.0,
                &format!("{}%", (fraction * 100.0).floor() as u32),
                Text::mono(12.0, 400, tokens.primary),
            );
            let bar = Rect::from_min_size(
                Pos2::new(inner.min.x, inner.min.y + 23.0),
                Vec2::new(inner.width(), 6.0),
            );
            widgets::progress_bar(ui, bar, fraction);
            let progress = ui.interact(bar, ui.scope_id().with("bulk-progress"), Sense::hover());
            let name = label.clone();
            progress.widget_info(move || {
                let mut info =
                    egui::WidgetInfo::labeled(Role::ProgressIndicator, true, name.clone());
                info.value = Some(f64::from(fraction));
                info
            });
            fonts::draw_left(
                ui.painter(),
                inner.min.x,
                inner.min.y + 53.0,
                &current,
                Text::mono(12.0, 400, tokens.text_2),
            );
            let list = Rect::from_min_max(Pos2::new(inner.min.x, inner.min.y + 77.0), inner.max);
            item_states(ui, list, &names, true);
        },
        |ui| {
            let label = if stopping {
                tl!("Stopping after this item…")
            } else {
                tl!("Stop after this item")
            };
            stop = Button::new(ButtonKind::Secondary, label)
                .icon(icons::SQUARE)
                .enabled(!stopping)
                .show(ui)
                .clicked();
        },
    );
    ctx.request_repaint_after(std::time::Duration::from_millis(50));
    if stop {
        acts.push(Act::Stop);
    }
}

/// "Viking · M0 ← XT M0": what is written now.
fn current_line(run: &BulkRun) -> String {
    run.states
        .iter()
        .zip(&run.tasks)
        .find_map(|(state, task)| match state {
            ItemState::Writing(k) => {
                let skin = task.writes.iter().find(|w| w.modification == *k)?;
                let source = skin.skin.source.map_or_else(
                    || run.look.clone(),
                    |s| format!("{} {}", run.look, grade(s)),
                );
                Some(format!("{} · {} ← {source}", task.name(), grade(*k)))
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// The items of the run with what happened to each.
fn item_states(ui: &mut Ui, rect: Rect, items: &[(String, ItemState)], follow: bool) {
    let tokens = Tokens::get(ui.ctx());
    let time = ui.input(|i| i.time) as f32;
    in_rect(ui, rect, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("bulk-items")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                for (index, (name, state)) in items.iter().enumerate() {
                    let (line, _) =
                        ui.allocate_exact_size(Vec2::new(rect.width(), 35.0), Sense::hover());
                    let now = matches!(state, ItemState::Writing(_));
                    let waiting = matches!(state, ItemState::Waiting | ItemState::NotRun);
                    let fill = if now {
                        tokens.surface_3
                    } else {
                        tokens.surface
                    };
                    let alpha = if waiting { 0.6 } else { 1.0 };
                    draw::rounded(
                        ui.painter(),
                        line,
                        CornerRadius::same(10),
                        draw::fade(fill, alpha),
                        None,
                    );
                    let center = Pos2::new(line.min.x + 12.0 + 8.0, line.center().y);
                    let (detail, ink) = match state {
                        ItemState::Waiting => {
                            fonts::icon(
                                ui.painter(),
                                center,
                                icons::CIRCLE_DASHED,
                                16.0,
                                tokens.text_3,
                            );
                            (tl!("waiting").to_owned(), tokens.text_3)
                        }
                        ItemState::Writing(k) => {
                            fonts::icon_rotated(
                                ui.painter(),
                                center,
                                icons::LOADER,
                                16.0,
                                tokens.accent,
                                fonts::spinner_angle(time),
                            );
                            (
                                fmt(
                                    tl!("replacing {modification}…"),
                                    &[("modification", &grade(*k))],
                                ),
                                tokens.text,
                            )
                        }
                        ItemState::Applied { skins } => {
                            fonts::icon(
                                ui.painter(),
                                center,
                                icons::CIRCLE_CHECK,
                                16.0,
                                tokens.primary,
                            );
                            (tn(*skins as u64, "{n} skin", "{n} skins"), tokens.text_3)
                        }
                        ItemState::Failed { .. } => {
                            fonts::icon(
                                ui.painter(),
                                center,
                                icons::TRIANGLE_ALERT,
                                16.0,
                                tokens.danger,
                            );
                            (tl!("not applied").to_owned(), tokens.danger)
                        }
                        ItemState::NotRun => {
                            fonts::icon(
                                ui.painter(),
                                center,
                                icons::CIRCLE_MINUS,
                                16.0,
                                tokens.text_3,
                            );
                            (tl!("not applied").to_owned(), tokens.text_3)
                        }
                    };
                    let detail_style = Text::body(12.0, 400, ink);
                    let used = fonts::draw_right(
                        ui.painter(),
                        line.max.x - 12.0,
                        line.center().y,
                        &detail,
                        detail_style,
                    );
                    let name_style = Text::body(14.0, 600, draw::fade(tokens.text, alpha));
                    let x = line.min.x + 12.0 + 16.0 + 10.0;
                    let shown =
                        fonts::ellipsize(ui.painter(), name, name_style, used.min.x - 10.0 - x);
                    fonts::draw_left(ui.painter(), x, line.center().y, &shown, name_style);
                    label_node(ui, line, ("item", index), &format!("{name}: {detail}"));
                    if follow && now {
                        ui.scroll_to_rect(line, None);
                    }
                }
            });
    });
}

// ---------------------------------------------------------------------------------------------
// The result.

fn done_step(app: &mut Studio, ctx: &Context, keys: bool, acts: &mut Vec<Act>) {
    let Some(wizard) = app.bulk.as_ref() else {
        return;
    };
    let Some(run) = wizard.run.as_ref() else {
        return;
    };
    let look = wizard.chosen().cloned();
    let applied = run.applied();
    let title = if applied == 0 {
        tl!("Nothing was put on").to_owned()
    } else {
        fmt(
            tl!("{look} is on {items}"),
            &[
                ("look", &run.look),
                ("items", &tn(applied as u64, "{n} item", "{n} items")),
            ],
        )
    };
    let subtitle = fmt(
        tl!("{skins} replaced · backup saved"),
        &[("skins", &tn(run.skins() as u64, "{n} skin", "{n} skins"))],
    );
    let spec = spec(&title, Some(&subtitle), &[], 0, keys, DONE, None);
    // What did not go: each failed item with its reason, and why the run stopped early.
    let mut problems: Vec<String> = run
        .tasks
        .iter()
        .zip(&run.states)
        .filter_map(|(task, state)| match state {
            ItemState::Failed {
                modification,
                failure,
                kept,
            } => Some(fmt(
                tl!("{item} · {modification}: {reason}. Nothing changed for this item."),
                &[
                    ("item", &task.name()),
                    ("modification", &grade(*modification)),
                    (
                        "reason",
                        &lower_first(
                            crate::actions::failure_text(failure, kept.as_deref())
                                .trim_end_matches('.'),
                        ),
                    ),
                ],
            )),
            _ => None,
        })
        .collect();
    let not_run = run
        .states
        .iter()
        .filter(|s| **s == ItemState::NotRun)
        .count();
    if let Some((true, game)) = &run.finished
        && not_run > 0
    {
        problems.push(if game.is_empty() {
            fmt(
                tl!("Stopped: {items} not changed."),
                &[("items", &tn(not_run as u64, "{n} item", "{n} items"))],
            )
        } else {
            fmt(
                tl!("The game was started ({names}), so the rest stopped: {items} not changed."),
                &[
                    ("names", &game.join(", ")),
                    ("items", &tn(not_run as u64, "{n} item", "{n} items")),
                ],
            )
        });
    }
    let skipped_text = look.as_ref().and_then(|look| {
        if run.skipped.is_empty() {
            return None;
        }
        let shown: Vec<String> = run
            .skipped
            .iter()
            .take(6)
            .map(|(name, k, cell)| {
                format!("{name} {} ({})", grade(*k), short_reason(look, *k, cell))
            })
            .collect();
        let mut list = shown.join(", ");
        if run.skipped.len() > 6 {
            list = fmt(
                tl!("{list} and {n} more"),
                &[("list", &list), ("n", &(run.skipped.len() - 6).to_string())],
            );
        }
        Some(fmt(tl!("Skipped: {list}."), &[("list", &list)]))
    });
    let stats = [
        (icons::CHECK, applied, tl!("applied"), Tone::Ok),
        (
            icons::CIRCLE_MINUS,
            run.skipped_items,
            tl!("skipped"),
            Tone::Neutral,
        ),
        (
            icons::TRIANGLE_ALERT,
            run.not_applied(),
            tl!("not applied"),
            Tone::Missing,
        ),
    ];
    let mut undo = false;
    let mut done = false;
    let event = widgets::wizard(
        ctx,
        &spec,
        |ui, rect| {
            let tokens = Tokens::get(ui.ctx());
            let width = rect.width() - 48.0;
            let style = Text::body(13.0, 400, tokens.text);
            let box_room = width - 12.0 - 16.0 - 10.0 - 12.0;
            let problem_heights: Vec<f32> = problems
                .iter()
                .map(|p| text_height(ui, p, style, box_room, 16.0) + 24.0)
                .collect();
            let skipped_height = skipped_text.as_ref().map_or(0.0, |t| {
                text_height(ui, t, style, box_room, 16.0) + 24.0 + 14.0
            });
            let undo_note = tl!(
                "Undo puts back what every item wore before. It is also in Packs → Backups as one entry."
            );
            let note_style = Text::body(12.0, 400, tokens.text_3);
            let note_height = text_height(ui, undo_note, note_style, width - 26.0, 16.0);
            let fixed = 24.0
                + 78.0
                + 14.0
                + problem_heights.iter().map(|h| h + 14.0).sum::<f32>()
                + skipped_height
                + note_height
                + 24.0;
            scroll_page(ui, rect, fixed, |ui, content| {
                let inner = content.shrink(24.0);
                let mut y = inner.min.y;
                let gap = 10.0;
                let card_width = (inner.width() - gap * 2.0) / 3.0;
                for (index, (icon, value, label, tone)) in stats.iter().enumerate() {
                    let card = Rect::from_min_size(
                        Pos2::new(inner.min.x + index as f32 * (card_width + gap), y),
                        Vec2::new(card_width, 78.0),
                    );
                    draw::rounded(
                        ui.painter(),
                        card,
                        CornerRadius::same(12),
                        tokens.surface,
                        Some((1.0, tokens.border)),
                    );
                    let ink = match tone {
                        Tone::Ok => tokens.primary,
                        Tone::Missing => tokens.danger,
                        _ => tokens.text_2,
                    };
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(card.min.x + 14.0 + 8.0, card.min.y + 28.0),
                        *icon,
                        16.0,
                        ink,
                    );
                    fonts::draw_left(
                        ui.painter(),
                        card.min.x + 14.0 + 16.0 + 8.0,
                        card.min.y + 28.0,
                        &value.to_string(),
                        Text::display(22.0, 700, tokens.text),
                    );
                    fonts::draw_left(
                        ui.painter(),
                        card.min.x + 14.0,
                        card.min.y + 56.0,
                        label,
                        Text::body(12.0, 400, tokens.text_2),
                    );
                    label_node(ui, card, ("result", index), &format!("{value} {label}"));
                }
                y += 78.0 + 14.0;
                for (problem, height) in problems.iter().zip(&problem_heights) {
                    let boxed = Rect::from_min_size(
                        Pos2::new(inner.min.x, y),
                        Vec2::new(inner.width(), *height),
                    );
                    draw::rounded(
                        ui.painter(),
                        boxed,
                        CornerRadius::same(10),
                        tokens.danger_soft,
                        None,
                    );
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(boxed.min.x + 12.0 + 8.0, boxed.min.y + 12.0 + 8.0),
                        icons::TRIANGLE_ALERT,
                        16.0,
                        tokens.danger,
                    );
                    text_at(
                        ui,
                        Pos2::new(boxed.min.x + 12.0 + 16.0 + 10.0, boxed.min.y + 12.0),
                        problem,
                        style,
                        box_room,
                        16.0,
                    );
                    label_node(ui, boxed, ("problem", problem), problem);
                    y += height + 14.0;
                }
                if let Some(text) = &skipped_text {
                    let height = skipped_height - 14.0;
                    let boxed = Rect::from_min_size(
                        Pos2::new(inner.min.x, y),
                        Vec2::new(inner.width(), height),
                    );
                    draw::rounded(
                        ui.painter(),
                        boxed,
                        CornerRadius::same(10),
                        egui::Color32::TRANSPARENT,
                        Some((1.0, tokens.border)),
                    );
                    fonts::icon(
                        ui.painter(),
                        Pos2::new(boxed.min.x + 12.0 + 8.0, boxed.min.y + 12.0 + 8.0),
                        icons::CIRCLE_MINUS,
                        16.0,
                        tokens.text_3,
                    );
                    text_at(
                        ui,
                        Pos2::new(boxed.min.x + 12.0 + 16.0 + 10.0, boxed.min.y + 12.0),
                        text,
                        Text::body(13.0, 400, tokens.text_2),
                        box_room,
                        16.0,
                    );
                    label_node(ui, boxed, "skipped-summary", text);
                    y += skipped_height;
                }
                fonts::icon(
                    ui.painter(),
                    Pos2::new(inner.min.x + 8.0, y + 8.0),
                    icons::HISTORY,
                    16.0,
                    tokens.text_3,
                );
                text_at(
                    ui,
                    Pos2::new(inner.min.x + 26.0, y),
                    undo_note,
                    note_style,
                    width - 26.0,
                    16.0,
                );
            });
        },
        |ui| {
            done = Button::new(ButtonKind::Primary, tl!("Done"))
                .icon(icons::CHECK)
                .show(ui)
                .clicked();
            undo = Button::new(ButtonKind::Secondary, tl!("Undo all"))
                .icon(icons::UNDO_2)
                .enabled(applied > 0)
                .show(ui)
                .clicked();
        },
    );
    if undo {
        acts.push(Act::Undo);
    } else if done || event == ModalEvent::Dismissed {
        acts.push(Act::Close);
    }
}
