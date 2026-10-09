//! Persistent, user-editable favorites store (`favorites.json`).
//!
//! An ordered list of `{ id, path, name }` favorites that the frontend's favorites menu (⌃D)
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
//!   fresh, so a stray hand-edit can't break the menu forever.
//! - The disk file is never locked across an `.await`; the in-memory mutex guard is always dropped
//!   before any `fs` call.

use crate::config;
use crate::ignore_poison::IgnorePoison;
use cmdr_fs::volume::app_paths::path_under;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

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
    /// Optional unmodified A–Z key that opens this favorite while its menu is visible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    /// The volume the folder lives on. `None` only on an entry written before this field existed
    /// that no listing has claimed yet (`claim_volumes`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<FavoriteVolume>,
}

/// Which volume a favorite's folder lives on, as it was when last seen live.
///
/// ❗ The id is the identity, exactly as a tab's `volumeId` is: a saved share or server and its
/// live volume share it, so a favorite on an offline place still names the row that dials it.
/// `path` is spelled relative to `root`, which is what lets the listing rebase a mount-rooted
/// favorite when its volume comes back at another mount point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteVolume {
    /// The registry id (`cmdr_fs::volume::ids`, `mtp_ids`), ❌ never derived here.
    pub id: String,
    /// The volume's root when the favorite was added or last claimed: what `path` is under.
    pub root: String,
    /// What the volume was called then, for wording a pick when the volume has no row at all
    /// (an unplugged phone, a forgotten server).
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
            shortcut: None,
            // The first listing claims them (boot volume), so the seed stays syscall-free.
            volume: None,
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

/// Whether `favorite` and an add of `path` on `volume` name the same folder.
///
/// By `(volume id, path under its root)` when both know their volume, so one share mounted at
/// `/Volumes/naspi` and later at `/Volumes/naspi-1` is one favorite. Otherwise by normalized path,
/// which is how a legacy entry (no volume yet) meets its own re-add.
fn is_same_folder(favorite: &Favorite, path: &str, volume: Option<&FavoriteVolume>) -> bool {
    if let (Some(known), Some(added)) = (favorite.volume.as_ref(), volume)
        && let (Some(known_under), Some(added_under)) =
            (path_under(&favorite.path, &known.root), path_under(path, &added.root))
    {
        return known.id == added.id && known_under == added_under;
    }
    normalize_for_dedup(&favorite.path) == normalize_for_dedup(path)
}

/// Adds a favorite, deduping by [`is_same_folder`]. A re-add moves the existing entry to the end,
/// applies an explicit `name` override when given, keeps its id, and takes the add's `path` and
/// `volume` when it names one (the volume's root and name may have moved since). Returns the id of
/// the affected entry.
fn add_to_store(
    store: &mut FavoritesStore,
    path: &str,
    name: Option<String>,
    volume: Option<FavoriteVolume>,
) -> String {
    if let Some(pos) = store
        .favorites
        .iter()
        .position(|f| is_same_folder(f, path, volume.as_ref()))
    {
        let mut existing = store.favorites.remove(pos);
        if let Some(name) = name {
            existing.name = name;
        }
        if volume.is_some() {
            existing.path = path.to_string();
            existing.volume = volume;
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
        shortcut: None,
        volume,
    });
    id
}

/// Fills in the volume of each favorite a claim names, ❗ only where it's still `None`: a volume
/// already there came from the user's own add or an earlier claim, and a claim is only the
/// listing's best reading. Returns whether anything changed.
fn claim_in_store(store: &mut FavoritesStore, claims: &[(String, FavoriteVolume)]) -> bool {
    let mut changed = false;
    for (id, volume) in claims {
        if let Some(favorite) = store.favorites.iter_mut().find(|f| &f.id == id && f.volume.is_none()) {
            favorite.volume = Some(volume.clone());
            changed = true;
        }
    }
    changed
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

/// Assigns one letter to an entry. A letter has one owner: assigning it again clears the old one.
/// `None` removes the target's shortcut. Invalid input and unknown ids leave the store untouched.
fn set_shortcut_in_store(store: &mut FavoritesStore, id: &str, shortcut: Option<&str>) -> bool {
    let normalized = match shortcut {
        Some(letter) if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() => {
            Some(letter.to_ascii_uppercase())
        }
        Some(_) => return false,
        None => None,
    };
    let Some(target) = store.favorites.iter().position(|favorite| favorite.id == id) else {
        return false;
    };
    if store.favorites[target].shortcut == normalized {
        return false;
    }
    if let Some(letter) = &normalized {
        for favorite in &mut store.favorites {
            if favorite.shortcut.as_ref() == Some(letter) {
                favorite.shortcut = None;
            }
        }
    }
    store.favorites[target].shortcut = normalized;
    true
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

/// Resolves the favorites file path without an `AppHandle`.
fn favorites_path() -> PathBuf {
    config::standalone_app_data_dir()
        .unwrap_or_default()
        .join(FAVORITES_FILE_NAME)
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

/// Which favorites gesture a [`StoreChange::Gesture`] is, for analytics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FavoriteAction {
    Added,
    Removed,
    Renamed,
    Reordered,
    ShortcutChanged,
}

impl FavoriteAction {
    fn as_token(self) -> &'static str {
        match self {
            FavoriteAction::Added => "added",
            FavoriteAction::Removed => "removed",
            FavoriteAction::Renamed => "renamed",
            FavoriteAction::Reordered => "reordered",
            FavoriteAction::ShortcutChanged => "shortcut_changed",
        }
    }
}

/// What a [`mutate_and_persist`] call is. A required parameter, so a new mutation can't be added
/// without deciding whether it's something the user did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoreChange {
    /// The user changed their list: reported as `favorite_changed`.
    Gesture(FavoriteAction),
    /// The listing filled in a legacy entry's volume (`claim_volumes`): bookkeeping, ❌ not a
    /// gesture, so it reports nothing.
    Claimed,
}

impl StoreChange {
    /// The `favorite_changed` action this change reports, or `None` for one that reports nothing.
    fn analytics_action(self) -> Option<FavoriteAction> {
        match self {
            Self::Gesture(action) => Some(action),
            Self::Claimed => None,
        }
    }
}

/// Applies a mutation to the cached store and persists it. The `mutate` closure runs under the cache
/// lock and returns whether the change is worth persisting (so a no-op skips the disk write).
fn mutate_and_persist<F>(change: StoreChange, mutate: F)
where
    F: FnOnce(&mut FavoritesStore) -> bool,
{
    // Make sure the store is loaded (and seeded) before mutating.
    // allowed-discarded-outcome: called for the seed side effect only; the list itself is read back under the cache lock below.
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

    // Reported from here, past the no-op guard, so a remove of an id that isn't
    // there (or a rename to the name it already has) doesn't inflate the count.
    // The list SIZE rides along bucketed, because "do people keep favorites?" is
    // answered by how many they end up with, not by how often they touch the list.
    // Never a path or a label: both are the user's own text.
    if let Some(action) = change.analytics_action() {
        crate::analytics::events::capture(
            "favorite_changed",
            serde_json::json!({
                "action": action.as_token(),
                "favorites": crate::analytics::item_count_bucket(snapshot.favorites.len()),
            }),
        );
    }

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

/// The favorites the in-memory cache already holds, without touching disk and without
/// ever waiting for a lock.
///
/// `None` means there is no answer RIGHT NOW: the cache is still cold, or another
/// thread is inside a mutation. Both are the caller's cue to offer nothing. ❗ Never
/// fall back to [`list`] on a `None`: that seeds the file on a cold cache and takes
/// the disk lock behind it, which is exactly what this exists to avoid.
///
/// Written for the Dock tile menu, which AppKit builds on the main thread while the
/// Dock waits on the answer. Rationale and the rest of that contract:
/// `../dock/menu/DETAILS.md`. That menu is the only caller, and only macOS has a Dock,
/// so this is gated the same way `mod dock` is: without the gate it's dead code on Linux
/// and `-D unused` fails the build there.
#[cfg(target_os = "macos")]
pub fn list_cached() -> Option<Vec<Favorite>> {
    let guard = match cache().try_lock() {
        Ok(guard) => guard,
        // Poisoned: some other thread panicked mid-mutation. The store is a plain value
        // holder, so the list it left behind is still the best answer there is — the
        // same call `lock_ignore_poison` makes everywhere else in this file.
        Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    guard.as_ref().map(|store| store.favorites.clone())
}

/// Adds a favorite for `path`, deduping by normalized path (a re-add moves the existing entry to the
/// end). When `name` is `None`, the label defaults to the path's file name.
pub fn add(path: &str, name: Option<String>) {
    mutate_and_persist(StoreChange::Gesture(FavoriteAction::Added), |store| {
        // allowed-discarded-outcome: nobody consumes the new id; both callers answer with `()`.
        add_to_store(store, path, name, None);
        true
    });
}

/// Removes a favorite by id. No-op when the id isn't present.
pub fn remove(id: &str) {
    mutate_and_persist(StoreChange::Gesture(FavoriteAction::Removed), |store| {
        remove_from_store(store, id)
    });
}

/// Renames a favorite by id. No-op when the id isn't present.
pub fn rename(id: &str, name: &str) {
    mutate_and_persist(StoreChange::Gesture(FavoriteAction::Renamed), |store| {
        rename_in_store(store, id, name)
    });
}

/// Sets or clears an A–Z menu shortcut. Reusing a letter transfers it from its previous owner.
pub fn set_shortcut(id: &str, shortcut: Option<&str>) {
    mutate_and_persist(StoreChange::Gesture(FavoriteAction::ShortcutChanged), |store| {
        set_shortcut_in_store(store, id, shortcut)
    });
}

/// Reorders the favorites to match `ordered_ids`. Unknown ids are ignored; favorites missing from the
/// list are appended in their current order.
pub fn reorder(ordered_ids: &[String]) {
    mutate_and_persist(StoreChange::Gesture(FavoriteAction::Reordered), |store| {
        reorder_store(store, ordered_ids);
        true
    });
}

/// Records which volume each named legacy favorite (`volume: None`) lives on, as the listing's
/// reach pass read it (`super::reach`). Fills `None` only, ❌ never overwrites; reports nothing to
/// analytics and emits no `volumes-changed`, because the published row already shows the claimed
/// facts.
///
/// ❗ Writes a file, so the listing calls it off its own path (`spawn_blocking`).
#[expect(dead_code, reason = "its caller, the listing's reach pass, lands next")]
pub fn claim_volumes(claims: &[(String, FavoriteVolume)]) {
    mutate_and_persist(StoreChange::Claimed, |store| claim_in_store(store, claims));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
