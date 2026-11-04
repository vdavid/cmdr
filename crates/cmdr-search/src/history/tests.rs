use super::*;

fn entry(mode: HistoryMode, query: &str) -> HistoryEntry {
    HistoryEntry {
        id: "id-1".to_string(),
        timestamp: 1_700_000_000_000,
        mode,
        query: query.to_string(),
        filters: HistoryFilters::default(),
        scope: String::new(),
        case_sensitive: false,
        exclude_system_dirs: true,
        result_count: 0,
    }
}

// -- The shared key pieces --

#[test]
fn normalize_query_collapses_whitespace_and_case() {
    assert_eq!(normalize_query("  Foo   Bar  "), "foo bar");
    assert_eq!(normalize_query("foo bar"), "foo bar");
}

#[test]
fn filters_fingerprint_orders_keys_alphabetically() {
    let a = HistoryFilters {
        size_min: Some(1024),
        modified_after: Some("2026-01-01".to_string()),
        ..Default::default()
    };
    // Same values, assigned in the other order: field order doesn't matter.
    let b = HistoryFilters {
        modified_after: Some("2026-01-01".to_string()),
        size_min: Some(1024),
        ..Default::default()
    };

    assert_eq!(filters_fingerprint(&a), filters_fingerprint(&b));
    assert_eq!(filters_fingerprint(&a), "modifiedAfter=2026-01-01,sizeMin=1024");
}

#[test]
fn filters_fingerprint_skips_undefined_fields() {
    assert_eq!(filters_fingerprint(&HistoryFilters::default()), "");
}

#[test]
fn filters_fingerprint_distinguishes_the_type_filter() {
    let folder = HistoryFilters {
        is_directory: Some(true),
        ..Default::default()
    };
    let file = HistoryFilters {
        is_directory: Some(false),
        ..Default::default()
    };
    assert_ne!(filters_fingerprint(&folder), filters_fingerprint(&file));
    assert_ne!(
        filters_fingerprint(&folder),
        filters_fingerprint(&HistoryFilters::default())
    );
}

// -- The search key --

#[test]
fn key_collapses_whitespace_and_case() {
    let a = entry(HistoryMode::Filename, "  Foo   Bar  ");
    let b = entry(HistoryMode::Filename, "foo bar");
    assert_eq!(a.dedupe_key(), b.dedupe_key());
}

#[test]
fn key_distinguishes_modes() {
    let f = entry(HistoryMode::Filename, "*.pdf");
    let r = entry(HistoryMode::Regex, "*.pdf");
    assert_ne!(f.dedupe_key(), r.dedupe_key());
}

#[test]
fn key_distinguishes_scope_and_flags() {
    let mut a = entry(HistoryMode::Filename, "*.pdf");
    let mut b = entry(HistoryMode::Filename, "*.pdf");
    b.scope = "/Users".to_string();
    assert_ne!(a.dedupe_key(), b.dedupe_key());

    a.case_sensitive = true;
    let c = entry(HistoryMode::Filename, "*.pdf");
    assert_ne!(a.dedupe_key(), c.dedupe_key());

    let mut d = entry(HistoryMode::Filename, "*.pdf");
    d.exclude_system_dirs = false;
    assert_ne!(c.dedupe_key(), d.dedupe_key());
}

#[test]
fn key_carries_scope_and_exclude_system_dirs() {
    // Six segments, against Selection's four. The shape is load-bearing: it's
    // what keeps the two dedupe keys from silently agreeing.
    let e = entry(HistoryMode::Filename, "*.pdf");
    assert_eq!(e.dedupe_key().split('|').count(), 6);
}

#[test]
fn adding_the_same_question_again_keeps_one_row_on_top() {
    // The list rule every host shares, over this entry's key: a browser host's
    // session list dedupes exactly as the desktop's file does.
    let mut list = vec![entry(HistoryMode::Filename, "*.pdf")];
    let mut again = entry(HistoryMode::Filename, "  *.PDF ");
    again.result_count = 7;
    cmdr_recents::add_to(&mut list, again, DEFAULT_MAX_COUNT, || "id-2".to_string());
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].result_count, 7, "the newest copy wins");

    // A different question with a taken id gets a fresh one.
    let other = entry(HistoryMode::Regex, "*.pdf");
    cmdr_recents::add_to(&mut list, other, DEFAULT_MAX_COUNT, || "id-3".to_string());
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, "id-3");
}

// -- Serialization round-trip --

#[test]
fn entry_serialization_round_trip() {
    let e = HistoryEntry {
        id: "abc-123".to_string(),
        timestamp: 1_700_000_000_000,
        mode: HistoryMode::Ai,
        query: "screenshots".to_string(),
        filters: HistoryFilters {
            size_min: Some(1024),
            modified_after: Some("2026-01-01".to_string()),
            ..Default::default()
        },
        scope: "/Users/test".to_string(),
        case_sensitive: false,
        exclude_system_dirs: true,
        result_count: 42,
    };
    let json = serde_json::to_string_pretty(&e).unwrap();
    // camelCase serialization
    assert!(json.contains("\"caseSensitive\""));
    assert!(json.contains("\"excludeSystemDirs\""));
    assert!(json.contains("\"resultCount\""));
    assert!(json.contains("\"sizeMin\": 1024"));
    // Mode lowercase
    assert!(json.contains("\"mode\": \"ai\""));

    let back: HistoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back, e);
}

#[test]
fn is_directory_filter_round_trips_and_an_old_entry_loads_without_it() {
    let mut e = entry(HistoryMode::Filename, "*.png");
    e.filters.is_directory = Some(true);
    let json = serde_json::to_string(&e).unwrap();
    assert!(json.contains("\"isDirectory\":true"));
    let back: HistoryEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(back.filters.is_directory, Some(true));

    // An old entry (filters object with no `isDirectory` key) loads as `None`.
    let legacy = r#"{"id":"x","timestamp":1,"mode":"filename","query":"*.png","filters":{"sizeMin":1024},"scope":"","caseSensitive":false,"excludeSystemDirs":true,"resultCount":0}"#;
    let parsed: HistoryEntry = serde_json::from_str(legacy).unwrap();
    assert_eq!(parsed.filters.is_directory, None);
    assert_eq!(parsed.filters.size_min, Some(1024));
}

#[test]
fn default_cap_is_a_thousand() {
    assert_eq!(DEFAULT_MAX_COUNT, 1000);
}
