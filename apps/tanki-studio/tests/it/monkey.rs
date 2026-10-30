// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! A monkey test: hundreds of random clicks, key presses and typed text across the whole app, with
//! fixed seeds so a failure replays exactly. Whatever the order, the app must not panic, must stay
//! on a real page, and every replacement it made must undo back to the original cache. The run is
//! checked to have really gone around the app, so a monkey that only ever misses fails too.

use std::{collections::BTreeSet, path::PathBuf};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use tanki_core::apply::{list_backups, restore_transaction};
use tanki_studio::app::Page;

use crate::common::{contents, env, harness_with_pack};

/// xorshift64: deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

/// Every enabled button on screen, by its accessible label (the harness turns the native file
/// choosers off, so every button may be pressed).
fn clickable_labels(h: &egui_kittest::Harness<'static, tanki_studio::app::Studio>) -> Vec<String> {
    h.query_all_by_role(Role::Button)
        .filter(|node| !node.accesskit_node().is_disabled())
        .filter_map(|node| node.accesskit_node().label())
        .collect()
}

/// The buttons that change the cache, in both languages. A share of the steps presses one of them
/// (a "smart" monkey): two specific clicks in a row (Apply, then Apply in the dialog) would almost
/// never happen among dozens of buttons by chance alone.
/// The fixture's skins are among them: nothing is picked until one is clicked.
const KEY_ACTIONS: &[&str] = &[
    "Apply",
    "Применить",
    "Restore",
    "Восстановить",
    "Wasp Red",
    "Railgun Blue",
];

/// What one seeded run did: the pages it saw and the replacements it made.
struct Run {
    pages: BTreeSet<&'static str>,
    replacements: usize,
}

fn run(seed: u64, steps: usize) -> Run {
    let env = env(&format!("monkey-{seed}"));
    let mut h = harness_with_pack(&env);
    let files: Vec<PathBuf> = env
        .cache
        .hull_files
        .iter()
        .chain(&env.cache.turret_files)
        .cloned()
        .collect();
    let before = contents(&files);
    let mut rng = Rng(seed);
    let mut pages = BTreeSet::new();
    for step in 0..steps {
        match rng.below(12) {
            0 => h.key_press(egui::Key::Escape),
            1 => {
                if let Some(field) = h.query_all_by_role(Role::TextInput).next() {
                    field.focus();
                    field.type_text(["wasp", "rail", "zzz", "m3", "Хорнет"][rng.below(5)]);
                }
            }
            2..=4 => {
                let labels: Vec<String> = clickable_labels(&h)
                    .into_iter()
                    .filter(|l| KEY_ACTIONS.iter().any(|k| l.starts_with(k)))
                    .collect();
                if let Some(label) = labels.get(rng.below(labels.len()))
                    && let Some(node) = h.query_all_by_label(label).next()
                {
                    node.click();
                }
            }
            _ => {
                let labels = clickable_labels(&h);
                if let Some(label) = labels.get(rng.below(labels.len()))
                    && let Some(node) = h.query_all_by_label(label).next()
                {
                    node.click();
                }
            }
        }
        h.run_steps(3);
        let page = h.state().page.code();
        assert!(
            Page::codes().contains(&page),
            "seed {seed}, step {step}: on a real page"
        );
        pages.insert(page);
    }
    // Close whatever is open, then undo every replacement, newest first.
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
    Run {
        pages,
        replacements: backups.len(),
    }
}

#[test]
fn random_use_never_breaks_the_app_and_every_replacement_undoes() {
    let mut pages = BTreeSet::new();
    let mut replacements = 0;
    for seed in [1, 7, 42, 2026] {
        let run = run(seed, 250);
        pages.extend(run.pages);
        replacements += run.replacements;
    }
    assert!(
        pages.len() >= 4,
        "the monkeys went around the app: {pages:?}"
    );
    assert!(replacements > 0, "and some of them applied a skin");
}
