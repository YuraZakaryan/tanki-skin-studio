// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[test]
fn the_limit_round_trips_and_defaults() {
    for limit in LIMITS {
        assert_eq!(limit_from_code(Some(&limit_code(limit))), limit);
    }
    assert_eq!(limit_from_code(None), Some(DEFAULT_LIMIT));
    assert_eq!(limit_from_code(Some("garbage")), Some(DEFAULT_LIMIT));
}

#[test]
fn a_limit_is_said_in_whole_gigabytes() {
    assert_eq!(limit_label(Some(2 * 1024 * 1024 * 1024)), "2 GB");
    assert_eq!(limit_label(Some(10 * 1024 * 1024 * 1024)), "10 GB");
    assert_eq!(limit_label(Some(1)), "1 B");
}
