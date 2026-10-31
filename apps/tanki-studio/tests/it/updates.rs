// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! New releases: the check at start and Check for updates, the update dialog, the update itself
//! and the report after the restart. GitHub and the installer are stood in for: nothing here
//! reaches the network or replaces the app.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};
use tanki_core::{
    state::StateStore,
    update::install::{Install, Manual},
};
use tanki_studio::{
    app::Studio,
    commands::Command,
    dev::sample_release,
    updates::{UpdateSource, UpdateView},
};

use crate::common::{Env, env, harness};

/// A stand-in for GitHub that answers `answer` and counts how often it was asked.
fn source(answer: Result<Option<&str>, &str>) -> (UpdateSource, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let answer = answer.map(|v| v.map(str::to_owned)).map_err(str::to_owned);
    let source: UpdateSource = Arc::new(move || {
        counted.fetch_add(1, Ordering::SeqCst);
        match &answer {
            Ok(version) => Ok(version.as_deref().map(sample_release)),
            Err(error) => Err(anyhow::anyhow!("{error}")),
        }
    });
    (source, calls)
}

/// The app opened on `env` as the desktop app opens: it checks at start, through `source`, and
/// updates itself as an installed copy does.
fn starting(env: &Env, source: &UpdateSource) -> Harness<'static, Studio> {
    let mut h = harness(env);
    let app = h.state_mut();
    app.updates.source = Some(source.clone());
    app.updates.install = Install::Msi(env.work.join("Programs/tanki-studio.exe"));
    app.updates.startup_check = true;
    h
}

/// Run frames until `done` holds (the checks and installs answer from their own threads).
fn settle(h: &mut Harness<'static, Studio>, done: impl Fn(&Studio) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done(h.state()) {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the update"
        );
        h.run_steps(2);
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(3);
}

/// Run frames until no check is running.
fn checked(h: &mut Harness<'static, Studio>) {
    h.run_steps(2);
    settle(h, |app| !app.updates.checking());
}

fn offered(app: &Studio) -> Option<String> {
    match &app.updates.view {
        Some(UpdateView::Offer { release, .. }) => Some(release.version.to_string()),
        _ => None,
    }
}

fn toasts(h: &Harness<'static, Studio>) -> Vec<String> {
    h.state().toasts.iter().map(|t| t.text.clone()).collect()
}

fn store(env: &Env) -> StateStore {
    StateStore::open(&env.work.join("data/state.sqlite")).unwrap()
}

#[test]
fn nothing_asks_github_until_the_desktop_app_turns_the_check_on() {
    let env = env("update-off");
    let (source, calls) = source(Ok(Some("9.0.0")));
    let mut h = harness(&env);
    h.state_mut().updates.source = Some(source);
    h.run_steps(10);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "tests and scripted runs never ask GitHub"
    );
    assert!(h.state().updates.view.is_none());
}

#[test]
fn a_newer_release_is_offered_at_start_and_later_closes_the_offer() {
    let env = env("update-offer");
    let (source, calls) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    settle(&mut h, |app| offered(app).is_some());
    assert_eq!(offered(h.state()).as_deref(), Some("9.0.0"));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    h.get_by_label("Update and restart");
    h.get_by_label("Skip this version");
    h.get_by_label("Later").click();
    h.run_steps(3);
    assert!(h.state().updates.view.is_none(), "Later closes the offer");
}

#[test]
fn the_check_at_start_asks_at_most_every_twelve_hours_and_not_at_all_when_off() {
    let env = env("update-throttle");
    let (first, first_calls) = source(Ok(None));
    let mut h = starting(&env, &first);
    checked(&mut h);
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert!(
        h.state().updates.view.is_none(),
        "nothing newer: the check at start says nothing"
    );
    drop(h);

    // A restart an hour later does not ask again.
    let (again, again_calls) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &again);
    h.run_steps(10);
    assert_eq!(again_calls.load(Ordering::SeqCst), 0);
    assert!(h.state().updates.view.is_none());
    // Turned off, it does not ask even when it is due.
    h.state_mut().set_update_check_at_start(false);
    drop(h);
    store(&env).set("update_checked_at", "0").unwrap();
    let mut h = starting(&env, &again);
    assert!(
        !h.state().updates.check_at_start,
        "the choice survives a restart"
    );
    h.run_steps(10);
    assert_eq!(again_calls.load(Ordering::SeqCst), 0);
    assert!(h.state().updates.view.is_none());
}

#[test]
fn a_failed_check_at_start_says_nothing_and_a_requested_one_says_why() {
    let env = env("update-failed");
    let (source, calls) = source(Err("could not reach GitHub"));
    let mut h = starting(&env, &source);
    checked(&mut h);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(h.state().updates.view.is_none());
    assert!(toasts(&h).is_empty());

    h.state_mut().run(Command::CheckUpdates);
    checked(&mut h);
    assert!(
        matches!(&h.state().updates.view, Some(UpdateView::Failed(e)) if e.contains("could not reach GitHub"))
    );
    // The footer's Close and the × share the name; either closes it.
    h.get_all_by_label("Close").next().unwrap().click();
    h.run_steps(3);
    assert!(h.state().updates.view.is_none());
}

#[test]
fn check_for_updates_says_when_this_is_the_newest_version() {
    let env = env("update-current");
    let (source, calls) = source(Ok(None));
    let mut h = harness(&env);
    h.state_mut().updates.source = Some(source);
    h.state_mut().run(Command::CheckUpdates);
    checked(&mut h);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(h.state().updates.view, Some(UpdateView::UpToDate)));
    // Its switch turns the check at start off, and the choice is stored.
    h.get_by_label("Check for updates when the app starts")
        .click();
    h.run_steps(3);
    assert!(!h.state().updates.check_at_start);
    assert_eq!(
        store(&env).get("update_check_at_start").unwrap().as_deref(),
        Some("off")
    );
    h.get_by_label("Done").click();
    h.run_steps(3);
    assert!(h.state().updates.view.is_none());
}

#[test]
fn a_skipped_version_is_not_offered_at_start_but_a_newer_one_and_a_requested_check_are() {
    let env = env("update-skip");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    settle(&mut h, |app| offered(app).is_some());
    h.get_by_label("Skip this version").click();
    h.run_steps(3);
    assert!(h.state().updates.view.is_none());
    drop(h);

    store(&env).set("update_checked_at", "0").unwrap();
    let mut h = starting(&env, &source);
    checked(&mut h);
    assert!(
        h.state().updates.view.is_none(),
        "the skipped version is not offered at start"
    );
    h.state_mut().run(Command::CheckUpdates);
    checked(&mut h);
    assert_eq!(
        offered(h.state()).as_deref(),
        Some("9.0.0"),
        "a requested check shows it"
    );
    drop(h);

    store(&env).set("update_checked_at", "0").unwrap();
    let (newer, _) = self::source(Ok(Some("9.1.0")));
    let mut h = starting(&env, &newer);
    settle(&mut h, |app| offered(app).is_some());
    assert_eq!(offered(h.state()).as_deref(), Some("9.1.0"));
}

#[test]
fn update_and_restart_installs_the_release_and_closes_the_app() {
    let env = env("update-install");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    let installed = Arc::new(Mutex::new(Vec::new()));
    let seen = installed.clone();
    h.state_mut().updates.installer = Some(Arc::new(move |release, _progress| {
        seen.lock().unwrap().push(release.tag.clone());
        Ok(())
    }));
    settle(&mut h, |app| offered(app).is_some());
    h.get_by_label("Update and restart").click();
    settle(&mut h, |app| app.updates.restarting);
    assert_eq!(*installed.lock().unwrap(), vec!["v9.0.0".to_owned()]);
    assert_eq!(
        store(&env).get("update_pending").unwrap().as_deref(),
        Some("9.0.0"),
        "the next start tells whether the new version came up"
    );
}

#[test]
fn a_failed_update_is_offered_again_with_the_reason_and_a_cancelled_one_without() {
    let env = env("update-retry");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    h.state_mut().updates.installer = Some(Arc::new(|_, _| {
        anyhow::bail!("the download does not match its checksum")
    }));
    settle(&mut h, |app| offered(app).is_some());
    h.get_by_label("Update and restart").click();
    settle(&mut h, |app| {
        matches!(
            &app.updates.view,
            Some(UpdateView::Offer { error: Some(_), .. })
        )
    });
    assert!(matches!(
        &h.state().updates.view,
        Some(UpdateView::Offer { error: Some(e), .. }) if e.contains("checksum")
    ));
    assert!(
        !h.state().updates.restarting,
        "nothing restarts after a failure"
    );
    h.get_by_label("Try again");
    h.get_by_label("Download");

    // A cancelled download comes back to the plain offer.
    h.state_mut().updates.installer = Some(Arc::new(|_, progress| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !progress.is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(tanki_core::update::Cancelled.into())
    }));
    h.get_by_label("Try again").click();
    settle(&mut h, |app| {
        matches!(app.updates.view, Some(UpdateView::Installing { .. }))
    });
    h.get_by_label("Cancel").click();
    settle(&mut h, |app| {
        matches!(
            &app.updates.view,
            Some(UpdateView::Offer { error: None, .. })
        )
    });
    assert!(!h.state().updates.restarting);
}

#[test]
fn a_copy_that_cannot_update_itself_is_offered_the_download() {
    let env = env("update-portable");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    h.state_mut().updates.install = Install::Manual(Manual::Portable);
    let called = Arc::new(AtomicUsize::new(0));
    let count = called.clone();
    h.state_mut().updates.installer = Some(Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    settle(&mut h, |app| offered(app).is_some());
    assert!(h.query_by_label("Update and restart").is_none());
    h.get_by_label("Download").click();
    h.run_steps(3);
    assert!(
        h.state().updates.view.is_none(),
        "the release page opens instead"
    );
    assert_eq!(called.load(Ordering::SeqCst), 0, "nothing is installed");
}

#[test]
fn an_update_waits_until_a_pack_is_imported() {
    let env = env("update-busy");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    let called = Arc::new(AtomicUsize::new(0));
    let count = called.clone();
    h.state_mut().updates.installer = Some(Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }));
    settle(&mut h, |app| offered(app).is_some());
    let (_sender, receiver) = std::sync::mpsc::channel();
    h.state_mut().import_job = Some(tanki_studio::app::ImportJob {
        name: "big.tankipack".into(),
        archive: false,
        progress: Arc::new(tanki_core::pack::ImportProgress::default()),
        started: Instant::now(),
        receiver,
    });
    h.get_by_label("Update and restart").click();
    h.run_steps(3);
    assert!(matches!(
        &h.state().updates.view,
        Some(UpdateView::Offer { error: Some(e), .. }) if e.contains("being imported")
    ));
    assert_eq!(called.load(Ordering::SeqCst), 0);
}

#[test]
fn after_the_restart_the_new_version_or_the_failure_is_reported_once() {
    let env = env("update-report");
    store(&env)
        .set("update_pending", env!("CARGO_PKG_VERSION"))
        .unwrap();
    let work = env.work.join("data/updates");
    std::fs::create_dir_all(work.join("helper")).unwrap();
    std::fs::write(
        work.join("tanki-skin-studio-9.0.0-windows-x64.msi"),
        b"left over",
    )
    .unwrap();
    let h = harness(&env);
    assert!(
        toasts(&h).iter().any(|t| t.contains("is updated to")),
        "{:?}",
        toasts(&h)
    );
    assert_eq!(
        std::fs::read_dir(&work).unwrap().count(),
        0,
        "what the update left behind is removed"
    );
    drop(h);

    let h = harness(&env);
    assert!(toasts(&h).is_empty(), "said once");
    drop(h);

    store(&env).set("update_pending", "9.0.0").unwrap();
    std::fs::write(work.join("failure.txt"), "the installer was cancelled").unwrap();
    let h = harness(&env);
    assert!(
        toasts(&h)
            .iter()
            .any(|t| t.contains("not installed") && t.contains("the installer was cancelled")),
        "{:?}",
        toasts(&h)
    );
}

#[test]
fn a_pack_dropped_during_an_update_is_not_imported() {
    let env = env("update-drop");
    let (source, _) = source(Ok(Some("9.0.0")));
    let mut h = starting(&env, &source);
    h.state_mut().updates.installer = Some(Arc::new(|_, progress| {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !progress.is_cancelled() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(tanki_core::update::Cancelled.into())
    }));
    settle(&mut h, |app| offered(app).is_some());
    // A drop while the offer is open waits too: the update would cut its import off.
    h.state_mut().handle_dropped(vec![env.zip.clone()]);
    assert!(h.state().packs.is_empty());
    h.get_by_label("Update and restart").click();
    settle(&mut h, |app| {
        matches!(app.updates.view, Some(UpdateView::Installing { .. }))
    });
    h.state_mut().handle_dropped(vec![env.zip.clone()]);
    h.run_steps(3);
    assert!(
        h.state().import_job.is_none(),
        "no import starts under the update"
    );
    assert!(h.state().packs.is_empty());
    h.get_by_label("Cancel").click();
    settle(&mut h, |app| offered(app).is_some());
}

#[test]
fn a_start_into_another_version_does_not_claim_the_update() {
    let env = env("update-other");
    store(&env).set("update_pending", "9.0.0").unwrap();
    let h = harness(&env);
    assert!(
        toasts(&h).iter().all(|t| !t.contains("updated to")),
        "{:?}",
        toasts(&h)
    );
}

#[test]
fn a_check_time_in_the_future_does_not_hold_the_check_off() {
    let env = env("update-clock");
    store(&env)
        .set("update_checked_at", &u64::MAX.to_string())
        .unwrap();
    let (source, calls) = source(Ok(None));
    let mut h = starting(&env, &source);
    checked(&mut h);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a clock set back still checks"
    );
}
