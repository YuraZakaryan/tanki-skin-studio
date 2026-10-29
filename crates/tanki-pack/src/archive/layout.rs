//! What a folder of an archive is, read from its path whatever the layout.
//!
//! Archives are laid out in many ways: `Flash/Hulls/New/Wasp/M3`, `2.0/hulls/wasp/m0`,
//! `html5/turrets/railgun/xt`, `Weapons/Railgun`, a bare `Wasp/` folder, names with typos
//! (`mamoth`, `frezee`). So no layout is assumed: every folder name is split into words, and the
//! words say what they are. A kind word (`hulls`, `tanks`, `weapons`...) says hull or turret; a
//! known item name (exact, another spelling, or one typo away) says the item; `old`/`new`... is
//! the style of a variant; the words below the item (`M3`, `XT M0`, `ALT`, `Prime`) are its
//! modification or edition; the words between the kind folder and the item (`Hulls/Neon/Wasp`)
//! name the skin.

use strsim::damerau_levenshtein;

/// Hull or turret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Hull,
    Turret,
}

impl Kind {
    pub(super) fn code(self) -> &'static str {
        match self {
            Kind::Hull => "hull",
            Kind::Turret => "turret",
        }
    }
}

/// The hulls and turrets of the game, with other names seen for them.
const HULLS: &[(&str, &[&str])] = &[
    ("wasp", &[]),
    ("hornet", &[]),
    ("hunter", &[]),
    ("viking", &[]),
    ("dictator", &[]),
    ("titan", &[]),
    ("mammoth", &["mamont"]),
    ("juggernaut", &[]),
    ("crusader", &[]),
    ("paladin", &[]),
    ("ares", &[]),
    ("hopper", &[]),
];
const TURRETS: &[(&str, &[&str])] = &[
    ("smoky", &[]),
    ("twins", &[]),
    ("firebird", &["flamethrower"]),
    ("freeze", &[]),
    ("isida", &[]),
    ("ricochet", &[]),
    ("thunder", &[]),
    ("railgun", &[]),
    ("magnum", &[]),
    ("hammer", &[]),
    ("shaft", &[]),
    ("striker", &[]),
    ("vulcan", &[]),
    ("gauss", &[]),
    ("terminator", &[]),
    ("tesla", &[]),
];

const HULL_WORDS: &[&str] = &["hull", "hulls", "tank", "tanks", "body", "bodies", "corpus"];
const TURRET_WORDS: &[&str] = &[
    "turret", "turrets", "weapon", "weapons", "gun", "guns", "cannon", "cannons",
];
const STYLE_WORDS: &[&str] = &["old", "new", "legacy", "classic", "original"];
/// Words that organise files and say nothing about the skin (`Wasp/M3/textures/details.png`).
const GENERIC_WORDS: &[&str] = &[
    "textures", "texture", "tex", "model", "models", "mesh", "files", "file", "assets", "asset",
    "skin", "skins", "data", "images", "image", "img", "preview", "previews", "variant",
    "variants", "pack", "packs",
];

/// What a folder holds, read from its path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Identity {
    pub kind: Kind,
    pub id: String,
    /// The item as a person writes it: `Wasp`, `Mammoth` for `mamoth`.
    pub name: String,
    /// `m3`, `xt`, `xt-m0`, `alt-m2`, `prime`, or `default` when the path names none.
    pub grade: String,
    /// The variant within the modification: `new`, `old`, `neon`, `neon-old`, or `default`.
    pub variant: String,
    pub variant_name: String,
    /// Index in the path of the folder named after the item.
    pub depth: usize,
    /// A kind folder (`Hulls`, `Weapons`...) above the item confirms it; a name alone does not
    /// (`Misc/Overdrives/Wasp` holds an effect, not a hull).
    pub strong: bool,
    /// A guess worth checking: a misspelt name, an item the catalog does not know.
    pub note: Option<String>,
}

/// A pack id fragment: lower-case ASCII letters, digits and `-`.
pub(super) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// The lower-case words of a folder name: split at anything not a letter or digit and where a
/// lower-case letter meets an upper-case one (`Hull_hornet_xt_m2`, `FirebirdXT`, `M3 ALT`).
pub(super) fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut after_lower = false;
    for c in text.chars() {
        if !c.is_alphanumeric() {
            if !word.is_empty() {
                out.push(std::mem::take(&mut word));
            }
            after_lower = false;
            continue;
        }
        if c.is_uppercase() && after_lower {
            out.push(std::mem::take(&mut word));
        }
        word.extend(c.to_lowercase());
        after_lower = c.is_lowercase();
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

fn title(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

fn kind_word(word: &str) -> Option<Kind> {
    if HULL_WORDS.contains(&word) {
        Some(Kind::Hull)
    } else if TURRET_WORDS.contains(&word) {
        Some(Kind::Turret)
    } else {
        None
    }
}

fn is_grade_word(word: &str) -> bool {
    matches!(word, "m0" | "m1" | "m2" | "m3")
}

/// A word that may name an item or a skin (not a kind, style, modification or filing word).
fn is_free(word: &str) -> bool {
    kind_word(word).is_none()
        && !STYLE_WORDS.contains(&word)
        && !GENERIC_WORDS.contains(&word)
        && !is_grade_word(word)
}

/// The item a word names: exactly, by another known name, or one typo away from exactly one
/// item (a swap of two letters counts as one). The flag is set for a typo.
fn match_item(word: &str) -> Option<(Kind, &'static str, bool)> {
    if word.len() < 3 {
        return None;
    }
    let catalog = || {
        HULLS
            .iter()
            .map(|entry| (Kind::Hull, entry))
            .chain(TURRETS.iter().map(|entry| (Kind::Turret, entry)))
    };
    if let Some((kind, (name, _))) =
        catalog().find(|(_, (name, aliases))| *name == word || aliases.contains(&word))
    {
        return Some((kind, name, false));
    }
    if word.len() < 5 {
        return None;
    }
    let mut near =
        catalog().filter(|(_, (name, _))| name.len() >= 5 && damerau_levenshtein(word, name) == 1);
    match (near.next(), near.next()) {
        (Some((kind, (name, _))), None) => Some((kind, name, true)),
        _ => None,
    }
}

/// The folder of a path that names the item, and which of its words do.
struct ItemAt {
    depth: usize,
    words: Vec<usize>,
    kind: Kind,
    id: String,
    name: String,
    note: Option<String>,
}

/// Read a folder path (`parts[0]` is the folder the user chose, the rest the folders below it).
/// `None` when nothing in it names a hull or a turret.
pub(super) fn interpret(parts: &[String]) -> Option<Identity> {
    let all: Vec<Vec<String>> = parts.iter().map(|part| words(part)).collect();
    let kind_at = (0..all.len())
        .rev()
        .find(|&i| all[i].iter().any(|w| kind_word(w).is_some()));

    // The item: the known name nearest to the files...
    let mut item: Option<ItemAt> = None;
    'search: for i in (0..all.len()).rev() {
        for (position, word) in all[i].iter().enumerate() {
            if let Some((kind, name, typo)) = match_item(word) {
                item = Some(ItemAt {
                    depth: i,
                    words: vec![position],
                    kind,
                    id: name.to_owned(),
                    name: title(name),
                    note: typo.then(|| format!("'{word}' read as {name}")),
                });
                break 'search;
            }
        }
    }
    // ...else the first named folder below a kind folder (`Hulls/Ufo`): an item the catalog does
    // not know yet.
    if item.is_none() {
        let k = kind_at?;
        let kind = all[k].iter().find_map(|w| kind_word(w))?;
        for (i, part) in all.iter().enumerate().skip(k + 1) {
            let free: Vec<usize> = (0..part.len()).filter(|&p| is_free(&part[p])).collect();
            let name: Vec<&str> = free.iter().map(|&p| part[p].as_str()).collect();
            let id = slug(&name.join("-"));
            if id.is_empty() {
                continue;
            }
            item = Some(ItemAt {
                depth: i,
                name: title(&name.join(" ")),
                note: Some(format!(
                    "'{}' is not a known hull or turret",
                    name.join(" ")
                )),
                words: free,
                kind,
                id,
            });
            break;
        }
    }
    let ItemAt {
        depth,
        words: item_words,
        kind,
        id,
        name,
        note,
    } = item?;
    let strong = kind_at.is_some_and(|k| k <= depth);
    let lower = kind_at.map_or(1, |k| k + 1).max(1);

    // The modification or edition: what is said at and below the item, in path order.
    let mut grade_words: Vec<&str> = Vec::new();
    for (i, part) in all.iter().enumerate().skip(depth) {
        for (position, word) in part.iter().enumerate() {
            let own_name = i == depth && item_words.contains(&position);
            let filing = kind_word(word).is_some()
                || STYLE_WORDS.contains(&word.as_str())
                || GENERIC_WORDS.contains(&word.as_str());
            if !own_name && !filing && !grade_words.contains(&word.as_str()) {
                grade_words.push(word);
            }
        }
    }
    let grade = if grade_words.is_empty() {
        "default".to_owned()
    } else {
        grade_words.join("-")
    };
    // The style nearest to the files, and the skin named between the kind folder and the item.
    let style = (lower..all.len())
        .rev()
        .flat_map(|i| all[i].iter())
        .find(|w| STYLE_WORDS.contains(&w.as_str()));
    let qualifier: Vec<&str> = if strong {
        all.iter()
            .take(depth)
            .skip(lower)
            .flatten()
            .map(String::as_str)
            .filter(|w| is_free(w))
            .collect()
    } else {
        Vec::new()
    };
    let label: Vec<&str> = qualifier
        .iter()
        .copied()
        .chain(style.map(String::as_str))
        .collect();
    let variant = match slug(&label.join("-")) {
        empty if empty.is_empty() => "default".to_owned(),
        variant => variant,
    };
    let variant_name = std::iter::once(name.clone())
        .chain(label.iter().map(|w| title(w)))
        .collect::<Vec<_>>()
        .join(" ");
    Some(Identity {
        kind,
        id,
        name,
        grade,
        variant,
        variant_name,
        depth,
        strong,
        note,
    })
}
