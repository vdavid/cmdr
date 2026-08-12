//! The importance scheduler: recompute a volume's folder weights when its index
//! finishes scanning, and once at startup for a volume that loaded ready.
//!
//! ## What drives a recompute
//!
//! Two triggers, unified through one coalescing coordinator:
//!
//! 1. **The lifecycle bus** ([`crate::indexing::lifecycle::lifecycle_bus`]): a
//!    `ScanCompleted` publish for a volume ⇒ recompute it. This catches every
//!    scan that finishes while the app runs.
//! 2. **The startup registry sweep** ([`crate::indexing::lifecycle::state::ready_volumes_with_kind`]):
//!    a volume already `Fresh` at launch (loaded from its persisted
//!    `scan_completed_at`) never re-fires a `ScanCompleted`, so the bus subscription
//!    the sweep WIRES would never score it — the common restart case (its retained
//!    bus value stays `Pending`). To close that, the sweep ALSO runs
//!    [`wiring::enqueue_initial_full_pass_if_unscored`] per ready volume: it forces the
//!    write-path store open (triggering any lazy schema recreate) and enqueues a full
//!    recompute IFF the store then carries no generation. Gating on "no generation"
//!    (not an unconditional kick) means an already-scored volume isn't rescored on
//!    every launch, while a fresh / schema-recreated / incremental-only store finally
//!    gets its full pass. Each carries its typed kind (MTP excluded, SMB degraded).
//! 3. **The registration bus** ([`crate::indexing::lifecycle::lifecycle_bus::subscribe_registrations`]):
//!    a volume that registers AFTER the sweep (a share mounted mid-session) is wired then.
//!
//! ## Coalescing
//!
//! Both triggers can target one volume at once (the sweep sees it Fresh AND a
//! concurrent startup scan completes). [`PassCoordinator`] guarantees ONE pass
//! runs per volume at a time: a request arriving while a pass runs sets a re-run
//! flag rather than starting a second pass. When the running pass finishes, it
//! re-runs once if the flag is set. This is the pure, unit-testable core.
//!
//! ## Recompute
//!
//! Full-volume: read `dir_stats` + the entry tree through the index read pool,
//! assemble a [`FolderSignals`](crate::importance::FolderSignals) per folder (via [`signals`](super::signals)), run
//! the pure scorer, and write every folder's weight through the
//! [`ImportanceWriter`] at a new generation. Cost-bounded by walking the index
//! (already in SQLite), not the filesystem. Runs on a blocking background task
//! through the host runtime seam, never on the IPC thread. Local and SMB
//! volumes; MTP is excluded at every entry point (`ScoringPolicy::for_kind`).
//!
//! **A pass can't be stopped**: no `CancellationToken`, no stop hook, so
//! `stop_all_indexing` (memory watchdog, shutdown) waits it out. Why, and the
//! `TODO` for closing it: `DETAILS.md` § "A pass can't be stopped".

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use super::scorer::{SignalSet, Weights};
use super::writer::ImportanceWriter;
use crate::IndexVolumeKind;
use crate::importance::read::WeightsChanged;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Comparing two walks of the same index, for the measurement tools. Nothing in
/// the app reaches it: the scheduler's own incremental path is `recompute`.
#[cfg(any(test, feature = "tooling"))]
mod differential;
mod recompute;
mod scoped_walk;
mod walk;
/// Construction plus the lifecycle subscriptions that decide when a pass runs.
mod wiring;
use recompute::{
    IncrementalInputs, IncrementalReport, RecomputeInputs, incremental_rescore, load_previous_markers, load_visits,
    recompute_folders, sanitize_incremental_batch, walk_for_incremental,
};
// Re-exported for the eval corpus tool, which walks a real index the SAME way a
// recompute does (so dumped signals match production exactly).
pub(crate) use walk::walk_index_folders;
// The measurement/tuning entry point: walk a real index, score, write an
// `importance.db` — the full-pass core without the registry or async driver.
#[cfg(any(test, feature = "tooling"))]
pub use recompute::{MeasureOutcome, recompute_index_to_db};
// The correctness harness: run the scoped walk and the full walk over the same real
// index and difference the rows they'd write.
#[cfg(any(test, feature = "tooling"))]
pub use differential::{OriginComparison, compare_walks_for_incremental, sample_origins};

// ── Volume kind → scoring policy (typed, never string-matched) ────────────

/// How the importance scheduler treats a volume, decided by its typed
/// [`IndexVolumeKind`] — never by inspecting the volume-id string.
///
/// - **Local** and **SMB** are background-scored. They differ only in signal
///   availability: SMB has no Spotlight, so `last_used` is UNAVAILABLE there and
///   its weight redistributes (the scorer's `SignalSet` handles this);
///   local macOS produces both optional signals.
/// - **MTP is an explicit exclusion, not an accident of gating**: a phone/camera
///   is on-demand only, never background-scored. The scheduler
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
pub fn is_background_scored(kind: IndexVolumeKind) -> bool {
    matches!(ScoringPolicy::for_kind(kind), ScoringPolicy::Scored { .. })
}

/// The scorer's signal-availability mask for a volume kind, or `None` when the kind
/// isn't background-scored (MTP). A read consumer (`cmdr://importance`) opens an
/// [`ImportanceIndex`](super::read::ImportanceIndex) with this so its `explain`
/// redistributes exactly as the recompute that wrote the weights did — otherwise an
/// SMB volume's breakdown (no Spotlight `last_used`) wouldn't sum to the stored
/// score. Single-sources the kind→availability policy that lives in `ScoringPolicy`.
pub fn signal_availability(kind: IndexVolumeKind) -> Option<SignalSet> {
    match ScoringPolicy::for_kind(kind) {
        ScoringPolicy::Scored { available } => Some(available),
        ScoringPolicy::Excluded => None,
    }
}

impl ScoringPolicy {
    /// The scoring policy for a volume kind. The availability mask degrades
    /// explicitly per kind — SMB drops Spotlight — so a missing signal
    /// redistributes rather than fabricating.
    fn for_kind(kind: IndexVolumeKind) -> Self {
        match kind {
            // Local macOS produces both optional signals (visits + Spotlight where
            // the OS supports it; off macOS Spotlight is simply absent). A local
            // external drive shares the local scan path and reads its own local DB,
            // so it scores identically; a per-folder missing signal redistributes.
            IndexVolumeKind::Local | IndexVolumeKind::LocalExternal => ScoringPolicy::Scored {
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

    /// Start the folder-importance scheduler and hand the host the value to hold:
    /// subscribes to volume registrations, sweeps the index registry for already-ready
    /// volumes, and wires each scored volume's scan-completion + dir-changed
    /// subscriptions. Local and SMB volumes are background-scored (SMB without
    /// Spotlight, so its weight redistributes); MTP is an explicit typed exclusion,
    /// on-demand only.
    ///
    /// `None` when there's no data dir to run under, which is a host that hasn't
    /// applied its config yet. Never registers itself anywhere: the host owns the
    /// returned value, and `record_visit` resolves it from there.
    pub fn start() -> Option<Arc<Self>> {
        wiring::build_and_wire()
    }
}

#[cfg(test)]
mod coalescing_tests;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod incremental_transition_tests;
#[cfg(test)]
mod multi_volume_tests;
#[cfg(test)]
mod recompute_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod walk_memory_tests;

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
    pub(crate) fn run_pass_blocking(
        &self,
        volume_id: &str,
        available: SignalSet,
        now_secs: u64,
    ) -> Result<usize, String> {
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };
        let home = Self::home_dir();

        // Time the two phases so real numbers surface any drift (a full pass on a
        // NAS-sized volume is the cost to watch): the READ phase (walk + visit load
        // + Spotlight sampling) vs the SCORE+WRITE phase (`recompute_folders`).
        let read_started = Instant::now();

        // Walk the index ONCE; reuse the result for both the `kMDItemLastUsedDate`
        // path-set and the score (no second traversal — M2 cleanup).
        let mut folders = pool
            .with_conn(|conn| walk_index_folders(conn, &home))
            .map_err(|e| format!("read pool error: {e}"))??;
        if folders.is_empty() {
            return Ok(0);
        }
        let folders_walked = folders.len();

        let visits = load_visits(&self.data_dir, volume_id);

        // Spotlight sampling ONLY when the kind's availability mask says so — SMB
        // has no Spotlight, and sampling would issue `MDItem` queries against the
        // mount, which the scheduler must never do (it reads only the local index).
        // The sample is capped and runs on a dedicated OS thread (never rayon — a
        // macOS framework call). When unavailable, the map is empty and the
        // `last_used` weight redistributes.
        // Only as many paths as the sample can use: it queries the first `SAMPLE_CAP`
        // and drops the rest, so materializing the whole volume's paths to hand over
        // 500 of them would cost one heap `String` per folder for nothing.
        let last_used = if available.last_used_available {
            let paths = folders.first_paths(super::last_used::SAMPLE_CAP);
            super::last_used::sample_last_used(&paths)
        } else {
            HashMap::new()
        };
        let read_elapsed = read_started.elapsed();

        let write_started = Instant::now();
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
            &mut folders,
        )?;
        let write_elapsed = write_started.elapsed();

        // One info line so a full pass's cost (and any regression) is visible in the
        // logs — the walk-dominated read phase vs the score+write phase.
        log::info!(
            target: "importance",
            "recompute of '{volume_id}' scored {} of {} folders in {:.2?} (walk+sample {:.2?}, score+write {:.2?}); floored folders omitted",
            outcome.count,
            folders_walked,
            read_elapsed + write_elapsed,
            read_elapsed,
            write_elapsed,
        );

        // Announce the pass so a read-API consumer reacts instead of polling. A full
        // pass REPLACES the whole table, so a cached-weight consumer must rebuild;
        // ❌ never a delta here, materializing every row's path is what it avoids.
        let generation = outcome.generation;
        super::read::notify_recompute_completed(volume_id, WeightsChanged::ReloadAll { generation });
        Ok(outcome.count)
    }

    /// Run one INCREMENTAL rescore for a volume: rescore only the folders whose
    /// listings changed (`changed_paths`) plus their capped ancestor chains, and
    /// write back only the rows whose signals moved, WITHOUT advancing the generation
    /// (untouched folders keep their as-of marker). Returns the pass's
    /// [`IncrementalReport`]: how many folders it rescored and how many it wrote.
    ///
    /// A `"/"` sentinel in `changed_paths` (a full-refresh emit) escalates to a
    /// full pass. Reads through the index read pool; a `None` pool is a no-op.
    pub(crate) fn run_incremental_blocking(
        &self,
        volume_id: &str,
        available: SignalSet,
        changed_paths: &[String],
        now_secs: u64,
    ) -> Result<IncrementalReport, String> {
        // The batch gate, BEFORE anything expensive: drop the bare root, empties, and
        // every path that floors (build output, caches, dot-directories — none of
        // which can produce a weight row). Never escalate to a full pass here; full
        // recomputes are `ScanCompleted`-driven. An empty result returns without
        // opening the read pool or walking, which is what stops constant background
        // churn from driving a pass a minute forever. See `sanitize_incremental_batch`.
        let home = Self::home_dir();
        let changed_paths = sanitize_incremental_batch(changed_paths, &home);
        if changed_paths.is_empty() {
            return Ok(IncrementalReport::default());
        }

        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(IncrementalReport::default());
        };

        // The "before" side of the scoped walk's guard, read for the whole sanitized
        // batch (a superset of what survives de-duplication) so the plan and the walk
        // still fit in ONE read-pool checkout — de-duplication needs the index, since
        // it turns on which origins are too big to descend.
        let previous_markers = load_previous_markers(&self.data_dir, volume_id, &changed_paths);
        let (mut folders, scope, plan) = pool
            .with_conn(|conn| walk_for_incremental(conn, &home, &changed_paths, &previous_markers))
            .map_err(|e| format!("read pool error: {e}"))??;
        // One origin per changed subtree, and the two lists kept apart: `cleared` is
        // what the writer clears and re-inserts, `demoted` is the over-budget origins
        // rescored alone. Both the clear and the insert take the SAME `cleared` slice,
        // so they can't disagree about the region.
        let (changed_paths, demoted) = plan.lists_for(scope);

        // ❌ No `folders.is_empty()` early return here. A scoped walk over a batch
        // whose origins were ALL deleted between the publish and this pass finds no
        // folder — and that batch is exactly the one whose rows have to be CLEARED.
        // Returning early would leave every deleted folder's weight behind until the
        // next full pass.

        let visits = load_visits(&self.data_dir, volume_id);
        let writer = self.writer_for(volume_id).map_err(|e| e.to_string())?;

        let outcome = incremental_rescore(
            &IncrementalInputs {
                writer: &writer,
                weights: &self.weights,
                home: &home,
                now_secs,
                available,
                visits: &visits,
            },
            &mut folders,
            &changed_paths,
            scope,
            &demoted,
        )?;

        // Announce the pass whether or not it wrote a row: it CLEARED each changed
        // subtree either way, so a pass that only deleted rows (every origin gone, or a
        // whole subtree newly floored) still moved the store. The rows carry the current
        // generation (no bump), so that's what the notice announces as freshly touched.
        let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);
        super::read::notify_recompute_completed(
            volume_id,
            match outcome.delta {
                Some(delta) => WeightsChanged::Delta {
                    generation,
                    upserted: delta.upserted.into(),
                    removed: delta.removed.into(),
                },
                // The pass was too big to describe row by row; reloading is cheaper.
                None => WeightsChanged::ReloadAll { generation },
            },
        );
        Ok(outcome.report)
    }
}
