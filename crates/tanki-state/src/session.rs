// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! What the user was doing when the app closed: the page, window, and per-cache selection.
//!
//! Stored as one JSON document under a versioned settings key so that
//! - a newer app version writes its own key and never corrupts what an older one reads;
//! - every field has a default, so documents written by older versions still load;
//! - nothing in it is trusted: [`SessionState::sanitize`] and [`WindowGeometry::fit_to_screen`]
//!   repair values that no longer make sense (a deleted pack, a monitor that was unplugged);
//! - selections are scoped to the cache folder they were made in, so two caches never overwrite
//!   each other's state.
//!
//! The interface language and theme are *not* here: they have their own settings keys, so a
//! damaged session can never lose them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Settings key of the current document format. A format change gets a new key.
pub const SESSION_KEY: &str = "ui.session.v1";
/// Most caches whose selection is remembered (least recently used are dropped).
pub const MAX_CACHES: usize = 16;
/// Smallest window the layout supports.
pub const MIN_WINDOW: (f32, f32) = (820.0, 520.0);
/// The window of a first start on a screen large enough for it: the design's frame.
pub const DEFAULT_WINDOW: (f32, f32) = (1440.0, 900.0);

/// Outer window rectangle in logical points.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl WindowGeometry {
    /// Repair a stored rectangle for a screen of `screen` points: finite numbers only, no smaller
    /// than the supported minimum and no larger than the screen, and at least a grabbable part of
    /// the title area on screen (the monitor it was on may be gone). `None` if it is garbage.
    /// The window of a first start on a screen of `screen` points: the design's frame when it
    /// fits, else 90% of the width and 85% of the height (room for the title bar and a taskbar),
    /// never below the supported minimum unless the screen is smaller; centred, a little above the
    /// middle so a bottom taskbar does not cover it. `None` for a screen that makes no sense.
    pub fn first_start(screen: (f32, f32)) -> Option<Self> {
        if !(screen.0.is_finite() && screen.1.is_finite()) || screen.0 < 1.0 || screen.1 < 1.0 {
            return None;
        }
        let width = DEFAULT_WINDOW
            .0
            .min(screen.0 * 0.9)
            .max(MIN_WINDOW.0.min(screen.0));
        let height = DEFAULT_WINDOW
            .1
            .min(screen.1 * 0.85)
            .max(MIN_WINDOW.1.min(screen.1));
        Some(Self {
            x: ((screen.0 - width) / 2.0).max(0.0),
            y: ((screen.1 - height) / 2.0 - 24.0).max(0.0),
            width,
            height,
            maximized: false,
        })
    }

    pub fn fit_to_screen(self, screen: (f32, f32)) -> Option<Self> {
        let finite = [self.x, self.y, self.width, self.height]
            .iter()
            .all(|v| v.is_finite());
        if !finite || screen.0 < 1.0 || screen.1 < 1.0 {
            return None;
        }
        let width = self.width.clamp(MIN_WINDOW.0.min(screen.0), screen.0);
        let height = self.height.clamp(MIN_WINDOW.1.min(screen.1), screen.1);
        let grab = 120.0_f32.min(width);
        let x = self.x.clamp(-(width - grab), screen.0 - grab);
        let y = self.y.clamp(0.0, (screen.1 - 60.0).max(0.0));
        Some(Self {
            x,
            y,
            width,
            height,
            maximized: self.maximized,
        })
    }
}

/// The item (hull, turret, paint) shown in the garage.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemRef {
    pub kind: String,
    pub id: String,
}

/// Selection state tied to one cache folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheSession {
    /// Canonical cache folder this state belongs to.
    pub root: String,
    /// Garage item and modification (`m0`..`m3`).
    pub item: Option<ItemRef>,
    pub grade: Option<String>,
    /// Chosen skin variant per pack and item, keyed `<pack id>/<kind>:<id>:<grade>`.
    pub variants: BTreeMap<String, String>,
    /// Resource group selected in the Resources page.
    pub resource: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// Page that was open (`overview`, `garage`, `resources`, `packs`, `info`).
    pub page: String,
    pub window: Option<WindowGeometry>,
    /// Garage category tab (`hull`, `turret`, `paint`).
    pub garage_tab: String,
    /// Resources category filter (`all`, `hull`, `image`, ...).
    pub resource_filter: String,
    /// Cache the user last worked with.
    pub last_cache: Option<String>,
    /// Per-cache selections, most recently used first.
    pub caches: Vec<CacheSession>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            page: "overview".into(),
            window: None,
            garage_tab: "hull".into(),
            resource_filter: "all".into(),
            last_cache: None,
            caches: Vec::new(),
        }
    }
}

impl SessionState {
    /// Replace values the running app cannot show with defaults. `pages`, `tabs` and `filters`
    /// are what this build knows.
    pub fn sanitize(&mut self, pages: &[&str], tabs: &[&str], filters: &[&str]) {
        let defaults = Self::default();
        if !pages.contains(&self.page.as_str()) {
            self.page = defaults.page;
        }
        if !tabs.contains(&self.garage_tab.as_str()) {
            self.garage_tab = defaults.garage_tab;
        }
        if !filters.contains(&self.resource_filter.as_str()) {
            self.resource_filter = defaults.resource_filter;
        }
        self.caches.retain(|c| !c.root.is_empty());
        let mut seen = std::collections::BTreeSet::new();
        self.caches.retain(|c| seen.insert(c.root.clone()));
        self.caches.truncate(MAX_CACHES);
    }

    /// The selection state of `root`, if any was saved.
    pub fn cache(&self, root: &str) -> Option<&CacheSession> {
        self.caches.iter().find(|c| c.root == root)
    }

    /// The selection state of `root`, created if needed and moved to the front (most recent).
    pub fn cache_mut(&mut self, root: &str) -> &mut CacheSession {
        match self.caches.iter().position(|c| c.root == root) {
            Some(0) => {}
            Some(index) => {
                let entry = self.caches.remove(index);
                self.caches.insert(0, entry);
            }
            None => {
                self.caches.insert(
                    0,
                    CacheSession {
                        root: root.to_owned(),
                        ..CacheSession::default()
                    },
                );
                self.caches.truncate(MAX_CACHES);
            }
        }
        &mut self.caches[0]
    }

    /// Parse a stored document. Anything unreadable gives the default session: losing the
    /// selection is harmless, refusing to start is not.
    pub fn from_json(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGES: &[&str] = &["overview", "garage", "resources", "packs", "info"];
    const TABS: &[&str] = &["hull", "turret", "paint"];
    const FILTERS: &[&str] = &["all", "hull", "image"];

    #[test]
    fn a_first_start_fits_the_screen_it_opens_on() {
        // A 1366x768 laptop: smaller than the design's frame, with a taskbar.
        let small = WindowGeometry::first_start((1366.0, 768.0)).unwrap();
        assert!(small.width <= 1366.0 * 0.9 && small.height <= 768.0 * 0.85);
        assert!(small.width >= MIN_WINDOW.0 && small.height >= MIN_WINDOW.1);
        assert!(
            small.x >= 0.0 && small.x + small.width <= 1366.0,
            "inside horizontally"
        );
        // The title bar (about 30 points) and a 48-point taskbar still leave it whole.
        assert!(small.y + 30.0 + small.height <= 768.0 - 48.0, "{small:?}");
        // A large screen gets the design's frame, centred.
        let large = WindowGeometry::first_start((2560.0, 1440.0)).unwrap();
        assert_eq!((large.width, large.height), DEFAULT_WINDOW);
        assert_eq!(large.x, (2560.0 - 1440.0) / 2.0);
        // A screen below the minimum: the whole screen, never more.
        let tiny = WindowGeometry::first_start((800.0, 500.0)).unwrap();
        assert!(tiny.width <= 800.0 && tiny.height <= 500.0);
        assert!(WindowGeometry::first_start((f32::NAN, 900.0)).is_none());
        assert!(WindowGeometry::first_start((0.0, 900.0)).is_none());
    }

    #[test]
    fn a_session_round_trips_through_json() {
        let mut session = SessionState {
            page: "garage".into(),
            ..SessionState::default()
        };
        let cache = session.cache_mut("/cache/a");
        cache.item = Some(ItemRef {
            kind: "hull".into(),
            id: "wasp".into(),
        });
        cache.grade = Some("m3".into());
        cache.variants.insert("hull:wasp:m3".into(), "red".into());
        assert_eq!(SessionState::from_json(&session.to_json()), session);
    }

    #[test]
    fn garbage_and_partial_documents_load_with_defaults() {
        assert_eq!(SessionState::from_json("not json"), SessionState::default());
        assert_eq!(SessionState::from_json(""), SessionState::default());
        // Older documents without newer fields, and newer ones with fields this build lacks.
        let partial = SessionState::from_json(r#"{"page":"packs","future_field":[1,2,3]}"#);
        assert_eq!(partial.page, "packs");
        assert_eq!(partial.garage_tab, "hull");
    }

    #[test]
    fn sanitize_replaces_unknown_values_and_prunes_caches() {
        let mut session = SessionState {
            page: "removed-page".into(),
            garage_tab: "?".into(),
            resource_filter: "?".into(),
            ..SessionState::default()
        };
        for i in 0..(MAX_CACHES + 5) {
            session.cache_mut(&format!("/cache/{i}"));
        }
        session.caches.push(CacheSession::default());
        session.caches.push(session.caches[0].clone());
        session.sanitize(PAGES, TABS, FILTERS);
        assert_eq!(session.page, "overview");
        assert_eq!(session.garage_tab, "hull");
        assert_eq!(session.resource_filter, "all");
        assert_eq!(session.caches.len(), MAX_CACHES);
        assert!(session.caches.iter().all(|c| !c.root.is_empty()));
        let mut roots: Vec<_> = session.caches.iter().map(|c| &c.root).collect();
        roots.dedup();
        assert_eq!(roots.len(), MAX_CACHES);
    }

    #[test]
    fn cache_state_is_scoped_and_most_recent_first() {
        let mut session = SessionState::default();
        session.cache_mut("/a").resource = Some("one".into());
        session.cache_mut("/b").resource = Some("two".into());
        session.cache_mut("/a");
        assert_eq!(session.caches[0].root, "/a");
        assert_eq!(
            session.cache("/a").unwrap().resource.as_deref(),
            Some("one")
        );
        assert_eq!(
            session.cache("/b").unwrap().resource.as_deref(),
            Some("two")
        );
        assert!(session.cache("/c").is_none());
    }

    #[test]
    fn window_geometry_is_repaired_for_the_current_screen() {
        let screen = (1920.0, 1080.0);
        let fine = WindowGeometry {
            x: 100.0,
            y: 80.0,
            width: 1280.0,
            height: 820.0,
            maximized: false,
        };
        assert_eq!(fine.fit_to_screen(screen), Some(fine));
        // A monitor that is gone: the window sat far to the right.
        let gone = WindowGeometry { x: 5000.0, ..fine };
        let fitted = gone.fit_to_screen(screen).unwrap();
        assert!(fitted.x <= screen.0 - 100.0);
        // Too small and too large sizes are clamped; garbage is dropped.
        let tiny = WindowGeometry {
            width: 100.0,
            height: 100.0,
            ..fine
        };
        let fitted = tiny.fit_to_screen(screen).unwrap();
        assert_eq!((fitted.width, fitted.height), MIN_WINDOW);
        let huge = WindowGeometry {
            width: 9000.0,
            height: 9000.0,
            ..fine
        };
        let fitted = huge.fit_to_screen(screen).unwrap();
        assert_eq!((fitted.width, fitted.height), screen);
        let nan = WindowGeometry {
            x: f32::NAN,
            ..fine
        };
        assert_eq!(nan.fit_to_screen(screen), None);
    }
}
