// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Widgets that paint themselves at the design's exact sizes. Each one allocates its rectangle
//! from the parent `Ui` (or takes one from [`crate::layout`]), draws with [`crate::draw`] and
//! [`crate::fonts`], and returns an `egui::Response` for interaction.

mod buttons;
mod cards;
mod chips;
mod forms;
mod modal;
mod nav;
mod panel;
mod rows;
mod select;
mod wizard;

pub use buttons::*;
pub use cards::*;
pub use chips::*;
pub use forms::*;
pub use modal::*;
pub use nav::*;
pub use panel::*;
pub use rows::*;
pub use select::*;
pub use wizard::*;

use egui::{Color32, Id, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo};

/// Allocate exactly `size` from the parent layout and make it interactive.
pub(crate) fn alloc(ui: &mut Ui, size: Vec2, sense: Sense) -> (Rect, Response) {
    ui.allocate_exact_size(size, sense)
}

/// Interact with a rectangle that was cut by [`crate::layout`], not allocated from the layout.
pub(crate) fn interact(
    ui: &mut Ui,
    rect: Rect,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    sense: Sense,
) -> Response {
    let id = ui.scope_id().with(id_salt);
    ui.interact(rect, id, sense)
}

/// The stable id of a widget, from the ui id and a salt.
pub fn salted(ui: &Ui, salt: impl std::hash::Hash + std::fmt::Debug) -> Id {
    ui.scope_id().with(salt)
}

/// A colour for a disabled widget.
pub(crate) fn dim(color: Color32, enabled: bool) -> Color32 {
    if enabled {
        color
    } else {
        crate::draw::fade(color, 0.45)
    }
}

/// Give a painted widget its accessible name, so screen readers (and tests, which look widgets up
/// by label) can find it. `selected` is `Some` for toggles, tabs and the current page.
pub(crate) fn describe(
    response: &Response,
    kind: Role,
    enabled: bool,
    selected: Option<bool>,
    label: &str,
) {
    let label = label.to_owned();
    response.widget_info(move || match selected {
        Some(selected) => WidgetInfo::selected(kind, enabled, selected, label.clone()),
        None => WidgetInfo::labeled(kind, enabled, label.clone()),
    });
}
