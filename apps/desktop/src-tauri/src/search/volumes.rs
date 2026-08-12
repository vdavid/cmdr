//! Per-volume search index registry, lifecycle timers, and importance weights.
//!
//! Any volume with a persisted `index-{volume_id}.db` is searchable — the root
//! drive, an SMB share, an MTP storage — though ONE search covers exactly one of
//! them (`execute.rs`). Each volume's arena loads lazily into
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
use std::sync::{Arc, LazyLock, Mutex};

use crate::ignore_poison::IgnorePoison;
use crate::index_host::index;
use cmdr_index::store::IndexStore;
use cmdr_index::{CoverageToken, ROOT_VOLUME_ID, ReadPool};

use super::index::{SearchIndex, load_search_index, now_secs};

mod weights;
use weights::{load_weights, store_weights};
pub(crate) use weights::{start_importance_weight_subscriber, weights_for};

// ── App data dir (set once at startup) ───────────────────────────────

/// The resolved app data dir, where every `index-{volume_id}.db` and
/// `importance-{volume_id}.db` lives. Set from app setup (search commands and MCP
/// have no `AppHandle`, so they read it from here instead of re-resolving), and
/// swapped under a guard by a test that needs its own — a live search builds a
/// real index DB in there, so a shared path would have two tests reading each
/// other's drives.
static DATA_DIR: LazyLock<Mutex<Option<PathBuf>>> = LazyLock::new(|| Mutex::new(None));

pub(crate) fn set_data_dir(dir: PathBuf) {
    *DATA_DIR.lock_ignore_poison() = Some(dir);
}

fn data_dir() -> Option<PathBuf> {
    DATA_DIR.lock_ignore_poison().clone()
}

/// Point search's data directory somewhere for one test, restoring the previous
/// value on drop. Hold `cmdr_index::testing::host::test_lock` first: this slot is
/// process-wide, like the index's own.
#[cfg(test)]
pub(crate) fn install_data_dir_for_test(dir: &Path) -> TestDataDirGuard {
    let previous = DATA_DIR.lock_ignore_poison().replace(dir.to_path_buf());
    TestDataDirGuard { previous }
}

/// Puts the previous search data directory back.
#[cfg(test)]
pub(crate) struct TestDataDirGuard {
    previous: Option<PathBuf>,
}

#[cfg(test)]
impl Drop for TestDataDirGuard {
    fn drop(&mut self) {
        *DATA_DIR.lock_ignore_poison() = self.previous.take();
    }
}

/// Drop one volume's arena and its walk mark, so a test's drive can't be served
/// to whatever runs next in the same binary.
#[cfg(test)]
pub(crate) fn forget_volume_for_test(volume_id: &str) {
    SEARCH_INDICES.lock_ignore_poison().remove(volume_id);
    take_walked_behind(volume_id);
}

// ── Loaded volume state ──────────────────────────────────────────────

/// A loaded volume's search state: the in-memory arena plus everything a search
/// needs against it — a read pool over its DB (include-path resolution + dir-size
/// enrichment) and its mount root (path prefixing/stripping). Importance weights
/// live in the separate `WEIGHTS` map so the root recompute subscriber can swap
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
    /// counter, so this drives root's staleness check (which triggers a BACKGROUND
    /// refresh, never a reload in front of a search — see [`get_loaded`]); a non-root
    /// volume stamps 0 and simply reloads on the next dialog session (its index is far
    /// less volatile, and it drops on idle anyway).
    generation: u64,
    /// Which state of the volume's index this arena is a snapshot of, read just
    /// BEFORE the rows were (so it can only under-claim). A coverage answer is
    /// honored only while its own token matches this one: an answer that calls a
    /// subtree covered is a promise the arena holds its rows, and a walk that
    /// wrote rows behind the arena breaks that promise silently
    /// (`docs/specs/unindexed-search-plan.md` Decision 12).
    pub(crate) coverage_token: CoverageToken,
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

/// In-flight load cancel flags, keyed by volume id, so `release_search_index` can
/// abort a long root pre-load the moment the dialog closes.
static LOADING: LazyLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Per-volume load gates: whoever holds one is loading that volume, and everyone
/// else waits for its result instead of starting a second pass over the same DB.
/// Keyed per volume, so different volumes still load concurrently.
static LOAD_GATES: LazyLock<Mutex<HashMap<String, Arc<Mutex<()>>>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Bumped by [`cancel_active_loads`]. A thread that WAITED on someone else's load
/// re-reads this before starting its own: without it, cancelling (dialog closed
/// mid-load) would just hand the multi-second load to the next thread in line.
static CANCEL_EPOCH: AtomicU64 = AtomicU64::new(0);

fn load_gate(volume_id: &str) -> Arc<Mutex<()>> {
    LOAD_GATES
        .lock_ignore_poison()
        .entry(volume_id.to_string())
        .or_default()
        .clone()
}

// ── Loading ──────────────────────────────────────────────────────────

fn get_loaded_raw(volume_id: &str) -> Option<Arc<LoadedVolume>> {
    SEARCH_INDICES.lock_ignore_poison().get(volume_id).cloned()
}

/// Whether a volume's arena has fallen behind its DB. Only root can: the global
/// writer generation moves on every root mutation, while a non-root volume stamps
/// `0` and simply reloads next dialog session.
fn is_stale(volume_id: &str, v: &LoadedVolume) -> bool {
    volume_id == ROOT_VOLUME_ID && v.generation != index().search_generation()
}

/// A cheap handle to a volume's warm arena, refreshing it in the BACKGROUND when it
/// has fallen behind the writer.
///
/// The arena is a snapshot by construction, and root's DB moves under it constantly:
/// a live-watched boot disk bumps `WRITER_GENERATION` several times a SECOND (measured
/// ~5.7/s idle on a dev machine, 2026-07-28 logs), so "the writer moved ⇒ rebuild
/// before answering" put a multi-second full reload (2.6 s for 6.3 M entries, warm
/// page cache) in front of nearly every dialog open and every auto-applied keystroke
/// search. Serving the warm arena and refreshing behind it trades at most
/// [`REFRESH_MIN_INTERVAL`] of extra staleness — on top of the seconds the indexer
/// itself lags disk — for an instant answer, and bounds the rebuild cost to one pass
/// per interval instead of one per search.
pub(crate) fn get_loaded(volume_id: &str) -> Option<Arc<LoadedVolume>> {
    let v = get_loaded_raw(volume_id)?;
    if is_stale(volume_id, &v) {
        spawn_background_refresh(volume_id);
    }
    Some(v)
}

/// The floor on how often a stale-but-warm arena is rebuilt in the background. A
/// rebuild costs seconds of CPU and a transient second copy of the arena, and the
/// generation moves again within moments of one finishing, so without a floor the
/// refreshes would run back-to-back for as long as the dialog stays open.
const REFRESH_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// When each volume's arena last STARTED a (re)load, for the [`REFRESH_MIN_INTERVAL`]
/// floor.
static LAST_LOAD_STARTED: LazyLock<Mutex<HashMap<String, std::time::Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Whether a fresh (re)load may start now, recording the attempt when it may. Also
/// declines while a load for this volume is already in flight.
fn claim_load_slot(volume_id: &str, min_interval: std::time::Duration) -> bool {
    if LOADING.lock_ignore_poison().contains_key(volume_id) {
        return false;
    }
    let mut last = LAST_LOAD_STARTED.lock_ignore_poison();
    match last.get(volume_id) {
        Some(at) if at.elapsed() < min_interval => false,
        _ => {
            last.insert(volume_id.to_string(), std::time::Instant::now());
            true
        }
    }
}

/// Rebuild a warm volume's arena off the IPC path, then swap it in.
///
/// Skips the swap when the arena was dropped meanwhile (idle timeout, dialog closed):
/// re-inserting there would resurrect hundreds of MB nobody asked for.
fn spawn_background_refresh(volume_id: &str) {
    if !claim_load_slot(volume_id, REFRESH_MIN_INTERVAL) {
        return;
    }
    let volume_id = volume_id.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(data_dir) = data_dir() else { return };
        // Same gate `ensure_volume` uses, so a refresh and a cold load can't read the
        // same DB at once (a cold `ensure_volume` waiting here gets this arena).
        let gate = load_gate(&volume_id);
        let _gate_held = gate.lock_ignore_poison();
        let cancel = Arc::new(AtomicBool::new(false));
        LOADING.lock_ignore_poison().insert(volume_id.clone(), cancel.clone());
        let outcome = load_volume_blocking(&volume_id, &data_dir, &cancel);
        LOADING.lock_ignore_poison().remove(&volume_id);

        match outcome {
            VolumeLoad::Loaded(v) => {
                let mut indices = SEARCH_INDICES.lock_ignore_poison();
                if indices.contains_key(&volume_id) {
                    indices.insert(volume_id.clone(), v);
                    log::debug!("Search index refreshed in the background for '{volume_id}'");
                }
            }
            VolumeLoad::NotIndexed => {}
            VolumeLoad::Failed(e) => log::debug!("Search index background refresh for '{volume_id}' stopped: {e}"),
        }
    });
}

/// Whether a volume has an index to load at all, without loading it: root's live
/// read pool, or a persisted `index-{volume_id}.db` on disk. Mirrors the two branches
/// [`load_volume_blocking`] takes before it reads anything.
///
/// The dialog asks this so it can tell "an arena is on its way, wait for the
/// `search-index-ready` event" from "there is nothing to wait for". Without the
/// distinction a machine that declined indexing waits forever and never searches.
pub(crate) fn has_searchable_index(volume_id: &str) -> bool {
    if get_loaded_raw(volume_id).is_some() {
        return true;
    }
    if volume_id == ROOT_VOLUME_ID {
        return index().read_pool(ROOT_VOLUME_ID).is_some();
    }
    data_dir().is_some_and(|dir| dir.join(format!("index-{volume_id}.db")).exists())
}

/// A non-root volume's mount root, needed to prefix its mount-relative index paths.
///
/// Prefers the persisted `volume_path` meta; falls back to the LIVE volume registry
/// when that meta is absent. The fallback matters because SMB index DBs written
/// before this meta existed never wrote it (only the local scan-completion path
/// did), so a real NAS index has no `volume_path` — without the fallback its mount
/// root reads as `None`, scope paths never get stripped, and every scoped search
/// returns nothing. While the volume is mounted (the only time a `/Volumes/…` scope
/// even routes to it) the registry knows its root; the DB is also healed on the next
/// SMB registration so offline reads recover too. `None` only when neither source
/// has it (an offline volume whose DB predates the meta) — the caller then treats
/// the scope as unresolved rather than mis-rooting it.
fn read_mount_root(pool: &ReadPool, volume_id: &str) -> Option<String> {
    let from_meta = pool
        .with_conn(|conn| IndexStore::get_meta(conn, "volume_path").ok().flatten())
        .ok()
        .flatten()
        .and_then(usable_mount_root);
    from_meta.or_else(|| registry_mount_root(volume_id))
}

/// A mounted volume's root, from the LIVE registry.
///
/// The fallback half of [`read_mount_root`], and the only source a live search
/// has when the volume has no arena to read the meta from — a drive nobody ever
/// indexed still has a mount root, and the walk reports paths under it.
pub(crate) fn registry_mount_root(volume_id: &str) -> Option<String> {
    crate::file_system::volume::manager::get_volume_manager()
        .get(volume_id)
        .map(|v| v.root().to_string_lossy().into_owned())
        .and_then(usable_mount_root)
}

/// `/` and the empty string aren't mount roots: they're what an index with no
/// `volume_path` meta, or the boot volume, reports.
fn usable_mount_root(root: String) -> Option<String> {
    (root != "/" && !root.is_empty()).then_some(root)
}

/// Load one volume's index synchronously (call inside `spawn_blocking`). Opens the
/// read pool (root's from the live registry; a non-root volume's read-only straight
/// from `index-{volume_id}.db` on disk), loads the arena, reads the mount root, and
/// loads the volume's importance weights into `WEIGHTS`.
fn load_volume_blocking(volume_id: &str, data_dir: &Path, cancel: &AtomicBool) -> VolumeLoad {
    // Taken BEFORE the rows, so a write racing this load makes the arena look
    // older than it is rather than newer. Under-claiming costs a reload; the
    // other way round would serve a coverage answer the arena can't back.
    let coverage_token = index().coverage_token(volume_id);

    let (pool, mount_root, generation) = if volume_id == ROOT_VOLUME_ID {
        // Root's pool is the live registry's; absent means the root scan hasn't
        // produced a searchable index yet (indexing off / first scan running).
        match index().read_pool(ROOT_VOLUME_ID) {
            Some(pool) => (pool, None, index().search_generation()),
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
        let mount_root = read_mount_root(&pool, volume_id);
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
        coverage_token,
    }))
}

// ── Arenas a walk left behind (Decision 12) ──────────────────────────

/// Volumes a cover walk has written rows into since their arena was loaded.
///
/// Only a WALK marks one. Background indexing writes rows constantly on the boot
/// disk, and rebuilding on that would put a multi-second pass in front of every
/// search ([`get_loaded`] says why that was removed once already) — while the lag
/// it would close is the ordinary index lag search has always had. A walk is
/// different in kind: the same search that just showed those files live would
/// prune them as "covered" on its next run and show FEWER results, with no
/// signal. So the mark is narrow on purpose.
static WALKED_BEHIND: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

/// Record that a walk wrote rows into this volume's index, so the next query
/// reloads its arena before pruning anything as covered.
pub(crate) fn mark_walked_behind(volume_id: &str) {
    WALKED_BEHIND.lock_ignore_poison().insert(volume_id.to_string());
}

/// Take the mark, if there is one. Taking it is the caller's promise to reload:
/// a walk still running re-marks with its next batch, so a query that reloads
/// mid-walk is behind by at most what landed since.
pub(crate) fn take_walked_behind(volume_id: &str) -> bool {
    WALKED_BEHIND.lock_ignore_poison().remove(volume_id)
}

/// Drop a volume's arena and load it again, synchronously.
///
/// For the one caller that must NOT be served a snapshot: a coverage answer whose
/// covered half this arena predates. Loading here rather than in the background is
/// the whole point — the answer is only true against an arena at least as new as
/// it is.
pub(crate) fn reload_volume(volume_id: &str) -> VolumeLoad {
    SEARCH_INDICES.lock_ignore_poison().remove(volume_id);
    ensure_volume(volume_id)
}

/// Ensure a volume's index is loaded and return it (cache-aware). A warm entry
/// returns immediately (refreshing in the background if it has fallen behind — see
/// [`get_loaded`]); a COLD one loads synchronously (open the DB + read the arena —
/// call inside `spawn_blocking`), caches it, and arms the backstop timer. The load is
/// cancelable via `release_search_index`.
///
/// SINGLE-FLIGHT: concurrent callers for the same volume take a per-volume gate, so
/// the second one waits for the first's arena instead of reading the same multi-GB DB
/// a second time (which cost a duplicate load AND a transient second copy of the
/// arena whenever a search arrived while the dialog's pre-load was still running).
pub(crate) fn ensure_volume(volume_id: &str) -> VolumeLoad {
    if let Some(v) = get_loaded(volume_id) {
        return VolumeLoad::Loaded(v);
    }

    let Some(data_dir) = data_dir() else {
        return VolumeLoad::Failed("search data dir not initialized".to_string());
    };

    let epoch = CANCEL_EPOCH.load(Ordering::Relaxed);
    let gate = load_gate(volume_id);
    let _gate_held = gate.lock_ignore_poison();

    // Whoever held the gate may have loaded it for us while we waited.
    if let Some(v) = get_loaded(volume_id) {
        return VolumeLoad::Loaded(v);
    }
    // …or may have been cancelled (dialog closed). Cancelling must not simply hand
    // the load to the next thread in line.
    if CANCEL_EPOCH.load(Ordering::Relaxed) != epoch {
        return VolumeLoad::Failed("Load cancelled".to_string());
    }

    LAST_LOAD_STARTED
        .lock_ignore_poison()
        .insert(volume_id.to_string(), std::time::Instant::now());
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

/// Signal every in-flight load to cancel (dialog closed mid-load), and bump
/// [`CANCEL_EPOCH`] so a thread queued behind one of them abandons its turn instead
/// of picking the load straight back up.
pub(crate) fn cancel_active_loads() {
    CANCEL_EPOCH.fetch_add(1, Ordering::Relaxed);
    for cancel in LOADING.lock_ignore_poison().values() {
        cancel.store(true, Ordering::Relaxed);
    }
}

/// Drop every loaded arena, reclaiming their RAM, and clear the timers. Cancels any
/// in-flight load first, so a background refresh can't finish afterwards and pull the
/// RAM straight back (it also declines to re-insert a dropped volume, belt and braces).
pub(crate) fn drop_all_indices() {
    cancel_active_loads();
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
