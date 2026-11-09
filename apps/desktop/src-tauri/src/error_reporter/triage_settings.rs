//! The handful of settings an error report carries so triage can tell "a setting" from "a bug".
//!
//! Hand-picked, ❌ never the heartbeat's whole config shape: the heartbeat ships every bool and number
//! under the `anal_` id, and a report ships under the `diag_` id, sometimes with an email. A full
//! snapshot in both would let the mix of changed settings join the two streams, which is what the
//! two-id split exists to prevent. A short list of settings that change what operations, listings,
//! and network access DO gives triage what it needs and little to match on.
//!
//! Each key resolves to what Cmdr ran with: the stored value under the organization's locks
//! (`managed_policy::overlay`), else the registry default the frontend pushed
//! ([`super::settings_defaults`]). A key with neither is left out rather than guessed. Values are
//! booleans, numbers, or enum tokens only: a string that isn't a short token never ships, so a
//! key added here by mistake can't carry a path or a name.

use std::collections::BTreeMap;

use serde_json::Value;

/// The settings a report carries. Each one changes what Cmdr does, not how it looks, except the
/// language (which catalog a quoted message came from) and the theme (which colors a screenshot
/// shows). ❗ Add a key only if it's a bool, a number, or an enum, and only if it explains a class
/// of reports; every key is a little more to match a report against.
pub(super) const TRIAGE_SETTING_KEYS: &[&str] = &[
    "listing.showHiddenFiles",
    "listing.directorySortMode",
    "fileExplorer.typeToJump.mode",
    "behavior.archiveEnter.zip",
    "fileOperations.allowFileExtensionChanges",
    "fileOperations.macOnlyFiles",
    "fileOperations.skipConfirmation",
    "fileOperations.defaultConflictPolicy",
    "fileOperations.mtpEnabled",
    "fileOperations.adbEnabled",
    "network.enabled",
    "network.directSmbConnection",
    "network.requireSmbEncryption",
    "network.timeoutMode",
    "network.smbConcurrency",
    "operationLog.enabled",
    "mediaIndex.enabled",
    "appearance.language",
    "theme.mode",
];

/// The triage settings for this install right now.
pub(super) fn current<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> BTreeMap<String, Value> {
    let raw = match crate::config::resolved_app_data_dir(app) {
        Ok(dir) => crate::state_recovery::settings_for_consent(&dir.join("settings.json")),
        Err(_) => Value::Null,
    };
    resolve(raw, &crate::managed_policy::current(), &|key| {
        super::settings_defaults::lookup_json(key)
    })
}

/// Pure: the stored `settings.json` (`Null` on a first launch), the policy, and the defaults lookup.
pub(super) fn resolve(
    mut raw: Value,
    policy: &crate::managed_policy::ManagedPolicy,
    default_for: &dyn Fn(&str) -> Option<Value>,
) -> BTreeMap<String, Value> {
    if !raw.is_object() {
        raw = Value::Object(serde_json::Map::new());
    }
    crate::managed_policy::overlay(policy, &mut raw);
    TRIAGE_SETTING_KEYS
        .iter()
        .filter_map(|&key| {
            let value = raw
                .get(key)
                .cloned()
                .filter(is_shippable)
                .or_else(|| default_for(key))?;
            is_shippable(&value).then(|| (key.to_string(), value))
        })
        .collect()
}

/// A bool, a number, or a short enum token (`name`, `in-app`, `zh-Hant`). Anything else stays home.
fn is_shippable(value: &Value) -> bool {
    match value {
        Value::Bool(_) | Value::Number(_) => true,
        Value::String(s) => {
            !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_policy::ManagedPolicy;
    use serde_json::json;

    fn no_defaults(_: &str) -> Option<Value> {
        None
    }

    #[test]
    fn carries_the_stored_value_of_each_listed_key() {
        let raw = json!({
            "listing.showHiddenFiles": true,
            "fileExplorer.typeToJump.mode": "filter",
            "network.smbConcurrency": 8,
        });
        let got = resolve(raw, &ManagedPolicy::default(), &no_defaults);
        assert_eq!(got["listing.showHiddenFiles"], json!(true));
        assert_eq!(got["fileExplorer.typeToJump.mode"], json!("filter"));
        assert_eq!(got["network.smbConcurrency"], json!(8));
    }

    #[test]
    fn leaves_out_every_key_not_on_the_list() {
        let raw = json!({ "analytics.email": "someone@example.com", "behavior.textEditorApp": "/Applications/X.app" });
        let got = resolve(raw, &ManagedPolicy::default(), &no_defaults);
        assert!(got.is_empty(), "unlisted keys shipped: {got:?}");
    }

    #[test]
    fn falls_back_to_the_registry_default_for_an_unset_key() {
        let defaults = |key: &str| (key == "theme.mode").then(|| json!("system"));
        let got = resolve(Value::Null, &ManagedPolicy::default(), &defaults);
        assert_eq!(got.get("theme.mode"), Some(&json!("system")));
        assert_eq!(
            got.len(),
            1,
            "a key with no value and no default is left out, not guessed"
        );
    }

    #[test]
    fn never_ships_a_string_that_isnt_an_enum_token() {
        // A listed key holding something path- or sentence-shaped (a bug elsewhere) still stays home.
        let raw = json!({
            "theme.mode": "/Users/someone/Documents",
            "listing.directorySortMode": "sort by name please",
            "appearance.language": "zh-Hant",
        });
        let got = resolve(raw, &ManagedPolicy::default(), &no_defaults);
        assert!(!got.contains_key("theme.mode"));
        assert!(!got.contains_key("listing.directorySortMode"));
        assert_eq!(got["appearance.language"], json!("zh-Hant"));
    }

    #[test]
    fn every_listed_key_is_a_real_setting() {
        // Renaming a setting must not silently drop it from reports. `types.ts` is the registry's typed index.
        let types = include_str!("../../../src/lib/settings/types.ts");
        for key in TRIAGE_SETTING_KEYS {
            assert!(
                types.contains(&format!("'{key}':")),
                "{key} isn't a setting id in settings/types.ts"
            );
        }
    }
}
