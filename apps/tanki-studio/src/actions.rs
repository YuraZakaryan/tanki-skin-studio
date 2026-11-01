// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Everything the user can do. The buttons, the command palette, the shortcuts and the dev hooks
//! all call these methods, so there is one implementation of each action.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use tanki_core::{
    apply::{
        ApplyNote, Batch, SKIN_FILES, apply_variant_in, missing_files, missing_from,
        pending_restores, planned_files, restore_all, restore_batch,
    },
    archive,
    cache::{client_id, known_target_group, looks_like_cache, scan_cache},
    pack::{ImportProgress, InstalledPack, PackRejected, PackTarget, import_pack_with},
    process::running_game_processes,
    recognize::recognise_by_content,
};
use tanki_i18n::{Lang, fmt, tl};
use tanki_ui::theme::ThemePreference;

use crate::{
    app::{
        BulkRun, BulkStep, BulkWizard, CacheChoice, Dialog, ImportJob, Imported, ItemState, Page,
        PendingApply, RowFilter, Selection, Studio, ToastAction,
    },
    bulk,
    model::{caches_to_ask, grade_code, grade_index, target_name},
};

impl Studio {
    pub fn go(&mut self, page: Page) {
        self.page = page;
    }

    pub fn set_language(&mut self, lang: Lang) {
        if self.lang == lang {
            return;
        }
        self.lang = lang;
        if let Err(error) = self.store.set_language(lang.code()) {
            log::warn!("could not save the language: {error}");
        }
    }

    pub fn set_theme_preference(&mut self, pref: ThemePreference) {
        if self.theme_pref == pref {
            return;
        }
        self.theme_pref = pref;
        if let Err(error) = self.store.set_theme(pref.code()) {
            log::warn!("could not save the theme: {error}");
        }
    }

    /// Zoom the interface one preset up (`direction > 0`) or down, or back to 100% (`0`).
    pub fn zoom(&mut self, direction: i32) {
        let next = if direction == 0 {
            1.0
        } else {
            tanki_ui::scale::step(self.ui_scale, direction)
        };
        if (next - self.ui_scale).abs() < f32::EPSILON {
            return;
        }
        self.ui_scale = next;
        if let Err(error) = self.store.set_ui_scale(tanki_ui::scale::percent(next)) {
            log::warn!("could not save the interface size: {error}");
        }
    }

    /// Re-read the files of the current cache.
    pub fn rescan(&mut self) {
        self.reload_roots();
        let Some(scan) = &self.scan else {
            // Nothing was connected: look again for the client's folders.
            if let Some((game, path)) = self.roots.first().cloned() {
                self.open_cache(&game, &path);
            }
            self.check_caches();
            return;
        };
        let (game, root) = (scan.game.clone(), scan.root.clone());
        match scan_cache(&root, Some(&game)) {
            Ok(scan) => {
                self.set_scan(scan);
                self.restore_cache_selection();
                self.toast(tl!("Cache rescanned"), None);
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
        // A client installed since the start brings a cache of its own.
        self.check_caches();
    }

    pub fn choose_cache(&mut self) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if !self.file_dialogs {
            return;
        }
        let Some(path) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        self.use_cache_folder(&path);
    }

    /// Make a folder the current cache and remember it as the custom one. A folder that holds no
    /// cache entries is not taken for one: a model archive becomes a pack instead, and anything else
    /// is refused with an explanation, so the real cache stays connected.
    pub fn use_cache_folder(&mut self, path: &Path) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if !looks_like_cache(path) {
            if archive::is_model_archive(path) {
                self.import_path(path);
                self.go(Page::Packs);
            } else {
                self.toast(
                    tl!("That folder is not a game cache: it has no cache files."),
                    None,
                );
            }
            return;
        }
        match scan_cache(path, None) {
            Ok(scan) => {
                let _ = self.store.set("custom_cache", &scan.root.to_string_lossy());
                // A folder the user picked is never news to them.
                self.remember_caches(vec![cache_key(&scan.root)]);
                self.set_scan(scan);
                self.reload_roots();
                self.restore_cache_selection();
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    pub fn switch_cache(&mut self, index: usize) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if let Some((game, path)) = self.roots.get(index).cloned() {
            self.persist(true);
            self.open_cache(&game, &path);
            // The cache chosen is the one the next start opens.
            self.persist(true);
        }
    }

    /// Ask which cache to use when several were found and one of them is new to the user (or
    /// this is the first start that looked; see [`caches_to_ask`]). With nothing to ask, the
    /// caches found are remembered, so one that appears later is asked about. Another open dialog
    /// is not replaced: the question waits for the next look.
    pub fn check_caches(&mut self) {
        let found: Vec<String> = self.roots.iter().map(|(_, p)| cache_key(p)).collect();
        let known = self.known_caches();
        let Some(new) = caches_to_ask(&found, known.as_deref()) else {
            self.remember_caches(found);
            return;
        };
        if self.dialog.is_some() {
            return;
        }
        let choices = self.cache_choices(&new);
        let selected = choices.iter().position(|c| c.current).unwrap_or(0);
        self.dialog = Some(Dialog::ChooseCache {
            choices,
            selected,
            switching: false,
        });
    }

    /// The cache card was clicked: the same list of caches, to switch on purpose, with a way to
    /// choose a folder that is not listed. The caches are looked for again first.
    pub fn open_cache_switcher(&mut self) {
        if self.dialog.is_some() {
            return;
        }
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        self.reload_roots();
        let choices = self.cache_choices(&vec![false; self.roots.len()]);
        let selected = choices.iter().position(|c| c.current).unwrap_or(0);
        self.dialog = Some(Dialog::ChooseCache {
            choices,
            selected,
            switching: true,
        });
    }

    /// The switcher's Choose folder…: the folder chooser instead of the list.
    pub fn choose_cache_folder_instead(&mut self) {
        self.dialog = None;
        self.choose_cache();
    }

    /// The cache folders found, as the question lists them; `new` marks the ones new to the user.
    fn cache_choices(&self, new: &[bool]) -> Vec<CacheChoice> {
        let current = self.scan.as_ref().map(|s| cache_key(&s.root));
        self.roots
            .iter()
            .enumerate()
            .map(|(root, (game, path))| CacheChoice {
                root,
                path: path.clone(),
                name: if game == "custom" {
                    crate::views::shell::game_name(game)
                } else {
                    cache_name(path)
                },
                modified: last_change(path),
                new: new.get(root).copied().unwrap_or(false),
                current: current.as_deref() == Some(cache_key(path).as_str()),
            })
            .collect()
    }

    /// The answer to [`Studio::check_caches`]: work with `choice`'s cache from now on, this start
    /// and the next ones.
    pub fn pick_cache(&mut self, choice: &CacheChoice) {
        self.answer_cache_question();
        if !choice.current {
            self.switch_cache(choice.root);
        }
    }

    /// The question which cache to use was closed: the current one stays, and the caches it
    /// offered are not asked about again.
    pub fn keep_cache(&mut self) {
        self.answer_cache_question();
        self.persist(true);
    }

    fn answer_cache_question(&mut self) {
        if let Some(Dialog::ChooseCache { choices, .. }) = self.dialog.take() {
            self.remember_caches(choices.iter().map(|c| cache_key(&c.path)).collect());
        }
    }

    /// The caches the user has been asked about or has chosen; `None` before the first look.
    fn known_caches(&self) -> Option<Vec<String>> {
        let stored = self.store.get(KNOWN_CACHES).ok().flatten()?;
        serde_json::from_str(&stored).ok()
    }

    fn remember_caches(&mut self, caches: Vec<String>) {
        let stored = self.known_caches();
        let mut known = stored.clone().unwrap_or_default();
        for cache in caches {
            if !known.contains(&cache) {
                known.push(cache);
            }
        }
        if stored.as_ref() == Some(&known) {
            return;
        }
        let saved = serde_json::to_string(&known)
            .map_err(anyhow::Error::from)
            .and_then(|json: String| self.store.set(KNOWN_CACHES, &json));
        if let Err(error) = saved {
            log::warn!("could not save the known caches: {error}");
        }
    }

    pub fn import(&mut self, folder: bool) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if !self.file_dialogs {
            return;
        }
        if self.import_job.is_some() {
            self.toast(
                tl!("A pack is being imported. Wait until it is done."),
                None,
            );
            return;
        }
        let source = if folder {
            rfd::FileDialog::new().pick_folder()
        } else {
            rfd::FileDialog::new()
                .add_filter("Tanki pack", &["tankipack", "zip"])
                .pick_file()
        };
        if let Some(path) = source {
            self.import_path(&path);
        }
    }

    /// Import a pack (a `.tankipack` zip or a folder) or build one from a model archive. The work
    /// runs on its own thread with a progress card, so a large pack never freezes the window;
    /// [`Studio::finish_import`] takes the answer. One import runs at a time.
    pub fn import_path(&mut self, path: &Path) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if self.import_job.is_some() {
            self.toast(
                tl!("A pack is being imported. Wait until it is done."),
                None,
            );
            return;
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let archive = archive::is_model_archive(path);
        let progress = Arc::new(ImportProgress::default());
        let (sender, receiver) = std::sync::mpsc::channel();
        let (source, data, work) = (path.to_path_buf(), self.data.clone(), progress.clone());
        let job = move || {
            let _ = sender.send(run_import(&source, &data, archive, &work));
        };
        if self.background_import {
            let spawned = std::thread::Builder::new()
                .name("tanki-import".into())
                .spawn(job);
            if let Err(error) = spawned {
                self.toast(crate::errors::user_message(&error), None);
                return;
            }
        } else {
            job();
        }
        self.import_job = Some(ImportJob {
            name,
            archive,
            progress,
            started: Instant::now(),
            receiver,
        });
        if !self.background_import {
            self.finish_import();
        }
    }

    /// Stop the running import at its next file; nothing of it stays in the library.
    pub fn cancel_import(&mut self) {
        if let Some(job) = &self.import_job {
            job.progress.cancel();
        }
    }

    /// Take the answer of the running import when it is there. Returns whether one is pending.
    pub fn finish_import(&mut self) -> bool {
        let Some(job) = &self.import_job else {
            return false;
        };
        let result = match job.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(anyhow::anyhow!("the import stopped unexpectedly"))
            }
        };
        let Some(job) = self.import_job.take() else {
            return false;
        };
        match result {
            Ok(Imported::Pack(pack)) => {
                let message = fmt(
                    tl!("Pack “{name}” imported"),
                    &[("name", &pack.manifest.name)],
                );
                self.add_imported(pack, message);
            }
            Ok(Imported::Archive(summary, pack)) => {
                let message = fmt(
                    tl!("Built a pack from the model archive: {items} · {variants}"),
                    &[
                        (
                            "items",
                            &tanki_i18n::tn(summary.items as u64, "{n} item", "{n} items"),
                        ),
                        (
                            "variants",
                            &tanki_i18n::tn(summary.variants as u64, "{n} variant", "{n} variants"),
                        ),
                    ],
                );
                self.add_imported(pack, message);
            }
            Err(_) if job.progress.is_cancelled() => {
                self.toast(tl!("Import cancelled. Nothing was added."), None);
            }
            Err(error) if job.archive => {
                self.toast(crate::errors::user_message(&error), None);
            }
            Err(error) => {
                let (issues, message) = match error.downcast::<PackRejected>() {
                    Ok(rejected) => (rejected.issues, None),
                    Err(other) => (Vec::new(), Some(format!("{other:#}"))),
                };
                self.dialog = Some(Dialog::ImportError {
                    name: job.name,
                    issues,
                    message,
                });
            }
        }
        false
    }

    /// Remember an imported pack, make it active and offer to undo the import.
    fn add_imported(&mut self, pack: InstalledPack, message: String) {
        if let Err(error) = self.store.save_pack(&pack) {
            self.toast(crate::errors::user_message(&error), None);
            return;
        }
        let action = ToastAction::UndoImport {
            id: pack.manifest.id.clone(),
            version: pack.manifest.version.clone(),
        };
        self.active_pack = Some(pack);
        self.packs = self.store.list_packs().unwrap_or_default();
        self.toast(message, Some(action));
        self.auto_recognise(true);
    }

    pub fn select_pack(&mut self, index: usize) {
        let Some(pack) = self.packs.get(index).cloned() else {
            return;
        };
        if self
            .store
            .set_active_pack(&pack.manifest.id, &pack.manifest.version)
            .is_ok()
        {
            self.active_pack = Some(pack);
        }
    }

    /// The library's Delete button: deleting removes the pack's files, so it is asked first. A
    /// double click on the bin then never reaches the card that moves into its place.
    pub fn ask_to_delete_pack(&mut self, id: &str, version: &str) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        let Some(pack) = self
            .packs
            .iter()
            .find(|p| p.manifest.id == id && p.manifest.version == version)
        else {
            return;
        };
        self.dialog = Some(Dialog::DeletePack {
            id: id.to_owned(),
            version: version.to_owned(),
            name: pack.manifest.name.clone(),
        });
    }

    pub fn delete_pack(&mut self, id: &str, version: &str) {
        if let Err(error) = self.store.delete_pack(id, version) {
            self.toast(crate::errors::user_message(&error), None);
            return;
        }
        self.packs = self.store.list_packs().unwrap_or_default();
        self.active_pack = self.store.active_pack().ok().flatten();
        self.toast(tl!("Pack removed"), None);
    }

    pub fn run_toast_action(&mut self, action: ToastAction) {
        match action {
            ToastAction::UndoImport { id, version } => self.delete_pack(&id, &version),
        }
    }

    /// Start replacing the files of a variant: check the game is closed, then ask to confirm.
    pub fn begin_apply(&mut self, item: usize, variant: usize) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        let Some((pack_item, pack_variant)) = self.pack_variant(item, variant) else {
            return;
        };
        // The files go to the modification selected in the cache, whichever modification the skin is
        // of, but only when the skin is of that item: a Railgun skin never goes onto a selected Wasp
        // (the palette and scripted runs name a pack item directly).
        let cache_target = match &self.selection {
            Some(s) if s.kind == pack_item.target.kind && s.id == pack_item.target.id => {
                PackTarget {
                    kind: s.kind.clone(),
                    id: s.id.clone(),
                    grade: grade_code(s.grade).to_owned(),
                }
            }
            _ => pack_item.target.clone(),
        };
        let target = target_name(&cache_target.id, &cache_target.grade);
        // A skin that does not bring every model and texture file of the item is never applied
        // (the Apply button is off for it; this covers the palette and scripted runs too).
        let missing = self
            .scan
            .as_ref()
            .map(|scan| missing_files(scan, &cache_target, pack_variant))
            .unwrap_or_default();
        if !missing.is_empty() {
            let message = fmt(
                tl!(
                    "Can't apply: this skin has no {files} in the pack. Every file except the preview is needed."
                ),
                &[("files", &missing.join(", "))],
            );
            self.toast(message, None);
            return;
        }
        let Some(pack) = &self.active_pack else {
            return;
        };
        let files = match planned_files(pack, pack_variant) {
            Ok(files) => files,
            Err(error) => {
                self.toast(crate::errors::user_message(&error), None);
                return;
            }
        };
        let pending = PendingApply {
            pack_id: pack.manifest.id.clone(),
            pack_version: pack.manifest.version.clone(),
            pack_name: pack.manifest.name.clone(),
            item,
            variant,
            target,
            cache_target,
            files,
        };
        self.ask_to_confirm(pending);
    }

    /// The confirmation, or the game-is-running warning while the game runs.
    pub fn ask_to_confirm(&mut self, pending: PendingApply) {
        let running = self.running_game();
        self.dialog = Some(if running.is_empty() {
            Dialog::Confirm(pending)
        } else {
            Dialog::GameRunning(pending, running)
        });
    }

    /// The game's processes running now (none when it is closed).
    fn running_game(&self) -> Vec<String> {
        self.game_processes_override.clone().unwrap_or_else(|| {
            self.scan
                .as_ref()
                .map(|scan| running_game_processes(&scan.root))
                .unwrap_or_default()
        })
    }

    /// Apply the variant of the current selection (the Apply buttons of the Garage and Info pages).
    pub fn begin_apply_selected(&mut self) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        if let Some((item, variant)) = self.chosen_variant(&selection) {
            self.begin_apply(item, variant);
        }
    }

    /// The user confirmed: write the files of the pack and variant the dialog named, if the game is
    /// still closed (it may have been started while the dialog was open).
    pub fn confirm_apply(&mut self, pending: PendingApply) {
        self.dialog = None;
        let running = self.running_game();
        if !running.is_empty() {
            self.dialog = Some(Dialog::GameRunning(pending, running));
            return;
        }
        let Some(scan) = self.scan.clone() else {
            return;
        };
        let pack = self
            .packs
            .iter()
            .find(|p| {
                p.manifest.id == pending.pack_id && p.manifest.version == pending.pack_version
            })
            .cloned();
        let Some((pack, variant, grade)) = pack.and_then(|pack| {
            let item = pack.manifest.items.get(pending.item)?;
            let variant = item.variants.get(pending.variant)?.clone();
            let grade = item.target.grade.clone();
            Some((pack, variant, grade))
        }) else {
            self.toast(
                fmt(
                    tl!("Pack “{name}” is no longer in the library. Nothing was replaced."),
                    &[("name", &pending.pack_name)],
                ),
                None,
            );
            return;
        };
        let note = ApplyNote {
            batch: None,
            skin_grade: Some(grade),
        };
        match apply_variant_in(
            &scan,
            &pack,
            &pending.cache_target,
            &variant,
            &self.data.join("backups"),
            &note,
        ) {
            Ok(tx) => {
                self.dialog = Some(Dialog::Done {
                    target: pending.target,
                    files: tx.changes.len(),
                });
                self.rescan_quietly();
                self.reload_backups();
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    fn rescan_quietly(&mut self) {
        if let Some(scan) = &self.scan {
            let (game, root) = (scan.game.clone(), scan.root.clone());
            if let Ok(scan) = scan_cache(&root, Some(&game)) {
                self.set_scan(scan);
            }
        }
    }

    /// Every Restore button and ⌘Z: say what will be put back and ask first.
    pub fn ask_to_restore(&mut self) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if !self.running_game().is_empty() {
            self.toast(
                tl!("Close the game before restoring the original files."),
                None,
            );
            return;
        }
        let pending = match pending_restores(&self.data.join("backups")) {
            Ok(pending) => pending,
            Err(error) => return self.toast(crate::errors::user_message(&error), None),
        };
        if pending.is_empty() {
            return self.toast(tl!("No backup to restore"), None);
        }
        let mut items: Vec<String> = Vec::new();
        for backup in &pending {
            let name = backup.tx.target.as_ref().map_or_else(
                || backup.tx.pack_id.clone(),
                |t| target_name(&t.id, &t.grade),
            );
            if !items.contains(&name) {
                items.push(name);
            }
        }
        self.dialog = Some(Dialog::RestoreAll {
            items,
            replacements: pending.len(),
            files: pending.iter().map(|b| b.tx.changes.len()).sum(),
        });
    }

    /// Put the cache back as it was before every replacement not restored yet (the confirmed
    /// dialog). Like applying, never while the game runs.
    pub fn restore_original(&mut self) {
        self.dialog = None;
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        if !self.running_game().is_empty() {
            self.toast(
                tl!("Close the game before restoring the original files."),
                None,
            );
            return;
        }
        match restore_all(&self.data.join("backups")) {
            Ok(summary) if summary.replacements == 0 => {
                self.toast(tl!("No backup to restore"), None);
            }
            Ok(summary) => {
                let message = fmt(
                    tl!("Restored {n} files. Restart the game."),
                    &[("n", &summary.files.to_string())],
                );
                self.toast(message, None);
                // The cache wears no skin any more: the picks made for it are reset too.
                if let Some(root) = self
                    .scan
                    .as_ref()
                    .map(|s| s.root.to_string_lossy().into_owned())
                {
                    self.session.cache_mut(&root).variants.clear();
                }
                self.rescan_quietly();
                self.reload_backups();
            }
            Err(error) => {
                self.toast(crate::errors::user_message(&error), None);
                self.reload_backups();
            }
        }
    }

    pub fn can_restore(&self) -> bool {
        self.backups.iter().any(|backup| !backup.restored)
    }

    /// Open an item in the Garage.
    pub fn open_in_garage(&mut self, kind: &str, id: &str, grade: usize) {
        self.garage_tab = kind.to_owned();
        self.selection = Some(Selection {
            kind: kind.to_owned(),
            id: id.to_owned(),
            grade,
        });
        self.label_resource = None;
        self.page = Page::Garage;
    }

    /// Open a catalog row in the Garage if it is a recognised item, else in the labelling form.
    pub fn open_row(&mut self, key: &str) {
        let Some(row) = self.rows.iter().find(|r| r.key == key).cloned() else {
            return;
        };
        match row.target {
            Some((kind, id, grade)) => {
                let grade = crate::model::grade_index(&grade).unwrap_or(3);
                self.open_in_garage(&kind, &id, grade);
            }
            None => self.open_label(key),
        }
    }

    pub fn open_label(&mut self, key: &str) {
        self.label_resource = Some(key.to_owned());
        self.label_id.clear();
        self.page = Page::Info;
    }

    /// Save the hand label of the resource being labelled (and, if asked, its whole item).
    pub fn save_label(&mut self) {
        let Some(key) = self.label_resource.clone() else {
            return;
        };
        let Some(scan) = self.scan.clone() else {
            return;
        };
        let id = self.label_id.trim().to_lowercase();
        let kind = ["hull", "turret", "paint"][self.label_kind.min(2)];
        let grade = grade_code(self.label_grade);
        let result = if self.label_whole {
            let known: Vec<String> = self.rows.iter().map(|r| r.key.clone()).collect();
            self.store
                .save_user_item(&scan.game, &key, kind, &id, grade, &|m| {
                    known.iter().any(|k| k == m)
                })
                .map(|labelled| labelled.len())
        } else {
            self.store
                .save_user_target(&scan.game, &key, kind, &id, grade)
                .map(|()| 1)
        };
        match result {
            Ok(count) => {
                // Labels change which groups are recognised: rebuild from the same files.
                self.set_scan(scan);
                self.label_resource = None;
                self.selection = Some(Selection {
                    kind: kind.to_owned(),
                    id,
                    grade: self.label_grade,
                });
                self.garage_tab = kind.to_owned();
                self.toast(
                    fmt(
                        tl!("Labelled {n} modifications"),
                        &[("n", &count.to_string())],
                    ),
                    None,
                );
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    /// Look for items in the cache by the content of the imported packs' files (see
    /// `tanki_apply::recognize`) and remember what is found as labels, so the Garage fills with every
    /// item the packs cover and no one labels anything by hand. The search decodes hundreds of images,
    /// so it runs on its own thread and [`Studio::finish_recognition`] applies the answer.
    pub fn auto_recognise(&mut self, announce: bool) {
        let Some(scan) = self.scan.clone() else {
            return;
        };
        if self.packs.is_empty() || self.recognition.is_some() {
            return;
        }
        let packs = self.packs.clone();
        self.recognition_announce |= announce;
        if self.background_recognition {
            self.recognition_root = Some(scan.root.clone());
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(recognise_by_content(&scan, &packs));
            });
            self.recognition = Some(receiver);
        } else {
            let found = recognise_by_content(&scan, &packs);
            self.apply_recognition(found);
        }
    }

    /// Take the answer of the background search when it is there. Returns whether one is pending.
    pub fn finish_recognition(&mut self) -> bool {
        let Some(receiver) = &self.recognition else {
            return false;
        };
        match receiver.try_recv() {
            Ok(found) => {
                self.recognition = None;
                let root = self.recognition_root.take();
                // The cache was switched while the search ran: its answer is for another folder.
                if self.scan.as_ref().map(|s| &s.root) == root.as_ref() {
                    self.apply_recognition(found);
                } else {
                    self.auto_recognise(false);
                }
                self.recognition.is_some()
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => true,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.recognition = None;
                false
            }
        }
    }

    fn apply_recognition(&mut self, found: Vec<tanki_core::recognize::Recognition>) -> usize {
        let announce = std::mem::take(&mut self.recognition_announce);
        let Some(scan) = self.scan.clone() else {
            return 0;
        };
        let saved = found
            .iter()
            .filter(|r| {
                self.store
                    .save_user_target(&scan.game, &r.marker, &r.kind, &r.id, &r.grade)
                    .is_ok()
            })
            .count();
        if saved > 0 {
            // The labels change which groups are recognised: rebuild the view from the same files.
            self.set_scan(scan);
        }
        if announce && saved > 0 {
            self.toast(
                fmt(
                    tl!("Recognised {n} from your packs"),
                    &[(
                        "n",
                        &tanki_i18n::tn(saved as u64, "{n} resource", "{n} resources"),
                    )],
                ),
                None,
            );
        }
        saved
    }

    pub fn open_palette(&mut self) {
        self.palette = Some(crate::views::palette::PaletteState::default());
    }

    /// Files dropped on the window are imported as packs, but not while a dialog is open: a pack
    /// imported under a confirmation would change the active pack it was asked about.
    pub fn handle_dropped(&mut self, paths: Vec<PathBuf>) {
        // An open dialog goes first; during an update a new import would be cut off by the restart.
        if self.dialog.is_some() || self.updates.view.is_some() || self.bulk.is_some() {
            return;
        }
        if let Some(path) = paths.into_iter().next() {
            self.import_path(&path);
            self.page = Page::Packs;
        }
    }
}

/// The import itself, off the window's thread: a model archive is built into a pack in a staging
/// folder first, then the pack is checked and copied into the library.
fn run_import(
    source: &Path,
    data: &Path,
    archive: bool,
    progress: &ImportProgress,
) -> anyhow::Result<Imported> {
    let library = data.join("packs");
    if !archive {
        return import_pack_with(source, &library, progress).map(Imported::Pack);
    }
    let staging = data.join(format!(".archive-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = archive::build_model_pack(source, &staging).and_then(|summary| {
        import_pack_with(&staging, &library, progress).map(|pack| Imported::Archive(summary, pack))
    });
    let _ = std::fs::remove_dir_all(&staging);
    result
}

/// The settings key of the caches the user has been asked about (a JSON list of paths).
const KNOWN_CACHES: &str = "known_caches";

/// A cache folder as remembered: its canonical path, so two spellings of one folder are one.
fn cache_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The name that tells this cache from the others on this machine: the AIR application id its
/// path holds, else the folder's own name (the parent's for a folder called `cache`).
pub(crate) fn cache_name(path: &Path) -> String {
    if let Some(id) = client_id(path) {
        return id;
    }
    let named = if path.file_name().is_some_and(|n| n == "cache") {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    named.file_name().map_or_else(
        || path.to_string_lossy().into_owned(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// When something in the cache folder last changed (Unix seconds): the newest of the folder and
/// the entries right in it. A client that is played writes there, so the cache in use is the
/// recent one. Looks at a few thousand entries at most.
fn last_change(path: &Path) -> Option<u64> {
    let modified = |meta: std::fs::Metadata| {
        meta.modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    };
    let own = std::fs::metadata(path).ok().and_then(modified);
    let entries = std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .take(4096)
        .filter_map(|e| e.metadata().ok().and_then(modified));
    own.into_iter().chain(entries).max()
}

/// What is said while "Apply to all items" writes and something else would touch the cache.
pub(crate) fn bulk_busy() -> &'static str {
    tl!("Skins are being put on all items. Wait until it is done.")
}

/// "Apply to all items" (see [`crate::bulk`]).
impl Studio {
    /// Whether a run of "Apply to all items" is writing now.
    pub fn bulk_running(&self) -> bool {
        self.bulk
            .as_ref()
            .and_then(|w| w.run.as_ref())
            .is_some_and(|run| run.receiver.is_some())
    }

    /// The packs in the app's order: the active pack first, then the library's.
    fn ordered_packs(&self) -> Vec<&InstalledPack> {
        let active = self
            .active_pack
            .as_ref()
            .map(|p| (p.manifest.id.as_str(), p.manifest.version.as_str()));
        let is_active = |p: &InstalledPack| {
            active == Some((p.manifest.id.as_str(), p.manifest.version.as_str()))
        };
        self.packs
            .iter()
            .filter(|p| is_active(p))
            .chain(self.packs.iter().filter(|p| !is_active(p)))
            .collect()
    }

    /// The Garage's "Apply to all items" and the palette command: the looks of every installed
    /// pack, the first one picked.
    pub fn open_bulk(&mut self) {
        if self.bulk.is_some() || self.dialog.is_some() {
            return;
        }
        self.palette = None;
        let looks = bulk::gather_looks(&self.ordered_packs(), &self.items);
        let mut cache_files: BTreeMap<(String, String, u8), (BTreeSet<String>, u64)> =
            BTreeMap::new();
        if let Some(scan) = &self.scan {
            for resource in &scan.resources {
                if !SKIN_FILES.contains(&resource.role.as_str()) {
                    continue;
                }
                let Some((kind, id, grade)) = known_target_group(&scan.game, &resource.group)
                else {
                    continue;
                };
                let Some(modification) = grade_index(grade) else {
                    continue;
                };
                let entry = cache_files
                    .entry((kind.to_owned(), id.to_owned(), modification as u8))
                    .or_default();
                entry.0.insert(resource.role.clone());
                entry.1 = entry.1.saturating_add(resource.size);
            }
        }
        self.bulk = Some(BulkWizard {
            step: BulkStep::Look,
            look: (!looks.is_empty()).then_some(0),
            looks,
            search: String::new(),
            settings: None,
            plan: None,
            filter: RowFilter::All,
            expanded: None,
            cache_files,
            run: None,
        });
    }

    /// Close the wizard (Cancel, ×, Esc, Done). Never while it writes; at the last step before
    /// writing it asks first ([`Studio::confirm_close_bulk`]).
    pub fn close_bulk(&mut self) {
        let Some(wizard) = &self.bulk else {
            return;
        };
        if self.bulk_running() {
            return;
        }
        if wizard.step == BulkStep::Check {
            self.dialog = Some(Dialog::CloseBulk);
            return;
        }
        self.bulk = None;
    }

    /// The answer to "Close without applying?".
    pub fn confirm_close_bulk(&mut self) {
        self.dialog = None;
        if !self.bulk_running() {
            self.bulk = None;
        }
    }

    pub fn bulk_select_look(&mut self, index: usize) {
        if let Some(wizard) = self.bulk.as_mut()
            && wizard.step == BulkStep::Look
            && index < wizard.looks.len()
        {
            wizard.look = Some(index);
        }
    }

    /// The step's primary button (and Enter): on to the next step, or start the run.
    pub fn bulk_next(&mut self) {
        let Some(wizard) = self.bulk.as_mut() else {
            return;
        };
        match wizard.step {
            BulkStep::Look => {
                let Some(look) = wizard.chosen() else {
                    return;
                };
                wizard.settings = Some(bulk::Settings::for_look(look));
                wizard.filter = RowFilter::All;
                wizard.expanded = None;
                wizard.step = BulkStep::Where;
                self.refresh_bulk_plan();
            }
            BulkStep::Where => {
                if wizard.plan.as_ref().is_some_and(|p| p.changes() > 0) {
                    wizard.step = BulkStep::Check;
                }
            }
            BulkStep::Check => self.bulk_apply(),
            BulkStep::Running => {}
            BulkStep::Done => self.bulk = None,
        }
    }

    pub fn bulk_back(&mut self) {
        if let Some(wizard) = self.bulk.as_mut() {
            wizard.step = match wizard.step {
                BulkStep::Where => BulkStep::Look,
                BulkStep::Check => BulkStep::Where,
                other => other,
            };
        }
    }

    /// Change the settings of the "Where" step and work the plan out again.
    fn bulk_settings(&mut self, change: impl FnOnce(&mut bulk::Settings)) {
        if let Some(settings) = self.bulk.as_mut().and_then(|w| w.settings.as_mut()) {
            change(settings);
            self.refresh_bulk_plan();
        }
    }

    /// A modification's switch.
    pub fn bulk_set_on(&mut self, modification: u8, on: bool) {
        self.bulk_settings(|s| {
            if let Some(choice) = s.choices.get_mut(usize::from(modification)) {
                choice.on = on;
            }
        });
    }

    /// A modification's "Put on" choice, for every item.
    pub fn bulk_set_source(&mut self, modification: u8, source: bulk::Source) {
        self.bulk_settings(|s| {
            if let Some(choice) = s.choices.get_mut(usize::from(modification)) {
                *choice = bulk::Choice { on: true, source };
            }
        });
    }

    /// One item's own choice for one modification; `None` goes back to the common choice.
    pub fn bulk_set_pick(
        &mut self,
        kind: &str,
        id: &str,
        modification: u8,
        pick: Option<bulk::Pick>,
    ) {
        let key = (kind.to_owned(), id.to_owned(), modification);
        self.bulk_settings(|s| match pick {
            Some(pick) => {
                s.overrides.insert(key, pick);
            }
            None => {
                s.overrides.remove(&key);
            }
        });
    }

    /// The Hulls and Turrets boxes: a kind left out is left out of the run.
    pub fn bulk_set_kind(&mut self, kind: &str, on: bool) {
        self.bulk_settings(|s| match kind {
            "hull" => s.hulls = on,
            "turret" => s.turrets = on,
            _ => {}
        });
    }

    pub fn bulk_set_filter(&mut self, filter: RowFilter) {
        if let Some(wizard) = self.bulk.as_mut() {
            wizard.filter = filter;
        }
    }

    /// Open (or close) an item's own choices in the table.
    pub fn bulk_toggle_row(&mut self, kind: &str, id: &str) {
        if let Some(wizard) = self.bulk.as_mut() {
            let key = (kind.to_owned(), id.to_owned());
            wizard.expanded = if wizard.expanded.as_ref() == Some(&key) {
                None
            } else {
                Some(key)
            };
        }
    }

    /// Work out what the look puts where with the current settings.
    pub fn refresh_bulk_plan(&mut self) {
        let Some(mut wizard) = self.bulk.take() else {
            return;
        };
        if let (Some(look), Some(settings)) = (wizard.chosen(), wizard.settings.as_ref()) {
            let packs = &self.packs;
            let files = &wizard.cache_files;
            let missing = |item: &bulk::LookItem, modification: u8, skin: &bulk::LookSkin| {
                let Some(variant) = packs
                    .iter()
                    .find(|p| {
                        p.manifest.id == skin.pack_id && p.manifest.version == skin.pack_version
                    })
                    .and_then(|p| p.manifest.items.get(skin.item))
                    .and_then(|i| i.variants.get(skin.variant))
                else {
                    return Vec::new();
                };
                files
                    .get(&(item.kind.clone(), item.id.clone(), modification))
                    .map(|(files, _)| missing_from(files, variant))
                    .unwrap_or_default()
            };
            let facts = bulk::CacheFacts {
                items: &self.items,
                applied: &self.applied,
                missing: &missing,
            };
            wizard.plan = Some(bulk::plan(look, settings, &facts));
        }
        self.bulk = Some(wizard);
    }

    /// The last step's Apply: the game must be closed, as for a single replacement.
    pub fn bulk_apply(&mut self) {
        if self.bulk.as_ref().is_none_or(|w| w.step != BulkStep::Check) {
            return;
        }
        let running = self.running_game();
        if running.is_empty() {
            self.start_bulk();
        } else {
            self.dialog = Some(Dialog::BulkGameRunning(running));
        }
    }

    /// The game-is-running warning's Check again.
    pub fn bulk_recheck(&mut self) {
        self.dialog = None;
        self.bulk_apply();
    }

    /// Start writing the plan, item by item, off the window's thread (see [`bulk::run`]).
    fn start_bulk(&mut self) {
        let Some(scan) = self.scan.clone() else {
            return;
        };
        let Some(wizard) = self.bulk.as_mut() else {
            return;
        };
        let (Some(look), Some(plan)) = (wizard.chosen(), wizard.plan.as_ref()) else {
            return;
        };
        let tasks = bulk::tasks(plan);
        if tasks.is_empty() {
            return;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let batch = Batch {
            id: stamp.to_string(),
            name: look.name.clone(),
        };
        let skipped = plan
            .rows
            .iter()
            .flat_map(|row| {
                row.cells
                    .iter()
                    .enumerate()
                    .filter_map(|(k, cell)| match cell {
                        Some(cell) if cell.skipped() => Some((row.name(), k as u8, cell.clone())),
                        _ => None,
                    })
            })
            .collect();
        let skipped_items = plan.skipped_items();
        let running: Box<dyn Fn() -> Vec<String> + Send> =
            match self.game_processes_override.clone() {
                Some(list) => Box::new(move || list.clone()),
                None => {
                    let root = scan.root.clone();
                    Box::new(move || running_game_processes(&root))
                }
            };
        let stop = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = std::sync::mpsc::channel();
        let work = bulk::Run {
            root: scan.root.clone(),
            game: scan.game.clone(),
            packs: self.packs.clone(),
            backups: self.data.join("backups"),
            batch: batch.clone(),
            tasks: tasks.clone(),
            running,
            gate: self.bulk_gate.take(),
        };
        let flag = stop.clone();
        let job = move || {
            bulk::run(work, &flag, &|event| {
                let _ = sender.send(event);
            });
        };
        wizard.run = Some(BulkRun {
            look: look.name.clone(),
            batch,
            states: vec![ItemState::Waiting; tasks.len()],
            tasks,
            skipped,
            skipped_items,
            stop,
            receiver: Some(receiver),
            finished: None,
        });
        wizard.step = BulkStep::Running;
        if self.background_bulk {
            let spawned = std::thread::Builder::new()
                .name("tanki-bulk".into())
                .spawn(job);
            if let Err(error) = spawned {
                if let Some(wizard) = self.bulk.as_mut() {
                    wizard.run = None;
                    wizard.step = BulkStep::Check;
                }
                self.toast(crate::errors::user_message(&error), None);
            }
        } else {
            job();
            self.finish_bulk();
        }
    }

    /// "Stop after this item": the item being written is finished, then the run stops.
    pub fn bulk_stop(&mut self) {
        if let Some(run) = self.bulk.as_ref().and_then(|w| w.run.as_ref()) {
            run.stop.store(true, Ordering::SeqCst);
        }
    }

    /// Take what the run reported since the last frame. Returns whether it still runs.
    pub fn finish_bulk(&mut self) -> bool {
        let Some(run) = self.bulk.as_mut().and_then(|w| w.run.as_mut()) else {
            return false;
        };
        let Some(receiver) = &run.receiver else {
            return false;
        };
        let mut finished = None;
        loop {
            match receiver.try_recv() {
                Ok(bulk::Event::Writing {
                    index,
                    modification,
                }) => {
                    if let Some(state) = run.states.get_mut(index) {
                        *state = ItemState::Writing(modification);
                    }
                }
                Ok(bulk::Event::Done { index, outcome }) => {
                    let state = match outcome {
                        bulk::Outcome::Applied { transactions } => ItemState::Applied {
                            skins: transactions.len(),
                        },
                        bulk::Outcome::Failed {
                            modification,
                            failure,
                            kept,
                        } => ItemState::Failed {
                            modification,
                            reason: failure_text(&failure, kept.as_deref()),
                        },
                    };
                    if let Some(slot) = run.states.get_mut(index) {
                        *slot = state;
                    }
                }
                Ok(bulk::Event::Finished { stopped, game }) => {
                    finished = Some((stopped, game));
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    finished = Some((true, Vec::new()));
                    break;
                }
            }
        }
        let Some(finished) = finished else {
            return true;
        };
        run.receiver = None;
        run.finished = Some(finished);
        for state in &mut run.states {
            if matches!(state, ItemState::Waiting | ItemState::Writing(_)) {
                *state = ItemState::NotRun;
            }
        }
        // The Garage shows what is on the items now: each applied skin is the item's pick.
        let picks: Vec<(String, String)> = run
            .tasks
            .iter()
            .zip(&run.states)
            .filter(|(_, state)| matches!(state, ItemState::Applied { .. }))
            .flat_map(|(task, _)| {
                task.writes.iter().map(|write| {
                    (
                        format!(
                            "{}/{}:{}:{}",
                            write.skin.pack_id,
                            task.kind,
                            task.id,
                            grade_code(usize::from(write.modification))
                        ),
                        format!("{}:{}", write.skin.grade, write.skin.variant_id),
                    )
                })
            })
            .collect();
        if let Some(wizard) = self.bulk.as_mut() {
            wizard.step = BulkStep::Done;
        }
        if let Some(root) = self
            .scan
            .as_ref()
            .map(|s| s.root.to_string_lossy().into_owned())
        {
            let entry = self.session.cache_mut(&root);
            for (key, value) in picks {
                entry.variants.insert(key, value);
            }
        }
        self.rescan_quietly();
        self.reload_backups();
        false
    }

    /// The result's Undo all: put back what every item of the run wore before it.
    pub fn bulk_undo(&mut self) {
        let Some(run) = self.bulk.as_ref().and_then(|w| w.run.as_ref()) else {
            return;
        };
        if run.receiver.is_some() {
            return;
        }
        let (batch, look) = (run.batch.id.clone(), run.look.clone());
        if self.undo_batch(&batch, &look) {
            self.bulk = None;
        }
    }

    /// A batch's entry in the backups: what it puts back, asked first.
    pub fn ask_to_restore_batch(&mut self, batch: &str) {
        if self.bulk_running() {
            return self.toast(bulk_busy(), None);
        }
        let entries: Vec<&tanki_core::apply::BackupEntry> = self
            .backups
            .iter()
            .filter(|b| !b.restored && b.tx.batch.as_ref().is_some_and(|t| t.id == batch))
            .collect();
        let Some(first) = entries.first() else {
            return;
        };
        let name = first
            .tx
            .batch
            .as_ref()
            .map(|b| b.name.clone())
            .unwrap_or_default();
        let mut items: Vec<String> = Vec::new();
        for entry in &entries {
            if let Some(target) = &entry.tx.target {
                let item = crate::model::target_title(&target.id);
                if !items.contains(&item) {
                    items.push(item);
                }
            }
        }
        self.dialog = Some(Dialog::RestoreBatch {
            batch: batch.to_owned(),
            name,
            items,
            files: entries.iter().map(|e| e.tx.changes.len()).sum(),
        });
    }

    /// The confirmed question of [`Studio::ask_to_restore_batch`].
    pub fn restore_batch_confirmed(&mut self, batch: &str, name: &str) {
        self.dialog = None;
        self.undo_batch(batch, name);
    }

    /// Undo a batch, never while the game runs. Returns whether it was undone.
    fn undo_batch(&mut self, batch: &str, name: &str) -> bool {
        if !self.running_game().is_empty() {
            self.toast(
                tl!("Close the game before restoring the original files."),
                None,
            );
            return false;
        }
        let backups = self.data.join("backups");
        // The picks the batch made go with it.
        let targets: Vec<String> = self
            .backups
            .iter()
            .filter(|b| b.tx.batch.as_ref().is_some_and(|t| t.id == batch))
            .filter_map(|b| b.tx.target.as_ref())
            .map(|t| format!("/{}:{}:{}", t.kind, t.id, t.grade))
            .collect();
        match restore_batch(&backups, batch) {
            Ok(summary) => {
                let message = fmt(
                    tl!("Undid “{look}”: {files} put back. Restart the game."),
                    &[
                        ("look", name),
                        (
                            "files",
                            &tanki_i18n::tn(summary.files as u64, "{n} file", "{n} files"),
                        ),
                    ],
                );
                self.toast(message, None);
                if let Some(root) = self
                    .scan
                    .as_ref()
                    .map(|s| s.root.to_string_lossy().into_owned())
                {
                    self.session
                        .cache_mut(&root)
                        .variants
                        .retain(|key, _| !targets.iter().any(|t| key.ends_with(t.as_str())));
                }
                self.rescan_quietly();
                self.reload_backups();
                true
            }
            Err(error) => {
                self.toast(crate::errors::user_message(&error), None);
                self.reload_backups();
                false
            }
        }
    }

    /// The empty state's Add a pack: the wizard closes and the import opens.
    pub fn bulk_add_pack(&mut self) {
        self.bulk = None;
        self.import(false);
        self.go(Page::Packs);
    }
}

/// Why an item of the run was not applied, in the interface's language.
fn failure_text(failure: &bulk::Failure, kept: Option<&str>) -> String {
    let text = match failure {
        bulk::Failure::PackGone(name) => fmt(
            tl!("the pack “{name}” is no longer in the library"),
            &[("name", name)],
        ),
        bulk::Failure::SkinGone => tl!("the pack no longer has this skin").to_owned(),
        bulk::Failure::Incomplete(files) => fmt(
            tl!("the skin has no {files}"),
            &[("files", &files.join(", "))],
        ),
        bulk::Failure::NotInCache => tl!("the item is no longer in the cache").to_owned(),
        bulk::Failure::Error(error) => crate::errors::user_message(error),
    };
    match kept {
        Some(error) => fmt(
            tl!("{reason}; its earlier modifications could not be put back ({error})"),
            &[("reason", text.trim_end_matches('.')), ("error", error)],
        ),
        None => text,
    }
}
