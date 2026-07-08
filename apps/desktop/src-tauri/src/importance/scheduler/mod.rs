//! The importance scheduler: recompute a volume's folder weights when its index
//! finishes scanning, and once at startup for a volume that loaded ready.
//!
//! ## What drives a recompute (plan Decision 4 / 5)
//!
//! Two triggers, unified through one coalescing coordinator:
//!
//! 1. **The lifecycle bus** ([`crate::indexing::lifecycle_bus`]): a
//!    `ScanCompleted` publish for a volume ⇒ recompute it. This catches every
//!    scan that finishes while the app runs.
//! 2. **The startup registry sweep** ([`crate::indexing::ready_volume_ids`]): a
//!    volume already `Fresh` at launch (loaded from its persisted
//!    `scan_completed_at`) never re-fires a `ScanCompleted`, so a bus-only
//!    scheduler would never score it — the common restart case. The sweep
//!    enqueues those once at startup.
//!
//! ## Coalescing (plan Decision 4)
//!
//! Both triggers can target one volume at once (the sweep sees it Fresh AND a
//! concurrent startup scan completes). [`PassCoordinator`] guarantees ONE pass
//! runs per volume at a time: a request arriving while a pass runs sets a re-run
//! flag rather than starting a second pass. When the running pass finishes, it
//! re-runs once if the flag is set. This is the pure, unit-testable core.
//!
//! ## Recompute (plan Decision 5)
//!
//! Full-volume: read `dir_stats` + the entry tree through the index read pool,
//! assemble a [`FolderSignals`] per folder (via [`signals`](super::signals)), run
//! the pure scorer, and write every folder's weight through the
//! [`ImportanceWriter`] at a new generation. Cost-bounded by walking the index
//! (already in SQLite), not the filesystem. Runs on a dedicated background task,
//! cancelable, never on the IPC thread. Local volumes only in M2 (SMB is M4).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use tauri::AppHandle;

use super::scorer::{SignalSet, Weights, score};
use super::signals::{OptionalSignals, signals_for_dir};
use super::store::{ImportanceStore, importance_db_path};
use super::writer::{ImportanceWriter, WeightRow};
use crate::ignore_poison::IgnorePoison;
use crate::indexing::ROOT_VOLUME_ID;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

// ── Coalescing coordinator (pure, testable) ──────────────────────────────

/// Per-volume pass bookkeeping: whether a pass is running, and whether another
/// was requested while it ran (the coalescing re-run flag).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct PassSlot {
    running: bool,
    rerun_requested: bool,
}

/// The outcome of requesting a pass for a volume.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum BeginOutcome {
    /// No pass was running; the caller should start one now.
    Start,
    /// A pass is already running; the request set the re-run flag instead of
    /// starting a second pass (coalesced).
    Coalesced,
}

/// The outcome of finishing a pass for a volume.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FinishOutcome {
    /// No re-run was requested while the pass ran; the volume is now idle.
    Done,
    /// A re-run was requested during the pass; the caller should run once more.
    RunAgain,
}

/// The coalescing core: guarantees one pass per volume at a time, folding
/// concurrent requests into a single re-run. Pure and lock-guarded; no async, no
/// I/O — so the "sweep + concurrent ScanCompleted ⇒ one pass" contract is
/// unit-testable without an app or a runtime.
#[derive(Default)]
pub(crate) struct PassCoordinator {
    slots: Mutex<HashMap<String, PassSlot>>,
}

impl PassCoordinator {
    fn new() -> Self {
        Self::default()
    }

    /// Request a pass for `volume_id`. Returns [`BeginOutcome::Start`] exactly
    /// when the caller should begin a pass; a request that arrives while a pass
    /// runs returns [`BeginOutcome::Coalesced`] and sets the re-run flag.
    pub(crate) fn request(&self, volume_id: &str) -> BeginOutcome {
        let mut slots = self.slots.lock_ignore_poison();
        let slot = slots.entry(volume_id.to_string()).or_default();
        if slot.running {
            slot.rerun_requested = true;
            BeginOutcome::Coalesced
        } else {
            slot.running = true;
            slot.rerun_requested = false;
            BeginOutcome::Start
        }
    }

    /// Finish the running pass for `volume_id`. If a re-run was requested while it
    /// ran, clears the flag and keeps the slot running (returns
    /// [`FinishOutcome::RunAgain`]); otherwise clears running (returns
    /// [`FinishOutcome::Done`]).
    pub(crate) fn finish(&self, volume_id: &str) -> FinishOutcome {
        let mut slots = self.slots.lock_ignore_poison();
        let slot = slots.entry(volume_id.to_string()).or_default();
        if slot.rerun_requested {
            slot.rerun_requested = false;
            // Stays running: the caller loops into another pass.
            FinishOutcome::RunAgain
        } else {
            slot.running = false;
            FinishOutcome::Done
        }
    }
}

// ── Recompute (full-volume) ───────────────────────────────────────────────

/// A folder discovered while walking the index, carrying everything the signal
/// assembler needs. Built by [`walk_index_folders`].
struct IndexFolder {
    entry: EntryRow,
    path: String,
    children: Vec<EntryRow>,
    has_marker_below: bool,
}

/// Walk every directory in a volume's index, newest-listed first is irrelevant —
/// order doesn't matter for a full rescore. For each directory, collect its row,
/// reconstructed path, direct children, and whether a project marker sits below
/// it. Reads through the given index connection (obtained from the read pool).
///
/// Bounded by the number of directories (walks the `entries` table, not the
/// filesystem). A `has_marker_below` is computed by a single upward propagation
/// after the walk, so a `.git` deep in a tree raises its ancestors.
fn walk_index_folders(conn: &rusqlite::Connection, home: &str) -> Result<Vec<IndexFolder>, String> {
    let _ = home; // classification uses it later, at assembly; kept for signature symmetry.
    // Collect directory ids by BFS from the root sentinel.
    let mut folders: Vec<IndexFolder> = Vec::new();
    let mut queue: Vec<i64> = vec![ROOT_ID];
    // Track which dir ids have a marker directly in them, to propagate to ancestors.
    let mut dir_id_to_index: HashMap<i64, usize> = HashMap::new();
    let mut parent_of: HashMap<i64, i64> = HashMap::new();

    while let Some(dir_id) = queue.pop() {
        let children = IndexStore::list_children_on(dir_id, conn).map_err(|e| e.to_string())?;
        // Enqueue child directories.
        for child in &children {
            if child.is_directory {
                queue.push(child.id);
                parent_of.insert(child.id, dir_id);
            }
        }
        // The root sentinel (id 1, empty name) is not a real folder to score.
        if dir_id == ROOT_ID {
            continue;
        }
        let entry = match IndexStore::get_entry_by_id(conn, dir_id).map_err(|e| e.to_string())? {
            Some(e) => e,
            None => continue,
        };
        let path = IndexStore::reconstruct_path(conn, dir_id).map_err(|e| e.to_string())?;
        dir_id_to_index.insert(dir_id, folders.len());
        folders.push(IndexFolder {
            entry,
            path,
            children,
            has_marker_below: false,
        });
    }

    // Propagate a direct project marker up to every ancestor: a `.git` deep in a
    // subtree marks the whole path above it as project-adjacent (plan Decision 3).
    // We seed from each folder's own direct-marker check, then walk parents.
    let marker_seed: Vec<i64> = folders
        .iter()
        .filter(|f| {
            f.children
                .iter()
                .any(|c| super::classify::is_project_marker(&c.name.to_lowercase()))
        })
        .map(|f| f.entry.id)
        .collect();
    for seed in marker_seed {
        let mut cursor = parent_of.get(&seed).copied();
        while let Some(pid) = cursor {
            if let Some(&idx) = dir_id_to_index.get(&pid) {
                folders[idx].has_marker_below = true;
            }
            cursor = parent_of.get(&pid).copied();
        }
    }

    Ok(folders)
}

/// Score every folder in `folders` and return the weight rows to persist.
///
/// Pure over the walked folders + the optional-signal lookups: given a function
/// that resolves a folder's visit count and last-used timestamp (from
/// `importance.db` + Spotlight sampling), it assembles each `FolderSignals`, runs
/// the scorer, and produces a `WeightRow`. Split out so a test can drive it with
/// synthetic folders and no index.
fn score_folders(
    folders: &[IndexFolder],
    home: &str,
    weights: &Weights,
    available: &SignalSet,
    now_secs: u64,
    mut optional_for: impl FnMut(&str) -> OptionalSignals,
) -> Vec<WeightRow> {
    folders
        .iter()
        .map(|f| {
            let optional = optional_for(&f.path);
            let signals = signals_for_dir(&f.entry, &f.children, &f.path, home, f.has_marker_below, optional);
            let s = score(&signals, available, weights, now_secs);
            let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
            WeightRow {
                path: f.path.clone(),
                score: s.value(),
                signals_json,
            }
        })
        .collect()
}

// ── The scheduler handle ──────────────────────────────────────────────────

/// The importance scheduler. Holds the coalescing coordinator, the default
/// weights, and the app data dir (for resolving each volume's `importance.db`).
/// Cloneable-by-`Arc` for use across the bus-listener tasks.
pub struct ImportanceScheduler {
    coordinator: PassCoordinator,
    weights: Weights,
    data_dir: PathBuf,
}

impl ImportanceScheduler {
    /// The user's home directory for path classification. Resolved once; a `None`
    /// falls back to a harmless empty string (every path then classifies
    /// `Neutral`, which is safe — it just doesn't apply the home-relative priors).
    fn home_dir() -> String {
        std::env::var("HOME").unwrap_or_default()
    }
}

/// The inputs to a full-volume recompute pass, bundled so the pass signature
/// stays readable (and under clippy's argument cap). Borrowed for the pass's
/// lifetime; nothing is retained.
struct RecomputeInputs<'a> {
    volume_id: &'a str,
    pool: &'a crate::indexing::ReadPool,
    data_dir: &'a std::path::Path,
    weights: &'a Weights,
    home: &'a str,
    now_secs: u64,
    /// The signal-availability mask for the volume kind: `SignalSet::all()` for a
    /// local macOS volume (both optional signals producible), `listing_only()`
    /// where Spotlight is absent.
    available: SignalSet,
    /// Per-folder navigation-visit counts (from `importance.db`).
    visits: &'a HashMap<String, u32>,
    /// Per-folder sampled `kMDItemLastUsedDate` seconds (macOS-local).
    last_used: &'a HashMap<String, u64>,
}

/// Run a full-volume recompute reading through the index read pool and writing to
/// `data_dir`'s `importance-{volume_id}.db`. Returns the number of folders scored.
///
/// Split from the volume-id resolution so a test drives it with a directly-built
/// [`ReadPool`] over a synthetic index (no registry, no FFI). Weights are stamped
/// at a freshly-bumped generation so every row carries the pass's as-of marker
/// (plan Decision 2/5).
fn recompute_from_pool(inputs: &RecomputeInputs<'_>) -> Result<usize, String> {
    let folders = inputs
        .pool
        .with_conn(|conn| walk_index_folders(conn, inputs.home))
        .map_err(|e| format!("read pool error: {e}"))??;

    if folders.is_empty() {
        return Ok(0);
    }

    let rows = score_folders(
        &folders,
        inputs.home,
        inputs.weights,
        &inputs.available,
        inputs.now_secs,
        |path| OptionalSignals {
            visit_count: inputs.visits.get(path).copied(),
            last_used_secs: inputs.last_used.get(path).copied(),
        },
    );
    let count = rows.len();

    // Advance the generation and write the whole volume in one pass. Open the
    // store to read the current generation (creating the schema if absent), then
    // write at generation+1 through the single-writer thread.
    let db_path = importance_db_path(inputs.data_dir, inputs.volume_id);
    let store = ImportanceStore::open(&db_path).map_err(|e| e.to_string())?;
    let generation = store.recompute_generation().map_err(|e| e.to_string())? + 1;
    drop(store);

    let writer = ImportanceWriter::spawn(&db_path).map_err(|e| e.to_string())?;
    writer.write_weights(generation, rows).map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    writer.shutdown();

    Ok(count)
}

/// Read the visit table into a path→count map for the recompute pass. A missing
/// or unopenable DB yields an empty map (the visit signal is absent, not an
/// error).
fn load_visits(data_dir: &std::path::Path, volume_id: &str) -> HashMap<String, u32> {
    let db_path = importance_db_path(data_dir, volume_id);
    let mut out = HashMap::new();
    if let Ok(conn) = super::store::open_read_connection(&db_path)
        && let Ok(mut stmt) = conn.prepare("SELECT path, visit_count FROM visits")
        && let Ok(rows) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as u32)))
    {
        for row in rows.flatten() {
            out.insert(row.0, row.1);
        }
    }
    out
}

#[cfg(test)]
mod tests;

impl ImportanceScheduler {
    /// Construct a scheduler with the default weights and the app's data dir.
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            coordinator: PassCoordinator::new(),
            weights: Weights::default(),
            data_dir,
        }
    }

    /// Run one full recompute pass for a volume synchronously (blocking).
    ///
    /// Resolves the volume's index read pool (a `None` — the index isn't
    /// registered — makes this a no-op returning `Ok(0)`, the same skip-on-`None`
    /// discipline as enrichment), loads the visit signal from `importance.db`, and
    /// samples `kMDItemLastUsedDate` for the local case. The async driver calls
    /// this on a blocking task after a `request` returns `Start`.
    pub fn run_pass_blocking(&self, volume_id: &str, now_secs: u64) -> Result<usize, String> {
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };
        let home = Self::home_dir();
        let visits = load_visits(&self.data_dir, volume_id);

        // The local macOS volume can produce both optional signals. The
        // `kMDItemLastUsedDate` sample is capped and runs on a dedicated OS thread
        // (never rayon — a macOS framework call). We must know the folder paths to
        // sample, so we walk once for the path set, sample, then recompute. On
        // non-macOS the sample map is empty and Spotlight is unavailable.
        let (available, last_used) = self.sample_last_used(&pool, &home);

        recompute_from_pool(&RecomputeInputs {
            volume_id,
            pool: &pool,
            data_dir: &self.data_dir,
            weights: &self.weights,
            home: &home,
            now_secs,
            available,
            visits: &visits,
            last_used: &last_used,
        })
    }

    /// Resolve the availability mask and the `kMDItemLastUsedDate` sample for a
    /// pass. Where Spotlight is available (macOS local), it's `SignalSet::all()`
    /// and a capped sample over the volume's folders (on a dedicated OS thread,
    /// inside `last_used`); elsewhere the sample is empty and `last_used` is
    /// unavailable so its weight redistributes. Platform-agnostic here — the
    /// platform split lives in `last_used`.
    fn sample_last_used(&self, pool: &crate::indexing::ReadPool, home: &str) -> (SignalSet, HashMap<String, u64>) {
        // Gather the folder paths to sample. On a platform without Spotlight the
        // sampler returns an empty map regardless, but computing the path list is
        // cheap and keeps this branch-free across platforms.
        let paths = pool
            .with_conn(|conn| walk_index_folders(conn, home))
            .ok()
            .and_then(Result::ok)
            .map(|folders| folders.into_iter().map(|f| f.path).collect::<Vec<_>>())
            .unwrap_or_default();
        let sample = super::last_used::sample_last_used(&paths);
        (
            SignalSet {
                visit_available: true,
                last_used_available: super::last_used::is_available(),
            },
            sample,
        )
    }
}

/// Wire the scheduler to the app: sweep the registry for ready volumes and
/// subscribe to the bus, running a coalesced recompute per volume on the tokio
/// runtime. Called from `setup()` after `indexing::init`.
///
/// Local volumes only in M2 (SMB is M4): the sweep and the per-volume bus
/// subscription both gate on the volume being the local `root` for now.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "importance", "importance scheduler not started: {e}");
            return;
        }
    };
    let scheduler = Arc::new(ImportanceScheduler::new(data_dir));

    // Startup sweep: any volume already Fresh at launch (loaded from its
    // persisted scan_completed_at) never re-fires ScanCompleted, so catch it here.
    // M2: local `root` only.
    let ready = crate::indexing::ready_volume_ids();
    for volume_id in ready {
        if volume_id != ROOT_VOLUME_ID {
            continue; // SMB/MTP scored in M4.
        }
        spawn_recompute(Arc::clone(&scheduler), volume_id);
    }

    // Subscribe to the bus for the local volume and recompute on each completion.
    // A subscription created here retains the last state, so a ScanCompleted fired
    // during setup (before this line) is still observed (late-subscriber replay).
    let sub_scheduler = Arc::clone(&scheduler);
    let mut rx = crate::indexing::lifecycle_bus::subscribe(ROOT_VOLUME_ID);
    tauri::async_runtime::spawn(async move {
        // Observe the retained value first (covers a completion before subscribe).
        if matches!(
            *rx.borrow_and_update(),
            crate::indexing::lifecycle_bus::ScanState::Completed { .. }
        ) {
            spawn_recompute(Arc::clone(&sub_scheduler), ROOT_VOLUME_ID.to_string());
        }
        while rx.changed().await.is_ok() {
            if matches!(
                *rx.borrow_and_update(),
                crate::indexing::lifecycle_bus::ScanState::Completed { .. }
            ) {
                spawn_recompute(Arc::clone(&sub_scheduler), ROOT_VOLUME_ID.to_string());
            }
        }
    });
}

/// Request a coalesced recompute for a volume and, if this request starts the
/// pass, drive it (plus any coalesced re-run) on a blocking background task.
fn spawn_recompute(scheduler: Arc<ImportanceScheduler>, volume_id: String) {
    if scheduler.coordinator.request(&volume_id) == BeginOutcome::Coalesced {
        // A pass is already running for this volume; it will re-run once when it
        // finishes (the coordinator set the flag). Nothing to spawn.
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            // Recompute is blocking (SQLite + scoring); run it off the async
            // worker so it never parks the runtime, and never on the IPC thread.
            let result = tauri::async_runtime::spawn_blocking(move || {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                sched.run_pass_blocking(&vid, now)
            })
            .await;
            match result {
                Ok(Ok(count)) => log::debug!(
                    target: "importance",
                    "recompute of '{volume_id}' scored {}",
                    crate::pluralize::pluralize(count as u64, "folder")
                ),
                Ok(Err(e)) => log::warn!(target: "importance", "recompute of '{volume_id}' failed: {e}"),
                Err(e) => log::warn!(target: "importance", "recompute task for '{volume_id}' panicked: {e}"),
            }
            if scheduler.coordinator.finish(&volume_id) == FinishOutcome::Done {
                break;
            }
            // RunAgain: a request arrived mid-pass; loop once more.
        }
    });
}
