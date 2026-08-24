//! Localized macOS system pane labels for user-facing copy.
//!
//! ## Why
//!
//! Onboarding and friendly-error copy point users at specific macOS System
//! Settings panes ("Privacy & Security", "Full Disk Access", "Files & Folders",
//! ...). If we hardcode the English labels, a user on a Hungarian macOS opens
//! System Settings and can't find "Privacy & Security" because it shows there as
//! "Adatvédelem és biztonság". The app's own language is independent of the
//! OS's: a user can run Cmdr in English on a French macOS. We always want the
//! labels to match what's on screen in System Settings, not what's in our app.
//!
//! ## How
//!
//! macOS ships `.loctable` files inside system app/extension bundles. Each is a
//! binary plist shaped as `{ language: { key: localized_string } }`. We load a
//! tiny whitelist of (bundle path, key) tuples once at startup, pick the user's
//! preferred language from `NSUserDefaults.AppleLanguages`, and fall back to the
//! English defaults we ship if any step fails. The frontend reads the snapshot
//! once via `get_localized_system_strings` and substitutes the `{system_settings}`
//! etc. placeholders into user-facing copy itself
//! (`src/lib/error-messages/compose.ts::expandSystemStrings`); all friendly-error words
//! live on the frontend.
//!
//! ## Risks (knowingly accepted)
//!
//! - **`.loctable` paths and string keys are undocumented.** Apple has changed
//!   bundle locations between major releases (System Preferences → System
//!   Settings + PrivacySecurity extension at Ventura). If a path moves or a key
//!   disappears, the affected field falls back to its English default. No
//!   crash, no degraded UI, just a missed translation. The English defaults
//!   live in [`LocalizedSystemStrings::english_defaults`].
//! - **`AppleLanguages` BCP-47 codes are loosely matched** to loctable language
//!   keys. We try the exact code (with `-`→`_`), then the base language. We do
//!   not try region fallbacks beyond that (a user on `pt-MZ` won't get `pt-PT`
//!   if only `pt` exists, which is fine, since `pt` does exist).
//!
//! ## Finder labels: why they're catalog strings, not OS-sourced
//!
//! Some error copy also quotes Finder labels (`Get Info`, the `Locked`
//! checkbox, `Sharing & Permissions`). Apple localizes all three, so they are
//! TRANSLATED in the message catalogs rather than resolved here. That is the
//! right answer for the common case and the wrong one for a user running Cmdr
//! in one language on a Mac set to another: the catalog follows the app
//! language, Finder follows the system language, and only the labels resolved
//! by this module follow the system language the way System Settings pane names
//! must.
//!
//! Moving them here is a real change, not a one-liner, which is why it hasn't
//! happened yet. What it would take:
//!
//! - **A second source shape.** Finder has no `Localizable.loctable`; it ships
//!   per-language `<lang>.lproj/*.strings` (binary plists), so the language
//!   comes from the PATH, not from an outer dict key. `StringSource` and
//!   `build_snapshot` would need a variant that probes
//!   `<bundle>/<candidate>.lproj/<file>.strings` over `candidate_lang_codes`.
//! - **Shakier keys.** The values live under nib object IDs, not names:
//!   `MenuBar.strings` `300801.title` (Get Info), `InfoWindowGeneralView`
//!   `1073.title` (Locked), `InfoWindowPermissionsView` `6.title` (Sharing &
//!   Permissions), plus `LocalizableMerged` `N30`/`N32`/`NE43` for the
//!   running-text forms. Loctable keys at least read like names; these are
//!   renumberable. (Verified on macOS 26.5.2, `plutil -convert json`,
//!   2026-08-24.)
//! - **No English row to fall back to.** `en.lproj` carries no `MenuBar.strings`
//!   (English lives in the compiled `Base.lproj` nibs), so `lookup_in_table`'s
//!   `en`-last fallback finds nothing and every miss lands on
//!   `english_defaults`. Workable, but it means the fallback is untested by the
//!   OS itself.
//! - **Grammar at the seam.** Unlike the pane names, which only ever appear
//!   inside an inert bold path (`**{a} > {b} > {c}**`), these sit mid-sentence
//!   with articles and quoting attached (hu `az Infó megjelenítése parancsot`,
//!   de `Häkchen bei „Geschützt“ entfernen`). Nine locales would each need
//!   their sentence reshaped so an OS-supplied noun drops in without agreement.
//! - **Reach.** Six keys across ten catalogs, plus new `{get_info}` /
//!   `{locked}` / `{sharing_and_permissions}` entries in `SYSTEM_TOKENS`
//!   (`scripts/i18n-catalog-lib.ts`), `ENGLISH_DEFAULTS`
//!   (`src/lib/system-strings.svelte.ts`), `expandSystemStrings`, and
//!   regenerated `bindings.ts`.
//!
//! ## When to refresh
//!
//! The snapshot is built once at first access and cached, then dropped whenever
//! the OS's locale answers move (`invalidate`, called by the locale watcher in
//! `intl/live_locale.rs`). Without that, a user who switches the macOS language
//! mid-session keeps reading the OLD pane names, which is worse than showing
//! English: the copy would point at a "System Settings" label that is no longer
//! on their screen. Rebuilding costs two `.loctable` parses and happens at most
//! once per language change.

#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::sync::RwLock;

use serde::Serialize;

/// Snapshot of the system pane labels we surface in user-facing copy.
///
/// Field names match the placeholder tokens the frontend substitutes
/// (`{system_settings}` → [`Self::system_settings`], etc.).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LocalizedSystemStrings {
    pub system_settings: String,
    pub privacy_and_security: String,
    pub full_disk_access: String,
    pub files_and_folders: String,
    pub local_network: String,
    pub appearance: String,
}

impl LocalizedSystemStrings {
    /// English defaults shipped as fallback when a lookup misses. These match
    /// the literal strings the codebase used before the localized lookup
    /// landed, so a complete failure of the lookup produces identical output.
    fn english_defaults() -> Self {
        Self {
            system_settings: "System Settings".to_string(),
            privacy_and_security: "Privacy & Security".to_string(),
            full_disk_access: "Full Disk Access".to_string(),
            files_and_folders: "Files & Folders".to_string(),
            local_network: "Local Network".to_string(),
            appearance: "Appearance".to_string(),
        }
    }
}

/// Cached snapshot, built on first access and dropped by `invalidate` when the
/// OS's language moves. `None` means "not built yet", never "no answer": a
/// rebuild always produces a full struct, falling back to the English defaults
/// field by field.
static SNAPSHOT: RwLock<Option<LocalizedSystemStrings>> = RwLock::new(None);

/// The cached snapshot, building it if this is the first read since a launch or
/// an `invalidate`. A read-lock hit on the common path.
///
/// Two threads racing the first read both build; they'd build the same answer,
/// and paying that once beats holding the write lock across two `.loctable`
/// parses.
pub fn snapshot() -> LocalizedSystemStrings {
    if let Some(cached) = SNAPSHOT.read().unwrap_or_else(|e| e.into_inner()).clone() {
        return cached;
    }
    let built = build_snapshot();
    *SNAPSHOT.write().unwrap_or_else(|e| e.into_inner()) = Some(built.clone());
    built
}

/// Drops the cached snapshot so the next read resolves against the language the
/// user reads NOW. Called from the locale watcher when the OS's answers move.
///
/// macOS-only, and deliberately not stubbed for Linux: its one caller is the
/// macOS locale observer (`intl/live_locale.rs`), Linux has no equivalent OS
/// signal to hang it off, and `build_snapshot` there returns the English
/// defaults unconditionally, so there'd be nothing to rebuild anyway. A Linux
/// stub would just be dead code that `deny(unused)` is right to reject.
#[cfg(target_os = "macos")]
pub fn invalidate() {
    *SNAPSHOT.write().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Tauri command: returns the localized system strings. The frontend holds them
/// in a reactive snapshot and substitutes the placeholders itself, re-reading
/// this command whenever the OS locale changes.
#[tauri::command]
#[specta::specta]
pub fn get_localized_system_strings() -> LocalizedSystemStrings {
    snapshot()
}

// =================================================================================
// Snapshot builder + loctable plumbing
// =================================================================================

/// One (bundle resource, key) tuple per field. Order doesn't matter; we just
/// look up each independently and merge the misses with the English defaults.
#[cfg(target_os = "macos")]
struct StringSource {
    /// Absolute path to a `.loctable` (binary plist) inside a system bundle.
    loctable: &'static str,
    /// The key under each language's dict whose value we want.
    key: &'static str,
}

/// Where each `LocalizedSystemStrings` field comes from.
///
/// These paths and keys are verified on macOS 14–26 (`System Settings.app`
/// shipped in Ventura+; `SecurityPrivacyExtension.appex` has the per-pane
/// labels). The `Appearance.appex/InfoPlist.loctable` key `CFBundleDisplayName`
/// is the bundle's own display name, which is what System Settings renders as
/// the pane title.
#[cfg(target_os = "macos")]
struct StringCatalog {
    system_settings: StringSource,
    privacy_and_security: StringSource,
    full_disk_access: StringSource,
    files_and_folders: StringSource,
    local_network: StringSource,
    appearance: StringSource,
}

#[cfg(target_os = "macos")]
const CATALOG: StringCatalog = StringCatalog {
    system_settings: StringSource {
        loctable: "/System/Applications/System Settings.app/Contents/Resources/Localizable.loctable",
        // Apple kept the legacy "System Preferences" key when they renamed the app
        // to "System Settings"; the value under it is the new localized name.
        key: "System Preferences",
    },
    privacy_and_security: StringSource {
        loctable: "/System/Applications/System Settings.app/Contents/Resources/Localizable.loctable",
        key: "PRIVACY_SECTION",
    },
    full_disk_access: StringSource {
        loctable: "/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable",
        key: "ALL_FILES",
    },
    files_and_folders: StringSource {
        loctable: "/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable",
        key: "FILE_ACCESS_COMBINED",
    },
    local_network: StringSource {
        loctable: "/System/Library/ExtensionKit/Extensions/SecurityPrivacyExtension.appex/Contents/Resources/Localizable.loctable",
        key: "LOCAL_NETWORK",
    },
    appearance: StringSource {
        loctable: "/System/Library/ExtensionKit/Extensions/Appearance.appex/Contents/Resources/InfoPlist.loctable",
        key: "CFBundleDisplayName",
    },
};

#[cfg(target_os = "macos")]
fn build_snapshot() -> LocalizedSystemStrings {
    let langs = apple_languages();
    let defaults = LocalizedSystemStrings::english_defaults();

    // Parse each loctable once even when several fields share one file.
    let mut tables: HashMap<&'static str, LoctableData> = HashMap::new();
    let mut load_for = |src: &StringSource| -> Option<String> {
        if !tables.contains_key(src.loctable)
            && let Some(data) = parse_loctable(src.loctable)
        {
            tables.insert(src.loctable, data);
        }
        let table = tables.get(src.loctable)?;
        lookup_in_table(table, &langs, src.key)
    };

    let resolved = LocalizedSystemStrings {
        system_settings: load_for(&CATALOG.system_settings).unwrap_or(defaults.system_settings.clone()),
        privacy_and_security: load_for(&CATALOG.privacy_and_security).unwrap_or(defaults.privacy_and_security.clone()),
        full_disk_access: load_for(&CATALOG.full_disk_access).unwrap_or(defaults.full_disk_access.clone()),
        files_and_folders: load_for(&CATALOG.files_and_folders).unwrap_or(defaults.files_and_folders.clone()),
        local_network: load_for(&CATALOG.local_network).unwrap_or(defaults.local_network.clone()),
        appearance: load_for(&CATALOG.appearance).unwrap_or(defaults.appearance.clone()),
    };

    log::debug!(
        target: "system_strings",
        "Resolved system strings for langs={:?}: {:?}",
        langs, resolved
    );
    resolved
}

#[cfg(not(target_os = "macos"))]
fn build_snapshot() -> LocalizedSystemStrings {
    // Stubs/Linux: the labels never reach the UI (the surfaces that use them
    // are macOS-only modals), but the snapshot exists so the IPC command
    // returns something sensible if a Linux harness calls it.
    LocalizedSystemStrings::english_defaults()
}

/// Parsed loctable: outer key is the language code (`en`, `hu`, `en_GB`,
/// `pt-PT`, ...), inner map is `string_key → localized_value`.
#[cfg(target_os = "macos")]
type LoctableData = HashMap<String, HashMap<String, String>>;

#[cfg(target_os = "macos")]
fn parse_loctable(path: &str) -> Option<LoctableData> {
    let value = plist::Value::from_file(path)
        .map_err(|e| log::debug!(target: "system_strings", "parse_loctable({path}): {e}"))
        .ok()?;
    let dict = value.into_dictionary()?;
    let mut out: LoctableData = HashMap::with_capacity(dict.len());
    for (lang, per_lang) in dict {
        // `LocProvenance` and similar metadata keys aren't language dicts;
        // skip anything that doesn't decode as a string map.
        let Some(inner) = per_lang.into_dictionary() else {
            continue;
        };
        let mut strings: HashMap<String, String> = HashMap::with_capacity(inner.len());
        for (k, v) in inner {
            if let Some(s) = v.into_string() {
                strings.insert(k, s);
            }
        }
        out.insert(lang, strings);
    }
    Some(out)
}

/// Picks the first language in `langs` whose loctable entry for `key` exists.
/// Falls back to `en` last, so a missing target language still produces the
/// canonical English string before bailing to `None`.
#[cfg(target_os = "macos")]
fn lookup_in_table(table: &LoctableData, langs: &[String], key: &str) -> Option<String> {
    for candidate in candidate_lang_codes(langs) {
        if let Some(inner) = table.get(&candidate)
            && let Some(value) = inner.get(key)
        {
            return Some(value.clone());
        }
    }
    None
}

/// Expands the user's preferred-language list into the loctable-key forms we
/// should try, in priority order. Each BCP-47 tag produces up to three
/// candidates: the original, an `_`-normalized form (`en-GB` → `en_GB`), and
/// the base language (`en-GB` → `en`). Duplicates are dropped while preserving
/// order. `en` is appended at the end as a universal fallback.
#[cfg(target_os = "macos")]
fn candidate_lang_codes(preferred: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(preferred.len() * 3 + 1);
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let push = |out: &mut Vec<String>, seen: &mut std::collections::HashSet<String>, s: String| {
        if !s.is_empty() && seen.insert(s.clone()) {
            out.push(s);
        }
    };
    for lang in preferred {
        push(&mut out, &mut seen, lang.clone());
        if lang.contains('-') {
            push(&mut out, &mut seen, lang.replace('-', "_"));
        }
        if let Some(base) = lang.split(['-', '_']).next() {
            push(&mut out, &mut seen, base.to_string());
        }
    }
    push(&mut out, &mut seen, "en".to_string());
    out
}

/// The user's most-preferred UI language as a BCP-47 code (for example `en-US`), for diagnostics.
/// `None` off macOS or when the OS reports no languages. Coarse locale only; carries no PII.
#[cfg(target_os = "macos")]
pub(crate) fn preferred_language() -> Option<String> {
    apple_languages().into_iter().find(|s| !s.is_empty())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn preferred_language() -> Option<String> {
    None
}

/// The user's UI languages, most-preferred first, straight from
/// `NSUserDefaults.AppleLanguages`. The ORDER is the user's own fallback plan,
/// so keep it: [`crate::intl::resolve_ui_locale`] walks it to find the first
/// language we ship.
#[cfg(target_os = "macos")]
pub(crate) fn apple_languages() -> Vec<String> {
    use objc2_foundation::{NSString, NSUserDefaults};

    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str("AppleLanguages");
    let Some(array) = defaults.stringArrayForKey(&key) else {
        return vec!["en".to_string()];
    };
    array.iter().map(|s| s.to_string()).collect()
}

// Every test here is macOS-only (they assert macOS system-string resolution), so
// gate the whole module to macOS: on Linux `#[cfg(test)]` alone leaves `use
// super::*` unused and trips `deny(unused)`.
#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn candidate_codes_handle_bcp47_to_underscore_and_base() {
        let out = candidate_lang_codes(&["en-GB".to_string(), "hu-HU".to_string()]);
        // Original, `_`-normalized, base, plus the universal `en` fallback once.
        assert!(out.starts_with(&[
            "en-GB".to_string(),
            "en_GB".to_string(),
            "en".to_string(),
            "hu-HU".to_string(),
            "hu_HU".to_string(),
            "hu".to_string(),
        ]));
        assert!(out.contains(&"en".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn candidate_codes_dedupe_when_base_matches_original() {
        let out = candidate_lang_codes(&["en".to_string()]);
        assert_eq!(out, vec!["en".to_string()]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn candidate_codes_always_include_english_fallback() {
        let out = candidate_lang_codes(&["fi".to_string()]);
        assert!(out.contains(&"en".to_string()));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn lookup_in_table_walks_candidate_languages_in_order() {
        let mut table: LoctableData = HashMap::new();
        let mut en_dict = HashMap::new();
        en_dict.insert("KEY".to_string(), "english".to_string());
        let mut hu_dict = HashMap::new();
        hu_dict.insert("KEY".to_string(), "magyar".to_string());
        table.insert("en".to_string(), en_dict);
        table.insert("hu".to_string(), hu_dict);

        // Hungarian preferred → magyar.
        let langs = vec!["hu-HU".to_string()];
        let out = lookup_in_table(&table, &candidate_lang_codes(&langs), "KEY");
        assert_eq!(out.as_deref(), Some("magyar"));

        // No Hungarian, English fallback kicks in.
        let langs = vec!["fi".to_string()];
        let out = lookup_in_table(&table, &candidate_lang_codes(&langs), "KEY");
        assert_eq!(out.as_deref(), Some("english"));

        // Missing key returns None even when the language is present.
        let out = lookup_in_table(&table, &candidate_lang_codes(&langs), "MISSING");
        assert_eq!(out, None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn invalidating_the_cache_rebuilds_a_complete_snapshot() {
        // The REFRESH path, not just the first-build one: after the locale
        // watcher drops the cache, the next read has to build a full struct
        // rather than surface the `None`. Otherwise a user who switches the
        // macOS language mid-session gets empty pane names in the very copy
        // that tells them where to click.
        //
        // Asserts the observable answer rather than the private `None` state:
        // tests share the process, so another one's `snapshot()` could refill
        // the cache between the two lines here.
        let before = snapshot();
        invalidate();
        let after = snapshot();
        assert!(!after.system_settings.is_empty());
        assert!(!after.privacy_and_security.is_empty());
        assert!(!after.full_disk_access.is_empty());
        assert!(!after.files_and_folders.is_empty());
        assert!(!after.local_network.is_empty());
        assert!(!after.appearance.is_empty());
        // Nothing moved the OS language between the two reads, so the rebuild
        // has to land on the same answer.
        assert_eq!(before.system_settings, after.system_settings);
        assert_eq!(before.full_disk_access, after.full_disk_access);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn snapshot_resolves_to_non_empty_strings_on_macos() {
        // Either fully localized or all English defaults. Either way, every
        // field must be non-empty so callers can blindly substitute.
        let s = snapshot();
        assert!(!s.system_settings.is_empty());
        assert!(!s.privacy_and_security.is_empty());
        assert!(!s.full_disk_access.is_empty());
        assert!(!s.files_and_folders.is_empty());
        assert!(!s.local_network.is_empty());
        assert!(!s.appearance.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_languages_returns_at_least_one_entry() {
        // On any macOS host, `AppleLanguages` always has a value (the OS seeds
        // it at first login). Empty here would mean our `NSUserDefaults` read
        // misfired and we'd silently always pick the English fallback.
        let langs = apple_languages();
        assert!(!langs.is_empty(), "AppleLanguages should never be empty on macOS");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn apple_languages_preserves_the_users_order() {
        // The whole auto-language feature rests on this list being the user's
        // ordered fallback plan: sorting or deduping it would quietly turn
        // "Hungarian, then Swedish" into "whatever comes first alphabetically".
        let langs = apple_languages();
        assert_eq!(langs.first().cloned(), preferred_language());
    }
}
