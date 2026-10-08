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
//! - Add `fdaGranted` and `managedByOrganization` explicitly (runtime state, not settings). The
//!   latter is one coarse bool: never which keys an organization set.

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
    "appearance.language",
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
    "behavior.archiveEnter.zip",
    "behavior.archiveEnter.ooxml",
    "behavior.archiveEnter.bundle",
    "behavior.fileSystemWatching.downloadsNotifications",
    "behavior.fileSystemWatching.lowDiskSpaceNotifications",
    "ai.provider",
    "ai.cloudProvider",
    "ai.localContextSize",
    "network.timeoutMode",
];

/// The runtime state the snapshot carries beside the settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuntimeState {
    pub fda_granted: bool,
    /// Whether an organization's managed policy restricts anything on this Mac.
    pub managed_by_organization: bool,
}

/// Builds the config-shape object from the raw `settings.json` value plus the runtime FDA-granted
/// flag. Pure: no I/O, so it's directly unit-testable against a seeded settings JSON.
///
/// `raw_settings` is the parsed `settings.json` (a flat object with dot-notation string keys). A
/// non-object value (missing/corrupt file) yields a snapshot with only `fdaGranted`.
pub fn build_config_shape(raw_settings: &Value, runtime: RuntimeState) -> Value {
    let mut shape = Map::new();

    if let Some(obj) = raw_settings.as_object() {
        for (key, value) in obj {
            if include_key(key, value) {
                shape.insert(key.clone(), value.clone());
            }
        }
    }

    // Runtime state, not settings, so added explicitly. Last so neither can be shadowed by a
    // (nonexistent) same-named setting.
    shape.insert("fdaGranted".to_string(), Value::Bool(runtime.fda_granted));
    shape.insert(
        "managedByOrganization".to_string(),
        Value::Bool(runtime.managed_by_organization),
    );

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
            "listing.showHiddenFiles": true,
            "listing.briefColumnWidthMaxPx": 320,
            "appearance.textSize": 125.0,
        });
        let shape = build_config_shape(&settings, RuntimeState::default());
        assert_eq!(shape["listing.showHiddenFiles"], json!(true));
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
        let shape = build_config_shape(&settings, RuntimeState::default());
        assert_eq!(shape["theme.mode"], json!("dark"));
        assert_eq!(shape["ai.provider"], json!("cloud"));
        assert_eq!(shape["listing.directorySortMode"], json!("name"));
    }

    #[test]
    fn includes_the_ui_language_setting() {
        // A BCP-47 tag or the `system` sentinel: a fixed vocabulary drawn from the
        // shipped catalogs, so it belongs on the allowlist rather than being dropped
        // with the free-text strings.
        let picked = build_config_shape(&json!({ "appearance.language": "hu" }), RuntimeState::default());
        assert_eq!(picked["appearance.language"], json!("hu"));

        let following_the_os = build_config_shape(&json!({ "appearance.language": "system" }), RuntimeState::default());
        assert_eq!(following_the_os["appearance.language"], json!("system"));
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
        let shape = build_config_shape(&settings, RuntimeState::default());
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
        let shape = build_config_shape(&settings, RuntimeState::default());
        let obj = shape.as_object().expect("object");
        assert!(!obj.contains_key("someObject"));
        assert!(!obj.contains_key("someArray"));
        assert!(!obj.contains_key("someNull"));
    }

    #[test]
    fn adds_fda_granted_explicitly() {
        let shape = build_config_shape(&json!({}), fda(true));
        assert_eq!(shape["fdaGranted"], json!(true));

        let shape_denied = build_config_shape(&json!({}), fda(false));
        assert_eq!(shape_denied["fdaGranted"], json!(false));
    }

    #[test]
    fn adds_the_managed_flag_explicitly() {
        let managed = RuntimeState {
            managed_by_organization: true,
            ..RuntimeState::default()
        };
        assert_eq!(
            build_config_shape(&json!({}), managed)["managedByOrganization"],
            json!(true)
        );
        assert_eq!(
            build_config_shape(&json!({}), fda(true))["managedByOrganization"],
            json!(false)
        );
    }

    fn fda(granted: bool) -> RuntimeState {
        RuntimeState {
            fda_granted: granted,
            ..RuntimeState::default()
        }
    }

    #[test]
    fn non_object_settings_yields_only_the_runtime_flags() {
        // A missing/corrupt settings file parses to something non-object; the snapshot still has
        // a valid shape carrying just the runtime flags.
        let shape = build_config_shape(&json!("not an object"), fda(true));
        let obj = shape.as_object().expect("object");
        assert_eq!(obj.len(), 2);
        assert_eq!(obj["fdaGranted"], json!(true));
        assert_eq!(obj["managedByOrganization"], json!(false));
    }
}
