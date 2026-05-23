//! Persistent recent-selections store for the Selection dialog.
//!
//! Mirrors `crate::search::history` line-for-line with one key difference: the entry
//! schema is narrower. Selection runs in the focused folder only, so there's no
//! `scope`, no `exclude_system_dirs`. Everything else (atomic write, schema version,
//! in-memory cache + disk lock, canonical-key dedupe, cap eviction, schema-version
//! quarantine) is identical.
//!
//! We reuse `HistoryMode` and `HistoryFilters` from `crate::search::history` so the
//! frontend can render mode badges and filter chips the same way for both consumers.
//! The entry struct is separate so the on-disk schema doesn't bind Selection to
//! Search's canonical-key shape.
//!
//! File: `{app_data_dir}/selection-history.json`. Schema-versioned via `_schemaVersion`
//! (currently 1). On parse failure or schema-version mismatch the file is renamed to
//! `.broken` and we start fresh; the user keeps using the dialog, the corrupted file
//! is preserved for one rotation in case we want to debug.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

// Re-export the shared types so the frontend bindings see the same wire shape for both
// consumers. Keeping these in `search::history` (rather than splitting into a third
// "history-shared" module) avoids churn until a future consumer actually needs them.
pub use crate::search::history::{HistoryFilters, HistoryMode};

/// Bump when the on-disk shape changes in an incompatible way.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Default cap when the user hasn't tuned `selection.recentSelections.maxCount`.
const DEFAULT_MAX_COUNT: usize = 1000;

/// Filename inside `{app_data_dir}/`.
const HISTORY_FILENAME: &str = "selection-history.json";

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

/// On-disk shape. `_schemaVersion` lets future versions detect incompatible files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryStore {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: Vec<SelectionHistoryEntry>,
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }
}

/// In-memory cache; loaded lazily from disk on first access.
static HISTORY: std::sync::OnceLock<Mutex<HistoryStore>> = std::sync::OnceLock::new();

/// Protects the disk read-modify-write cycle so concurrent commands can't clobber each
/// other's writes. The in-memory cache itself is already serialized by its own mutex;
/// this second mutex serializes the cache → disk flush.
static DISK_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

fn cache() -> &'static Mutex<HistoryStore> {
    HISTORY.get_or_init(|| Mutex::new(HistoryStore::default()))
}

fn disk_lock() -> &'static Mutex<()> {
    DISK_LOCK.get_or_init(|| Mutex::new(()))
}

fn mode_as_str(mode: HistoryMode) -> &'static str {
    match mode {
        HistoryMode::Ai => "ai",
        HistoryMode::Filename => "filename",
        HistoryMode::Regex => "regex",
    }
}

/// Builds the canonical dedupe key for an entry. Narrower than Search's: there's no
/// `scope` and no `exclude_system_dirs` because Selection always runs in the current
/// folder. The key is never persisted; it only exists at compare time.
fn canonical_key(entry: &SelectionHistoryEntry) -> String {
    // 1. Mode lowercase.
    let mode = mode_as_str(entry.mode).to_lowercase();

    // 2. Query: trim, collapse internal whitespace runs to single spaces, lowercase.
    let normalized_query: String = entry
        .query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    // 3. Filters: keys sorted alphabetically, undefined fields skipped entirely.
    let mut filter_kv: BTreeMap<&str, String> = BTreeMap::new();
    if let Some(v) = entry.filters.size_min {
        filter_kv.insert("sizeMin", v.to_string());
    }
    if let Some(v) = entry.filters.size_max {
        filter_kv.insert("sizeMax", v.to_string());
    }
    if let Some(ref v) = entry.filters.modified_after {
        filter_kv.insert("modifiedAfter", v.clone());
    }
    if let Some(ref v) = entry.filters.modified_before {
        filter_kv.insert("modifiedBefore", v.clone());
    }
    let filter_str = filter_kv
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",");

    // 4. Case-sensitive flag: single-char t/f.
    let cs = if entry.case_sensitive { "t" } else { "f" };

    format!("{mode}|{normalized_query}|{filter_str}|{cs}")
}

// ---------------------------------------------------------------------------
// Atomic file I/O helpers (mirrors `crate::search::history`).
// ---------------------------------------------------------------------------

fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Renames the file to a `.broken` sibling so we can keep one corrupted snapshot for
/// debugging without leaving the user blocked. If the rename itself fails, drop the
/// file outright; a deleted history beats a broken one.
fn quarantine_broken(path: &Path) {
    let broken = path.with_extension("json.broken");
    if broken.exists() {
        // Overwrite any previous quarantine; we only care about the most recent corruption.
        let _ = fs::remove_file(&broken);
    }
    if let Err(e) = fs::rename(path, &broken) {
        log::warn!(
            target: "selection::history",
            "Couldn't quarantine corrupted selection history at {:?} (will delete instead): {e}",
            path
        );
        let _ = fs::remove_file(path);
    } else {
        log::warn!(
            target: "selection::history",
            "Quarantined corrupted selection history to {:?}",
            broken
        );
    }
}

fn read_store_from_path(path: &Path) -> HistoryStore {
    cleanup_tmp_file(path);

    let Ok(contents) = fs::read_to_string(path) else {
        return HistoryStore::default();
    };

    match serde_json::from_str::<HistoryStore>(&contents) {
        Ok(store) => {
            if store.schema_version != CURRENT_SCHEMA_VERSION {
                log::warn!(
                    target: "selection::history",
                    "Selection history schema mismatch (file: {}, expected: {}); quarantining and starting fresh",
                    store.schema_version, CURRENT_SCHEMA_VERSION
                );
                quarantine_broken(path);
                return HistoryStore::default();
            }
            store
        }
        Err(e) => {
            log::warn!(
                target: "selection::history",
                "Couldn't parse selection history at {:?}: {e}",
                path
            );
            quarantine_broken(path);
            HistoryStore::default()
        }
    }
}

fn write_store_to_path(path: &Path, store: &HistoryStore) -> std::io::Result<()> {
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
// Core operations (path-based so they can be unit-tested without an AppHandle).
// ---------------------------------------------------------------------------

/// Apply a fresh `add` to a store. Pure function so tests can exercise dedupe + cap
/// without touching disk.
fn add_to_store(store: &mut HistoryStore, mut entry: SelectionHistoryEntry, max_count: usize) {
    // Max-count of zero clears history wholesale and ignores the new entry.
    if max_count == 0 {
        store.entries.clear();
        return;
    }

    let key = canonical_key(&entry);
    // Drop any existing copies of the same canonical key (self-heal if a previous
    // version of the code allowed duplicates).
    store.entries.retain(|e| canonical_key(e) != key);

    // Ensure the new entry's id stays unique even if the caller passed a duplicate id.
    if store.entries.iter().any(|e| e.id == entry.id) {
        entry.id = uuid::Uuid::new_v4().to_string();
    }

    // Newest first.
    store.entries.insert(0, entry);

    // Enforce cap from the tail (oldest).
    if store.entries.len() > max_count {
        store.entries.truncate(max_count);
    }
}

fn trim_to_cap(store: &mut HistoryStore, max_count: usize) {
    if max_count == 0 {
        store.entries.clear();
        return;
    }
    if store.entries.len() > max_count {
        store.entries.truncate(max_count);
    }
}

fn remove_from_store(store: &mut HistoryStore, id: &str) -> bool {
    let before = store.entries.len();
    store.entries.retain(|e| e.id != id);
    store.entries.len() != before
}

// ---------------------------------------------------------------------------
// AppHandle-bound public API.
// ---------------------------------------------------------------------------

/// Loads the persisted store into the in-memory cache. Call once at startup; safe to
/// call again as a refresh if the file is suspected stale.
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

/// Returns a snapshot of the cached entries (newest first). `limit = None` returns all.
pub fn list_entries(limit: Option<usize>) -> Vec<SelectionHistoryEntry> {
    let snapshot: Vec<SelectionHistoryEntry> = match cache().lock() {
        Ok(guard) => guard.entries.clone(),
        Err(e) => e.into_inner().entries.clone(),
    };
    match limit {
        Some(n) => snapshot.into_iter().take(n).collect(),
        None => snapshot,
    }
}

/// Adds an entry, deduping by canonical key and trimming to `max_count`.
///
/// The disk write is best-effort: failures are logged, but the in-memory state always
/// stays consistent.
pub fn add_entry<R: tauri::Runtime>(app: &tauri::AppHandle<R>, entry: SelectionHistoryEntry, max_count: usize) {
    let snapshot = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        add_to_store(&mut guard, entry, max_count);
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        guard.clone()
    };

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "selection::history", "Couldn't write selection history: {e}");
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
        log::warn!(target: "selection::history", "Couldn't write selection history: {e}");
    }
}

/// Clears every entry. The on-disk file is rewritten with an empty list (rather than
/// deleted) so a future write can't race a missing-file load.
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
        log::warn!(target: "selection::history", "Couldn't write selection history: {e}");
    }
}

/// Applies a freshly-tuned cap to the in-memory store. Used by the settings live-apply
/// flow when the user changes `selection.recentSelections.maxCount`. The disk file is
/// only rewritten when the cap actually drops entries.
pub fn apply_max_count<R: tauri::Runtime>(app: &tauri::AppHandle<R>, max_count: usize) {
    let (changed, snapshot) = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = guard.entries.len();
        trim_to_cap(&mut guard, max_count);
        let after = guard.entries.len();
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        (before != after, guard.clone())
    };

    if !changed {
        return;
    }

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(
            target: "selection::history",
            "Couldn't write selection history after cap change: {e}"
        );
    }
}

/// Default cap exposed for callers (the IPC layer) that don't have a live setting yet.
pub fn default_max_count() -> usize {
    DEFAULT_MAX_COUNT
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

    // -- canonical_key --

    #[test]
    fn canonical_key_collapses_whitespace_and_case() {
        let a = entry(HistoryMode::Filename, "  Foo   Bar  ");
        let b = entry(HistoryMode::Filename, "foo bar");
        assert_eq!(canonical_key(&a), canonical_key(&b));
    }

    #[test]
    fn canonical_key_distinguishes_modes() {
        let f = entry(HistoryMode::Filename, "*.pdf");
        let r = entry(HistoryMode::Regex, "*.pdf");
        assert_ne!(canonical_key(&f), canonical_key(&r));
    }

    #[test]
    fn canonical_key_distinguishes_case_sensitive_flag() {
        let mut a = entry(HistoryMode::Filename, "*.pdf");
        let b = entry(HistoryMode::Filename, "*.pdf");
        a.case_sensitive = true;
        assert_ne!(canonical_key(&a), canonical_key(&b));
    }

    #[test]
    fn canonical_key_orders_filters_alphabetically() {
        let mut a = entry(HistoryMode::Filename, "*.pdf");
        a.filters.size_min = Some(1024);
        a.filters.modified_after = Some("2026-01-01".to_string());

        let mut b = entry(HistoryMode::Filename, "*.pdf");
        // Assigned in different order; struct field order doesn't matter for the key.
        b.filters.modified_after = Some("2026-01-01".to_string());
        b.filters.size_min = Some(1024);

        assert_eq!(canonical_key(&a), canonical_key(&b));
        // Spot-check the key has filters in alphabetical order:
        assert!(canonical_key(&a).contains("modifiedAfter=2026-01-01,sizeMin=1024"));
    }

    #[test]
    fn canonical_key_has_no_scope_or_exclude_system_dirs() {
        // Search's key has 6 fields separated by '|'; Selection's has 4
        // (mode | normalized_query | filters | case_sensitive). The narrower shape is
        // load-bearing: it prevents accidentally re-introducing scope-style fields.
        let e = entry(HistoryMode::Filename, "*.pdf");
        let key = canonical_key(&e);
        assert_eq!(
            key.split('|').count(),
            4,
            "selection key should have exactly 4 segments"
        );
    }

    // -- add_to_store: dedupe + move-to-top + cap --

    #[test]
    fn add_dedupes_by_canonical_key_and_moves_to_top() {
        let mut store = HistoryStore::default();
        let mut first = entry(HistoryMode::Filename, "*.pdf");
        first.timestamp = 1;
        add_to_store(&mut store, first.clone(), 10);

        let other = entry(HistoryMode::Filename, "*.dmg");
        add_to_store(&mut store, other, 10);

        // Add a near-duplicate of `first` (same canonical key, different id/timestamp).
        let mut dup = entry(HistoryMode::Filename, "  *.PDF  ");
        dup.timestamp = 100;
        let dup_id = dup.id.clone();
        add_to_store(&mut store, dup, 10);

        assert_eq!(store.entries.len(), 2, "duplicate should have collapsed");
        assert_eq!(store.entries[0].id, dup_id, "newest copy should win");
        assert_eq!(store.entries[0].timestamp, 100);
    }

    #[test]
    fn add_enforces_cap_evicting_oldest() {
        let mut store = HistoryStore::default();
        for i in 0..5 {
            let mut e = entry(HistoryMode::Filename, &format!("query-{i}"));
            e.timestamp = i64::from(i);
            add_to_store(&mut store, e, 3);
        }
        assert_eq!(store.entries.len(), 3);
        // Newest first.
        assert_eq!(store.entries[0].query, "query-4");
        assert_eq!(store.entries[1].query, "query-3");
        assert_eq!(store.entries[2].query, "query-2");
    }

    #[test]
    fn add_with_zero_cap_clears_history() {
        let mut store = HistoryStore::default();
        add_to_store(&mut store, entry(HistoryMode::Filename, "*.pdf"), 10);
        assert_eq!(store.entries.len(), 1);

        add_to_store(&mut store, entry(HistoryMode::Filename, "*.dmg"), 0);
        assert!(store.entries.is_empty(), "zero cap should clear history");
    }

    #[test]
    fn add_assigns_fresh_id_if_caller_collides() {
        let mut store = HistoryStore::default();
        let mut first = entry(HistoryMode::Filename, "*.pdf");
        first.id = "fixed-id".to_string();
        add_to_store(&mut store, first, 10);

        let mut second = entry(HistoryMode::Filename, "*.dmg");
        second.id = "fixed-id".to_string();
        add_to_store(&mut store, second, 10);

        assert_eq!(store.entries.len(), 2);
        assert_ne!(store.entries[0].id, store.entries[1].id);
    }

    // -- trim_to_cap --

    #[test]
    fn trim_to_cap_drops_oldest() {
        let mut store = HistoryStore::default();
        for i in 0..5 {
            let mut e = entry(HistoryMode::Filename, &format!("query-{i}"));
            e.timestamp = i64::from(i);
            add_to_store(&mut store, e, 10);
        }
        assert_eq!(store.entries.len(), 5);

        trim_to_cap(&mut store, 2);
        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.entries[0].query, "query-4");
        assert_eq!(store.entries[1].query, "query-3");
    }

    #[test]
    fn trim_to_cap_zero_clears() {
        let mut store = HistoryStore::default();
        add_to_store(&mut store, entry(HistoryMode::Filename, "*.pdf"), 10);
        trim_to_cap(&mut store, 0);
        assert!(store.entries.is_empty());
    }

    // -- remove --

    #[test]
    fn remove_returns_true_when_found() {
        let mut store = HistoryStore::default();
        let mut e = entry(HistoryMode::Filename, "*.pdf");
        e.id = "abc".to_string();
        add_to_store(&mut store, e, 10);

        assert!(remove_from_store(&mut store, "abc"));
        assert!(store.entries.is_empty());
    }

    #[test]
    fn remove_returns_false_when_not_found() {
        let mut store = HistoryStore::default();
        assert!(!remove_from_store(&mut store, "nope"));
    }

    // -- Serialization round-trip --

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
    fn store_serialization_carries_schema_version() {
        let store = HistoryStore::default();
        let json = serde_json::to_string_pretty(&store).unwrap();
        assert!(json.contains("\"_schemaVersion\": 1"));

        let back: HistoryStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, 1);
        assert!(back.entries.is_empty());
    }

    // -- File round-trip and recovery --

    #[test]
    fn save_then_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(HISTORY_FILENAME);

        let mut store = HistoryStore::default();
        add_to_store(&mut store, entry(HistoryMode::Filename, "*.pdf"), 10);
        add_to_store(&mut store, entry(HistoryMode::Ai, "screenshots"), 10);
        write_store_to_path(&path, &store).expect("write");

        let loaded = read_store_from_path(&path);
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.entries.len(), 2);
        assert_eq!(loaded.entries[0].query, "screenshots");
        assert_eq!(loaded.entries[1].query, "*.pdf");
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
        // Hand-write a v2 file that we don't know how to parse.
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

    #[test]
    fn default_max_count_is_a_thousand() {
        assert_eq!(default_max_count(), 1000);
    }
}
