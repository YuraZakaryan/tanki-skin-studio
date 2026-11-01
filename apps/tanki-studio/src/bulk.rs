// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! "Apply to all items": the looks the installed packs share across items, the plan of what one
//! look puts on which item and modification, and the run that writes it.
//!
//! A *look* is the skins of the same name for different items: the variant's name without the
//! item's own name in front ("Wasp Legacy" is the look "Legacy"), plus the pack's edition when
//! the pack item's grade names one (`xt-m0` is the look "XT"). Case, spaces, `-` and `_` do not
//! tell looks apart. A skin whose name is only the item's name, of a plain modification (`m3`,
//! `default`), is the item's plain skin and no look of its own; a pack that names it ("Classic")
//! makes it one. The `mN` part of the grade is the modification the skin was made for (its
//! *source*); an edition without one (`prime`) fits any modification.
//!
//! The model and the plan are plain data with unit tests; [`run`] does the writing off the
//! window's thread, one item at a time, through the transactions of `tanki-apply`.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use tanki_core::{
    apply::{
        AppliedSkin, ApplyNote, Batch, Transaction, apply_variant_in, missing_files,
        restore_transaction,
    },
    cache::scan_cache,
    pack::{InstalledPack, PackTarget},
};

use crate::model::{BaseItem, natural_cmp, target_title};

/// The kinds of item a look is put on: what the apply code replaces (not paints).
pub const KINDS: [&str; 2] = ["hull", "turret"];

/// One skin of a look for one item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookSkin {
    pub pack_id: String,
    pub pack_version: String,
    pub pack_name: String,
    /// Index of the pack item and of its variant in the pack's manifest.
    pub item: usize,
    pub variant: usize,
    pub variant_id: String,
    /// The pack item's grade: `xt-m0`, `m3`, `prime`.
    pub grade: String,
    /// The modification the skin was made for; `None` for an edition that fits any.
    pub source: Option<u8>,
}

/// The skins of a look for one item: at most one per source, from the first pack that has it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LookItem {
    pub kind: String,
    pub id: String,
    pub skins: Vec<LookSkin>,
}

impl LookItem {
    pub fn name(&self) -> String {
        target_title(&self.id)
    }

    /// The skin made for `source` exactly.
    pub fn skin(&self, source: Option<u8>) -> Option<&LookSkin> {
        self.skins.iter().find(|s| s.source == source)
    }

    /// The skin for `source`, else the edition's own skin that fits any modification.
    pub fn resolve(&self, source: u8) -> Option<&LookSkin> {
        self.skin(Some(source)).or_else(|| self.skin(None))
    }
}

/// Skins of the same name for several items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Look {
    /// The normalised name: what tells looks apart.
    pub key: String,
    /// The name as the first pack writes it.
    pub name: String,
    /// Hulls first, then turrets, each by name.
    pub items: Vec<LookItem>,
    /// The names of the packs its skins come from, in the app's pack order.
    pub packs: Vec<String>,
}

impl Look {
    pub fn count(&self, kind: &str) -> usize {
        self.items.iter().filter(|i| i.kind == kind).count()
    }

    /// Which modifications some item has a skin made for.
    pub fn sources(&self) -> [bool; 4] {
        let mut sources = [false; 4];
        for skin in self.items.iter().flat_map(|i| &i.skins) {
            if let Some(slot) = skin.source.and_then(|s| sources.get_mut(usize::from(s))) {
                *slot = true;
            }
        }
        sources
    }

    /// Whether some item has a skin that fits any modification.
    pub fn has_edition_skins(&self) -> bool {
        self.items
            .iter()
            .flat_map(|i| &i.skins)
            .any(|s| s.source.is_none())
    }

    pub fn item(&self, kind: &str, id: &str) -> Option<&LookItem> {
        self.items.iter().find(|i| i.kind == kind && i.id == id)
    }
}

/// Lower case, `-` and `_` as spaces, runs of spaces as one: "Desert_Camo " is "desert camo".
pub fn normalise(text: &str) -> String {
    text.to_lowercase()
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The modification of a grade (`xt-m2` → 2) and the words of its edition (`xt`); `default` is
/// no edition.
pub fn split_grade(grade: &str) -> (Option<u8>, Vec<String>) {
    let mut source = None;
    let mut edition = Vec::new();
    for word in normalise(grade).split(' ').filter(|w| !w.is_empty()) {
        let modification = match word {
            "m0" => Some(0),
            "m1" => Some(1),
            "m2" => Some(2),
            "m3" => Some(3),
            _ => None,
        };
        match modification {
            Some(m) if source.is_none() => source = Some(m),
            _ if word == "default" => {}
            _ => edition.push(word.to_owned()),
        }
    }
    (source, edition)
}

/// An edition as packs and players write it: `xt` and `alt` in capitals, others capitalised.
fn edition_word(word: &str) -> String {
    if matches!(word, "xt" | "alt") {
        return word.to_uppercase();
    }
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The look a variant belongs to: (key, name), or `None` for an item's plain skin. `item_id` is
/// the item the variant is for; its name in front of the variant's name is not part of the look.
pub fn look_of(variant_name: &str, item_id: &str, grade: &str) -> Option<(String, String)> {
    let words: Vec<&str> = variant_name
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect();
    let item = normalise(item_id);
    let item_words: Vec<&str> = item.split(' ').filter(|w| !w.is_empty()).collect();
    let starts_with_item = !item_words.is_empty()
        && words.len() >= item_words.len()
        && words
            .iter()
            .zip(&item_words)
            .all(|(word, item)| word.to_lowercase() == *item);
    let label: Vec<&str> = if starts_with_item {
        words.get(item_words.len()..).unwrap_or_default().to_vec()
    } else {
        words
    };
    let (_, edition) = split_grade(grade);
    let label_key: Vec<String> = label.iter().map(|w| w.to_lowercase()).collect();
    let mut name: Vec<String> = label.iter().map(|w| (*w).to_owned()).collect();
    // "Wasp XT" of an `xt-m3` item is XT, not XT XT.
    if !edition.iter().all(|w| label_key.contains(w)) {
        name.extend(edition.iter().map(|w| edition_word(w)));
    }
    if name.is_empty() {
        return None;
    }
    let name = name.join(" ");
    Some((normalise(&name), name))
}

/// Every look of `packs` (in the app's order: the active pack first) for the hulls and turrets
/// of `items` (the cache's). The same look for the same item and source in two packs: the first
/// pack's skin. Looks with the most items first, then by name.
pub fn gather_looks(packs: &[&InstalledPack], items: &[BaseItem]) -> Vec<Look> {
    let mut looks: Vec<Look> = Vec::new();
    for pack in packs {
        for (item_index, pack_item) in pack.manifest.items.iter().enumerate() {
            let target = &pack_item.target;
            if !KINDS.contains(&target.kind.as_str())
                || !items
                    .iter()
                    .any(|i| i.kind == target.kind && i.id == target.id)
            {
                continue;
            }
            let (source, _) = split_grade(&target.grade);
            for (variant_index, variant) in pack_item.variants.iter().enumerate() {
                let Some((key, name)) = look_of(&variant.name, &target.id, &target.grade) else {
                    continue;
                };
                let skin = LookSkin {
                    pack_id: pack.manifest.id.clone(),
                    pack_version: pack.manifest.version.clone(),
                    pack_name: pack.manifest.name.clone(),
                    item: item_index,
                    variant: variant_index,
                    variant_id: variant.id.clone(),
                    grade: target.grade.clone(),
                    source,
                };
                let Some(look) = entry(
                    &mut looks,
                    |l| l.key == key,
                    || Look {
                        key: key.clone(),
                        name: name.clone(),
                        items: Vec::new(),
                        packs: Vec::new(),
                    },
                ) else {
                    continue;
                };
                let Some(item) = entry(
                    &mut look.items,
                    |i| i.kind == target.kind && i.id == target.id,
                    || LookItem {
                        kind: target.kind.clone(),
                        id: target.id.clone(),
                        skins: Vec::new(),
                    },
                ) else {
                    continue;
                };
                if item.skin(source).is_none() {
                    item.skins.push(skin);
                    if !look.packs.contains(&pack.manifest.name) {
                        look.packs.push(pack.manifest.name.clone());
                    }
                }
            }
        }
    }
    for look in &mut looks {
        look.items.sort_by(|a, b| {
            let kind = |k: &str| KINDS.iter().position(|x| *x == k);
            kind(&a.kind)
                .cmp(&kind(&b.kind))
                .then_with(|| natural_cmp(&a.id, &b.id))
        });
        look.items
            .iter_mut()
            .for_each(|i| i.skins.sort_by_key(|s| s.source.map_or(4, u8::from)));
    }
    looks.sort_by(|a, b| {
        b.items
            .len()
            .cmp(&a.items.len())
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    looks
}

/// The element of `list` that `matches`, added with `make` when there is none.
fn entry<T>(
    list: &mut Vec<T>,
    matches: impl Fn(&T) -> bool,
    make: impl FnOnce() -> T,
) -> Option<&mut T> {
    if !list.iter().any(&matches) {
        list.push(make());
    }
    list.iter_mut().find(|x| matches(x))
}

/// Where a modification's skin comes from, for every item at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The skin made for this modification.
    Own,
    /// The skin made for another modification.
    From(u8),
}

/// What one modification (a column) gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice {
    pub on: bool,
    pub source: Source,
}

/// One item's own choice for one modification, in place of the common one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// The item's skin made for this source (`None`: the edition's skin).
    Skin(Option<u8>),
    /// Leave the modification as it is.
    Off,
}

/// Everything the user chose in the "Where" step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub choices: [Choice; 4],
    /// (kind, id, modification) → the item's own pick.
    pub overrides: BTreeMap<(String, String, u8), Pick>,
    pub hulls: bool,
    pub turrets: bool,
}

impl Settings {
    /// Every modification on; each takes its own skin when an item has one (or the edition's),
    /// else the skin of the highest modification some item has.
    pub fn for_look(look: &Look) -> Self {
        let sources = look.sources();
        let edition = look.has_edition_skins();
        let highest = (0..4u8).rev().find(|s| sources[usize::from(*s)]);
        let choices = std::array::from_fn(|k| {
            let source = if sources[k] || edition {
                Source::Own
            } else {
                highest.map_or(Source::Own, Source::From)
            };
            Choice { on: true, source }
        });
        Self {
            choices,
            overrides: BTreeMap::new(),
            hulls: true,
            turrets: true,
        }
    }

    pub fn includes(&self, kind: &str) -> bool {
        match kind {
            "hull" => self.hulls,
            "turret" => self.turrets,
            _ => false,
        }
    }

    pub fn pick(&self, kind: &str, id: &str, modification: u8) -> Option<Pick> {
        self.overrides
            .get(&(kind.to_owned(), id.to_owned(), modification))
            .copied()
    }
}

/// How a skin was chosen for a cell that changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum How {
    /// The skin made for this modification.
    Own,
    /// The skin of another modification, chosen for every item.
    From(u8),
    /// The edition's skin, which fits any modification.
    Edition,
    /// The item's own pick.
    Picked,
}

/// What happens to one item's modification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cell {
    Change {
        skin: LookSkin,
        how: How,
    },
    /// The packs have no skin of the look for the chosen source (`None`: no edition skin).
    Missing {
        source: Option<u8>,
    },
    /// The item has no such modification in the cache.
    NotInGame,
    /// Exactly this skin is on it already.
    Already,
    /// The skin does not bring every model and texture file the item has.
    Incomplete {
        files: Vec<String>,
    },
    /// Turned off: left as it is.
    Off,
}

impl Cell {
    pub fn changes(&self) -> bool {
        matches!(self, Cell::Change { .. })
    }

    /// Skipped for a reason (not turned off on purpose).
    pub fn skipped(&self) -> bool {
        !matches!(self, Cell::Change { .. } | Cell::Off)
    }
}

/// One item of the plan; `cells[k]` is `None` when no item of the look has M`k` in the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub kind: String,
    pub id: String,
    pub cells: [Option<Cell>; 4],
}

impl Row {
    pub fn name(&self) -> String {
        target_title(&self.id)
    }

    pub fn changes(&self) -> bool {
        self.cells.iter().flatten().any(Cell::changes)
    }

    pub fn skipped(&self) -> bool {
        self.cells.iter().flatten().any(Cell::skipped)
    }

    pub fn change_count(&self) -> usize {
        self.cells.iter().flatten().filter(|c| c.changes()).count()
    }
}

/// What the look puts where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    /// The modifications some item of the look has in the cache: the columns.
    pub columns: [bool; 4],
    /// The items of the kinds included.
    pub rows: Vec<Row>,
}

impl Plan {
    pub fn changing_items(&self) -> usize {
        self.rows.iter().filter(|r| r.changes()).count()
    }

    pub fn skipped_items(&self) -> usize {
        self.rows.iter().filter(|r| r.skipped()).count()
    }

    pub fn changes(&self) -> usize {
        self.rows.iter().map(Row::change_count).sum()
    }
}

/// What the plan needs to know of the cache.
pub struct CacheFacts<'a> {
    /// The cache's items (which modifications each has).
    pub items: &'a [BaseItem],
    /// The skins in the cache now.
    pub applied: &'a [AppliedSkin],
    /// The model and texture files a skin would not bring for the item's modification.
    pub missing: &'a dyn Fn(&LookItem, u8, &LookSkin) -> Vec<String>,
}

/// Whether `skin` is exactly what the item's modification wears now. A backup that did not record
/// the pack item's grade (an older one) cannot tell the `default` variant of one modification
/// from another's, so it counts only when the variant id is the item's only one of that id.
fn already_on(
    applied: &[AppliedSkin],
    kind: &str,
    id: &str,
    modification: u8,
    skin: &LookSkin,
    look: &LookItem,
) -> bool {
    let grade = format!("m{modification}");
    applied.iter().any(|worn| {
        worn.target.kind == kind
            && worn.target.id == id
            && worn.target.grade == grade
            && worn.pack_id == skin.pack_id
            && worn.variant_id == skin.variant_id
            && match &worn.skin_grade {
                Some(worn_grade) => *worn_grade == skin.grade,
                None => {
                    look.skins
                        .iter()
                        .filter(|s| s.pack_id == skin.pack_id && s.variant_id == skin.variant_id)
                        .count()
                        == 1
                }
            }
    })
}

/// The skin the common choice gives the item for `modification`, with how it was chosen, or the
/// source that is missing.
pub fn common_skin(
    item: &LookItem,
    choice: Choice,
    modification: u8,
) -> Result<(&LookSkin, How), Option<u8>> {
    let source = match choice.source {
        Source::Own => modification,
        Source::From(source) => source,
    };
    match item.resolve(source) {
        Some(skin) if skin.source.is_none() => Ok((skin, How::Edition)),
        Some(skin) => Ok((
            skin,
            match choice.source {
                Source::Own => How::Own,
                Source::From(source) => How::From(source),
            },
        )),
        None => Err(Some(source)),
    }
}

/// The plan of `look` with `settings` on the cache described by `facts`.
pub fn plan(look: &Look, settings: &Settings, facts: &CacheFacts<'_>) -> Plan {
    let present = |item: &LookItem| {
        facts
            .items
            .iter()
            .find(|i| i.kind == item.kind && i.id == item.id)
            .map_or([false; 4], |i| i.present)
    };
    let mut columns = [false; 4];
    for item in &look.items {
        for (k, here) in present(item).iter().enumerate() {
            columns[k] |= *here;
        }
    }
    let rows = look
        .items
        .iter()
        .filter(|item| settings.includes(&item.kind))
        .map(|item| {
            let here = present(item);
            let cells = std::array::from_fn(|k| {
                if !columns[k] {
                    return None;
                }
                let modification = k as u8;
                let pick = settings.pick(&item.kind, &item.id, modification);
                let choice = settings.choices[k];
                if pick == Some(Pick::Off) || (pick.is_none() && !choice.on) {
                    return Some(Cell::Off);
                }
                if !here[k] {
                    return Some(Cell::NotInGame);
                }
                let chosen = match pick {
                    Some(Pick::Skin(source)) => item
                        .skin(source)
                        .map(|skin| (skin, How::Picked))
                        .ok_or(source),
                    _ => common_skin(item, choice, modification),
                };
                let (skin, how) = match chosen {
                    Ok(found) => found,
                    Err(source) => return Some(Cell::Missing { source }),
                };
                if already_on(
                    facts.applied,
                    &item.kind,
                    &item.id,
                    modification,
                    skin,
                    item,
                ) {
                    return Some(Cell::Already);
                }
                let files = (facts.missing)(item, modification, skin);
                if !files.is_empty() {
                    return Some(Cell::Incomplete { files });
                }
                Some(Cell::Change {
                    skin: skin.clone(),
                    how,
                })
            });
            Row {
                kind: item.kind.clone(),
                id: item.id.clone(),
                cells,
            }
        })
        .collect();
    Plan { columns, rows }
}

/// How many of the included items the common choice of `modification` finds a skin for, of how
/// many: "9 of 14 items have it".
pub fn coverage(look: &Look, settings: &Settings, modification: u8) -> (usize, usize) {
    let choice = settings.choices[usize::from(modification.min(3))];
    let items: Vec<&LookItem> = look
        .items
        .iter()
        .filter(|item| settings.includes(&item.kind))
        .collect();
    let have = items
        .iter()
        .filter(|item| common_skin(item, choice, modification).is_ok())
        .count();
    (have, items.len())
}

/// The sources the "Put on" choice of `modification` offers: its own skin, then the skins of
/// the other modifications some item has.
pub fn source_options(look: &Look, modification: u8) -> Vec<Source> {
    let sources = look.sources();
    std::iter::once(Source::Own)
        .chain(
            (0..4u8)
                .rev()
                .filter(|s| *s != modification && sources[usize::from(*s)])
                .map(Source::From),
        )
        .collect()
}

/// One cache modification of one item to write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Write {
    pub modification: u8,
    pub skin: LookSkin,
}

/// One item of the run, with every modification it gets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub kind: String,
    pub id: String,
    pub writes: Vec<Write>,
}

impl Task {
    pub fn name(&self) -> String {
        target_title(&self.id)
    }
}

/// The items the plan changes, in its order.
pub fn tasks(plan: &Plan) -> Vec<Task> {
    plan.rows
        .iter()
        .filter(|row| row.changes())
        .map(|row| Task {
            kind: row.kind.clone(),
            id: row.id.clone(),
            writes: row
                .cells
                .iter()
                .enumerate()
                .filter_map(|(k, cell)| match cell {
                    Some(Cell::Change { skin, .. }) => Some(Write {
                        modification: k as u8,
                        skin: skin.clone(),
                    }),
                    _ => None,
                })
                .collect(),
        })
        .collect()
}

/// Why an item of the run was not applied. Said in the interface's language by the window (the
/// run's thread has no language of its own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The pack was removed from the library.
    PackGone(String),
    /// The pack no longer has the skin.
    SkinGone,
    /// The skin does not bring these files of the item.
    Incomplete(Vec<String>),
    /// The item is no longer in the cache.
    NotInCache,
    /// The core's own words (see `errors::user_message`).
    Error(String),
}

/// What happened to one item.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// Every modification was replaced.
    Applied { transactions: Vec<Transaction> },
    /// The write of `modification` failed and the item's earlier ones of this run were put back:
    /// nothing changed for it. `kept` is set when putting them back failed too.
    Failed {
        modification: u8,
        failure: Failure,
        kept: Option<String>,
    },
}

/// What the run reports to the window, in order.
#[derive(Clone, Debug)]
pub enum Event {
    /// The item `index` is being written, at `modification`.
    Writing {
        index: usize,
        modification: u8,
    },
    Done {
        index: usize,
        outcome: Outcome,
    },
    /// The run is over: every item done, or stopped after `done` items (the user asked, or the
    /// game was started: `game` names its processes).
    Finished {
        stopped: bool,
        game: Vec<String>,
    },
}

/// What the run needs; it owns all of it, so it can live on its own thread.
pub struct Run {
    pub root: PathBuf,
    pub game: String,
    pub packs: Vec<InstalledPack>,
    pub backups: PathBuf,
    pub batch: Batch,
    pub tasks: Vec<Task>,
    /// The game's processes running now (none when it is closed).
    pub running: Box<dyn Fn() -> Vec<String> + Send>,
    /// Tests: each item waits for a message here first (a closed channel stops the run).
    pub gate: Option<std::sync::mpsc::Receiver<()>>,
}

/// Write the run item by item: before each, stop if asked or if the game was started, read the
/// cache again (it may have changed) and check the pack still has the skin; then every
/// modification goes through a `tanki-apply` transaction (backup, write, verify, roll back on
/// failure). A failure puts the item's earlier modifications of this run back, so the item is
/// left as it was, and the run goes on with the next item.
pub fn run(work: Run, stop: &AtomicBool, send: &dyn Fn(Event)) {
    for (index, task) in work.tasks.iter().enumerate() {
        if let Some(gate) = &work.gate
            && gate.recv().is_err()
        {
            send(Event::Finished {
                stopped: true,
                game: Vec::new(),
            });
            return;
        }
        if stop.load(Ordering::SeqCst) {
            send(Event::Finished {
                stopped: true,
                game: Vec::new(),
            });
            return;
        }
        let game = (work.running)();
        if !game.is_empty() {
            send(Event::Finished {
                stopped: true,
                game,
            });
            return;
        }
        let outcome = apply_task(&work, index, task, send);
        send(Event::Done { index, outcome });
    }
    send(Event::Finished {
        stopped: false,
        game: Vec::new(),
    });
}

fn apply_task(work: &Run, index: usize, task: &Task, send: &dyn Fn(Event)) -> Outcome {
    let mut done: Vec<Transaction> = Vec::new();
    let fail = |modification: u8, failure: Failure, done: &mut Vec<Transaction>| {
        // Newest first, so each file gets back what it had before this run.
        let mut kept = None;
        for tx in done.iter().rev() {
            if let Err(error) = restore_transaction(&work.backups.join(&tx.id), tx) {
                kept = Some(format!("{error:#}"));
            }
        }
        Outcome::Failed {
            modification,
            failure,
            kept,
        }
    };
    let first = task.writes.first().map_or(0, |w| w.modification);
    let scan = match scan_cache(&work.root, Some(&work.game)) {
        Ok(scan) => scan,
        Err(error) => return fail(first, Failure::Error(format!("{error:#}")), &mut done),
    };
    for write in &task.writes {
        send(Event::Writing {
            index,
            modification: write.modification,
        });
        let skin = &write.skin;
        let Some(pack) = work
            .packs
            .iter()
            .find(|p| p.manifest.id == skin.pack_id && p.manifest.version == skin.pack_version)
            .filter(|p| p.path.exists())
        else {
            return fail(
                write.modification,
                Failure::PackGone(skin.pack_name.clone()),
                &mut done,
            );
        };
        let Some(variant) = pack
            .manifest
            .items
            .get(skin.item)
            .filter(|i| i.target.grade == skin.grade && i.target.id == task.id)
            .and_then(|i| i.variants.get(skin.variant))
            .filter(|v| v.id == skin.variant_id)
        else {
            return fail(write.modification, Failure::SkinGone, &mut done);
        };
        let target = PackTarget {
            kind: task.kind.clone(),
            id: task.id.clone(),
            grade: format!("m{}", write.modification),
        };
        if tanki_core::apply::cache_skin_files(&scan, &target).is_empty() {
            return fail(write.modification, Failure::NotInCache, &mut done);
        }
        let missing = missing_files(&scan, &target, variant);
        if !missing.is_empty() {
            return fail(write.modification, Failure::Incomplete(missing), &mut done);
        }
        let note = ApplyNote {
            batch: Some(work.batch.clone()),
            skin_grade: Some(skin.grade.clone()),
        };
        match apply_variant_in(&scan, pack, &target, variant, &work.backups, &note) {
            Ok(tx) => done.push(tx),
            Err(error) => {
                return fail(
                    write.modification,
                    Failure::Error(format!("{error:#}")),
                    &mut done,
                );
            }
        }
    }
    Outcome::Applied { transactions: done }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tanki_core::pack::{PackItem, PackManifest, PackVariant};

    fn variant(id: &str, name: &str) -> PackVariant {
        PackVariant {
            id: id.into(),
            name: name.into(),
            preview: None,
            resources: Vec::new(),
        }
    }

    /// (kind, item, grade, variants as (id, name)).
    type Items<'a> = [(&'a str, &'a str, &'a str, &'a [(&'a str, &'a str)])];

    fn pack(id: &str, items: &Items<'_>) -> InstalledPack {
        InstalledPack {
            path: PathBuf::from(id),
            manifest: PackManifest {
                format_version: 1,
                id: id.into(),
                name: format!("Pack {id}"),
                version: "1".into(),
                author: String::new(),
                license: String::new(),
                games: Vec::new(),
                items: items
                    .iter()
                    .map(|(kind, item, grade, variants)| PackItem {
                        target: PackTarget {
                            kind: (*kind).into(),
                            id: (*item).into(),
                            grade: (*grade).into(),
                        },
                        variants: variants
                            .iter()
                            .map(|(id, name)| variant(id, name))
                            .collect(),
                    })
                    .collect(),
            },
        }
    }

    fn item(kind: &str, id: &str, present: [bool; 4]) -> BaseItem {
        BaseItem {
            kind: kind.into(),
            id: id.into(),
            present,
            markers: [None, None, None, None],
        }
    }

    #[test]
    fn a_look_is_the_name_without_the_item_plus_the_edition() {
        assert_eq!(
            look_of("Wasp Legacy", "wasp", "m3"),
            Some(("legacy".into(), "Legacy".into()))
        );
        assert_eq!(
            look_of("Wasp", "wasp", "xt-m0"),
            Some(("xt".into(), "XT".into()))
        );
        assert_eq!(
            look_of("Desert Camo", "thunder", "m3"),
            Some(("desert camo".into(), "Desert Camo".into()))
        );
        assert_eq!(
            look_of("Desert_Camo", "smoky", "m1").map(|l| l.0),
            Some("desert camo".into())
        );
        assert_eq!(
            look_of("Viking", "viking", "prime"),
            Some(("prime".into(), "Prime".into()))
        );
        assert_eq!(
            look_of("Wasp Neon", "wasp", "alt-m2"),
            Some(("neon alt".into(), "Neon ALT".into()))
        );
        assert_eq!(
            look_of("Wasp XT", "wasp", "xt-m3"),
            Some(("xt".into(), "XT".into())),
            "no XT XT"
        );
        assert_eq!(
            look_of("WASP  legacy", "wasp", "m1").map(|l| l.0),
            Some("legacy".into())
        );
        // An item's plain skin is no look; a pack that names it makes it one.
        assert_eq!(look_of("Wasp", "wasp", "m3"), None);
        assert_eq!(look_of("Wasp", "wasp", "default"), None);
        assert_eq!(
            look_of("Classic", "wasp", "default"),
            Some(("classic".into(), "Classic".into()))
        );
        // Only the item's own name in front is dropped.
        assert_eq!(
            look_of("Waspish", "wasp", "m3").map(|l| l.1),
            Some("Waspish".into())
        );
        assert_eq!(look_of("", "wasp", "m3"), None);
    }

    #[test]
    fn a_grade_gives_the_source_modification_and_the_edition() {
        assert_eq!(split_grade("xt-m2"), (Some(2), vec!["xt".to_owned()]));
        assert_eq!(split_grade("m3"), (Some(3), Vec::new()));
        assert_eq!(split_grade("prime"), (None, vec!["prime".to_owned()]));
        assert_eq!(split_grade("default"), (None, Vec::new()));
        assert_eq!(split_grade("M1_ALT"), (Some(1), vec!["alt".to_owned()]));
    }

    fn looks_fixture() -> (Vec<InstalledPack>, Vec<BaseItem>) {
        let a = pack(
            "a",
            &[
                ("hull", "viking", "xt-m0", &[("default", "Viking")]),
                ("hull", "viking", "xt-m3", &[("default", "Viking")]),
                (
                    "hull",
                    "viking",
                    "m3",
                    &[("legacy", "Viking Legacy"), ("default", "Viking")],
                ),
                ("hull", "hunter", "xt-m3", &[("default", "Hunter")]),
                ("turret", "smoky", "xt-m3", &[("default", "Smoky")]),
                ("turret", "thunder", "prime", &[("default", "Thunder")]),
                ("paint", "red", "m0", &[("xt", "XT")]),
                ("hull", "titan", "xt-m3", &[("default", "Titan")]),
            ],
        );
        let b = pack(
            "b",
            &[
                ("turret", "smoky", "xt-m0", &[("default", "Smoky")]),
                ("hull", "viking", "xt-m3", &[("default", "Viking")]),
            ],
        );
        let items = vec![
            item("hull", "hunter", [false, true, true, true]),
            item("hull", "viking", [true; 4]),
            item("turret", "smoky", [true; 4]),
            item("turret", "thunder", [true; 4]),
        ];
        (vec![a, b], items)
    }

    #[test]
    fn looks_gather_the_same_name_across_items_grades_and_packs() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let names: Vec<&str> = looks.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            ["XT", "Legacy", "Prime"],
            "most items first; paints and plain skins are no look"
        );
        let xt = &looks[0];
        let ids: Vec<&str> = xt.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            ["hunter", "viking", "smoky"],
            "hulls first; an item not in the cache is left out"
        );
        assert_eq!(xt.packs, ["Pack a", "Pack b"]);
        let viking = xt.item("hull", "viking").expect("viking");
        assert_eq!(viking.skins.len(), 2, "one skin per source");
        assert_eq!(
            viking.skin(Some(3)).map(|s| s.pack_id.as_str()),
            Some("a"),
            "the first pack wins"
        );
        let smoky = xt.item("turret", "smoky").expect("smoky");
        assert_eq!(
            smoky.skin(Some(0)).map(|s| s.pack_id.as_str()),
            Some("b"),
            "another pack fills a source"
        );
        assert_eq!(xt.sources(), [true, false, false, true]);
        let prime = &looks[2];
        assert!(prime.has_edition_skins());
        assert_eq!(
            prime
                .item("turret", "thunder")
                .and_then(|i| i.resolve(2))
                .map(|s| s.source),
            Some(None),
            "an edition fits any modification"
        );
    }

    fn facts<'a>(
        items: &'a [BaseItem],
        applied: &'a [AppliedSkin],
        missing: &'a dyn Fn(&LookItem, u8, &LookSkin) -> Vec<String>,
    ) -> CacheFacts<'a> {
        CacheFacts {
            items,
            applied,
            missing,
        }
    }

    fn cell<'a>(plan: &'a Plan, id: &str, k: usize) -> &'a Option<Cell> {
        &plan.rows.iter().find(|r| r.id == id).expect("row").cells[k]
    }

    #[test]
    fn defaults_take_each_modifications_own_skin_else_the_highest() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let xt = &looks[0];
        let settings = Settings::for_look(xt);
        assert!(settings.choices.iter().all(|c| c.on));
        assert_eq!(settings.choices[0].source, Source::Own);
        assert_eq!(
            settings.choices[1].source,
            Source::From(3),
            "no item has its own M1"
        );
        assert_eq!(settings.choices[3].source, Source::Own);
        assert_eq!(
            source_options(xt, 1),
            [Source::Own, Source::From(3), Source::From(0)]
        );
        let prime = &looks[2];
        assert!(
            Settings::for_look(prime)
                .choices
                .iter()
                .all(|c| c.source == Source::Own),
            "an edition skin is every modification's own"
        );
    }

    #[test]
    fn every_cell_says_what_happens_and_why() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let xt = &looks[0];
        let applied = vec![AppliedSkin {
            target: PackTarget {
                kind: "hull".into(),
                id: "viking".into(),
                grade: "m3".into(),
            },
            pack_id: "a".into(),
            variant_id: "default".into(),
            variant_name: "Viking".into(),
            skin_grade: Some("xt-m3".into()),
            batch: None,
        }];
        let incomplete = |item: &LookItem, k: u8, _: &LookSkin| -> Vec<String> {
            if item.id == "smoky" && k == 2 {
                vec!["object.3ds".into()]
            } else {
                Vec::new()
            }
        };
        let mut settings = Settings::for_look(xt);
        settings.choices[1] = Choice {
            on: false,
            source: Source::Own,
        };
        let plan = plan(xt, &settings, &facts(&items, &applied, &incomplete));
        assert_eq!(plan.columns, [true; 4]);
        assert!(matches!(
            cell(&plan, "viking", 0),
            Some(Cell::Change { how: How::Own, .. })
        ));
        assert_eq!(cell(&plan, "viking", 1), &Some(Cell::Off), "turned off");
        assert!(
            matches!(
                cell(&plan, "viking", 2),
                Some(Cell::Change {
                    how: How::From(3),
                    ..
                })
            ),
            "no own M2: from M3"
        );
        assert_eq!(
            cell(&plan, "viking", 3),
            &Some(Cell::Already),
            "exactly this skin is on"
        );
        assert_eq!(
            cell(&plan, "hunter", 0),
            &Some(Cell::NotInGame),
            "no M0 in the cache"
        );
        assert_eq!(
            cell(&plan, "hunter", 2).as_ref().map(Cell::changes),
            Some(true)
        );
        assert_eq!(
            cell(&plan, "smoky", 2),
            &Some(Cell::Incomplete {
                files: vec!["object.3ds".into()]
            })
        );
        assert!(
            matches!(cell(&plan, "smoky", 0), Some(Cell::Change { skin, .. }) if skin.pack_id == "b")
        );

        // M0 from M3: the Hunter has no M0; the Smoky's own M0 is not used.
        settings.choices[0].source = Source::From(3);
        let from = plan_of(xt, &settings, &items, &applied, &incomplete);
        assert!(
            matches!(cell(&from, "smoky", 0), Some(Cell::Change { skin, how: How::From(3) }) if skin.pack_id == "a")
        );
        // Own M0: the Hunter's M0 is not in the game, a Viking skin made for M0 exists.
        settings.choices[0].source = Source::Own;
        settings.choices[2] = Choice {
            on: true,
            source: Source::Own,
        };
        let own = plan_of(xt, &settings, &items, &applied, &incomplete);
        assert_eq!(
            cell(&own, "viking", 2),
            &Some(Cell::Missing { source: Some(2) }),
            "no XT made for M2"
        );
        assert_eq!(coverage(xt, &settings, 2), (0, 3));
        assert_eq!(coverage(xt, &settings, 3), (3, 3));
        // Counting: items that change, items with something skipped.
        assert_eq!(own.changing_items(), 3);
        assert_eq!(own.skipped_items(), 3);
    }

    fn plan_of(
        look: &Look,
        settings: &Settings,
        items: &[BaseItem],
        applied: &[AppliedSkin],
        missing: &dyn Fn(&LookItem, u8, &LookSkin) -> Vec<String>,
    ) -> Plan {
        plan(look, settings, &facts(items, applied, missing))
    }

    #[test]
    fn an_items_own_pick_replaces_the_common_choice_for_it_only() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let xt = &looks[0];
        let none = |_: &LookItem, _: u8, _: &LookSkin| Vec::new();
        let mut settings = Settings::for_look(xt);
        settings
            .overrides
            .insert(("hull".into(), "viking".into(), 3), Pick::Skin(Some(0)));
        settings
            .overrides
            .insert(("turret".into(), "smoky".into(), 3), Pick::Off);
        let plan = plan_of(xt, &settings, &items, &[], &none);
        assert!(
            matches!(cell(&plan, "viking", 3), Some(Cell::Change { skin, how: How::Picked }) if skin.source == Some(0))
        );
        assert_eq!(cell(&plan, "smoky", 3), &Some(Cell::Off));
        assert!(
            matches!(
                cell(&plan, "hunter", 3),
                Some(Cell::Change { how: How::Own, .. })
            ),
            "the others keep the common choice"
        );
        settings.overrides.clear();
        let reset = plan_of(xt, &settings, &items, &[], &none);
        assert!(matches!(
            cell(&reset, "viking", 3),
            Some(Cell::Change { how: How::Own, .. })
        ));
    }

    #[test]
    fn kinds_left_out_leave_the_plan_and_the_run() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let xt = &looks[0];
        let none = |_: &LookItem, _: u8, _: &LookSkin| Vec::new();
        let mut settings = Settings::for_look(xt);
        settings.turrets = false;
        let plan = plan_of(xt, &settings, &items, &[], &none);
        assert!(plan.rows.iter().all(|r| r.kind == "hull"));
        let work = tasks(&plan);
        assert_eq!(work.len(), 2);
        assert!(work.iter().all(|t| t.kind == "hull"));
        assert_eq!(coverage(xt, &settings, 3), (2, 2));
        let hunter = work.iter().find(|t| t.id == "hunter").expect("hunter");
        let modifications: Vec<u8> = hunter.writes.iter().map(|w| w.modification).collect();
        assert_eq!(modifications, [1, 2, 3], "no M0 in the cache");
    }

    #[test]
    fn an_older_backup_without_the_grade_counts_only_an_unambiguous_variant() {
        let (packs, items) = looks_fixture();
        let refs: Vec<&InstalledPack> = packs.iter().collect();
        let looks = gather_looks(&refs, &items);
        let xt = &looks[0];
        let none = |_: &LookItem, _: u8, _: &LookSkin| Vec::new();
        let worn = |grade: &str| AppliedSkin {
            target: PackTarget {
                kind: "hull".into(),
                id: "viking".into(),
                grade: grade.into(),
            },
            pack_id: "a".into(),
            variant_id: "default".into(),
            variant_name: "Viking".into(),
            skin_grade: None,
            batch: None,
        };
        let plan = plan_of(xt, &Settings::for_look(xt), &items, &[worn("m3")], &none);
        assert!(
            cell(&plan, "viking", 3).as_ref().is_some_and(Cell::changes),
            "XT M0 and XT M3 share the id `default`"
        );
    }
}
