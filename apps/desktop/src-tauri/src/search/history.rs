//! Persistent recent-searches store for the search dialog.
//!
//! Adds an entry only when the user clicks "Open in pane" — see the FE
//! `lib/search/CLAUDE.md` for the call-site rule.
//!
//! The list machinery (dedupe, cap, the durable file, quarantine) lives in
//! `crate::recents`; this module owns the entry shape and the canonical key that
//! decides when two searches ask the same question. It also owns the two pure
//! types both query-shaped histories share, [`HistoryMode`] and [`HistoryFilters`],
//! plus the key-building helpers over them, which `crate::selection::history`
//! reuses so a new filter field can't teach one key about itself and not the other.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::recents::{RecentEntry, RecentsFile};

/// Default cap when the user hasn't tuned `search.recentSearches.maxCount`.
pub const DEFAULT_MAX_COUNT: usize = 1000;

/// The recent-searches list. Loaded at startup; read and written through
/// `crate::commands::search`.
pub static RECENT_SEARCHES: RecentsFile<HistoryEntry> = RecentsFile::new();

/// Search modes recorded in history. Mirrors the frontend `SearchMode` union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMode {
    Ai,
    Filename,
    Regex,
}

impl HistoryMode {
    /// The lowercase wire form, which is also what the canonical key carries.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            HistoryMode::Ai => "ai",
            HistoryMode::Filename => "filename",
            HistoryMode::Regex => "regex",
        }
    }
}

/// Filter slice of a history entry. Mirrors what the dialog carries on the wire.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryFilters {
    #[serde(default)]
    pub size_min: Option<u64>,
    #[serde(default)]
    pub size_max: Option<u64>,
    #[serde(default)]
    pub modified_after: Option<String>,
    #[serde(default)]
    pub modified_before: Option<String>,
    /// Type filter, round-tripping the frontend `typeFilter` toggle:
    /// `Some(true) = folder`, `Some(false) = file`, `None = both`. Additive with
    /// `#[serde(default)]`, so older history files (no `isDirectory` key) load as `None`
    /// without a schema bump.
    #[serde(default)]
    pub is_directory: Option<bool>,
}

/// A single recent-search entry, persisted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    /// Unix epoch milliseconds.
    pub timestamp: i64,
    pub mode: HistoryMode,
    pub query: String,
    #[serde(default)]
    pub filters: HistoryFilters,
    pub scope: String,
    pub case_sensitive: bool,
    pub exclude_system_dirs: bool,
    pub result_count: u32,
}

impl RecentEntry for HistoryEntry {
    const FILENAME: &'static str = "search-history.json";
    const LOG_TARGET: &'static str = "search::history";
    const LOG_NAME: &'static str = "search history";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Six segments: two entries with the same one asked the same question, so the
    /// newest copy wins and the older one goes. Selection's key is the same minus
    /// `scope` and `exclude_system_dirs`, which it has no concept of.
    fn dedupe_key(&self) -> String {
        let mode = self.mode.as_str();
        let query = normalize_query(&self.query);
        let filters = filters_fingerprint(&self.filters);
        // Scope: trim + lowercase so "/Users" and " /users " collapse.
        let scope = self.scope.trim().to_lowercase();
        let cs = flag(self.case_sensitive);
        let es = flag(self.exclude_system_dirs);
        format!("{mode}|{query}|{filters}|{scope}|{cs}|{es}")
    }
}

// ---------------------------------------------------------------------------
// Canonical-key pieces, shared with `crate::selection::history`. The key itself is
// never persisted; it only exists at compare time.
// ---------------------------------------------------------------------------

/// Collapses a query to its comparable form: trimmed, internal whitespace runs
/// squeezed to single spaces, lowercased.
pub(crate) fn normalize_query(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// Renders the filter slice into the key: keys sorted alphabetically, undefined
/// fields skipped entirely. `BTreeMap` gives the sort and keeps the key set
/// explicit, and sharing this is what keeps a newly-added filter field from
/// reaching one consumer's key and not the other's.
pub(crate) fn filters_fingerprint(filters: &HistoryFilters) -> String {
    let mut filter_kv: BTreeMap<&str, String> = BTreeMap::new();
    if let Some(v) = filters.size_min {
        filter_kv.insert("sizeMin", v.to_string());
    }
    if let Some(v) = filters.size_max {
        filter_kv.insert("sizeMax", v.to_string());
    }
    if let Some(ref v) = filters.modified_after {
        filter_kv.insert("modifiedAfter", v.clone());
    }
    if let Some(ref v) = filters.modified_before {
        filter_kv.insert("modifiedBefore", v.clone());
    }
    if let Some(v) = filters.is_directory {
        filter_kv.insert("isDirectory", v.to_string());
    }
    filter_kv
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// A bool as the key's single-char form.
pub(crate) fn flag(value: bool) -> &'static str {
    if value { "t" } else { "f" }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mode: HistoryMode, query: &str) -> HistoryEntry {
        HistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
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
    fn is_directory_filter_round_trips_without_schema_bump() {
        // The type filter is an additive `#[serde(default)]` field: a new value serializes
        // and deserializes cleanly, AND an old file missing the key still loads (as `None`),
        // all on schema v1. This pins "no schema bump needed".
        assert_eq!(
            HistoryEntry::SCHEMA_VERSION,
            1,
            "the type filter must NOT bump the schema"
        );

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

    // -- On-disk compatibility --

    #[test]
    fn a_file_from_the_previous_build_still_loads() {
        // What the writer emitted before the list moved to `crate::recents`. Nothing
        // about the envelope or the field names may drift: if it does, a user's
        // recent searches quietly vanish on the next launch.
        let legacy = r#"{
  "_schemaVersion": 1,
  "entries": [
    {
      "id": "abc-123",
      "timestamp": 1700000000000,
      "mode": "filename",
      "query": "*.pdf",
      "filters": {
        "sizeMin": 1024,
        "sizeMax": null,
        "modifiedAfter": null,
        "modifiedBefore": null,
        "isDirectory": null
      },
      "scope": "/Users/test",
      "caseSensitive": false,
      "excludeSystemDirs": true,
      "resultCount": 42
    }
  ]
}"#;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HistoryEntry::FILENAME);
        std::fs::write(&path, legacy).expect("write");

        let list = RecentsFile::<HistoryEntry>::new();
        list.load_at(&path);

        let entries = list.entries(None);
        assert_eq!(entries.len(), 1, "the legacy file should have loaded, not quarantined");
        assert_eq!(entries[0].id, "abc-123");
        assert_eq!(entries[0].query, "*.pdf");
        assert_eq!(entries[0].scope, "/Users/test");
        assert_eq!(entries[0].result_count, 42);
        assert!(entries[0].exclude_system_dirs);
        assert_eq!(entries[0].filters.size_min, Some(1024));
    }

    #[test]
    fn default_cap_is_a_thousand() {
        assert_eq!(DEFAULT_MAX_COUNT, 1000);
    }
}
