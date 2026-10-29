//! Resources: the searchable catalog of everything in the cache, and an inspector.

use eframe::egui::{self, CornerRadius, Pos2, Rect, Ui, Vec2};
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
    model::{Filter, ResourceRow, format_bytes, row_matches, target_name, thousands},
};

pub fn filter_label(filter: Filter) -> &'static str {
    match filter {
        Filter::All => tl!("All"),
        Filter::Hull => tl!("Hulls"),
        Filter::Turret => tl!("Turrets"),
        Filter::Paint => tl!("Paints"),
        Filter::Image => tl!("Images"),
        Filter::Model => tl!("3D models"),
        Filter::Sound => tl!("Sounds"),
        Filter::Archive => tl!("Archives"),
    }
}

fn kind_chip(row: &ResourceRow) -> (ChipKind, String) {
    if let Some((_, id, grade)) = &row.target {
        return (ChipKind::Success, target_name(id, grade));
    }
    let label = match row.kind.as_str() {
        "image" => tl!("Image"),
        "model" => tl!("3D model"),
        "sound" => tl!("Sound"),
        "archive" => tl!("Archive"),
        "metadata" => tl!("Metadata"),
        _ => tl!("File"),
    };
    (ChipKind::Neutral, label.to_owned())
}

pub fn draw(app: &mut Studio, ui: &mut Ui, main: Rect) {
    let subtitle = fmt(
        tl!("Game cache catalog · {n} groups"),
        &[("n", &thousands(app.rows.len()))],
    );
    widgets::top_bar(
        ui,
        shell::top_bar_rect(main),
        tl!("Resources"),
        &subtitle,
        |ui| {
            if Button::new(ButtonKind::Secondary, tl!("Rescan cache"))
                .icon(icons::REFRESH_CW)
                .show(ui)
                .clicked()
            {
                app.rescan();
            }
        },
    );
    let body = Rect::from_min_max(
        Pos2::new(main.min.x, main.min.y + shell::TOP_BAR_HEIGHT),
        main.max,
    );
    let wide = super::is_wide(body.shrink2(Vec2::new(32.0, 0.0)));
    let toolbar_h = toolbar_height(ui, body.width() - 64.0);
    let min_height = if wide {
        640.0 + toolbar_h - 40.0
    } else {
        32.0 + toolbar_h + 20.0 + 480.0 + 24.0 + 640.0 + 32.0
    };
    scroll_page(ui, body, min_height, |ui, rect| {
        let content = rect.shrink(32.0);
        let parts = rows(content, 20.0, &[Size::Fixed(toolbar_h), Size::Fill(1.0)]);
        toolbar(app, ui, parts[0]);
        if wide {
            let cols = columns(parts[1], 24.0, &[Size::Fill(1.0), Size::Fixed(420.0)]);
            catalog(app, ui, cols[0]);
            inspector(app, ui, cols[1]);
        } else {
            let stack = rows(parts[1], 24.0, &[Size::Fixed(480.0), Size::Fixed(640.0)]);
            catalog(app, ui, stack[0]);
            inspector(app, ui, stack[1]);
        }
    });
}

/// The narrowest search field next to the filter tabs; with less room the tabs take a row of
/// their own.
const MIN_SEARCH: f32 = 220.0;

/// The width of the row of filter tabs.
fn tabs_width(ui: &Ui) -> f32 {
    Filter::ALL
        .iter()
        .map(|f| widgets::tab_width(ui, filter_label(*f)))
        .sum::<f32>()
        + 4.0 * (Filter::ALL.len() - 1) as f32
}

/// Whether the filter tabs need a row of their own under the search field (a narrow window, or
/// the longer Russian labels).
fn tabs_below(ui: &Ui, width: f32) -> bool {
    width - 16.0 - tabs_width(ui) < MIN_SEARCH
}

/// The filter tabs in rows no wider than `width`: one row when they fit, more in a narrow window
/// (with a large interface size or the longer Russian labels), so no tab is ever cut off.
fn tab_rows(ui: &Ui, width: f32) -> Vec<Vec<Filter>> {
    let mut rows: Vec<Vec<Filter>> = vec![Vec::new()];
    let mut used = 0.0;
    for filter in Filter::ALL {
        let tab = widgets::tab_width(ui, filter_label(filter));
        let row = rows.last_mut().expect("a row");
        let need = if row.is_empty() {
            tab
        } else {
            used + 4.0 + tab
        };
        if need > width && !row.is_empty() {
            rows.push(vec![filter]);
            used = tab;
        } else {
            row.push(filter);
            used = need;
        }
    }
    rows
}

/// The toolbar's height: one row, or the search field with the rows of tabs under it.
fn toolbar_height(ui: &Ui, width: f32) -> f32 {
    if tabs_below(ui, width) {
        let rows = tab_rows(ui, width).len() as f32;
        40.0 + 10.0 + rows * 34.0 + (rows - 1.0) * 6.0
    } else {
        40.0
    }
}

fn filter_tabs(app: &mut Studio, ui: &mut Ui, filters: &[Filter]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        for &filter in filters {
            if widgets::tab(ui, filter_label(filter), app.filter == filter).clicked() {
                app.filter = filter;
            }
        }
    });
}

fn toolbar(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    // The search field gives up width to the filter tabs before any tab is cut off; when even
    // then they do not fit, the tabs go under it, in as many rows as they need.
    let below = tabs_below(ui, rect.width());
    let (search, tabs_row) = if below {
        let search = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 40.0));
        let tabs = Rect::from_min_max(Pos2::new(rect.min.x, rect.min.y + 50.0), rect.max);
        (search, tabs)
    } else {
        let search_width = (rect.width() * 0.4)
            .clamp(MIN_SEARCH, 340.0)
            .min(rect.width() - 16.0 - tabs_width(ui));
        let cells = columns(rect, 16.0, &[Size::Fixed(search_width), Size::Fill(1.0)]);
        let tabs = Rect::from_min_size(
            Pos2::new(cells[1].min.x, cells[1].min.y + 3.0),
            Vec2::new(cells[1].width(), 34.0),
        );
        (cells[0], tabs)
    };
    let hint = tl!("Search items and resources").to_string();
    widgets::text_field(
        ui,
        search,
        icons::SEARCH,
        &hint,
        &mut app.search,
        crate::commands::COMMANDS
            .iter()
            .find(|spec| spec.command == crate::commands::Command::OpenPalette)
            .and_then(|spec| spec.shortcut.as_ref())
            .map(crate::commands::shortcut_label)
            .as_deref(),
    );
    if below {
        for (index, filters) in tab_rows(ui, rect.width()).iter().enumerate() {
            let row = Rect::from_min_size(
                Pos2::new(tabs_row.min.x, tabs_row.min.y + index as f32 * 40.0),
                Vec2::new(tabs_row.width(), 34.0),
            );
            in_rect(ui, row, |ui| filter_tabs(app, ui, filters));
        }
    } else {
        in_rect(ui, tabs_row, |ui| filter_tabs(app, ui, &Filter::ALL));
    }
}

fn catalog(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let shown: Vec<usize> = app
        .rows
        .iter()
        .enumerate()
        .filter(|(_, r)| r.filter_matches(app.filter) && row_matches(r, &app.search))
        .map(|(i, _)| i)
        .collect();
    // The inspector always has something to show: the first entry until the user picks another.
    let selected_exists = app
        .selected_resource
        .as_ref()
        .is_some_and(|key| shown.iter().any(|i| &app.rows[*i].key == key));
    if !selected_exists {
        app.selected_resource = shown.first().map(|i| app.rows[*i].key.clone());
    }
    let files = app.scan.as_ref().map_or(0, |s| s.resources.len());
    let meta = fmt(
        tl!("{files} files · {groups} groups"),
        &[
            ("files", &thousands(files)),
            ("groups", &thousands(app.rows.len())),
        ],
    );
    let body = widgets::panel(ui, rect, tl!("Resource catalog"), Some(&meta));
    if shown.is_empty() {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width(), 18.0)),
            tl!("No resources match."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    }
    let list = body.shrink(8.0);
    let mut clicked = None;
    let mut opened = None;
    in_rect(ui, list, |ui| {
        // Virtualised: a cache holds thousands of groups, only the visible rows are laid out.
        egui::ScrollArea::vertical()
            .id_salt("resources-2")
            .auto_shrink([false, false])
            .show_rows(ui, 56.0, shown.len(), |ui, range| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for slot in range {
                    let row = &app.rows[shown[slot]];
                    let title = row.title();
                    let meta = fmt(
                        tl!("{files} · revision {r}"),
                        &[
                            ("files", &tn(row.files as u64, "{n} file", "{n} files")),
                            ("r", &row.revision),
                        ],
                    );
                    let (kind, chip) = kind_chip(row);
                    let selected = app.selected_resource.as_deref() == Some(row.key.as_str());
                    let response = ListRow::new(
                        if row.target.is_some() {
                            icons::PACKAGE
                        } else {
                            icons::FILE
                        },
                        &title,
                        &meta,
                    )
                    .selected(selected)
                    .chip(kind, &chip)
                    .size(&format_bytes(row.bytes))
                    .show(ui);
                    if response.clicked() {
                        clicked = Some(row.key.clone());
                    }
                    if response.double_clicked() {
                        opened = Some(row.key.clone());
                    }
                }
            });
    });
    if let Some(key) = clicked {
        let _ = app.store.record_recent_asset(&key);
        app.selected_resource = Some(key);
    }
    if let Some(key) = opened {
        app.open_row(&key);
    }
}

fn inspector(app: &mut Studio, ui: &mut Ui, rect: Rect) {
    let tokens = Tokens::get(ui.ctx());
    let selected = app
        .selected_resource
        .as_ref()
        .and_then(|key| app.rows.iter().find(|r| &r.key == key))
        .cloned();
    let meta = selected.as_ref().map(|r| format!("#{}", r.id));
    let body = widgets::panel(ui, rect, tl!("Resource"), meta.as_deref());
    let Some(row) = selected else {
        fonts::draw_centered(
            ui.painter(),
            Rect::from_center_size(body.center(), Vec2::new(body.width() - 36.0, 18.0)),
            tl!("Select a resource to inspect it."),
            Text::body(13.0, 400, tokens.text_3),
        );
        return;
    };
    let inner = body.shrink(18.0);
    let parts = rows(
        inner,
        16.0,
        &[
            Size::Fixed(190.0),
            Size::Fixed(58.0),
            Size::Fixed(120.0),
            Size::Fill(1.0),
            Size::Fixed(44.0),
            Size::Fixed(44.0),
        ],
    );
    // Preview.
    let stage = parts[0];
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
    let ctx = ui.ctx().clone();
    let texture = row
        .preview_group
        .clone()
        .and_then(|g| app.cache_preview(&ctx, &g));
    match texture {
        Some(texture) => {
            let area = stage.shrink2(Vec2::new(12.0, 12.0));
            let size = texture.size_vec2();
            let scale = (area.width() / size.x)
                .min((area.height() - 20.0) / size.y)
                .min(2.0);
            let image = Rect::from_center_size(
                Pos2::new(area.center().x, area.center().y - 8.0),
                size * scale,
            );
            ui.painter().image(
                texture.id(),
                image,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            fonts::draw_centered(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(stage.center().x, stage.max.y - 14.0),
                    Vec2::new(stage.width(), 16.0),
                ),
                &format!("{}×{}", texture.size()[0], texture.size()[1]),
                Text::mono(12.0, 400, tokens.text_3),
            );
        }
        None => {
            fonts::icon(
                ui.painter(),
                Pos2::new(stage.center().x, stage.center().y - 12.0),
                icons::IMAGE,
                48.0,
                tokens.text_3,
            );
            fonts::draw_centered(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(stage.center().x, stage.center().y + 32.0),
                    Vec2::new(stage.width(), 16.0),
                ),
                tl!("No texture to preview"),
                Text::mono(12.0, 400, tokens.text_3),
            );
        }
    }
    // Heading and chips.
    let heading = parts[1];
    fonts::draw(
        ui.painter(),
        heading.min,
        &row.title(),
        Text::display(20.0, 700, tokens.text),
    );
    let (kind, chip_label) = kind_chip(&row);
    let mut x = heading.min.x;
    let y = heading.min.y + 34.0;
    if let Some((target_kind, _, _)) = &row.target {
        let label = tl!("Recognised");
        let width = widgets::chip_width(ui, label);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
            ChipKind::Success,
            label,
        );
        x += width + 8.0;
        let kind_text = super::garage::kind_label(target_kind);
        let width = widgets::chip_width(ui, kind_text);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
            ChipKind::Info,
            kind_text,
        );
    } else {
        let width = widgets::chip_width(ui, &chip_label);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 24.0)),
            kind,
            &chip_label,
        );
        let label = tl!("Not recognised");
        let second = widgets::chip_width(ui, label);
        widgets::paint_chip(
            ui,
            Rect::from_min_size(Pos2::new(x + width + 8.0, y), Vec2::new(second, 24.0)),
            ChipKind::Warning,
            label,
        );
    }
    // Facts.
    let details = parts[2];
    let facts = [
        (tl!("Group"), row.key.clone(), true),
        (tl!("Files"), row.files.to_string(), true),
        (tl!("Revision"), row.revision.clone(), true),
        (tl!("Size"), format_bytes(row.bytes), true),
        (tl!("Role"), row.roles.join(", "), true),
    ];
    for (index, (key, value, mono)) in facts.iter().enumerate() {
        let line = Rect::from_min_size(
            Pos2::new(details.min.x, details.min.y + index as f32 * 26.0),
            Vec2::new(details.width(), 16.0),
        );
        widgets::key_value(ui, line, key, value, *mono);
    }
    // Actions.
    let open = parts[4];
    in_rect(ui, open, |ui| {
        let label = if row.target.is_some() {
            tl!("Open in Garage")
        } else {
            tl!("Label manually")
        };
        let icon = if row.target.is_some() {
            icons::WAREHOUSE
        } else {
            icons::TAG
        };
        if Button::new(ButtonKind::Primary, label)
            .icon(icon)
            .width(Width::Fill)
            .height(44.0)
            .show(ui)
            .clicked()
        {
            app.open_row(&row.key);
        }
    });
    in_rect(ui, parts[5], |ui| {
        if Button::new(ButtonKind::Secondary, tl!("Label manually"))
            .icon(icons::TAG)
            .width(Width::Fill)
            .height(44.0)
            .show(ui)
            .clicked()
        {
            app.open_label(&row.key);
        }
    });
}
