// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A pack imported on its own thread with a progress card, so a pack of hundreds of megabytes never
//! freezes the window.

use std::{
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use egui_kittest::{Harness, kittest::Queryable};
use tanki_core::pack::ImportProgress;
use tanki_studio::app::{ImportJob, Imported, Studio};

use crate::common::{env, harness};

fn toasts(h: &Harness<'static, Studio>) -> Vec<String> {
    h.state().toasts.iter().map(|t| t.text.clone()).collect()
}

/// An import that never answers until the test sends its result: the card stays up.
fn pending_job(
    h: &mut Harness<'static, Studio>,
) -> (Arc<ImportProgress>, mpsc::Sender<anyhow::Result<Imported>>) {
    let progress = Arc::new(ImportProgress::default());
    let (sender, receiver) = mpsc::channel();
    h.state_mut().import_job = Some(ImportJob {
        name: "big.tankipack".into(),
        archive: false,
        progress: progress.clone(),
        started: Instant::now(),
        receiver,
    });
    h.run_steps(2);
    (progress, sender)
}

#[test]
fn an_import_runs_off_the_window_and_lands_in_the_library() {
    let env = env("import-background");
    let mut h = harness(&env);
    h.state_mut().background_import = true;
    h.state_mut().import_path(&env.zip);
    let progress = h
        .state()
        .import_job
        .as_ref()
        .map(|job| job.progress.clone())
        .expect("the import runs on its own thread; the window goes on at once");
    let deadline = Instant::now() + Duration::from_secs(20);
    while h.state().import_job.is_some() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        h.run();
    }
    assert!(h.state().import_job.is_none(), "the import finished");
    assert!(
        h.state().active_pack.is_some(),
        "the pack is in the library: {:?}",
        toasts(&h)
    );
    assert_eq!(h.state().packs.len(), 1);
    assert_eq!(
        progress.fraction(),
        Some(1.0),
        "the progress reached the end"
    );
    assert!(
        toasts(&h).iter().any(|t| t.contains("imported")),
        "{:?}",
        toasts(&h)
    );
}

#[test]
fn the_card_shows_the_import_and_cancel_adds_nothing() {
    let env = env("import-cancel");
    let mut h = harness(&env);
    let (progress, sender) = pending_job(&mut h);
    h.get_by_label("Importing “big.tankipack”");
    h.get_by_label("Cancel import").click();
    h.run_steps(2);
    assert!(
        progress.is_cancelled(),
        "Cancel stops the import at its next file"
    );
    assert!(
        h.state().import_job.is_some(),
        "the card waits for the import to stop"
    );
    // The import stops with the error of a cancelled one.
    sender
        .send(Err(anyhow::anyhow!("import cancelled")))
        .unwrap();
    h.run_steps(2);
    assert!(h.state().import_job.is_none());
    assert!(h.state().packs.is_empty(), "nothing was added");
    assert!(
        h.state().dialog.is_none(),
        "a cancel is no failure to explain"
    );
    assert_eq!(toasts(&h), ["Import cancelled. Nothing was added."]);
    assert!(
        h.query_by_label("Importing “big.tankipack”").is_none(),
        "the card is gone"
    );
}

#[test]
fn a_second_import_waits_until_the_first_is_done() {
    let env = env("import-twice");
    let mut h = harness(&env);
    let (_progress, _sender) = pending_job(&mut h);
    h.state_mut().handle_dropped(vec![env.zip.clone()]);
    h.state_mut().import_path(&env.zip);
    h.run_steps(2);
    assert!(
        h.state().packs.is_empty(),
        "nothing else was imported meanwhile"
    );
    assert_eq!(
        h.state().import_job.as_ref().map(|job| job.name.as_str()),
        Some("big.tankipack"),
        "the running import is left alone"
    );
    assert!(
        toasts(&h)
            .iter()
            .any(|t| t == "A pack is being imported. Wait until it is done."),
        "{:?}",
        toasts(&h)
    );
}
