// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A deep monkey ("chaos") test: random clicks, keys, shortcuts and text, plus everything the world
//! can do to the app while it runs: the window is resized, the game starts and stops, packs good and
//! broken are imported or dropped, the cache folder disappears and comes back, and the app is
//! restarted on the same data. Seeds are fixed, so a failure replays exactly.
//!
//! After every step the app must hold its invariants (a real page, an active pack that exists, no
//! pile of toasts, nothing cut off at the window's sides); at the end every replacement must undo
//! back to the original cache, byte for byte. See `docs/test-checklist.md`.

use std::path::{Path, PathBuf};

use egui::{Key, Modifiers, accesskit::Role, vec2};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use tanki_core::apply::{list_backups, restore_transaction};
use tanki_studio::app::{Page, Studio};
use tanki_testkit::{PackOptions, build_pack_dir};

use crate::common::{Env, contents, env, harness};

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

/// Window sizes from the smallest the app allows to a large monitor.
const SIZES: &[(f32, f32)] = &[
    (860.0, 600.0),
    (1000.0, 700.0),
    (1280.0, 800.0),
    (1440.0, 900.0),
    (1920.0, 1200.0),
];
const TEXTS: &[&str] = &[
    "wasp",
    "rail",
    "m3",
    "zzz",
    "Хорнет",
    "  ",
    "/0/0/10/14/",
    "🙂",
    "a-very-long-item-name-that-never-ends",
];
const KEY_ACTIONS: &[&str] = &["Apply", "Применить", "Restore", "Восстановить"];

/// Packs and folders the monkey imports or drops: good ones and broken ones.
struct Inputs {
    good_zip: PathBuf,
    broken_zip: PathBuf,
    other_game: PathBuf,
    archive: PathBuf,
    not_a_pack: PathBuf,
}

fn inputs(env: &Env) -> Inputs {
    let dir = env.work.join("inputs");
    std::fs::create_dir_all(&dir).expect("inputs dir");
    let broken_zip = dir.join("broken.tankipack");
    std::fs::write(&broken_zip, b"PK\x03\x04 this is not a zip").expect("broken zip");
    let other_game = build_pack_dir(
        &dir.join("other-game"),
        &PackOptions {
            id: "other-game",
            games: &["other-game"],
            ..PackOptions::default()
        },
    )
    .expect("other game pack");
    let archive = dir.join("archive");
    for (path, bytes) in [
        ("Flash/Hulls/New/Wasp/M3/object.3ds", b"model".as_slice()),
        ("Flash/Hulls/New/Wasp/M3/lightmap.jpg", b"jpg"),
        ("Flash/Hulls/New/Wasp/M3/details.png", b"png"),
    ] {
        let file = archive.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    let not_a_pack = dir.join("notes");
    std::fs::create_dir_all(&not_a_pack).unwrap();
    std::fs::write(not_a_pack.join("readme.txt"), b"hello").unwrap();
    Inputs {
        good_zip: env.zip.clone(),
        broken_zip,
        other_game,
        archive,
        not_a_pack,
    }
}

fn buttons(h: &Harness<'static, Studio>) -> Vec<String> {
    h.query_all_by_role(Role::Button)
        .filter(|n| !n.accesskit_node().is_disabled())
        .filter_map(|n| n.accesskit_node().label())
        .collect()
}

fn click(h: &Harness<'static, Studio>, label: &str) {
    if let Some(node) = h.query_all_by_label(label).next() {
        node.click();
    }
}

/// The app on `env`. Text the monkey types may hold characters no bundled font has (an emoji):
/// the app then draws a box as it does for a user, instead of the strict test default that panics
/// (every other UI test keeps the strict check for the app's own strings).
fn open(env: &Env) -> Harness<'static, Studio> {
    let h = harness(env);
    h.ctx
        .set_missing_glyph_policy(egui::MissingGlyphPolicy::Tofu);
    h
}

/// The invariants that must hold after any step.
fn check(h: &Harness<'static, Studio>, size: (f32, f32), at: &str) {
    let state = h.state();
    assert!(
        Page::codes().contains(&state.page.code()),
        "{at}: a real page"
    );
    assert!(state.toasts.len() <= 3, "{at}: toasts do not pile up");
    let mut texts: Vec<&str> = state.toasts.iter().map(|t| t.text.as_str()).collect();
    texts.sort_unstable();
    texts.dedup();
    assert_eq!(texts.len(), state.toasts.len(), "{at}: no message twice");
    if let Some(active) = &state.active_pack {
        assert!(
            state
                .packs
                .iter()
                .any(|p| p.manifest.id == active.manifest.id
                    && p.manifest.version == active.manifest.version),
            "{at}: the active pack is in the library"
        );
    }
    // Nothing that can be pressed or read sticks out at the window's sides. Widget rectangles are
    // in points, so the window is measured in points too (the interface size zooms them).
    let width = h.ctx.content_rect().width();
    for node in h
        .query_all_by_role(Role::Button)
        .chain(h.query_all_by_role(Role::Label))
    {
        let rect = node.rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        let label = node.accesskit_node().label().unwrap_or_default();
        assert!(
            rect.min.x >= -1.0 && rect.max.x <= width + 1.0,
            "{at}: {label:?} at {rect:?} is cut off by a {width}-point wide window ({size:?} px)"
        );
    }
}

fn hide_cache(root: &Path, hidden: &Path, gone: &mut bool) {
    let moved = if *gone {
        std::fs::rename(hidden, root)
    } else {
        std::fs::rename(root, hidden)
    };
    if moved.is_ok() {
        *gone = !*gone;
    }
}

fn run(seed: u64, steps: usize) -> (usize, usize) {
    let env = env(&format!("chaos-{seed}"));
    let inputs = inputs(&env);
    let root = env.cache.root.clone();
    let hidden = env.work.join("cache-hidden");
    let files: Vec<PathBuf> = env
        .cache
        .hull_files
        .iter()
        .chain(&env.cache.turret_files)
        .cloned()
        .collect();
    let before = contents(&files);
    let mut h = open(&env);
    h.state_mut().import_path(&inputs.good_zip);
    h.run_steps(4);
    let mut rng = Rng(seed);
    let mut size = (1440.0, 900.0);
    let mut cache_gone = false;
    let mut pages = std::collections::BTreeSet::new();
    for step in 0..steps {
        let at = format!("seed {seed}, step {step}");
        match rng.below(40) {
            0 => {
                size = *rng.pick(SIZES);
                h.set_size(vec2(size.0, size.1));
            }
            1 => h.key_press(*rng.pick(&[Key::Escape, Key::Enter, Key::Tab, Key::ArrowDown])),
            2 => {
                let key = *rng.pick(&[Key::K, Key::Z, Key::Plus, Key::Minus, Key::Num0, Key::I]);
                h.key_combination_modifiers(Modifiers::COMMAND, &[key]);
            }
            3 => {
                if let Some(field) = h.query_all_by_role(Role::TextInput).next() {
                    field.focus();
                    field.type_text(rng.pick::<&str>(TEXTS));
                }
            }
            4 => {
                let running = rng.below(2) == 0;
                h.state_mut().game_processes_override = Some(if running {
                    vec!["GameClient.exe".into()]
                } else {
                    Vec::new()
                });
            }
            5 => {
                let path = match rng.below(5) {
                    0 => &inputs.good_zip,
                    1 => &inputs.broken_zip,
                    2 => &inputs.other_game,
                    3 => &inputs.archive,
                    _ => &inputs.not_a_pack,
                };
                h.state_mut().handle_dropped(vec![path.clone()]);
            }
            6 => hide_cache(&root, &hidden, &mut cache_gone),
            7 => h.state_mut().rescan(),
            8 => {
                // A restart on the same data folder: the session must come back without trouble.
                drop(h);
                h = open(&env);
                h.state_mut().game_processes_override = Some(Vec::new());
            }
            9..=14 => {
                let keys: Vec<String> = buttons(&h)
                    .into_iter()
                    .filter(|l| KEY_ACTIONS.iter().any(|k| l.starts_with(k)))
                    .collect();
                if let Some(label) = keys.get(rng.below(keys.len())) {
                    click(&h, label);
                }
            }
            _ => {
                let all = buttons(&h);
                if let Some(label) = all.get(rng.below(all.len())) {
                    click(&h, label);
                }
            }
        }
        h.run_steps(3);
        check(&h, size, &at);
        pages.insert(h.state().page.code());
    }
    if cache_gone {
        hide_cache(&root, &hidden, &mut cache_gone);
    }
    h.state_mut().dialog = None;
    h.run_steps(2);
    let backups = list_backups(&env.work.join("data/backups")).expect("backups readable");
    for backup in &backups {
        restore_transaction(&backup.dir, &backup.tx).expect("every backup restores");
    }
    assert_eq!(
        contents(&files),
        before,
        "seed {seed}: after {} replacements the cache is the original again",
        backups.len()
    );
    (pages.len(), backups.len())
}

/// `TANKI_CHAOS_SEEDS=40 TANKI_CHAOS_STEPS=800` runs a deeper search than the default; a failing
/// seed it reports can be added to the fixed list.
fn seeds_and_steps() -> (Vec<u64>, usize) {
    let number = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok());
    let seeds = match number("TANKI_CHAOS_SEEDS") {
        Some(n) => (1..=n).map(|i| i * 7919).collect(),
        None => vec![3, 11, 99, 512, 4096, 65537],
    };
    let steps = number("TANKI_CHAOS_STEPS").map_or(400, |n| n as usize);
    (seeds, steps)
}

#[test]
fn the_app_survives_chaos_and_every_replacement_undoes() {
    let mut pages = 0;
    let mut replacements = 0;
    let (seeds, steps) = seeds_and_steps();
    for seed in seeds {
        let (p, r) = run(seed, steps);
        pages = pages.max(p);
        replacements += r;
    }
    assert!(pages >= 4, "the monkey went around the app");
    assert!(replacements > 0, "and applied skins");
}
