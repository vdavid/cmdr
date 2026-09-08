//! How file names rank against each other.
//!
//! One Unicode collator, shared by every list the app orders. It answers the
//! same question two ways: [`NameCollator::compare`] for a one-off comparison,
//! and [`NameCollator::key`] for a byte key that a bulk sort builds once per row
//! and then compares directly. The two agree by construction (a sort key IS the
//! collation weights, serialized), and `sorting_test` pins that they do.
//!
//! Rationale, the alternatives weighed, and the measurements: `DETAILS.md`
//! § "Collating names".

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use icu_collator::options::{CollatorOptions, Strength};
use icu_collator::preferences::CollationNumericOrdering;
use icu_collator::{Collator, CollatorBorrowed, CollatorPreferences};
use icu_locale_core::Locale;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

/// A file name's collation weights, serialized to bytes that order the same way
/// the collator does.
///
/// ❌ Never persist one. The bytes are ICU4X's internal encoding: a library or
/// CLDR data update may re-tune them, which is harmless while every key in a
/// comparison was built by the running binary, and silently wrong the moment an
/// old key is compared against a fresh one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameKey(Vec<u8>);

impl NameKey {
    /// Ranks two keys. Equal means the two names carry the same collation
    /// weights, which is what the NFC and NFD spellings of one name do; the
    /// caller breaks that tie.
    pub fn compare(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

/// Orders names for one locale.
pub struct NameCollator {
    inner: CollatorBorrowed<'static>,
}

impl NameCollator {
    fn new(locale: &str) -> Self {
        Self {
            inner: build(locale)
                .unwrap_or_else(|| build("und").expect("ICU4X ships compiled collation data for the root locale")),
        }
    }

    /// Ranks two names, most-significant difference first, falling back to the
    /// raw bytes so two distinct names never tie.
    ///
    /// The tiebreak is what keeps the order total. Without it the NFC and NFD
    /// spellings of one name compare `Equal`, and the watcher's re-read could
    /// order them either way between two passes, which `compute_diff` would
    /// report as a `DiffChangeType::Move` that nothing actually moved.
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.inner.compare(a, b).then_with(|| a.as_bytes().cmp(b.as_bytes()))
    }

    /// Builds `name`'s collation key, for a caller that will compare it many
    /// times. Pair it with the same raw-bytes tiebreak [`Self::compare`] applies.
    pub fn key(&self, name: &str) -> NameKey {
        let mut bytes = Vec::new();
        // `Vec<u8>` is an infallible `CollationKeySink`, so this cannot fail.
        let Ok(()) = self.inner.write_sort_key_to(name, &mut bytes);
        NameKey(bytes)
    }
}

/// Builds a collator, or `None` when the locale tag doesn't parse.
///
/// An unrecognized-but-well-formed tag needs no handling here: ICU4X falls back
/// through the locale chain to root on its own.
fn build(locale: &str) -> Option<CollatorBorrowed<'static>> {
    let parsed: Locale = locale.parse().ok()?;
    let mut prefs = CollatorPreferences::from(&parsed);

    // Digit runs compare by numeric value, so `img_9` precedes `img_10`.
    prefs.numeric_ordering = Some(CollationNumericOrdering::True);

    let mut options = CollatorOptions::default();
    // Case is a tertiary difference: it breaks ties between otherwise equal
    // names rather than driving the order, so `apple` still sorts beside
    // `Apple` and nowhere near `Zebra`.
    options.strength = Some(Strength::Tertiary);

    Collator::try_new(prefs, options).ok()
}

/// The collator for the locale the user reads now, built once per locale.
///
/// A language switch is picked up here rather than pushed: `intl::active_locale`
/// re-resolves on an OS locale change, so the next sort asks for a collator for
/// the new language. Listings already sorted keep their order until something
/// re-reads them, the same way they do when the sort column changes.
///
/// Locales are a bounded set (13 catalogs ship), so the cache never grows past
/// a handful of entries.
pub fn active_collator() -> Arc<NameCollator> {
    static CACHE: RwLock<Option<HashMap<String, Arc<NameCollator>>>> = RwLock::new(None);

    let locale = crate::intl::active_locale();

    if let Some(cached) = CACHE
        .read_ignore_poison()
        .as_ref()
        .and_then(|by_locale| by_locale.get(&locale))
    {
        return Arc::clone(cached);
    }

    let collator = Arc::new(NameCollator::new(&locale));
    CACHE
        .write_ignore_poison()
        .get_or_insert_with(HashMap::new)
        .insert(locale, Arc::clone(&collator));
    collator
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the key: it has to rank the same way `compare` does,
    /// or the bulk sort and the incremental insert disagree about one listing.
    #[test]
    fn a_key_ranks_names_the_way_compare_does() {
        let collator = NameCollator::new("en");
        let names = [
            "_cameras",
            "2023",
            "2026-08-27 David pixel pics",
            "Da\u{301}vid.txt",
            "D\u{e1}vid.txt",
            "D\u{e1}viad.txt",
            "\u{e1}rv\u{ed}zt\u{171}r\u{151}.txt",
            "alma.txt",
            "Alma.txt",
            "img_2.png",
            "img_10.png",
            "\u{f6}ver.txt",
            "zebra.txt",
        ];

        for a in names {
            for b in names {
                let via_key = collator
                    .key(a)
                    .compare(&collator.key(b))
                    .then_with(|| a.as_bytes().cmp(b.as_bytes()));
                assert_eq!(via_key, collator.compare(a, b), "ranking {a:?} against {b:?}");
            }
        }
    }

    /// Distinct names must never tie, or a re-sort can reorder them freely.
    #[test]
    fn distinct_names_never_tie() {
        let collator = NameCollator::new("en");
        // Same name, both spellings: equal collation weights, different bytes.
        assert_eq!(collator.compare("D\u{e1}vid", "D\u{e1}vid"), Ordering::Equal);
        assert_ne!(collator.compare("D\u{e1}vid", "Da\u{301}vid"), Ordering::Equal);
    }

    /// An unparseable tag must not take the collator down with it.
    #[test]
    fn a_broken_locale_tag_falls_back_to_root() {
        let collator = NameCollator::new("not a locale");
        assert_eq!(collator.compare("a.txt", "b.txt"), Ordering::Less);
    }

    /// Swedish seats `ö` after `z`; Hungarian and English seat it beside `o`.
    /// Proves the active locale actually reaches the collator.
    #[test]
    fn the_locale_decides_where_an_accented_letter_sits() {
        let swedish = NameCollator::new("sv");
        assert_eq!(swedish.compare("\u{f6}ver.txt", "zebra.txt"), Ordering::Greater);

        let hungarian = NameCollator::new("hu");
        assert_eq!(hungarian.compare("\u{f6}ver.txt", "zebra.txt"), Ordering::Less);
    }
}
