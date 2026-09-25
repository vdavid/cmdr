//! Which shipped catalog a reader gets, and which catalogs a shipped one
//! inherits from: the pure matching rules, over the generated tables.
//!
//! Two questions, one rule set (same language, same script, CLDR's parents):
//! [`resolve_ui_locale`] turns an OS preference list into a catalog, and
//! [`inheritance_chain`] says where a catalog's missing keys come from. No OS
//! reads here: `mod.rs` fetches the preferences and hands them in, which is
//! what lets every rule below run under plain unit tests on any platform.

use super::ShippedLocale;
use super::shipped_locales::PARENT_LOCALES;
#[cfg(test)]
use super::shipped_locales::SHIPPED_LOCALES;

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
pub(super) fn normalize(tag: &str) -> String {
    tag.trim().replace('_', "-").to_ascii_lowercase()
}

/// The base language subtag of a tag (`zh-hant-tw` → `zh`).
pub(super) fn base_language(tag: &str) -> &str {
    tag.split('-').next().unwrap_or(tag)
}

/// The catalog an already-[`normalize`]d preference should open, in the
/// catalog's own spelling.
///
/// Two gates, then a walk.
///
/// The gates say which catalogs this reader can read at all: the same LANGUAGE
/// (so `pt-PT` reaches the Brazilian `pt` catalog and `en-CA` reaches US `en`,
/// deliberately) and the same SCRIPT (so `zh-Hant-TW` does NOT reach the
/// Simplified `zh` one). The script gate is the guard: a fallback is only a
/// kindness when it lands somewhere the reader can actually read, and Simplified
/// Chinese in front of a Traditional reader is worse than English, a language
/// they at least chose to list. Dialect friction is a papercut a later catalog
/// fixes; an unreadable script is a wall. ❌ Don't "fix" this by blocking
/// regional fallback too.
///
/// The walk then takes the first node of [`ancestor_chain`] one of those
/// catalogs answers for, so the most specific catalog on the way home wins:
/// `en-GB` opens the British overlay, `en-NZ` reaches it through CLDR's
/// `en-001`, and `en-CA` opens plain `en`. Canonical rationale for all of it:
/// `DETAILS.md` § The script guard, and why regional fallback survives it.
fn match_shipped(tag: &str, shipped: &[ShippedLocale]) -> Option<String> {
    let language = base_language(tag);
    let same_language: Vec<&ShippedLocale> = shipped
        .iter()
        .filter(|entry| base_language(entry.tag).eq_ignore_ascii_case(language))
        .collect();
    // The script facts are language-level, so every entry of this language
    // answers identically; asking just needs one of them in hand. No entry at
    // all means we ship nothing for this language, which is the `None` case.
    let script = script_of(tag, same_language.first()?);
    let readable: Vec<&ShippedLocale> = same_language
        .into_iter()
        .filter(|entry| entry.script.eq_ignore_ascii_case(script))
        .collect();

    ancestor_chain(tag, script)
        .iter()
        .find_map(|node| readable.iter().find(|entry| entry.answers_for(node)))
        .map(|entry| entry.tag.to_string())
}

/// The nodes to try for an already-[`normalize`]d `tag`, most specific first:
/// the tag itself, then its ancestors, then the tag CLDR's likely subtags would
/// have maximized it to.
///
/// An ancestor is [`PARENT_LOCALES`]'s override where CLDR states one, and the
/// tag minus its last subtag otherwise. The overrides are the whole point:
/// `en-nz` truncates to `en`, US English, while CLDR parents it to `en-001`,
/// the World English our `en-GB` catalog answers for. Every regional English
/// CLDR knows about reaches an overlay this way, so ❌ don't add a region table
/// beside this: that's the thing CLDR's data replaces.
///
/// A `und` override means the locale has NO parent, and the walk stops there
/// rather than truncating: it's how CLDR spells the wall between `zh-Hant` and
/// Simplified `zh`.
///
/// The `<language>-<script>` node comes last, and only when the walk hasn't
/// already produced it. It's what lets `zh-tw` reach the `zh-Hant` catalog: the
/// tag names no script, its ancestors are `zh`, and only maximization says out
/// loud that this reader reads Traditional.
fn ancestor_chain(tag: &str, script: &str) -> Vec<String> {
    // CLDR's parent graph is acyclic and every truncation step shortens the tag,
    // so this cap can only fire on a corrupted table. It's here so that such a
    // table costs a wrong answer rather than a hung app.
    const MAX_DEPTH: usize = 8;

    let mut chain = vec![tag.to_string()];
    let mut current = tag.to_string();
    while chain.len() < MAX_DEPTH {
        let parent = match PARENT_LOCALES
            .iter()
            .find(|(child, _)| child.eq_ignore_ascii_case(&current))
        {
            Some(&(_, "und")) => break,
            Some(&(_, parent)) => parent.to_string(),
            None => match current.rsplit_once('-') {
                Some((head, _)) => head.to_string(),
                None => break,
            },
        };
        chain.push(parent.clone());
        current = parent;
    }

    let maximized = format!("{}-{script}", base_language(tag));
    if !chain.iter().any(|node| node.eq_ignore_ascii_case(&maximized)) {
        chain.push(maximized);
    }
    chain
}

/// The script an already-[`normalize`]d `tag` is written in, per CLDR's likely
/// subtags, read off `entry`'s generated facts (which are language-level, so
/// any entry for the same language answers identically).
///
/// Three sources, most explicit first: the tag's own script subtag
/// (`zh-hant-tw`), then its region when that region implies a different script
/// (`zh-tw`), then the language's default (`zh` alone is Simplified).
///
/// ❌ Don't read the subtags by POSITION. An extended-language subtag shifts
/// everything right (`zh-yue-hant-hk`), so peeking only at slot two sees `yue`,
/// matches neither shape, and quietly answers "Simplified" for a tag that says
/// `hant` out loud. Scanning is also why a singleton ends the walk: `x-hant` is
/// private-use payload, not a script.
fn script_of<'a>(tag: &'a str, entry: &'a ShippedLocale) -> &'a str {
    let mut region = None;
    for part in tag.split('-').skip(1) {
        if part.len() == 1 {
            break; // A singleton opens an extension or private-use sequence.
        }
        if is_script_subtag(part) {
            return part;
        }
        if region.is_none() && is_region_subtag(part) {
            region = Some(part);
        }
    }
    region
        .and_then(|region| {
            entry
                .region_scripts
                .iter()
                .find(|(candidate, _)| candidate.eq_ignore_ascii_case(region))
        })
        .map_or(entry.default_script, |(_, script)| script)
}

/// Whether a subtag is a script: four letters (`hant`). Unambiguous, because a
/// four-character BCP-47 variant has to start with a digit.
fn is_script_subtag(part: &str) -> bool {
    part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic())
}

/// Whether a subtag is a region: two letters (`tw`) or three digits (`419`).
fn is_region_subtag(part: &str) -> bool {
    (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic()))
        || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit()))
}

/// The shipped catalogs `tag` inherits the keys it lacks from, nearest first:
/// its shipped ancestors written in the same script. Empty for a full
/// translation (which owes every key itself) and for a tag we don't ship.
/// Callers append `en` as the final fallback, as the frontend does.
///
/// The same rule as the frontend's `inheritableAncestors`
/// (`apps/desktop/src/lib/intl/locale-inheritance.ts`), so the two layers can't
/// disagree about what a catalog owes or where its missing keys come from:
/// `es-419` inherits `es`, `en-GB` inherits `en`, while `zh-Hant` inherits
/// NOTHING (Simplified is a wall) and is therefore a full translation despite
/// reading like a variant of `zh`.
///
/// [`crate::intl::menu_t`] walks it at runtime, so an overlay's native menu
/// reads its base language for every label it doesn't fork. ❌ Don't shortcut
/// that walk to `en`: it's invisible for `en-GB`, and an `es-419` menu bar in
/// English.
pub(crate) fn inheritance_chain(tag: &str, shipped: &[ShippedLocale]) -> Vec<&'static str> {
    let Some(entry) = shipped.iter().find(|entry| entry.tag.eq_ignore_ascii_case(tag)) else {
        return Vec::new();
    };
    let mut chain: Vec<&'static str> = shipped
        .iter()
        .filter(|candidate| !candidate.tag.eq_ignore_ascii_case(entry.tag))
        .filter(|candidate| is_ancestor_tag(candidate.tag, entry.tag))
        .filter(|candidate| candidate.script.eq_ignore_ascii_case(entry.script))
        .map(|candidate| candidate.tag)
        .collect();
    chain.sort_by_key(|ancestor| std::cmp::Reverse(ancestor.len()));
    chain
}

/// The shipped catalog `tag` is an OVERLAY of, if any: the nearest entry of
/// its [`inheritance_chain`]. `None` for a full translation.
///
/// ❌ Don't classify a catalog by how many keys it carries. A full translation
/// that lost one key would reclassify itself as an overlay and start being
/// held to the weaker contract.
///
/// Test-only: it lives here, beside the table it reads, so the guard in
/// `native_strings.rs` derives the classification from the shipped facts
/// instead of guessing at one.
#[cfg(test)]
pub(crate) fn overlay_base(tag: &str, shipped: &[ShippedLocale]) -> Option<&'static str> {
    inheritance_chain(tag, shipped).first().copied()
}

/// Whether `ancestor` is a subtag-aligned prefix of `tag`: `pt` is one of
/// `pt-BR`, `pt-BR` is not one of `pt-PT`, and `p` is not one of `pt`.
///
/// This is plain truncation, matching the frontend's `ancestorTags`. It is NOT
/// how a PREFERENCE finds its catalog ([`ancestor_chain`] follows CLDR's parent
/// overrides for that); it answers the different question
/// [`inheritance_chain`] asks, which is what one shipped catalog inherits its
/// missing keys from.
fn is_ancestor_tag(ancestor: &str, tag: &str) -> bool {
    let mut theirs = ancestor.split('-');
    let mut ours = tag.split('-');
    loop {
        match (theirs.next(), ours.next()) {
            (None, _) => return true,
            (Some(a), Some(b)) if a.eq_ignore_ascii_case(b) => (),
            _ => return false,
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
        // American-flavored English is base `en`'s own crowd, and CLDR agrees:
        // it lists no parent for either, so both truncate straight to `en`.
        assert_eq!(resolve_ui_locale(&prefs(&["en-CA"]), SHIPPED), Some("en".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["en-PH"]), SHIPPED), Some("en".to_string()));
    }

    #[test]
    fn a_uk_or_australian_mac_reaches_its_own_english_overlay() {
        // The whole point of shipping the two overlays: a Mac set to English (UK)
        // or English (Australia) must land on that catalog, not on US English.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-GB"]), SHIPPED),
            Some("en-GB".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-AU"]), SHIPPED),
            Some("en-AU".to_string())
        );
        // The POSIX path on Linux spells it with an underscore, and casing isn't
        // guaranteed on either path.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en_GB"]), SHIPPED),
            Some("en-GB".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["EN-au"]), SHIPPED),
            Some("en-AU".to_string())
        );
        // A script subtag WEDGED between the language and the region defeats the
        // match, because subtags are compared by position: `en-latn-au` shares
        // only `en` with `en-au`, exactly as `fr-Latn-CA` shares only `fr` with
        // `fr` below, so the shortest-tag tiebreak takes base English. Neither
        // macOS nor a POSIX `LANG` emits that shape, so this pins the behavior
        // rather than blessing it.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-Latn-AU"]), SHIPPED),
            Some("en".to_string())
        );
        // A US Mac is untouched by their arrival.
        assert_eq!(resolve_ui_locale(&prefs(&["en-US"]), SHIPPED), Some("en".to_string()));
    }

    #[test]
    fn a_regional_english_with_no_catalog_of_its_own_reaches_the_british_overlay() {
        // The bug this chain exists to kill: a New Zealander whose Mac is set to
        // English (New Zealand) was reading "Trash", because `en-NZ` truncates to
        // `en`. CLDR parents it to `en-001`, World English, which `en-GB` answers
        // for, so they read "Bin" like every other reader of that English.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-NZ"]), SHIPPED),
            Some("en-GB".to_string())
        );
        // Same road, three more ways onto it: a direct `en-001` child, a
        // two-hop child through the Europe group, and `en-001` itself.
        for tag in ["en-IE", "en-ZA", "en-IN", "en-SG", "en-AT", "en-SE", "en-001"] {
            assert_eq!(
                resolve_ui_locale(&prefs(&[tag]), SHIPPED),
                Some("en-GB".to_string()),
                "{tag} should reach the British overlay"
            );
        }
    }

    #[test]
    fn a_shipped_regional_catalog_still_beats_the_group_it_belongs_to() {
        // `en-AU` is a child of `en-001` too, so the walk has to try the tag
        // itself before it ever reaches the node `en-GB` answers for.
        assert_eq!(
            resolve_ui_locale(&prefs(&["en-AU"]), SHIPPED),
            Some("en-AU".to_string())
        );
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

    /// A table with the Traditional catalog removed, so the tests that pin what
    /// the guard does with NOTHING Traditional to offer keep testing that, now
    /// that `zh-Hant` ships. The `zh` entry mirrors the generated one.
    const SIMPLIFIED_ONLY: &[ShippedLocale] = &[
        ShippedLocale {
            tag: "zh",
            script: "hans",
            default_script: "hans",
            region_scripts: &[("tw", "hant"), ("hk", "hant"), ("mo", "hant")],
            covers: &[],
        },
        ShippedLocale {
            tag: "sv",
            script: "latn",
            default_script: "latn",
            region_scripts: &[],
            covers: &[],
        },
    ];

    #[test]
    fn a_traditional_reader_never_lands_on_the_simplified_catalog() {
        // Every way a reader can ask for Traditional reaches the Traditional
        // catalog, and none of them reaches Simplified `zh`.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hant-TW"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        // Explicit script, no region.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hant"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        // No script subtag: the REGION says Traditional (CLDR likely subtags).
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-TW"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-HK"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-MO"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
    }

    #[test]
    fn with_no_traditional_catalog_a_traditional_reader_gets_english() {
        // The guard's own rule, independent of what we happen to ship: a
        // Simplified catalog in front of a Traditional reader is worse than
        // English, a language they at least chose to list.
        assert_eq!(resolve_ui_locale(&prefs(&["zh-Hant-TW"]), SIMPLIFIED_ONLY), None);
        assert_eq!(resolve_ui_locale(&prefs(&["zh-TW"]), SIMPLIFIED_ONLY), None);
        assert_eq!(resolve_ui_locale(&prefs(&["zh-HK"]), SIMPLIFIED_ONLY), None);
    }

    #[test]
    fn a_blocked_script_falls_through_to_the_next_preference() {
        // The guard doesn't end the walk, it only rules out one catalog: the
        // user's next choice still gets its turn.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-Hant-TW", "sv-SE"]), SIMPLIFIED_ONLY),
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
        // `en-CA` reading "Trash" and `-ize`, is a papercut a later catalog
        // fixes (`en-GB` and `en-AU` are exactly that later catalog, for the
        // regions CLDR routes their way). An unreadable script is a wall. Don't
        // "fix" this by blocking regional fallback.
        assert_eq!(resolve_ui_locale(&prefs(&["pt-PT"]), SHIPPED), Some("pt".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["en-CA"]), SHIPPED), Some("en".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["de-AT"]), SHIPPED), Some("de".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["nl-BE"]), SHIPPED), Some("nl".to_string()));
        assert_eq!(
            resolve_ui_locale(&prefs(&["fr-Latn-CA"]), SHIPPED),
            Some("fr".to_string())
        );
    }

    #[test]
    fn an_extended_language_subtag_does_not_hide_the_script() {
        // `zh-yue-Hant-HK` says `Hant` out loud, with an extlang in the way.
        // Reading the script positionally would see `yue`, match neither script
        // nor region, and fall through to the language default (Simplified),
        // handing a Traditional reader the one catalog the guard exists to
        // block. Same for the region half: `zh-yue-HK` still implies `Hant`.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-yue-Hant-HK"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-yue-HK"]), SHIPPED),
            Some("zh-Hant".to_string())
        );
        // A private-use or extension sequence is not a script, however
        // script-shaped its subtags look: `hant` here is `x`'s payload.
        assert_eq!(
            resolve_ui_locale(&prefs(&["zh-CN-x-hant"]), SHIPPED),
            Some("zh".to_string())
        );
    }

    #[test]
    fn a_catalog_that_names_its_own_script_guards_against_the_language_default() {
        // The mirror case, on a fixture where the Traditional catalog is the
        // ONLY one: a Simplified reader must not land on it just because the
        // base language matches.
        const ZH_HANT: &[ShippedLocale] = &[ShippedLocale {
            tag: "zh-Hant",
            script: "hant",
            default_script: "hans",
            region_scripts: &[("tw", "hant"), ("hk", "hant")],
            covers: &[],
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
        // `en-GB` and `en-AU` exercise this against the real table elsewhere in
        // this module. The `pt` fixture pins the same rule for a language whose
        // regional catalogs haven't landed yet.
        const PT: &[ShippedLocale] = &[
            ShippedLocale {
                tag: "pt",
                script: "latn",
                default_script: "latn",
                region_scripts: &[],
                covers: &[],
            },
            ShippedLocale {
                tag: "pt-BR",
                script: "latn",
                default_script: "latn",
                region_scripts: &[],
                covers: &[],
            },
        ];
        assert_eq!(resolve_ui_locale(&prefs(&["pt-BR"]), PT), Some("pt-BR".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["pt-PT"]), PT), Some("pt".to_string()));
        assert_eq!(resolve_ui_locale(&prefs(&["pt"]), PT), Some("pt".to_string()));
    }

    /// A Latin, region-free entry, for fixtures that add a regional catalog the
    /// real table doesn't ship yet.
    const fn latin(tag: &'static str) -> ShippedLocale {
        ShippedLocale {
            tag,
            script: "latn",
            default_script: "latn",
            region_scripts: &[],
            covers: &[],
        }
    }

    /// Every region CLDR's `parentLocales` sends to `es-419`, spelled the way a
    /// Mac reports it. Read off the generated [`PARENT_LOCALES`] rather than
    /// listed, so a CLDR bump that grows the group grows this test with it.
    fn latin_american_spanish_tags() -> Vec<String> {
        let tags: Vec<String> = PARENT_LOCALES
            .iter()
            .filter(|(_, parent)| *parent == "es-419")
            .map(|(child, _)| format!("es-{}", child[3..].to_ascii_uppercase()))
            .collect();
        // The set the Spanish split is for; a table missing them is a broken
        // generator, not a smaller Latin America.
        for expected in ["es-MX", "es-AR", "es-CO", "es-CL", "es-US"] {
            assert!(
                tags.iter().any(|tag| tag == expected),
                "{expected} lost its CLDR parent"
            );
        }
        tags
    }

    #[test]
    fn the_latin_american_overlay_catches_every_region_cldr_parents_to_it() {
        // CLDR's parent data routes every Latin American and US Spanish to the
        // shipped `es-419` overlay with no region table of ours, while Spain and
        // the rest of the Spanish-speaking world keep `es`.
        for tag in latin_american_spanish_tags()
            .iter()
            .map(String::as_str)
            .chain(["es-419"])
        {
            assert_eq!(
                resolve_ui_locale(&prefs(&[tag]), SHIPPED),
                Some("es-419".to_string()),
                "{tag} should reach the Latin American overlay"
            );
        }
        for tag in ["es", "es-ES", "es-GQ", "es-EA", "es-IC", "es-PH"] {
            assert_eq!(
                resolve_ui_locale(&prefs(&[tag]), SHIPPED),
                Some("es".to_string()),
                "{tag} should stay on Spain's Spanish"
            );
        }
    }

    #[test]
    fn a_european_portuguese_overlay_catches_the_lusophone_world_and_leaves_brazil_alone() {
        // Our `pt` is Brazilian, which is also CLDR's reading of bare `pt`.
        // A `pt-PT` overlay picks up the regions CLDR parents to it.
        const PORTUGUESE: &[ShippedLocale] = &[latin("en"), latin("pt"), latin("pt-PT")];
        for tag in ["pt-PT", "pt-AO", "pt-MZ", "pt-CV", "pt-MO", "pt-CH"] {
            assert_eq!(
                resolve_ui_locale(&prefs(&[tag]), PORTUGUESE),
                Some("pt-PT".to_string()),
                "{tag} should reach the European overlay"
            );
        }
        for tag in ["pt", "pt-BR"] {
            assert_eq!(resolve_ui_locale(&prefs(&[tag]), PORTUGUESE), Some("pt".to_string()));
        }
    }

    #[test]
    fn an_overlay_inherits_from_the_catalog_it_is_written_against() {
        // What `menu_t` walks for a key the active catalog lacks.
        const SPANISH: &[ShippedLocale] = &[latin("en"), latin("es"), latin("es-419")];
        assert_eq!(inheritance_chain("es-419", SPANISH), vec!["es"]);
        assert_eq!(inheritance_chain("es", SPANISH), Vec::<&str>::new());
        assert_eq!(inheritance_chain("en-GB", SHIPPED), vec!["en"]);
        assert_eq!(inheritance_chain("zh-Hant", SHIPPED), Vec::<&str>::new());
        assert_eq!(inheritance_chain("kl", SHIPPED), Vec::<&str>::new());
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
