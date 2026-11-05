// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The 3D preview on screen: the stage in "My tank" with its toolbar and before/after divider, the
//! three settings (shown in the Garage group of Settings and in the stage's own quick dialog, drawn
//! by the same code), and the dialog itself.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Ui, Vec2};
use tanki_i18n::{fmt, tl};
use tanki_ui::{
    draw, fonts,
    fonts::Text,
    icons,
    theme::Tokens,
    widgets::{self, ChipKind, GroupRow, ModalEvent, ModalSpec, Tint},
};
use tanki_viewer::{Quality, QualityPreference, StageView};

use crate::{
    app::{Dialog, Selection, Studio},
    preview3d::{Keyed, Preview3d, SlotState},
};

const SEGMENTS: f32 = 32.0;
/// How faded the model before is while the next one loads.
const LOADING_OPACITY: f32 = 0.4;
/// The stage's toolbar buttons and the gaps between them.
const TOOL: f32 = 28.0;
const TOOL_GAP: f32 = 2.0;

// --- The three settings -----------------------------------------------------------------------

fn quality_items() -> [(&'static str, Option<String>); 4] {
    [
        (tl!("Auto"), None),
        (tl!("Economy"), None),
        (tl!("Balanced"), None),
        (tl!("High"), None),
    ]
}

fn quality_index(app: &Studio) -> usize {
    QualityPreference::ALL
        .iter()
        .position(|q| *q == app.preview.quality)
        .unwrap_or(0)
}

fn quality_name(quality: Quality) -> &'static str {
    match quality {
        Quality::Economy => tl!("Economy (basic graphics)"),
        Quality::Balanced => tl!("Balanced (integrated graphics)"),
        Quality::High => tl!("High (discrete graphics)"),
    }
}

/// What the quality row says under its title.
fn quality_hint(app: &Studio) -> String {
    match app.preview.quality {
        QualityPreference::Auto => format!(
            "{} {}",
            tl!("Auto:"),
            quality_name(app.preview.stage.detected_quality())
        ),
        _ => tl!("Lower is smoother on older computers").to_owned(),
    }
}

/// The 3D rows of a settings group: the switch, the whole tank, and the quality.
pub struct SettingsRows {
    quality: Vec2,
    hint: String,
}

pub fn settings_rows(app: &Studio, ui: &Ui) -> SettingsRows {
    SettingsRows {
        quality: Vec2::new(
            widgets::choice_segments_width(ui, &quality_items(), quality_index(app)),
            SEGMENTS,
        ),
        hint: quality_hint(app),
    }
}

impl SettingsRows {
    pub fn rows(&self) -> [GroupRow<'_>; 3] {
        [
            GroupRow {
                icon: icons::BOX,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Show the tank in 3D"),
                desc: Some(tl!("Turn off to show the flat picture.")),
                control: Vec2::new(40.0, 22.0),
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::TANK,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Show the whole tank"),
                desc: Some(tl!(
                    "The hull, turret and paint last chosen, together. Off: the selected item alone."
                )),
                control: Vec2::new(40.0, 22.0),
                extra: 0.0,
                click: false,
            },
            GroupRow {
                icon: icons::GAUGE,
                mark: None,
                tint: Tint::Plain,
                title: tl!("Quality"),
                desc: Some(&self.hint),
                control: self.quality,
                extra: 0.0,
                click: false,
            },
        ]
    }

    pub fn height(&self, ui: &Ui, width: f32) -> f32 {
        widgets::group_height(ui, width, &self.rows())
    }

    /// Draw the rows at `origin` and act on them.
    pub fn draw(&self, app: &mut Studio, ui: &mut Ui, origin: Pos2, width: f32, salt: &str) {
        let rows = self.rows();
        let places = widgets::group(ui, origin, width, &rows, salt);
        let (Some(switch), Some(whole), Some(quality)) =
            (places.first(), places.get(1), places.get(2))
        else {
            return;
        };
        let mut on = app.preview.enabled;
        let changed = tanki_ui::layout::in_rect(ui, switch.control, |ui| {
            widgets::toggle(ui, &mut on, tl!("Show the tank in 3D")).changed()
        });
        if changed {
            app.set_preview_3d(on);
        }
        // The other rows mean nothing while the preview is off: shown, but not to be changed.
        let enabled = app.preview.enabled;
        let mut whole_tank = app.preview.whole_tank;
        let changed = ui
            .scope(|ui| {
                if !enabled {
                    ui.disable();
                    ui.set_opacity(0.45);
                }
                tanki_ui::layout::in_rect(ui, whole.control, |ui| {
                    widgets::toggle(ui, &mut whole_tank, tl!("Show the whole tank")).changed()
                })
            })
            .inner;
        if changed && enabled {
            app.set_whole_tank(whole_tank);
        }
        let index = quality_index(app);
        let clicked = ui
            .scope(|ui| {
                if !enabled {
                    ui.disable();
                    ui.set_opacity(0.45);
                }
                widgets::choice_segments(
                    ui,
                    quality.control.min,
                    SEGMENTS,
                    &quality_items(),
                    index,
                    &format!("{salt}-quality"),
                )
                .1
            })
            .inner;
        if let Some(pref) = clicked
            .filter(|_| enabled)
            .and_then(|i| QualityPreference::ALL.get(i))
        {
            app.set_stage_quality(*pref);
        }
    }
}

// --- The quick dialog ---------------------------------------------------------------------------

/// The stage's own settings: the same three rows as in Settings.
pub fn dialog(app: &mut Studio, ctx: &egui::Context) {
    const WIDTH: f32 = 520.0;
    let probe = Ui::new(
        ctx.clone(),
        egui::Id::unique("preview-3d-probe"),
        egui::UiBuilder::new(),
    );
    let rows = settings_rows(app, &probe);
    let height = rows.height(&probe, WIDTH - 48.0);
    let spec = ModalSpec {
        id: "preview-3d",
        title: tl!("3D preview"),
        close_label: tl!("Close"),
        width: WIDTH,
        body_height: height + 24.0,
    };
    // The settings apply as they change: no buttons, the × closes it.
    let event = widgets::modal_without_footer(ctx, &spec, |ui| {
        let body = ui.max_rect();
        rows.draw(app, ui, body.min, body.width(), "preview-3d-dialog");
    });
    if event == ModalEvent::Dismissed {
        app.dialog = None;
    }
}

// --- The stage in "My tank" ----------------------------------------------------------------------

/// The looks the stage shows: the main one (the picked skin, else the item as in the game) and,
/// when a skin is picked that is not in the game yet, the item as in the game to compare with.
pub struct Looks {
    pub main: Option<Keyed>,
    pub compared: Option<Keyed>,
    /// The item, so a new one starts from the home view.
    pub item: String,
    /// Under the model once the drag hint is gone: whether the skin is in the game.
    pub caption: String,
}

/// What the stage shows for `selection`, the same in the Garage and on the item's page: the picked
/// skin (else the item as in the game), the whole tank when that is on, and a caption saying
/// whether the look is in the game.
pub fn looks(app: &mut Studio, selection: &Selection) -> Looks {
    let chosen = app.chosen_variant(selection);
    let in_game = chosen.is_some() && chosen == app.applied_variant(selection);
    let (main, compared) = app.stage_looks(selection, chosen, in_game);
    let caption = if in_game {
        tl!("In the game now").to_owned()
    } else if chosen.is_some() {
        tl!("Skin preview · not applied").to_owned()
    } else {
        app.in_game_name(selection).map_or_else(
            || tl!("Current look in the game").to_owned(),
            |name| fmt(tl!("In the game: {name}"), &[("name", &name)]),
        )
    };
    // What the view is kept for: the tank (its hull and turret), a paint, or an item alone.
    let item = if main.as_ref().is_some_and(|look| look.parts.len() > 1) {
        let tank = &app.preview.tank;
        format!("tank:{}:{}", tank.hull.0, tank.turret.0)
    } else if selection.kind == "paint" {
        "paint".to_owned()
    } else {
        format!("{}:{}", selection.kind, selection.id)
    };
    Looks {
        main,
        compared,
        item,
        caption,
    }
}

/// The toolbar, its right edge at `right`; returns its left edge, for the Details button to keep
/// clear of. Only the settings button shows while the stage does not.
pub fn toolbar(app: &mut Studio, ui: &mut Ui, right: f32, top: f32, stage_shown: bool) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    let can_compare = stage_shown && app.preview.stage.key(Preview3d::COMPARED).is_some();
    let tools: Vec<(char, &str, bool, bool)> = if stage_shown {
        vec![
            (
                icons::COLUMNS_2,
                tl!("Compare with the game"),
                app.preview.compare.is_some(),
                can_compare,
            ),
            (
                icons::ROTATE_3D,
                tl!("Turn by itself"),
                app.preview.auto_rotating(),
                true,
            ),
            (icons::SCAN, tl!("Reset the view"), false, true),
            (icons::CAMERA, tl!("Save a picture"), false, true),
        ]
    } else {
        Vec::new()
    };
    let divider = if tools.is_empty() { 0.0 } else { 9.0 };
    let count = tools.len() as f32 + 1.0;
    let width = 4.0 + count * TOOL + (count - 1.0) * TOOL_GAP + divider;
    let bar = Rect::from_min_size(Pos2::new(right - width, top), Vec2::new(width, TOOL + 4.0));
    draw::rounded(
        ui.painter(),
        bar,
        CornerRadius::same(9),
        tokens.surface_2,
        Some((1.0, tokens.border)),
    );
    let mut x = bar.min.x + 2.0;
    let mut clicked = None;
    for (index, (glyph, label, active, enabled)) in tools.iter().enumerate() {
        let rect = Rect::from_min_size(Pos2::new(x, bar.min.y + 2.0), Vec2::splat(TOOL));
        if tool(ui, rect, *glyph, label, *active, *enabled) {
            clicked = Some(index);
        }
        x += TOOL + TOOL_GAP;
    }
    if divider > 0.0 {
        draw::vrule(
            ui.painter(),
            x + (divider - TOOL_GAP) / 2.0 - 1.0,
            bar.min.y + 8.0,
            bar.max.y - 8.0,
            tokens.border,
        );
        x += divider - TOOL_GAP;
    }
    let settings = Rect::from_min_size(Pos2::new(x, bar.min.y + 2.0), Vec2::splat(TOOL));
    if tool(
        ui,
        settings,
        icons::SLIDERS_HORIZONTAL,
        tl!("3D settings"),
        false,
        true,
    ) {
        app.dialog = Some(Dialog::Preview3d);
    }
    match clicked {
        Some(0) => app.preview.toggle_compare(),
        Some(1) => app.preview.toggle_auto_rotate(),
        Some(2) => app.preview.stage.camera_mut().go_home(),
        Some(3) => app.save_stage_picture(),
        _ => {}
    }
    bar.min.x
}

/// One toolbar button. Returns whether it was clicked.
fn tool(ui: &mut Ui, rect: Rect, glyph: char, label: &str, active: bool, enabled: bool) -> bool {
    let tokens = Tokens::get(ui.ctx());
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let response = ui.interact(rect, ui.make_persistent_id(("stage-tool", label)), sense);
    let text = label.to_owned();
    response.widget_info(move || {
        egui::WidgetInfo::selected(egui::Role::Button, enabled, active, text.clone())
    });
    if active {
        draw::rounded(
            ui.painter(),
            rect,
            CornerRadius::same(7),
            tokens.primary_soft,
            None,
        );
    } else if enabled && response.hovered() {
        draw::rounded(
            ui.painter(),
            rect,
            CornerRadius::same(7),
            tokens.surface_3,
            None,
        );
    }
    let ink = match (active, enabled) {
        (true, _) => tokens.primary,
        (false, true) => tokens.text_2,
        (false, false) => tokens.text_3,
    };
    fonts::icon(ui.painter(), rect.center(), glyph, 15.0, ink);
    let response = response.on_hover_text(label);
    enabled
        && response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
}

/// Draw the stage: turned from `scene`, the room between the chips and the modification selector,
/// and drawn over the whole height of `picture` (the chips, the toolbar and the selector lie over
/// the model, as in a garage), in the shape slot `under` reserved below them, with a soft light
/// behind it in the slot `backdrop` below that.
/// Returns false when there is no 3D picture to show (the caller draws the flat one).
pub fn stage(
    app: &mut Studio,
    ui: &mut Ui,
    scene: Rect,
    picture: Rect,
    [backdrop, under]: [egui::layers::ShapeIdx; 2],
    looks: Looks,
) -> bool {
    let tokens = Tokens::get(ui.ctx());
    app.preview.frame_item(&looks.item);
    let main = app.preview.want(Preview3d::MAIN, looks.main.as_ref());
    let compared = app
        .preview
        .want(Preview3d::COMPARED, looks.compared.as_ref());
    if compared != SlotState::Ready {
        app.preview.compare = None;
    }
    // While the next model loads, the one before stays on the stage, faded under a loading pill
    // (no blank frame between them); only the first one loads on an empty stage.
    let loading = main == SlotState::Loading;
    let shown_before = app.preview.stage.key(Preview3d::MAIN).is_some();
    match main {
        SlotState::Unavailable => return false,
        SlotState::Loading if !shown_before => {
            let c = scene.center();
            let seconds = ui.input(|i| i.time) as f32;
            fonts::icon_rotated(
                ui.painter(),
                Pos2::new(c.x, c.y - 12.0),
                icons::LOADER,
                22.0,
                tokens.text_3,
                fonts::spinner_angle(seconds),
            );
            fonts::draw_centered(
                ui.painter(),
                Rect::from_center_size(Pos2::new(c.x, c.y + 18.0), Vec2::new(scene.width(), 16.0)),
                tl!("Loading 3D model…"),
                Text::body(12.0, 400, tokens.text_3),
            );
            ui.ctx().request_repaint();
            return true;
        }
        SlotState::Loading | SlotState::Ready => {}
    }
    let opacity = ui.ctx().animate_value_with_time(
        ui.make_persistent_id("preview-3d-loading-fade"),
        if loading { LOADING_OPACITY } else { 1.0 },
        0.2,
    );
    app.preview.stage.set_opacity(opacity);
    // The hint under the model until the stage is first used, then the caption in its place.
    let note = Rect::from_min_size(
        Pos2::new(scene.min.x, scene.max.y - 24.0),
        Vec2::new(scene.width(), 24.0),
    );
    let mut room = Rect::from_min_max(scene.min, Pos2::new(scene.max.x, note.min.y - 8.0));
    let view = match app.preview.compare {
        Some(split) => {
            // The labels' row first; the model in the room left under it.
            let labels_y = room.min.y;
            room.min.y += 24.0 + 8.0;
            compare_labels(ui, room.min.x + room.width() * split, labels_y);
            StageView::Compare { split }
        }
        None => StageView::Single,
    };
    // The model's picture keeps the room's sides (the compare divider lines up with it) and grows
    // as far up and down as the stage allows on both sides, so the model stays centred between the
    // chips and the selector while its edges may pass under them; while comparing, it stays under
    // the labels.
    let grow = if app.preview.compare.is_some() {
        0.0
    } else {
        (room.min.y - picture.min.y)
            .min(picture.max.y - room.max.y)
            .max(0.0)
    };
    let drawn = room.expand2(Vec2::new(0.0, grow));
    // A soft green light behind the model, fading to the stage's dark edges: the model stands out
    // of its background and belongs to it.
    ui.painter().set(
        backdrop,
        draw::glow(
            Rect::from_center_size(drawn.center(), drawn.size() * Vec2::new(0.7, 0.95)),
            tokens.primary.gamma_multiply(0.09),
        ),
    );
    let response = app.preview.stage.show_in(
        ui,
        drawn,
        room,
        view,
        tl!("3D view of the item"),
        Some(under),
    );
    if response.dragged() || response.double_clicked() {
        app.preview.used = true;
    }
    if let Some(split) = app.preview.compare {
        app.preview.compare = Some(divider(ui, room, split));
    }
    if loading {
        loading_pill(ui, room.center());
        ui.ctx().request_repaint();
    }
    if app.preview.used {
        fonts::draw_centered(
            ui.painter(),
            note,
            &looks.caption,
            Text::mono(12.0, 400, tokens.text_3),
        );
    } else {
        hint(ui, note);
    }
    true
}

/// "In the game" left of the divider and "Skin" right of it, 12 from it.
fn compare_labels(ui: &Ui, divider_x: f32, y: f32) {
    let game = tl!("In the game");
    let skin = tl!("Skin");
    let game_width = widgets::chip_width(ui, game);
    let skin_width = widgets::chip_width(ui, skin);
    widgets::paint_chip(
        ui,
        Rect::from_min_size(
            Pos2::new(divider_x - 12.0 - game_width, y),
            Vec2::new(game_width, 24.0),
        ),
        ChipKind::Neutral,
        game,
    );
    widgets::paint_chip(
        ui,
        Rect::from_min_size(Pos2::new(divider_x + 12.0, y), Vec2::new(skin_width, 24.0)),
        ChipKind::Success,
        skin,
    );
}

/// The before/after divider with its round handle; dragging it moves the split. Returns the split.
fn divider(ui: &mut Ui, room: Rect, split: f32) -> f32 {
    let tokens = Tokens::get(ui.ctx());
    let x = room.min.x + room.width() * split;
    let grip = Rect::from_center_size(
        Pos2::new(x, room.center().y),
        Vec2::new(28.0, room.height()),
    );
    let response = ui.interact(grip, ui.make_persistent_id("stage-divider"), Sense::drag());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Splitter, true, tl!("Before and after divider"))
    });
    let mut split = split;
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        split = ((pointer.x - room.min.x) / room.width().max(1.0)).clamp(0.05, 0.95);
    }
    let ink = if response.hovered() || response.dragged() {
        tokens.primary
    } else {
        tokens.text_2
    };
    ui.painter()
        .vline(x, room.min.y..=room.max.y, egui::Stroke::new(1.5, ink));
    let centre = Pos2::new(x, room.center().y);
    ui.painter()
        .circle(centre, 14.0, tokens.surface_2, egui::Stroke::new(1.5, ink));
    fonts::icon(ui.painter(), centre, icons::ARROW_LEFT_RIGHT, 13.0, ink);
    response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    split
}

/// "Drag to rotate · Scroll to zoom" in a quiet pill.
/// Over the model before while the next one loads: a spinner and what is happening, on a pill
/// lifted off the stage by a soft shadow.
fn loading_pill(ui: &Ui, center: Pos2) {
    let tokens = Tokens::get(ui.ctx());
    let painter = ui.painter();
    let text = tl!("Loading 3D model…");
    let style = Text::body(12.0, 500, tokens.text_2);
    let width = fonts::width(painter, text, style) + 16.0 + 16.0 + 8.0 + 16.0;
    let pill = Rect::from_center_size(center, Vec2::new(width, 32.0));
    draw::shadow(painter, pill, 16.0, 6, 24, Color32::from_black_alpha(70));
    draw::rounded(
        painter,
        pill,
        CornerRadius::same(16),
        tokens.surface_2.gamma_multiply(0.94),
        Some((1.0, tokens.border)),
    );
    let seconds = ui.input(|i| i.time) as f32;
    fonts::icon_rotated(
        painter,
        Pos2::new(pill.min.x + 16.0 + 8.0, pill.center().y),
        icons::LOADER,
        14.0,
        tokens.primary,
        fonts::spinner_angle(seconds),
    );
    fonts::draw_left(
        painter,
        pill.min.x + 16.0 + 16.0 + 8.0,
        pill.center().y,
        text,
        style,
    );
}

fn hint(ui: &Ui, note: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let text = tl!("Drag to rotate · Scroll to zoom");
    let style = Text::body(12.0, 400, tokens.text_3);
    let width = fonts::width(ui.painter(), text, style) + 30.0 + 12.0;
    let pill = Rect::from_center_size(note.center(), Vec2::new(width, 24.0));
    draw::rounded(
        ui.painter(),
        pill,
        CornerRadius::same(12),
        tokens.surface_2,
        Some((1.0, tokens.border)),
    );
    fonts::icon(
        ui.painter(),
        Pos2::new(pill.min.x + 18.0, pill.center().y),
        icons::ROTATE_3D,
        12.0,
        tokens.text_3,
    );
    fonts::draw_left(
        ui.painter(),
        pill.min.x + 30.0,
        pill.center().y,
        text,
        style,
    );
}
