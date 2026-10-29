// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The ⌘K command palette: items, resources and every command of the registry, with a
//! substring match that falls back to a subsequence match (PdfCraft's ranking), searched in both
//! the translated and the English text.

use eframe::egui::{
    self, Area, Context, CornerRadius, Id, Key, Order, Pos2, Rect, Sense, TextEdit, Vec2,
};
use tanki_i18n::{tl, tn};
use tanki_ui::{draw, fonts, fonts::Text, icons, theme::Tokens, widgets};

use crate::{
    app::Studio,
    commands::{COMMANDS, Command, shortcut_label},
    model::{format_bytes, grade_code, target_name},
};

#[derive(Default)]
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
    last_query: String,
    focused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Items,
    Resources,
    Actions,
}

#[derive(Clone)]
enum Action {
    Item {
        kind: String,
        id: String,
        grade: usize,
    },
    Row(String),
    Run(Command),
}

#[derive(Clone)]
struct Hit {
    group: Group,
    icon: char,
    title: String,
    meta: String,
    keys: Option<String>,
    enabled: bool,
    action: Action,
    score: usize,
}

/// Where `needle` matches `hay` (lower-cased), for ranking; lower is better.
pub fn score(hay: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let hay = hay.to_lowercase();
    if let Some(position) = hay.find(needle) {
        return Some(position);
    }
    let mut chars = hay.chars();
    needle.chars().all(|c| chars.any(|x| x == c)).then_some(100)
}

fn collect(app: &Studio, query: &str) -> Vec<Hit> {
    let needle = query.trim().to_lowercase();
    let mut hits = Vec::new();
    for item in &app.items {
        for grade in 0..4 {
            let title = target_name(&item.id, grade_code(grade));
            let Some(s) = score(&title, &needle) else {
                continue;
            };
            let kind = super::garage::kind_label(&item.kind);
            let meta = if item.present[grade] {
                format!(
                    "{kind} · {}",
                    tn(
                        item.present.iter().filter(|p| **p).count() as u64,
                        "{n} modification",
                        "{n} modifications"
                    )
                )
            } else {
                format!("{kind} · {}", tl!("not in cache"))
            };
            hits.push(Hit {
                group: Group::Items,
                icon: icons::TRUCK,
                title,
                meta,
                keys: None,
                enabled: true,
                action: Action::Item {
                    kind: item.kind.clone(),
                    id: item.id.clone(),
                    grade,
                },
                score: s + usize::from(!item.present[grade]) * 1000,
            });
        }
    }
    if !needle.is_empty() {
        for row in &app.rows {
            let title = row.title();
            let Some(s) = score(&title, &needle) else {
                continue;
            };
            hits.push(Hit {
                group: Group::Resources,
                icon: icons::IMAGE,
                meta: format!(
                    "{} · {}",
                    tn(row.files as u64, "{n} file", "{n} files"),
                    format_bytes(row.bytes)
                ),
                title,
                keys: None,
                enabled: true,
                action: Action::Row(row.key.clone()),
                score: s,
            });
        }
    }
    for spec in COMMANDS {
        let translated = tanki_i18n::t(spec.label);
        let Some(s) = score(translated, &needle).or_else(|| score(spec.label, &needle)) else {
            continue;
        };
        hits.push(Hit {
            group: Group::Actions,
            icon: spec.icon,
            title: translated.to_owned(),
            meta: tanki_i18n::t(spec.detail).to_owned(),
            keys: spec.shortcut.as_ref().map(shortcut_label),
            enabled: app.command_enabled(spec.command),
            action: Action::Run(spec.command),
            score: s,
        });
    }
    // Per group: best first, enabled before disabled; a few of each so the list stays short.
    let mut out = Vec::new();
    for (group, limit) in [
        (Group::Items, 4),
        (Group::Resources, 4),
        (Group::Actions, 6),
    ] {
        let mut part: Vec<Hit> = hits.iter().filter(|h| h.group == group).cloned().collect();
        part.sort_by_key(|h| (!h.enabled, h.score));
        part.truncate(limit);
        out.extend(part);
    }
    out
}

pub fn draw(app: &mut Studio, ctx: &Context) {
    if app.palette.is_none() {
        return;
    }
    let tokens = Tokens::get(ctx);
    let query = app
        .palette
        .as_ref()
        .map(|p| p.query.clone())
        .unwrap_or_default();
    let hits = collect(app, &query);
    let screen = ctx.content_rect();
    let (down, up, enter, escape) = ctx.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::Enter),
            i.consume_key(egui::Modifiers::NONE, Key::Escape),
        )
    });
    let Some(state) = app.palette.as_mut() else {
        return;
    };
    // A new query starts at the top, as in PdfCraft.
    if state.query != state.last_query {
        state.selected = 0;
        state.last_query = state.query.clone();
    }
    let count = hits.len();
    if count > 0 {
        if down {
            state.selected = (state.selected + 1) % count;
        }
        if up {
            state.selected = (state.selected + count - 1) % count;
        }
        state.selected = state.selected.min(count - 1);
    }
    let selected = state.selected;
    let mut chosen: Option<usize> = enter.then_some(selected).filter(|_| count > 0);
    let mut close = escape;

    let groups = [Group::Items, Group::Resources, Group::Actions]
        .iter()
        .filter(|g| hits.iter().any(|h| h.group == **g))
        .count();
    let results_height = 20.0
        + groups as f32 * 14.0
        + hits.len() as f32 * 44.0
        + (hits.len() + groups).saturating_sub(1) as f32 * 2.0;
    let height = 60.0 + results_height.max(60.0) + 40.0;
    Area::new(Id::unique("palette"))
        .order(Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let scrim = ui.allocate_rect(screen, Sense::click());
            ui.painter().rect_filled(screen, 0.0, tokens.scrim);
            let rect = Rect::from_min_size(
                Pos2::new(screen.center().x - 320.0, screen.min.y + 130.0),
                Vec2::new(640.0, height),
            );
            if scrim.clicked() && !rect.contains(scrim.interact_pointer_pos().unwrap_or_default()) {
                close = true;
            }
            draw::shadow(
                ui.painter(),
                rect,
                16.0,
                24,
                56,
                egui::Color32::from_black_alpha(0xA0),
            );
            draw::rounded(
                ui.painter(),
                rect,
                CornerRadius::same(16),
                tokens.surface_2,
                Some((1.0, tokens.border_strong)),
            );
            // Search row.
            fonts::icon(
                ui.painter(),
                Pos2::new(rect.min.x + 20.0 + 9.0, rect.min.y + 30.0),
                icons::SEARCH,
                18.0,
                tokens.primary,
            );
            let esc = widgets::paint_kbd(
                ui,
                rect.max.x - 20.0,
                rect.min.y + 30.0,
                "esc",
                tokens.text_2,
            );
            let edit = Rect::from_min_max(
                Pos2::new(rect.min.x + 50.0, rect.min.y + 10.0),
                Pos2::new(rect.max.x - 20.0 - esc - 12.0, rect.min.y + 50.0),
            );
            let hint = tl!("Search items, resources and actions");
            let focus_needed = !state.focused;
            let mut text = std::mem::take(&mut state.query);
            tanki_ui::layout::in_rect(ui, edit, |ui| {
                let response = ui.add_sized(
                    edit.size(),
                    TextEdit::singleline(&mut text)
                        .frame(egui::Frame::NONE)
                        .hint_text(egui::RichText::new(hint).color(tokens.text_3).size(17.0))
                        .font(egui::FontId::new(
                            17.0,
                            egui::FontFamily::Name("inter".into()),
                        ))
                        .text_color(tokens.text)
                        .align(egui::Align2::LEFT_CENTER),
                );
                if focus_needed {
                    response.request_focus();
                }
            });
            state.query = text;
            state.focused = true;
            draw::hrule(
                ui.painter(),
                rect.min.x + 1.0,
                rect.max.x - 1.0,
                rect.min.y + 60.0,
                tokens.border,
            );
            // Results.
            let mut y = rect.min.y + 60.0 + 10.0;
            let mut last_group = None;
            if hits.is_empty() {
                fonts::draw_centered(
                    ui.painter(),
                    Rect::from_min_size(
                        Pos2::new(rect.min.x, y + 10.0),
                        Vec2::new(rect.width(), 20.0),
                    ),
                    tl!("Nothing found."),
                    Text::body(13.0, 400, tokens.text_3),
                );
            }
            for (index, hit) in hits.iter().enumerate() {
                if last_group != Some(hit.group) {
                    last_group = Some(hit.group);
                    let title = match hit.group {
                        Group::Items => tl!("Items"),
                        Group::Resources => tl!("Resources"),
                        Group::Actions => tl!("Actions"),
                    };
                    widgets::caption(ui, Pos2::new(rect.min.x + 10.0, y), title);
                    y += 14.0 + 2.0;
                }
                let row = Rect::from_min_size(
                    Pos2::new(rect.min.x + 10.0, y),
                    Vec2::new(rect.width() - 20.0, 44.0),
                );
                let response = ui.interact(
                    row,
                    ui.scope_id().with(("palette-row", index)),
                    Sense::click(),
                );
                let is_selected = index == selected;
                if is_selected {
                    draw::rounded(
                        ui.painter(),
                        row,
                        CornerRadius::same(8),
                        tokens.primary_soft,
                        None,
                    );
                } else if response.hovered() {
                    draw::rounded(
                        ui.painter(),
                        row,
                        CornerRadius::same(8),
                        tokens.surface_3,
                        None,
                    );
                }
                let ink = if hit.enabled {
                    tokens.text
                } else {
                    tokens.text_3
                };
                let icon_ink = if is_selected {
                    tokens.primary
                } else {
                    tokens.text_2
                };
                fonts::icon(
                    ui.painter(),
                    Pos2::new(row.min.x + 14.0 + 8.0, row.center().y),
                    hit.icon,
                    16.0,
                    icon_ink,
                );
                let title_style = Text::body(14.0, if is_selected { 600 } else { 500 }, ink);
                let used = fonts::draw_left(
                    ui.painter(),
                    row.min.x + 42.0,
                    row.center().y,
                    &hit.title,
                    title_style,
                );
                let mut right = row.max.x - 14.0;
                if let Some(keys) = &hit.keys {
                    right -=
                        widgets::paint_kbd(ui, right, row.center().y, keys, tokens.text_2) + 12.0;
                } else if is_selected && hit.enabled {
                    right -=
                        widgets::paint_kbd(ui, right, row.center().y, "↵", tokens.text_2) + 12.0;
                }
                let clip = Rect::from_min_max(
                    Pos2::new(used.max.x + 12.0, row.min.y),
                    Pos2::new(right, row.max.y),
                );
                fonts::draw_left(
                    &ui.painter().with_clip_rect(clip),
                    clip.min.x,
                    row.center().y,
                    &hit.meta,
                    Text::body(12.0, 400, tokens.text_3),
                );
                if response.clicked() && hit.enabled {
                    chosen = Some(index);
                }
                y += 44.0 + 2.0;
            }
            // Footer.
            let footer = Rect::from_min_max(Pos2::new(rect.min.x, rect.max.y - 40.0), rect.max);
            ui.painter().rect_filled(
                footer,
                CornerRadius {
                    nw: 0,
                    ne: 0,
                    sw: 16,
                    se: 16,
                },
                tokens.surface,
            );
            draw::hrule(
                ui.painter(),
                rect.min.x + 1.0,
                rect.max.x - 1.0,
                footer.min.y,
                tokens.border,
            );
            let mut x = footer.min.x + 18.0;
            for label in [tl!("↑↓ navigate"), tl!("↵ open"), tl!("esc close")] {
                let used = fonts::draw_left(
                    ui.painter(),
                    x,
                    footer.center().y + 0.5,
                    label,
                    Text::body(12.0, 400, tokens.text_3),
                );
                x = used.max.x + 18.0;
            }
        });
    if close {
        app.palette = None;
        return;
    }
    if let Some(index) = chosen {
        let hit = &hits[index];
        if !hit.enabled {
            return;
        }
        let action = hit.action.clone();
        app.palette = None;
        match action {
            Action::Item { kind, id, grade } => app.open_in_garage(&kind, &id, grade),
            Action::Row(key) => app.open_row(&key),
            Action::Run(command) => app.run(command),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn substring_beats_subsequence_and_earlier_beats_later() {
        assert_eq!(score("Wasp M3", ""), Some(0));
        assert_eq!(score("Wasp M3", "wasp"), Some(0));
        assert_eq!(score("Railgun Wasp", "wasp"), Some(8));
        assert_eq!(score("Restore last replacement", "rlr"), Some(100));
        assert_eq!(score("Wasp M3", "zzz"), None);
    }
}
