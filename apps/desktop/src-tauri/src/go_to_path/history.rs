//! Persistent recent-paths store for the "Go to path" dialog.
//!
//! A trimmed clone of `search/history.rs` (the proven pattern). Simpler: an
//! entry is `{ id, timestamp, path }`, the dedupe key is the resolved path
//! string, and the cap is a fixed const, not a user setting.
//!
//! The dialog records the **resolved target it actually jumped to** (a dir, the
//! file path, or the nearest ancestor), never the raw typed input. Populated
//! only by manual jumps in the dialog (matching the search-history "record only
//! on the explicit action" precedent); the Rust side doesn't enforce that gate,
//! the frontend's only `add` call site does.
//!
//! ## Design notes
//!
//! - In-memory `Mutex<RecentPathsStore>` loaded lazily from disk via `OnceLock`.
//! - Atomic JSON write via the same temp-then-rename helper used elsewhere
//!   (`crate::config::durable_write_json`).
//! - Dedupe key is the raw resolved-path string. **Case-sensitivity is a v1
//!   limitation**: on case-insensitive APFS `/Users/x/Foo` and `/Users/x/foo`
//!   are the same dir but show as two entries. Accepted (worst case: a
//!   duplicate-looking row). We don't `canonicalize()` to fix it (symlink /
//!   nearest-ancestor reasons live in the parent module doc).
//! - Schema-versioned: a mismatch (or any parse error) renames the broken file
//!   aside and starts fresh, so a stray edit can't break the dialog forever.
//! - The disk file is never locked across a `.await`; the in-memory mutex guard
//!   is always dropped before any `fs` call.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bump when the on-disk shape changes in an incompatible way.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Fixed cap on recent paths. Not a setting: the dialog shows at most 10 recents
/// (digit keys 1-9, 0), so the store mirrors that hard limit.
const MAX_RECENTS: usize = 10;

/// Filename inside `{app_data_dir}/`.
const HISTORY_FILENAME: &str = "go-to-path-history.json";

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

/// On-disk shape. `_schemaVersion` lets future versions detect incompatible files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RecentPathsStore {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: Vec<RecentPathEntry>,
}

impl Default for RecentPathsStore {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

/// In-memory cache; loaded lazily from disk on first access.
static HISTORY: std::sync::OnceLock<Mutex<RecentPathsStore>> = std::sync::OnceLock::new();

/// Protects the disk read-modify-write cycle so concurrent commands can't clobber
/// each other's writes. The in-memory cache itself is already serialized by its
/// own mutex; this second mutex serializes the cache → disk flush.
static DISK_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

fn cache() -> &'static Mutex<RecentPathsStore> {
    HISTORY.get_or_init(|| Mutex::new(RecentPathsStore::default()))
}

fn disk_lock() -> &'static Mutex<()> {
    DISK_LOCK.get_or_init(|| Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Atomic file I/O helpers (mirrors `search/history.rs`).
// ---------------------------------------------------------------------------

/// Durably writes content to a file: write-to-temp + fsync + rename + parent-dir
/// fsync, so the write survives a power loss, not just process death.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    crate::config::durable_write_json(path, &tmp, content)
}

fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Renames the file to a `.broken` sibling so we keep one corrupted snapshot for
/// debugging without leaving the user blocked. If the rename itself fails, drop
/// the file outright; a deleted history beats a broken one.
fn quarantine_broken(path: &Path) {
    let broken = path.with_extension("json.broken");
    if broken.exists() {
        let _ = fs::remove_file(&broken);
    }
    if let Err(e) = fs::rename(path, &broken) {
        log::warn!(
            target: "go_to_path::history",
            "Couldn't quarantine corrupted go-to-path history at {:?} (will delete instead): {e}",
            path
        );
        let _ = fs::remove_file(path);
    } else {
        log::warn!(
            target: "go_to_path::history",
            "Quarantined corrupted go-to-path history to {:?}",
            broken
        );
    }
}

fn read_store_from_path(path: &Path) -> RecentPathsStore {
    cleanup_tmp_file(path);

    let Ok(contents) = fs::read_to_string(path) else {
        return RecentPathsStore::default();
    };

    match serde_json::from_str::<RecentPathsStore>(&contents) {
        Ok(store) => {
            if store.schema_version != CURRENT_SCHEMA_VERSION {
                log::warn!(
                    target: "go_to_path::history",
                    "Go-to-path history schema mismatch (file: {}, expected: {}); quarantining and starting fresh",
                    store.schema_version, CURRENT_SCHEMA_VERSION
                );
                quarantine_broken(path);
                return RecentPathsStore::default();
            }
            store
        }
        Err(e) => {
            log::warn!(
                target: "go_to_path::history",
                "Couldn't parse go-to-path history at {:?}: {e}",
                path
            );
            quarantine_broken(path);
            RecentPathsStore::default()
        }
    }
}

fn write_store_to_path(path: &Path, store: &RecentPathsStore) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(store).map_err(std::io::Error::other)?;
    atomic_write_json(path, &json)
}

/// Returns the on-disk location, or `None` when the app data dir can't be resolved.
fn get_store_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join(HISTORY_FILENAME))
}

// ---------------------------------------------------------------------------
// Core operations (pure so they can be unit-tested without an AppHandle).
// ---------------------------------------------------------------------------

/// Applies a fresh `add` to a store: dedupe by resolved path (move-to-top),
/// newest first, capped at [`MAX_RECENTS`]. Pure so tests can exercise dedupe +
/// cap without touching disk.
fn add_to_store(store: &mut RecentPathsStore, mut entry: RecentPathEntry) {
    // Drop any existing copy of the same resolved path; the new one wins.
    store.entries.retain(|e| e.path != entry.path);

    // Keep ids unique even if the caller passed a duplicate id.
    if store.entries.iter().any(|e| e.id == entry.id) {
        entry.id = uuid::Uuid::new_v4().to_string();
    }

    store.entries.insert(0, entry);

    if store.entries.len() > MAX_RECENTS {
        store.entries.truncate(MAX_RECENTS);
    }
}

fn remove_from_store(store: &mut RecentPathsStore, id: &str) -> bool {
    let before = store.entries.len();
    store.entries.retain(|e| e.id != id);
    store.entries.len() != before
}

// ---------------------------------------------------------------------------
// AppHandle-bound public API.
// ---------------------------------------------------------------------------

/// Loads the persisted store into the in-memory cache. Call once at startup.
pub fn load_history<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    let store = read_store_from_path(&path);
    drop(_disk_guard);

    if let Ok(mut cache_guard) = cache().lock() {
        *cache_guard = store;
    }
}

/// Returns a snapshot of the cached entries (newest first).
pub fn list_entries() -> Vec<RecentPathEntry> {
    match cache().lock() {
        Ok(guard) => guard.entries.clone(),
        Err(e) => e.into_inner().entries.clone(),
    }
}

/// Adds an entry, deduping by resolved path and trimming to the fixed cap.
///
/// The disk write is best-effort: failures are logged, but the in-memory state
/// always stays consistent.
pub fn add_entry<R: tauri::Runtime>(app: &tauri::AppHandle<R>, entry: RecentPathEntry) {
    let snapshot = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        add_to_store(&mut guard, entry);
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        guard.clone()
    };

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "go_to_path::history", "Couldn't write go-to-path history: {e}");
    }
}

/// Removes an entry by id. No-op if the id isn't present.
pub fn remove_entry<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    let snapshot = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !remove_from_store(&mut guard, id) {
            return; // Not present: skip the disk write entirely.
        }
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        guard.clone()
    };

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "go_to_path::history", "Couldn't write go-to-path history: {e}");
    }
}

/// Clears every entry. The on-disk file is rewritten with an empty list (rather
/// than deleted) so a future write can't race a missing-file load.
pub fn clear_entries<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let snapshot = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.entries.clear();
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        guard.clone()
    };

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "go_to_path::history", "Couldn't write go-to-path history: {e}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    // -- add_to_store: dedupe + move-to-top + cap --

    #[test]
    fn add_dedupes_by_path_and_moves_to_top() {
        let mut store = RecentPathsStore::default();
        add_to_store(&mut store, entry("/Users/x/a"));
        add_to_store(&mut store, entry("/Users/x/b"));

        // Re-add `/Users/x/a` (same path, different id/timestamp).
        let mut dup = entry("/Users/x/a");
        dup.timestamp = 100;
        let dup_id = dup.id.clone();
        add_to_store(&mut store, dup);

        assert_eq!(store.entries.len(), 2, "duplicate path should have collapsed");
        assert_eq!(store.entries[0].id, dup_id, "newest copy should win");
        assert_eq!(store.entries[0].path, "/Users/x/a");
        assert_eq!(store.entries[1].path, "/Users/x/b");
    }

    #[test]
    fn add_enforces_cap_of_ten_evicting_oldest() {
        let mut store = RecentPathsStore::default();
        for i in 0..15 {
            add_to_store(&mut store, entry(&format!("/p/{i}")));
        }
        assert_eq!(store.entries.len(), MAX_RECENTS);
        // Newest first: the last 10 added survive.
        assert_eq!(store.entries[0].path, "/p/14");
        assert_eq!(store.entries[MAX_RECENTS - 1].path, "/p/5");
    }

    #[test]
    fn add_assigns_fresh_id_if_caller_collides() {
        let mut store = RecentPathsStore::default();
        let mut first = entry("/p/a");
        first.id = "fixed-id".to_string();
        add_to_store(&mut store, first);

        let mut second = entry("/p/b");
        second.id = "fixed-id".to_string();
        add_to_store(&mut store, second);

        assert_eq!(store.entries.len(), 2);
        assert_ne!(store.entries[0].id, store.entries[1].id);
    }

    // -- remove --

    #[test]
    fn remove_returns_true_when_found() {
        let mut store = RecentPathsStore::default();
        let mut e = entry("/p/a");
        e.id = "abc".to_string();
        add_to_store(&mut store, e);

        assert!(remove_from_store(&mut store, "abc"));
        assert!(store.entries.is_empty());
    }

    #[test]
    fn remove_returns_false_when_not_found() {
        let mut store = RecentPathsStore::default();
        assert!(!remove_from_store(&mut store, "nope"));
    }

    // -- Serialization round-trip --

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

    #[test]
    fn store_serialization_carries_schema_version() {
        let store = RecentPathsStore::default();
        let json = serde_json::to_string_pretty(&store).unwrap();
        assert!(json.contains("\"_schemaVersion\": 1"));

        let back: RecentPathsStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, 1);
        assert!(back.entries.is_empty());
    }

    // -- File round-trip and recovery --

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);

        let mut store = RecentPathsStore::default();
        add_to_store(&mut store, entry("/p/a"));
        add_to_store(&mut store, entry("/p/b"));
        write_store_to_path(&path, &store).expect("write");

        let loaded = read_store_from_path(&path);
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].path, "/p/b");
        assert_eq!(loaded.entries[1].path, "/p/a");
    }

    #[test]
    fn corrupted_json_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);
        fs::write(&path, "{not valid json at all").expect("write garbage");

        let store = read_store_from_path(&path);
        assert!(store.entries.is_empty());
        assert_eq!(store.schema_version, 1);

        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "expected quarantine at {broken:?}");
        assert!(!path.exists(), "original file should be gone");
    }

    #[test]
    fn schema_version_mismatch_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);
        let v2 = r#"{"_schemaVersion": 2, "entries": []}"#;
        fs::write(&path, v2).expect("write");

        let store = read_store_from_path(&path);
        assert_eq!(store.schema_version, 1);
        assert!(store.entries.is_empty());

        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "version mismatch should quarantine");
    }

    #[test]
    fn missing_file_yields_default_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);
        let store = read_store_from_path(&path);
        assert!(store.entries.is_empty());
        assert_eq!(store.schema_version, 1);
    }

    #[test]
    fn stale_tmp_file_is_cleaned_up_on_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "stale").expect("write tmp");
        assert!(tmp.exists());

        let _ = read_store_from_path(&path);
        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
