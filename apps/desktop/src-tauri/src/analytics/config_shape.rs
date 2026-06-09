//! Builds the PII-free config-shape snapshot shipped with each heartbeat (and, later, mirrored as
//! PostHog person properties).
//!
//! This module owns the ONE rule for what's in the snapshot, by allowlist, never by redaction (see
//! `analytics/CLAUDE.md` § "PII-free by allowlist"). Settings hold SMB hostnames, paths, recent
//! lists, AI key refs, and the beta email, all as strings, so a denylist would eventually leak one.
//!
//! The rule (David: "the whole config except string fields"):
//!
//! - Include every key whose JSON value is a boolean or a number. Bools and numbers are PII-free by
//!   nature, so this auto-extends as new bool/number settings land, zero maintenance.
//! - Plus the small [`CATEGORICAL_STRING_KEYS`] allowlist: categorical enum-strings (theme, view
//!   preferences, AI mode, sort mode, etc.) that are non-PII despite being strings.
//! - Exclude every other string, and all objects and arrays.
//! - Add `fdaGranted` explicitly (it's runtime state, not a setting).

use serde_json::{Map, Value};

/// The categorical enum-string settings worth keeping. These hold a fixed, non-PII vocabulary
/// (light/dark/system, off/cloud/local, etc.), so they're safe to ship even though they're strings.
///
/// Deliberately excludes free-text and identifier strings: `appearance.customDateTimeFormat` (user
/// free-text), `ai.cloudProviderConfigs` (a JSON blob with per-provider model/baseUrl),
/// `behavior.fileSystemWatching.globalGoToLatestShortcut.binding` (a key combo), and
/// `analytics.email` (PII). Those stay out by being absent from this list.
const CATEGORICAL_STRING_KEYS: &[&str] = &[
    "theme.mode",
    "appearance.appColor",
    "appearance.sizeColors",
    "appearance.dateColors",
    "appearance.dateTimeFormat",
    "appearance.uiDensity",
    "appearance.fileSizeFormat",
    "appearance.tintLocal",
    "appearance.tintSmb",
    "appearance.tintMtp",
    "listing.sizeDisplay",
    "listing.sizeUnit",
    "listing.directorySortMode",
    "listing.briefColumnWidthMode",
    "fileOperations.allowFileExtensionChanges",
    "behavior.fileSystemWatching.downloadsNotifications",
    "behavior.fileSystemWatching.lowDiskSpaceNotifications",
    "ai.provider",
    "ai.cloudProvider",
    "ai.localContextSize",
    "network.timeoutMode",
];

/// Builds the config-shape object from the raw `settings.json` value plus the runtime FDA-granted
/// flag. Pure: no I/O, so it's directly unit-testable against a seeded settings JSON.
///
/// `raw_settings` is the parsed `settings.json` (a flat object with dot-notation string keys). A
/// non-object value (missing/corrupt file) yields a snapshot with only `fdaGranted`.
pub fn build_config_shape(raw_settings: &Value, fda_granted: bool) -> Value {
    let mut shape = Map::new();

    if let Some(obj) = raw_settings.as_object() {
        for (key, value) in obj {
            if include_key(key, value) {
                shape.insert(key.clone(), value.clone());
            }
        }
    }

    // FDA-granted is runtime state, not a setting, so add it explicitly. Last so it can't be
    // shadowed by a (nonexistent) same-named setting.
    shape.insert("fdaGranted".to_string(), Value::Bool(fda_granted));

    Value::Object(shape)
}

/// The allowlist decision for one key/value pair: bools and numbers always pass; strings pass only
/// if categorical; everything else (objects, arrays, null) is excluded.
fn include_key(key: &str, value: &Value) -> bool {
    match value {
        Value::Bool(_) | Value::Number(_) => true,
        Value::String(_) => CATEGORICAL_STRING_KEYS.contains(&key),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn includes_bools_and_numbers() {
        let settings = json!({
            "showHiddenFiles": true,
            "listing.briefColumnWidthMaxPx": 320,
            "appearance.textSize": 125.0,
        });
        let shape = build_config_shape(&settings, false);
        assert_eq!(shape["showHiddenFiles"], json!(true));
        assert_eq!(shape["listing.briefColumnWidthMaxPx"], json!(320));
        assert_eq!(shape["appearance.textSize"], json!(125.0));
    }

    #[test]
    fn includes_categorical_string_keys() {
        let settings = json!({
            "theme.mode": "dark",
            "ai.provider": "cloud",
            "listing.directorySortMode": "name",
        });
        let shape = build_config_shape(&settings, false);
        assert_eq!(shape["theme.mode"], json!("dark"));
        assert_eq!(shape["ai.provider"], json!("cloud"));
        assert_eq!(shape["listing.directorySortMode"], json!("name"));
    }

    #[test]
    fn excludes_pii_shaped_strings() {
        // This is the privacy invariant: PII-shaped string values must NOT appear in the snapshot.
        let settings = json!({
            "analytics.email": "person@example.com",
            "network.lastHost": "smb://192.168.1.42/share",
            "fileExplorer.recentPaths": "/Users/dave/secret",
            "appearance.customDateTimeFormat": "YYYY-MM-DD",
            "ai.cloudProviderConfigs": "{\"openai\":{\"baseUrl\":\"https://api.openai.com\"}}",
            "behavior.fileSystemWatching.globalGoToLatestShortcut.binding": "\u{2303}\u{2325}\u{2318}J",
        });
        let shape = build_config_shape(&settings, false);
        let obj = shape.as_object().expect("object");

        // None of the PII-shaped keys are present.
        assert!(!obj.contains_key("analytics.email"));
        assert!(!obj.contains_key("network.lastHost"));
        assert!(!obj.contains_key("fileExplorer.recentPaths"));
        assert!(!obj.contains_key("appearance.customDateTimeFormat"));
        assert!(!obj.contains_key("ai.cloudProviderConfigs"));
        assert!(!obj.contains_key("behavior.fileSystemWatching.globalGoToLatestShortcut.binding"));

        // And no value in the whole snapshot carries the PII substrings, by construction.
        let serialized = shape.to_string();
        assert!(!serialized.contains("person@example.com"), "email leaked: {serialized}");
        assert!(!serialized.contains("192.168.1.42"), "host leaked: {serialized}");
        assert!(!serialized.contains("/Users/dave"), "path leaked: {serialized}");
    }

    #[test]
    fn excludes_objects_and_arrays() {
        let settings = json!({
            "someObject": { "nested": true },
            "someArray": [1, 2, 3],
            "someNull": null,
        });
        let shape = build_config_shape(&settings, false);
        let obj = shape.as_object().expect("object");
        assert!(!obj.contains_key("someObject"));
        assert!(!obj.contains_key("someArray"));
        assert!(!obj.contains_key("someNull"));
    }

    #[test]
    fn adds_fda_granted_explicitly() {
        let shape = build_config_shape(&json!({}), true);
        assert_eq!(shape["fdaGranted"], json!(true));

        let shape_denied = build_config_shape(&json!({}), false);
        assert_eq!(shape_denied["fdaGranted"], json!(false));
    }

    #[test]
    fn non_object_settings_yields_only_fda() {
        // A missing/corrupt settings file parses to something non-object; the snapshot still has
        // a valid shape carrying just the runtime flag.
        let shape = build_config_shape(&json!("not an object"), true);
        let obj = shape.as_object().expect("object");
        assert_eq!(obj.len(), 1);
        assert_eq!(obj["fdaGranted"], json!(true));
    }
}
