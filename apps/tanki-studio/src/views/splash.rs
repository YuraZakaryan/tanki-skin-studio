//! The start-up screen: brand, progress and the three start-up steps, shown for about three
//! seconds (a click, Space or Enter skips it).

use std::time::Duration;

use eframe::egui::{self, Color32, Key, Pos2, Rect, Ui, Vec2};
use tanki_i18n::tl;
use tanki_ui::{draw, fonts, fonts::Text, icons, theme::Tokens, widgets};

use crate::app::Studio;

/// Time the bar takes to fill, and how long the finished state (every step ticked) stays on
/// screen before the app opens: about three seconds in all.
const LOAD: f32 = 2.4;
const HOLD: f32 = 0.6;

pub fn draw(app: &mut Studio, ui: &mut Ui, full: Rect) {
    let tokens = Tokens::get(ui.ctx());
    // `TANKI_SPLASH=hold` freezes the bar at 66%, `done` at the finished state (for screenshots).
    let elapsed = match app.splash_freeze {
        Some(frozen) => frozen,
        None => app.started.elapsed().as_secs_f32(),
    };
    let progress = (elapsed / LOAD).clamp(0.0, 1.0);
    let finished = elapsed >= LOAD;
    let skip = ui
        .ctx()
        .input(|i| i.key_pressed(Key::Space) || i.key_pressed(Key::Enter) || i.pointer.any_click());
    if (elapsed >= LOAD + HOLD || skip) && app.splash_freeze.is_none() {
        app.splash_done = true;
        ui.ctx().request_repaint();
        return;
    }
    ui.ctx().request_repaint_after(Duration::from_millis(16));
    draw::radial(
        ui.painter(),
        full,
        0.0,
        tokens.surface_3,
        tokens.bg,
        tokens.bg,
    );
    let stack = 86.0 + 36.0 + 30.0 + 36.0 + 68.0 + 36.0 + 15.0;
    let top = full.center().y - stack / 2.0;
    let cx = full.center().x;
    fonts::draw_centered(
        ui.painter(),
        Rect::from_min_size(Pos2::new(cx - 200.0, top), Vec2::new(400.0, 62.0)),
        "TANKI",
        Text::display(48.0, 700, tokens.text).spaced(10.0),
    );
    fonts::draw_centered(
        ui.painter(),
        Rect::from_min_size(Pos2::new(cx - 200.0, top + 68.0), Vec2::new(400.0, 18.0)),
        tl!("Skin Studio"),
        Text::body(15.0, 400, tokens.text_2),
    );
    let steps = [
        tl!("Searching for the game cache"),
        tl!("Reading pack library…"),
        tl!("Done"),
    ];
    // Which step is running: the last one ("Done") is only reached by finishing.
    let current = if finished {
        3
    } else {
        ((progress * 3.0) as usize).min(2)
    };
    let bar_top = top + 86.0 + 36.0;
    let bar = Rect::from_min_size(Pos2::new(cx - 170.0, bar_top), Vec2::new(340.0, 30.0));
    fonts::draw(
        ui.painter(),
        bar.min,
        steps[current.min(2)],
        Text::body(13.0, 400, tokens.text_2),
    );
    fonts::draw_right(
        ui.painter(),
        bar.max.x,
        bar.min.y + 8.0,
        &format!("{}%", (progress * 100.0) as u32),
        Text::mono(12.0, 400, tokens.primary),
    );
    widgets::progress_bar(
        ui,
        Rect::from_min_size(
            Pos2::new(bar.min.x, bar.min.y + 24.0),
            Vec2::new(340.0, 6.0),
        ),
        progress,
    );
    let steps_top = bar_top + 30.0 + 36.0;
    for (index, label) in steps.iter().enumerate() {
        let cy = steps_top + index as f32 * 26.0 + 8.0;
        let (glyph, icon_color, text_color) = if index < current {
            (
                icons::CIRCLE_CHECK,
                tokens.primary,
                if index == 2 {
                    tokens.text
                } else {
                    tokens.text_2
                },
            )
        } else if index == current {
            (icons::LOADER, tokens.accent, tokens.text)
        } else {
            (icons::CIRCLE, tokens.text_3, tokens.text_3)
        };
        let center = Pos2::new(bar.min.x + 8.0, cy);
        if glyph == icons::LOADER {
            // The running step's loader spins.
            fonts::icon_rotated(
                ui.painter(),
                center,
                glyph,
                16.0,
                icon_color,
                fonts::spinner_angle(elapsed),
            );
        } else {
            fonts::icon(ui.painter(), center, glyph, 16.0, icon_color);
        }
        fonts::draw_left(
            ui.painter(),
            bar.min.x + 26.0,
            cy,
            label,
            Text::body(13.0, 400, text_color),
        );
    }
    let legal = format!(
        "v{} · MIT · {}",
        env!("CARGO_PKG_VERSION"),
        tl!("Game assets are not distributed")
    );
    fonts::draw_centered(
        ui.painter(),
        Rect::from_min_size(
            Pos2::new(cx - 250.0, steps_top + 68.0 + 36.0),
            Vec2::new(500.0, 15.0),
        ),
        &legal,
        Text::body(12.0, 400, tokens.text_3),
    );
    let _ = (Color32::WHITE, egui::Sense::hover());
}
