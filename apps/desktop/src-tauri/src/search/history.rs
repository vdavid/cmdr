//! Persistent recent-searches store for the search dialog.
//!
//! Adds an entry only when the user clicks "Open in pane" — see the FE
//! `lib/search/CLAUDE.md` for the call-site rule. This module owns the storage
//! layer plus the IPC commands the frontend consumes.
//!
//! ## Design notes
//!
//! - In-memory `Mutex<HistoryStore>`, mirroring `network/known_shares.rs`.
//! - Atomic JSON write via the same temp-then-rename helper used elsewhere in the app.
//! - Canonical dedupe key collapses entries that ask the same question: same mode,
//!   normalized query, identical filter set, identical bool flags. The key is built
//!   purely at runtime; it never appears in the persisted JSON.
//! - Schema-versioned: a mismatch (or any parse error) renames the broken file aside
//!   and starts fresh, so a stray edit can't break the dialog forever.
//! - The disk file is never locked for the duration of a `.await`; the in-memory
//!   `Mutex<HistoryStore>` is a `std::sync::Mutex` and the guard is always dropped
//!   before any `fs` call.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Bump when the on-disk shape changes in an incompatible way.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Default cap when the user hasn't tuned `search.recentSearches.maxCount`.
const DEFAULT_MAX_COUNT: usize = 1000;

/// Filename inside `{app_data_dir}/`.
const HISTORY_FILENAME: &str = "search-history.json";

/// Search modes recorded in history. Mirrors the frontend `SearchMode` union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum HistoryMode {
    Ai,
    Filename,
    Regex,
}

impl HistoryMode {
    fn as_str(self) -> &'static str {
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

/// On-disk shape. `_schemaVersion` lets future versions detect incompatible files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HistoryStore {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: Vec<HistoryEntry>,
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

/// Builds the canonical dedupe key for an entry. Two entries with the same key are
/// considered the "same search"; the most recent one wins and the older copy is
/// dropped. The key is never persisted; it only exists at compare time.
fn canonical_key(entry: &HistoryEntry) -> String {
    // 1. Mode goes in lowercase. Already lowercase via the enum's `as_str`, but be
    //    explicit so the contract is visible.
    let mode = entry.mode.as_str().to_lowercase();

    // 2. Query: trim, collapse internal whitespace runs to single spaces, lowercase.
    let normalized_query: String = entry
        .query
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    // 3. Filters: keys sorted alphabetically, undefined fields skipped entirely.
    //    `BTreeMap<&str, String>` gives us the sort and keeps the key set explicit.
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

    // 4. Scope: trim + lowercase so "/Users" and " /users " collapse.
    let scope = entry.scope.trim().to_lowercase();

    // 5. Bool flags: single-char t/f.
    let cs = if entry.case_sensitive { "t" } else { "f" };
    let es = if entry.exclude_system_dirs { "t" } else { "f" };

    format!("{mode}|{normalized_query}|{filter_str}|{scope}|{cs}|{es}")
}

// ---------------------------------------------------------------------------
// Atomic file I/O helpers (mirrors `network/known_shares.rs` and `manual_servers.rs`).
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
            target: "search::history",
            "Couldn't quarantine corrupted search history at {:?} (will delete instead): {e}",
            path
        );
        let _ = fs::remove_file(path);
    } else {
        log::warn!(
            target: "search::history",
            "Quarantined corrupted search history to {:?}",
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
                    target: "search::history",
                    "Search history schema mismatch (file: {}, expected: {}); quarantining and starting fresh",
                    store.schema_version, CURRENT_SCHEMA_VERSION
                );
                quarantine_broken(path);
                return HistoryStore::default();
            }
            store
        }
        Err(e) => {
            log::warn!(
                target: "search::history",
                "Couldn't parse search history at {:?}: {e}",
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
fn add_to_store(store: &mut HistoryStore, mut entry: HistoryEntry, max_count: usize) {
    // Max-count of zero clears history wholesale and ignores the new entry.
    if max_count == 0 {
        store.entries.clear();
        return;
    }

    let key = canonical_key(&entry);
    // Drop any existing copies of the same canonical key (could be more than one if a
    // previous version of the code allowed duplicates; this is also a self-heal path).
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
pub fn list_entries(limit: Option<usize>) -> Vec<HistoryEntry> {
    let snapshot: Vec<HistoryEntry> = match cache().lock() {
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
/// Returns the resulting in-memory entries (newest first). The disk write is best-effort:
/// failures are logged, but the in-memory state always stays consistent.
pub fn add_entry<R: tauri::Runtime>(app: &tauri::AppHandle<R>, entry: HistoryEntry, max_count: usize) {
    // 1. Update the in-memory cache and clone the result for the disk write. We never
    //    hold the cache mutex across the `fs::write` call.
    let snapshot = {
        let mut guard = match cache().lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        add_to_store(&mut guard, entry, max_count);
        // Always normalize schema_version on write so older files heal forward.
        guard.schema_version = CURRENT_SCHEMA_VERSION;
        guard.clone()
    };

    let Some(path) = get_store_path(app) else {
        return;
    };

    let _disk_guard = disk_lock().lock().unwrap_or_else(|e| e.into_inner());
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "search::history", "Couldn't write search history: {e}");
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
        log::warn!(target: "search::history", "Couldn't write search history: {e}");
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
        log::warn!(target: "search::history", "Couldn't write search history: {e}");
    }
}

/// Applies a freshly-tuned cap to the in-memory store. Used by the settings live-apply
/// flow when the user changes `search.recentSearches.maxCount`. The disk file is only
/// rewritten when the cap actually drops entries.
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
        log::warn!(target: "search::history", "Couldn't write search history after cap change: {e}");
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
    fn canonical_key_distinguishes_scope_and_flags() {
        let mut a = entry(HistoryMode::Filename, "*.pdf");
        let mut b = entry(HistoryMode::Filename, "*.pdf");
        b.scope = "/Users".to_string();
        assert_ne!(canonical_key(&a), canonical_key(&b));

        a.case_sensitive = true;
        let c = entry(HistoryMode::Filename, "*.pdf");
        assert_ne!(canonical_key(&a), canonical_key(&c));
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

        // Both entries are present, with unique ids.
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

        // The garbage file should have been moved to .broken so we can debug later.
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
        // File doesn't exist on purpose.
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

        // Reading should clean it up even when the real file doesn't exist.
        let _ = read_store_from_path(&path);
        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
