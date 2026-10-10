//! Persistent recent-paths store for the "Go to path" dialog.
//!
//! The list machinery lives in `crate::recents`; this module only says what an
//! entry is, what makes two of them the same, and how many we keep.
//!
//! The dialog records the **resolved target it actually jumped to** (a dir, the
//! file path, or the nearest ancestor), never the raw typed input. Populated only
//! by manual jumps in the dialog (matching the search-history "record only on the
//! explicit action" precedent); the Rust side doesn't enforce that gate, the
//! frontend's only `add` call site does.

use serde::{Deserialize, Serialize};

use crate::recents::{RecentEntry, RecentsFile};

/// Fixed cap on recent paths. Not a setting: the dialog shows at most 10 recents
/// (digit keys 1-9, 0), so the store mirrors that hard limit.
pub const MAX_RECENTS: usize = 10;

/// The recent-paths list. Loaded at startup; read and written through
/// `crate::commands::go_to_path`.
pub static RECENT_PATHS: RecentsFile<RecentPathEntry> = RecentsFile::new();

/// A single recent-path entry, persisted verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RecentPathEntry {
    pub id: String,
    /// Unix epoch milliseconds.
    pub timestamp: i64,
    /// The resolved target we actually jumped to (dir, file, or ancestor).
    pub path: String,
}

impl RecentEntry for RecentPathEntry {
    const FILENAME: &'static str = "go-to-path-history.json";
    const LOG_TARGET: &'static str = "go_to_path::history";
    const LOG_NAME: &'static str = "go-to-path history";

    fn id(&self) -> &str {
        &self.id
    }

    fn set_id(&mut self, id: String) {
        self.id = id;
    }

    /// The resolved path itself. **Case-sensitivity is a v1 limitation**: on
    /// case-insensitive APFS `/Users/x/Foo` and `/Users/x/foo` are the same dir but
    /// show as two rows. Accepted (worst case: a duplicate-looking row). We don't
    /// `canonicalize()` to fix it; the symlink and nearest-ancestor reasons live in
    /// `DETAILS.md`.
    fn dedupe_key(&self) -> String {
        self.path.clone()
    }
}

/// Respells every recent path under a saved server's old address with its new one
/// (`network/server_move.rs`), by whole segments: the old address names nothing saved
/// any more, so a row spelled with it would open an Add sheet.
pub fn follow_server_move(old_prefix: &str, new_prefix: &str) {
    let file = crate::config::standalone_app_data_dir().map(|dir| dir.join(RecentPathEntry::FILENAME));
    follow_server_move_at(&RECENT_PATHS, file.as_deref(), old_prefix, new_prefix);
}

fn follow_server_move_at(
    list: &RecentsFile<RecentPathEntry>,
    file: Option<&std::path::Path>,
    old_prefix: &str,
    new_prefix: &str,
) {
    list.rewrite_at(file, |entry| {
        let Some(path) = cmdr_fs::volume::app_paths::rebase(&entry.path, old_prefix, new_prefix) else {
            return false;
        };
        entry.path = path;
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> RecentPathEntry {
        RecentPathEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: 1_700_000_000_000,
            path: path.to_string(),
        }
    }

    /// The old address names nothing saved any more, so a recent spelled with it would open an
    /// Add sheet. Whole segments: the server on another port keeps its own row.
    #[test]
    fn a_server_move_respells_the_recent_paths_under_its_old_address() {
        let list = RecentsFile::<RecentPathEntry>::new();
        list.add_at(None, entry("sftp://ada@nas.local:2222/srv"), MAX_RECENTS);
        list.add_at(None, entry("/Users/x/a"), MAX_RECENTS);
        list.add_at(None, entry("sftp://ada@nas.local:22/srv/data"), MAX_RECENTS);

        follow_server_move_at(&list, None, "sftp://ada@nas.local:22", "sftp://ada@10.0.0.5:22");

        let paths: Vec<String> = list.entries(None).into_iter().map(|e| e.path).collect();
        assert_eq!(
            paths,
            [
                "sftp://ada@10.0.0.5:22/srv/data",
                "/Users/x/a",
                "sftp://ada@nas.local:2222/srv"
            ]
        );
    }

    /// A list that already held the new spelling keeps one row for it, the newer one.
    #[test]
    fn a_server_move_onto_a_path_already_listed_keeps_one_row() {
        let list = RecentsFile::<RecentPathEntry>::new();
        list.add_at(None, entry("sftp://ada@10.0.0.5:22/srv"), MAX_RECENTS);
        list.add_at(None, entry("sftp://ada@nas.local:22/srv"), MAX_RECENTS);

        follow_server_move_at(&list, None, "sftp://ada@nas.local:22", "sftp://ada@10.0.0.5:22");

        let paths: Vec<String> = list.entries(None).into_iter().map(|e| e.path).collect();
        assert_eq!(paths, ["sftp://ada@10.0.0.5:22/srv"]);
    }

    #[test]
    fn two_jumps_to_one_path_are_the_same_entry() {
        assert_eq!(entry("/Users/x/a").dedupe_key(), entry("/Users/x/a").dedupe_key());
        assert_ne!(entry("/Users/x/a").dedupe_key(), entry("/Users/x/b").dedupe_key());
    }

    #[test]
    fn the_dedupe_key_is_case_sensitive() {
        // Pins the documented v1 limitation, so a change to it is deliberate.
        assert_ne!(entry("/Users/x/Foo").dedupe_key(), entry("/Users/x/foo").dedupe_key());
    }

    #[test]
    fn the_list_keeps_the_ten_newest_paths() {
        let list = RecentsFile::<RecentPathEntry>::new();
        for i in 0..15 {
            list.add_at(None, entry(&format!("/p/{i}")), MAX_RECENTS);
        }

        let kept = list.entries(None);
        assert_eq!(kept.len(), MAX_RECENTS);
        assert_eq!(kept[0].path, "/p/14", "newest first");
        assert_eq!(kept[MAX_RECENTS - 1].path, "/p/5");
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
      "path": "/Users/test/Documents"
    }
  ]
}"#;

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(RecentPathEntry::FILENAME);
        std::fs::write(&path, legacy).expect("write");

        let list = RecentsFile::<RecentPathEntry>::new();
        list.load_at(&path);

        let entries = list.entries(None);
        assert_eq!(entries.len(), 1, "the legacy file should have loaded, not quarantined");
        assert_eq!(entries[0].id, "abc-123");
        assert_eq!(entries[0].path, "/Users/test/Documents");
    }

    #[test]
    fn entry_serialization_round_trip() {
        let e = RecentPathEntry {
            id: "abc-123".to_string(),
            timestamp: 1_700_000_000_000,
            path: "/Users/test/Documents".to_string(),
        };
        let json = serde_json::to_string_pretty(&e).unwrap();
        assert!(json.contains("\"timestamp\""));
        assert!(json.contains("\"path\": \"/Users/test/Documents\""));

        let back: RecentPathEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, e);
    }
}
