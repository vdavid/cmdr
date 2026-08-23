//! Unit tests for the settings loader's parsing surface.
//!
//! A child module of `loader` (wired in with `#[path]`), so the tests reach its
//! private `parse_settings` / `parse_restricted_window_settings` helpers directly
//! rather than through a public seam that only exists for them.

use super::*;

/// The launch that performs the frontend's schema-4 migration reads `settings.json`
/// before the frontend runs, so it still sees the pre-migration key. Missing this
/// would close the FDA gate on a user who answered Deny long ago, silently skipping
/// drive indexing and the Downloads watcher for that launch.
#[test]
fn fda_choice_falls_back_to_the_pre_migration_key() {
    let legacy = r#"{ "fullDiskAccessChoice": "deny" }"#;
    assert_eq!(
        parse_settings(legacy).unwrap().full_disk_access_choice,
        FullDiskAccessChoice::Deny
    );
}

#[test]
fn fda_choice_prefers_the_registry_key_once_the_migration_has_run() {
    // Both present can't happen (the migration deletes the legacy key as it moves the
    // value), but if it ever did, the registry key is the live one.
    let both = r#"{ "onboarding.fullDiskAccessChoice": "allow", "fullDiskAccessChoice": "deny" }"#;
    assert_eq!(
        parse_settings(both).unwrap().full_disk_access_choice,
        FullDiskAccessChoice::Allow
    );
}

#[test]
fn fda_choice_defaults_to_not_asked_yet_when_absent() {
    assert_eq!(
        parse_settings("{}").unwrap().full_disk_access_choice,
        FullDiskAccessChoice::NotAskedYet
    );
}

/// Dotfiles stay out of the way until someone asks for them. Three defaults have to
/// agree or the app contradicts itself between a fresh install, an existing
/// `settings.json`, and the native View menu built from whatever this returns.
#[test]
fn hidden_files_are_off_unless_the_user_turned_them_on() {
    // A `settings.json` with no such key: the sparse frontend store writes the key only
    // when an actor sets it, so "absent" means "never chose".
    assert!(!parse_settings("{}").unwrap().show_hidden_files);
    // No file at all, or an unreadable one: `load_settings` falls back to this, and the
    // View menu's checked state is built from it.
    assert!(!Settings::default().show_hidden_files);
}

#[test]
fn an_explicit_show_hidden_files_choice_wins_over_the_default() {
    // Someone who turned dotfiles on keeps them on; someone who turned them off keeps
    // that too, even though it now matches the default.
    let on = r#"{ "listing.showHiddenFiles": true }"#;
    assert!(parse_settings(on).unwrap().show_hidden_files);
    let off = r#"{ "listing.showHiddenFiles": false }"#;
    assert!(!parse_settings(off).unwrap().show_hidden_files);
}

#[test]
fn restricted_window_settings_parse_set_values() {
    let json = r#"{
        "viewer.wordWrap": true,
        "fileViewer.suppressBinaryWarning": true,
        "appearance.textSize": 125,
        "appearance.appColor": "blue",
        "developer.mcpEnabled": true
    }"#;
    let parsed = parse_restricted_window_settings(json);
    assert_eq!(parsed.viewer_word_wrap, Some(true));
    assert_eq!(parsed.file_viewer_suppress_binary_warning, Some(true));
    assert_eq!(parsed.appearance_text_size, Some(125.0));
    assert_eq!(parsed.appearance_app_color.as_deref(), Some("blue"));
}

#[test]
fn restricted_window_settings_carry_the_size_format() {
    // The operation queue window is restricted (no `store:default`) but renders
    // `<Size>`, so it needs the binary/SI choice in the snapshot. Without it
    // the window silently falls back to the registry default and shows a
    // different number than the copy dialog for the same byte count.
    let json = r#"{ "appearance.fileSizeFormat": "si" }"#;
    let parsed = parse_restricted_window_settings(json);
    assert_eq!(parsed.appearance_file_size_format.as_deref(), Some("si"));
}

#[test]
fn restricted_window_settings_carry_the_pinned_ui_language() {
    // The viewer and queue windows are restricted (no `store:default`) but each
    // resolves its own UI language through `initWindowLanguageSync`. Without the
    // pinned setting in the snapshot they read the registry default `'system'`,
    // so a user who chose Hungarian on an English Mac would get an English
    // viewer while every other window spoke Hungarian.
    let json = r#"{ "appearance.language": "hu" }"#;
    let parsed = parse_restricted_window_settings(json);
    assert_eq!(parsed.appearance_language.as_deref(), Some("hu"));
}

#[test]
fn operation_log_retention_defaults_forever_and_3gb() {
    // Absent keys ⇒ forever age, 3 GB size.
    let limits = parse_operation_log_retention_limits("{}");
    assert_eq!(limits.max_age_secs, None);
    assert_eq!(limits.max_size_bytes, Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES));
    // Bad JSON ⇒ same defaults.
    let bad = parse_operation_log_retention_limits("not json");
    assert_eq!(bad.max_age_secs, None);
    assert_eq!(bad.max_size_bytes, Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES));
}

#[test]
fn operation_log_retention_reads_persisted_values() {
    // Age in ms → seconds; size in bytes verbatim.
    let json = r#"{ "operationLog.maxAge": 90000, "operationLog.maxSize": 104857600 }"#;
    let limits = parse_operation_log_retention_limits(json);
    assert_eq!(limits.max_age_secs, Some(90), "90000 ms ⇒ 90 s");
    assert_eq!(limits.max_size_bytes, Some(104_857_600));
}

#[test]
fn operation_log_retention_matches_frontend_registry_values() {
    // Round-trip guard: the exact values the settings registry
    // (`settings-registry.ts`) persists must produce the intended limits, so a
    // drift on either side (a changed preset ms/byte constant, or a renamed
    // key) fails here rather than silently mis-pruning. The `operationLog.maxAge`
    // "Forever" default (0) and the 3 GB `operationLog.maxSize` default are the
    // registry `default` values; the 30-day age preset and 1 GB size preset are
    // registry option values.
    let defaults =
        parse_operation_log_retention_limits(r#"{ "operationLog.maxAge": 0, "operationLog.maxSize": 3221225472 }"#);
    assert_eq!(
        defaults.max_age_secs, None,
        "the age default (0) is the Forever sentinel"
    );
    assert_eq!(
        defaults.max_size_bytes,
        Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES),
        "the 3 GB size default must equal the backend's byte constant"
    );
    assert_eq!(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES, 3_221_225_472);

    let presets = parse_operation_log_retention_limits(
        r#"{ "operationLog.maxAge": 2592000000, "operationLog.maxSize": 1073741824 }"#,
    );
    assert_eq!(presets.max_age_secs, Some(2_592_000), "30 days in ms ⇒ seconds");
    assert_eq!(presets.max_size_bytes, Some(1_073_741_824), "1 GB preset in bytes");
}

#[test]
fn operation_log_retention_zero_sentinels_mean_unlimited() {
    // Age 0 = the "Forever" sentinel; size 0 = unlimited.
    let json = r#"{ "operationLog.maxAge": 0, "operationLog.maxSize": 0 }"#;
    let limits = parse_operation_log_retention_limits(json);
    assert_eq!(limits.max_age_secs, None);
    assert_eq!(limits.max_size_bytes, None);
}

#[test]
fn restricted_window_settings_missing_keys_are_none() {
    let parsed = parse_restricted_window_settings("{}");
    assert_eq!(parsed.viewer_word_wrap, None);
    assert_eq!(parsed.file_viewer_suppress_binary_warning, None);
    assert_eq!(parsed.appearance_text_size, None);
    assert_eq!(parsed.appearance_app_color, None);
    assert_eq!(parsed.appearance_file_size_format, None);
    // `None` here means `'system'`: follow the OS, which is the registry default.
    assert_eq!(parsed.appearance_language, None);
}

#[test]
fn restricted_window_settings_bad_json_yields_defaults() {
    let parsed = parse_restricted_window_settings("not json at all");
    assert_eq!(parsed.viewer_word_wrap, None);
    assert_eq!(parsed.appearance_app_color, None);
}

#[test]
fn restricted_window_settings_wrong_types_are_none() {
    let json = r#"{ "viewer.wordWrap": "yes", "appearance.textSize": "big" }"#;
    let parsed = parse_restricted_window_settings(json);
    assert_eq!(parsed.viewer_word_wrap, None);
    assert_eq!(parsed.appearance_text_size, None);
}

#[test]
fn parses_drive_indexing_freshness_keys() {
    // The drive-indexing freshness toggles round-trip from their literal
    // dot-notation keys. A missing key stays `None` (the FE applies the
    // registry default: both ON).
    let json = r#"{ "indexing.askForEachDrive": false, "indexing.staleNotify": true }"#;
    let parsed = parse_settings(json).expect("valid settings json");
    assert_eq!(parsed.indexing_ask_for_each_drive, Some(false));
    assert_eq!(parsed.indexing_stale_notify, Some(true));

    let empty = parse_settings("{}").expect("empty settings json");
    assert_eq!(
        empty.indexing_ask_for_each_drive, None,
        "missing key → None (FE default)"
    );
    assert_eq!(empty.indexing_stale_notify, None, "missing key → None (FE default)");
}

#[test]
fn chat_memory_size_reads_a_preset_and_treats_everything_else_as_automatic() {
    assert_eq!(
        parse_ask_cmdr_chat_memory_size(r#"{ "askCmdr.chatMemorySize": "32000" }"#),
        Some(32_000)
    );
    // Automatic is the absence of a number, however it's spelled: the budget module then
    // follows the model's own window. A hand-edited or half-written value must not become a
    // budget nobody chose.
    for contents in [
        r#"{ "askCmdr.chatMemorySize": "auto" }"#,
        r#"{ "askCmdr.chatMemorySize": "" }"#,
        r#"{ "askCmdr.chatMemorySize": "0" }"#,
        r#"{ "askCmdr.chatMemorySize": 32000 }"#, // a number, not the stored string form
        "{}",
        "not json at all",
    ] {
        assert_eq!(
            parse_ask_cmdr_chat_memory_size(contents),
            None,
            "{contents} must read as Automatic"
        );
    }
}

/// ⚠️ `settings.json` is SPARSE: it holds only what an actor explicitly set, so the proactive
/// key is absent for every user who never touched the row. The absence has to read as "unset"
/// rather than as `false`, or the loader cannot tell "the user turned it off" apart from "the
/// user never saw it" and the registry default has nowhere to apply.
#[test]
fn the_proactive_toggle_reads_as_unset_when_nobody_has_touched_it() {
    assert_eq!(parse_ask_cmdr_proactive(r#"{ "askCmdr.proactive": true }"#), Some(true));
    assert_eq!(
        parse_ask_cmdr_proactive(r#"{ "askCmdr.proactive": false }"#),
        Some(false)
    );
    for contents in [
        "{}",
        r#"{ "askCmdr.proactive": "true" }"#, // a string, not the stored boolean form
        "not json at all",
    ] {
        assert_eq!(
            parse_ask_cmdr_proactive(contents),
            None,
            "{contents} must read as unset, so the registry default decides"
        );
    }
}

/// The cadence is stored as a plain count of seconds, which is what the slider persists. A
/// zero, a negative, or a string is not a cadence anybody chose, so it reads as unset and the
/// default applies rather than becoming a wake every no seconds.
#[test]
fn the_wake_delay_reads_seconds_and_rejects_what_is_not_a_cadence() {
    assert_eq!(
        parse_ask_cmdr_wake_delay_secs(r#"{ "askCmdr.wakeDelay": 300 }"#),
        Some(300)
    );
    for contents in [
        "{}",
        r#"{ "askCmdr.wakeDelay": "300" }"#,
        r#"{ "askCmdr.wakeDelay": 0 }"#,
        r#"{ "askCmdr.wakeDelay": -5 }"#,
        "not json at all",
    ] {
        assert_eq!(
            parse_ask_cmdr_wake_delay_secs(contents),
            None,
            "{contents} must read as unset, so the registry default decides"
        );
    }
}
