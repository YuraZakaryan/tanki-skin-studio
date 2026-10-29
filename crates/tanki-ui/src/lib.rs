// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Layer 0: reusable interface building blocks drawn to the app's design file (`design.pen`):
//! colour tokens for a dark and a light theme, bundled fonts and icons, and widgets that paint
//! themselves at the design's exact sizes.
//!
//! This crate knows nothing about caches, packs or skins, so any screen (or another app) can be
//! assembled from it.

pub mod draw;
pub mod fonts;
pub mod icons;
pub mod layout;
pub mod scale;
pub mod theme;
pub mod widgets;
