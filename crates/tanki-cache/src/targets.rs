//! Table of resource groups that are positively identified as game items.
//!
//! Adding support for a new hull, turret or paint is one row in [`KNOWN_TARGETS`]:
//! `marker` is the path fragment shared by every cache URL of that item. Items the user
//! labelled in the app are added at run time and persisted by the state store.

use std::sync::{OnceLock, RwLock};

#[derive(Debug, PartialEq, Eq)]
pub struct KnownTarget {
    pub game: &'static str,
    pub marker: &'static str,
    pub kind: &'static str,
    pub id: &'static str,
    pub grade: &'static str,
}

pub static KNOWN_TARGETS: &[KnownTarget] = &[
    KnownTarget {
        game: "air-cache",
        marker: "/0/0/10/14/",
        kind: "hull",
        id: "wasp",
        grade: "m3",
    },
    KnownTarget {
        game: "air-cache",
        marker: "/0/0/10/64/",
        kind: "turret",
        id: "railgun",
        grade: "m3",
    },
];

/// Rows the user labelled by hand. Leaked on purpose: they live for the whole process and
/// keep `KnownTarget` `'static` for every caller.
static USER_TARGETS: OnceLock<RwLock<Vec<&'static KnownTarget>>> = OnceLock::new();

fn user_targets() -> &'static RwLock<Vec<&'static KnownTarget>> {
    USER_TARGETS.get_or_init(|| RwLock::new(Vec::new()))
}

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

pub fn identify(game: &str, group: &str) -> Option<&'static KnownTarget> {
    let matches = |t: &&'static KnownTarget| t.game == game && group.contains(t.marker);
    KNOWN_TARGETS.iter().find(matches).or_else(|| {
        user_targets()
            .read()
            .ok()
            .and_then(|rows| rows.iter().copied().find(matches))
    })
}

/// Path fragment shared by every revision of an item: the URL path without its last segment,
/// e.g. `http://host/0/0/10/20/31464` -> `/0/0/10/20/`.
pub fn marker_for_group(group: &str) -> Option<String> {
    let path = group.split_once("://")?.1;
    let path = &path[path.find('/')?..];
    let (parent, revision) = path.trim_end_matches('/').rsplit_once('/')?;
    let marker = format!("{parent}/");
    (!revision.is_empty() && marker.matches('/').count() >= 3).then_some(marker)
}

fn valid_token(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 40
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// Teach the adapter that `marker` is the given item. Replaces an earlier label of the same marker.
pub fn register_user_target(
    game: &str,
    marker: &str,
    kind: &str,
    id: &str,
    grade: &str,
) -> Result<(), String> {
    if !matches!(kind, "hull" | "turret" | "paint") {
        return Err(format!("unknown kind: {kind}"));
    }
    if !valid_token(id) || !valid_token(grade) {
        return Err("id and grade: lowercase latin letters, digits, - and _".into());
    }
    if marker.matches('/').count() < 3 || !marker.starts_with('/') || !marker.ends_with('/') {
        return Err(format!("marker too generic: {marker}"));
    }
    if KNOWN_TARGETS
        .iter()
        .any(|t| t.game == game && t.marker == marker)
    {
        return Err("this resource is already built in".into());
    }
    let row: &'static KnownTarget = Box::leak(Box::new(KnownTarget {
        game: leak(game),
        marker: leak(marker),
        kind: leak(kind),
        id: leak(id),
        grade: leak(grade),
    }));
    let mut rows = user_targets().write().map_err(|e| e.to_string())?;
    rows.retain(|t| !(t.game == game && t.marker == marker));
    rows.push(row);
    Ok(())
}

fn marker_id(marker: &str) -> Option<u64> {
    let parts: Vec<u64> = marker
        .trim_matches('/')
        .split('/')
        .map(|p| u64::from_str_radix(p, 8).ok())
        .collect::<Option<_>>()?;
    let [a, b, c, d] = parts.as_slice() else {
        return None;
    };
    (*b <= 0xffff && *c <= 0xff && *d <= 0xff && *a <= 0xffff_ffff)
        .then_some((a << 32) | (b << 16) | (c << 8) | d)
}

fn id_marker(id: u64) -> String {
    format!(
        "/{:o}/{:o}/{:o}/{:o}/",
        id >> 32,
        (id >> 16) & 0xffff,
        (id >> 8) & 0xff,
        id & 0xff
    )
}

/// The four modifications of one item sit on consecutive even resource ids (M0, M1, M2, M3 with
/// step 2). Given the marker and grade of one of them, return the markers and grades of all four.
/// Verified on two anchors in a real cache: Wasp M3 and Railgun M3 are each the last id of such a block.
pub fn modification_markers(marker: &str, grade: &str) -> Option<Vec<(String, String)>> {
    let index: u64 = grade.strip_prefix('m')?.parse().ok().filter(|i| *i <= 3)?;
    let start = marker_id(marker)?.checked_sub(index * 2)?;
    Some(
        (0..4)
            .map(|j| (id_marker(start + j * 2), format!("m{j}")))
            .collect(),
    )
}

/// Markers of the resources up to `steps` slots either side of `marker` (slots are 2 ids apart), as
/// (signed distance in slots, marker); distance 0 is `marker` itself. Lets the UI show what lies
/// next to an unknown resource so a user can confirm which ones form one item.
pub fn neighbor_markers(marker: &str, steps: i64) -> Option<Vec<(i64, String)>> {
    let id = i64::try_from(marker_id(marker)?).ok()?;
    Some(
        (-steps..=steps)
            .filter_map(|k| u64::try_from(id + k * 2).ok().map(|v| (k, id_marker(v))))
            .collect(),
    )
}

/// True when some built-in or user label already claims this marker.
pub fn is_marker_known(game: &str, marker: &str) -> bool {
    identify(game, marker).is_some()
}

pub fn unregister_user_target(game: &str, marker: &str) {
    if let Ok(mut rows) = user_targets().write() {
        rows.retain(|t| !(t.game == game && t.marker == marker));
    }
}

/// True when the group is recognised only because the user labelled it.
pub fn is_user_target(game: &str, group: &str) -> bool {
    identify(game, group).is_some_and(|t| !KNOWN_TARGETS.iter().any(|b| std::ptr::eq(b, t)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_unique_per_game() {
        for (i, a) in KNOWN_TARGETS.iter().enumerate() {
            for b in &KNOWN_TARGETS[i + 1..] {
                assert!(
                    a.game != b.game || a.marker != b.marker,
                    "duplicate marker {}",
                    a.marker
                );
            }
        }
    }

    #[test]
    fn derives_a_marker_from_a_cache_group() {
        assert_eq!(
            marker_for_group("http://host.test/0/0/10/20/31464").as_deref(),
            Some("/0/0/10/20/")
        );
        assert_eq!(marker_for_group("/just/a/file/path"), None);
        assert_eq!(
            marker_for_group("http://host.test/a/1"),
            None,
            "too generic"
        );
    }

    #[test]
    fn user_labels_are_validated_and_recognised() {
        let game = "label-test";
        assert!(register_user_target(game, "/0/0/10/14/", "hull", "x", "m1").is_ok());
        assert!(identify(game, "http://h/0/0/10/14/1").is_some());
        assert!(register_user_target("air-cache", "/0/0/10/14/", "hull", "x", "m1").is_err());
        assert!(register_user_target(game, "/9/9/9/", "tank", "x", "m1").is_err());
        assert!(register_user_target(game, "/9/9/9/", "hull", "Bad Id", "m1").is_err());
        assert!(register_user_target(game, "/9/", "hull", "x", "m1").is_err());
        assert!(identify(game, "http://h/0/0/10/14/1").unwrap().id == "x");
        assert!(is_user_target(game, "http://h/0/0/10/14/1"));
        assert!(!is_user_target("air-cache", "http://h/0/0/10/14/1"));
        unregister_user_target(game, "/0/0/10/14/");
        assert!(identify(game, "http://h/0/0/10/14/1").is_none());
    }

    #[test]
    fn derives_the_four_modifications_from_one() {
        let block = modification_markers("/0/0/10/14/", "m3").expect("m3 of Wasp");
        let markers: Vec<&str> = block.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(
            markers,
            ["/0/0/10/6/", "/0/0/10/10/", "/0/0/10/12/", "/0/0/10/14/"]
        );
        assert_eq!(block[0].1, "m0");
        // Any member of the block yields the same block.
        assert_eq!(modification_markers("/0/0/10/10/", "m1"), Some(block));
        assert!(modification_markers("/0/0/10/14/", "m4").is_none());
        assert!(modification_markers("/0/0/10/14/", "x").is_none());
        assert!(modification_markers("/0/0/10/", "m1").is_none());
        assert!(
            modification_markers("/0/0/0/2/", "m3").is_none(),
            "would underflow"
        );
    }

    #[test]
    fn lists_neighbours_in_two_id_slots() {
        let n = neighbor_markers("/0/0/10/14/", 1).unwrap();
        assert_eq!(
            n,
            [
                (-1, "/0/0/10/12/".to_string()),
                (0, "/0/0/10/14/".to_string()),
                (1, "/0/0/10/16/".to_string())
            ]
        );
        assert_eq!(
            neighbor_markers("/0/0/0/2/", 3).unwrap().len(),
            5,
            "no negative ids"
        );
        assert!(neighbor_markers("not a marker", 1).is_none());
    }

    #[test]
    fn identifies_only_for_matching_game() {
        assert_eq!(
            identify("air-cache", "http://x/0/0/10/64/9").unwrap().id,
            "railgun"
        );
        assert!(identify("other-game", "http://x/0/0/10/64/9").is_none());
    }
}
