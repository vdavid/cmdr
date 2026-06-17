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
//! (`src/lib/errors/compose.ts::expandSystemStrings`); all friendly-error words
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
//!   if only `pt` exists, which is fine — `pt` does exist).
//!
//! ## When to refresh
//!
//! The snapshot is built once at first access and cached. macOS rarely changes
//! the user's preferred language during a session, and even when it does the
//! cost of being one session behind is zero (relaunch picks up the change).

#[cfg(target_os = "macos")]
use std::collections::HashMap;
use std::sync::LazyLock;

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

/// Cached snapshot. Built on first access; never refreshed during the session.
static SNAPSHOT: LazyLock<LocalizedSystemStrings> = LazyLock::new(build_snapshot);

/// Returns a `'static` reference to the cached snapshot. Fast (no lock,
/// pointer-copy after first call). Placeholder expansion now lives on the
/// frontend (`src/lib/errors/compose.ts::expandSystemStrings`), which reads the
/// snapshot via `get_localized_system_strings`; this accessor is test-only.
/// macOS-only: its sole caller is the macOS-gated snapshot test, so on Linux
/// `#[cfg(test)]` alone would leave it unused and trip `deny(unused)`.
#[cfg(all(test, target_os = "macos"))]
pub fn snapshot() -> &'static LocalizedSystemStrings {
    &SNAPSHOT
}

/// Tauri command: returns the localized system strings. The frontend caches
/// the result for the session and substitutes the placeholders itself.
#[tauri::command]
#[specta::specta]
pub fn get_localized_system_strings() -> LocalizedSystemStrings {
    SNAPSHOT.clone()
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

#[cfg(target_os = "macos")]
fn apple_languages() -> Vec<String> {
    use objc2_foundation::{NSString, NSUserDefaults};

    let defaults = NSUserDefaults::standardUserDefaults();
    let key = NSString::from_str("AppleLanguages");
    let Some(array) = defaults.stringArrayForKey(&key) else {
        return vec!["en".to_string()];
    };
    array.iter().map(|s| s.to_string()).collect()
}

// Every test here is macOS-only (they assert macOS system-string resolution), so
// gate the whole module to macOS — on Linux `#[cfg(test)]` alone leaves `use
// super::*` (and `snapshot()`) unused and trips `deny(unused)`.
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
    fn snapshot_resolves_to_non_empty_strings_on_macos() {
        // Either fully localized or all English defaults — either way every
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
}
