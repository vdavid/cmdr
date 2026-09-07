//! Holds [`resolve_ui_locale`] to what the user's own Mac would do with their
//! English.
//!
//! `match_shipped` walks CLDR's parent data; macOS walks Apple's copy of roughly
//! the same idea, inside `CFBundleCopyLocalizationsForPreferences`. That one is a
//! pure function over two arrays of tags, with no bundle involved, so we can ask
//! it the exact question we answer ("given these three English catalogs, which
//! does an `en-NZ` reader want?") and compare.
//!
//! The invariant is one-directional on purpose: **we are never further from home
//! than macOS puts them.** If Finder gives a reader a regional overlay, Cmdr has
//! to as well. The converse is fine and happens: CLDR knows about English in
//! more places than Apple's table does, so a handful of regions get `en-GB` from
//! us and US English from Apple. Being more precise than the OS is not a bug.
//!
//! macOS-only, because CFBundle is. The rule it guards is cross-platform, and
//! the plain unit tests in `mod.rs` cover it everywhere.

use super::{SHIPPED_LOCALES, base_language, resolve_ui_locale};
use objc2_foundation::{NSArray, NSBundle, NSLocale, NSString};

/// Tags where macOS reaches a regional overlay and we stop at base `en`.
///
/// Every one of them is a country CLDR ships no English locale for, so
/// `parentLocales.json` says nothing and the tag truncates to `en`. Apple's
/// table is broader here, mostly folding "English in a European country" toward
/// `en-GB`. None of these is offered in the macOS language picker, so the
/// practical exposure is a hand-set POSIX `LANG`.
///
/// This is an EQUALITY, not an allowlist: a tag that starts or stops diverging
/// fails the test, which is the point. If one shows up in the picker, the fix is
/// a CLDR bump, not another line here.
const KNOWN_GAPS: &[&str] = &["en-AL", "en-BD", "en-BG", "en-BN", "en-GR", "en-RU"];

/// The English catalogs we ship, spelled as CFBundle wants them.
fn english_catalogs() -> Vec<String> {
    SHIPPED_LOCALES
        .iter()
        .filter(|entry| base_language(entry.tag).eq_ignore_ascii_case("en"))
        .map(|entry| entry.tag.to_string())
        .collect()
}

/// Every English locale identifier this macOS knows, BCP-47 spelled.
///
/// `NSLocale` reports POSIX-ish underscores (`en_NZ`); `AppleLanguages` reports
/// dashes. We compare against the dashed form because that's what reaches the
/// resolver in production.
fn english_locale_identifiers() -> Vec<String> {
    NSLocale::availableLocaleIdentifiers()
        .iter()
        .map(|identifier| identifier.to_string().replace('_', "-"))
        .filter(|identifier| base_language(identifier).eq_ignore_ascii_case("en"))
        .collect()
}

/// Which of `catalogs` macOS itself would open for a reader who asked for `tag`.
fn macos_pick(tag: &str, catalogs: &[String]) -> Option<String> {
    let available = NSArray::from_retained_slice(
        &catalogs
            .iter()
            .map(|catalog| NSString::from_str(catalog))
            .collect::<Vec<_>>(),
    );
    let preferences = NSArray::from_retained_slice(&[NSString::from_str(tag)]);
    NSBundle::preferredLocalizationsFromArray_forPreferences(&available, Some(&preferences))
        .iter()
        .next()
        .map(|picked| picked.to_string())
}

#[test]
fn a_regional_english_never_lands_further_from_home_than_macos_puts_it() {
    let catalogs = english_catalogs();
    assert!(
        catalogs.iter().any(|tag| tag != "en"),
        "this test is meaningless without a regional English catalog to reach"
    );

    let mut gaps: Vec<String> = Vec::new();
    for tag in english_locale_identifiers() {
        let Some(theirs) = macos_pick(&tag, &catalogs) else {
            continue;
        };
        let ours = resolve_ui_locale(std::slice::from_ref(&tag), SHIPPED_LOCALES).unwrap_or_else(|| "en".to_string());
        // Only the direction that hurts a reader: macOS found them a regional
        // catalog and we dropped them on US English instead.
        if theirs != "en" && ours == "en" {
            gaps.push(tag);
        }
    }
    gaps.sort();

    assert_eq!(
        gaps, KNOWN_GAPS,
        "the set of English tags where macOS reaches an overlay and we don't has moved; \
         see KNOWN_GAPS for what the list means and how to react"
    );
}

#[test]
fn the_new_zealand_case_that_started_all_this_reaches_an_overlay_on_both_sides() {
    // A named regression anchor for the bug report: an `en-NZ` Mac was reading
    // "Trash". Asserting "not `en`" rather than a specific catalog is deliberate,
    // because Apple lands on `en-AU` and CLDR on `en-GB`, and both say "Bin".
    let catalogs = english_catalogs();
    assert_ne!(macos_pick("en-NZ", &catalogs).as_deref(), Some("en"));
    assert_ne!(
        resolve_ui_locale(&["en-NZ".to_string()], SHIPPED_LOCALES).as_deref(),
        Some("en")
    );
}
