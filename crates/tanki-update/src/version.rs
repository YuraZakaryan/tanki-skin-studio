// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Versions as Semantic Versioning orders them: a pre-release (`0.3.0-rc.1`) comes before its
//! release (`0.3.0`), and a tag may carry a leading `v`.

pub use semver::Version;

/// `v0.3.0`, `0.3.0` or `0.3.0-rc.1`; `None` for anything else.
pub fn parse(text: &str) -> Option<Version> {
    let text = text.trim();
    Version::parse(text.strip_prefix(['v', 'V']).unwrap_or(text)).ok()
}

/// Whether `release` is newer than the running `current` version. A version that cannot be read
/// is never offered an update.
pub fn is_newer(release: &Version, current: &str) -> bool {
    parse(current).is_some_and(|current| *release > current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_reads_with_or_without_its_v() {
        assert_eq!(parse("v0.3.0"), Some(Version::new(0, 3, 0)));
        assert_eq!(parse(" 0.3.0 "), Some(Version::new(0, 3, 0)));
        assert_eq!(parse("0.3"), None);
        assert_eq!(parse("latest"), None);
    }

    #[test]
    fn only_a_higher_version_is_newer() {
        let release = parse("0.3.0").unwrap();
        assert!(is_newer(&release, "0.2.0"));
        assert!(is_newer(&release, "0.2.9"));
        assert!(!is_newer(&release, "0.3.0"));
        assert!(!is_newer(&release, "0.10.0"), "numbers, not text");
        assert!(!is_newer(&release, "not a version"));
    }

    #[test]
    fn a_release_is_newer_than_its_own_release_candidate() {
        let release = parse("0.3.0").unwrap();
        assert!(is_newer(&release, "0.3.0-rc.2"));
        assert!(!is_newer(&parse("0.3.0-rc.1").unwrap(), "0.3.0"));
    }
}
