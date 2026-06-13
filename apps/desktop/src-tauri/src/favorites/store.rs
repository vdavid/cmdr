//! Persistent, user-editable favorites store (`favorites.json`).
//!
//! An ordered list of `{ id, path, name }` favorites that the volume switcher's "Favorites" section
//! renders. The store is the single source of truth: it replaces the previously hardcoded four
//! favorites (`/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`).
//!
//! ## Seed-once via file presence
//!
//! On first launch (file absent) we write the four defaults, computed from `dirs::home_dir()`. Every
//! launch after that reads the file verbatim and NEVER re-injects defaults. An emptied list stays
//! empty. Existing beta users (data dir present, no `favorites.json` yet) get the four seeded on the
//! first launch after the update, with no regression.
//!
//! ## Design notes (mirrors `go_to_path/history.rs` and `install_id.rs`)
//!
//! - In-memory `Mutex<FavoritesStore>` loaded lazily from disk via `OnceLock`.
//! - The data dir is resolved WITHOUT an `AppHandle` (mirroring `install_id.rs`): `CMDR_DATA_DIR` if
//!   set, else the OS default for the bundle id. This is load-bearing: `get_favorites()` (the read
//!   path, in `volumes/mod.rs`) is sync and has no `AppHandle`, so the accessors must stay no-arg.
//! - Atomic JSON write via the shared temp-then-rename helper (`crate::config::durable_write_json`).
//! - `id` is a stable random UUID minted on add, NEVER derived from the path: paths can repeat across
//!   a rename, and a user can re-add a path they removed, so the id must outlive the path string.
//! - `add` dedups by normalized path: re-adding an existing path moves it to the end (keeps its id),
//!   so the user's existing label and position context isn't silently dropped.
//! - Schema-versioned: a parse error or version mismatch quarantines the file aside and starts
//!   fresh, so a stray hand-edit can't break the switcher forever.
//! - The disk file is never locked across an `.await`; the in-memory mutex guard is always dropped
//!   before any `fs` call.

use crate::config;
use crate::ignore_poison::IgnorePoison;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

/// Bundle id from `tauri.conf.json`. Mirrored here so the data-dir resolution works without an
/// `AppHandle`, matching `install_id.rs`. Keep in sync if it ever changes.
const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

/// Filename inside `{app_data_dir}/`.
const FAVORITES_FILE_NAME: &str = "favorites.json";

/// Bump when the on-disk shape changes in an incompatible way.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A single favorite, persisted verbatim and serialized to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Favorite {
    /// Stable random id minted on add. Never derived from `path`.
    pub id: String,
    /// Absolute filesystem path the favorite points at.
    pub path: String,
    /// Display label. Defaults to the path's file name on add; the user can override via rename.
    pub name: String,
}

/// On-disk shape. `_schemaVersion` lets future versions detect incompatible files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FavoritesStore {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    favorites: Vec<Favorite>,
}

impl Default for FavoritesStore {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            favorites: Vec::new(),
        }
    }
}

/// Tri-state cache: `None` until the first access loads (and lazily seeds) from disk.
static CACHE: OnceLock<Mutex<Option<FavoritesStore>>> = OnceLock::new();

/// Serializes the disk read-modify-write cycle so concurrent commands can't clobber each other.
static DISK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<FavoritesStore>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

fn disk_lock() -> &'static Mutex<()> {
    DISK_LOCK.get_or_init(|| Mutex::new(()))
}

// ---------------------------------------------------------------------------
// Default seed
// ---------------------------------------------------------------------------

/// The favorites seeded on first launch (file absent). Computed from `dirs::home_dir()`.
///
/// Platform-native (per `design-principles.md`): macOS seeds `/Applications`, `~/Desktop`,
/// `~/Documents`, `~/Downloads` (the previous hardcoded four); Linux seeds Home, `~/Desktop`,
/// `~/Documents`, `~/Downloads` (matching the previous `volumes_linux` favorites).
fn default_favorites() -> Vec<Favorite> {
    let home = dirs::home_dir().unwrap_or_default();

    #[cfg(target_os = "macos")]
    let entries: Vec<(PathBuf, &str)> = vec![
        (PathBuf::from("/Applications"), "Applications"),
        (home.join("Desktop"), "Desktop"),
        (home.join("Documents"), "Documents"),
        (home.join("Downloads"), "Downloads"),
    ];

    #[cfg(not(target_os = "macos"))]
    let entries: Vec<(PathBuf, &str)> = vec![
        (home.clone(), "Home"),
        (home.join("Desktop"), "Desktop"),
        (home.join("Documents"), "Documents"),
        (home.join("Downloads"), "Downloads"),
    ];

    entries
        .into_iter()
        .map(|(path, name)| Favorite {
            id: new_id(),
            path: path.to_string_lossy().to_string(),
            name: name.to_string(),
        })
        .collect()
}

fn new_id() -> String {
    Uuid::new_v4().to_string()
}

/// Derives a display label from a path: the last component, falling back to the full path when there
/// isn't one (for example `/`).
fn name_from_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| path.to_string())
}

/// Normalizes a path for dedup comparison: strips a single trailing separator (but never the root
/// `/`). Case-sensitivity is a known limitation, same as `go_to_path/history.rs`: on
/// case-insensitive APFS `/Users/x/Foo` and `/Users/x/foo` compare unequal. Worst case is a
/// duplicate-looking row.
fn normalize_for_dedup(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Pure core (testable without disk or an AppHandle)
// ---------------------------------------------------------------------------

/// Adds a favorite, deduping by normalized path. If the path already exists, moves the existing entry
/// to the end and applies an explicit `name` override when given (keeping its id). Returns the id of
/// the affected entry.
fn add_to_store(store: &mut FavoritesStore, path: &str, name: Option<String>) -> String {
    let normalized = normalize_for_dedup(path);
    if let Some(pos) = store
        .favorites
        .iter()
        .position(|f| normalize_for_dedup(&f.path) == normalized)
    {
        let mut existing = store.favorites.remove(pos);
        if let Some(name) = name {
            existing.name = name;
        }
        let id = existing.id.clone();
        store.favorites.push(existing);
        return id;
    }

    let id = new_id();
    let label = name.unwrap_or_else(|| name_from_path(path));
    store.favorites.push(Favorite {
        id: id.clone(),
        path: path.to_string(),
        name: label,
    });
    id
}

/// Removes a favorite by id. Returns `true` if an entry was removed.
fn remove_from_store(store: &mut FavoritesStore, id: &str) -> bool {
    let before = store.favorites.len();
    store.favorites.retain(|f| f.id != id);
    store.favorites.len() != before
}

/// Renames a favorite by id. Returns `true` if the entry was found.
fn rename_in_store(store: &mut FavoritesStore, id: &str, name: &str) -> bool {
    if let Some(f) = store.favorites.iter_mut().find(|f| f.id == id) {
        f.name = name.to_string();
        true
    } else {
        false
    }
}

/// Reorders the favorites to match `ordered_ids`. Ids not present in the store are ignored; favorites
/// whose ids are missing from `ordered_ids` are appended in their current relative order, so a
/// partial/stale order from the frontend never drops an entry.
fn reorder_store(store: &mut FavoritesStore, ordered_ids: &[String]) {
    let mut remaining: Vec<Favorite> = std::mem::take(&mut store.favorites);
    let mut reordered: Vec<Favorite> = Vec::with_capacity(remaining.len());
    for id in ordered_ids {
        if let Some(pos) = remaining.iter().position(|f| &f.id == id) {
            reordered.push(remaining.remove(pos));
        }
    }
    // Append any favorites not named in `ordered_ids`, preserving their order.
    reordered.extend(remaining);
    store.favorites = reordered;
}

// ---------------------------------------------------------------------------
// Disk I/O (mirrors `go_to_path/history.rs`)
// ---------------------------------------------------------------------------

/// Resolves the favorites file path without an `AppHandle` (mirrors `install_id.rs`).
fn favorites_path() -> PathBuf {
    let data_dir: PathBuf = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        dirs::data_dir().map(|base| base.join(BUNDLE_ID)).unwrap_or_default()
    };
    data_dir.join(FAVORITES_FILE_NAME)
}

fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Renames a corrupted file to a `.broken` sibling so one bad snapshot survives for debugging without
/// leaving the user blocked. If the rename fails, drop the file outright.
fn quarantine_broken(path: &Path) {
    let broken = path.with_extension("json.broken");
    if broken.exists() {
        let _ = fs::remove_file(&broken);
    }
    if let Err(e) = fs::rename(path, &broken) {
        log::warn!(
            target: "favorites::store",
            "Couldn't quarantine corrupted favorites at {path:?} (will delete instead): {e}"
        );
        let _ = fs::remove_file(path);
    } else {
        log::warn!(target: "favorites::store", "Quarantined corrupted favorites to {broken:?}");
    }
}

/// Reads the store from disk. Returns `None` when the file is absent (the signal to seed). A parse
/// error or schema mismatch quarantines the file and returns a fresh default store (treated as
/// already-initialized: a corrupt file is not "first launch").
fn read_store_from_path(path: &Path) -> Option<FavoritesStore> {
    cleanup_tmp_file(path);

    let contents = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            // The file is PRESENT but unreadable right now (transient I/O error, permission blip).
            // Returning `None` here would signal "first launch" and re-seed the defaults OVER the
            // user's real list (data loss). Instead read as an empty store WITHOUT overwriting disk:
            // `load_or_seed` only writes when the read is `None`, so the unreadable file is left
            // intact for a later successful read. We don't quarantine, since we couldn't read it to
            // confirm it's actually corrupt.
            log::warn!(
                target: "favorites::store",
                "Couldn't read favorites at {path:?} ({e}); treating as present (no re-seed) to protect the user's list"
            );
            return Some(FavoritesStore::default());
        }
    };

    match serde_json::from_str::<FavoritesStore>(&contents) {
        Ok(store) if store.schema_version == CURRENT_SCHEMA_VERSION => Some(store),
        Ok(store) => {
            log::warn!(
                target: "favorites::store",
                "Favorites schema mismatch (file: {}, expected: {}); quarantining and starting fresh",
                store.schema_version, CURRENT_SCHEMA_VERSION
            );
            quarantine_broken(path);
            Some(FavoritesStore::default())
        }
        Err(e) => {
            log::warn!(target: "favorites::store", "Couldn't parse favorites at {path:?}: {e}");
            quarantine_broken(path);
            Some(FavoritesStore::default())
        }
    }
}

fn write_store_to_path(path: &Path, store: &FavoritesStore) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(store).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    config::durable_write_json(path, &tmp, &json)
}

// ---------------------------------------------------------------------------
// Cache + seed-once
// ---------------------------------------------------------------------------

/// Loads the store into the cache if not loaded yet, seeding the four defaults to disk when the file
/// is absent. Returns a clone of the loaded favorites. Holds the disk lock across the read-and-seed
/// so two concurrent first-access callers can't both seed.
fn load_or_seed() -> Vec<Favorite> {
    {
        let guard = cache().lock_ignore_poison();
        if let Some(store) = guard.as_ref() {
            return store.favorites.clone();
        }
    }

    let path = favorites_path();
    let _disk_guard = disk_lock().lock_ignore_poison();

    // Re-check under the disk lock in case another thread seeded while we waited.
    {
        let guard = cache().lock_ignore_poison();
        if let Some(store) = guard.as_ref() {
            return store.favorites.clone();
        }
    }

    let store = match read_store_from_path(&path) {
        Some(store) => store,
        None => {
            // First launch: seed the defaults and persist them.
            let seeded = FavoritesStore {
                schema_version: CURRENT_SCHEMA_VERSION,
                favorites: default_favorites(),
            };
            if let Err(e) = write_store_to_path(&path, &seeded) {
                log::warn!(target: "favorites::store", "Couldn't seed favorites file: {e}");
            } else {
                log::info!(target: "favorites::store", "Seeded {} default favorites", seeded.favorites.len());
            }
            seeded
        }
    };

    let favorites = store.favorites.clone();
    *cache().lock_ignore_poison() = Some(store);
    favorites
}

/// Applies a mutation to the cached store and persists it. The `mutate` closure runs under the cache
/// lock and returns whether the change is worth persisting (so a no-op skips the disk write).
fn mutate_and_persist<F>(mutate: F)
where
    F: FnOnce(&mut FavoritesStore) -> bool,
{
    // Make sure the store is loaded (and seeded) before mutating.
    load_or_seed();

    let snapshot = {
        let mut guard = cache().lock_ignore_poison();
        let store = guard.get_or_insert_with(FavoritesStore::default);
        store.schema_version = CURRENT_SCHEMA_VERSION;
        if !mutate(store) {
            return; // No-op: skip the disk write.
        }
        store.clone()
    };

    let path = favorites_path();
    let _disk_guard = disk_lock().lock_ignore_poison();
    if let Err(e) = write_store_to_path(&path, &snapshot) {
        log::warn!(target: "favorites::store", "Couldn't write favorites: {e}");
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns the favorites in order, seeding the defaults on first access if the file is absent.
pub fn list() -> Vec<Favorite> {
    load_or_seed()
}

/// Adds a favorite for `path`, deduping by normalized path (a re-add moves the existing entry to the
/// end). When `name` is `None`, the label defaults to the path's file name.
pub fn add(path: &str, name: Option<String>) {
    mutate_and_persist(|store| {
        add_to_store(store, path, name);
        true
    });
}

/// Removes a favorite by id. No-op when the id isn't present.
pub fn remove(id: &str) {
    mutate_and_persist(|store| remove_from_store(store, id));
}

/// Renames a favorite by id. No-op when the id isn't present.
pub fn rename(id: &str, name: &str) {
    mutate_and_persist(|store| rename_in_store(store, id, name));
}

/// Reorders the favorites to match `ordered_ids`. Unknown ids are ignored; favorites missing from the
/// list are appended in their current order.
pub fn reorder(ordered_ids: &[String]) {
    mutate_and_persist(|store| {
        reorder_store(store, ordered_ids);
        true
    });
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(paths: &[(&str, &str)]) -> FavoritesStore {
        FavoritesStore {
            schema_version: CURRENT_SCHEMA_VERSION,
            favorites: paths
                .iter()
                .map(|(path, name)| Favorite {
                    id: new_id(),
                    path: path.to_string(),
                    name: name.to_string(),
                })
                .collect(),
        }
    }

    // -- add: dedup, naming, ordering --

    #[test]
    fn add_appends_new_path_with_derived_name() {
        let mut store = FavoritesStore::default();
        add_to_store(&mut store, "/Users/x/Projects", None);
        assert_eq!(store.favorites.len(), 1);
        assert_eq!(store.favorites[0].path, "/Users/x/Projects");
        assert_eq!(
            store.favorites[0].name, "Projects",
            "name defaults to the path's file name"
        );
    }

    #[test]
    fn add_uses_explicit_name_when_given() {
        let mut store = FavoritesStore::default();
        add_to_store(&mut store, "/Users/x/Projects", Some("Work".to_string()));
        assert_eq!(store.favorites[0].name, "Work");
    }

    #[test]
    fn add_dedups_by_normalized_path_and_keeps_id() {
        let mut store = FavoritesStore::default();
        let first_id = add_to_store(&mut store, "/Users/x/a", None);
        add_to_store(&mut store, "/Users/x/b", None);

        // Re-add `/a` with a trailing slash: should dedup (not grow) and keep the original id.
        let dup_id = add_to_store(&mut store, "/Users/x/a/", None);
        assert_eq!(store.favorites.len(), 2, "trailing-slash re-add should collapse");
        assert_eq!(dup_id, first_id, "re-add keeps the existing id");
        // The re-added entry moves to the end.
        assert_eq!(store.favorites[0].path, "/Users/x/b");
        assert_eq!(store.favorites[1].path, "/Users/x/a");
    }

    #[test]
    fn add_re_add_with_name_overrides_existing_label() {
        let mut store = FavoritesStore::default();
        add_to_store(&mut store, "/Users/x/a", None);
        add_to_store(&mut store, "/Users/x/a", Some("Renamed".to_string()));
        assert_eq!(store.favorites.len(), 1);
        assert_eq!(store.favorites[0].name, "Renamed");
    }

    #[test]
    fn ids_are_unique_across_adds() {
        let mut store = FavoritesStore::default();
        add_to_store(&mut store, "/a", None);
        add_to_store(&mut store, "/b", None);
        assert_ne!(store.favorites[0].id, store.favorites[1].id);
    }

    // -- remove --

    #[test]
    fn remove_returns_true_when_found_and_false_otherwise() {
        let mut store = store_with(&[("/a", "a")]);
        let id = store.favorites[0].id.clone();
        assert!(remove_from_store(&mut store, &id));
        assert!(store.favorites.is_empty());
        assert!(!remove_from_store(&mut store, &id));
    }

    // -- rename --

    #[test]
    fn rename_updates_label_by_id() {
        let mut store = store_with(&[("/a", "a")]);
        let id = store.favorites[0].id.clone();
        assert!(rename_in_store(&mut store, &id, "New name"));
        assert_eq!(store.favorites[0].name, "New name");
        assert!(!rename_in_store(&mut store, "missing-id", "x"));
    }

    // -- reorder --

    #[test]
    fn reorder_applies_the_given_order() {
        let mut store = store_with(&[("/a", "a"), ("/b", "b"), ("/c", "c")]);
        let ids: Vec<String> = store.favorites.iter().map(|f| f.id.clone()).collect();
        let new_order = vec![ids[2].clone(), ids[0].clone(), ids[1].clone()];
        reorder_store(&mut store, &new_order);
        assert_eq!(store.favorites[0].path, "/c");
        assert_eq!(store.favorites[1].path, "/a");
        assert_eq!(store.favorites[2].path, "/b");
    }

    #[test]
    fn reorder_appends_unnamed_entries_and_ignores_unknown_ids() {
        let mut store = store_with(&[("/a", "a"), ("/b", "b"), ("/c", "c")]);
        let ids: Vec<String> = store.favorites.iter().map(|f| f.id.clone()).collect();
        // Only name `/c`, plus a stale id. `/a` and `/b` must survive, appended in order.
        let partial = vec![ids[2].clone(), "stale-id".to_string()];
        reorder_store(&mut store, &partial);
        assert_eq!(store.favorites.len(), 3, "reorder must never drop entries");
        assert_eq!(store.favorites[0].path, "/c");
        assert_eq!(store.favorites[1].path, "/a");
        assert_eq!(store.favorites[2].path, "/b");
    }

    // -- name_from_path --

    #[test]
    fn name_from_path_uses_last_component_with_root_fallback() {
        assert_eq!(name_from_path("/Users/x/Downloads"), "Downloads");
        assert_eq!(name_from_path("/Applications"), "Applications");
        assert_eq!(name_from_path("/"), "/");
    }

    // -- default seed --

    #[test]
    fn default_favorites_are_four_with_unique_ids() {
        let defaults = default_favorites();
        assert_eq!(defaults.len(), 4);
        let names: Vec<&str> = defaults.iter().map(|f| f.name.as_str()).collect();
        #[cfg(target_os = "macos")]
        assert_eq!(names, vec!["Applications", "Desktop", "Documents", "Downloads"]);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(names, vec!["Home", "Desktop", "Documents", "Downloads"]);
        let unique: std::collections::HashSet<&str> = defaults.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(unique.len(), 4, "every seeded favorite gets its own id");
    }

    // -- serialization round-trip --

    #[test]
    fn favorite_serialization_round_trip() {
        let f = Favorite {
            id: "abc-123".to_string(),
            path: "/Users/test/Documents".to_string(),
            name: "Documents".to_string(),
        };
        let json = serde_json::to_string_pretty(&f).unwrap();
        assert!(json.contains("\"path\": \"/Users/test/Documents\""));
        let back: Favorite = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn store_serialization_carries_schema_version() {
        let store = FavoritesStore::default();
        let json = serde_json::to_string_pretty(&store).unwrap();
        assert!(json.contains("\"_schemaVersion\": 1"));
        let back: FavoritesStore = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, 1);
        assert!(back.favorites.is_empty());
    }

    // -- disk: seed-once, no-reseed, empty-stays-empty, round-trip, recovery --

    #[test]
    fn missing_file_reads_as_none_signaling_seed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        assert!(read_store_from_path(&path).is_none());
    }

    #[test]
    fn seed_then_load_round_trip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);

        let seeded = FavoritesStore {
            schema_version: CURRENT_SCHEMA_VERSION,
            favorites: default_favorites(),
        };
        write_store_to_path(&path, &seeded).expect("write");

        let loaded = read_store_from_path(&path).expect("present");
        assert_eq!(loaded.favorites.len(), 4);
        assert_eq!(loaded.favorites[0].path, "/Applications");
    }

    #[test]
    fn present_empty_file_stays_empty_no_reseed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        // A user who removed every favorite: file present, empty list.
        let empty = FavoritesStore {
            schema_version: CURRENT_SCHEMA_VERSION,
            favorites: Vec::new(),
        };
        write_store_to_path(&path, &empty).expect("write");

        let loaded = read_store_from_path(&path).expect("present, not None");
        assert!(loaded.favorites.is_empty(), "an emptied list must not re-seed");
    }

    #[test]
    fn persistence_round_trip_after_mutations() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);

        let mut store = FavoritesStore::default();
        add_to_store(&mut store, "/Users/x/a", None);
        let removed_id = add_to_store(&mut store, "/Users/x/b", None);
        add_to_store(&mut store, "/Users/x/c", Some("See".to_string()));
        remove_from_store(&mut store, &removed_id);
        write_store_to_path(&path, &store).expect("write");

        let loaded = read_store_from_path(&path).expect("present");
        let paths: Vec<&str> = loaded.favorites.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["/Users/x/a", "/Users/x/c"]);
        assert_eq!(loaded.favorites[1].name, "See");
    }

    #[test]
    fn corrupted_json_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        fs::write(&path, "{not valid json").expect("write garbage");

        // A corrupt file is "initialized but broken", so it reads as an empty store (Some), not None
        // (which would re-seed the defaults over the user's lost-but-not-first-launch state).
        let store = read_store_from_path(&path).expect("corrupt reads as Some(empty)");
        assert!(store.favorites.is_empty());
        let broken = path.with_extension("json.broken");
        assert!(broken.exists(), "expected quarantine at {broken:?}");
        assert!(!path.exists());
    }

    #[test]
    fn schema_version_mismatch_quarantines_and_starts_fresh() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        fs::write(&path, r#"{"_schemaVersion": 2, "favorites": []}"#).expect("write");

        let store = read_store_from_path(&path).expect("mismatch reads as Some(empty)");
        assert!(store.favorites.is_empty());
        assert!(path.with_extension("json.broken").exists());
    }

    #[test]
    fn present_but_unreadable_reads_as_some_empty_and_never_reseeds() {
        // A present-but-unreadable file (here simulated by a directory at the favorites path, which
        // makes `read_to_string` fail with a non-NotFound error) must NOT read as `None`: `None`
        // means "first launch" and would re-seed the four defaults OVER the user's real list.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        fs::create_dir(&path).expect("create dir where the file would be");

        let store = read_store_from_path(&path).expect("present-but-unreadable must read as Some, not None");
        assert!(store.favorites.is_empty());
        // It must NOT have been quarantined or removed: we can't confirm corruption, so leave it be.
        assert!(path.exists(), "the unreadable path is left intact for a later read");
    }

    #[test]
    fn stale_tmp_file_is_cleaned_up_on_read() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FAVORITES_FILE_NAME);
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, "stale").expect("write tmp");
        let _ = read_store_from_path(&path);
        assert!(!tmp.exists(), "stale tmp should be removed");
    }
}
