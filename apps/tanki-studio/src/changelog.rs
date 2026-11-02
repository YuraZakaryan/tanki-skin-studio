// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The release history the About page shows, read from the changelog built into the app (so it
//! works offline and in the interface language): each version, its date and the headline of
//! each change.

use tanki_i18n::Lang;

const EN: &str = include_str!("../../../CHANGELOG.md");
const RU: &str = include_str!("../../../CHANGELOG.ru.md");

/// One version of the changelog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    /// `2026-11-02`.
    pub date: String,
    /// The headline of each change: its bold lead, else its first sentence.
    pub notes: Vec<String>,
}

/// The releases in the interface language, newest first. A version the translation does not
/// cover (the Russian changelog starts later) comes from the English one.
pub fn releases(lang: Lang) -> Vec<Release> {
    let english = parse(EN);
    if lang == Lang::EN {
        return english;
    }
    let translated = parse(RU);
    english
        .into_iter()
        .map(|release| {
            translated
                .iter()
                .find(|r| r.version == release.version)
                .cloned()
                .unwrap_or(release)
        })
        .collect()
}

/// The releases of a Keep a Changelog file, newest first; "Unreleased" is left out.
pub fn parse(text: &str) -> Vec<Release> {
    let mut releases: Vec<Release> = Vec::new();
    let mut note: Option<String> = None;
    let flush = |releases: &mut Vec<Release>, note: &mut Option<String>| {
        if let (Some(text), Some(release)) = (note.take(), releases.last_mut()) {
            let headline = headline(&text);
            if !headline.is_empty() {
                release.notes.push(headline);
            }
        }
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## [") {
            flush(&mut releases, &mut note);
            let Some((version, tail)) = rest.split_once(']') else {
                continue;
            };
            let unreleased = version.eq_ignore_ascii_case("unreleased");
            releases.push(Release {
                version: if unreleased {
                    String::new()
                } else {
                    version.to_owned()
                },
                date: tail.trim_start_matches([' ', '-']).trim().to_owned(),
                notes: Vec::new(),
            });
        } else if line.starts_with("## ") || line.starts_with("### ") || line.trim().is_empty() {
            flush(&mut releases, &mut note);
        } else if let Some(item) = line.strip_prefix("- ") {
            flush(&mut releases, &mut note);
            note = Some(item.to_owned());
        } else if line.starts_with("  ")
            && let Some(text) = note.as_mut()
        {
            text.push(' ');
            text.push_str(line.trim());
        }
    }
    flush(&mut releases, &mut note);
    releases.retain(|r| !r.version.is_empty());
    releases
}

/// The headline of a change: its text up to the first ":" or ". " ("**Apply to all items**: one
/// look…" gives "Apply to all items"), without Markdown, and at most [`HEADLINE`] characters,
/// cut at a word with "…".
fn headline(text: &str) -> String {
    let plain = text.trim().replace("**", "").replace('`', "");
    let end = [plain.find(": "), plain.find(". "), plain.find(':')]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(plain.len());
    let line = plain[..end].trim_end_matches(['.', ' ']).to_owned();
    if line.chars().count() <= HEADLINE {
        return line;
    }
    let mut cut: String = line.chars().take(HEADLINE).collect();
    if let Some(space) = cut.rfind(' ') {
        cut.truncate(space);
    }
    format!("{}…", cut.trim_end_matches([',', ' ']))
}

/// The longest headline, in characters.
const HEADLINE: usize = 120;

#[cfg(test)]
mod tests;
