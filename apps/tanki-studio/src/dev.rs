// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Hooks for tests and screenshots, driven by environment variables. They call the same methods
//! the buttons call, so end-to-end tests exercise the real wiring without a mouse.
//!
//! - `TANKI_VIEW=<page>`: start on that page (`overview`, `garage`, `resources`, `packs`, `info`).
//! - `TANKI_LANG=<code>` / `TANKI_THEME=<dark|light>`: override the stored preference for this run.
//! - `TANKI_ACTIONS="verb:arg;verb:arg"`: run actions after start-up:
//!   `import:<path>`, `page:<code>`, `select:<kind|/marker/>`, `grade:<0-3>`, `filter:<code>`,
//!   `pick:<n>` (picks the selection's n-th skin, as a click on it does),
//!   `apply:<item>:<variant>` (applies at once), `begin:<item>:<variant>` (opens the dialog),
//!   `confirm`, `restore` (everything back, no question), `ask-restore` (the Restore button's
//!   question), `cache-switcher` (the cache card's switcher, in place of any question at start),
//!   `palette:<query>`, `label:<marker>:<kind>:<id>:<grade>`, `dismiss`
//!   (closes the toasts, which are never part of what a restart restores), `close` (closes the
//!   dialog, as its Done or Cancel does), `importing:<name>:<percent>` (the progress card of an
//!   import that stays at that point, copying, about 20 seconds from the end),
//!   `update-offer:<version>` (the offer of a release with notes, by a copy that updates itself),
//!   `update-manual:<version>` (the same offer to a portable copy), `updating:<version>:<percent>`
//!   (the download at that point), `update-current` and `update-failed` (the outcomes of Check for
//!   updates). The release is made up: nothing is downloaded.
//!   "Apply to all items": `bulk` (opens it), `bulk-look:<n>`, `bulk-next`, `bulk-back`,
//!   `bulk-off:<0-3>`, `bulk-source:<0-3>:<own|0-3>`, `bulk-row:<kind>:<id>` (opens an item's own
//!   choices), `bulk-pick:<kind>:<id>:<0-3>:<0-3|edition|off|common>`, `bulk-kind:<kind>:<on|off>`,
//!   `bulk-apply` (writes at once, without the thread), `bulk-running:<done>` (the run frozen with
//!   that many items done, for the screenshot), `bulk-undo` (the result's Undo all) and
//!   `restore-batch` (the newest batch's entry in the backups, confirmed).
//!   Also `lang:<code>`, `theme:<system|dark|light>` and `zoom:<+|-|0>`, which call the same methods as the
//!   switches and shortcuts, so the choice is stored like a real one.
//! - `TANKI_GAME_PROCESSES=<a,b>`: the processes "found" when checking whether the game runs
//!   (empty: none), instead of looking at the real process list.
//! - `TANKI_SCRIPT="step;step"`: an autopilot that drives the visible window with pauses, so a
//!   person can watch a journey on their real data: `wait:<ms>`, `page:<code>`, `tab:<kind>`,
//!   `select:<kind>`, `variant:<n>`, `begin`, `confirm`, `restore`, `close`, `snapshot`, `diff`,
//!   `assert_same`, `log:<text>`, `quit`. Results are printed as `AUTOPILOT ...` lines.
//! - `TANKI_SIZE=<w>x<h>`: the window's inner size in points.
//! - `TANKI_SHOT=<file.png>`: save the app's own frame to the file and exit, so the UI can be
//!   inspected without screen-recording permission.

use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use eframe::egui;
use sha2::{Digest, Sha256};

use crate::{
    app::{Dialog, Page, Selection, Studio},
    model::{Filter, grade_index},
};

pub fn run_actions(app: &mut Studio) {
    if let Ok(page) = std::env::var("TANKI_VIEW") {
        app.page = Page::from_code(&page);
    }
    if let Some(lang) = std::env::var("TANKI_LANG")
        .ok()
        .and_then(|code| tanki_i18n::Lang::from_code(&code))
    {
        app.lang = lang;
    }
    if let Ok(theme) = std::env::var("TANKI_THEME") {
        app.theme_pref = tanki_ui::theme::ThemePreference::from_pref(Some(&theme));
    }
    // A comma-separated list stands in for the real process list (empty: none).
    if let Ok(list) = std::env::var("TANKI_GAME_PROCESSES") {
        app.game_processes_override = Some(
            list.split(',')
                .filter(|n| !n.is_empty())
                .map(str::to_owned)
                .collect(),
        );
    }
    if let Ok(script) = std::env::var("TANKI_SCRIPT") {
        app.autopilot = Some(Autopilot::new(&script));
    }
    let Ok(script) = std::env::var("TANKI_ACTIONS") else {
        return;
    };
    for step in script.split(';').filter(|s| !s.is_empty()) {
        let (verb, arg) = step.split_once(':').unwrap_or((step, ""));
        match verb {
            "import" => {
                // The scene's next actions need the pack: import it at once, without the card.
                let background = std::mem::replace(&mut app.background_import, false);
                app.import_path(std::path::Path::new(arg));
                app.background_import = background;
            }
            "page" => app.page = Page::from_code(arg),
            "filter" => app.filter = Filter::from_code(arg),
            "select" => select(app, arg),
            "grade" => {
                if let (Some(selection), Ok(grade)) = (app.selection.as_mut(), arg.parse::<usize>())
                {
                    selection.grade = grade.min(3);
                }
            }
            "apply" | "begin" => {
                let mut parts = arg.split(':').map(|n| n.parse::<usize>().unwrap_or(0));
                let (item, variant) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
                app.begin_apply(item, variant);
                if verb == "apply"
                    && let Some(Dialog::Confirm(pending)) = app.dialog.clone()
                {
                    app.confirm_apply(pending);
                }
            }
            "confirm" => {
                if let Some(Dialog::Confirm(pending)) = app.dialog.clone() {
                    app.confirm_apply(pending);
                }
            }
            "pick" => {
                if let (Some(selection), Ok(index)) = (app.selection.clone(), arg.parse::<usize>())
                {
                    let options = app.pack_variants(&selection.kind, &selection.id);
                    if let Some(key) = options.get(index).and_then(|&(i, v)| app.variant_key(i, v))
                    {
                        app.remember_variant(&selection, &key);
                    }
                }
            }
            "restore" => app.restore_original(),
            "ask-restore" => app.ask_to_restore(),
            "cache-switcher" => {
                app.dialog = None;
                app.open_cache_switcher();
            }
            "dismiss" => app.toasts.clear(),
            "close" => app.dialog = None,
            "importing" => importing(app, arg),
            verb if verb.starts_with("bulk") || verb == "restore-batch" => {
                bulk_verb(app, verb, arg)
            }
            "update-offer" | "update-manual" | "updating" | "update-current" | "update-failed" => {
                update_scene(app, verb, arg)
            }
            "lang" => {
                if let Some(lang) = tanki_i18n::Lang::from_code(arg) {
                    app.set_language(lang);
                }
            }
            "theme" => {
                app.set_theme_preference(tanki_ui::theme::ThemePreference::from_pref(Some(arg)))
            }
            "zoom" => app.zoom(match arg {
                "+" => 1,
                "-" => -1,
                _ => 0,
            }),
            "palette" => {
                app.open_palette();
                if let Some(palette) = app.palette.as_mut() {
                    palette.query = arg.to_owned();
                }
            }
            "label" => {
                // label:<marker>:<kind>:<id>:<grade> (the marker contains no colon).
                let p: Vec<&str> = arg.split(':').collect();
                if let (Some(scan), [marker, kind, id, grade]) = (&app.scan, p.as_slice()) {
                    let game = scan.game.clone();
                    if let Err(error) = app.store.save_user_target(&game, marker, kind, id, grade) {
                        app.toast(format!("label failed: {error}"), None);
                    }
                    if let Some(scan) = app.scan.clone() {
                        app.set_scan(scan);
                        app.restore_cache_selection();
                    }
                }
            }
            other => app.toast(format!("unknown dev action: {other}"), None),
        }
    }
}

/// Whether this run is scripted (a screenshot, a scene, an autopilot): such a run never asks
/// GitHub for a release.
pub fn scripted() -> bool {
    ["TANKI_VIEW", "TANKI_SHOT", "TANKI_ACTIONS", "TANKI_SCRIPT"]
        .iter()
        .any(|v| std::env::var_os(v).is_some())
}

/// A made-up release for the update scenes, with notes like a real one's.
pub fn sample_release(version: &str) -> tanki_core::update::Release {
    use tanki_core::update::NoteLine;
    let version = tanki_core::update::version::parse(version)
        .unwrap_or_else(|| tanki_core::update::version::Version::new(0, 3, 0));
    tanki_core::update::Release {
        tag: format!("v{version}"),
        page: format!("{}/tag/v{version}", tanki_core::update::RELEASES_PAGE),
        notes: vec![
            (
                "en".into(),
                vec![
                    NoteLine::Heading("Added".into()),
                    NoteLine::Item(
                        "Tanki Skin Studio offers a new release when it starts, and updates itself: the new version is downloaded, checked against the release's checksums, installed, and opened.".into(),
                    ),
                    NoteLine::Item("Check for updates in the command palette.".into()),
                    NoteLine::Heading("Fixed".into()),
                    NoteLine::Item(
                        "A pack dropped on the window while a dialog is open waits for it.".into(),
                    ),
                ],
            ),
            (
                "ru".into(),
                vec![
                    NoteLine::Heading("Добавлено".into()),
                    NoteLine::Item(
                        "Tanki Skin Studio предлагает новый релиз при запуске и обновляется сам: новая версия скачивается, сверяется с контрольными суммами релиза, устанавливается и открывается.".into(),
                    ),
                    NoteLine::Item("Проверка обновлений в палитре команд.".into()),
                    NoteLine::Heading("Исправлено".into()),
                    NoteLine::Item(
                        "Пак, брошенный на окно при открытом диалоге, ждёт его закрытия.".into(),
                    ),
                ],
            ),
        ],
        assets: Vec::new(),
        version,
    }
}

fn update_scene(app: &mut Studio, verb: &str, arg: &str) {
    use crate::updates::UpdateView;
    use tanki_core::update::{
        UpdateProgress, UpdateStage,
        install::{Install, Manual},
    };
    let (version, percent) = arg.split_once(':').unwrap_or((arg, "40"));
    let release = sample_release(version);
    app.updates.install = if verb == "update-manual" {
        Install::Manual(Manual::Portable)
    } else {
        Install::MacApp(PathBuf::from("/Applications/Tanki Skin Studio.app"))
    };
    app.updates.view = Some(match verb {
        "updating" => {
            let total = 21_153_371;
            let percent = percent.parse::<u64>().unwrap_or(40).min(99);
            UpdateView::Installing {
                release,
                progress: std::sync::Arc::new(UpdateProgress::at(
                    UpdateStage::Downloading,
                    total * percent / 100,
                    total,
                )),
            }
        }
        "update-current" => UpdateView::UpToDate,
        "update-failed" => UpdateView::Failed(
            "could not reach GitHub (io: failed to lookup address information)".into(),
        ),
        _ => UpdateView::Offer {
            release,
            error: None,
        },
    });
}

/// `select:hull` picks the first recognised hull; `select:/0/0/10/12/` the group with that marker.
fn select(app: &mut Studio, arg: &str) {
    let found = app.rows.iter().find(|row| {
        if arg.starts_with('/') {
            row.key.contains(arg)
        } else {
            row.target.as_ref().is_some_and(|t| t.0 == arg)
        }
    });
    let Some(row) = found.cloned() else {
        return;
    };
    match row.target {
        Some((kind, id, grade)) => {
            app.selection = Some(Selection {
                kind: kind.clone(),
                id,
                grade: grade_index(&grade).unwrap_or(3),
            });
            app.garage_tab = kind;
        }
        None => app.selected_resource = Some(row.key),
    }
}

/// With `TANKI_SHOT=<file.png>` save this frame once the UI has settled, then close the window.
/// Animations are turned off for it: a dialog fading in would otherwise be caught half visible
/// when frames are fast, and waiting for wall-clock time hangs when the window is covered and
/// stops being drawn.
pub fn screenshot(ctx: &egui::Context) {
    let Ok(path) = std::env::var("TANKI_SHOT") else {
        return;
    };
    ctx.all_styles_mut(|style| style.animation_time = 0.0);
    ctx.request_repaint();
    if ctx.cumulative_pass_nr() == 40 {
        let close = ctx.clone();
        ctx.request_screenshot(move |image| {
            let [w, h] = image.size;
            if let Some(buffer) =
                image::RgbaImage::from_raw(w as u32, h as u32, image.as_raw().to_vec())
            {
                let _ = buffer.save(path);
            }
            close.send_viewport_cmd(egui::ViewportCommand::Close);
        });
    }
}

/// File name -> SHA-256 of every file under a folder.
type Tree = BTreeMap<String, String>;

fn tree(root: &Path) -> Tree {
    fn walk(dir: &Path, root: &Path, out: &mut Tree) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let name = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .into_owned();
                out.insert(name, format!("{:x}", Sha256::digest(bytes)));
            }
        }
    }
    let mut out = Tree::new();
    walk(root, root, &mut out);
    out
}

/// A scripted run of the visible window (see `TANKI_SCRIPT`).
pub struct Autopilot {
    steps: VecDeque<String>,
    resume_at: Instant,
    snapshot: Option<Tree>,
    root: Option<PathBuf>,
    pub failures: u32,
}

impl Autopilot {
    pub fn new(script: &str) -> Self {
        Self {
            steps: script
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect(),
            resume_at: Instant::now() + Duration::from_millis(1500),
            snapshot: None,
            root: None,
            failures: 0,
        }
    }
}

fn say(text: &str) {
    println!("AUTOPILOT {text}");
}

fn differing(before: &Tree, after: &Tree) -> Vec<String> {
    let mut names: Vec<String> = after
        .iter()
        .filter(|(name, hash)| before.get(*name) != Some(*hash))
        .map(|(name, _)| name.clone())
        .chain(before.keys().filter(|n| !after.contains_key(*n)).cloned())
        .collect();
    names.sort();
    names
}

/// Run the next step of the autopilot when its pause is over.
pub fn autopilot_tick(app: &mut Studio, ctx: &egui::Context) {
    let Some(mut pilot) = app.autopilot.take() else {
        return;
    };
    if !app.splash_done {
        // The script starts when the start-up screen is gone.
        pilot.resume_at = Instant::now() + Duration::from_millis(1500);
    }
    if app.import_job.is_some() {
        // A journey's next step needs the pack the running import brings.
        pilot.resume_at = pilot
            .resume_at
            .max(Instant::now() + Duration::from_millis(100));
    }
    if app.splash_done && Instant::now() >= pilot.resume_at {
        match pilot.steps.pop_front() {
            Some(step) => {
                run_step(app, &mut pilot, ctx, &step);
            }
            None => {
                say(&format!("DONE failures={}", pilot.failures));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
    }
    ctx.request_repaint_after(Duration::from_millis(50));
    app.autopilot = Some(pilot);
}

fn run_step(app: &mut Studio, pilot: &mut Autopilot, ctx: &egui::Context, step: &str) {
    let (verb, arg) = step.split_once(':').unwrap_or((step, ""));
    let pause = |pilot: &mut Autopilot, ms: u64| {
        pilot.resume_at = Instant::now() + Duration::from_millis(ms)
    };
    pause(pilot, 400);
    match verb {
        "wait" => pause(pilot, arg.parse().unwrap_or(1000)),
        "log" => say(arg),
        "page" => {
            say(&format!("page {arg}"));
            app.page = Page::from_code(arg);
        }
        "tab" => app.garage_tab = arg.to_owned(),
        "select" => {
            select(app, arg);
            say(&format!(
                "selected {:?}",
                app.selection
                    .as_ref()
                    .map(|s| format!("{}:{}:m{}", s.kind, s.id, s.grade))
            ));
        }
        "items" => {
            let list: Vec<String> = app
                .items
                .iter()
                .map(|i| {
                    format!(
                        "{}:{}[{}]",
                        i.kind,
                        i.id,
                        i.present
                            .iter()
                            .map(|p| if *p { '#' } else { '.' })
                            .collect::<String>()
                    )
                })
                .collect();
            say(&format!(
                "garage items ({}): {}",
                list.len(),
                list.join(" ")
            ));
        }
        "item" => {
            // item:<kind>:<id>:<grade 0-3>
            let p: Vec<&str> = arg.split(':').collect();
            if let [kind, id, grade] = p[..] {
                let exists = app.items.iter().any(|i| i.kind == kind && i.id == id);
                if exists {
                    app.open_in_garage(kind, id, grade.parse().unwrap_or(3));
                    say(&format!("item {kind}:{id}:m{grade}"));
                } else {
                    say(&format!("FAIL item {kind}:{id} is not in the garage"));
                    pilot.failures += 1;
                }
            }
        }
        "variant" => {
            let index: usize = arg.parse().unwrap_or(0);
            if let Some(selection) = app.selection.clone() {
                let options = app.pack_variants(&selection.kind, &selection.id);
                match options.get(index).and_then(|(i, v)| {
                    let (_, variant) = app.pack_variant(*i, *v)?;
                    Some((app.variant_key(*i, *v)?, variant.name.clone()))
                }) {
                    Some((id, name)) => {
                        app.remember_variant(&selection, &id);
                        say(&format!("variant {index}: {name}"));
                    }
                    None => {
                        say(&format!("FAIL no variant {index} for {}", selection.key()));
                        pilot.failures += 1;
                    }
                }
            }
        }
        "begin" => {
            app.begin_apply_selected();
            say(&format!("dialog after begin: {}", dialog_name(app)));
        }
        "confirm" => {
            if let Some(Dialog::Confirm(pending)) = app.dialog.clone() {
                say(&format!(
                    "applying {} ({} files)",
                    pending.target,
                    pending.files.len()
                ));
                app.confirm_apply(pending);
                say(&format!("dialog after apply: {}", dialog_name(app)));
            } else {
                say(&format!(
                    "FAIL expected the confirmation, got {}",
                    dialog_name(app)
                ));
                pilot.failures += 1;
            }
        }
        "restore" => {
            app.restore_original();
            say("restored last replacement");
        }
        "close" => app.dialog = None,
        "snapshot" => {
            let root = app.scan.as_ref().map(|s| s.root.clone());
            if let Some(root) = root {
                let started = Instant::now();
                pilot.snapshot = Some(tree(&root));
                say(&format!(
                    "snapshot of {} files in {:?}",
                    pilot.snapshot.as_ref().map_or(0, BTreeMap::len),
                    started.elapsed()
                ));
                pilot.root = Some(root);
            }
        }
        "diff" | "assert_same" => {
            let (Some(before), Some(root)) = (&pilot.snapshot, &pilot.root) else {
                say("FAIL no snapshot taken");
                pilot.failures += 1;
                return;
            };
            let changed = differing(before, &tree(root));
            if verb == "diff" {
                say(&format!(
                    "cache differs from the snapshot in {} file(s)",
                    changed.len()
                ));
            } else if changed.is_empty() {
                say("PASS cache is identical to the snapshot");
            } else {
                say(&format!(
                    "FAIL cache differs in {} file(s): {:?}",
                    changed.len(),
                    &changed[..changed.len().min(5)]
                ));
                pilot.failures += 1;
            }
        }
        "quit" => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
        other => {
            say(&format!("FAIL unknown step {other}"));
            pilot.failures += 1;
        }
    }
}

fn dialog_name(app: &Studio) -> &'static str {
    match &app.dialog {
        None => "none",
        Some(Dialog::Confirm(_)) => "confirm",
        Some(Dialog::GameRunning(..)) => "game running",
        Some(Dialog::Done { .. }) => "done",
        Some(Dialog::ImportError { .. }) => "import error",
        Some(Dialog::DeletePack { .. }) => "delete pack",
        Some(Dialog::RestoreAll { .. }) => "restore all",
        Some(Dialog::ChooseCache { .. }) => "choose cache",
        Some(Dialog::BulkGameRunning(_)) => "game running (all items)",
        Some(Dialog::CloseBulk) => "close apply to all",
        Some(Dialog::RestoreBatch { .. }) => "restore batch",
    }
}

/// The verbs of "Apply to all items" (see the module's documentation).
fn bulk_verb(app: &mut Studio, verb: &str, arg: &str) {
    use crate::bulk::{Pick, Source};
    let number = |text: &str| text.parse::<u8>().ok().filter(|n| *n < 4);
    let parts: Vec<&str> = arg.split(':').collect();
    match (verb, parts.as_slice()) {
        ("bulk", _) => {
            app.dialog = None;
            app.open_bulk();
        }
        ("bulk-look", [n]) => app.bulk_select_look(n.parse().unwrap_or(0)),
        ("bulk-next", _) => app.bulk_next(),
        ("bulk-back", _) => app.bulk_back(),
        ("bulk-off", [k]) => {
            if let Some(k) = number(k) {
                app.bulk_set_on(k, false);
            }
        }
        ("bulk-source", [k, source]) => {
            if let Some(k) = number(k) {
                let source = number(source).map_or(Source::Own, Source::From);
                app.bulk_set_source(k, source);
            }
        }
        ("bulk-row", [kind, id]) => app.bulk_toggle_row(kind, id),
        ("bulk-pick", [kind, id, k, pick]) => {
            if let Some(k) = number(k) {
                let pick = match *pick {
                    "common" => None,
                    "off" => Some(Pick::Off),
                    "edition" => Some(Pick::Skin(None)),
                    source => Some(Pick::Skin(number(source))),
                };
                app.bulk_set_pick(kind, id, k, pick);
            }
        }
        ("bulk-kind", [kind, on]) => app.bulk_set_kind(kind, *on == "on"),
        ("bulk-apply", _) => {
            let background = std::mem::replace(&mut app.background_bulk, false);
            app.bulk_apply();
            app.background_bulk = background;
        }
        ("bulk-running", [done]) => bulk_running(app, done.parse().unwrap_or(0)),
        ("bulk-undo", _) => app.bulk_undo(),
        ("restore-batch", _) => {
            let newest = app
                .backups
                .iter()
                .find(|b| !b.restored)
                .and_then(|b| b.tx.batch.clone());
            if let Some(batch) = newest {
                app.restore_batch_confirmed(&batch.id, &batch.name);
            }
        }
        _ => app.toast(format!("unknown dev action: {verb}:{arg}"), None),
    }
}

/// `bulk-running:<done>`: the run of the plan at the last step, frozen with `done` items applied
/// and the next one being written, for the screenshot. Nothing is written.
fn bulk_running(app: &mut Studio, done: usize) {
    use crate::app::{BulkRun, BulkStep, ItemState};
    let Some(wizard) = app.bulk.as_mut() else {
        return;
    };
    let (Some(look), Some(plan)) = (wizard.chosen(), wizard.plan.as_ref()) else {
        return;
    };
    let tasks = crate::bulk::tasks(plan);
    let states = tasks
        .iter()
        .enumerate()
        .map(|(index, task)| match index.cmp(&done) {
            std::cmp::Ordering::Less => ItemState::Applied {
                skins: task.writes.len(),
            },
            std::cmp::Ordering::Equal => {
                ItemState::Writing(task.writes.first().map_or(0, |w| w.modification))
            }
            std::cmp::Ordering::Greater => ItemState::Waiting,
        })
        .collect();
    let (sender, receiver) = std::sync::mpsc::channel();
    // The answer never comes: the run stays where it is for the screenshot.
    std::mem::forget(sender);
    wizard.run = Some(BulkRun {
        look: look.name.clone(),
        batch: tanki_core::apply::Batch {
            id: "0".into(),
            name: look.name.clone(),
        },
        tasks,
        states,
        skipped: Vec::new(),
        skipped_items: plan.skipped_items(),
        stop: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        receiver: Some(receiver),
        finished: None,
    });
    wizard.step = BulkStep::Running;
}

/// `importing:<name>:<percent>`: the card of an import frozen while copying, for the screenshot.
fn importing(app: &mut Studio, arg: &str) {
    let (name, percent) = arg.rsplit_once(':').unwrap_or((arg, "45"));
    let percent = percent.parse::<u64>().unwrap_or(45).min(99);
    let progress =
        tanki_core::pack::ImportProgress::at(tanki_core::pack::ImportStage::Copying, percent, 100);
    // Started long enough ago that about 20 seconds are left.
    let elapsed = 20.0 * percent as f32 / (100 - percent).max(1) as f32;
    let (sender, receiver) = std::sync::mpsc::channel();
    // The answer never comes: the card stays for the screenshot.
    std::mem::forget(sender);
    app.import_job = Some(crate::app::ImportJob {
        name: name.to_owned(),
        archive: false,
        progress: std::sync::Arc::new(progress),
        started: Instant::now() - Duration::from_secs_f32(elapsed),
        receiver,
    });
}
