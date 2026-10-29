//! UI localisation. Strings in code stay English and are the lookup keys; a per-language catalog
//! (`*.tsv`, see `ru.tsv` for the format) maps them to display text at render time. File names,
//! pack manifests, cache paths, the CLI and the dev hooks never see translated text, so scripts
//! and tests are unaffected. A string without a translation is shown in English, so coverage can
//! grow incrementally (`cargo xtask i18n` keeps the catalog and the code in step).
//!
//! The design follows PdfCraft's `i18n` module (English-keyed TSV catalogs, a thread-local current
//! language set once per frame, plural rules per language), reduced to the languages this app
//! ships. English is the default; the user's choice is stored in the app database (see
//! `tanki-state`) and restored at the next start.
//!
//! # Adding a language
//! 1. Add `xx.tsv` next to `ru.tsv` (copy its header; translate from the *meaning* of the English
//!    text).
//! 2. Add one row to [`LANGUAGES`] (code, native name, catalog, plural rule).
//!
//! The language switch, the preference parser and the catalog tests pick it up from the registry.
//!
//! # Looking strings up
//! - [`tl!`]/[`t`]: a plain string in the current language. [`tr`]: in a given one.
//! - [`tl_ctx!`]/[`tr_ctx`]: when one English word needs different translations.
//! - [`trn`]: plural-aware (`{n}` is filled in). [`fmt`]: fill `{name}` placeholders after [`tr`];
//!   translators may reorder placeholders freely.

mod catalog;

use std::cell::Cell;
use std::sync::OnceLock;

use catalog::Catalog;

pub use catalog::{Entry, parse_entries, placeholders};

/// The `language` preference value used when nothing was chosen yet.
pub const DEFAULT: &str = "en";

/// One supported UI language.
pub struct LangInfo {
    /// BCP 47 code, lowercase. Also the stored `language` preference value.
    pub code: &'static str,
    /// The language's name in itself, shown on the language switch.
    pub name: &'static str,
    /// Catalog file contents (empty for the built-in English).
    pub source: &'static str,
    /// Plural form index for a count (English: 0 = one, 1 = other). A catalog's `@plural` entries
    /// list one form per index.
    pub plural: fn(u64) -> usize,
    /// What separates groups of three digits in a count: `3,549` in English, `3 549` (a no-break
    /// space, so a number never wraps) in Russian.
    pub digit_group: char,
    catalog: OnceLock<Catalog>,
}

fn plural_one_other(n: u64) -> usize {
    usize::from(n != 1)
}

/// Russian: 1 (but not 11) is `one`, 2–4 (but not 12–14) `few`, everything else `many`.
fn plural_russian(n: u64) -> usize {
    match n % 100 {
        11..=14 => 2,
        _ => match n % 10 {
            1 => 0,
            2..=4 => 1,
            _ => 2,
        },
    }
}

/// The registry. English first: it is the fallback and the source language.
pub static LANGUAGES: [LangInfo; 2] = [
    LangInfo {
        code: "en",
        name: "English",
        source: "",
        plural: plural_one_other,
        digit_group: ',',
        catalog: OnceLock::new(),
    },
    LangInfo {
        code: "ru",
        name: "Русский",
        source: include_str!("ru.tsv"),
        plural: plural_russian,
        digit_group: '\u{a0}',
        catalog: OnceLock::new(),
    },
];

impl LangInfo {
    /// How many plural forms the language's `@plural` entries list.
    fn plural_forms(&self) -> usize {
        (0..=1000).map(self.plural).max().unwrap_or(0) + 1
    }

    /// The parsed catalog, built on first use; entries that fail validation fall back to English.
    fn catalog(&self) -> &Catalog {
        self.catalog.get_or_init(|| {
            let (catalog, errors) = Catalog::parse(self.source, self.plural_forms());
            for error in errors {
                eprintln!(
                    "{} interface catalog: {error}; that entry shows in English",
                    self.code
                );
            }
            catalog
        })
    }
}

/// A language the UI can be shown in (a handle into [`LANGUAGES`]).
#[derive(Clone, Copy)]
pub struct Lang(&'static LangInfo);

impl std::fmt::Debug for Lang {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Lang({})", self.0.code)
    }
}

impl PartialEq for Lang {
    fn eq(&self, other: &Self) -> bool {
        self.0.code == other.0.code
    }
}

impl Eq for Lang {}

impl Lang {
    pub const EN: Lang = Lang(&LANGUAGES[0]);

    pub fn code(self) -> &'static str {
        self.0.code
    }

    /// The language's name in itself.
    pub fn name(self) -> &'static str {
        self.0.name
    }

    /// A language by its exact code.
    pub fn from_code(code: &str) -> Option<Lang> {
        LANGUAGES
            .iter()
            .find(|l| l.code.eq_ignore_ascii_case(code))
            .map(Lang)
    }

    /// Resolve the stored `language` preference. Missing or unknown values (for example a code
    /// written by a newer version) give English, the default.
    pub fn from_pref(pref: Option<&str>) -> Lang {
        pref.and_then(Lang::from_code).unwrap_or(Lang::EN)
    }

    /// A count with its digits grouped by threes the way the language writes them.
    pub fn group_digits(self, n: u64) -> String {
        let digits = n.to_string();
        let mut out = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                out.push(self.0.digit_group);
            }
            out.push(c);
        }
        out
    }

    /// Every registered language, English first.
    pub fn all() -> impl Iterator<Item = Lang> {
        LANGUAGES.iter().map(Lang)
    }

    fn catalog(self) -> &'static Catalog {
        self.0.catalog()
    }
}

thread_local! {
    // Per thread, as in PdfCraft: independent app and test threads must not change each other's
    // drawing language.
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::EN) };
}

/// Set the UI language for drawing (the shell calls this once per frame from the preference), so
/// widgets can translate without every call site carrying a language around.
pub fn set_current(lang: Lang) {
    CURRENT.set(lang);
}

/// The language the UI is drawn in.
pub fn current() -> Lang {
    CURRENT.get()
}

/// Does `lang` have a catalog entry for this plain string? (English never does: it is the source.)
pub fn has(lang: Lang, s: &str) -> bool {
    lang.catalog().plain(s).is_some()
}

/// Translate an English UI string into the current language ([`tr`] with [`current`]).
pub fn t(s: &str) -> &str {
    tr(current(), s)
}

/// Translate an English UI string; unknown strings come back unchanged.
pub fn tr(lang: Lang, s: &str) -> &str {
    lang.catalog().plain(s).unwrap_or(s)
}

/// Like [`tr`], for an English string that needs a disambiguating `context`.
pub fn tr_ctx<'a>(lang: Lang, context: &str, s: &'a str) -> &'a str {
    lang.catalog()
        .contextual(context, s)
        .unwrap_or_else(|| tr(lang, s))
}

/// Fill `{name}` placeholders in one pass. Unknown placeholders are left as written, and inserted
/// values are never read as templates again, so a file name containing `{n}` stays intact.
pub fn fmt(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((before, after_open)) = rest.split_once('{') {
        out.push_str(before);
        let Some((name, after_close)) = after_open.split_once('}') else {
            out.push('{');
            out.push_str(after_open);
            return out;
        };
        match args.iter().find(|(key, _)| *key == name) {
            Some((_, value)) => out.push_str(value),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = after_close;
    }
    out.push_str(rest);
    out
}

/// A plural-aware message: `one`/`other` are the English forms (with `{n}` where the count goes).
pub fn trn(lang: Lang, n: u64, one: &str, other: &str) -> String {
    let index = (lang.0.plural)(n);
    let text = lang
        .catalog()
        .plural(one, other, index)
        .unwrap_or(if n == 1 { one } else { other });
    fmt(text, &[("n", &lang.group_digits(n))])
}

/// [`Lang::group_digits`] in the current language.
pub fn group_digits(n: u64) -> String {
    current().group_digits(n)
}

/// [`trn`] in the current language.
pub fn tn(n: u64, one: &str, other: &str) -> String {
    trn(current(), n, one, other)
}

/// Translate an English UI string into the current language (see the crate docs).
#[macro_export]
macro_rules! tl {
    ($s:expr) => {
        $crate::t($s)
    };
}

/// [`tl!`] for a label whose meaning depends on where it appears; a catalog can translate it under
/// `context`, otherwise the plain translation is used.
#[macro_export]
macro_rules! tl_ctx {
    ($context:expr, $s:expr) => {
        $crate::tr_ctx($crate::current(), $context, $s)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_unknown_preferences_fall_back_to_it() {
        assert_eq!(Lang::from_pref(None), Lang::EN);
        assert_eq!(Lang::from_pref(Some("xx")), Lang::EN);
        assert_eq!(Lang::from_pref(Some("RU")).code(), "ru");
        assert_eq!(current(), Lang::EN);
        assert_eq!(t("Overview"), "Overview");
    }

    #[test]
    fn every_bundled_catalog_parses_without_errors() {
        for info in &LANGUAGES {
            let (_, errors) = Catalog::parse(info.source, info.plural_forms());
            assert!(errors.is_empty(), "{}: {errors:#?}", info.code);
        }
    }

    #[test]
    fn current_language_is_per_thread() {
        set_current(Lang::from_code("ru").unwrap());
        assert_eq!(t("Overview"), "Обзор");
        std::thread::spawn(|| assert_eq!(current(), Lang::EN))
            .join()
            .unwrap();
        set_current(Lang::EN);
    }

    #[test]
    fn missing_translation_shows_english() {
        let ru = Lang::from_code("ru").unwrap();
        assert_eq!(
            tr(ru, "A string nobody translated"),
            "A string nobody translated"
        );
        assert!(!has(ru, "A string nobody translated"));
    }

    #[test]
    fn context_selects_between_translations_of_one_word() {
        let ru = Lang::from_code("ru").unwrap();
        assert_eq!(tr_ctx(ru, "action", "Apply"), "Применить");
        assert_eq!(tr_ctx(ru, "unknown-context", "Overview"), "Обзор");
    }

    #[test]
    fn placeholders_are_filled_once_and_never_reinterpreted() {
        assert_eq!(fmt("{a} and {b}", &[("a", "{b}"), ("b", "x")]), "{b} and x");
        assert_eq!(fmt("{missing} stays", &[]), "{missing} stays");
        assert_eq!(fmt("open { brace", &[]), "open { brace");
    }

    #[test]
    fn plural_forms_follow_each_language() {
        let ru = Lang::from_code("ru").unwrap();
        for (n, expected) in [
            (1, "1 файл"),
            (2, "2 файла"),
            (5, "5 файлов"),
            (11, "11 файлов"),
            (21, "21 файл"),
            (112, "112 файлов"),
        ] {
            assert_eq!(trn(ru, n, "{n} file", "{n} files"), expected, "n = {n}");
        }
        assert_eq!(trn(Lang::EN, 1, "{n} file", "{n} files"), "1 file");
        assert_eq!(trn(Lang::EN, 3, "{n} file", "{n} files"), "3 files");
    }

    #[test]
    fn counts_are_grouped_the_way_each_language_writes_them() {
        let ru = Lang::from_code("ru").unwrap();
        assert_eq!(Lang::EN.group_digits(3549), "3,549");
        assert_eq!(Lang::EN.group_digits(1_000_000), "1,000,000");
        assert_eq!(
            ru.group_digits(3549),
            "3\u{a0}549",
            "a no-break space, not a comma"
        );
        assert_eq!(ru.group_digits(999), "999");
        assert_eq!(ru.group_digits(0), "0");
        assert_eq!(
            trn(ru, 1108, "{n} file", "{n} files"),
            "1\u{a0}108 файлов",
            "counts in messages are grouped too"
        );
    }
}
