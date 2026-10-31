// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A release as GitHub's REST API describes it (`GET /repos/{repo}/releases/latest`): its version,
//! page, files and the "What's new" part of its notes.

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::{REPO, version::Version};

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The tag, `v0.3.0`.
    pub tag: String,
    pub version: Version,
    /// The release's page on GitHub.
    pub page: String,
    /// The "What's new" section of the notes, empty when the notes have none.
    pub notes: Vec<NoteLine>,
    pub assets: Vec<Asset>,
}

/// One downloadable file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
    /// GitHub's SHA-256 of the file (lowercase hex), when the API reports it.
    pub sha256: Option<String>,
}

/// A line of the "What's new" section, as plain text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NoteLine {
    /// `### Added`.
    Heading(String),
    /// A list item, its wrapped lines joined.
    Item(String),
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

#[derive(Deserialize)]
struct ApiRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

#[derive(Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    #[serde(default)]
    digest: Option<String>,
}

/// The release in `json`, or `None` for a draft, a pre-release or a tag that is not a version.
/// Links that do not point into this repository's releases are refused: the app opens the page
/// and downloads the files.
pub fn parse_latest(json: &str) -> Result<Option<Release>> {
    let api: ApiRelease = serde_json::from_str(json).context("the release list is not readable")?;
    if api.draft || api.prerelease {
        return Ok(None);
    }
    let Some(version) = crate::version::parse(&api.tag_name) else {
        return Ok(None);
    };
    let page_prefix = format!("https://github.com/{REPO}/releases/");
    anyhow::ensure!(
        api.html_url.starts_with(&page_prefix),
        "the release page is not in {REPO}"
    );
    let download_prefix = format!("https://github.com/{REPO}/releases/download/");
    let assets = api
        .assets
        .into_iter()
        .filter(|a| a.browser_download_url.starts_with(&download_prefix))
        .map(|a| Asset {
            sha256: a
                .digest
                .as_deref()
                .and_then(|d| d.strip_prefix("sha256:"))
                .filter(|hex| crate::verify::is_sha256(hex))
                .map(str::to_ascii_lowercase),
            name: a.name,
            url: a.browser_download_url,
            size: a.size,
        })
        .collect();
    Ok(Some(Release {
        tag: api.tag_name,
        version,
        page: api.html_url,
        notes: whats_new(api.body.as_deref().unwrap_or_default()),
        assets,
    }))
}

/// The `## What's new` section of release notes in Markdown, as headings and list items in plain
/// text. Wrapped list items (continuation lines indented) are joined.
pub fn whats_new(body: &str) -> Vec<NoteLine> {
    let mut lines = Vec::new();
    let mut inside = false;
    let mut item: Option<String> = None;
    for raw in body.lines() {
        let line = raw.trim_end();
        if let Some(title) = line.strip_prefix("## ") {
            if inside {
                break;
            }
            inside = title.trim().to_ascii_lowercase().starts_with("what's new");
            continue;
        }
        if !inside {
            continue;
        }
        let trimmed = line.trim_start();
        let continues = !trimmed.is_empty() && line.len() > trimmed.len() && item.is_some();
        if continues {
            if let Some(item) = &mut item {
                item.push(' ');
                item.push_str(trimmed);
            }
            continue;
        }
        if let Some(text) = item.take() {
            lines.push(NoteLine::Item(plain(&text)));
        }
        if let Some(heading) = trimmed.strip_prefix("### ") {
            lines.push(NoteLine::Heading(plain(heading)));
        } else if let Some(text) = trimmed.strip_prefix("- ").or(trimmed.strip_prefix("* ")) {
            item = Some(text.to_owned());
        }
    }
    if let Some(text) = item {
        lines.push(NoteLine::Item(plain(&text)));
    }
    lines
}

/// Markdown inline syntax removed: `code`, **bold** marks and `[text](link)` → text.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let (before, after) = rest.split_at(start);
        out.push_str(before);
        match after.find("](").zip(after.find(')')) {
            Some((mid, end)) if mid < end => {
                out.push_str(&after[1..mid]);
                rest = &after[end + 1..];
            }
            _ => {
                out.push('[');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(tag: &str, draft: bool, prerelease: bool, body: &str) -> String {
        serde_json::json!({
            "tag_name": tag,
            "html_url": format!("https://github.com/{REPO}/releases/tag/{tag}"),
            "draft": draft,
            "prerelease": prerelease,
            "body": body,
            "assets": [
                {
                    "name": "SHA256SUMS.txt",
                    "browser_download_url": format!("https://github.com/{REPO}/releases/download/{tag}/SHA256SUMS.txt"),
                    "size": 556,
                    "digest": "sha256:644CFD480A2D519EC6305038FE13EAF2B0E2F00089E949F910B1960747F2DF4A"
                },
                {
                    "name": "elsewhere.msi",
                    "browser_download_url": "https://example.com/elsewhere.msi",
                    "size": 1
                }
            ]
        })
        .to_string()
    }

    #[test]
    fn a_published_release_is_read_with_its_files_and_digests() {
        let release = parse_latest(&json("v0.3.0", false, false, ""))
            .unwrap()
            .unwrap();
        assert_eq!(release.tag, "v0.3.0");
        assert_eq!(release.version, Version::new(0, 3, 0));
        assert!(release.page.ends_with("/releases/tag/v0.3.0"));
        let sums = release.asset("SHA256SUMS.txt").unwrap();
        assert_eq!(sums.size, 556);
        assert_eq!(
            sums.sha256.as_deref(),
            Some("644cfd480a2d519ec6305038fe13eaf2b0e2f00089e949f910b1960747f2df4a")
        );
        assert!(
            release.asset("elsewhere.msi").is_none(),
            "a file outside the repository's releases is never downloaded"
        );
    }

    #[test]
    fn drafts_pre_releases_and_odd_tags_are_not_offered() {
        assert_eq!(
            parse_latest(&json("v0.3.0", true, false, "")).unwrap(),
            None
        );
        assert_eq!(
            parse_latest(&json("v0.3.0-rc.1", false, true, "")).unwrap(),
            None
        );
        assert_eq!(
            parse_latest(&json("nightly", false, false, "")).unwrap(),
            None
        );
    }

    #[test]
    fn a_release_page_outside_the_repository_is_refused() {
        let text = json("v0.3.0", false, false, "").replace(
            &format!("https://github.com/{REPO}/releases/tag/"),
            "https://example.com/",
        );
        assert!(parse_latest(&text).is_err());
        assert!(parse_latest("not json").is_err());
    }

    #[test]
    fn whats_new_is_the_notes_section_as_plain_text() {
        let body = "## What's new in 0.3.0\n\n### Added\n\n- The app offers a **new** release\n  at start, from `GitHub`.\n- See [the guide](https://example.com).\n\n### Fixed\n\n* One fix.\n\n## Downloads\n\n- not a change\n";
        assert_eq!(
            whats_new(body),
            vec![
                NoteLine::Heading("Added".into()),
                NoteLine::Item("The app offers a new release at start, from GitHub.".into()),
                NoteLine::Item("See the guide.".into()),
                NoteLine::Heading("Fixed".into()),
                NoteLine::Item("One fix.".into()),
            ]
        );
    }

    #[test]
    fn notes_without_the_section_have_nothing_new_to_show() {
        assert!(whats_new("## Downloads\n\n- a file\n").is_empty());
        assert!(whats_new("").is_empty());
    }
}
