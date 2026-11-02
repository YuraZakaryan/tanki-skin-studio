// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Skins that go on later: a modification the game has not downloaded yet gets its skin once it
//! appears in the cache. Kept as one JSON document in the settings table.

use serde::{Deserialize, Serialize};

/// Settings key of the queue.
pub const LATER_KEY: &str = "later.v1";
/// Settings key of how often the cache is read again (`auto`, `hourly`, `manual`).
pub const RESCAN_KEY: &str = "rescan";

/// One modification waiting for its skin.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaterSkin {
    /// Canonical cache folder the skin is for.
    pub root: String,
    pub kind: String,
    pub id: String,
    /// The cache modification (0..=3) that gets the skin.
    pub modification: u8,
    pub pack_id: String,
    pub pack_version: String,
    pub pack_name: String,
    /// Index of the pack item and its variant in the pack's manifest, and the variant's id.
    pub item: usize,
    pub variant: usize,
    pub variant_id: String,
    /// The pack item's grade (`xt-m3`, `prime`).
    pub grade: String,
    /// The look's name as the user saw it ("XT").
    pub look: String,
    /// When it was queued (Unix seconds).
    pub added: u64,
}

impl LaterSkin {
    /// Whether `other` is for the same cache, item and modification (a newer choice replaces it).
    pub fn same_slot(&self, other: &LaterSkin) -> bool {
        self.root == other.root
            && self.kind == other.kind
            && self.id == other.id
            && self.modification == other.modification
    }
}

/// Parse the stored queue. Anything unreadable gives an empty queue: losing a reminder is
/// harmless, refusing to start is not.
pub fn from_json(text: &str) -> Vec<LaterSkin> {
    serde_json::from_str(text).unwrap_or_default()
}

pub fn to_json(queue: &[LaterSkin]) -> String {
    serde_json::to_string(queue).unwrap_or_else(|_| "[]".into())
}

/// Add `new` to `queue`; an entry for the same cache, item and modification is replaced.
pub fn merge(queue: &mut Vec<LaterSkin>, new: Vec<LaterSkin>) {
    for skin in new {
        queue.retain(|old| !old.same_slot(&skin));
        queue.push(skin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(id: &str, modification: u8, look: &str) -> LaterSkin {
        LaterSkin {
            root: "/cache".into(),
            kind: "hull".into(),
            id: id.into(),
            modification,
            look: look.into(),
            ..LaterSkin::default()
        }
    }

    #[test]
    fn a_newer_choice_replaces_the_same_slot() {
        let mut queue = vec![skin("dictator", 0, "XT"), skin("titan", 1, "XT")];
        merge(&mut queue, vec![skin("dictator", 0, "Prime")]);
        assert_eq!(queue.len(), 2);
        assert!(
            queue
                .iter()
                .any(|s| s.id == "dictator" && s.look == "Prime")
        );
    }

    #[test]
    fn round_trips_and_survives_garbage() {
        let queue = vec![skin("dictator", 0, "XT")];
        assert_eq!(from_json(&to_json(&queue)), queue);
        assert!(from_json("not json").is_empty());
    }
}
