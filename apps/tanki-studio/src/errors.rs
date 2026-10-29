// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! What the user reads when an operation of the core fails. The core reports in English for logs
//! and the CLI; the known failures are said here in the interface's language, with what to do
//! next, and anything else is shown as it is.

use tanki_i18n::tl;

/// A translated message, looked up when it is shown (the language may change).
type Say = fn() -> &'static str;

/// A failure, in the interface's language. The core's own words go to the log file, where a bug
/// report finds them whatever language the interface speaks.
pub fn user_message(error: impl std::fmt::Display) -> String {
    let text = format!("{error:#}");
    log::warn!("{text}");
    // The more specific message first: "target missing" is part of "target resources missing".
    let known: [(&str, Say); 10] = [
        ("target not recognized in this cache", || {
            tl!("This item is not recognised in this cache.")
        }),
        ("pack does not support", || {
            tl!("This pack is for another game.")
        }),
        ("variant has no applicable resources", || {
            tl!("This skin has nothing that can be applied.")
        }),
        ("target resources missing", || {
            tl!("The cache is missing a file this skin replaces. Rescan the cache and try again.")
        }),
        ("required cache resource missing", || {
            tl!("The cache is missing a file this skin replaces. Rescan the cache and try again.")
        }),
        ("target outside selected cache", || {
            tl!("A cache file is not where it should be. Rescan the cache and try again.")
        }),
        ("invalid target path", || {
            tl!("A cache file is not where it should be. Rescan the cache and try again.")
        }),
        ("backup corrupt", || {
            tl!("The backup is damaged and cannot be restored.")
        }),
        ("target missing", || {
            tl!("A file to restore is no longer in the cache.")
        }),
        ("cache folder does not exist", || {
            tl!("The cache folder is gone. Choose it again.")
        }),
    ];
    known
        .iter()
        .find(|(english, _)| text.contains(english))
        .map_or(text.clone(), |(_, say)| say().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tanki_i18n::{Lang, set_current};

    #[test]
    fn known_failures_are_said_in_the_interface_language() {
        set_current(Lang::from_code("ru").unwrap());
        let said = user_message(anyhow::anyhow!("target not recognized in this cache"));
        let resources = user_message(anyhow::anyhow!("target resources missing"));
        set_current(Lang::EN);
        assert_eq!(said, "Этот предмет не опознан в этом кэше.");
        assert!(
            resources.starts_with("В кэше нет файла"),
            "the longer message wins over its part: {resources}"
        );
        assert_eq!(
            user_message(anyhow::anyhow!("pack does not support air-cache")),
            "This pack is for another game."
        );
    }

    #[test]
    fn unknown_failures_are_shown_as_they_are() {
        assert_eq!(user_message(anyhow::anyhow!("disk full")), "disk full");
    }
}
