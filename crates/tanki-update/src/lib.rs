// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Layer 1: updates from the project's GitHub Releases.
//!
//! [`check`] asks GitHub for the newest published release. [`install::prepare`] downloads the
//! file this copy was installed from, checks it against the release's `SHA256SUMS.txt` (and
//! GitHub's own digest of the file), and puts it in place where that can happen while the app
//! runs; [`install::restart_after_exit`] then starts a helper that finishes the update once the app
//! has exited and opens the new version. A copy that cannot replace itself (the portable zip,
//! Linux, a development build) is offered the release page instead.

pub mod http;
pub mod install;
pub mod progress;
pub mod release;
pub mod verify;
pub mod version;

use anyhow::Result;

pub use progress::{Cancelled, UpdateProgress, UpdateStage};
pub use release::{Asset, NoteLine, Release};

/// The repository whose releases are the app's.
pub const REPO: &str = "YuraZakaryan/tanki-skin-studio";

/// Where every release is listed.
pub const RELEASES_PAGE: &str = "https://github.com/YuraZakaryan/tanki-skin-studio/releases";

/// The newest published release, when it is newer than `current`. Drafts and pre-releases are
/// never offered: GitHub's "latest release" is the newest full release.
pub fn check(current: &str) -> Result<Option<Release>> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let Some(json) = http::get_api(&url)? else {
        // No release published yet.
        return Ok(None);
    };
    let release = release::parse_latest(&json)?;
    Ok(release.filter(|r| version::is_newer(&r.version, current)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asks the real GitHub (`cargo test -p tanki-update -- --ignored`): the network stays out of
    /// the gate.
    #[test]
    #[ignore = "reaches GitHub"]
    fn the_published_release_is_offered_to_an_older_version() {
        let release = check("0.0.1").unwrap().expect("a release is published");
        assert!(release.version >= version::Version::new(0, 2, 0));
        assert!(release.asset(verify::SUMS).is_some());
        assert_eq!(check("999.0.0").unwrap(), None);
    }
}
