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
//! 2. **The startup registry sweep** ([`crate::indexing::ready_volumes_with_kind`]):
//!    a volume already `Fresh` at launch (loaded from its persisted
//!    `scan_completed_at`) never re-fires a `ScanCompleted`, so a bus-only
//!    scheduler would never score it — the common restart case. The sweep
//!    enqueues those once at startup, each with its typed kind (so MTP is
//!    excluded and SMB degrades correctly).
//! 3. **The registration bus** ([`crate::indexing::lifecycle_bus::subscribe_registrations`]):
//!    a volume that registers AFTER the sweep (a share mounted mid-session) is
//!    wired then (plan M4 late-registering volumes).
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

use tauri::{AppHandle, Manager};

use super::scorer::{SignalSet, Weights, score};
use super::signals::{ChildAggregate, OptionalSignals, signals_for_dir};
use super::store::importance_db_path;
use super::writer::{ImportanceWriter, WeightRow};
use crate::ignore_poison::IgnorePoison;
use crate::indexing::IndexVolumeKind;
use crate::indexing::store::{EntryRow, IndexStore, ROOT_ID};

// ── Volume kind → scoring policy (plan M4, typed, never string-matched) ────

/// How the importance scheduler treats a volume, decided by its typed
/// [`IndexVolumeKind`] — never by inspecting the volume-id string (`no-string-matching`).
///
/// - **Local** and **SMB** are background-scored. They differ only in signal
///   availability: SMB has no Spotlight, so `last_used` is UNAVAILABLE there and
///   its weight redistributes (the scorer's `SignalSet` handles this since M1);
///   local macOS produces both optional signals.
/// - **MTP is an explicit exclusion, not an accident of gating** (plan / agent
///   spec): a phone/camera is on-demand only, never background-scored. The scheduler
///   skips it at every entry point (sweep, registration, bus subscription).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScoringPolicy {
    /// Background-scored, with the given signal-availability mask for the kind.
    Scored { available: SignalSet },
    /// Never background-scored (MTP: on-demand only).
    Excluded,
}

/// Whether a volume of this kind is background-scored (Local/SMB), as opposed to
/// on-demand only (MTP). The `record_visit` command uses it to skip persisting a
/// visit for a volume that's never scored — typed, never a volume-id string check.
pub(super) fn is_background_scored(kind: IndexVolumeKind) -> bool {
    matches!(ScoringPolicy::for_kind(kind), ScoringPolicy::Scored { .. })
}

impl ScoringPolicy {
    /// The scoring policy for a volume kind. The availability mask degrades
    /// explicitly per kind — SMB drops Spotlight — so a missing signal
    /// redistributes rather than fabricating (plan Decision 3).
    fn for_kind(kind: IndexVolumeKind) -> Self {
        match kind {
            // Local macOS produces both optional signals (visits + Spotlight where
            // the OS supports it; off macOS Spotlight is simply absent).
            IndexVolumeKind::Local => ScoringPolicy::Scored {
                available: SignalSet {
                    visit_available: true,
                    last_used_available: super::last_used::is_available(),
                },
            },
            // SMB has NO Spotlight metadata: `last_used` is unavailable and its
            // weight redistributes onto the listing signals. Visits still apply
            // (they come from Cmdr navigation, not the mount).
            IndexVolumeKind::Smb => ScoringPolicy::Scored {
                available: SignalSet {
                    visit_available: true,
                    last_used_available: false,
                },
            },
            // MTP: on-demand only, never background-scored.
            IndexVolumeKind::Mtp => ScoringPolicy::Excluded,
        }
    }
}

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
/// assembler needs. Built by [`walk_index_folders`]. Holds its children's
/// pre-aggregated summary ([`ChildAggregate`]), NOT the child rows — the walk
/// folds each file into this so no file rows stay resident (the O(dirs) memory
/// shape).
struct IndexFolder {
    entry: EntryRow,
    path: String,
    children: ChildAggregate,
    has_marker_below: bool,
}

/// Walk every directory in a volume's index and build each folder's row,
/// reconstructed path, aggregated child summary, and marker-below flag —
/// materializing DIRECTORIES only, not the whole entries table.
///
/// The memory shape matters: on a multi-million-entry NAS the directories are a
/// small fraction of the rows, so this pulls only them into memory
/// ([`all_directories`](IndexStore::all_directories)) and STREAMS file rows
/// ([`for_each_file_child`](IndexStore::for_each_file_child)) into small per-parent
/// accumulators (extension set, file count, direct-marker flag), which are then
/// collapsed to a [`ChildAggregate`] per folder. So pass memory is O(dirs), not
/// O(entries) — the earlier `all_entries` walk went transiently into the hundreds
/// of MB on exactly the NAS-sized volumes SMB scoring now enables.
///
/// Directory children still come from the directory set itself (a `.git`/`.hg`
/// marker is a directory), so the direct-marker flag folds both the streamed file
/// children and the sibling directory children. Paths are reconstructed from the
/// in-memory `id → (parent_id, name)` directory map (no per-directory point
/// query). `has_marker_below` is a single upward propagation after the walk, so a
/// `.git` deep in a tree raises its ancestors (plan Decision 3).
fn walk_index_folders(conn: &rusqlite::Connection, _home: &str) -> Result<Vec<IndexFolder>, String> {
    let dirs = IndexStore::all_directories(conn).map_err(|e| e.to_string())?;

    // Index the directory rows: a lookup for path reconstruction, keyed by id.
    let by_id: HashMap<i64, &EntryRow> = dirs.iter().map(|e| (e.id, e)).collect();

    // Per-directory accumulator, folded from the streamed file children plus the
    // sibling directory children. Kept tiny (a small extension set + two scalars)
    // so the map is O(dirs), never O(files).
    #[derive(Default)]
    struct Accum {
        extensions: std::collections::HashSet<String>,
        file_count: u32,
        has_direct_marker: bool,
    }
    let mut accum: HashMap<i64, Accum> = HashMap::new();

    // Directory children first: a `.git`/`.hg`/`.svn` marker is a DIRECTORY, so
    // fold the directory set into each parent's direct-marker flag. (Directories
    // never contribute to the extension count or file count.)
    for d in dirs.iter().filter(|e| e.id != ROOT_ID) {
        if super::classify::is_project_marker(&d.name.to_lowercase()) {
            accum.entry(d.parent_id).or_default().has_direct_marker = true;
        }
    }

    // File children streamed one row at a time: fold each into its parent's
    // extension set, file count, and (for a `Cargo.toml`/`package.json`/… file)
    // marker flag. The file rows are never all resident.
    IndexStore::for_each_file_child(conn, |parent_id, name| {
        let entry = accum.entry(parent_id).or_default();
        entry.file_count += 1;
        let ext = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        entry.extensions.insert(ext);
        if super::classify::is_project_marker(&name.to_lowercase()) {
            entry.has_direct_marker = true;
        }
    })
    .map_err(|e| e.to_string())?;

    // One folder per directory entry (the root sentinel isn't a real folder).
    let mut folders: Vec<IndexFolder> = Vec::new();
    let mut dir_id_to_index: HashMap<i64, usize> = HashMap::new();
    for entry in dirs.iter().filter(|e| e.id != ROOT_ID) {
        let path = reconstruct_path_from_map(entry.id, &by_id);
        let a = accum.remove(&entry.id).unwrap_or_default();
        dir_id_to_index.insert(entry.id, folders.len());
        folders.push(IndexFolder {
            entry: entry.clone(),
            path,
            children: ChildAggregate {
                distinct_extension_count: a.extensions.len() as u32,
                file_count: a.file_count,
                has_direct_marker: a.has_direct_marker,
            },
            has_marker_below: false,
        });
    }

    // Propagate a direct project marker up to every ancestor: a `.git` deep in a
    // subtree marks the whole path above it as project-adjacent (plan Decision 3).
    // Seed from each folder's own direct-marker flag, then walk parent pointers.
    let marker_seed: Vec<i64> = folders
        .iter()
        .filter(|f| f.children.has_direct_marker)
        .map(|f| f.entry.id)
        .collect();
    for seed in marker_seed {
        let mut cursor = by_id.get(&seed).map(|e| e.parent_id);
        while let Some(pid) = cursor {
            if pid == ROOT_ID {
                break;
            }
            if let Some(&idx) = dir_id_to_index.get(&pid) {
                folders[idx].has_marker_below = true;
            }
            cursor = by_id.get(&pid).map(|e| e.parent_id);
        }
    }

    Ok(folders)
}

/// Reconstruct an entry's absolute path from an in-memory `id → row` map, walking
/// parent pointers up to the root sentinel. The in-memory twin of the store's
/// `reconstruct_path` point query — used because a full recompute reconstructs
/// every folder's path and the per-query cost would be O(dirs × depth). The map
/// holds only directory rows, which is all a path walk (dir → dir → …) needs.
fn reconstruct_path_from_map(id: i64, by_id: &HashMap<i64, &EntryRow>) -> String {
    let mut components: Vec<&str> = Vec::new();
    let mut cursor = Some(id);
    while let Some(cid) = cursor {
        if cid == ROOT_ID {
            break;
        }
        let Some(entry) = by_id.get(&cid) else { break };
        components.push(&entry.name);
        cursor = Some(entry.parent_id);
    }
    components.reverse();
    format!("/{}", components.join("/"))
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
            let signals = signals_for_dir(&f.entry, f.children, &f.path, home, f.has_marker_below, optional);
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
/// weights, the app data dir (for resolving each volume's `importance.db`), and
/// the long-lived per-volume writer registry. Cloneable-by-`Arc` for use across
/// the bus-listener tasks and as Tauri managed state (so `record_visit` reaches
/// the shared writer).
pub struct ImportanceScheduler {
    coordinator: PassCoordinator,
    weights: Weights,
    data_dir: PathBuf,
    writers: super::writer_registry::WriterRegistry,
    /// Per-volume accumulator of changed paths awaiting an incremental rescore. A
    /// burst of dir-changed batches coalesces here so overlapping passes drain one
    /// combined set, not one pass per batch.
    pending_incremental: Mutex<HashMap<String, std::collections::HashSet<String>>>,
}

impl ImportanceScheduler {
    /// The user's home directory for path classification. Resolved once; a `None`
    /// falls back to a harmless empty string (every path then classifies
    /// `Neutral`, which is safe — it just doesn't apply the home-relative priors).
    fn home_dir() -> String {
        std::env::var("HOME").unwrap_or_default()
    }

    /// The shared writer for a volume (long-lived, one thread per DB). Both the
    /// recompute pass and `record_visit` route writes through this.
    pub fn writer_for(&self, volume_id: &str) -> Result<ImportanceWriter, super::store::ImportanceStoreError> {
        self.writers.writer_for(&self.data_dir, volume_id)
    }

    /// The app data dir this scheduler resolves `importance.db` paths under.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }
}

/// The inputs to a full-volume recompute pass, bundled so the pass signature
/// stays readable (and under clippy's argument cap). Borrowed for the pass's
/// lifetime; nothing is retained.
struct RecomputeInputs<'a> {
    /// The shared long-lived writer for this volume's `importance.db` (one writer
    /// thread per DB). Reads the current generation and writes the pass through it.
    writer: &'a ImportanceWriter,
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

/// Run a full-volume recompute over the already-walked `folders`, writing to
/// `data_dir`'s `importance-{volume_id}.db`. Returns the number of folders scored.
///
/// Takes the walked folders (not the pool) so the caller walks the index ONCE and
/// reuses that walk for both the `kMDItemLastUsedDate` path-set and the score —
/// no second traversal. Split from the volume-id resolution so a test drives it
/// with a synthetic walk (no registry, no FFI). Weights are stamped at a
/// freshly-bumped generation so every row carries the pass's as-of marker (plan
/// Decision 2/5).
fn recompute_folders(inputs: &RecomputeInputs<'_>, folders: &[IndexFolder]) -> Result<RecomputeOutcome, String> {
    if folders.is_empty() {
        return Ok(RecomputeOutcome {
            count: 0,
            generation: 0,
        });
    }

    let rows = score_folders(
        folders,
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

    let writer = inputs.writer;
    let generation = writer.next_generation().map_err(|e| e.to_string())?;
    writer.write_weights(generation, rows).map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;

    Ok(RecomputeOutcome { count, generation })
}

/// The result of a recompute pass: how many folders were scored and the
/// generation the pass wrote at (the as-of marker consumers see; the recompute
/// subscription fires with it).
struct RecomputeOutcome {
    count: usize,
    generation: u64,
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
            writers: super::writer_registry::WriterRegistry::new(),
            pending_incremental: Mutex::new(HashMap::new()),
        }
    }

    /// Accumulate `paths` into the volume's pending incremental set (union).
    fn pending_incremental_paths(&self, volume_id: &str, paths: Vec<String>) {
        let mut pending = self.pending_incremental.lock_ignore_poison();
        let set = pending.entry(volume_id.to_string()).or_default();
        set.extend(paths);
    }

    /// Drain and return the volume's pending incremental paths (empties the set).
    fn take_incremental_paths(&self, volume_id: &str) -> Vec<String> {
        let mut pending = self.pending_incremental.lock_ignore_poison();
        match pending.get_mut(volume_id) {
            Some(set) => set.drain().collect(),
            None => Vec::new(),
        }
    }

    /// Run one full recompute pass for a volume synchronously (blocking).
    ///
    /// Resolves the volume's index read pool (a `None` — the index isn't
    /// registered — makes this a no-op returning `Ok(0)`, the same skip-on-`None`
    /// discipline as enrichment), walks the index ONCE, loads the visit signal
    /// from `importance.db`, samples `kMDItemLastUsedDate` for the local case over
    /// that one walk's paths, and writes through the shared long-lived writer. The
    /// async driver calls this on a blocking task after a `request` returns
    /// `Start`.
    pub fn run_pass_blocking(&self, volume_id: &str, available: SignalSet, now_secs: u64) -> Result<usize, String> {
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };
        let home = Self::home_dir();

        // Walk the index ONCE; reuse the result for both the `kMDItemLastUsedDate`
        // path-set and the score (no second traversal — M2 cleanup).
        let folders = pool
            .with_conn(|conn| walk_index_folders(conn, &home))
            .map_err(|e| format!("read pool error: {e}"))??;
        if folders.is_empty() {
            return Ok(0);
        }

        let visits = load_visits(&self.data_dir, volume_id);

        // Spotlight sampling ONLY when the kind's availability mask says so — SMB
        // has no Spotlight, and sampling would issue `MDItem` queries against the
        // mount, which the scheduler must never do (it reads only the local index).
        // The sample is capped and runs on a dedicated OS thread (never rayon — a
        // macOS framework call). When unavailable, the map is empty and the
        // `last_used` weight redistributes.
        let last_used = if available.last_used_available {
            let paths: Vec<String> = folders.iter().map(|f| f.path.clone()).collect();
            super::last_used::sample_last_used(&paths)
        } else {
            HashMap::new()
        };

        let writer = self.writer_for(volume_id).map_err(|e| e.to_string())?;
        let outcome = recompute_folders(
            &RecomputeInputs {
                writer: &writer,
                weights: &self.weights,
                home: &home,
                now_secs,
                available,
                visits: &visits,
                last_used: &last_used,
            },
            &folders,
        )?;

        // Announce the completed full pass so a read-API consumer reacts instead
        // of polling (plan Decision 6, subscribe-don't-poll).
        super::read::notify_recompute_completed(volume_id, outcome.generation);
        Ok(outcome.count)
    }

    /// Run one INCREMENTAL rescore for a volume: rescore only the folders whose
    /// listings changed (`changed_paths`) plus their capped ancestor chains, and
    /// upsert those rows WITHOUT advancing the generation (untouched folders keep
    /// their as-of marker — plan Decision 5). Returns the number of folders
    /// rescored.
    ///
    /// A `"/"` sentinel in `changed_paths` (a full-refresh emit) escalates to a
    /// full pass. Reads through the index read pool; a `None` pool is a no-op.
    pub fn run_incremental_blocking(
        &self,
        volume_id: &str,
        available: SignalSet,
        changed_paths: &[String],
        now_secs: u64,
    ) -> Result<usize, String> {
        // A full-refresh sentinel means "everything changed" — fall back to the
        // full pass rather than resolving `/` as a single folder.
        if changed_paths.iter().any(|p| p == "/") {
            return self.run_pass_blocking(volume_id, available, now_secs);
        }

        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };
        let home = Self::home_dir();

        let folders = pool
            .with_conn(|conn| walk_index_folders(conn, &home))
            .map_err(|e| format!("read pool error: {e}"))??;
        if folders.is_empty() {
            return Ok(0);
        }

        let visits = load_visits(&self.data_dir, volume_id);
        let writer = self.writer_for(volume_id).map_err(|e| e.to_string())?;

        let count = incremental_rescore(
            &IncrementalInputs {
                writer: &writer,
                weights: &self.weights,
                home: &home,
                now_secs,
                available,
                visits: &visits,
            },
            &folders,
            changed_paths,
        )?;

        if count > 0 {
            // The incremental rows carry the current generation (no bump), so the
            // notification announces that generation as freshly touched.
            let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);
            super::read::notify_recompute_completed(volume_id, generation);
        }
        Ok(count)
    }
}

/// The inputs to an incremental rescore, bundled like [`RecomputeInputs`].
struct IncrementalInputs<'a> {
    writer: &'a ImportanceWriter,
    weights: &'a Weights,
    home: &'a str,
    now_secs: u64,
    available: SignalSet,
    visits: &'a HashMap<String, u32>,
}

/// Rescore only the folders whose paths are in the touched set (changed paths +
/// their capped ancestors) and upsert them WITHOUT advancing the generation, so
/// every untouched folder keeps its as-of marker (plan Decision 5). Returns the
/// number of folders rescored.
///
/// Split from the pool/registry resolution so a test drives it with a synthetic
/// walk and a directly-built writer (no registry, no FFI). Samples
/// `kMDItemLastUsedDate` only for the touched subset (bounded work).
fn incremental_rescore(
    inputs: &IncrementalInputs<'_>,
    folders: &[IndexFolder],
    changed_paths: &[String],
) -> Result<usize, String> {
    // The set of folders to rescore: each changed path plus its ancestors up to
    // the cap. Bounding the ancestor walk keeps a deep project marker from
    // rescoping half the volume (plan open-question / Decision 5).
    let touched = touched_folder_set(changed_paths);
    let subset: Vec<&IndexFolder> = folders.iter().filter(|f| touched.contains(&f.path)).collect();
    if subset.is_empty() {
        return Ok(0);
    }

    // Sample Spotlight only when the kind's mask allows it (SMB has none, and
    // sampling would touch the mount). When unavailable the map is empty and the
    // `last_used` weight redistributes.
    let last_used = if inputs.available.last_used_available {
        let subset_paths: Vec<String> = subset.iter().map(|f| f.path.clone()).collect();
        super::last_used::sample_last_used(&subset_paths)
    } else {
        HashMap::new()
    };

    let writer = inputs.writer;
    // The incremental rows carry the CURRENT generation (no bump), so they're
    // as-fresh-as the last full pass and untouched folders don't turn stale.
    let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);

    let rows: Vec<WeightRow> = subset
        .iter()
        .map(|f| {
            let optional = OptionalSignals {
                visit_count: inputs.visits.get(&f.path).copied(),
                last_used_secs: last_used.get(&f.path).copied(),
            };
            let signals = signals_for_dir(&f.entry, f.children, &f.path, inputs.home, f.has_marker_below, optional);
            let s = score(&signals, &inputs.available, inputs.weights, inputs.now_secs);
            let signals_json = serde_json::to_string(&signals).unwrap_or_else(|_| "{}".to_string());
            WeightRow {
                path: f.path.clone(),
                score: s.value(),
                signals_json,
            }
        })
        .collect();
    let count = rows.len();

    writer
        .write_weights_incremental(generation, rows)
        .map_err(|e| e.to_string())?;
    writer.flush_blocking().map_err(|e| e.to_string())?;
    Ok(count)
}

/// The maximum number of ancestor levels an incremental rescore walks up from a
/// changed folder. A project marker (or a size/mtime change) can raise ancestors,
/// but a deep change must not rescope half the volume, so the walk is capped
/// (plan open-question / Decision 5). Generous enough for realistic home trees.
const ANCESTOR_WALK_CAP: usize = 32;

/// Build the set of folder paths an incremental rescore should touch: each changed
/// path plus its ancestors, up to [`ANCESTOR_WALK_CAP`] levels each. Pure string
/// math over absolute paths, so it's unit-testable without an index.
fn touched_folder_set(changed_paths: &[String]) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    for path in changed_paths {
        set.insert(path.clone());
        let mut current = path.as_str();
        for _ in 0..ANCESTOR_WALK_CAP {
            let Some(pos) = current.rfind('/') else { break };
            if pos == 0 {
                break; // reached the root `/`; don't add the bare root as a folder.
            }
            let parent = &current[..pos];
            set.insert(parent.to_string());
            current = parent;
        }
    }
    set
}

/// Wire the scheduler to the app: subscribe to volume registrations, sweep the
/// registry for already-ready volumes, and wire each scored volume's
/// scan-completion + dir-changed subscriptions. Called from `setup()` after
/// `indexing::init`.
///
/// Multi-volume + kind-aware (plan M4): Local and SMB volumes are background-scored
/// (SMB with Spotlight unavailable, so its weight redistributes); MTP is an
/// explicit typed exclusion (on-demand only). The registration bus catches a share
/// mounted MID-SESSION; the startup sweep catches volumes already ready at launch —
/// subscribing to the bus BEFORE the sweep closes the gap so no registration is
/// missed.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "importance", "importance scheduler not started: {e}");
            return;
        }
    };
    let scheduler = Arc::new(ImportanceScheduler::new(data_dir));

    // Make the scheduler reachable from the IPC layer: `record_visit` routes its
    // write through the shared per-volume writer the scheduler owns (one writer
    // thread per DB), rather than spawning a short-lived writer per navigation.
    app.manage(Arc::clone(&scheduler));

    // Subscribe to registrations FIRST (before the sweep), so a volume that
    // registers during the sweep isn't dropped in the gap (plan M4
    // late-registering volumes). Each registration wires that volume's per-volume
    // subscriptions and scores it if it's already ready.
    let reg_scheduler = Arc::clone(&scheduler);
    let mut reg_rx = crate::indexing::lifecycle_bus::subscribe_registrations();
    tauri::async_runtime::spawn(async move {
        loop {
            match reg_rx.recv().await {
                Ok(reg) => wire_volume(Arc::clone(&reg_scheduler), reg.volume_id, reg.kind),
                // A lag only skips a registration the next scan-completion covers
                // anyway; keep listening. A closed bus (never, it's process-global)
                // ends the task.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Startup sweep: any volume already ready at launch (loaded from its persisted
    // scan_completed_at) never re-fires ScanCompleted, so catch it here — WITH its
    // typed kind so MTP is excluded and SMB degrades correctly.
    for (volume_id, kind) in crate::indexing::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id, kind);
    }
}

/// Wire one volume into the scheduler by its typed kind: skip MTP (on-demand
/// only), and for Local/SMB set up its scan-completion subscription (full
/// recompute) and its dir-changed subscription (incremental rescore), then score
/// it once if it's already ready.
///
/// Idempotent per volume in practice: the coalescing coordinator collapses a
/// re-wire's duplicate recompute into the running one, and the underlying `watch`
/// buses are per-volume, so re-subscribing spawns a second listener but each drives
/// the same coalesced pass. A volume is wired from at most two places (the sweep
/// and one registration), so no unbounded listener growth.
fn wire_volume(scheduler: Arc<ImportanceScheduler>, volume_id: String, kind: IndexVolumeKind) {
    let available = match ScoringPolicy::for_kind(kind) {
        ScoringPolicy::Scored { available } => available,
        // MTP: on-demand only, never background-scored (plan M4 typed exclusion).
        ScoringPolicy::Excluded => {
            log::debug!(target: "importance", "importance skips '{volume_id}' ({kind:?}): on-demand only");
            return;
        }
    };

    // Incremental recompute: rescore only the touched subtrees + capped ancestor
    // chains as live listing changes land (plan Decision 5). Full-volume recompute
    // stays the scan-completion default below.
    start_incremental(Arc::clone(&scheduler), volume_id.clone(), available);

    // Subscribe to the scan bus for this volume; a subscription retains the last
    // state, so a ScanCompleted fired before this line is still observed
    // (late-subscriber replay). Recompute on each completion.
    let sub_scheduler = Arc::clone(&scheduler);
    let sub_volume = volume_id.clone();
    let mut rx = crate::indexing::lifecycle_bus::subscribe(&volume_id);
    tauri::async_runtime::spawn(async move {
        // Observe the retained value first (covers a completion before subscribe,
        // and a sweep-ready volume that already loaded Completed).
        if matches!(
            *rx.borrow_and_update(),
            crate::indexing::lifecycle_bus::ScanState::Completed { .. }
        ) {
            spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available);
        }
        while rx.changed().await.is_ok() {
            if matches!(
                *rx.borrow_and_update(),
                crate::indexing::lifecycle_bus::ScanState::Completed { .. }
            ) {
                spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available);
            }
        }
    });
}

/// Subscribe to a volume's dir-changed bus and run a bounded incremental rescore
/// for each batch of live listing changes (plan Decision 5). Coalesces overlapping
/// batches per volume (accumulating their paths) so a burst of FSEvents collapses
/// to one pass plus at most one re-run, never a pass per event.
fn start_incremental(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
    let mut rx = crate::indexing::lifecycle_bus::subscribe_dirs_changed(&volume_id);
    tauri::async_runtime::spawn(async move {
        // The retained initial value is the empty batch (nothing published yet);
        // `borrow_and_update` marks it seen so the first real change triggers.
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            let paths = rx.borrow_and_update().paths.clone();
            if paths.is_empty() {
                continue;
            }
            spawn_incremental(Arc::clone(&scheduler), volume_id.clone(), available, paths);
        }
    });
}

/// Coalescing key for incremental passes: distinct from the full-pass key so an
/// incremental rescore and a full recompute for the same volume don't block each
/// other in the coordinator (they serialize at the writer thread anyway).
fn incremental_key(volume_id: &str) -> String {
    format!("{volume_id}#incremental")
}

/// Request a coalesced incremental rescore, accumulating `paths` into the pending
/// set. If this request starts the pass, drive it (plus any coalesced re-run,
/// draining whatever accumulated meanwhile) on a blocking background task.
fn spawn_incremental(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet, paths: Vec<String>) {
    let key = incremental_key(&volume_id);
    scheduler.pending_incremental_paths(&volume_id, paths);
    if scheduler.coordinator.request(&key) == BeginOutcome::Coalesced {
        return; // a pass is running; it will drain the accumulated paths on re-run.
    }
    tauri::async_runtime::spawn(async move {
        let key = incremental_key(&volume_id);
        loop {
            let batch = scheduler.take_incremental_paths(&volume_id);
            if !batch.is_empty() {
                let sched = Arc::clone(&scheduler);
                let vid = volume_id.clone();
                let result = tauri::async_runtime::spawn_blocking(move || {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    sched.run_incremental_blocking(&vid, available, &batch, now)
                })
                .await;
                match result {
                    Ok(Ok(count)) => log::debug!(
                        target: "importance",
                        "incremental rescore of '{volume_id}' updated {}",
                        crate::pluralize::pluralize(count as u64, "folder")
                    ),
                    Ok(Err(e)) => log::warn!(target: "importance", "incremental rescore of '{volume_id}' failed: {e}"),
                    Err(e) => log::warn!(target: "importance", "incremental task for '{volume_id}' panicked: {e}"),
                }
            }
            if scheduler.coordinator.finish(&key) == FinishOutcome::Done {
                break;
            }
            // RunAgain: more paths accumulated mid-pass; loop and drain them.
        }
    });
}

/// Request a coalesced recompute for a volume and, if this request starts the
/// pass, drive it (plus any coalesced re-run) on a blocking background task.
fn spawn_recompute(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
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
                sched.run_pass_blocking(&vid, available, now)
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
