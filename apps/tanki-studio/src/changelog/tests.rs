// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const SAMPLE: &str = "# Changelog\n\n## [Unreleased]\n\n- Nothing yet.\n\n## [0.2.0] - 2026-10-30\n\n### Added\n\n- **Apply to all items**: one look on every\n  item at once.\n- The `Garage` lists items. More text here.\n\n### Changed\n\n- **Settings** moved.\n\n## [0.1.0] - 2026-10-29\n\n- First release.\n";

#[test]
fn versions_dates_and_headlines_come_from_the_changelog() {
    let releases = parse(SAMPLE);
    assert_eq!(releases.len(), 2, "Unreleased is left out");
    assert_eq!(releases[0].version, "0.2.0");
    assert_eq!(releases[0].date, "2026-10-30");
    assert_eq!(
        releases[0].notes,
        vec![
            "Apply to all items",
            "The Garage lists items",
            "Settings moved"
        ]
    );
    assert_eq!(releases[1].notes, vec!["First release"]);
}

#[test]
fn the_built_in_changelogs_have_the_running_version_in_both_languages() {
    for lang in Lang::all() {
        let releases = releases(lang);
        assert!(
            releases
                .iter()
                .any(|r| r.version == env!("CARGO_PKG_VERSION") && !r.notes.is_empty()),
            "{lang:?}"
        );
        assert!(releases.iter().all(|r| !r.date.is_empty()));
    }
}

#[test]
fn a_long_change_is_cut_at_a_word_and_an_untranslated_version_is_in_english() {
    let long = format!("- {}\n", "слово ".repeat(40));
    let releases = parse(&format!("## [1.0.0] - 2026-01-01\n\n{long}"));
    let note = &releases[0].notes[0];
    assert!(note.ends_with('…') && note.chars().count() <= 121, "{note}");
    assert!(!note.contains("сло…"), "cut at a word: {note}");
    let russian = releases_for_test();
    assert!(
        russian.iter().any(|r| r.version == "0.1.0"),
        "older versions in English"
    );
}

fn releases_for_test() -> Vec<Release> {
    releases(
        Lang::all()
            .find(|l| *l != Lang::EN)
            .expect("a second language"),
    )
}
