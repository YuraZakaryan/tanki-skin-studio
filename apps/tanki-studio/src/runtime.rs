// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The `eframe::App` of the studio: per-frame housekeeping (language, theme, zoom, shortcuts,
//! dropped files, toasts, saving the session) and the window's remembered place.

use eframe::egui;

use crate::{app::Studio, dev, views};

impl eframe::App for Studio {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        tanki_i18n::set_current(self.lang);
        tanki_ui::scale::sync(ctx, self.ui_scale, &mut self.applied_scale);
        self.theme = self.theme_pref.resolve(ctx.system_theme());
        if self.applied_theme != Some(self.theme) {
            tanki_ui::theme::apply(ctx, self.theme);
            self.applied_theme = Some(self.theme);
        }
        self.track_window(ctx);
        if self.splash_done && !self.recognised_on_start {
            self.recognised_on_start = true;
            self.auto_recognise(true);
        }
        if self.finish_recognition() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.finish_import() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.finish_bulk() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.poll_updates(ctx);
        crate::dev::autopilot_tick(self, ctx);
        self.handle_shortcuts(ctx);
        let dropped = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect::<Vec<_>>()
        });
        if !dropped.is_empty() {
            self.handle_dropped(dropped);
        }
        self.expire_toasts(ctx);
        self.persist(false);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        views::draw(self, ui);
        dev::screenshot(&ctx);
    }

    fn on_exit(&mut self) {
        self.persist(true);
    }
}

impl Studio {
    /// Remember the window's place for the next start, and repair a stored one that does not fit
    /// the screen it opened on (the monitor may be gone). A minimized or full-screen window has
    /// no meaningful rectangle (Windows reports a minimized one at -32000), so it is left alone.
    fn track_window(&mut self, ctx: &egui::Context) {
        let info = ctx.input(|i| i.viewport().clone());
        let minimized = info.minimized.unwrap_or(false);
        let fullscreen = info.fullscreen.unwrap_or(false);
        if minimized || fullscreen {
            return;
        }
        if !self.window_checked
            && let (Some(monitor), Some(rect)) = (info.monitor_size, info.outer_rect)
        {
            self.window_checked = true;
            let current = tanki_core::session::WindowGeometry {
                x: rect.min.x,
                y: rect.min.y,
                width: rect.width(),
                height: rect.height(),
                maximized: false,
            };
            // A first start opens at the design's frame, which can be larger than a laptop screen:
            // size it to this screen instead, centred.
            let wanted = if self.first_start {
                tanki_core::session::WindowGeometry::first_start((monitor.x, monitor.y))
            } else {
                current.fit_to_screen((monitor.x, monitor.y))
            };
            if let Some(fitted) = wanted
                && fitted != current
            {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(
                    fitted.x, fitted.y,
                )));
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                    fitted.width,
                    fitted.height,
                )));
            }
        }
        let maximized = info.maximized.unwrap_or(false);
        // The place is the outer rectangle's corner, the size the inner one's: that is what the
        // window is created from, so a window does not grow by its title bar on every start.
        if let (Some(outer), Some(inner)) = (info.outer_rect, info.inner_rect) {
            let normal = self.window.filter(|_| maximized);
            self.window = Some(match normal {
                // Keep the last normal size while maximized, so un-maximizing restores it.
                Some(window) => tanki_core::session::WindowGeometry {
                    maximized: true,
                    ..window
                },
                None => tanki_core::session::WindowGeometry {
                    x: outer.min.x,
                    y: outer.min.y,
                    width: inner.width(),
                    height: inner.height(),
                    maximized,
                },
            });
        }
    }

    fn expire_toasts(&mut self, ctx: &egui::Context) {
        const LIFETIME: std::time::Duration = std::time::Duration::from_secs(5);
        self.toasts.retain(|t| t.born.elapsed() < LIFETIME);
        if !self.toasts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
}
