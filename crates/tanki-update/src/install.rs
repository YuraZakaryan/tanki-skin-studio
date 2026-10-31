// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Putting a new version in place, the way this copy was installed:
//!
//! | Installed as | Update |
//! |---|---|
//! | the macOS `.app` | the new bundle is copied from the `.dmg` over this one (fine while it runs), then reopened once the app has exited |
//! | the Windows installer (`.msi`) | once the app has exited, a helper runs the new `.msi` (it replaces the old version) and starts the app again |
//! | the Windows portable zip, Linux, a development build | the release page, to download it by hand |
//!
//! The file is downloaded into the app's `updates` folder and must match the SHA-256 that the
//! release's `SHA256SUMS.txt` lists for it before anything is replaced.

pub mod helper;
#[cfg(target_os = "macos")]
mod macos;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::{
    progress::{UpdateProgress, UpdateStage},
    release::Release,
    verify,
    version::Version,
};

/// The largest release file the app downloads; a release file is tens of megabytes.
const MAX_DOWNLOAD: u64 = 1 << 30;

/// How this copy of the app was installed, which decides how it is updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// The macOS `.app` bundle, in a folder this user can write.
    MacApp(PathBuf),
    /// The Windows installer's copy; its exe.
    Msi(PathBuf),
    /// A copy that is updated by downloading the new version by hand.
    Manual(Manual),
}

/// Why a copy cannot replace itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manual {
    /// Built from source (it runs from Cargo's `target` folder).
    Development,
    /// The Windows portable zip, unzipped anywhere.
    Portable,
    /// A macOS app run from the disk image, from a folder this user cannot write, or from the
    /// randomised location macOS runs a downloaded app from until it is moved.
    NotWritable,
    /// A platform without a release file (Linux).
    Unsupported,
}

impl Install {
    pub fn can_update_itself(&self) -> bool {
        !matches!(self, Install::Manual(_))
    }
}

/// How the running copy was installed.
pub fn current() -> Install {
    match std::env::current_exe() {
        Ok(exe) => detect(&exe.canonicalize().unwrap_or(exe)),
        Err(_) => Install::Manual(Manual::Unsupported),
    }
}

/// Whether `exe` runs from a Cargo build folder: the target folder's root carries Cargo's
/// `.rustc_info.json` or its `CACHEDIR.TAG`, which the folders installers use never do.
pub fn is_development_build(exe: &Path) -> bool {
    exe.ancestors()
        .skip(1)
        .any(|dir| dir.join(".rustc_info.json").is_file() || dir.join("CACHEDIR.TAG").is_file())
}

#[cfg(target_os = "macos")]
fn detect(exe: &Path) -> Install {
    if is_development_build(exe) {
        return Install::Manual(Manual::Development);
    }
    match macos::bundle_of(exe) {
        Some(bundle) if macos::can_replace(&bundle) => Install::MacApp(bundle),
        Some(_) => Install::Manual(Manual::NotWritable),
        None => Install::Manual(Manual::Unsupported),
    }
}

#[cfg(windows)]
fn detect(exe: &Path) -> Install {
    if is_development_build(exe) {
        return Install::Manual(Manual::Development);
    }
    // The installer always puts the app in the same per-user folder (it offers no other).
    let Some(installed) = std::env::var_os("LOCALAPPDATA").map(|local| {
        PathBuf::from(local)
            .join("Programs")
            .join("Tanki Skin Studio")
    }) else {
        return Install::Manual(Manual::Portable);
    };
    match (installed.canonicalize(), exe.parent(), exe.file_name()) {
        // The exe by the folder's plain path: the canonical one is a `\\?\` path, which the
        // helper would start the app from.
        (Ok(dir), Some(parent), Some(name)) if same_dir(&dir, parent) => {
            Install::Msi(installed.join(name))
        }
        _ => Install::Manual(Manual::Portable),
    }
}

#[cfg(windows)]
fn same_dir(a: &Path, b: &Path) -> bool {
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
}

#[cfg(not(any(target_os = "macos", windows)))]
fn detect(exe: &Path) -> Install {
    if is_development_build(exe) {
        Install::Manual(Manual::Development)
    } else {
        Install::Manual(Manual::Unsupported)
    }
}

/// The release file for `install`: `tanki-skin-studio-0.3.0-macos-universal.dmg`, or the `.msi`
/// for this processor. The name carries the version, so an older file is never taken for it.
pub fn asset_name(install: &Install, version: &Version) -> Option<String> {
    match install {
        Install::MacApp(_) => Some(format!("tanki-skin-studio-{version}-macos-universal.dmg")),
        Install::Msi(_) => {
            let arch = match std::env::consts::ARCH {
                "x86_64" => "x64",
                "aarch64" => "arm64",
                _ => return None,
            };
            Some(format!("tanki-skin-studio-{version}-windows-{arch}.msi"))
        }
        Install::Manual(_) => None,
    }
}

/// A downloaded and checked update, waiting for the app to exit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ready {
    /// The new bundle is in place; it is reopened once the app has exited.
    MacApp { bundle: PathBuf },
    /// The new installer runs once the app has exited, if it still has the checked `sha256`;
    /// then `exe` is started.
    Msi {
        msi: PathBuf,
        exe: PathBuf,
        sha256: String,
    },
}

/// Download the file of `release` for `install` into `work`, check it against the release's
/// checksums, and put what can be put in place while the app runs. Then the app should call
/// [`restart_after_exit`] and quit.
pub fn prepare(
    release: &Release,
    install: &Install,
    work: &Path,
    progress: &UpdateProgress,
) -> Result<Ready> {
    let Some(name) = asset_name(install, &release.version) else {
        bail!("this copy cannot update itself; download the new version instead");
    };
    let asset = release
        .asset(&name)
        .with_context(|| format!("the release has no {name}"))?;
    anyhow::ensure!(asset.size <= MAX_DOWNLOAD, "{name} is too large");
    let sums = release
        .asset(verify::SUMS)
        .with_context(|| format!("the release has no {}", verify::SUMS))?;
    let sums_text = crate::http::get_text(&sums.url)?;
    if let Some(digest) = &sums.sha256 {
        anyhow::ensure!(
            verify::sha256(sums_text.as_bytes()) == *digest,
            "{} does not match GitHub's checksum of it",
            verify::SUMS
        );
    }
    let expected = verify::expected(&sums_text, asset)?;
    std::fs::create_dir_all(work)
        .with_context(|| format!("could not create {}", work.display()))?;
    let file = work.join(&asset.name);
    progress.set_stage(UpdateStage::Downloading);
    let hash = crate::http::download(&asset.url, &file, asset.size, progress)?;
    progress.set_stage(UpdateStage::Verifying);
    if hash != expected {
        let _ = std::fs::remove_file(&file);
        bail!("the download does not match its checksum");
    }
    // The last moment a cancel is honoured: from here the new version is put in place.
    if let Err(error) = progress.check() {
        let _ = std::fs::remove_file(&file);
        return Err(error);
    }
    progress.set_stage(UpdateStage::Installing);
    match install {
        #[cfg(target_os = "macos")]
        Install::MacApp(bundle) => {
            let placed = macos::install_dmg(&file, bundle, &release.version, work);
            let _ = std::fs::remove_file(&file);
            placed?;
            Ok(Ready::MacApp {
                bundle: bundle.clone(),
            })
        }
        Install::Msi(exe) => Ok(Ready::Msi {
            msi: file,
            exe: exe.clone(),
            sha256: expected,
        }),
        _ => {
            let _ = std::fs::remove_file(&file);
            bail!("this copy cannot update itself; download the new version instead")
        }
    }
}

/// Start the helper that finishes `ready` once this process has exited and opens the new version.
/// `data` is the app's data folder: the helper waits for the app to let go of its lock there.
/// On success the app must quit promptly.
pub fn restart_after_exit(ready: &Ready, data: &Path) -> Result<()> {
    match ready {
        Ready::MacApp { bundle } => helper::reopen_after_exit(std::process::id(), bundle),
        Ready::Msi { msi, exe, sha256 } => helper::install_msi_after_exit(data, msi, exe, sha256),
    }
}

/// The folder of downloads and the helper, inside the app's data folder.
pub fn work_dir(data: &Path) -> PathBuf {
    data.join("updates")
}

/// Remove what an earlier update left in `work`: downloads, a helper, a mounted image's folder.
/// The helper's report of a failure is kept for [`helper::take_failure`].
pub fn clean(work: &Path) {
    let Ok(entries) = std::fs::read_dir(work) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().is_some_and(|n| n == helper::FAILURE) {
            continue;
        }
        // A helper that is still finishing holds its own exe: it goes next time.
        let _ = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tanki-update-{label}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// The whole macOS update against the real latest release, on a stand-in bundle in a temporary
    /// folder: download, checksum, disk image, swap (`cargo test -p tanki-update -- --ignored`).
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "reaches GitHub and downloads a release"]
    fn the_real_release_replaces_an_installed_bundle() {
        let release = crate::check("0.0.1")
            .unwrap()
            .expect("a release is published");
        let dir = scratch("real");
        let bundle = dir.join("Tanki Skin Studio.app");
        std::fs::create_dir_all(bundle.join("Contents/MacOS")).unwrap();
        std::fs::write(
            bundle.join("Contents/Info.plist"),
            "<plist><dict><key>CFBundleIdentifier</key><string>dev.tanki.skinstudio</string><key>CFBundleShortVersionString</key><string>0.0.1</string></dict></plist>",
        )
        .unwrap();
        let progress = UpdateProgress::default();
        let ready = prepare(
            &release,
            &Install::MacApp(bundle.clone()),
            &dir.join("updates"),
            &progress,
        )
        .unwrap();
        assert_eq!(
            ready,
            Ready::MacApp {
                bundle: bundle.clone()
            }
        );
        let plist = std::fs::read_to_string(bundle.join("Contents/Info.plist")).unwrap();
        assert!(
            plist.contains(&format!("<string>{}</string>", release.version)),
            "{plist}"
        );
        assert!(bundle.join("Contents/MacOS/tanki-studio").is_file());
        let (done, total) = progress.bytes();
        assert!(total > 1_000_000 && done == total, "{done} of {total}");
        assert_eq!(
            std::fs::read_dir(dir.join("updates")).unwrap().count(),
            0,
            "the download and the mount point are gone"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_cargo_build_is_a_development_build_and_an_installed_copy_is_not() {
        let root = scratch("dev");
        let target = root.join("target");
        std::fs::create_dir_all(target.join("release")).unwrap();
        std::fs::write(
            target.join("CACHEDIR.TAG"),
            "Signature: 8a477f597d28d172789f06886806bc55",
        )
        .unwrap();
        assert!(is_development_build(&target.join("release/tanki-studio")));
        let installed = root.join("Programs/Tanki Skin Studio");
        std::fs::create_dir_all(&installed).unwrap();
        assert!(!is_development_build(&installed.join("tanki-studio.exe")));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_release_file_is_named_by_version_and_platform() {
        let version = Version::new(0, 3, 0);
        assert_eq!(
            asset_name(
                &Install::MacApp("/Applications/Tanki Skin Studio.app".into()),
                &version
            )
            .as_deref(),
            Some("tanki-skin-studio-0.3.0-macos-universal.dmg")
        );
        let msi = asset_name(&Install::Msi("tanki-studio.exe".into()), &version);
        match std::env::consts::ARCH {
            "x86_64" => assert_eq!(
                msi.as_deref(),
                Some("tanki-skin-studio-0.3.0-windows-x64.msi")
            ),
            "aarch64" => assert_eq!(
                msi.as_deref(),
                Some("tanki-skin-studio-0.3.0-windows-arm64.msi")
            ),
            _ => assert_eq!(msi, None),
        }
        assert_eq!(
            asset_name(&Install::Manual(Manual::Portable), &version),
            None
        );
        assert!(!Install::Manual(Manual::Development).can_update_itself());
    }

    #[test]
    fn a_copy_that_cannot_update_itself_downloads_nothing() {
        let release = Release {
            tag: "v0.3.0".into(),
            version: Version::new(0, 3, 0),
            page: String::new(),
            notes: Vec::new(),
            assets: Vec::new(),
        };
        let work = scratch("manual");
        let error = prepare(
            &release,
            &Install::Manual(Manual::Portable),
            &work,
            &UpdateProgress::default(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("download the new version"),
            "{error}"
        );
        assert_eq!(std::fs::read_dir(&work).unwrap().count(), 0);
        std::fs::remove_dir_all(work).unwrap();
    }

    #[test]
    fn cleaning_keeps_only_the_report_of_a_failure() {
        let work = scratch("clean");
        std::fs::write(work.join("old.dmg"), b"x").unwrap();
        std::fs::write(work.join("old.part"), b"x").unwrap();
        std::fs::create_dir_all(work.join("helper")).unwrap();
        std::fs::write(work.join(helper::FAILURE), "it failed").unwrap();
        clean(&work);
        let left: Vec<_> = std::fs::read_dir(&work)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, vec![std::ffi::OsString::from(helper::FAILURE)]);
        std::fs::remove_dir_all(work).unwrap();
    }
}
