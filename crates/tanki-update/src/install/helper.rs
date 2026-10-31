// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The helper that finishes an update after the app has exited.
//!
//! On macOS it is a shell that waits for the app's process to end and reopens the bundle. On
//! Windows a running exe cannot be replaced, so the app copies itself into its `updates` folder
//! and starts the copy with [`FLAG`]: the copy waits until the app lets go of its data folder's
//! lock (the OS frees it when the process ends, however it ends), runs the new `.msi`, and starts
//! the installed app. A failure is written to [`FAILURE`] for the next start to report, and the
//! app is started again either way.

use std::{
    ffi::OsString,
    fs::{File, OpenOptions, TryLockError},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};

/// The first argument of the Windows helper.
pub const FLAG: &str = "--finish-update";

/// The helper's report of a failed update, in the `updates` folder.
pub const FAILURE: &str = "failure.txt";

/// How long the helper waits for the app to exit.
const WAIT: Duration = Duration::from_secs(120);

/// Reopen `bundle` once the process `pid` has exited (macOS).
pub fn reopen_after_exit(pid: u32, bundle: &Path) -> Result<()> {
    run_after_exit(
        pid,
        Path::new("/usr/bin/open"),
        &[bundle.as_os_str().to_owned()],
    )
}

/// Run `program` with `args` once the process `pid` has exited, from a detached shell.
#[cfg(unix)]
pub fn run_after_exit(pid: u32, program: &Path, args: &[OsString]) -> Result<()> {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(r#"pid=$1; shift; i=0; while kill -0 "$pid" 2>/dev/null && [ $i -lt 1200 ]; do sleep 0.1; i=$((i+1)); done; exec "$@""#)
        .arg("sh")
        .arg(pid.to_string())
        .arg(program)
        .args(args);
    spawn_detached(command).context("could not schedule the restart")
}

#[cfg(not(unix))]
pub fn run_after_exit(_pid: u32, _program: &Path, _args: &[OsString]) -> Result<()> {
    anyhow::bail!("this copy cannot update itself; download the new version instead")
}

/// Copy this program into the `updates` folder and start the copy as the helper that installs
/// `msi`, if it still has the checked `sha256`, once the app has exited, and then starts `exe`
/// (Windows). Run from that folder, the helper does not hold the installed exe the installer
/// replaces.
pub fn install_msi_after_exit(data: &Path, msi: &Path, exe: &Path, sha256: &str) -> Result<()> {
    let program = std::env::current_exe().context("could not find this program")?;
    let dir = super::work_dir(data).join("helper");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).context("could not schedule the update")?;
    let helper = dir.join(
        program
            .file_name()
            .context("this program has no file name")?,
    );
    std::fs::copy(&program, &helper).context("could not schedule the update")?;
    let mut command = Command::new(&helper);
    command.args(args(data, msi, exe, sha256));
    spawn_detached(command).context("could not schedule the update")
}

fn args(data: &Path, msi: &Path, exe: &Path, sha256: &str) -> Vec<OsString> {
    vec![
        FLAG.into(),
        data.into(),
        msi.into(),
        exe.into(),
        sha256.into(),
    ]
}

/// The helper's job, read back from its arguments (without the program name).
#[derive(Debug, PartialEq, Eq)]
struct Job {
    data: PathBuf,
    msi: PathBuf,
    exe: PathBuf,
    /// The SHA-256 the installer had when it was checked.
    sha256: String,
}

fn parse(args: &[OsString]) -> Option<Job> {
    let [flag, data, msi, exe, sha256] = args else {
        return None;
    };
    let sha256 = sha256.to_str().filter(|h| crate::verify::is_sha256(h))?;
    (flag == FLAG).then(|| Job {
        data: data.into(),
        msi: msi.into(),
        exe: exe.into(),
        sha256: sha256.to_owned(),
    })
}

/// When this program was started as the update helper (`args` without the program name begin
/// with [`FLAG`]), finish the update and return the exit code; otherwise `None`.
pub fn run(args: &[OsString]) -> Option<i32> {
    if args.first().is_none_or(|a| a != FLAG) {
        return None;
    }
    let Some(job) = parse(args) else {
        return Some(2);
    };
    let work = super::work_dir(&job.data);
    let done = if wait_for_exit(&job.data, WAIT) {
        // The installer waited in a folder of this user's while the app closed: it runs only if
        // it is still the file that was checked.
        match crate::verify::sha256_file(&job.msi) {
            Ok(hash) if hash == job.sha256 => install_msi(&job),
            Ok(_) => Err(anyhow::anyhow!(
                "the installer changed after it was checked, so it was not run"
            )),
            Err(error) => Err(error),
        }
    } else {
        Err(anyhow::anyhow!(
            "Tanki Skin Studio did not close, so the update was not installed"
        ))
    };
    let _ = std::fs::remove_file(&job.msi);
    if let Err(error) = &done {
        record_failure(&work, &format!("{error:#}"));
    }
    // The new version, or the old one when the installer failed: it is still in place.
    let mut start = Command::new(&job.exe);
    if let Some(dir) = job.exe.parent() {
        start.current_dir(dir);
    }
    if let Err(error) = spawn_detached(start) {
        record_failure(
            &work,
            &format!("could not start {} ({error})", job.exe.display()),
        );
    }
    Some(i32::from(done.is_err()))
}

#[cfg(windows)]
fn install_msi(job: &Job) -> Result<()> {
    let system =
        std::env::var_os("SystemRoot").map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
    let logs = job.data.join("logs");
    let _ = std::fs::create_dir_all(&logs);
    let log = logs.join("update-install.log");
    // /passive shows the installer's progress bar without questions; the package installs for
    // this user only, so it needs no administrator rights.
    let status = Command::new(system.join("System32").join("msiexec.exe"))
        .arg("/i")
        .arg(&job.msi)
        .args(["/passive", "/norestart", "/l*"])
        .arg(&log)
        .status()
        .context("could not start the installer")?;
    match status.code() {
        // 3010 and 1641: installed, a restart of Windows finishes it.
        Some(0 | 3010 | 1641) => Ok(()),
        Some(1602) => anyhow::bail!("the installer was cancelled"),
        code => anyhow::bail!(
            "the installer stopped (code {}); its log is {}",
            code.map_or_else(|| "none".to_owned(), |c| c.to_string()),
            log.display()
        ),
    }
}

#[cfg(not(windows))]
fn install_msi(_job: &Job) -> Result<()> {
    anyhow::bail!("an installer package can only be installed on Windows")
}

/// Wait until no process holds the app's lock in `data` (the app has exited), at most `timeout`.
pub fn wait_for_exit(data: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        // A lock that cannot be tested is taken for a running app: installing over a running app
        // is worse than reporting that the update waited in vain.
        match try_lock(data) {
            Ok(true) => return true,
            _ if Instant::now() >= deadline => return false,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Take and at once free the app's lock: `Ok(false)` while the app holds it.
fn try_lock(data: &Path) -> std::io::Result<bool> {
    let file: File = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(data.join("instance.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(true),
        Err(TryLockError::WouldBlock) => Ok(false),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

fn record_failure(work: &Path, error: &str) {
    let _ = std::fs::create_dir_all(work);
    if let Err(e) = std::fs::write(work.join(FAILURE), error) {
        log::warn!("could not record the failed update ({e}): {error}");
    }
}

/// The helper's report of a failed update in `work`, once: it is removed as it is read.
pub fn take_failure(work: &Path) -> Option<String> {
    let path = work.join(FAILURE);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Start `command` on its own, so it outlives this process.
// The helper is meant to outlive the app: nobody waits for it, and the app exits right after.
#[allow(clippy::zombie_processes)]
fn spawn_detached(mut command: Command) -> std::io::Result<()> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    command.spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("tanki-helper-{label}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_helper_reads_back_its_arguments_and_nothing_else_starts_it() {
        let hash = "3a6d1e5e2186db01b9fadbadea2e4ac5683a6ab842a588136260d31967eb4d7b";
        let job = Job {
            data: "C:/data".into(),
            msi: "C:/data/updates/app.msi".into(),
            exe: "C:/Programs/app.exe".into(),
            sha256: hash.into(),
        };
        assert_eq!(parse(&args(&job.data, &job.msi, &job.exe, hash)), Some(job));
        let odd = args("d".as_ref(), "m".as_ref(), "e".as_ref(), "not a hash");
        assert_eq!(parse(&odd), None);
        assert_eq!(run(&[]), None);
        assert_eq!(run(&["--other".into()]), None);
        assert_eq!(run(&[FLAG.into(), "incomplete".into()]), Some(2));
    }

    #[test]
    fn the_helper_waits_until_the_app_lets_go_of_its_lock() {
        let data = scratch("lock");
        let app = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(data.join("instance.lock"))
            .unwrap();
        app.lock().unwrap();
        assert!(
            !wait_for_exit(&data, Duration::from_millis(250)),
            "the app still runs"
        );
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            drop(app);
        });
        let started = Instant::now();
        assert!(wait_for_exit(&data, Duration::from_secs(10)));
        assert!(
            started.elapsed() >= Duration::from_millis(150),
            "it waited for the exit"
        );
        release.join().unwrap();
        std::fs::remove_dir_all(data).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn an_installer_changed_after_its_check_is_not_run_and_the_app_starts_again() {
        use std::os::unix::fs::PermissionsExt;
        let data = scratch("changed");
        let msi = data.join("app.msi");
        std::fs::write(&msi, b"changed after the check").unwrap();
        // The "app" the helper starts again: a script that leaves a mark.
        let marker = data.join("started");
        let exe = data.join("app.sh");
        std::fs::write(&exe, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let expected = crate::verify::sha256(b"the checked installer");
        assert_eq!(run(&args(&data, &msi, &exe, &expected)), Some(1));
        let failure = take_failure(&super::super::work_dir(&data)).unwrap();
        assert!(
            failure.contains("changed after it was checked"),
            "{failure}"
        );
        assert!(!msi.exists(), "the installer is removed");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(marker.exists(), "the app is started again");
        std::fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn a_failure_is_reported_once() {
        let work = scratch("failure");
        assert_eq!(take_failure(&work), None);
        record_failure(&work, "the installer was cancelled");
        assert_eq!(
            take_failure(&work).as_deref(),
            Some("the installer was cancelled")
        );
        assert_eq!(take_failure(&work), None);
        std::fs::remove_dir_all(work).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_restart_waits_for_the_old_process_to_exit() {
        let dir = scratch("restart");
        let marker = dir.join("started");
        let mut old = Command::new("/bin/sleep").arg("0.5").spawn().unwrap();
        run_after_exit(
            old.id(),
            Path::new("/usr/bin/touch"),
            &[marker.clone().into()],
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !marker.exists(),
            "nothing starts while the old process runs"
        );
        old.wait().unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(
            marker.exists(),
            "the new process started after the old one exited"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
