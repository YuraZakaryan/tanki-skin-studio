// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Updates in the app, through `tanki_core::update`: the check at start (at most every twelve
//! hours, and only while it is on), Check for updates in the palette, the update dialog with
//! Update and restart, Later and Skip this version, and the report after a restart.
//!
//! The checks and the download run on their own threads; their answers arrive here each frame
//! (see [`Studio::poll_updates`]). A failed check at start says nothing: only a check the user
//! asked for reports one.

use std::{
    sync::{
        Arc,
        mpsc::{Receiver, TryRecvError},
    },
    time::Duration,
};

use anyhow::Result;
use eframe::egui;
use tanki_core::update::{
    Cancelled, Release, UpdateProgress,
    install::{self, Install},
};
use tanki_i18n::{fmt, tl};

use crate::app::Studio;

/// Asks for the newest release (blocking; it runs on its own thread). Tests stand in for GitHub
/// with one.
pub type UpdateSource = Arc<dyn Fn() -> Result<Option<Release>> + Send + Sync>;

/// Downloads, checks and installs a release, and schedules the restart (blocking; it runs on its
/// own thread). Tests stand in for it.
pub type Installer = Arc<dyn Fn(&Release, &UpdateProgress) -> Result<()> + Send + Sync>;

/// How often the check at start asks GitHub: once in twelve hours is enough to hear of a release
/// the day it is out, and keeps far below GitHub's limit of requests.
const CHECK_EVERY: u64 = 12 * 60 * 60;

/// What the update dialog shows.
#[derive(Clone)]
pub enum UpdateView {
    /// A check the user asked for is running.
    Checking,
    /// A check the user asked for found nothing newer.
    UpToDate,
    /// A check the user asked for failed.
    Failed(String),
    /// A newer release, with the reason the last attempt to install it failed, if it did.
    Offer {
        release: Release,
        error: Option<String>,
    },
    /// The release is downloading and being put in place.
    Installing {
        release: Release,
        progress: Arc<UpdateProgress>,
    },
}

pub struct Updates {
    /// The user's choice: ask for a newer release when the app starts.
    pub check_at_start: bool,
    /// The desktop app turns the check at start on; tests leave it off, so they never reach GitHub.
    pub startup_check: bool,
    /// How this copy is updated: it replaces itself, or the release page is opened.
    pub install: Install,
    pub source: Option<UpdateSource>,
    pub installer: Option<Installer>,
    pub view: Option<UpdateView>,
    /// The update is in place: the app closes and the helper opens the new version.
    pub restarting: bool,
    started: bool,
    /// The running check, and whether the user asked for it.
    check: Option<(Receiver<Result<Option<Release>>>, bool)>,
    installing: Option<Receiver<Result<()>>>,
    /// A page to open in the browser on the next frame.
    open: Option<String>,
}

impl Updates {
    pub fn new(check_at_start: bool) -> Self {
        Self {
            check_at_start,
            startup_check: false,
            install: install::current(),
            source: None,
            installer: None,
            view: None,
            restarting: false,
            started: false,
            check: None,
            installing: None,
            open: None,
        }
    }

    /// Whether a check is running.
    pub fn checking(&self) -> bool {
        self.check.is_some()
    }
}

/// The version this copy reports: its own, or `TANKI_PRETEND_VERSION` to try an update against
/// the current release.
pub fn running_version() -> String {
    std::env::var("TANKI_PRETEND_VERSION")
        .ok()
        .filter(|v| tanki_core::update::version::parse(v).is_some())
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_owned())
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Studio {
    /// Check for updates: ask GitHub now and show the outcome.
    pub fn check_for_updates(&mut self) {
        if self.updates.installing.is_some() {
            return;
        }
        self.updates.started = true;
        self.updates.view = Some(UpdateView::Checking);
        match &mut self.updates.check {
            // A check at start already runs: its answer now goes to the dialog.
            Some((_, requested)) => *requested = true,
            None => self.begin_check(true),
        }
    }

    /// Turn the check at start on or off; the choice is remembered.
    pub fn set_update_check_at_start(&mut self, on: bool) {
        self.updates.check_at_start = on;
        let value = if on { "on" } else { "off" };
        if let Err(error) = self.store.set("update_check_at_start", value) {
            log::warn!("could not save the update setting: {error}");
        }
        self.toast(
            if on {
                tl!("Tanki Skin Studio checks for updates when it starts.")
            } else {
                tl!("Tanki Skin Studio no longer checks for updates when it starts.")
            },
            None,
        );
    }

    fn check_due(&self) -> bool {
        let last = self
            .store
            .get("update_checked_at")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok());
        // A time in the future (a clock set back) does not hold the check off.
        last.is_none_or(|last| last > now() || now() - last >= CHECK_EVERY)
    }

    fn begin_check(&mut self, requested: bool) {
        let (tx, rx) = std::sync::mpsc::channel();
        let source = self.updates.source.clone();
        let spawned = std::thread::Builder::new()
            .name("tanki-update-check".into())
            .spawn(move || {
                let answer = match source {
                    Some(source) => source(),
                    None => tanki_core::update::check(&running_version()),
                };
                let _ = tx.send(answer);
            });
        match spawned {
            Ok(_) => self.updates.check = Some((rx, requested)),
            Err(error) => self.finish_check(Err(error.into()), requested),
        }
    }

    fn finish_check(&mut self, answer: Result<Option<Release>>, requested: bool) {
        if answer.is_ok()
            && let Err(error) = self.store.set("update_checked_at", &now().to_string())
        {
            log::warn!("could not save the time of the update check: {error}");
        }
        match answer {
            Ok(Some(release)) => {
                let skipped = self
                    .store
                    .get("update_skipped")
                    .ok()
                    .flatten()
                    .and_then(|v| tanki_core::update::version::parse(&v));
                // A skipped version is not offered at start again; a newer one, or a check the
                // user asked for, is.
                if requested || skipped.is_none_or(|s| release.version > s) {
                    log::info!("update available: {}", release.tag);
                    self.updates.view = Some(UpdateView::Offer {
                        release,
                        error: None,
                    });
                } else if matches!(self.updates.view, Some(UpdateView::Checking)) {
                    self.updates.view = None;
                }
            }
            Ok(None) => {
                if requested {
                    self.updates.view = Some(UpdateView::UpToDate);
                }
            }
            Err(error) => {
                log::warn!("update check failed: {error:#}");
                if requested {
                    self.updates.view = Some(UpdateView::Failed(format!("{error:#}")));
                }
            }
        }
    }

    /// Start the check at start once it is due, pick up the answers of checks and installs, open a
    /// page asked for, and close the app once an update is in place. Each frame.
    pub fn poll_updates(&mut self, ctx: &egui::Context) {
        if self.updates.startup_check && !self.updates.started {
            self.updates.started = true;
            if self.updates.check_at_start && self.check_due() {
                self.begin_check(false);
            }
        }
        if let Some((rx, requested)) = &self.updates.check {
            let requested = *requested;
            match rx.try_recv() {
                Ok(answer) => {
                    self.updates.check = None;
                    self.finish_check(answer, requested);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.updates.check = None;
                    self.finish_check(Err(anyhow::anyhow!("the update check stopped")), requested);
                }
            }
        }
        if let Some(rx) = &self.updates.installing {
            match rx.try_recv() {
                Ok(outcome) => {
                    self.updates.installing = None;
                    self.finish_update(outcome);
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(100)),
                Err(TryRecvError::Disconnected) => {
                    self.updates.installing = None;
                    self.finish_update(Err(anyhow::anyhow!("the update stopped")));
                }
            }
        }
        if let Some(url) = self.updates.open.take() {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        if self.updates.restarting {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Update and restart: download and install the offered release. A copy that cannot replace
    /// itself opens the release page instead.
    pub fn start_update(&mut self) {
        let Some(UpdateView::Offer { release, .. }) = self.updates.view.clone() else {
            return;
        };
        if !self.updates.install.can_update_itself() {
            self.download_update();
            return;
        }
        // The restart ends this process: nothing else may be under way.
        if self.import_job.is_some() {
            self.updates.view = Some(UpdateView::Offer {
                release,
                error: Some(tl!("A pack is being imported. Wait until it is done.").to_owned()),
            });
            return;
        }
        if self.bulk_running() {
            self.updates.view = Some(UpdateView::Offer {
                release,
                error: Some(crate::actions::bulk_busy().to_owned()),
            });
            return;
        }
        let progress = Arc::new(UpdateProgress::default());
        let installer = self.updates.installer.clone().unwrap_or_else(|| {
            let install = self.updates.install.clone();
            let data = self.data.clone();
            Arc::new(move |release: &Release, progress: &UpdateProgress| {
                let ready =
                    install::prepare(release, &install, &install::work_dir(&data), progress)?;
                install::restart_after_exit(&ready, &data)
            })
        });
        let (tx, rx) = std::sync::mpsc::channel();
        let (worker_release, worker_progress) = (release.clone(), progress.clone());
        let spawned = std::thread::Builder::new()
            .name("tanki-update".into())
            .spawn(move || {
                let _ = tx.send(installer(&worker_release, &worker_progress));
            });
        match spawned {
            Ok(_) => {
                log::info!("updating to {}", release.tag);
                self.updates.installing = Some(rx);
                self.updates.view = Some(UpdateView::Installing { release, progress });
            }
            Err(error) => {
                self.updates.view = Some(UpdateView::Offer {
                    release,
                    error: Some(error.to_string()),
                });
            }
        }
    }

    fn finish_update(&mut self, outcome: Result<()>) {
        let Some(UpdateView::Installing { release, .. }) = self.updates.view.clone() else {
            return;
        };
        match outcome {
            Ok(()) => {
                // The next start tells whether the new version came up.
                if let Err(error) = self
                    .store
                    .set("update_pending", &release.version.to_string())
                {
                    log::warn!("could not record the update: {error}");
                }
                log::info!("{} is in place; restarting", release.tag);
                self.updates.restarting = true;
            }
            // Only a stop the user asked for: a failure after Cancel is still reported.
            Err(error) if error.is::<Cancelled>() => {
                self.updates.view = Some(UpdateView::Offer {
                    release,
                    error: None,
                });
            }
            Err(error) => {
                log::warn!("update to {} failed: {error:#}", release.tag);
                self.updates.view = Some(UpdateView::Offer {
                    release,
                    error: Some(format!("{error:#}")),
                });
            }
        }
    }

    /// Stop a running download; nothing is replaced.
    pub fn cancel_update(&mut self) {
        if let Some(UpdateView::Installing { progress, .. }) = &self.updates.view {
            progress.cancel();
        }
    }

    /// Open the offered release's page, to download it by hand.
    pub fn download_update(&mut self) {
        if let Some(UpdateView::Offer { release, .. }) = self.updates.view.take() {
            self.updates.open = Some(release.page);
        }
    }

    /// Later: close the dialog; the next check at start offers the release again. A check still
    /// running carries on like a check at start: it opens the dialog only for a release.
    pub fn later_update(&mut self) {
        if matches!(self.updates.view, Some(UpdateView::Installing { .. })) {
            return;
        }
        if let Some((_, requested)) = &mut self.updates.check {
            *requested = false;
        }
        self.updates.view = None;
    }

    /// Skip this version: the check at start offers only a newer one.
    pub fn skip_update(&mut self) {
        if let Some(UpdateView::Offer { release, .. }) = self.updates.view.take()
            && let Err(error) = self
                .store
                .set("update_skipped", &release.version.to_string())
        {
            log::warn!("could not save the skipped version: {error}");
        }
    }

    /// After a restart for an update: say that the new version runs, or why the update did not
    /// finish, and clear what the update left behind.
    pub fn report_finished_update(&mut self) {
        let work = install::work_dir(&self.data);
        let failure = install::helper::take_failure(&work);
        install::clean(&work);
        let pending = self.store.get("update_pending").ok().flatten();
        if pending.is_some() {
            let _ = self.store.set("update_pending", "");
        }
        let version = env!("CARGO_PKG_VERSION");
        if let Some(error) = failure {
            log::warn!("the last update did not finish: {error}");
            self.toast(
                fmt(
                    tl!("The update was not installed: {error}"),
                    &[("error", &error)],
                ),
                None,
            );
        } else if pending.as_deref() == Some(version) {
            self.toast(
                fmt(
                    tl!("Tanki Skin Studio is updated to {version}."),
                    &[("version", version)],
                ),
                None,
            );
        }
    }
}
