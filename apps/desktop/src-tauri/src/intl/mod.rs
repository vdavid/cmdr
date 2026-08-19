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

use std::cmp::Reverse;

// The catalog table is generated from the message-catalog directories. The
// `#[path]` keeps the `.gen.rs` spelling the repo already uses to mark a
// generated artifact (`keys.gen.ts`, `bindings.ts`).
#[path = "shipped_locales.gen.rs"]
mod shipped_locales;

use shipped_locales::SHIPPED_LOCALES;

/// One catalog we ship, plus the script facts the resolver's guard needs.
///
/// The scripts come from CLDR's likely-subtags data, which Rust has no runtime
/// access to; `apps/desktop/scripts/gen-shipped-locales.ts` asks Node's `Intl`
/// for them at build time and emits [`SHIPPED_LOCALES`].
#[derive(Debug, Clone, Copy)]
pub(crate) struct ShippedLocale {
    /// The catalog directory name, verbatim (`zh`, `en`). Compared
    /// case-insensitively; handed back to the caller spelled as it is here,
    /// because the frontend keys its catalog map on the directory name.
    pub(crate) tag: &'static str,
    /// The likely script of `tag` itself, lowercased: what a reader of this
    /// catalog reads. Our `zh` catalog is Simplified, so `"hans"`.
    pub(crate) script: &'static str,
    /// The likely script of the bare language subtag, lowercased. Differs from
    /// [`Self::script`] only when `tag` names a script itself (`zh-Hant`).
    pub(crate) default_script: &'static str,
    /// Regions of that language whose likely script differs from
    /// [`Self::default_script`], lowercased: `zh` carries `("tw", "hant")` and
    /// friends. Empty for every Latin-script language.
    pub(crate) region_scripts: &'static [(&'static str, &'static str)],
}

/// Tauri command: the locale the UI should run in while the language setting is
/// `'system'`, or `None` when there's no OS preference list to read.
///
/// `None` is a platform answer, not a "nothing matched" answer: off macOS the
/// webview's own default stands, which is the right behavior on Linux. On macOS
/// we always answer, falling back to English, because that IS the answer when
/// the user reads no language we ship.
#[tauri::command]
#[specta::specta]
pub fn get_ui_locale() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let preferences = crate::system_strings::apple_languages();
        Some(resolve_ui_locale(&preferences, SHIPPED_LOCALES).unwrap_or_else(|| "en".to_string()))
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// The locale the UI should use, or `None` to stay on English.
///
/// Walks `preferences` in order and takes the first catalog the user can read:
/// same language, same script. Exhausts one preference before advancing to the
/// next, because the order is the user's own fallback plan.
///
/// Returning `None` means "nothing matched"; the caller uses English. That is
/// NOT the same as matching `en`, which stops the walk deliberately: a user who
/// listed English above Swedish wants English, not the next-best translation.
pub(crate) fn resolve_ui_locale(preferences: &[String], shipped: &[ShippedLocale]) -> Option<String> {
    preferences
        .iter()
        .find_map(|pref| match_shipped(&normalize(pref), shipped))
}

/// A preference tag in a shape we can compare: lowercase, `_` separators folded
/// to `-`. macOS reports BCP-47 (`hu-HU`), but the same list reaches us through
/// paths that use the POSIX `hu_HU` spelling, and neither casing is guaranteed.
fn normalize(tag: &str) -> String {
    tag.trim().replace('_', "-").to_ascii_lowercase()
}

/// The base language subtag of a tag (`zh-hant-tw` → `zh`).
fn base_language(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

/// The catalog an already-[`normalize`]d preference should open, in the
/// catalog's own spelling.
///
/// One rule, two halves: the catalog has to be the same LANGUAGE (so `pt-PT`
/// reaches the Brazilian `pt` catalog and `en-GB` reaches US `en`, deliberately)
/// and the same SCRIPT (so `zh-Hant-TW` does NOT reach the Simplified `zh` one).
/// The script half is the guard: a fallback is only a kindness when it lands
/// somewhere the reader can actually read, and Simplified Chinese in front of a
/// Traditional reader is worse than English, a language they at least chose to
/// list. Dialect friction is a papercut a later catalog fixes; an unreadable
/// script is a wall. ❌ Don't "fix" this by blocking regional fallback too.
///
/// Among the catalogs that qualify, the most specific one wins: with both `pt`
/// and `pt-BR` shipped, `pt-BR` takes `pt-BR` and `pt-PT` takes plain `pt`.
fn match_shipped(tag: &str, shipped: &[ShippedLocale]) -> Option<String> {
    let language = base_language(tag);
    shipped
        .iter()
        .filter(|entry| base_language(entry.tag).eq_ignore_ascii_case(language))
        .filter(|entry| script_of(tag, entry).eq_ignore_ascii_case(entry.script))
        .max_by_key(|entry| (shared_subtags(tag, entry.tag), Reverse(entry.tag.len())))
        .map(|entry| entry.tag.to_string())
}

/// The script an already-[`normalize`]d `tag` is written in, per CLDR's likely
/// subtags, read off `entry`'s generated facts (which are language-level, so
/// any entry for the same language answers identically).
///
/// Three sources, most explicit first: the tag's own script subtag
/// (`zh-hant-tw`), then its region when that region implies a different script
/// (`zh-tw`), then the language's default (`zh` alone is Simplified).
fn script_of<'a>(tag: &'a str, entry: &'a ShippedLocale) -> &'a str {
    let mut subtags = tag.split('-').skip(1).peekable();
    if let Some(script) = subtags.next_if(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic())) {
        return script;
    }
    subtags
        .next_if(|part| is_region_subtag(part))
        .and_then(|region| {
            entry
                .region_scripts
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(region))
        })
        .map_or(entry.default_script, |(_, script)| script)
}

/// Whether a subtag is a region: two letters (`tw`) or three digits (`419`).
fn is_region_subtag(part: &str) -> bool {
    (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
        || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
}

/// How specifically `entry_tag` matches `tag`: the count of leading subtags they
/// share, or 0 unless `entry_tag` is a subtag-aligned prefix of `tag`. Against
/// `pt-BR`, the `pt-BR` catalog scores 2 and `pt` scores 1; against `pt-PT`,
/// `pt-BR` scores 0 and `pt` still scores 1.
fn shared_subtags(tag: &str, entry_tag: &str) -> usize {
    let mut theirs = entry_tag.split('-');
    let mut ours = tag.split('-');
    let mut shared = 0;
    loop {
        match (theirs.next(), ours.next()) {
            (None, _) => return shared,
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => shared += 1,
            _ => return 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real table, so the tests exercise the data the app actually ships
    /// rather than a hand-made stand-in that can drift from it.
    const SHIPPED: &[ShippedLocale] = SHIPPED_LOCALES;

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

    #[test]
    fn a_traditional_reader_never_lands_on_the_simplified_catalog() {
        // Our `zh` catalog is Simplified. For a Traditional reader it's worse
        // than English, which is at least a language they chose to list.
        assert_eq!(resolve_ui_locale(&prefs(&["zh-Hant-TW"]), SHIPPED), None);
        // Explicit script, no region.
        assert_eq!(resolve_ui_locale(&prefs(&["zh-Hant"]), SHIPPED), None);
        // No script subtag: the REGION says Traditional (CLDR likely subtags).
        assert_eq!(resolve_ui_locale(&prefs(&["zh-TW"]), SHIPPED), None);
        assert_eq!(resolve_ui_locale(&prefs(&["zh-HK"]), SHIPPED), None);
        assert_eq!(resolve_ui_locale(&prefs(&["zh-MO"]), SHIPPED), None);
    }

    #[test]
    fn a_blocked_script_falls_through_to_the_next_preference() {
        // The guard doesn't end the walk, it only rules out one catalog: the
        // user's next choice still gets its turn.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hant-TW", "sv-SE"]), SHIPPED),
            Some("sv".to_string())
        );
    }

    #[test]
    fn the_guard_does_not_block_the_common_simplified_case() {
        assert_eq!(resolve_ui_locale(&prefs(&["zh-CN"]), SHIPPED), Some("zh".to_string()));
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hans-CN"]), SHIPPED),
            Some("zh".to_string())
        );
        assert_eq!(resolve_ui_locale(&prefs(&["zh-SG"]), SHIPPED), Some("zh".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["zh"]), SHIPPED), Some("zh".to_string()));
    }

    #[test]
    fn the_guard_is_about_legibility_not_dialect() {
        // Regional fallback is WANTED: `pt-PT` reading Brazilian Portuguese, or
        // `en-GB` reading "Trash" and `-ize`, is a papercut a later catalog
        // fixes. An unreadable script is a wall. Don't "fix" this by blocking
        // regional fallback.
        assert_eq!(resolve_ui_locale(&prefs(&["pt-PT"]), SHIPPED), Some("pt".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["en-GB"]), SHIPPED), Some("en".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["de-AT"]), SHIPPED), Some("de".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["es-419"]), SHIPPED), Some("es".to_string()));
        assert_eq!(
            resolve_ui_locale(&prefs(&["fr-Latn-CA"]), SHIPPED),
            Some("fr".to_string())
        );
    }

    #[test]
    fn a_catalog_that_names_its_own_script_guards_against_the_language_default() {
        // Nothing we ship today names a script, so this pins the mirror case a
        // future `zh-Hant` catalog would need: a Simplified reader must not
        // land on it just because the base language matches.
        const ZH_HANT: &[ShippedLocale] = &[ShippedLocale {
            tag: "zh-Hant",
            script: "hant",
            default_script: "hans",
            region_scripts: &[("tw", "hant"), ("hk", "hant")],
        }];
        assert_eq!(resolve_ui_locale(&prefs(&["zh-CN"]), ZH_HANT), None);
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hant-TW"]), ZH_HANT),
            Some("zh-Hant".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-TW"]), ZH_HANT),
            Some("zh-Hant".to_string())
        );
    }

    #[test]
    fn the_most_specific_catalog_of_a_language_wins() {
        // Nothing we ship today has a regional sibling; this pins the rule for
        // the day `pt-BR` or `en-GB` joins the roster as a wave-2 variant.
        const PT: &[ShippedLocale] = &[
            ShippedLocale {
                tag: "pt",
                script: "latn",
                default_script: "latn",
                region_scripts: &[],
            },
            ShippedLocale {
                tag: "pt-BR",
                script: "latn",
                default_script: "latn",
                region_scripts: &[],
            },
        ];
        assert_eq!(resolve_ui_locale(&prefs(&["pt-BR"]), PT), Some("pt-BR".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["pt-PT"]), PT), Some("pt".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["pt"]), PT), Some("pt".to_string()));
    }

    /// Catalog directories that exist but are never a language anyone reads.
    /// Mirrors `NON_LOCALE_DIRS` + `PSEUDO_LOCALE` in the generator.
    const NON_CATALOG_DIRS: &[&str] = &["screenshots", "en-XA"];

    /// The catalog directory names on disk, which are what we ship.
    fn catalog_dirs_on_disk() -> Vec<String> {
        let messages = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/intl/messages");
        let entries = std::fs::read_dir(&messages).expect("the message catalogs ship in-tree next to the crate");
        let mut dirs: Vec<String> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_str()?.to_string();
                let is_dir = entry.file_type().ok()?.is_dir();
                (is_dir && !NON_CATALOG_DIRS.contains(&name.as_str())).then_some(name)
            })
            .collect();
        dirs.sort();
        dirs
    }

    #[test]
    fn the_generated_table_covers_every_shipped_catalog() {
        // Without this, adding a catalog dir and forgetting to regenerate would
        // leave that language silently unreachable AND unguarded, which is the
        // exact failure the script guard exists to prevent.
        let mut table: Vec<String> = SHIPPED_LOCALES.iter().map(|entry| entry.tag.to_string()).collect();
        table.sort();
        assert_eq!(
            table,
            catalog_dirs_on_disk(),
            "the shipped-locale table is stale; run `pnpm intl:shipped-locales` from `apps/desktop/`"
        );
    }

    #[test]
    fn every_table_entry_carries_its_script_facts() {
        for entry in SHIPPED_LOCALES {
            assert!(!entry.script.is_empty(), "{} has no script", entry.tag);
            assert!(!entry.default_script.is_empty(), "{} has no default script", entry.tag);
        }
    }

    #[test]
    fn the_pseudolocale_is_not_selectable() {
        // `en-XA` is accented, inflated English for overflow testing. A tester
        // whose app came up in it would file a very confusing bug.
        assert!(
            !SHIPPED_LOCALES
                .iter()
                .any(|entry| entry.tag.eq_ignore_ascii_case("en-XA"))
        );
        assert_eq!(resolve_ui_locale(&prefs(&["en-XA"]), SHIPPED), Some("en".to_string()));
    }
}
