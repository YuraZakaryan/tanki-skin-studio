//! The provenance check, enforced by `cargo xtask assets` (part of `cargo xtask ci`), the way
//! PdfCraft guards its assets.
//!
//! The repository must never carry game content: whatever reaches a public commit stays in its
//! history for good. So every file that git would commit is checked for game file types and names,
//! for binary files outside the few places that hold our own (fonts, README screenshots, the design
//! file), for dumps by size, and for names of the original game's official client or its developer.
//! The rules work on a small model so they can be unit tested; `main.rs` feeds it from git.

use std::sync::LazyLock;

/// One file git would commit: its path (with `/`), size and, when it is UTF-8, its text.
pub struct Entry<'a> {
    pub path: &'a str,
    pub size: u64,
    pub text: Option<&'a str>,
}

/// Extensions of game files (models, textures, sounds, archives, packs). Never committed.
const GAME_EXTENSIONS: &[&str] = &[
    "3ds",
    "atf",
    "dds",
    "jpg",
    "jpeg",
    "mp3",
    "ogg",
    "swf",
    "tankipack",
    "tara",
    "wav",
    "zip",
];

/// Names of the files of a client cache item. Never committed, whatever their folder.
const GAME_FILE_NAMES: &[&str] = &[
    "object.3ds",
    "lightmap.jpg",
    "details.jpg",
    "details_alpha.jpg",
    "images.xml",
    "image.jpg",
    "alpha.jpg",
];

/// Our own binary files and the only folders they may live in.
const ALLOWED_BINARIES: &[(&str, &str)] =
    &[("ttf", "crates/tanki-ui/assets/"), ("png", "docs/images/")];

/// A file this large is a dump or an archive, not source.
pub const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

/// No game, game client, loader or developer is ever named: the app finds the client on the user's
/// machine instead. The pattern is assembled from parts so this file does not match itself.
static FORBIDDEN_NAMES: LazyLock<regex::Regex> = LazyLock::new(|| {
    let names = [
        format!("{}[\\s_-]*{}", "tanki", "online"),
        format!("{}{}", "pro", "tanki"),
        format!("{}[\\s_-]*{}", "standalone", "loader"),
        format!("{}{}", "alterna", "tiva"),
    ];
    regex::Regex::new(&format!("(?i){}", names.join("|"))).expect("a valid pattern")
});

fn extension(path: &str) -> Option<String> {
    let name = path.rsplit('/').next()?;
    let (stem, ext) = name.rsplit_once('.')?;
    (!stem.is_empty()).then(|| ext.to_ascii_lowercase())
}

/// Every problem found, one line each; empty when the files are clean.
pub fn check(entries: &[Entry<'_>]) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in entries {
        let path = entry.path;
        let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
        let ext = extension(path);
        if GAME_FILE_NAMES.contains(&name.as_str()) {
            problems.push(format!(
                "{path}: a game cache file; game content is never committed"
            ));
        } else if let Some(ext) = ext.as_deref().filter(|e| GAME_EXTENSIONS.contains(e)) {
            problems.push(format!(
                "{path}: .{ext} is a game file type; game content is never committed"
            ));
        } else if let Some(ext) = ext.as_deref() {
            let homes: Vec<&str> = ALLOWED_BINARIES
                .iter()
                .filter(|(allowed, _)| *allowed == ext)
                .map(|(_, home)| *home)
                .collect();
            if !homes.is_empty() && !homes.iter().any(|home| path.starts_with(home)) {
                problems.push(format!(
                    "{path}: .{ext} files belong in {} only",
                    homes.join(", ")
                ));
            }
        }
        if entry.size > MAX_FILE_BYTES {
            problems.push(format!(
                "{path}: {} bytes, over the {MAX_FILE_BYTES}-byte limit for a source file",
                entry.size
            ));
        }
        if let Some(text) = entry.text {
            for (number, line) in text.lines().enumerate() {
                if let Some(found) = FORBIDDEN_NAMES.find(line) {
                    problems.push(format!(
                        "{path}:{}: names a game, client, loader or developer (\"{}\"); the app finds the client on the user's machine instead",
                        number + 1,
                        found.as_str()
                    ));
                }
            }
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> Entry<'_> {
        Entry {
            path,
            size: 10,
            text: None,
        }
    }

    fn text<'a>(path: &'a str, text: &'a str) -> Entry<'a> {
        Entry {
            path,
            size: text.len() as u64,
            text: Some(text),
        }
    }

    #[test]
    fn our_own_files_are_clean() {
        let clean = [
            file("crates/tanki-ui/assets/Inter.ttf"),
            file("docs/images/garage.png"),
            file("design.pen"),
            file("Cargo.lock"),
            text("README.md", "A skin manager for a local game cache."),
            text(
                "crates/tanki-apply/src/apply.rs",
                "const SKIN: &str = \"object.3ds\";",
            ),
        ];
        assert_eq!(check(&clean), Vec::<String>::new());
    }

    #[test]
    fn game_files_are_refused_by_name_and_by_type() {
        for path in [
            "fixtures/hull/object.3ds",
            "assets/LIGHTMAP.JPG",
            "cache/images.xml",
            "packs/wasp.tankipack",
            "dump/cache.zip",
            "sounds/shot.ogg",
            "textures/hull.jpeg",
        ] {
            assert_eq!(check(&[file(path)]).len(), 1, "{path} must be refused");
        }
    }

    #[test]
    fn our_binaries_stay_in_their_folders() {
        assert_eq!(check(&[file("crates/tanki-ui/assets/x.ttf")]).len(), 0);
        assert_eq!(check(&[file("docs/images/x.png")]).len(), 0);
        let stray = check(&[file("tests/data/skin.png"), file("fonts/x.ttf")]);
        assert_eq!(stray.len(), 2, "{stray:?}");
        assert!(stray[0].contains("docs/images/"));
    }

    #[test]
    fn a_dump_sized_file_is_refused() {
        let big = Entry {
            path: "data/blob.bin",
            size: MAX_FILE_BYTES + 1,
            text: None,
        };
        assert_eq!(check(&[big]).len(), 1);
    }

    #[test]
    fn no_game_client_loader_or_developer_is_ever_named() {
        let official = format!("{} {}", "Tanki", "Online");
        let id = format!("{}-{}", "tanki", "online");
        let folder = format!("{}{}", "tanki", "online");
        let developer = format!("{}{}Platform", "Alterna", "tiva");
        let server = format!("{}{}", "Pro", "Tanki");
        let server_id = format!("{}{}", "pro", "tanki");
        let loader = format!("{}{}", "Standalone", "Loader");
        let loader_path = format!("Application Support/{loader}/Local Store/cache");
        for line in [
            &official,
            &id,
            &folder,
            &developer,
            &server,
            &server_id,
            &loader,
            &loader_path,
        ] {
            let found = check(&[text("README.md", line)]);
            assert_eq!(found.len(), 1, "{line} must be refused");
            assert!(found[0].starts_with("README.md:1:"), "{found:?}");
        }
        let fine = "air-cache, tanki-studio, online play, standalone app, Local Store";
        assert_eq!(check(&[text("a.rs", fine)]).len(), 0);
    }

    #[test]
    fn this_file_does_not_match_itself() {
        let source = include_str!("assets.rs");
        assert_eq!(
            check(&[text("xtask/src/assets.rs", source)]),
            Vec::<String>::new()
        );
    }
}
