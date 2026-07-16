//! Per-volume search index registry, lifecycle timers, and importance weights.
//!
//! Search is multi-volume: the root drive, plus any SMB share or MTP storage that
//! has a persisted `index-{volume_id}.db`. Each volume's arena loads lazily into
//! [`SEARCH_INDICES`] on first use and all of them drop together when the dialog
//! goes idle (RAM reclaim). The DB FILE on disk is the source of truth — a volume
//! need NOT be registered in `INDEX_REGISTRY` to be searched (an ejected drive's
//! index is still on disk), so a non-root volume opens its own read-only pool
//! straight from the file rather than routing through the live registry.
//!
//! The lifecycle is dialog-scoped, not per-volume: opening the dialog pre-loads
//! root and starts the timers; a search lazily loads whatever volumes its scope
//! needs; closing the dialog (or inactivity) drops every loaded arena at once. So
//! the timers are global here, keyed off `DIALOG_OPEN` + `LAST_SEARCH_ACTIVITY`,
//! exactly as the single-volume design was.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use crate::ignore_poison::IgnorePoison;
use crate::indexing::store::IndexStore;
use crate::indexing::writer::WRITER_GENERATION;
use crate::indexing::{ROOT_VOLUME_ID, ReadPool, get_read_pool};

use super::index::{SearchIndex, load_search_index, now_secs};
use super::ranking::ImportanceWeights;

// ── App data dir (set once at startup) ───────────────────────────────

/// The resolved app data dir, where every `index-{volume_id}.db` and
/// `importance-{volume_id}.db` lives. Set once from app setup (search commands and
/// MCP have no `AppHandle`, so they read it from here instead of re-resolving).
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub(crate) fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

fn data_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

// ── Loaded volume state ──────────────────────────────────────────────

/// A loaded volume's search state: the in-memory arena plus everything a search
/// needs against it — a read pool over its DB (include-path resolution + dir-size
/// enrichment) and its mount root (path prefixing/stripping). Importance weights
/// live in the separate [`WEIGHTS`] map so the root recompute subscriber can swap
/// them live without rebuilding this.
pub(crate) struct LoadedVolume {
    pub(crate) index: Arc<SearchIndex>,
    pub(crate) pool: Arc<ReadPool>,
    /// The volume's mount root (`/Volumes/naspi`), or `None` for `root` (whose index
    /// is `/`-rooted, so paths are already absolute). Read from the index DB's
    /// `volume_path` meta, so it's known even when the volume isn't currently
    /// mounted (an unscoped all-volumes search over an offline drive still reports
    /// absolute paths).
    pub(crate) mount_root: Option<String>,
    /// `WRITER_GENERATION` stamped at load. Only the root writer bumps that global
    /// counter, so this drives root's staleness check; a non-root volume stamps 0
    /// and simply reloads on the next dialog session (its index is far less
    /// volatile, and it drops on idle anyway).
    generation: u64,
}

/// The outcome of loading a volume's index.
pub(crate) enum VolumeLoad {
    /// Loaded (or already warm) and ready to search.
    Loaded(Arc<LoadedVolume>),
    /// No persisted index DB for this volume — it isn't covered by search. The
    /// honesty signal for a scope pointing at an unindexed volume.
    NotIndexed,
    /// The DB exists but couldn't be opened or read (corruption, I/O). Rare;
    /// surfaced so the caller can log rather than silently return empty.
    Failed(String),
}

/// Every loaded volume's search state, keyed by volume id. Cleared wholesale when
/// the dialog goes idle.
static SEARCH_INDICES: LazyLock<Mutex<HashMap<String, Arc<LoadedVolume>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per-volume importance weight snapshots (folder path → weight), blended into
/// ranking. Kept separate from [`SEARCH_INDICES`] so the root recompute subscriber
/// can refresh root's map live (subscribe-don't-poll) without touching the arena.
/// A missing/empty entry degrades ranking to match-quality + recency — today's
/// behavior. Held as `Arc` so a search clones a cheap handle and ranks against a
/// stable snapshot even if a reload swaps it mid-search.
static WEIGHTS: LazyLock<Mutex<HashMap<String, Arc<ImportanceWeights>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// In-flight load cancel flags, keyed by volume id, so `release_search_index` can
/// abort a long root pre-load the moment the dialog closes.
static LOADING: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

// ── Importance weights ───────────────────────────────────────────────

/// A cheap clone of a volume's importance weight snapshot, for the engine to rank
/// against. Empty when none loaded (degrades to match-quality + recency).
pub(crate) fn weights_for(volume_id: &str) -> Arc<ImportanceWeights> {
    WEIGHTS
        .lock_ignore_poison()
        .get(volume_id)
        .cloned()
        .unwrap_or_else(|| Arc::new(ImportanceWeights::empty()))
}

fn store_weights(volume_id: &str, weights: ImportanceWeights) {
    WEIGHTS
        .lock_ignore_poison()
        .insert(volume_id.to_string(), Arc::new(weights));
}

/// Load a volume's importance weights from its `importance-{volume_id}.db`. A
/// missing/empty DB yields an empty map — ranking then degrades cleanly. Runs on a
/// blocking thread (a SQLite read); never on the IPC thread.
fn load_weights(data_dir: &Path, volume_id: &str) -> ImportanceWeights {
    use crate::importance::{ImportanceIndex, SignalSet};
    // `SignalSet::all()` matters only for `explain`, which the bulk weight read
    // ignores; it's the correct default regardless.
    let index = ImportanceIndex::open(data_dir, volume_id, SignalSet::all());
    match index.all_nonzero_weights() {
        Ok(map) => {
            log::debug!(target: "search", "importance weights loaded for '{volume_id}': {} scored folders", map.len());
            ImportanceWeights::from_map(map)
        }
        Err(e) => {
            log::debug!(target: "search", "importance weights not loaded for '{volume_id}': {e}");
            ImportanceWeights::empty()
        }
    }
}

/// Start the root recompute subscriber that keeps root's importance weight map
/// fresh, and record the app data dir for the search commands + MCP.
///
/// Subscribes to root's recompute-completed `watch` (subscribe-don't-poll) and
/// reloads root's weights on each pass, plus once up front (the `watch` retains the
/// last generation, so a recompute finished before this subscription is covered by
/// the initial `borrow_and_update` reload). Non-root volumes take a load-time weight
/// snapshot instead (they drop on idle and reload next session; their importance
/// rarely recomputes mid-session). Called once from app setup.
pub(crate) fn start_importance_weight_subscriber(data_dir: PathBuf) {
    set_data_dir(data_dir.clone());
    let mut rx = crate::importance::read::subscribe(ROOT_VOLUME_ID);
    tauri::async_runtime::spawn(async move {
        let reload = {
            let dir = data_dir.clone();
            move || store_weights(ROOT_VOLUME_ID, load_weights(&dir, ROOT_VOLUME_ID))
        };
        let r = reload.clone();
        let _ = tauri::async_runtime::spawn_blocking(r).await;
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            let _generation = *rx.borrow_and_update();
            let r = reload.clone();
            let _ = tauri::async_runtime::spawn_blocking(r).await;
        }
    });
}

// ── Volume enumeration ───────────────────────────────────────────────

/// Parse the volume id out of an `index-{volume_id}.db` filename, or `None` for a
/// non-index file / sidecar. A volume id may itself contain `-` (MTP serials), so
/// strip the fixed prefix/suffix rather than splitting.
fn volume_id_from_index_db(file_name: &str) -> Option<&str> {
    file_name.strip_prefix("index-")?.strip_suffix(".db")
}

/// Every volume with a persisted index DB on disk, `root` first. The target set for
/// an unscoped search (search all indexed volumes). A missing data dir yields just
/// `root` optimistically (its pool lives in the registry, not a file this enumerates).
pub(crate) fn all_indexed_volume_ids() -> Vec<String> {
    match data_dir() {
        Some(dir) => indexed_volume_ids_in(&dir),
        None => vec![ROOT_VOLUME_ID.to_string()],
    }
}

/// `all_indexed_volume_ids` over an explicit dir (pure enough to unit-test).
fn indexed_volume_ids_in(dir: &Path) -> Vec<String> {
    let mut ids = vec![ROOT_VOLUME_ID.to_string()];
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return ids;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(volume_id) = volume_id_from_index_db(name) else {
            continue;
        };
        if volume_id != ROOT_VOLUME_ID && !volume_id.is_empty() {
            ids.push(volume_id.to_string());
        }
    }
    ids
}

// ── Loading ──────────────────────────────────────────────────────────

fn get_loaded_raw(volume_id: &str) -> Option<Arc<LoadedVolume>> {
    SEARCH_INDICES.lock_ignore_poison().get(volume_id).cloned()
}

/// A cheap handle to a volume's loaded state if it's warm and fresh. Root reloads
/// when the global writer generation moved past its stamp; a non-root volume is
/// always considered fresh for the session (its writer doesn't bump the counter).
pub(crate) fn get_loaded(volume_id: &str) -> Option<Arc<LoadedVolume>> {
    let v = get_loaded_raw(volume_id)?;
    if volume_id == ROOT_VOLUME_ID && v.generation != WRITER_GENERATION.load(Ordering::Relaxed) {
        return None;
    }
    Some(v)
}

/// Read a volume's mount root from its index DB's `volume_path` meta. `None` for
/// `root` (stored as `/`) or when unset — the caller then treats paths as absolute.
fn read_mount_root(pool: &ReadPool) -> Option<String> {
    let stored = pool
        .with_conn(|conn| IndexStore::get_meta(conn, "volume_path").ok().flatten())
        .ok()
        .flatten()?;
    (stored != "/" && !stored.is_empty()).then_some(stored)
}

/// Load one volume's index synchronously (call inside `spawn_blocking`). Opens the
/// read pool (root's from the live registry; a non-root volume's read-only straight
/// from `index-{volume_id}.db` on disk), loads the arena, reads the mount root, and
/// loads the volume's importance weights into [`WEIGHTS`].
fn load_volume_blocking(volume_id: &str, data_dir: &Path, cancel: &AtomicBool) -> VolumeLoad {
    let (pool, mount_root, generation) = if volume_id == ROOT_VOLUME_ID {
        // Root's pool is the live registry's; absent means the root scan hasn't
        // produced a searchable index yet (indexing off / first scan running).
        match get_read_pool() {
            Some(pool) => (pool, None, WRITER_GENERATION.load(Ordering::Relaxed)),
            None => return VolumeLoad::NotIndexed,
        }
    } else {
        let db_path = data_dir.join(format!("index-{volume_id}.db"));
        if !db_path.exists() {
            return VolumeLoad::NotIndexed;
        }
        let pool = match ReadPool::new(db_path) {
            Ok(pool) => Arc::new(pool),
            Err(e) => return VolumeLoad::Failed(format!("open index for '{volume_id}': {e}")),
        };
        let mount_root = read_mount_root(&pool);
        (pool, mount_root, 0)
    };

    let index = match load_search_index(&pool, cancel) {
        Ok(index) => Arc::new(index),
        Err(e) => return VolumeLoad::Failed(e),
    };

    store_weights(volume_id, load_weights(data_dir, volume_id));

    VolumeLoad::Loaded(Arc::new(LoadedVolume {
        index,
        pool,
        mount_root,
        generation,
    }))
}

/// Ensure a volume's index is loaded and return it (cache-aware). A warm, fresh
/// entry returns immediately; otherwise it loads synchronously (open the DB + read
/// the arena — call inside `spawn_blocking`), caches it, and arms the backstop
/// timer. The load is cancelable via `release_search_index`.
pub(crate) fn ensure_volume(volume_id: &str) -> VolumeLoad {
    if let Some(v) = get_loaded(volume_id) {
        return VolumeLoad::Loaded(v);
    }

    let Some(data_dir) = data_dir() else {
        return VolumeLoad::Failed("search data dir not initialized".to_string());
    };

    let cancel = Arc::new(AtomicBool::new(false));
    LOADING
        .lock_ignore_poison()
        .insert(volume_id.to_string(), cancel.clone());

    let outcome = load_volume_blocking(volume_id, &data_dir, &cancel);

    LOADING.lock_ignore_poison().remove(volume_id);

    if let VolumeLoad::Loaded(ref v) = outcome {
        SEARCH_INDICES
            .lock_ignore_poison()
            .insert(volume_id.to_string(), v.clone());
        ensure_backstop_running();
    }
    outcome
}

// ── Lifecycle: dialog state + timers ─────────────────────────────────

/// Timestamp of the last search-related IPC call, for the backstop timeout.
static LAST_SEARCH_ACTIVITY: AtomicU64 = AtomicU64::new(0);

/// Whether the search dialog is open. Timers defer dropping while it's true.
pub(crate) static DIALOG_OPEN: AtomicBool = AtomicBool::new(false);

/// Idle timeout: drop every loaded arena 5 minutes after the dialog closes.
const IDLE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Backstop timeout: drop everything if no search calls arrive within 10 minutes
/// (covers MCP-driven loads, which have no dialog to close).
const BACKSTOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// The lifecycle timer handles. Global (not per-volume): the whole set of loaded
/// arenas shares one idle + one backstop timer.
#[derive(Default)]
struct Timers {
    idle: Option<tauri::async_runtime::JoinHandle<()>>,
    backstop: Option<tauri::async_runtime::JoinHandle<()>>,
}

static TIMERS: LazyLock<Mutex<Timers>> = LazyLock::new(|| Mutex::new(Timers::default()));

/// Record search activity (resets the backstop window).
pub(crate) fn touch_activity() {
    LAST_SEARCH_ACTIVITY.store(now_secs(), Ordering::Relaxed);
}

/// Signal every in-flight load to cancel (dialog closed mid-load).
pub(crate) fn cancel_active_loads() {
    for cancel in LOADING.lock_ignore_poison().values() {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// Drop every loaded arena, reclaiming their RAM, and clear the timers.
pub(crate) fn drop_all_indices() {
    SEARCH_INDICES.lock_ignore_poison().clear();
    let mut timers = TIMERS.lock_ignore_poison();
    if let Some(h) = timers.idle.take() {
        h.abort();
    }
    if let Some(h) = timers.backstop.take() {
        h.abort();
    }
    log::debug!("Search indices dropped (all volumes)");
}

/// Start the backstop timer if one isn't already running. Called after any load, so
/// MCP-driven loads (no dialog) still get reclaimed.
fn ensure_backstop_running() {
    let mut timers = TIMERS.lock_ignore_poison();
    if timers.backstop.is_some() {
        return;
    }
    timers.backstop = Some(tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(BACKSTOP_TIMEOUT).await;
            let elapsed = now_secs().saturating_sub(LAST_SEARCH_ACTIVITY.load(Ordering::Relaxed));
            if elapsed >= BACKSTOP_TIMEOUT.as_secs() {
                if DIALOG_OPEN.load(Ordering::Relaxed) {
                    log::debug!("Search backstop timer deferred, dialog still open");
                    continue;
                }
                log::debug!("Search backstop timeout reached, dropping indices");
                drop_all_indices();
                break;
            }
        }
    }));
}

/// (Re)start the backstop timer, cancelling a prior one. Called when the dialog
/// opens with a warm index so a stale session's timer can't fire mid-use.
pub(crate) fn reset_backstop_timer() {
    let mut timers = TIMERS.lock_ignore_poison();
    if let Some(h) = timers.backstop.take() {
        h.abort();
    }
    drop(timers);
    ensure_backstop_running();
}

/// Cancel any pending idle timer (a new search is active).
pub(crate) fn cancel_idle_timer() {
    if let Some(h) = TIMERS.lock_ignore_poison().idle.take() {
        h.abort();
    }
}

/// Start the idle timer (5 min). Called when the search dialog closes; drops every
/// loaded arena when it fires unless the dialog reopened.
pub(crate) fn start_idle_timer() {
    let mut timers = TIMERS.lock_ignore_poison();
    if let Some(h) = timers.idle.take() {
        h.abort();
    }
    timers.idle = Some(tauri::async_runtime::spawn(async {
        loop {
            tokio::time::sleep(IDLE_TIMEOUT).await;
            if DIALOG_OPEN.load(Ordering::Relaxed) {
                log::debug!("Search idle timer deferred, dialog still open");
                continue;
            }
            log::debug!("Search idle timeout reached, dropping indices");
            drop_all_indices();
            break;
        }
    }));
}

#[cfg(test)]
mod tests;
