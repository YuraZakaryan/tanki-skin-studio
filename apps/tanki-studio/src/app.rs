// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The application state and its construction. Behaviour that changes state lives in
//! [`crate::actions`]; drawing lives in [`crate::views`].

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Result;
use eframe::egui::{self, TextureHandle};
use tanki_core::{
    apply::{AppliedSkin, BackupEntry},
    archive::ArchiveSummary,
    cache::{CacheScan, existing_cache_roots, looks_like_cache, scan_cache},
    pack::{ImportProgress, InstalledPack, PackIssue, PackItem, PackTarget, PackVariant},
    paths::app_data_dir,
    session::{SessionState, WindowGeometry},
    state::StateStore,
};
use tanki_i18n::Lang;
use tanki_ui::theme::{ThemeKind, ThemePreference};

use crate::model::{
    BaseItem, Filter, ResourceRow, base_items, build_rows, grade_code, grade_index,
};

/// The pages of the sidebar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Garage,
    Resources,
    Packs,
    Info,
}

impl Page {
    pub const ALL: [Page; 5] = [
        Page::Overview,
        Page::Garage,
        Page::Resources,
        Page::Packs,
        Page::Info,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::Garage => "garage",
            Page::Resources => "resources",
            Page::Packs => "packs",
            Page::Info => "info",
        }
    }

    pub fn from_code(code: &str) -> Page {
        Page::ALL
            .into_iter()
            .find(|p| p.code() == code)
            .unwrap_or(Page::Overview)
    }

    pub fn codes() -> Vec<&'static str> {
        Page::ALL.iter().map(|p| p.code()).collect()
    }
}

/// Garage categories (the kind of item).
pub const TABS: [&str; 3] = ["hull", "turret", "paint"];

/// The item and modification shown in the Garage and Info pages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection {
    pub kind: String,
    pub id: String,
    pub grade: usize,
}

impl Selection {
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.kind, self.id, grade_code(self.grade))
    }
}

/// A replacement waiting for the user's confirmation.
#[derive(Clone, Debug)]
pub struct PendingApply {
    /// The pack the dialog names. The active pack can change while the dialog is open (a pack
    /// dropped on the window, an Undo), so `item` and `variant` index this pack, never the active one.
    pub pack_id: String,
    pub pack_version: String,
    pub pack_name: String,
    pub item: usize,
    pub variant: usize,
    /// "Wasp M3": the cache modification that gets the files.
    pub target: String,
    /// Where in the cache the files go (the selected item, not the pack's own modification).
    pub cache_target: PackTarget,
    /// Cache files that will be overwritten, with replacement sizes.
    pub files: Vec<(String, u64)>,
}

/// A pack being imported on its own thread: a pack of hundreds of megabytes takes a while to
/// check and copy, and the window stays responsive meanwhile. See [`Studio::import_path`].
pub struct ImportJob {
    /// The file or folder's name, shown on the progress card.
    pub name: String,
    /// A model archive: a pack is built from it before the import.
    pub archive: bool,
    pub progress: std::sync::Arc<ImportProgress>,
    pub started: Instant,
    pub receiver: std::sync::mpsc::Receiver<Result<Imported>>,
}

/// What a finished import brings into the library.
pub enum Imported {
    Pack(InstalledPack),
    Archive(ArchiveSummary, InstalledPack),
}

/// A modal dialog.
#[derive(Clone, Debug)]
pub enum Dialog {
    Confirm(PendingApply),
    GameRunning(PendingApply, Vec<String>),
    Done {
        target: String,
        files: usize,
    },
    ImportError {
        name: String,
        issues: Vec<PackIssue>,
        message: Option<String>,
    },
    /// Restoring puts the cache back as it was before every replacement not restored yet.
    RestoreAll {
        /// The items that get their original files back, newest first.
        items: Vec<String>,
        replacements: usize,
        files: usize,
    },
    /// Deleting a pack removes its files from the library, so it is asked first.
    DeletePack {
        id: String,
        version: String,
        name: String,
    },
    /// "Apply to all items" found the game running: closing it and checking again starts the run.
    BulkGameRunning(Vec<String>),
    /// Leave "Apply to all items" at its last step, where everything is chosen: asked first.
    CloseBulk,
    /// Undo one run of "Apply to all items" (its entry in the backups): asked first.
    RestoreBatch {
        batch: String,
        name: String,
        /// The items that get back what they wore before the run.
        items: Vec<String>,
        files: usize,
    },
    /// Which game cache the app works with: asked when several were found (see
    /// [`Studio::check_caches`]), or opened from the cache card to switch on purpose
    /// (`switching`, see [`Studio::open_cache_switcher`]).
    ChooseCache {
        choices: Vec<CacheChoice>,
        selected: usize,
        switching: bool,
    },
}

/// The steps of "Apply to all items".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkStep {
    /// Pick a look (or, with none, the empty state).
    Look,
    /// Choose what each modification gets.
    Where,
    /// See every change before it is made.
    Check,
    /// The run, item by item.
    Running,
    /// What was done, with Undo all.
    Done,
}

/// Which items of the "Where" table are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowFilter {
    All,
    Change,
    Skipped,
}

/// What happened, or is happening, to one item of the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ItemState {
    Waiting,
    Writing(u8),
    Applied {
        skins: usize,
    },
    /// Not applied; nothing changed for the item. `reason` is said in the interface's language.
    Failed {
        modification: u8,
        reason: String,
    },
    /// The run stopped before it.
    NotRun,
}

/// A run of "Apply to all items" (see [`crate::bulk::run`]).
pub struct BulkRun {
    pub look: String,
    pub batch: tanki_core::apply::Batch,
    pub tasks: Vec<crate::bulk::Task>,
    pub states: Vec<ItemState>,
    /// Skipped modifications of the plan, for the result: (item, modification, why).
    pub skipped: Vec<(String, u8, crate::bulk::Cell)>,
    pub skipped_items: usize,
    pub stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The run's reports; `None` once it is over.
    pub receiver: Option<std::sync::mpsc::Receiver<crate::bulk::Event>>,
    /// Over: stopped (by the user, or because the game was started, which `game` names).
    pub finished: Option<(bool, Vec<String>)>,
}

impl BulkRun {
    pub fn applied(&self) -> usize {
        self.states
            .iter()
            .filter(|s| matches!(s, ItemState::Applied { .. }))
            .count()
    }

    pub fn skins(&self) -> usize {
        self.states
            .iter()
            .map(|s| match s {
                ItemState::Applied { skins } => *skins,
                _ => 0,
            })
            .sum()
    }

    /// Items done one way or another.
    pub fn done(&self) -> usize {
        self.states
            .iter()
            .filter(|s| !matches!(s, ItemState::Waiting | ItemState::Writing(_)))
            .count()
    }

    pub fn not_applied(&self) -> usize {
        self.states
            .iter()
            .filter(|s| matches!(s, ItemState::Failed { .. } | ItemState::NotRun))
            .count()
    }
}

/// "Apply to all items": one look on every hull and turret that has it.
pub struct BulkWizard {
    pub step: BulkStep,
    /// The looks of the installed packs, gathered when the wizard opened.
    pub looks: Vec<crate::bulk::Look>,
    pub look: Option<usize>,
    pub search: String,
    pub settings: Option<crate::bulk::Settings>,
    pub plan: Option<crate::bulk::Plan>,
    pub filter: RowFilter,
    /// The item whose own choices are open in the table: (kind, id).
    pub expanded: Option<(String, String)>,
    /// The model and texture files each item's modification has in the cache, and their size.
    pub cache_files:
        std::collections::BTreeMap<(String, String, u8), (std::collections::BTreeSet<String>, u64)>,
    pub run: Option<BulkRun>,
}

impl BulkWizard {
    pub fn chosen(&self) -> Option<&crate::bulk::Look> {
        self.looks.get(self.look?)
    }
}

/// A cache folder offered when the user chooses which cache to use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheChoice {
    /// Index into [`Studio::roots`].
    pub root: usize,
    pub path: PathBuf,
    /// The AIR application id read from the folder's own path, else the folder's name: whatever
    /// tells the clients apart on this machine.
    pub name: String,
    /// When something in the folder last changed (Unix seconds).
    pub modified: Option<u64>,
    /// Not there the last time the app looked.
    pub new: bool,
    /// The cache open now.
    pub current: bool,
}

/// What a toast's action button does.
#[derive(Clone, Debug)]
pub enum ToastAction {
    /// Remove the pack that was just imported.
    UndoImport { id: String, version: String },
}

pub struct Toast {
    pub text: String,
    pub action: Option<ToastAction>,
    pub born: Instant,
}

pub struct Studio {
    pub store: StateStore,
    pub data: PathBuf,
    pub roots: Vec<(String, PathBuf)>,
    /// Fixed cache folders (tests); `None` looks for the game's own.
    pub roots_override: Option<Vec<(String, PathBuf)>>,
    pub scan: Option<CacheScan>,
    pub scanned_at: Instant,
    pub rows: Vec<ResourceRow>,
    pub items: Vec<BaseItem>,
    pub packs: Vec<InstalledPack>,
    pub active_pack: Option<InstalledPack>,
    pub backups: Vec<BackupEntry>,
    /// The skins in the cache now (see `tanki_apply::apply::applied_skins`).
    pub applied: Vec<AppliedSkin>,

    pub page: Page,
    pub garage_tab: String,
    pub filter: Filter,
    pub search: String,
    pub selection: Option<Selection>,
    pub selected_resource: Option<String>,
    /// An unrecognised resource opened for labelling in the Info page.
    pub label_resource: Option<String>,
    pub label_kind: usize,
    pub label_grade: usize,
    pub label_id: String,
    pub label_whole: bool,

    pub dialog: Option<Dialog>,
    /// Processes "found" when checking whether the game runs, instead of the real list (tests).
    pub game_processes_override: Option<Vec<String>>,
    /// Whether the folder and file choosers open (a native dialog would block an in-process test).
    pub file_dialogs: bool,
    /// A scripted run that drives the visible window step by step (`TANKI_SCRIPT`).
    pub autopilot: Option<crate::dev::Autopilot>,
    pub palette: Option<crate::views::palette::PaletteState>,
    pub toasts: Vec<Toast>,

    pub lang: Lang,
    /// What the user chose, and the theme it resolves to right now.
    pub theme_pref: ThemePreference,
    pub theme: ThemeKind,
    pub applied_theme: Option<ThemeKind>,
    /// Interface size factor (1.0 = 100%) and the one last handed to egui.
    pub ui_scale: f32,
    pub applied_scale: Option<f32>,

    pub textures: HashMap<String, Option<TextureHandle>>,

    pub session: SessionState,
    pub saved_session: SessionState,
    pub last_session_write: Instant,
    pub window: Option<WindowGeometry>,
    pub window_checked: bool,
    /// No window was saved yet: the first frames size it to the screen it opens on.
    pub first_start: bool,

    pub started: Instant,
    pub splash_done: bool,
    /// Items were looked for by content once since the start-up screen closed.
    pub recognised_on_start: bool,
    /// The search for items by content runs on its own thread (it decodes hundreds of images);
    /// its answer arrives here. Tests turn this off to get the answer at once.
    pub background_recognition: bool,
    pub recognition: Option<std::sync::mpsc::Receiver<Vec<tanki_core::recognize::Recognition>>>,
    /// The cache folder the pending search is for: an answer for another cache is dropped.
    pub recognition_root: Option<PathBuf>,
    pub recognition_announce: bool,
    /// Packs are imported on their own thread with a progress card. Tests turn this off to get
    /// the pack at once.
    pub background_import: bool,
    pub import_job: Option<ImportJob>,
    /// Elapsed splash time pinned by `TANKI_SPLASH=hold|done`, for screenshots.
    pub splash_freeze: Option<f32>,
    /// New releases: the check, the update dialog and the update itself (see [`crate::updates`]).
    pub updates: crate::updates::Updates,
    /// "Apply to all items", while it is open.
    pub bulk: Option<BulkWizard>,
    /// The run writes on its own thread; tests turn this off to get the result at once.
    pub background_bulk: bool,
    /// Tests: each item of the next run waits for a message here (see [`crate::bulk::Run::gate`]).
    pub bulk_gate: Option<std::sync::mpsc::Receiver<()>>,
}

/// Everything the app needs from outside to start: where its data lives, the state it was left
/// in, and (for tests) a fixed set of cache folders instead of looking for the game's own.
pub struct Startup {
    pub store: StateStore,
    pub session: SessionState,
    pub data: PathBuf,
    /// Cache folders to use instead of searching this machine; `None` searches.
    pub roots: Option<Vec<(String, PathBuf)>>,
    /// Open without the start-up screen.
    pub skip_splash: bool,
}

impl Startup {
    /// The real thing: the app's data folder and its stored state.
    pub fn from_disk() -> Result<Self> {
        let data = app_data_dir();
        let store = StateStore::open(&data.join("state.sqlite"))?;
        let mut session = store.session();
        session.sanitize(&Page::codes(), &TABS, &Filter::codes());
        Ok(Self {
            store,
            session,
            data,
            roots: None,
            skip_splash: false,
        })
    }
}

impl Studio {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: Startup) -> Result<Self> {
        let Startup {
            store,
            session,
            data,
            roots,
            skip_splash,
        } = startup;
        let lang = Lang::from_pref(store.language()?.as_deref());
        // Texts made while starting (a toast of a scripted import) are in the chosen language too.
        tanki_i18n::set_current(lang);
        let theme_pref = ThemePreference::from_pref(store.theme()?.as_deref());
        let theme = theme_pref.resolve(cc.egui_ctx.system_theme());
        let ui_scale = store
            .ui_scale()?
            .and_then(|v| tanki_ui::scale::parse(&v))
            .unwrap_or(1.0);
        tanki_ui::fonts::install(&cc.egui_ctx);
        tanki_ui::theme::apply(&cc.egui_ctx, theme);
        // A size given for screenshots and tests (`TANKI_SIZE`, `TANKI_SHOT`) is kept as it is.
        let check_at_start = store.get("update_check_at_start")?.as_deref() != Some("off");
        let first_start = session.window.is_none()
            && std::env::var_os("TANKI_SIZE").is_none()
            && std::env::var_os("TANKI_SHOT").is_none();
        let mut app = Self {
            store,
            data,
            roots: Vec::new(),
            roots_override: roots,
            scan: None,
            scanned_at: Instant::now(),
            rows: Vec::new(),
            items: Vec::new(),
            packs: Vec::new(),
            active_pack: None,
            backups: Vec::new(),
            applied: Vec::new(),
            page: Page::from_code(&session.page),
            garage_tab: session.garage_tab.clone(),
            filter: Filter::from_code(&session.resource_filter),
            search: String::new(),
            selection: None,
            selected_resource: None,
            label_resource: None,
            label_kind: 0,
            label_grade: 3,
            label_id: String::new(),
            label_whole: true,
            dialog: None,
            game_processes_override: None,
            file_dialogs: true,
            autopilot: None,
            palette: None,
            toasts: Vec::new(),
            lang,
            theme_pref,
            theme,
            applied_theme: Some(theme),
            ui_scale,
            applied_scale: None,
            textures: HashMap::new(),
            window: session.window,
            saved_session: session.clone(),
            session,
            last_session_write: Instant::now(),
            window_checked: false,
            first_start,
            started: Instant::now(),
            // Scripted runs skip the splash; TANKI_SPLASH=hold freezes it for screenshots.
            recognised_on_start: false,
            background_recognition: true,
            recognition: None,
            recognition_root: None,
            recognition_announce: false,
            background_import: true,
            import_job: None,
            splash_done: skip_splash
                || std::env::var("TANKI_SPLASH").map_or_else(
                    |_| {
                        ["TANKI_VIEW", "TANKI_SHOT", "TANKI_ACTIONS"]
                            .iter()
                            .any(|v| std::env::var_os(v).is_some())
                    },
                    |v| v == "off",
                ),
            splash_freeze: match std::env::var("TANKI_SPLASH").as_deref() {
                Ok("hold") => Some(1.58),
                Ok("done") => Some(2.5),
                _ => None,
            },
            updates: crate::updates::Updates::new(check_at_start),
            bulk: None,
            background_bulk: true,
            bulk_gate: None,
        };
        app.reload_roots();
        app.packs = app.store.list_packs().unwrap_or_default();
        app.active_pack = app.store.active_pack().ok().flatten();
        app.open_initial_cache();
        app.check_caches();
        app.reload_backups();
        app.report_finished_update();
        crate::dev::run_actions(&mut app);
        Ok(app)
    }

    /// Cache folders to offer: the ones found on this machine plus the one the user picked.
    pub fn reload_roots(&mut self) {
        self.roots = self
            .roots_override
            .clone()
            .unwrap_or_else(existing_cache_roots);
        if let Ok(Some(custom)) = self.store.get("custom_cache") {
            let path = PathBuf::from(custom);
            // A folder picked earlier that holds no cache entries (a pack or an archive chosen by
            // mistake) is not offered as a cache.
            if path.is_dir()
                && looks_like_cache(&path)
                && !self.roots.iter().any(|(_, p)| p == &path)
            {
                self.roots.push(("custom".into(), path));
            }
        }
    }

    /// Open the cache the user worked with last if it still exists, else the first one found.
    fn open_initial_cache(&mut self) {
        let wanted = self.session.last_cache.clone();
        let index = wanted
            .as_deref()
            .and_then(|root| {
                self.roots.iter().position(|(_, p)| {
                    p.canonicalize()
                        .ok()
                        .is_some_and(|c| c.to_string_lossy() == root)
                })
            })
            .or_else(|| (!self.roots.is_empty()).then_some(0));
        if let Some(index) = index {
            let (game, path) = self.roots[index].clone();
            self.open_cache(&game, &path);
        }
    }

    /// Scan `path` and make it the current cache; restores the selection saved for it.
    pub fn open_cache(&mut self, game: &str, path: &std::path::Path) {
        match scan_cache(path, Some(game)) {
            Ok(scan) => {
                self.set_scan(scan);
                self.restore_cache_selection();
            }
            Err(error) => self.toast(crate::errors::user_message(&error), None),
        }
    }

    pub fn set_scan(&mut self, scan: CacheScan) {
        self.rows = build_rows(&scan);
        self.items = base_items(&scan);
        self.scanned_at = Instant::now();
        self.session.last_cache = Some(scan.root.to_string_lossy().into_owned());
        self.scan = Some(scan);
        self.textures.retain(|key, _| !key.starts_with("cache:"));
    }

    /// Apply what was saved for the current cache, dropping anything that no longer exists.
    pub fn restore_cache_selection(&mut self) {
        self.selection = None;
        self.selected_resource = None;
        let Some(root) = self
            .scan
            .as_ref()
            .map(|s| s.root.to_string_lossy().into_owned())
        else {
            return;
        };
        let Some(saved) = self.session.cache(&root).cloned() else {
            self.selection = self.default_selection();
            return;
        };
        if let Some(item) = &saved.item
            && self
                .items
                .iter()
                .any(|i| i.kind == item.kind && i.id == item.id)
        {
            self.selection = Some(Selection {
                kind: item.kind.clone(),
                id: item.id.clone(),
                grade: saved.grade.as_deref().and_then(grade_index).unwrap_or(3),
            });
        }
        if self.selection.is_none() {
            self.selection = self.default_selection();
        }
        self.selected_resource = saved
            .resource
            .filter(|key| self.rows.iter().any(|r| &r.key == key));
    }

    /// The first recognised item, at its highest modification present.
    pub fn default_selection(&self) -> Option<Selection> {
        let item = self.items.first()?;
        let grade = (0..4).rev().find(|g| item.present[*g]).unwrap_or(3);
        Some(Selection {
            kind: item.kind.clone(),
            id: item.id.clone(),
            grade,
        })
    }

    pub fn reload_backups(&mut self) {
        let root = self.data.join("backups");
        self.backups = tanki_core::apply::list_backups(&root).unwrap_or_default();
        self.applied = tanki_core::apply::applied_skins(&root).unwrap_or_default();
    }

    /// The skin the selected cache modification wears now, whichever pack it came from.
    pub fn applied_skin(&self, selection: &Selection) -> Option<&AppliedSkin> {
        let grade = grade_code(selection.grade);
        self.applied.iter().find(|skin| {
            skin.target.kind == selection.kind
                && skin.target.id == selection.id
                && skin.target.grade == grade
        })
    }

    /// The name of the skin the selection wears now, for the Garage caption. A skin of another pack
    /// than the active one is named with its pack: packs share skin names ("Old"), and the active
    /// pack's skin of the same name is not the one in the game.
    pub fn in_game_name(&self, selection: &Selection) -> Option<String> {
        let skin = self.applied_skin(selection)?;
        if self
            .active_pack
            .as_ref()
            .is_some_and(|pack| pack.manifest.id == skin.pack_id)
        {
            return Some(skin.variant_name.clone());
        }
        let pack = self
            .packs
            .iter()
            .find(|p| p.manifest.id == skin.pack_id)
            .map_or_else(|| skin.pack_id.clone(), |p| p.manifest.name.clone());
        Some(format!("{} · {pack}", skin.variant_name))
    }

    /// The variant of the active pack that the selection wears now, if any. A variant id repeats
    /// in every modification of an item, so the one of the selected modification wins.
    pub fn applied_variant(&self, selection: &Selection) -> Option<(usize, usize)> {
        let skin = self.applied_skin(selection)?;
        let pack = self.active_pack.as_ref()?;
        if pack.manifest.id != skin.pack_id {
            return None;
        }
        let grade = grade_code(selection.grade);
        self.pack_variants(&selection.kind, &selection.id)
            .into_iter()
            .filter(|&(i, v)| {
                self.pack_variant(i, v)
                    .is_some_and(|(_, variant)| variant.id == skin.variant_id)
            })
            .min_by_key(|&(i, _)| {
                self.pack_variant(i, 0)
                    .is_none_or(|(item, _)| item.target.grade != grade)
            })
    }

    /// Variants of the active pack for the item `kind`/`id`, as (item index, variant index). Every
    /// modification of the item counts, so any M0..M3 skin can be put on any cache modification.
    /// Ordered as `model::variant_order` says: M0..M3, then the special editions, each by name.
    pub fn pack_variants(&self, kind: &str, id: &str) -> Vec<(usize, usize)> {
        let Some(pack) = &self.active_pack else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (i, item) in pack.manifest.items.iter().enumerate() {
            if item.target.kind == kind && item.target.id == id {
                found.extend((0..item.variants.len()).map(|v| (i, v)));
            }
        }
        // A stable sort: variants that compare equal keep the pack's own order.
        found.sort_by(|&(ai, av), &(bi, bv)| {
            let (a, b) = (&pack.manifest.items[ai], &pack.manifest.items[bi]);
            crate::model::variant_order(
                (&a.target.grade, &a.variants[av].name),
                (&b.target.grade, &b.variants[bv].name),
            )
        });
        found
    }

    /// Saved identity of a variant: the same variant id repeats in every modification of an item.
    pub fn variant_key(&self, item: usize, variant: usize) -> Option<String> {
        let (item, variant) = self.pack_variant(item, variant)?;
        Some(format!("{}:{}", item.target.grade, variant.id))
    }

    pub fn pack_variant(&self, item: usize, variant: usize) -> Option<(&PackItem, &PackVariant)> {
        let item = self.active_pack.as_ref()?.manifest.items.get(item)?;
        Some((item, item.variants.get(variant)?))
    }

    /// The variant picked for the selection: the user's saved choice if it still exists, else the
    /// skin it wears now; none when nothing is picked or applied (Restore resets the choices).
    pub fn chosen_variant(&self, selection: &Selection) -> Option<(usize, usize)> {
        let options = self.pack_variants(&selection.kind, &selection.id);
        let root = self.scan.as_ref()?.root.to_string_lossy().into_owned();
        let saved = self
            .pick_key(selection)
            .and_then(|key| self.session.cache(&root)?.variants.get(&key).cloned());
        saved
            .and_then(|key| {
                options
                    .iter()
                    .copied()
                    .find(|(i, v)| self.variant_key(*i, *v).as_deref() == Some(key.as_str()))
            })
            .or_else(|| self.applied_variant(selection))
    }

    pub fn remember_variant(&mut self, selection: &Selection, variant_id: &str) {
        let root = self
            .scan
            .as_ref()
            .map(|s| s.root.to_string_lossy().into_owned());
        if let (Some(root), Some(key)) = (root, self.pick_key(selection)) {
            self.session
                .cache_mut(&root)
                .variants
                .insert(key, variant_id.to_owned());
        }
    }

    /// Where a pick is remembered: per pack, as a pick names a skin of the pack it was made in.
    /// Packs share variant ids ("old", "default"), so a pick saved per item alone showed another
    /// pack's skin of the same id as picked after a switch.
    fn pick_key(&self, selection: &Selection) -> Option<String> {
        let pack = self.active_pack.as_ref()?;
        Some(format!("{}/{}", pack.manifest.id, selection.key()))
    }

    pub fn toast(&mut self, text: impl Into<String>, action: Option<ToastAction>) {
        let text = text.into();
        // The same message again (a repeated click) refreshes it instead of stacking a copy.
        self.toasts.retain(|toast| toast.text != text);
        self.toasts.push(Toast {
            text,
            action,
            born: Instant::now(),
        });
        let overflow = self.toasts.len().saturating_sub(3);
        self.toasts.drain(..overflow);
    }

    /// Time since the cache was scanned.
    pub fn scan_age(&self) -> Duration {
        self.scanned_at.elapsed()
    }

    /// Put the current page, selection and filters into the session document.
    pub fn snapshot_session(&mut self) {
        self.session.page = self.page.code().to_owned();
        self.session.garage_tab = self.garage_tab.clone();
        self.session.resource_filter = self.filter.code().to_owned();
        self.session.window = self.window;
        let Some(root) = self
            .scan
            .as_ref()
            .map(|s| s.root.to_string_lossy().into_owned())
        else {
            return;
        };
        let selection = self.selection.clone();
        let resource = self.selected_resource.clone();
        let entry = self.session.cache_mut(&root);
        if let Some(selection) = selection {
            entry.item = Some(tanki_core::session::ItemRef {
                kind: selection.kind,
                id: selection.id,
            });
            entry.grade = Some(grade_code(selection.grade).to_owned());
        }
        entry.resource = resource;
    }

    /// Write the session if it changed (at most every half second unless `force`).
    pub fn persist(&mut self, force: bool) {
        self.snapshot_session();
        if self.session == self.saved_session {
            return;
        }
        if !force && self.last_session_write.elapsed() < Duration::from_millis(500) {
            return;
        }
        let mut clean = self.session.clone();
        clean.sanitize(&Page::codes(), &TABS, &Filter::codes());
        match self.store.save_session(&clean) {
            Ok(_) => {
                self.saved_session = self.session.clone();
                self.last_session_write = Instant::now();
            }
            Err(error) => log::warn!("could not save the session: {error}"),
        }
    }

    /// A texture decoded once from `load`'s bytes and kept under `key`.
    pub fn texture(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        load: impl FnOnce() -> Option<Vec<u8>>,
    ) -> Option<TextureHandle> {
        self.textures
            .entry(key.to_owned())
            .or_insert_with(|| {
                let bytes = load()?;
                let image = image::load_from_memory(&bytes).ok()?.to_rgba8();
                let size = [image.width() as usize, image.height() as usize];
                let color = egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw());
                Some(ctx.load_texture(key, color, egui::TextureOptions::LINEAR))
            })
            .clone()
    }
}
