//! Everything the user can do. The buttons, the command palette, the shortcuts and the dev hooks
//! all call these methods, so there is one implementation of each action.

use std::path::{Path, PathBuf};

use tanki_core::{
    apply::{apply_variant, latest_transaction, missing_files, planned_files, restore_transaction},
    archive,
    cache::{looks_like_cache, scan_cache},
    pack::{PackRejected, PackTarget, import_pack},
    process::running_game_processes,
    recognize::recognise_by_content,
};
use tanki_i18n::{Lang, fmt, tl};
use tanki_ui::theme::ThemePreference;

use crate::{
    app::{Dialog, Page, PendingApply, Selection, Studio, ToastAction},
    model::{grade_code, target_name},
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
    }

    pub fn choose_cache(&mut self) {
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
                self.set_scan(scan);
                self.reload_roots();
                self.restore_cache_selection();
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    pub fn switch_cache(&mut self, index: usize) {
        if let Some((game, path)) = self.roots.get(index).cloned() {
            self.persist(true);
            self.open_cache(&game, &path);
        }
    }

    pub fn import(&mut self, folder: bool) {
        if !self.file_dialogs {
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

    /// Build a pack from a model archive folder and import it like any other pack.
    fn import_archive(&mut self, path: &Path) {
        let staging = self.data.join(format!(".archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        let built = archive::build_model_pack(path, &staging);
        let result = built.and_then(|summary| {
            import_pack(&staging, &self.data.join("packs")).map(|pack| (summary, pack))
        });
        let _ = std::fs::remove_dir_all(&staging);
        match result {
            Ok((summary, pack)) => {
                if let Err(error) = self.store.save_pack(&pack) {
                    self.toast(crate::errors::user_message(&error), None);
                    return;
                }
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
                let action = ToastAction::UndoImport {
                    id: pack.manifest.id.clone(),
                    version: pack.manifest.version.clone(),
                };
                self.active_pack = Some(pack);
                self.packs = self.store.list_packs().unwrap_or_default();
                self.toast(message, Some(action));
                self.auto_recognise(true);
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    pub fn import_path(&mut self, path: &Path) {
        if archive::is_model_archive(path) {
            return self.import_archive(path);
        }
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match import_pack(path, &self.data.join("packs")) {
            Ok(pack) => {
                if let Err(error) = self.store.save_pack(&pack) {
                    self.toast(crate::errors::user_message(&error), None);
                    return;
                }
                let action = ToastAction::UndoImport {
                    id: pack.manifest.id.clone(),
                    version: pack.manifest.version.clone(),
                };
                let message = fmt(
                    tl!("Pack “{name}” imported"),
                    &[("name", &pack.manifest.name)],
                );
                self.active_pack = Some(pack);
                self.packs = self.store.list_packs().unwrap_or_default();
                self.toast(message, Some(action));
                self.auto_recognise(true);
            }
            Err(error) => {
                let (issues, message) = match error.downcast::<PackRejected>() {
                    Ok(rejected) => (rejected.issues, None),
                    Err(other) => (Vec::new(), Some(format!("{other:#}"))),
                };
                self.dialog = Some(Dialog::ImportError {
                    name,
                    issues,
                    message,
                });
            }
        }
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
        let files = match self
            .active_pack
            .as_ref()
            .map(|pack| planned_files(pack, pack_variant))
        {
            Some(Ok(files)) => files,
            Some(Err(error)) => {
                self.toast(crate::errors::user_message(&error), None);
                return;
            }
            None => return,
        };
        let pending = PendingApply {
            item,
            variant,
            target,
            cache_target,
            files,
        };
        let cache_root = self.scan.as_ref().map(|s| s.root.clone());
        let running = self.game_processes_override.clone().unwrap_or_else(|| {
            cache_root
                .as_deref()
                .map(running_game_processes)
                .unwrap_or_default()
        });
        self.dialog = Some(if running.is_empty() {
            Dialog::Confirm(pending)
        } else {
            Dialog::GameRunning(pending, running)
        });
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

    /// The user confirmed: write the files.
    pub fn confirm_apply(&mut self, pending: PendingApply) {
        self.dialog = None;
        let Some(pack) = self.active_pack.clone() else {
            return;
        };
        let Some(scan) = self.scan.clone() else {
            return;
        };
        let Some(variant) = self
            .pack_variant(pending.item, pending.variant)
            .map(|(_, v)| v.clone())
        else {
            return;
        };
        match apply_variant(
            &scan,
            &pack,
            &pending.cache_target,
            &variant,
            &self.data.join("backups"),
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

    /// Put back the files of the most recent replacement.
    pub fn restore_last(&mut self) {
        match latest_transaction(&self.data.join("backups")) {
            Ok(Some((dir, tx))) => match restore_transaction(&dir, &tx) {
                Ok(()) => {
                    let message = fmt(
                        tl!("Restored {n} files. Restart the game."),
                        &[("n", &tx.changes.len().to_string())],
                    );
                    self.dialog = None;
                    self.toast(message, None);
                    self.rescan_quietly();
                    self.reload_backups();
                }
                Err(error) => self.toast(crate::errors::user_message(&error), None),
            },
            Ok(None) => self.toast(tl!("No backup to restore"), None),
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    pub fn can_restore(&self) -> bool {
        !self.backups.is_empty()
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

    /// Files dropped on the window are imported as packs.
    pub fn handle_dropped(&mut self, paths: Vec<PathBuf>) {
        if let Some(path) = paths.into_iter().next() {
            self.import_path(&path);
            self.page = Page::Packs;
        }
    }
}
