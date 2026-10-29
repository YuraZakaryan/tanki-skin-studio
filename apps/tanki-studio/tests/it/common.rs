use std::path::PathBuf;

use egui_kittest::Harness;
use tanki_core::{session::SessionState, state::StateStore};
use tanki_studio::app::{Page, Startup, Studio, TABS};
use tanki_testkit::{CacheFixture, PackOptions, build_cache, build_pack_dir, scratch, zip_dir};

/// A synthetic game cache, a pack to import, and an empty data folder.
pub struct Env {
    pub work: PathBuf,
    pub cache: CacheFixture,
    pub zip: PathBuf,
}

pub fn env(label: &str) -> Env {
    let work = scratch(label);
    let cache = build_cache(&work.join("cache")).expect("cache fixture");
    let pack = build_pack_dir(&work.join("pack"), &PackOptions::default()).expect("pack fixture");
    let zip = zip_dir(&pack, &work.join("fixture.tankipack"), &[]).expect("zip fixture");
    Env { work, cache, zip }
}

/// The app opened on `env` (the same data folder every time, so a second harness is a restart).
pub fn harness(env: &Env) -> Harness<'static, Studio> {
    let data = env.work.join("data");
    let root = env.cache.root.clone();
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(move |cc| {
            let store = StateStore::open(&data.join("state.sqlite")).expect("state store");
            let mut session: SessionState = store.session();
            session.sanitize(&Page::codes(), &TABS, &tanki_studio::model::Filter::codes());
            let startup = Startup {
                store,
                session,
                data,
                roots: Some(vec![("air-cache".into(), root)]),
                skip_splash: true,
            };
            let mut app = Studio::new(cc, startup).expect("studio starts");
            // A player's own running game must not change what a test sees.
            app.game_processes_override = Some(Vec::new());
            // A native file chooser would block the test.
            app.file_dialogs = false;
            // Tests want the answer of the content search at once, not a frame later.
            app.background_recognition = false;
            app
        });
    harness.run_steps(4);
    harness
}

/// `harness` with the fixture pack already imported and active.
pub fn harness_with_pack(env: &Env) -> Harness<'static, Studio> {
    let mut harness = harness(env);
    harness.state_mut().import_path(&env.zip);
    harness.run_steps(4);
    harness
}

pub fn contents(files: &[PathBuf]) -> Vec<Vec<u8>> {
    files
        .iter()
        .map(|p| std::fs::read(p).unwrap_or_default())
        .collect()
}
