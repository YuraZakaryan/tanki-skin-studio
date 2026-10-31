// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Replacing the macOS app bundle with the one in a release's disk image. A running app can be
//! replaced: the process keeps the files it opened, and the bundle is swapped by renaming, so the
//! folder never holds half of each version.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::version::Version;

/// The bundle identifier `packaging/macos/package.sh` writes into `Info.plist`.
const BUNDLE_ID: &str = "dev.tanki.skinstudio";
/// The app's bundle inside the disk image.
const BUNDLE_NAME: &str = "Tanki Skin Studio.app";

// Apple's tools by full path: an app started from Finder has a minimal `PATH`.
const HDIUTIL: &str = "/usr/bin/hdiutil";
const DITTO: &str = "/usr/bin/ditto";

/// The app bundle that holds `exe`, when it is this app's.
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    let bundle = exe
        .ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app"))?;
    (plist_string(bundle, "CFBundleIdentifier").as_deref() == Some(BUNDLE_ID))
        .then(|| bundle.to_path_buf())
}

/// Whether `bundle` can be replaced: not the read-only location macOS runs a downloaded app from
/// until it is moved (App Translocation), and in a folder this user can write (not the disk image).
pub fn can_replace(bundle: &Path) -> bool {
    if bundle.to_string_lossy().contains("/AppTranslocation/") {
        return false;
    }
    let Some(parent) = bundle.parent() else {
        return false;
    };
    let probe = parent.join(format!(".tanki-update-probe-{}", std::process::id()));
    let writable = std::fs::create_dir(&probe).is_ok();
    let _ = std::fs::remove_dir(&probe);
    writable
}

/// The `<string>` value of `key` in the bundle's `Info.plist` (the XML form the build writes).
fn plist_string(bundle: &Path, key: &str) -> Option<String> {
    let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).ok()?;
    let after = &plist[plist.find(&format!("<key>{key}</key>"))?..];
    let start = after.find("<string>")? + "<string>".len();
    let end = after[start..].find("</string>")? + start;
    Some(after[start..end].trim().to_owned())
}

/// Whether `bundle` is this app at `version`.
fn is_release(bundle: &Path, version: &Version) -> bool {
    plist_string(bundle, "CFBundleIdentifier").as_deref() == Some(BUNDLE_ID)
        && plist_string(bundle, "CFBundleShortVersionString")
            .and_then(|v| crate::version::parse(&v))
            .is_some_and(|v| {
                (v.major, v.minor, v.patch) == (version.major, version.minor, version.patch)
            })
}

fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .output()
        .with_context(|| format!("could not run {program}"))?;
    if output.status.success() {
        Ok(())
    } else {
        bail!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
}

/// Copy the app out of the disk image `dmg` over `bundle`, after checking it is this app at
/// `version`. `work` holds the image's mount point.
pub fn install_dmg(dmg: &Path, bundle: &Path, version: &Version, work: &Path) -> Result<()> {
    let mount = work.join(format!("mount-{}", std::process::id()));
    let _ = std::fs::remove_dir(&mount);
    std::fs::create_dir_all(&mount).context("could not create a folder for the disk image")?;
    // The download's SHA-256 is already checked, so hdiutil's own pass (-noverify) adds nothing.
    let mut attached = Err(anyhow::anyhow!("not attached"));
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(attempt));
        }
        attached = run(
            HDIUTIL,
            &[
                "attach".as_ref(),
                "-nobrowse".as_ref(),
                "-readonly".as_ref(),
                "-noautoopen".as_ref(),
                "-noverify".as_ref(),
                "-mountpoint".as_ref(),
                mount.as_os_str(),
                dmg.as_os_str(),
            ],
        );
        if attached.is_ok() {
            break;
        }
    }
    if let Err(error) = attached {
        let _ = std::fs::remove_dir(&mount);
        return Err(error.context("could not open the disk image"));
    }
    let copied = copy_app(&mount.join(BUNDLE_NAME), bundle, version);
    let detached = run(
        HDIUTIL,
        &["detach".as_ref(), mount.as_os_str(), "-quiet".as_ref()],
    )
    .or_else(|_| {
        run(
            HDIUTIL,
            &["detach".as_ref(), mount.as_os_str(), "-force".as_ref()],
        )
    });
    if let Err(error) = detached {
        log::warn!("could not detach the disk image: {error:#}");
    }
    let _ = std::fs::remove_dir(&mount);
    copied
}

fn copy_app(source: &Path, bundle: &Path, version: &Version) -> Result<()> {
    anyhow::ensure!(
        is_release(source, version),
        "the disk image does not hold Tanki Skin Studio {version}"
    );
    let parent = bundle.parent().context("the app is not in a folder")?;
    let name = bundle
        .file_name()
        .context("the app has no name")?
        .to_string_lossy()
        .into_owned();
    let staged = parent.join(format!(".{name}.update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    // ditto keeps the bundle's symlinks and signature; the disk image's Finder metadata and
    // extended attributes stay behind (a quarantine flag among them, there is none to begin with).
    run(
        DITTO,
        &[
            "--norsrc".as_ref(),
            "--noextattr".as_ref(),
            "--noacl".as_ref(),
            source.as_os_str(),
            staged.as_os_str(),
        ],
    )
    .context("could not copy the new version")?;
    if !is_release(&staged, version) {
        let _ = std::fs::remove_dir_all(&staged);
        bail!("the copied app is incomplete");
    }
    swap(
        &staged,
        bundle,
        &parent.join(format!(".{name}.old-{}", std::process::id())),
    )
}

/// Put `staged` where `bundle` is, keeping `bundle` as `old` until the new one is there.
fn swap(staged: &Path, bundle: &Path, old: &Path) -> Result<()> {
    let _ = std::fs::remove_dir_all(old);
    std::fs::rename(bundle, old).context("could not move the old version aside")?;
    if let Err(error) = std::fs::rename(staged, bundle) {
        let _ = std::fs::rename(old, bundle);
        let _ = std::fs::remove_dir_all(staged);
        return Err(anyhow::Error::new(error).context("could not put the new version in place"));
    }
    let _ = std::fs::remove_dir_all(old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tanki-macos-{label}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn bundle(at: &Path, id: &str, version: &str) {
        std::fs::create_dir_all(at.join("Contents/MacOS")).unwrap();
        std::fs::write(
            at.join("Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\"?><plist><dict>\n<key>CFBundleIdentifier</key><string>{id}</string>\n<key>CFBundleShortVersionString</key><string>{version}</string>\n</dict></plist>"
            ),
        )
        .unwrap();
        std::fs::write(at.join("Contents/MacOS/tanki-studio"), version).unwrap();
    }

    #[test]
    fn only_this_apps_bundle_is_recognised_with_its_version() {
        let dir = scratch("bundle");
        let app = dir.join("Tanki Skin Studio.app");
        bundle(&app, BUNDLE_ID, "0.2.0");
        let exe = app.join("Contents/MacOS/tanki-studio");
        assert_eq!(bundle_of(&exe), Some(app.clone()));
        assert!(is_release(&app, &Version::new(0, 2, 0)));
        assert!(!is_release(&app, &Version::new(0, 3, 0)));
        assert!(can_replace(&app));
        let other = dir.join("Other.app");
        bundle(&other, "com.example.other", "0.2.0");
        assert_eq!(bundle_of(&other.join("Contents/MacOS/tanki-studio")), None);
        assert!(!can_replace(Path::new(
            "/private/var/folders/x/AppTranslocation/1234/d/Tanki Skin Studio.app"
        )));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_new_bundle_replaces_the_old_one_whole() {
        let dir = scratch("swap");
        let app = dir.join("Tanki Skin Studio.app");
        bundle(&app, BUNDLE_ID, "0.2.0");
        std::fs::write(app.join("Contents/stale"), "from 0.2.0").unwrap();
        let source = dir.join("image/Tanki Skin Studio.app");
        bundle(&source, BUNDLE_ID, "0.3.0");
        copy_app(&source, &app, &Version::new(0, 3, 0)).unwrap();
        assert!(is_release(&app, &Version::new(0, 3, 0)));
        assert!(
            !app.join("Contents/stale").exists(),
            "nothing of the old version is left"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with('.'))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_image_with_another_version_replaces_nothing() {
        let dir = scratch("wrong");
        let app = dir.join("Tanki Skin Studio.app");
        bundle(&app, BUNDLE_ID, "0.2.0");
        let source = dir.join("image/Tanki Skin Studio.app");
        bundle(&source, BUNDLE_ID, "0.2.5");
        assert!(copy_app(&source, &app, &Version::new(0, 3, 0)).is_err());
        assert!(
            is_release(&app, &Version::new(0, 2, 0)),
            "the old version stays"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_real_disk_image_is_opened_and_copied() {
        let dir = scratch("dmg");
        let stage = dir.join("stage");
        bundle(&stage.join(BUNDLE_NAME), BUNDLE_ID, "0.3.0");
        let dmg = dir.join("app.dmg");
        let created = Command::new(HDIUTIL)
            .args([
                "create",
                "-quiet",
                "-fs",
                "HFS+",
                "-format",
                "UDZO",
                "-srcfolder",
            ])
            .arg(&stage)
            .arg(&dmg)
            .status()
            .unwrap();
        assert!(created.success(), "hdiutil create failed");
        let app = dir.join("Applications/Tanki Skin Studio.app");
        bundle(&app, BUNDLE_ID, "0.2.0");
        install_dmg(&dmg, &app, &Version::new(0, 3, 0), &dir.join("work")).unwrap();
        assert!(is_release(&app, &Version::new(0, 3, 0)));
        assert_eq!(
            std::fs::read_dir(dir.join("work")).unwrap().count(),
            0,
            "the mount point is removed"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
