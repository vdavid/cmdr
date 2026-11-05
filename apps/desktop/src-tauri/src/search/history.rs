//! Persistent recent-searches store for the search dialog.
//!
//! Adds an entry only when the user clicks "Open in pane"; see the FE
//! `lib/search/CLAUDE.md` for the call-site rule.
//!
//! The entry, its canonical key, and the key helpers Selection shares are
//! `cmdr_search::history`'s; the list rules are `cmdr-recents`'; the durable file
//! and its quarantine are `crate::recents`'. This module ties them into the one
//! list the app keeps.

use crate::recents::{RecentEntry, RecentsFile};

pub use cmdr_search::history::{
    DEFAULT_MAX_COUNT, HistoryEntry, HistoryFilters, HistoryMode, filters_fingerprint, flag, normalize_query,
};

/// The recent-searches list. Loaded at startup; read and written through
/// `crate::commands::search`.
pub static RECENT_SEARCHES: RecentsFile<HistoryEntry> = RecentsFile::new();

impl RecentEntry for HistoryEntry {
    const FILENAME: &'static str = "search-history.json";
    const LOG_TARGET: &'static str = "search::history";
    const LOG_NAME: &'static str = "search history";
    const STATE_FILE: crate::state_recovery::StateFile = crate::state_recovery::StateFile::SearchHistory;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_type_filter_needs_no_schema_bump() {
        // The type filter is an additive `#[serde(default)]` field (the crate's
        // round-trip test loads an entry without it), so the file stays on v1.
        assert_eq!(
            HistoryEntry::SCHEMA_VERSION,
            1,
            "the type filter must NOT bump the schema"
        );
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
}
