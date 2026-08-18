//! Persistent recent-selections store for the Selection dialog.
//!
//! The list machinery lives in `crate::recents`; this module owns the entry shape
//! and the canonical key. Both come from `crate::search::history` narrowed:
//! Selection runs in the focused folder only, so there's no `scope` and no
//! `exclude_system_dirs`.
//!
//! [`HistoryMode`] and [`HistoryFilters`] are re-exported from `crate::search::history`
//! so the frontend renders mode badges and filter chips the same way for both
//! consumers. The entry struct stays separate, so the on-disk schema doesn't bind
//! Selection to Search's canonical-key shape.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};
use crate::search::history::{filters_fingerprint, flag, normalize_query};

// Re-export the shared types so the frontend bindings see the same wire shape for both
// consumers. Keeping these in `search::history` (rather than splitting into a third
// "history-shared" module) avoids churn until a future consumer actually needs them.
pub use crate::search::history::{HistoryFilters, HistoryMode};

/// Default cap when the user hasn't tuned `selection.recentSelections.maxCount`.
pub const DEFAULT_MAX_COUNT: usize = 1000;

/// The recent-selections list. Loaded at startup; read and written through
/// `crate::commands::selection`.
pub static RECENT_SELECTIONS: RecentsFile<SelectionHistoryEntry> = RecentsFile::new();

/// A single recent-selection entry, persisted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SelectionHistoryEntry {
    pub id: String,
    /// Unix epoch milliseconds.
    pub timestamp: i64,
    pub mode: HistoryMode,
    pub query: String,
    #[serde(default)]
    pub filters: HistoryFilters,
    pub case_sensitive: bool,
    /// Number of entries the matcher selected when the user committed this query.
    /// Equivalent to Search's `result_count`; renamed because Selection "matches"
    /// rather than "returns results".
    pub match_count: u32,
}

impl RecentEntry for SelectionHistoryEntry {
    const FILENAME: &'static str = "selection-history.json";
    const LOG_TARGET: &'static str = "selection::history";
    const LOG_NAME: &'static str = "selection history";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// Four segments, against Search's six: Selection always runs in the current
    /// folder, so it has no `scope` and no `exclude_system_dirs` to key on.
    fn dedupe_key(&self) -> String {
        let mode = self.mode.as_str();
        let query = normalize_query(&self.query);
        let filters = filters_fingerprint(&self.filters);
        let cs = flag(self.case_sensitive);
        format!("{mode}|{query}|{filters}|{cs}")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(mode: HistoryMode, query: &str) -> SelectionHistoryEntry {
        SelectionHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: 1_700_000_000_000,
            mode,
            query: query.to_string(),
            filters: HistoryFilters::default(),
            case_sensitive: false,
            match_count: 0,
        }
    }

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
    fn key_distinguishes_the_case_sensitive_flag() {
        let mut a = entry(HistoryMode::Filename, "*.pdf");
        let b = entry(HistoryMode::Filename, "*.pdf");
        a.case_sensitive = true;
        assert_ne!(a.dedupe_key(), b.dedupe_key());
    }

    #[test]
    fn key_carries_no_scope_or_exclude_system_dirs() {
        // Search's key has 6 segments; Selection's has 4
        // (mode | normalized_query | filters | case_sensitive). The narrower shape is
        // load-bearing: it prevents accidentally re-introducing scope-style fields.
        let e = entry(HistoryMode::Filename, "*.pdf");
        assert_eq!(
            e.dedupe_key().split('|').count(),
            4,
            "selection key should have exactly 4 segments"
        );
    }

    #[test]
    fn entry_serialization_round_trip() {
        let e = SelectionHistoryEntry {
            id: "abc-123".to_string(),
            timestamp: 1_700_000_000_000,
            mode: HistoryMode::Ai,
            query: "logs from this week".to_string(),
            filters: HistoryFilters {
                size_min: Some(1024),
                modified_after: Some("2026-01-01".to_string()),
                ..Default::default()
            },
            case_sensitive: false,
            match_count: 17,
        };
        let json = serde_json::to_string_pretty(&e).unwrap();
        // camelCase serialization
        assert!(json.contains("\"caseSensitive\""));
        assert!(json.contains("\"matchCount\""));
        assert!(json.contains("\"sizeMin\": 1024"));
        // Mode lowercase
        assert!(json.contains("\"mode\": \"ai\""));
        // No scope or excludeSystemDirs leaked from search's struct.
        assert!(!json.contains("scope"));
        assert!(!json.contains("excludeSystemDirs"));

        let back: SelectionHistoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn a_file_from_the_previous_build_still_loads() {
        // What the writer emitted before the list moved to `crate::recents`.
        let legacy = r#"{
  "_schemaVersion": 1,
  "entries": [
    {
      "id": "abc-123",
      "timestamp": 1700000000000,
      "mode": "regex",
      "query": "^report.*",
      "filters": {
        "sizeMin": null,
        "sizeMax": null,
        "modifiedAfter": null,
        "modifiedBefore": null,
        "isDirectory": true
      },
      "caseSensitive": true,
      "matchCount": 7
    }
  ]
}"#;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(SelectionHistoryEntry::FILENAME);
        std::fs::write(&path, legacy).expect("write");

        let list = RecentsFile::<SelectionHistoryEntry>::new();
        list.load_at(&path);

        let entries = list.entries(None);
        assert_eq!(entries.len(), 1, "the legacy file should have loaded, not quarantined");
        assert_eq!(entries[0].query, "^report.*");
        assert_eq!(entries[0].match_count, 7);
        assert!(entries[0].case_sensitive);
        assert_eq!(entries[0].filters.is_directory, Some(true));
    }

    #[test]
    fn default_cap_is_a_thousand() {
        assert_eq!(DEFAULT_MAX_COUNT, 1000);
    }
}
