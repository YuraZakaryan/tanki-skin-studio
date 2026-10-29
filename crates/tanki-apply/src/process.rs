// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Is the game running? Replacing cache files under a running client can corrupt them, so the UI
//! asks the user to close it first.
//!
//! No client is known by name. A cache sits in its client's Adobe AIR application storage,
//! `<base>/<application id>/Local Store/cache`, so the cache's own path says whose it is
//! ([`tanki_cache::cache::client_id`]). Every AIR application carries its descriptor,
//! `META-INF/AIR/application.xml`, beside its executable (inside `Contents/Resources` in a macOS
//! bundle), and the descriptor's `<id>` is that same application id. A running process is the
//! client when its descriptor's id matches the cache's. The names the user sees are the process
//! names found on their machine.

use std::path::Path;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tanki_cache::cache::client_id;

/// Where an AIR application keeps its descriptor, relative to a folder above its executable.
const DESCRIPTORS: [&str; 2] = [
    "META-INF/AIR/application.xml",
    "Resources/META-INF/AIR/application.xml",
];
/// How many folders above the executable are searched (a macOS bundle's `Contents` is two up).
const ANCESTORS: usize = 3;
/// A descriptor larger than this is not one.
const MAX_DESCRIPTOR_BYTES: u64 = 1024 * 1024;

/// The application id an AIR descriptor declares: the text of its `<id>` element.
pub fn descriptor_id(xml: &str) -> Option<&str> {
    let start = xml.find("<id>")? + "<id>".len();
    let length = xml.get(start..)?.find("</id>")?;
    let id = xml.get(start..start + length)?.trim();
    (!id.is_empty()).then_some(id)
}

/// The id of the AIR application `executable` belongs to, from the descriptor installed with it.
pub fn air_application_id(executable: &Path) -> Option<String> {
    executable
        .ancestors()
        .skip(1)
        .take(ANCESTORS)
        .flat_map(|folder| {
            DESCRIPTORS
                .iter()
                .map(move |relative| folder.join(relative))
        })
        .filter(|path| {
            path.metadata()
                .is_ok_and(|meta| meta.is_file() && meta.len() <= MAX_DESCRIPTOR_BYTES)
        })
        .find_map(|path| {
            let xml = std::fs::read_to_string(path).ok()?;
            descriptor_id(&xml).map(str::to_owned)
        })
}

/// Names of the running processes of the client whose cache is `cache_root`, without duplicates.
/// Empty when the folder is not in an AIR application's storage (a cache chosen by hand elsewhere).
pub fn running_game_processes(cache_root: &Path) -> Vec<String> {
    let Some(client) = client_id(cache_root) else {
        return Vec::new();
    };
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut names: Vec<String> = system
        .processes()
        .values()
        .filter(|process| {
            process
                .exe()
                .and_then(air_application_id)
                .is_some_and(|id| id == client)
        })
        .map(|process| process.name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.dedup();
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tanki-process-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn descriptor(id: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?>\n<application xmlns=\"http://ns.adobe.com/air/application/4.0\">\n  <id>{id}</id>\n  <filename>Client</filename>\n</application>\n"
        )
    }

    #[test]
    fn the_descriptor_id_is_read_and_junk_is_refused() {
        assert_eq!(
            descriptor_id(&descriptor("com.example.Game")),
            Some("com.example.Game")
        );
        assert_eq!(descriptor_id("<id>  spaced  </id>"), Some("spaced"));
        for junk in [
            "",
            "<id></id>",
            "<id>   </id>",
            "<id>no end",
            "no id at all",
            "</id><id>",
        ] {
            assert_eq!(descriptor_id(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn an_installed_air_client_is_known_by_its_descriptor_on_both_layouts() {
        let root = scratch("layouts");
        // macOS: Client.app/Contents/MacOS/Client, descriptor in Contents/Resources.
        let bundle = root.join("Client.app/Contents");
        std::fs::create_dir_all(bundle.join("MacOS")).unwrap();
        std::fs::create_dir_all(bundle.join("Resources/META-INF/AIR")).unwrap();
        std::fs::write(
            bundle.join("Resources/META-INF/AIR/application.xml"),
            descriptor("mac.app"),
        )
        .unwrap();
        assert_eq!(
            air_application_id(&bundle.join("MacOS/Client")).as_deref(),
            Some("mac.app")
        );
        // Windows: Client.exe with META-INF beside it.
        let install = root.join("Client");
        std::fs::create_dir_all(install.join("META-INF/AIR")).unwrap();
        std::fs::write(
            install.join("META-INF/AIR/application.xml"),
            descriptor("win.app"),
        )
        .unwrap();
        assert_eq!(
            air_application_id(&install.join("Client.exe")).as_deref(),
            Some("win.app")
        );
        // Any other program has no descriptor.
        assert_eq!(
            air_application_id(&root.join("Other.app/Contents/MacOS/Other")),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_cache_names_its_client_through_its_storage_folder() {
        let cache =
            Path::new("/Users/u/Library/Application Support/com.example.Game/Local Store/cache");
        assert_eq!(client_id(cache).as_deref(), Some("com.example.Game"));
        assert_eq!(client_id(Path::new("/Users/u/Desktop/cache")), None);
    }

    #[test]
    fn a_cache_outside_any_air_storage_finds_no_running_client() {
        assert!(running_game_processes(Path::new("/nowhere/cache")).is_empty());
        let cache = Path::new("/nowhere/no.such.client/Local Store/cache");
        assert!(
            running_game_processes(cache).is_empty(),
            "this test process is not the game"
        );
    }
}
