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
//!    [`enqueue_initial_full_pass_if_unscored`] per ready volume: it forces the
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
use std::time::{Duration, Instant};

use super::scorer::{SignalSet, Weights};
use super::writer::ImportanceWriter;
use crate::IndexVolumeKind;
use cmdr_fs::ignore_poison::IgnorePoison;

/// Comparing two walks of the same index, for the measurement tools. Nothing in
/// the app reaches it: the scheduler's own incremental path is `recompute`.
#[cfg(any(test, feature = "tooling"))]
mod differential;
mod recompute;
mod scoped_walk;
mod walk;
use recompute::{
    IncrementalInputs, RecomputeInputs, dedupe_nested_origins, incremental_rescore, load_previous_markers, load_visits,
    recompute_folders, sanitize_incremental_batch, walk_for_incremental,
};
// Re-exported for the eval corpus tool, which walks a real index the SAME way a
// recompute does (so dumped signals match production exactly).
pub(crate) use walk::walk_index_folders;
// The measurement/tuning entry point: walk a real index, score, write an
// `importance.db` — the full-pass core without the registry or async driver.
use crate::indexing::lifecycle::lifecycle_bus;
#[cfg(any(test, feature = "tooling"))]
pub use recompute::{MeasureOutcome, recompute_index_to_db};
// The correctness harness: run the scoped walk and the full walk over the same real
// index and difference the rows they'd write.
#[cfg(any(test, feature = "tooling"))]
pub use differential::{OriginComparison, compare_walks_for_incremental, sample_origins};

// ── Volume kind → scoring policy (typed, never string-matched) ────────────

/// How the importance scheduler treats a volume, decided by its typed
/// [`IndexVolumeKind`] — never by inspecting the volume-id string (`no-string-matching`).
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
        build_and_wire()
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

        // Announce the completed full pass so a read-API consumer reacts instead
        // of polling (subscribe-don't-poll).
        super::read::notify_recompute_completed(volume_id, outcome.generation);
        Ok(outcome.count)
    }

    /// Run one INCREMENTAL rescore for a volume: rescore only the folders whose
    /// listings changed (`changed_paths`) plus their capped ancestor chains, and
    /// upsert those rows WITHOUT advancing the generation (untouched folders keep
    /// their as-of marker). Returns the number of folders
    /// rescored.
    ///
    /// A `"/"` sentinel in `changed_paths` (a full-refresh emit) escalates to a
    /// full pass. Reads through the index read pool; a `None` pool is a no-op.
    pub(crate) fn run_incremental_blocking(
        &self,
        volume_id: &str,
        available: SignalSet,
        changed_paths: &[String],
        now_secs: u64,
    ) -> Result<usize, String> {
        // The batch gate, BEFORE anything expensive: drop the bare root, empties, and
        // every path that floors (build output, caches, dot-directories — none of
        // which can produce a weight row). Never escalate to a full pass here; full
        // recomputes are `ScanCompleted`-driven. An empty result returns without
        // opening the read pool or walking, which is what stops constant background
        // churn from driving a pass a minute forever. See `sanitize_incremental_batch`.
        let home = Self::home_dir();
        let changed_paths = sanitize_incremental_batch(changed_paths, &home);
        // One origin per changed subtree: a nested origin adds nothing and costs a
        // second read of the same rows. Both the clear and the insert take THIS
        // slice, so they can't disagree about the region.
        let changed_paths = dedupe_nested_origins(&changed_paths);
        if changed_paths.is_empty() {
            return Ok(0);
        }

        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };

        // The "before" side of the scoped walk's guard, read before the walk so the
        // whole decision fits in one read-pool checkout.
        let previous_markers = load_previous_markers(&self.data_dir, volume_id, &changed_paths);
        let (mut folders, scope) = pool
            .with_conn(|conn| walk_for_incremental(conn, &home, &changed_paths, &previous_markers))
            .map_err(|e| format!("read pool error: {e}"))??;

        // ❌ No `folders.is_empty()` early return here. A scoped walk over a batch
        // whose origins were ALL deleted between the publish and this pass finds no
        // folder — and that batch is exactly the one whose rows have to be CLEARED.
        // Returning early would leave every deleted folder's weight behind until the
        // next full pass.

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
            &mut folders,
            &changed_paths,
            scope,
        )?;

        // Announce the pass whether or not it wrote a row: it CLEARED each changed
        // subtree either way, so a pass that only deleted rows (every origin gone,
        // or a whole subtree newly floored) still moved the store. The incremental
        // rows carry the current generation (no bump), so the notification announces
        // that generation as freshly touched.
        let generation = writer.next_generation().map_err(|e| e.to_string())?.saturating_sub(1);
        super::read::notify_recompute_completed(volume_id, generation);
        Ok(count)
    }
}

/// Build and wire the scheduler, behind [`ImportanceScheduler::start`], which carries
/// the contract this fulfils. The registration bus catches a share mounted
/// MID-SESSION; the startup sweep catches volumes already ready at launch —
/// subscribing to the bus BEFORE the sweep closes the gap so no registration is
/// missed.
fn build_and_wire() -> Option<Arc<ImportanceScheduler>> {
    let data_dir = match crate::indexing::host::config::data_dir() {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "importance", "importance scheduler not started: {e}");
            return None;
        }
    };
    let scheduler = Arc::new(ImportanceScheduler::new(data_dir));

    // Subscribe to registrations FIRST (before the sweep), so a volume that
    // registers during the sweep isn't dropped in the gap. Each registration
    // wires that volume's per-volume subscriptions and scores it if it's
    // already ready.
    let reg_scheduler = Arc::clone(&scheduler);
    let mut reg_rx = lifecycle_bus::subscribe_registrations();
    crate::indexing::host::runtime::spawn(async move {
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
    // typed kind so MTP is excluded and SMB degrades correctly. Wiring alone only
    // sets up subscriptions (the retained bus value stays `Pending`); the
    // initial-pass trigger is what actually scores a fresh / recreated store.
    for (volume_id, kind) in crate::indexing::lifecycle::state::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id.clone(), kind);
        enqueue_initial_full_pass_if_unscored(Arc::clone(&scheduler), volume_id, kind);
    }

    // The caller owns the handle: the app keeps it in Tauri state so `record_visit`
    // can route its write through the shared per-volume writer the scheduler owns
    // (one writer thread per DB) rather than spawning one per navigation.
    Some(scheduler)
}

/// For a volume READY at launch (Fresh, so no `ScanCompleted` will fire), enqueue a
/// full recompute IFF its store has no generation yet — a fresh install, a
/// schema-recreated store (the prod schema-3 upgrade), or one maintained only by
/// incremental rescores (which never stamp a generation). An already-scored volume is
/// left alone; an unconditional kick would rescore every volume on every launch
/// (importance's policy differs from media's cheap unconditional kick).
///
/// The "unscored?" decision binds to the WRITE-path store open via
/// [`super::store::needs_initial_full_pass`], which forces the lazy schema recreate
/// BEFORE reading the generation — never a sweep-time read probe, which would read the
/// outgoing schema's stamped generation and skip, only for the recreate to wipe it
/// moments later (the prod-upgrade ordering trap). The probe (a DB open) runs on a
/// blocking task; when unscored it hands off to the normal coordinated
/// [`spawn_recompute`], so a concurrent `ScanCompleted` coalesces correctly.
fn enqueue_initial_full_pass_if_unscored(
    scheduler: Arc<ImportanceScheduler>,
    volume_id: String,
    kind: IndexVolumeKind,
) {
    let available = match ScoringPolicy::for_kind(kind) {
        ScoringPolicy::Scored { available } => available,
        // MTP: on-demand only, never background-scored.
        ScoringPolicy::Excluded => return,
    };
    crate::indexing::host::runtime::spawn(async move {
        let data_dir = scheduler.data_dir().to_path_buf();
        let vid = volume_id.clone();
        let needs = crate::indexing::host::runtime::spawn_blocking(move || {
            should_enqueue_initial_full_pass(kind, &data_dir, &vid)
        })
        .await;
        match needs {
            Ok(Ok(true)) => {
                log::info!(
                    target: "importance",
                    "volume '{volume_id}' ready at launch with no generation (fresh/recreated); enqueuing an initial full recompute"
                );
                spawn_recompute(scheduler, volume_id, available);
            }
            Ok(Ok(false)) => {} // already scored — leave it.
            Ok(Err(e)) => log::warn!(target: "importance", "initial-pass probe for '{volume_id}' failed: {e}"),
            Err(e) => log::warn!(target: "importance", "initial-pass probe task for '{volume_id}' panicked: {e}"),
        }
    });
}

/// Whether a volume ready at launch needs an initial full recompute enqueued: its kind
/// is background-scored (not MTP) AND its store carries no generation yet (fresh /
/// schema-recreated / incremental-only). Binds the "unscored?" check to the write-path
/// store open ([`super::store::needs_initial_full_pass`]), which forces any lazy schema
/// recreate before reading the generation. Extracted from
/// [`enqueue_initial_full_pass_if_unscored`] so the combined kind + store-state decision
/// is testable without spawning the recompute (which needs a read pool).
fn should_enqueue_initial_full_pass(
    kind: IndexVolumeKind,
    data_dir: &std::path::Path,
    volume_id: &str,
) -> Result<bool, super::store::ImportanceStoreError> {
    if matches!(ScoringPolicy::for_kind(kind), ScoringPolicy::Excluded) {
        return Ok(false); // MTP: on-demand only, never background-scored.
    }
    super::store::needs_initial_full_pass(data_dir, volume_id)
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
        // MTP: on-demand only, never background-scored (a typed exclusion).
        ScoringPolicy::Excluded => {
            log::debug!(target: "importance", "importance skips '{volume_id}' ({kind:?}): on-demand only");
            return;
        }
    };

    // Incremental recompute: rescore only the touched subtrees + capped ancestor
    // chains as live listing changes land. Full-volume recompute
    // stays the scan-completion default below.
    start_incremental(Arc::clone(&scheduler), volume_id.clone(), available);

    // Subscribe to the scan bus for this volume; a subscription retains the last
    // state, so a ScanCompleted fired before this line is still observed
    // (late-subscriber replay). Recompute on each completion.
    let sub_scheduler = Arc::clone(&scheduler);
    let sub_volume = volume_id.clone();
    let mut rx = lifecycle_bus::subscribe(&volume_id);
    crate::indexing::host::runtime::spawn(async move {
        // Observe the retained value first (covers a completion before subscribe,
        // and a sweep-ready volume that already loaded Completed).
        if matches!(*rx.borrow_and_update(), lifecycle_bus::ScanState::Completed { .. }) {
            spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available);
        }
        while rx.changed().await.is_ok() {
            if matches!(*rx.borrow_and_update(), lifecycle_bus::ScanState::Completed { .. }) {
                spawn_recompute(Arc::clone(&sub_scheduler), sub_volume.clone(), available);
            }
        }
    });
}

/// Subscribe to a volume's dir-changed bus and run a bounded incremental rescore
/// for each batch of live listing changes. Coalesces overlapping
/// batches per volume (accumulating their paths) so a burst of FSEvents collapses
/// to one pass plus at most one re-run, never a pass per event.
fn start_incremental(scheduler: Arc<ImportanceScheduler>, volume_id: String, available: SignalSet) {
    let mut rx = lifecycle_bus::subscribe_dirs_changed(&volume_id);
    crate::indexing::host::runtime::spawn(async move {
        // The retained initial value is the empty batch (nothing published yet);
        // `borrow_and_update` marks it seen so the first real change triggers.
        rx.borrow_and_update();
        while rx.changed().await.is_ok() {
            // The bus carries the ORIGIN dirs (those whose own listings changed), not
            // their ancestor closure, so the rescore's downward subtree expansion
            // stays proportional to what actually changed.
            let origins = rx.borrow_and_update().origins.clone();
            if origins.is_empty() {
                continue;
            }
            spawn_incremental(Arc::clone(&scheduler), volume_id.clone(), available, origins);
        }
    });
}

/// Coalescing key for incremental passes: distinct from the full-pass key so an
/// incremental rescore and a full recompute for the same volume don't block each
/// other in the coordinator (they serialize at the writer thread anyway).
fn incremental_key(volume_id: &str) -> String {
    format!("{volume_id}#incremental")
}

/// The minimum spacing between two incremental rescores of the same volume under
/// sustained change. A busy boot volume is never truly idle, so without a window
/// the FSEvent firehose would drive back-to-back passes forever.
///
/// What the window paces is NOT the walk: the scoped walk made a typical pass
/// microseconds (`scoped_walk.rs`). It's the store write and, through
/// `notify_recompute_completed`, the weight reload in `search::volumes` — which is
/// O(ALL weights) on the volume, not O(changed). ❌ Don't relax this window on the
/// grounds that the walk is now cheap; that trades a cheap walk for a frequent
/// full weight-map rebuild. Lower it once that reload is incremental too.
/// Rationale and numbers: `DETAILS.md` § Throttle.
///
/// Importance is a background signal, so a lag of this order is invisible to its
/// consumers.
const INCREMENTAL_THROTTLE_WINDOW: Duration = Duration::from_secs(60);

/// How long to wait before the next incremental rescore of a volume may start,
/// given when the previous one for this run started. The FIRST pass of a burst
/// (`last_started == None`) runs immediately (leading edge — a real edit scores
/// promptly); each further pass while change keeps arriving waits out the window
/// (trailing edge — at most one walk per window under sustained churn). Pure so the
/// spacing is unit-testable without a runtime; the caller sleeps this long.
fn incremental_debounce_wait(last_started: Option<Instant>, now: Instant, window: Duration) -> Duration {
    match last_started {
        // Leading edge: nothing ran yet this run, so go now.
        None => Duration::ZERO,
        // Trailing edge: wait out whatever remains of the window since the last
        // pass started (zero once the window has fully elapsed).
        Some(started) => window.saturating_sub(now.saturating_duration_since(started)),
    }
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
    crate::indexing::host::runtime::spawn(async move {
        let key = incremental_key(&volume_id);
        // Debounce across this run's passes: the first runs immediately (leading
        // edge), each further one waits out the window so sustained churn drives at
        // most one index walk per window. Requests arriving during the wait coalesce
        // (the coordinator slot stays running), so the next drain folds them all in.
        let mut last_started: Option<Instant> = None;
        loop {
            let wait = incremental_debounce_wait(last_started, Instant::now(), INCREMENTAL_THROTTLE_WINDOW);
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
            let batch = scheduler.take_incremental_paths(&volume_id);
            if !batch.is_empty() {
                last_started = Some(Instant::now());
                let sched = Arc::clone(&scheduler);
                let vid = volume_id.clone();
                let result = crate::indexing::host::runtime::spawn_blocking(move || {
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
                        cmdr_fs::pluralize::pluralize(count as u64, "folder")
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
    crate::indexing::host::runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            // Recompute is blocking (SQLite + scoring); run it off the async
            // worker so it never parks the runtime, and never on the IPC thread.
            let result = crate::indexing::host::runtime::spawn_blocking(move || {
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
                    cmdr_fs::pluralize::pluralize(count as u64, "folder")
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
