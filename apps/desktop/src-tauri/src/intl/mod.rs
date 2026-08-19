//! Which language the app speaks, resolved from the user's OS preferences.
//!
//! macOS hands us an ORDERED list of languages the user reads
//! ([`crate::system_strings::apple_languages`]). This module answers the one
//! question that list is for: given the catalogs we actually ship, which one
//! should the app run in?
//!
//! The answer lives in Rust rather than in the frontend because two of its three
//! consumers run before the webview exists: the native menu bar (built during
//! `setup`) and the already-running-instance alert (fires before any window).
//! A second resolver in Rust for those, with the real one in TypeScript, would
//! be two implementations of one rule, drifting apart.

/// The locale the UI should use, or `None` to stay on English.
///
/// Walks `preferences` in order and takes the first entry we ship, trying the
/// full tag before its base language (`fr-CA` → `fr`) so a regional variant
/// lands on its parent rather than skipping the language entirely.
///
/// Returning `None` means "nothing matched"; the caller uses English. That is
/// NOT the same as matching `en`, which stops the walk deliberately: a user who
/// listed English above Swedish wants English, not the next-best translation.
// `expect` only outside test builds: the tests below DO call this, so an
// unconditional `expect(dead_code)` reads as unfulfilled in the lib-test unit
// and `-D unfulfilled-lint-expectations` turns that into a build failure.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired to its callers later in this milestone; tests drive it meanwhile"
    )
)]
pub(crate) fn resolve_ui_locale(preferences: &[String], shipped: &[&str]) -> Option<String> {
    preferences.iter().find_map(|pref| {
        let tag = normalize(pref);
        // Full tag before base language, so `fr-CA` lands on `fr` rather than
        // skipping French to try the user's SECOND choice. Exhaust one
        // preference before moving to the next; the order is the user's answer.
        let base = base_language(&tag);
        [tag.as_str(), base]
            .into_iter()
            .find_map(|candidate| match_shipped(candidate, shipped))
    })
}

/// A preference tag in a shape we can compare: lowercase, `_` separators folded
/// to `-`. macOS reports BCP-47 (`hu-HU`), but the same list reaches us through
/// paths that use the POSIX `hu_HU` spelling, and neither casing is guaranteed.
fn normalize(tag: &str) -> String {
    tag.trim().replace('_', "-").to_ascii_lowercase()
}

/// The base language subtag of an already-[`normalize`]d tag (`zh-hant-tw` → `zh`).
fn base_language(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

/// The shipped tag matching `candidate`, in the catalog's own spelling.
fn match_shipped(candidate: &str, shipped: &[&str]) -> Option<String> {
    shipped
        .iter()
        .find(|s| normalize(s) == candidate)
        .map(|s| (*s).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nine shipped locales plus `en`, as `resolve_ui_locale` sees them.
    const SHIPPED: &[&str] = &["de", "en", "es", "fr", "hu", "nl", "pt", "sv", "vi", "zh"];

    fn prefs(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| (*t).to_string()).collect()
    }

    #[test]
    fn takes_the_first_preference_we_ship() {
        assert_eq!(
            resolve_ui_locale(&prefs(&["hu-HU", "en-US"]), SHIPPED),
            Some("hu".to_string())
        );
    }

    #[test]
    fn falls_through_to_a_later_preference_when_the_first_is_unshipped() {
        // The case the pre-Rust code structurally could not express: the webview
        // exposed ONE tag, so a user's second choice was unreachable.
        assert_eq!(
            resolve_ui_locale(&prefs(&["pl-PL", "sv-SE"]), SHIPPED),
            Some("sv".to_string())
        );
    }

    #[test]
    fn a_regional_variant_falls_back_to_its_base_language() {
        assert_eq!(resolve_ui_locale(&prefs(&["fr-CA"]), SHIPPED), Some("fr".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["pt-PT"]), SHIPPED), Some("pt".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["en-GB"]), SHIPPED), Some("en".to_string()));
    }

    #[test]
    fn the_base_fallback_happens_before_advancing_to_the_next_preference() {
        // `fr-CA` must resolve to `fr`, NOT skip ahead to Swedish.
        assert_eq!(
            resolve_ui_locale(&prefs(&["fr-CA", "sv-SE"]), SHIPPED),
            Some("fr".to_string())
        );
    }

    #[test]
    fn english_stops_the_walk() {
        // Listing English above Swedish is a choice, not an absence of one.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-US", "sv-SE"]), SHIPPED),
            Some("en".to_string())
        );
    }

    #[test]
    fn tag_case_does_not_matter() {
        assert_eq!(resolve_ui_locale(&prefs(&["HU-hu"]), SHIPPED), Some("hu".to_string()));
    }

    #[test]
    fn no_match_returns_none_so_the_caller_uses_english() {
        assert_eq!(resolve_ui_locale(&prefs(&["pl-PL", "cs-CZ"]), SHIPPED), None);
    }

    #[test]
    fn an_empty_preference_list_returns_none() {
        assert_eq!(resolve_ui_locale(&[], SHIPPED), None);
    }
}
