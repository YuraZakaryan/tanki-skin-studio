// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! The view model: what the screens show, derived from a cache scan and the installed packs.
//! Everything here is plain data and pure functions, so it is unit-tested without a window.

use std::{collections::BTreeMap, path::PathBuf};

use tanki_core::{
    cache::{CacheScan, known_target_group},
    targets::marker_for_group,
};

/// "wasp", "m3" -> "Wasp M3".
pub fn target_name(id: &str, grade: &str) -> String {
    format!("{} {}", target_title(id), grade.to_uppercase())
}

/// "wasp" -> "Wasp".
pub fn target_title(id: &str) -> String {
    let mut chars = id.chars();
    let head = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_default();
    format!("{head}{}", chars.as_str())
}

/// 412 KB, 1.2 MB, 14 MB: the sizes the design shows.
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < MB {
        format!("{} KB", (b / KB).round() as u64)
    } else if b < 10.0 * MB {
        format!("{:.1} MB", b / MB)
    } else if b < GB {
        format!("{} MB", (b / MB).round() as u64)
    } else {
        format!("{:.1} GB", b / GB)
    }
}

/// A fact for a table row: the text, or a dash when there is none (a pack without an author).
pub fn or_dash(text: &str) -> String {
    if text.trim().is_empty() {
        "—".to_owned()
    } else {
        text.to_owned()
    }
}

/// The non-empty parts joined with " · ", so a missing part leaves no stray separator.
pub fn dotted<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 3549 -> "3,549" in English, "3 549" in Russian (the current UI language decides).
pub fn thousands(n: usize) -> String {
    tanki_i18n::group_digits(n as u64)
}

/// The resource filter of the Resources page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Hull,
    Turret,
    Paint,
    Image,
    Model,
    Sound,
    Archive,
}

impl Filter {
    pub const ALL: [Filter; 8] = [
        Filter::All,
        Filter::Hull,
        Filter::Turret,
        Filter::Paint,
        Filter::Image,
        Filter::Model,
        Filter::Sound,
        Filter::Archive,
    ];

    /// The stored value.
    pub fn code(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Hull => "hull",
            Filter::Turret => "turret",
            Filter::Paint => "paint",
            Filter::Image => "image",
            Filter::Model => "model",
            Filter::Sound => "sound",
            Filter::Archive => "archive",
        }
    }

    pub fn from_code(code: &str) -> Filter {
        Filter::ALL
            .into_iter()
            .find(|f| f.code() == code)
            .unwrap_or(Filter::All)
    }

    pub fn codes() -> Vec<&'static str> {
        Filter::ALL.iter().map(|f| f.code()).collect()
    }
}

/// One entry of the resource catalog: every file of one resource id, all its revisions together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceRow {
    /// Stable key: the `/a/b/c/d/` marker, or the folder for files outside the client's layout.
    pub key: String,
    /// The resource id decoded from the marker (0 when there is none).
    pub id: u64,
    pub files: usize,
    pub bytes: u64,
    /// The newest revision found (the last URL segment of the group), as written in the cache.
    pub revision: String,
    /// Hull / turret / paint, kind and grade when the item is recognised.
    pub target: Option<(String, String, String)>,
    /// `image`, `model`, `sound`, `archive`, ... of the biggest kind present.
    pub kind: String,
    /// File roles present (`details.jpg`, `object.3ds`, ...), sorted.
    pub roles: Vec<String>,
    /// A group (URL minus file name) to take the preview texture from.
    pub preview_group: Option<String>,
    /// The garage picture of an item: the preview resource stored on the id right after the model's.
    pub picture_group: Option<String>,
    /// The lone file name for single-file resources such as `ui_atlas.tara`.
    pub lone_file: Option<String>,
}

impl ResourceRow {
    /// What the list shows as the title.
    pub fn title(&self) -> String {
        if let Some((_, id, grade)) = &self.target {
            return target_name(id, grade);
        }
        if let Some(file) = &self.lone_file
            && file.contains('.')
            && !is_generic_role(file)
        {
            return file.clone();
        }
        format!("#{}", self.id)
    }

    pub fn filter_matches(&self, filter: Filter) -> bool {
        match filter {
            Filter::All => true,
            Filter::Hull => self.target.as_ref().is_some_and(|t| t.0 == "hull"),
            Filter::Turret => self.target.as_ref().is_some_and(|t| t.0 == "turret"),
            Filter::Paint => self.target.as_ref().is_some_and(|t| t.0 == "paint"),
            Filter::Image => self.kind == "image",
            Filter::Model => self.kind == "model",
            Filter::Sound => self.kind == "sound",
            Filter::Archive => self.kind == "archive",
        }
    }
}

fn is_generic_role(file: &str) -> bool {
    matches!(
        file,
        "object.3ds" | "lightmap.jpg" | "details.jpg" | "details_alpha.jpg" | "image.jpg"
    )
}

/// The resource id of a `/a/b/c/d/` marker: four octal numbers packed as in the client.
pub fn marker_id(marker: &str) -> u64 {
    let parts: Vec<u64> = marker
        .trim_matches('/')
        .split('/')
        .filter_map(|p| u64::from_str_radix(p, 8).ok())
        .collect();
    match parts.as_slice() {
        [a, b, c, d] => (a << 32) | (b << 16) | (c << 8) | d,
        _ => 0,
    }
}

fn revision_of(group: &str) -> &str {
    group.rsplit('/').next().unwrap_or("")
}

/// Group the scan's files into one row per resource id.
pub fn build_rows(scan: &CacheScan) -> Vec<ResourceRow> {
    #[derive(Default)]
    struct Acc {
        files: usize,
        bytes: u64,
        kinds: BTreeMap<String, usize>,
        roles: BTreeMap<String, ()>,
        revision: String,
        revision_num: u64,
        preview_group: Option<String>,
        lone_file: Option<String>,
        target: Option<(String, String, String)>,
        id: u64,
    }
    let mut map: BTreeMap<String, Acc> = BTreeMap::new();
    for resource in &scan.resources {
        let marker = marker_for_group(&resource.group);
        let key = marker.clone().unwrap_or_else(|| resource.group.clone());
        let acc = map.entry(key).or_default();
        acc.files += 1;
        acc.bytes += resource.size;
        *acc.kinds.entry(resource.kind.clone()).or_default() += 1;
        acc.roles.insert(resource.role.clone(), ());
        acc.lone_file = Some(resource.role.clone());
        if let Some(marker) = &marker {
            acc.id = marker_id(marker);
        }
        let revision = revision_of(&resource.group).to_owned();
        let revision_num = revision.parse::<u64>().unwrap_or(0);
        if revision_num >= acc.revision_num {
            acc.revision_num = revision_num;
            acc.revision = revision;
        }
        if acc.target.is_none() {
            acc.target = known_target_group(&scan.game, &resource.group)
                .map(|(k, i, g)| (k.to_owned(), i.to_owned(), g.to_owned()));
        }
        if matches!(resource.role.as_str(), "details.jpg" | "image.jpg")
            && (acc.preview_group.is_none() || acc.revision_num == revision_num)
        {
            acc.preview_group = Some(resource.group.clone());
        }
    }
    let mut rows: Vec<ResourceRow> = map
        .into_iter()
        .map(|(key, acc)| {
            let kind = ["archive", "model", "sound", "image"]
                .iter()
                .find(|k| acc.kinds.contains_key(**k))
                .map(|k| (*k).to_owned())
                .or_else(|| acc.kinds.keys().next().cloned())
                .unwrap_or_else(|| "other".into());
            ResourceRow {
                key,
                id: acc.id,
                files: acc.files,
                bytes: acc.bytes,
                revision: acc.revision,
                target: acc.target,
                kind,
                roles: acc.roles.into_keys().collect(),
                preview_group: acc.preview_group,
                picture_group: None,
                lone_file: if acc.files == 1 { acc.lone_file } else { None },
            }
        })
        .collect();
    // The picture of a model is the preview resource on the next id.
    let pictures: BTreeMap<u64, String> = rows
        .iter()
        .filter(|r| r.roles.iter().any(|role| role == "image.jpg"))
        .filter_map(|r| Some((r.id, r.preview_group.clone()?)))
        .collect();
    for row in &mut rows {
        if row.roles.iter().any(|role| role == "object.3ds") {
            row.picture_group = pictures.get(&(row.id + 1)).cloned();
        }
    }
    // Recognised items first, then by resource id.
    rows.sort_by(|a, b| {
        b.target
            .is_some()
            .cmp(&a.target.is_some())
            .then(a.id.cmp(&b.id))
            .then(a.key.cmp(&b.key))
    });
    rows
}

/// Case-insensitive match of a search text against a row's title, kind and roles.
pub fn row_matches(row: &ResourceRow, needle: &str) -> bool {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }
    let mut hay = row.title().to_lowercase();
    hay.push(' ');
    hay.push_str(&row.kind);
    for role in &row.roles {
        hay.push(' ');
        hay.push_str(&role.to_lowercase());
    }
    needle.split_whitespace().all(|word| hay.contains(word))
}

/// When a backup was made, relative to now, in the user's local time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Day {
    Today,
    Yesterday,
    /// `2026-10-27` for anything older.
    Date(String),
}

/// The local day and `HH:MM` of a Unix time. `offset_secs` is the local UTC offset; `now_unix`
/// decides what counts as today and yesterday.
pub fn local_time(created_unix: u64, now_unix: u64, offset_secs: i32) -> (Day, String) {
    use chrono::{DateTime, FixedOffset, Utc};
    let offset = FixedOffset::east_opt(offset_secs)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("utc"));
    let at = |unix: u64| {
        DateTime::<Utc>::from_timestamp(unix as i64, 0)
            .unwrap_or_default()
            .with_timezone(&offset)
    };
    let (then, now) = (at(created_unix), at(now_unix));
    let clock = then.format("%H:%M").to_string();
    let days = now
        .date_naive()
        .signed_duration_since(then.date_naive())
        .num_days();
    let day = match days {
        0 => Day::Today,
        1 => Day::Yesterday,
        _ => Day::Date(then.format("%Y-%m-%d").to_string()),
    };
    (day, clock)
}

/// The system's current Unix time and local UTC offset in seconds.
pub fn now_and_offset() -> (u64, i32) {
    use chrono::{Local, Offset};
    let now = Local::now();
    (
        now.timestamp().max(0) as u64,
        now.offset().fix().local_minus_utc(),
    )
}

/// One garage entry: an item (Wasp) with its modifications.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseItem {
    pub kind: String,
    pub id: String,
    /// `present[i]` is true when M`i` exists in the cache.
    pub present: [bool; 4],
    /// Cache marker of each present modification.
    pub markers: [Option<String>; 4],
}

impl BaseItem {
    pub fn name(&self) -> String {
        target_title(&self.id)
    }
}

/// Garage items of the scan: one per recognised (kind, id), grades marked present.
pub fn base_items(scan: &CacheScan) -> Vec<BaseItem> {
    let mut map: BTreeMap<(String, String), BaseItem> = BTreeMap::new();
    for row in build_rows(scan) {
        let Some((kind, id, grade)) = row.target else {
            continue;
        };
        let item = map
            .entry((kind.clone(), id.clone()))
            .or_insert_with(|| BaseItem {
                kind,
                id,
                present: [false; 4],
                markers: [None, None, None, None],
            });
        if let Some(index) = grade_index(&grade) {
            item.present[index] = true;
            item.markers[index] = Some(row.key);
        }
    }
    map.into_values().collect()
}

/// `m0`..`m3` -> 0..3.
pub fn grade_index(grade: &str) -> Option<usize> {
    match grade {
        "m0" => Some(0),
        "m1" => Some(1),
        "m2" => Some(2),
        "m3" => Some(3),
        _ => None,
    }
}

/// Where a pack item's modification or edition sorts: the regular line M0..M3 first, then a skin
/// whose path named none (`default`), then the special editions (XT, ALT, Prime, then any other
/// word, alphabetically), each again by its modification. An explicit rank, not the alphabet, as
/// tiered inventories sort; the names inside a rank sort naturally (see [`natural_cmp`]).
pub fn grade_rank(grade: &str) -> (u8, String, u8) {
    let mut modification = None;
    let mut edition: Vec<&str> = Vec::new();
    for word in grade.split('-').filter(|w| !w.is_empty()) {
        match grade_index(word) {
            Some(index) if modification.is_none() => modification = Some(index as u8),
            _ => edition.push(word),
        }
    }
    // Inside an edition its plain form comes first, then its modifications (XT, XT M0, XT M1...).
    let edition_step = modification.map_or(0, |m| m + 1);
    match edition.as_slice() {
        [] => (0, String::new(), modification.unwrap_or(4)),
        ["default"] => (1, String::new(), edition_step),
        words => {
            let name = words.join("-");
            let known = ["xt", "alt", "prime"].iter().position(|e| *e == name);
            (
                2 + known.map_or(3, |p| p as u8),
                if known.is_some() { String::new() } else { name },
                edition_step,
            )
        }
    }
}

/// Text order where a run of digits compares by its value ("Skin 2" before "Skin 10") and letters
/// ignore case: the numeric collation of Unicode UTS #35 (`kn`), known as natural sort.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars<'_>>| {
                    let mut digits = String::new();
                    while let Some(ch) = it.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(ch);
                        it.next();
                    }
                    digits
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let order = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(c), Some(d)) => {
                let order = c.to_lowercase().cmp(d.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// The order of the skin variants of one item: by [`grade_rank`], then by name, naturally.
pub fn variant_order(a: (&str, &str), b: (&str, &str)) -> std::cmp::Ordering {
    grade_rank(a.0)
        .cmp(&grade_rank(b.0))
        .then_with(|| natural_cmp(a.1, b.1))
}

pub fn grade_code(index: usize) -> &'static str {
    ["m0", "m1", "m2", "m3"][index.min(3)]
}

/// Paths of the cached `details.jpg` of a group, for previews.
pub fn preview_path(scan: &CacheScan, group: &str) -> Option<PathBuf> {
    scan.resources
        .iter()
        .find(|r| r.group == group && matches!(r.role.as_str(), "details.jpg" | "image.jpg"))
        .map(|r| r.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_sort_m0_to_m3_then_the_editions_each_by_name_naturally() {
        let mut variants = vec![
            ("m3", "Hunter New"),
            ("prime", "Hunter"),
            ("m3", "Hunter Old"),
            ("xt-m0", "Hunter"),
            ("m0", "Hunter Old"),
            ("default", "Hunter"),
            ("alt-m2", "Hunter"),
            ("m0", "Hunter New"),
            ("xt", "Hunter"),
            ("legacy", "Hunter"),
            ("m1", "Hunter Camo 10"),
            ("m1", "Hunter Camo 2"),
            ("m2", "hunter neon"),
            ("m2", "Hunter Neon Old"),
        ];
        variants.sort_by(|a, b| variant_order(*a, *b));
        assert_eq!(
            variants,
            [
                ("m0", "Hunter New"),
                ("m0", "Hunter Old"),
                ("m1", "Hunter Camo 2"),
                ("m1", "Hunter Camo 10"),
                ("m2", "hunter neon"),
                ("m2", "Hunter Neon Old"),
                ("m3", "Hunter New"),
                ("m3", "Hunter Old"),
                ("default", "Hunter"),
                ("xt", "Hunter"),
                ("xt-m0", "Hunter"),
                ("alt-m2", "Hunter"),
                ("prime", "Hunter"),
                ("legacy", "Hunter"),
            ]
        );
    }

    #[test]
    fn names_compare_digits_by_value_and_letters_without_case() {
        use std::cmp::Ordering::*;
        assert_eq!(natural_cmp("Skin 2", "Skin 10"), Less);
        assert_eq!(natural_cmp("Skin 010", "Skin 9"), Greater);
        assert_eq!(natural_cmp("neon", "Neon Old"), Less);
        assert_eq!(natural_cmp("Камуфляж 3", "Камуфляж 12"), Less);
        assert_eq!(natural_cmp("", "a"), Less);
        assert_eq!(natural_cmp("a1b", "a1b"), Equal);
    }
    use tanki_testkit::{HORNET_MARKER, build_cache, scratch};

    #[test]
    fn backup_times_are_relative_to_the_local_day() {
        // 2026-10-27 21:40 UTC+3 is 18:40 UTC.
        let evening = 1_793_126_400; // 2026-10-27 18:40:00 UTC
        let (day, clock) = local_time(evening, evening + 600, 3 * 3600);
        assert_eq!((day, clock.as_str()), (Day::Today, "21:40"));
        let (day, _) = local_time(evening, evening + 24 * 3600, 3 * 3600);
        assert_eq!(day, Day::Yesterday);
        let (day, _) = local_time(evening, evening + 5 * 24 * 3600, 3 * 3600);
        assert_eq!(day, Day::Date("2026-10-27".into()));
        // The offset moves the day boundary: 21:40 local is already tomorrow at UTC+5.
        let (day, clock) = local_time(evening, evening, 5 * 3600);
        assert_eq!((day, clock.as_str()), (Day::Today, "23:40"));
    }

    #[test]
    fn sizes_read_like_the_design() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(412 * 1024), "412 KB");
        assert_eq!(format_bytes(1_258_291), "1.2 MB");
        assert_eq!(format_bytes(14 * 1024 * 1024), "14 MB");
        assert_eq!(thousands(3549), "3,549");
        assert_eq!(thousands(12), "12");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn counts_follow_the_ui_language() {
        tanki_i18n::set_current(tanki_i18n::Lang::from_code("ru").unwrap());
        let russian = thousands(3660);
        tanki_i18n::set_current(tanki_i18n::Lang::EN);
        assert_eq!(
            russian, "3\u{a0}660",
            "Russian groups digits with a no-break space"
        );
        assert_eq!(thousands(3660), "3,660");
    }

    #[test]
    fn missing_facts_leave_no_stray_separator() {
        assert_eq!(or_dash(""), "—");
        assert_eq!(or_dash("  "), "—");
        assert_eq!(or_dash("Ann"), "Ann");
        assert_eq!(dotted(["v1", "", "MIT"]), "v1 · MIT");
        assert_eq!(dotted(["", " "]), "");
        assert_eq!(dotted(["v1", "Ann", "MIT"]), "v1 · Ann · MIT");
    }

    #[test]
    fn marker_ids_follow_the_octal_layout() {
        assert_eq!(marker_id("/0/0/10/14/"), 2060);
        assert_eq!(marker_id("/0/0/10/64/"), 2100);
        assert_eq!(marker_id("nonsense"), 0);
    }

    #[test]
    fn rows_merge_revisions_and_put_recognised_items_first() {
        let root = scratch("model-rows");
        let cache = build_cache(&root).unwrap();
        let scan = tanki_core::cache::scan_cache(&cache.root, Some("air-cache")).unwrap();
        let rows = build_rows(&scan);
        let hull = rows
            .iter()
            .find(|r| r.target.as_ref().is_some_and(|t| t.1 == "wasp"))
            .expect("wasp row");
        // Two revisions of four files each are one row.
        assert_eq!(hull.files, 8);
        assert_eq!(hull.revision, "222");
        assert_eq!(hull.title(), "Wasp M3");
        assert!(rows[0].target.is_some() && rows[1].target.is_some());
        let hornet = rows
            .iter()
            .find(|r| r.key.contains(HORNET_MARKER) || r.key.contains("/0/0/10/20/"))
            .expect("unlabelled hornet row");
        assert!(hornet.target.is_none());
        assert!(hornet.title().starts_with('#'));
        let archive = rows.iter().find(|r| r.kind == "archive").expect("archive");
        assert_eq!(archive.title(), "image.tara");
    }

    #[test]
    fn filters_and_search_narrow_the_rows() {
        let root = scratch("model-filter");
        let cache = build_cache(&root).unwrap();
        let scan = tanki_core::cache::scan_cache(&cache.root, Some("air-cache")).unwrap();
        let rows = build_rows(&scan);
        let hulls: Vec<_> = rows
            .iter()
            .filter(|r| r.filter_matches(Filter::Hull))
            .collect();
        assert_eq!(hulls.len(), 1);
        assert!(
            rows.iter()
                .filter(|r| r.filter_matches(Filter::Sound))
                .count()
                >= 1
        );
        assert!(rows.iter().any(|r| row_matches(r, "wasp m3")));
        assert!(!rows.iter().any(|r| row_matches(r, "no-such-thing")));
        assert_eq!(Filter::from_code("garbage"), Filter::All);
        assert_eq!(Filter::from_code("turret"), Filter::Turret);
    }

    #[test]
    fn base_items_mark_which_modifications_exist() {
        let root = scratch("model-items");
        let cache = build_cache(&root).unwrap();
        let scan = tanki_core::cache::scan_cache(&cache.root, Some("air-cache")).unwrap();
        let items = base_items(&scan);
        let wasp = items.iter().find(|i| i.id == "wasp").unwrap();
        assert_eq!(wasp.present, [false, false, false, true]);
        assert_eq!(wasp.name(), "Wasp");
        assert_eq!(grade_index("m2"), Some(2));
        assert_eq!(grade_code(9), "m3");
    }
}
