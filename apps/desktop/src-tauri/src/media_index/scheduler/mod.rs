//! The media enrichment scheduler: run a volume's image-OCR enrichment when its
//! index finishes scanning, and once at startup for a volume Fresh at launch.
//!
//! Ported from `importance/scheduler` (plan Decision 7), with its OWN `start()`
//! mirroring importance's ordering (subscribe to registrations → sweep
//! `ready_volumes_with_kind()` → wire per-volume subscriptions). It can't piggyback
//! importance's subscription; because `app.manage` is keyed by type, an
//! `Arc<MediaScheduler>` coexists fine alongside `importance`'s scheduler.
//!
//! ## What drives a pass
//!
//! Triggers, all folded through the [`PassCoordinator`] (full detail in `DETAILS.md`):
//!
//! - **The lifecycle bus** ([`crate::indexing::lifecycle_bus`]): a `ScanCompleted` ⇒
//!   enrich. Consumed **edge-triggered** (`borrow_and_update`), NEVER a `borrow()` poll:
//!   the `watch` retains the last `Completed` across a new scan's truncate window, so a
//!   poll could GC live rows mid-truncate. The edge is the data-safety line (Decision 3).
//! - **The startup sweep** ([`crate::indexing::ready_volumes_with_kind`]) only WIRES
//!   subscriptions — a volume Fresh at launch keeps a `Pending` bus and never re-fires,
//!   so [`kick_all_ready_passes_with`] at the end of [`start`] (master toggle on) is what
//!   actually enriches on a persisted-on restart.
//! - **User actions** ([`kick_all_ready_passes`] / [`kick_network_pass`]): toggle-on, a
//!   threshold DECREASE, or a network opt-in kicks an immediate pass.
//! - **The importance bridge** ([`wire_volume`]'s subscriber): a pass that DEFERRED its
//!   gated remainder (importance unscored) is re-kicked when importance first scores
//!   (defer-until-scored). **The registration bus** wires a late-registered volume.
//!
//! Local volumes enrich by default when the master toggle is on; opted-in SMB volumes
//! run the CONSERVATIVE network pass ([`MediaScheduler::run_network_pass_blocking`]);
//! MTP is NEVER background-swept (on-demand only). The [`PassCoordinator`] clone
//! guarantees ONE pass per volume, folding a concurrent request into a single re-run.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};

use super::backend::VisionBackend;
use super::network;
use super::network::enrich::{NetworkEnrichCtx, NetworkPassOutcome, PauseReason, enrich_network_and_gc};
use super::network::fetch::FsByteFetcher;
use super::network::policy::ConservativeFetchPolicy;
// The fake backend is production's fallback only off-macOS (macOS uses real Vision);
// tests import it themselves.
#[cfg(not(target_os = "macos"))]
use super::backend::fake::FakeVisionBackend;
use super::gate;
use crate::ignore_poison::IgnorePoison;
use crate::indexing::IndexVolumeKind;

pub(crate) mod enrich;
use enrich::{EnrichGates, GcScope, PassHooks, enrich_and_gc_scoped, load_statuses, walk_image_entries};

mod live;

use super::events::{EnrichTerminalGuard, MediaEnrichTerminalReason, TauriEnrichEmitter};
use super::progress::{EnrichProgressSink, NoopProgressSink};

mod reclaim;
pub use reclaim::{PruneOutcome, StoredCoverage, StoredCoverageCounts};

#[cfg(test)]
mod coalescing_tests;
#[cfg(test)]
mod enrich_tests;
#[cfg(test)]
mod kick_tests;
#[cfg(test)]
mod reclaim_tests;

// ── Coalescing coordinator (pure, testable — a `PassCoordinator` clone) ────

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
    /// A pass is already running; the request set the re-run flag (coalesced).
    Coalesced,
}

/// The outcome of finishing a pass for a volume.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FinishOutcome {
    /// No re-run was requested; the volume is now idle.
    Done,
    /// A re-run was requested during the pass; the caller should run once more.
    RunAgain,
}

/// The coalescing core: one pass per volume at a time, folding concurrent requests
/// into a single re-run. Pure and lock-guarded, so the "sweep + concurrent
/// ScanCompleted ⇒ one pass" contract is unit-testable without an app or a runtime.
#[derive(Default)]
pub(crate) struct PassCoordinator {
    slots: Mutex<HashMap<String, PassSlot>>,
}

impl PassCoordinator {
    fn new() -> Self {
        Self::default()
    }

    /// Request a pass for `volume_id`. Returns [`BeginOutcome::Start`] exactly when
    /// the caller should begin a pass; a request arriving while a pass runs returns
    /// [`BeginOutcome::Coalesced`] and sets the re-run flag.
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

    /// Whether a pass is currently running for `volume_id`. Drives the honest
    /// "still indexing images…" coverage state the search UI shows (a snapshot, not
    /// a subscription — the minimal per-volume enrichment signal).
    pub(crate) fn is_running(&self, volume_id: &str) -> bool {
        self.slots
            .lock_ignore_poison()
            .get(volume_id)
            .is_some_and(|slot| slot.running)
    }

    /// Finish the running pass for `volume_id`. Returns [`FinishOutcome::RunAgain`]
    /// (keeping the slot running) if a re-run was requested, else
    /// [`FinishOutcome::Done`].
    pub(crate) fn finish(&self, volume_id: &str) -> FinishOutcome {
        let mut slots = self.slots.lock_ignore_poison();
        let slot = slots.entry(volume_id.to_string()).or_default();
        if slot.rerun_requested {
            slot.rerun_requested = false;
            FinishOutcome::RunAgain
        } else {
            slot.running = false;
            FinishOutcome::Done
        }
    }
}

// ── The scheduler handle ────────────────────────────────────────────────────

/// The media enrichment scheduler. Holds the coalescing coordinator, the app data
/// dir, the long-lived per-volume writer registry, and the inference backend behind
/// the [`VisionBackend`] seam. Cloneable-by-`Arc` for the bus-listener tasks and as
/// Tauri managed state.
pub struct MediaScheduler {
    coordinator: PassCoordinator,
    data_dir: PathBuf,
    writers: super::writer_registry::WriterRegistry,
    backend: Arc<dyn VisionBackend>,
    /// The app handle used to emit `media-enrich-progress` / `media-enrich-terminal`
    /// events. `None` in unit tests (constructed via [`MediaScheduler::new`],
    /// no app), so a pass emits nothing; production wires it in [`start`].
    app: Option<AppHandle>,
    /// Volume ids whose last pass DEFERRED its importance-gated remainder because
    /// importance hadn't scored the volume yet (`folder_scores` was `None`). The
    /// unscored → scored bridge reads and clears this: when importance first
    /// completes a recompute, [`wire_volume`]'s subscriber re-kicks exactly these
    /// volumes so the deferred images enrich, then clears the flag. Scoped to the
    /// bridge so a normal volume (scored from the start) never re-kicks, and a later
    /// incremental bump doesn't re-walk the index for nothing.
    deferred_for_importance: Mutex<HashSet<String>>,
    /// Per-volume accumulator of touched DIRECTORY paths awaiting a live enrichment
    /// tick. A burst of dir-changed batches coalesces here so overlapping
    /// ticks drain one combined set, not one tick per batch — mirroring importance's
    /// `pending_incremental`.
    pending_touched_dirs: Mutex<HashMap<String, HashSet<String>>>,
}

impl MediaScheduler {
    /// Construct a scheduler over `data_dir` with the given inference backend, NOT wired
    /// to an app (so it emits no progress events — the unit-test constructor).
    pub fn new(data_dir: PathBuf, backend: Arc<dyn VisionBackend>) -> Self {
        Self {
            coordinator: PassCoordinator::new(),
            data_dir,
            writers: super::writer_registry::WriterRegistry::new(),
            backend,
            app: None,
            deferred_for_importance: Mutex::new(HashSet::new()),
            pending_touched_dirs: Mutex::new(HashMap::new()),
        }
    }

    /// Construct a scheduler wired to `app`, so its passes emit the progress +
    /// terminal events onto the top-right indicator. Used by [`start`].
    fn new_with_app(data_dir: PathBuf, backend: Arc<dyn VisionBackend>, app: AppHandle) -> Self {
        Self {
            app: Some(app),
            ..Self::new(data_dir, backend)
        }
    }

    /// Build the throttled progress sink + terminal guard for a pass. When the
    /// scheduler has an app, the sink emits `media-enrich-progress` and the guard emits
    /// `media-enrich-terminal` on drop (its default `Failed` reason covers an error
    /// bubble); without an app (unit tests) both are no-ops.
    fn pass_emitters(&self, volume_id: &str) -> (Box<dyn EnrichProgressSink>, EnrichTerminalGuard) {
        match &self.app {
            Some(app) => (
                Box::new(TauriEnrichEmitter::new(app.clone(), volume_id.to_string())),
                EnrichTerminalGuard::for_app(app.clone(), volume_id.to_string()),
            ),
            None => (Box::new(NoopProgressSink), EnrichTerminalGuard::disabled()),
        }
    }

    /// Mark `volume_id` as having deferred its importance-gated remainder (its last
    /// pass ran with importance unavailable). The unscored → scored bridge re-kicks
    /// it once importance lands.
    fn mark_deferred_for_importance(&self, volume_id: &str) {
        self.deferred_for_importance
            .lock_ignore_poison()
            .insert(volume_id.to_string());
    }

    /// Take (read-and-clear) whether `volume_id` deferred on importance. Returns
    /// `true` exactly once per deferral, so the importance subscriber re-kicks the
    /// bridge only on the unscored → scored transition, never on every later bump.
    fn take_deferred_for_importance(&self, volume_id: &str) -> bool {
        self.deferred_for_importance.lock_ignore_poison().remove(volume_id)
    }

    /// The app data dir this scheduler resolves `media.db` paths under.
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    /// Whether an enrichment pass is currently running for `volume_id`. The honest
    /// signal behind the search UI's "still indexing images, results may be
    /// incomplete" state; full counts + ETA are a later milestone.
    pub fn is_enriching(&self, volume_id: &str) -> bool {
        self.coordinator.is_running(volume_id)
    }

    /// Run one full enrichment pass for a volume synchronously (blocking).
    ///
    /// Gated on the master toggle (off ⇒ no-op). Resolves the volume's index read
    /// pool (a `None` — the index isn't registered — is a no-op returning `Ok(0)`,
    /// the same skip-on-`None` discipline as importance), walks the index for
    /// qualifying images, loads the stored statuses, and enriches the stale ones +
    /// GCs vanished rows through the shared writer. GC is safe here: this runs only
    /// on a `Completed` edge / the Fresh sweep, so the tree is complete.
    pub fn run_pass_blocking(&self, volume_id: &str) -> Result<usize, String> {
        if !gate::is_enabled() {
            return Ok(0);
        }
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(0);
        };
        let images = pool
            .with_conn(walk_image_entries)
            .map_err(|e| format!("read pool error: {e}"))??;

        let statuses = load_statuses(&self.data_dir, volume_id);
        let writer = self
            .writers
            .writer_for(&self.data_dir, volume_id)
            .map_err(|e| e.to_string())?;

        // Importance-prioritized enrichment (plan Cross-cutting): read the folder
        // scores at the user's threshold, enrich the qualifying folders (high score
        // first), and defer the rest. When importance hasn't scored this volume yet
        // (`None`), DEFER the importance-gated remainder while still honoring an
        // explicit "always index" override — never enrich the whole volume, or a
        // first-run race (importance's multi-second recompute hasn't finished) would
        // over-index everything permanently (forward-only semantics). The unscored →
        // scored bridge re-kicks the deferred remainder once importance lands. An
        // "always index" override always enriches; a user-excluded folder never does
        // (privacy veto).
        let threshold = gate::importance_threshold();
        let scores = self.folder_scores(volume_id, threshold);
        if scores.is_none() {
            // Importance unavailable ⇒ this pass deferred its gated remainder; the
            // importance subscriber re-kicks once a recompute completes.
            self.mark_deferred_for_importance(volume_id);
        }
        // Coverage (override + importance threshold) is read from the START-OF-PASS
        // snapshot; the privacy exclusion is read LIVE (`network::config::is_excluded`),
        // so a folder excluded WHILE this pass runs is vetoed immediately — the veto is
        // a hard privacy line, not a tuning knob that can wait for the next pass.
        let config = network::config::snapshot();
        let should_enrich = |path: &str| -> bool { local_should_enrich(path, scores.as_ref(), &config, volume_id) };
        let is_excluded = |path: &str| -> bool { network::config::is_excluded(path) };
        let folder_score = |dir: &str| -> f64 { scores.as_ref().and_then(|m| m.get(dir)).copied().unwrap_or(0.0) };
        let ordered = enrich::prioritized(&images, &folder_score);

        // Progress + terminal emitters. The guard emits `Failed` on drop if the
        // pass bubbles an error before we set a clean reason below (the `?` on
        // `enrich_and_gc`), so EVERY exit path reports a terminal.
        let (progress, mut terminal) = self.pass_emitters(volume_id);
        let hooks = PassHooks {
            cancel: &gate::is_cancelled,
            progress: progress.as_ref(),
        };
        // A full pass walks the whole index, so a stored row absent from the walk genuinely
        // vanished: `GcScope::WholeStore` GCs the whole store. The scoped live tick uses
        // `GcScope::TouchedDirs` instead. The installed CLIP stamp drives the CLIP half of
        // two-part staleness (`None` = no model ⇒ Vision-only); `enrich_and_gc` (the
        // Vision-only wrapper) can't carry it, so the full pass reaches the core directly.
        let clip_stamp = crate::media_index::clip::current_stamp(&self.data_dir);
        let summary = enrich_and_gc_scoped(
            &ordered,
            &statuses,
            self.backend.as_ref(),
            &writer,
            &EnrichGates {
                should_enrich: &should_enrich,
                is_excluded: &is_excluded,
                gc_scope: GcScope::WholeStore,
                clip_stamp: clip_stamp.as_deref(),
            },
            &hooks,
        )?;
        terminal.set(if summary.cancelled {
            MediaEnrichTerminalReason::Cancelled
        } else {
            MediaEnrichTerminalReason::Completed {
                enriched: summary.enriched as u64,
                gc_count: summary.gc_count as u64,
            }
        });

        // The volume's embeddings changed; drop the resident cache so the next
        // find-similar / dedup reloads (per-pass invalidation, not per-write — plan §
        // Query-time vector residency).
        if summary.enriched > 0 || summary.gc_count > 0 {
            super::vector::cache::invalidate(&super::store::media_db_path(&self.data_dir, volume_id));
        }
        // Refill the covered-count cache from THIS pass's own walk instead of invalidating:
        // the pass already ran the exact whole-volume `walk_image_entries`, so refilling keeps
        // the slider preview warm rather than forcing the next preview to pay a fresh cold
        // O(entries) walk. `images` is the full qualifying set (unfiltered by threshold).
        super::coverage::replace_from_entries(volume_id, &images);
        log::info!(
            target: "media_index",
            "enrichment of '{volume_id}': {} of {} images enriched, {} rows GC'd",
            summary.enriched,
            images.len(),
            summary.gc_count,
        );
        Ok(summary.enriched)
    }

    /// The folder importance scores for `volume_id` at `threshold`: `Some(map)` of
    /// `folder → score` for every folder scoring at or above `threshold`, or `None`
    /// when importance has NEVER scored this volume (fresh, offline, or importance
    /// disabled). The `None` case is load-bearing: it tells the local pass to fall
    /// back to "enrich all" and the network pass to fall back to "override only"
    /// (plan Cross-cutting — the override stays load-bearing when importance is
    /// unavailable). Reads through `ImportanceIndex` (the read API answers OFFLINE),
    /// never a raw `rusqlite` dep.
    pub(crate) fn folder_scores(&self, volume_id: &str, threshold: f64) -> Option<HashMap<String, f64>> {
        use crate::importance::{ImportanceIndex, SignalSet};
        let index = ImportanceIndex::open(&self.data_dir, volume_id, SignalSet::all());
        // "Importance unavailable" (missing DB / offline / genuinely unscored) ⇒
        // `None`. Keys on live weight rows, not solely the generation stamp — an
        // incrementally-maintained or schema-recreated store has usable weights at
        // generation 0 (`super::coverage::importance_scored`).
        if !super::coverage::importance_scored(&index) {
            return None;
        }
        match index.above_threshold(threshold) {
            Ok(weights) => Some(weights.into_iter().map(|w| (w.path, w.score.value())).collect()),
            Err(e) => {
                log::warn!(target: "media_index", "importance read failed for '{volume_id}': {e}");
                None
            }
        }
    }

    /// Retro-delete every stored row at or under `folder` (an OS-mount path) across the
    /// reachable volumes in `mounts` (`volume_id`, `mount_root`) — the privacy
    /// complement to the veto, invoked when the user excludes a folder. USER-EXPLICIT
    /// deletion: it derives ONLY from settings state, never scan/bus/gate state, so it
    /// needs no completed-scan edge (unlike GC — see `DETAILS.md` § GC safety).
    ///
    /// Each volume maps the OS folder into its own index-path space
    /// ([`os_folder_to_index_prefix`]): the folder passes through on a local volume,
    /// strips the mount root on a network one, and a volume the folder isn't under is
    /// skipped. The delete is a DOUBLE-TAP through the volume's ONE writer thread (the
    /// second prune sweeps any straggler an in-flight upsert re-added), then a `VACUUM`
    /// reclaims the pages (privacy: the OCR text leaves the disk), then the vector +
    /// coverage caches for the volume drop. Returns the total rows deleted.
    ///
    /// **Offline network volumes** aren't in `mounts` (no mount root while unmounted),
    /// so they're skipped here and the retro-delete re-fires on reconnect via
    /// [`wire_volume`]. Runs off the IPC thread (the caller uses `spawn_blocking`), so
    /// the blocking prunes are deadlock-safe.
    ///
    /// [`os_folder_to_index_prefix`]: super::network::fetch::os_folder_to_index_prefix
    pub fn retro_delete_excluded_folder(&self, folder: &str, mounts: &[(String, String)]) -> usize {
        let mut total = 0usize;
        for (volume_id, mount_root) in mounts {
            // Only volumes that were actually enriched have a `media.db`; don't create an
            // empty one just to prune nothing.
            let db_path = super::store::media_db_path(&self.data_dir, volume_id);
            if !db_path.exists() {
                continue;
            }
            // Map the OS folder into this volume's index-path space; `None` ⇒ the folder
            // isn't under this mount, so this volume has no matching rows.
            let Some(index_prefix) = network::fetch::os_folder_to_index_prefix(folder, mount_root) else {
                continue;
            };
            let writer = match self.writers.writer_for(&self.data_dir, volume_id) {
                Ok(w) => w,
                Err(e) => {
                    log::warn!(target: "media_index", "retro-delete: writer for '{volume_id}' failed: {e}");
                    continue;
                }
            };
            // Double-tap through the ONE writer thread: the first (blocking) prune drains
            // the queue up to it; the second sweeps any straggler an in-flight upsert
            // re-added before its own pre-upsert veto re-check could stop it.
            let n1 = writer.prune_under_folder(&index_prefix).unwrap_or(0);
            let n2 = writer.prune_under_folder(&index_prefix).unwrap_or(0);
            let deleted = n1 + n2;
            if deleted > 0 {
                // Reclaim the pages (privacy: the OCR text leaves the disk), then drop
                // the derived caches so a later search / slider preview rebuilds honestly.
                let _ = writer.vacuum();
                super::vector::cache::invalidate(&db_path);
                super::coverage::invalidate(volume_id);
                log::info!(
                    target: "media_index",
                    "retro-delete under '{folder}' on '{volume_id}': {} removed",
                    crate::pluralize::pluralize(deleted as u64, "row")
                );
                total += deleted;
            }
        }
        total
    }

    /// Run one CONSERVATIVE network enrichment pass for an opted-in SMB volume
    /// (network enrichment): read each eligible image's bytes off the OS mount (bounded against
    /// a hung mount), OCR them, and GC vanished rows — idle-gated, bandwidth-bounded,
    /// resumable, and disconnect-paused.
    ///
    /// No-ops (returns `Ok`) when the master toggle is off, the volume isn't opted in,
    /// the volume isn't registered (no mount root / no index) — the same skip-on-absence
    /// discipline as the local pass. A disconnect mid-pass PAUSES the volume (keeps
    /// completed rows, no `Failed`, no GC); it resumes on reconnect via the bus.
    pub fn run_network_pass_blocking(&self, volume_id: &str) -> Result<(), String> {
        if !gate::is_enabled() {
            return Ok(());
        }
        // The per-volume SMB opt-in: turning on the master toggle does NOT auto-enrich
        // network volumes (plan Decision 6).
        if !network::config::is_opted_in(volume_id) {
            log::debug!(target: "media_index", "network enrichment skips '{volume_id}': not opted in");
            return Ok(());
        }
        // The OS mount root we read image bytes from (`/Volumes/<share>`), via the
        // VolumeManager — the same source `indexing::routing` uses for the read-side
        // mount strip. An unregistered volume (unmounted) is a no-op.
        let Some(mount_root) = crate::file_system::get_volume_manager()
            .get(volume_id)
            .map(|v| v.root().to_string_lossy().into_owned())
        else {
            log::debug!(target: "media_index", "network enrichment skips '{volume_id}': volume not registered");
            return Ok(());
        };
        let Some(pool) = crate::indexing::get_read_pool_for(volume_id) else {
            return Ok(());
        };
        let images = pool
            .with_conn(walk_image_entries)
            .map_err(|e| format!("read pool error: {e}"))??;
        let statuses = load_statuses(&self.data_dir, volume_id);
        let writer = self
            .writers
            .writer_for(&self.data_dir, volume_id)
            .map_err(|e| e.to_string())?;

        let policy = ConservativeFetchPolicy::default();
        let fetcher = FsByteFetcher;
        let idle_threshold = policy.idle_threshold;
        let is_idle = move || super::foreground::global().idle_for(idle_threshold);
        // The conservative per-image gate (plan Decision 6 + importance): an excluded folder
        // never enriches (privacy veto); otherwise enrich when an "always index"
        // override covers it OR its folder importance meets the slider threshold.
        // Importance keys on the INDEX identity, so strip the mount root off the OS
        // path to look it up. When importance is unavailable (`None`) the network path
        // stays conservative — override-only — never dragging the whole NAS.
        let threshold = gate::importance_threshold();
        let scores = self.folder_scores(volume_id, threshold);
        if scores.is_none() {
            // Same bridge as the local pass: importance unavailable ⇒ this pass ran
            // override-only; re-kick once a recompute completes so the threshold
            // applies (the network fallback stays conservative until then).
            self.mark_deferred_for_importance(volume_id);
        }
        // Coverage (override + importance threshold) rides the start-of-pass snapshot;
        // the privacy exclusion is read LIVE, so a folder excluded mid-pass is vetoed
        // at once (a hard privacy line, not a tuning knob).
        let config = network::config::snapshot();
        let should_enrich = |os_path: &str| -> bool {
            let covered = config.covers(volume_id, os_path);
            let index_path = os_path.strip_prefix(mount_root.as_str()).unwrap_or(os_path);
            let importance = scores
                .as_ref()
                .map(|m| m.get(enrich::parent_dir(index_path)).copied().unwrap_or(0.0) as f32);
            network::policy::should_enrich_image(covered, importance, threshold as f32)
        };
        let is_excluded = |os_path: &str| -> bool { network::config::is_excluded(os_path) };
        let cancel = || gate::is_cancelled();
        let sleep = |d: Duration| std::thread::sleep(d);

        // Progress + terminal emitters; the guard's default `Failed` covers an
        // error bubble on the `?` below, so every exit path reports a terminal.
        let (progress, mut terminal) = self.pass_emitters(volume_id);
        let clip_stamp = crate::media_index::clip::current_stamp(&self.data_dir);
        let ctx = NetworkEnrichCtx {
            volume_id,
            mount_root: &mount_root,
            images: &images,
            statuses: &statuses,
            backend: self.backend.as_ref(),
            fetcher: &fetcher,
            writer: &writer,
            policy: &policy,
            is_idle: &is_idle,
            should_enrich: &should_enrich,
            is_excluded: &is_excluded,
            cancel: &cancel,
            sleep: &sleep,
            progress: progress.as_ref(),
            clip_stamp: clip_stamp.as_deref(),
        };
        match enrich_network_and_gc(&ctx)? {
            NetworkPassOutcome::Completed(summary) => {
                network::config::clear_paused(volume_id);
                if summary.enriched > 0 || summary.gc_count > 0 {
                    super::vector::cache::invalidate(&super::store::media_db_path(&self.data_dir, volume_id));
                }
                // Refill coverage from this pass's whole-index walk (as the local pass does),
                // so an opted-in SMB volume's slider preview also stays warm without a cold
                // rewalk. `images` is the full qualifying set.
                super::coverage::replace_from_entries(volume_id, &images);
                terminal.set(MediaEnrichTerminalReason::Completed {
                    enriched: summary.enriched as u64,
                    gc_count: summary.gc_count as u64,
                });
                log::info!(
                    target: "media_index",
                    "network enrichment of '{volume_id}': {} of {} images enriched, {} rows GC'd",
                    summary.enriched,
                    images.len(),
                    summary.gc_count,
                );
            }
            NetworkPassOutcome::Paused { summary, reason } => {
                if reason == PauseReason::Disconnected {
                    // Mark paused so the coverage signal reads "paused, resumes on
                    // reconnect" and the resume happens via the registration bus.
                    network::config::mark_paused(volume_id);
                }
                // The terminal event re-voices the row (paused) or clears it (cancelled),
                // so it never sticks at "enriching" — the stuck-row bug.
                terminal.set(match reason {
                    PauseReason::NotIdle => MediaEnrichTerminalReason::PausedWaitingForIdle,
                    PauseReason::Disconnected => MediaEnrichTerminalReason::PausedDisconnected,
                    PauseReason::Cancelled => MediaEnrichTerminalReason::Cancelled,
                });
                log::info!(
                    target: "media_index",
                    "network enrichment of '{volume_id}' paused ({reason:?}) after {} enriched",
                    summary.enriched,
                );
            }
        }
        Ok(())
    }
}

/// Whether a LOCAL image at index path `path` is COVERED this pass — the pure
/// coverage gate (override + importance threshold), unit-testable without a DB or an
/// app. The privacy exclusion is a SEPARATE, live hard veto applied in
/// [`enrich::enrich_and_gc`] (never here), so coverage stays snapshot-pure while the
/// veto reads live config.
///
/// - When importance HASN'T scored the volume yet (`scores` is `None`), DEFER the
///   importance-gated remainder but still honor an explicit "always index" override
///   (`config.covers`), so a user directive is never postponed on a fresh volume.
///   This mirrors the network `None` → override-only fallback, keeping the two paths
///   symmetric. ❌ Never fall back to "enrich all" here: a first-run race against
///   importance's multi-second recompute would over-index the whole volume, and
///   forward-only semantics make that permanent until a manual reclaim. The
///   unscored → scored bridge ([`wire_volume`]'s subscriber) re-kicks the remainder
///   once importance lands.
/// - When SCORED, cover an override-covered folder OR one whose parent folder met
///   the threshold (already filtered into `scores`).
fn local_should_enrich(
    path: &str,
    scores: Option<&HashMap<String, f64>>,
    config: &network::config::NetworkEnrichConfig,
    volume_id: &str,
) -> bool {
    match scores {
        None => config.covers(volume_id, path),
        Some(map) => config.covers(volume_id, path) || map.contains_key(enrich::parent_dir(path)),
    }
}

/// Kick a coalesced enrichment pass for every volume ready to enrich right now —
/// the user-action entry point behind the master toggle, a persisted-on restart, and
/// a threshold decrease. Resolves the managed scheduler and delegates to
/// [`kick_all_ready_passes_with`]. A no-op when the scheduler isn't managed yet (an
/// early call before [`start`]).
pub fn kick_all_ready_passes(app: &AppHandle) {
    if let Some(scheduler) = app.try_state::<Arc<MediaScheduler>>() {
        kick_all_ready_passes_with(scheduler.inner());
    }
}

/// Kick a coalesced pass for every ready volume, given the scheduler handle
/// directly (so [`start`] can call it without a managed-state round-trip). Iterates
/// [`crate::indexing::ready_volumes_with_kind`] and spawns the kind-mapped pass
/// (Local → local, SMB → network which self-checks opt-in, MTP → never). The
/// [`PassCoordinator`] folds a kick that races a running pass into one re-run, and
/// each pass self-gates on the master toggle, so an errant kick while disabled is a
/// cheap no-op. Unconditional by design: staleness makes a redundant pass a fast
/// no-op, so there's no need to gate per volume (contrast importance, which gates on
/// "store has no generation").
pub fn kick_all_ready_passes_with(scheduler: &Arc<MediaScheduler>) {
    kick_ready_passes_from(scheduler, crate::indexing::ready_volumes_with_kind());
}

/// Kick a coalesced pass for each `(volume_id, kind)` in `ready`, mapping the kind to a
/// pass (Local → local, SMB → network which self-checks opt-in) and skipping the
/// never-swept kinds. Split from [`kick_all_ready_passes_with`] so the kind mapping +
/// spawn is testable against a controlled volume list, without the process-global index
/// registry.
fn kick_ready_passes_from(scheduler: &Arc<MediaScheduler>, ready: Vec<(String, IndexVolumeKind)>) {
    for (volume_id, kind) in ready {
        let pass_kind = match kind {
            IndexVolumeKind::Local => PassKind::Local,
            IndexVolumeKind::Smb => PassKind::Network,
            // MTP is never background-swept (on-demand only); nothing to kick.
            IndexVolumeKind::Mtp => continue,
            // A LocalExternal (USB/SD) drive's index paths are MOUNT-RELATIVE, so the
            // local pass (which treats stored paths as OS paths) would hand Vision
            // relative paths — the phantom-path bug class. Skip it until mount-root
            // mapping lands (parked: mount-relative paths aren't mapped yet).
            IndexVolumeKind::LocalExternal => continue,
        };
        spawn_pass(Arc::clone(scheduler), volume_id, pass_kind);
    }
}

/// Wire the scheduler to the app: seed the master toggle + network opt-in/override
/// state from settings, register the memory-watchdog stop hook, subscribe to
/// registrations, sweep the registry for already-ready volumes, and wire each
/// volume's scan-completion subscription by kind (local + opted-in SMB enrich; MTP
/// never background-sweeps).
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "media_index", "media scheduler not started: {e}");
            return;
        }
    };

    // Tell the CLIP module where the model installs, so the query-time text tower and the
    // enrichment image tower can load it (a no-op off macOS).
    crate::media_index::clip::set_data_dir(&data_dir);

    // Seed the master toggle + the network opt-in / always-index overrides from
    // settings (all off/empty by default; sparse-persisted, so absent keys mean off).
    let settings = crate::settings::load_settings(app);
    gate::set_enabled(settings.image_index_enabled == Some(true));
    gate::set_importance_threshold(
        settings
            .media_index_importance_threshold
            .unwrap_or(gate::DEFAULT_IMPORTANCE_THRESHOLD),
    );
    network::config::set_config(network::config::NetworkEnrichConfig {
        opted_in_volumes: settings.media_index_network_volumes.iter().cloned().collect(),
        always_index_volumes: settings.media_index_always_index_volumes.iter().cloned().collect(),
        always_index_folders: settings.media_index_always_index_folders.iter().cloned().collect(),
        excluded_folders: settings.media_index_excluded_folders.iter().cloned().collect(),
    });

    // Share the ONE resident-memory ceiling: the indexing memory watchdog's stop
    // action runs this hook, telling in-flight enrichment to yield — rather than a
    // second independent 16 GB ceiling over the same pool (plan Resources).
    crate::indexing::register_subsystem_stop_hook(Box::new(|| {
        gate::request_cancel();
        // Release the resident vector caches too, so they're counted against the ONE
        // shared ceiling (plan § Query-time vector residency): they reload lazily.
        super::vector::cache::clear_all();
    }));

    // Production selects the REAL Vision OCR backend on macOS; other platforms (where
    // Vision doesn't exist) fall back to the deterministic fake so the crate still
    // builds and the scheduler still runs. Tests inject their own fake directly via
    // `MediaScheduler::new`, never through `start`.
    #[cfg(target_os = "macos")]
    let backend: Arc<dyn VisionBackend> = Arc::new(super::backend::vision::VisionOcrBackend::new());
    #[cfg(not(target_os = "macos"))]
    let backend: Arc<dyn VisionBackend> = Arc::new(FakeVisionBackend::new());
    log::info!(target: "media_index", "media enrichment scheduler starting");
    let scheduler = Arc::new(MediaScheduler::new_with_app(data_dir, backend, app.clone()));
    app.manage(Arc::clone(&scheduler));

    // Subscribe to registrations FIRST (before the sweep) so a volume registering in
    // the gap isn't dropped (late-registering volumes).
    let reg_scheduler = Arc::clone(&scheduler);
    let mut reg_rx = crate::indexing::lifecycle_bus::subscribe_registrations();
    tauri::async_runtime::spawn(async move {
        loop {
            match reg_rx.recv().await {
                Ok(reg) => wire_volume(Arc::clone(&reg_scheduler), reg.volume_id, reg.kind),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // Startup sweep: wire each ready volume's subscriptions. A volume Fresh at launch
    // keeps a `Pending` bus and never re-fires `ScanCompleted`, so wiring alone never
    // enriches it — the kick below is what starts work.
    for (volume_id, kind) in crate::indexing::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id, kind);
    }

    // The persisted-on restart case: with the master toggle already on, kick every
    // ready volume now. Without this, a user whose toggle is on gets "0 of N indexed"
    // after every restart until some volume happens to rescan. Each pass
    // self-gates, and coalescing folds this into any pass a concurrent scan starts.
    if gate::is_enabled() {
        kick_all_ready_passes_with(&scheduler);
    }
}

/// Whether a volume's pass reads bytes locally or off the network (SMB). The
/// coalescing + bus wiring is identical; only which pass method runs differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PassKind {
    /// A local volume: the backend reads on-disk paths directly.
    Local,
    /// An opted-in SMB volume: the conservative byte-fetch pass reads off the mount.
    Network,
}

/// Wire one volume into the scheduler by its typed kind.
///
/// - **Local**: subscribe to the scan-completion bus and enrich locally.
/// - **SMB**: subscribe the same way and run the CONSERVATIVE network pass; the
///   per-volume opt-in is checked INSIDE the pass, so flipping the opt-in on takes
///   effect on the next scan completion (and the opt-in command kicks an immediate
///   pass — see [`kick_network_pass`]).
/// - **MTP**: NEVER background-swept: a phone/camera on MTP is transient
///   and slow, so enrichment is on-demand-per-visit, not a background sweep. The
///   on-demand trigger is a later slice; this gate is real now.
fn wire_volume(scheduler: Arc<MediaScheduler>, volume_id: String, kind: IndexVolumeKind) {
    let pass_kind = match kind {
        IndexVolumeKind::Local => PassKind::Local,
        IndexVolumeKind::Smb => PassKind::Network,
        IndexVolumeKind::Mtp => {
            log::debug!(
                target: "media_index",
                "media enrichment skips MTP '{volume_id}': never background-swept (on-demand-per-visit only)"
            );
            return;
        }
        // A LocalExternal (USB/SD) drive's index paths are MOUNT-RELATIVE, not OS paths,
        // so running the local pass (which reads stored paths as OS paths) would feed
        // Vision relative paths — the phantom-path bug class. NOT `PassKind::Local`. Skip
        // it until mount-root mapping lands (parked: mount-relative paths aren't mapped yet).
        IndexVolumeKind::LocalExternal => {
            log::debug!(
                target: "media_index",
                "media enrichment skips LocalExternal '{volume_id}': mount-relative index paths not yet mapped"
            );
            return;
        }
    };

    // The Fresh-at-launch dead-start: this volume's lifecycle bus stays `Pending` and
    // never re-fires `ScanCompleted`, so the subscription below never kicks it — and the
    // `start()`-time sweep kick can race the volume's registration (the sweep runs before
    // the volume is ready, then the registration bus wires it here). So kick an initial
    // coalesced pass for the volume we just wired when the master toggle is on, mirroring
    // importance's `enqueue_initial_full_pass_if_unscored`. The `PassCoordinator` folds
    // this with any sweep-time kick, so a double-kick is a harmless no-op; the network
    // pass self-checks opt-in inside itself.
    if gate::is_enabled() {
        spawn_pass(Arc::clone(&scheduler), volume_id.clone(), pass_kind);
    }

    // Live enrichment follows the index: a modified/new/deleted image under a
    // covered folder re-enriches (or GCs) within the throttle window, without waiting for
    // the next completed scan. LOCAL only: the tick treats stored paths as OS paths (no
    // mount mapping), and SMB's live path never publishes dirs_changed anyway, so wiring
    // it for network would be dead. MTP/LocalExternal already returned above.
    if pass_kind == PassKind::Local {
        live::start_live_follow(Arc::clone(&scheduler), volume_id.clone());
    }

    // Privacy retro-delete re-fire: a folder excluded while this volume was
    // OFFLINE never got purged (the retro-delete had no mount root then). On
    // (re)registration the volume is mounted, so purge any currently-excluded folder
    // that falls under it now. Idempotent and cheap: skipped entirely when nothing is
    // excluded, and a folder on another volume maps to `None` and no-ops.
    {
        let excluded = network::config::snapshot().excluded_folders;
        if !excluded.is_empty()
            && let Some(mount_root) = crate::file_system::get_volume_manager()
                .get(&volume_id)
                .map(|v| v.root().to_string_lossy().into_owned())
        {
            let re_scheduler = Arc::clone(&scheduler);
            let re_volume = volume_id.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let mounts = [(re_volume, mount_root)];
                for folder in &excluded {
                    re_scheduler.retro_delete_excluded_folder(folder, &mounts);
                }
            });
        }
    }

    // The unscored → scored bridge (defer-until-scored). Subscribe to
    // importance's recompute-completed `watch` SYNCHRONOUSLY here — BEFORE and
    // independent of the first pass. Watch semantics: a receiver is caught up to the
    // current version at subscribe time, so `changed()` fires only on the NEXT bump. A
    // lazy "a pass reads `None` → then subscribe" flow has a hole: importance can
    // complete in the gap, the receiver comes up already-caught-up, and the volume
    // defers forever. Subscribing up front (mirroring `search`'s
    // `start_importance_weight_subscriber`) closes it. Re-kick only the unscored →
    // scored transition: `take_deferred_for_importance` gates on a per-volume flag a
    // deferring pass set, so a normal volume never re-kicks and a later incremental
    // bump doesn't re-walk the index for nothing.
    let bridge_scheduler = Arc::clone(&scheduler);
    let bridge_volume = volume_id.clone();
    let mut imp_rx = crate::importance::read::subscribe(&volume_id);
    tauri::async_runtime::spawn(async move {
        // Catch up to the current version so `changed()` fires only on a later bump.
        imp_rx.borrow_and_update();
        while imp_rx.changed().await.is_ok() {
            imp_rx.borrow_and_update();
            if bridge_scheduler.take_deferred_for_importance(&bridge_volume) {
                spawn_pass(Arc::clone(&bridge_scheduler), bridge_volume.clone(), pass_kind);
            }
        }
    });

    let sub_scheduler = Arc::clone(&scheduler);
    let sub_volume = volume_id.clone();
    let mut rx = crate::indexing::lifecycle_bus::subscribe(&volume_id);
    tauri::async_runtime::spawn(async move {
        // Observe the retained value EDGE-triggered: `borrow_and_update` marks it
        // seen, so a later `changed()` fires only on a NEW completion, never on a
        // re-read of the retained `Completed`. This is the data-safety property —
        // GC (inside the pass) never runs off a stale retained `Completed`.
        if matches!(
            *rx.borrow_and_update(),
            crate::indexing::lifecycle_bus::ScanState::Completed { .. }
        ) {
            spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone(), pass_kind);
        }
        while rx.changed().await.is_ok() {
            if matches!(
                *rx.borrow_and_update(),
                crate::indexing::lifecycle_bus::ScanState::Completed { .. }
            ) {
                spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone(), pass_kind);
            }
        }
    });
}

/// Kick an immediate network pass for a volume (used when the user opts a volume in,
/// so enrichment starts without waiting for the next scan completion). Coalesces with
/// any running pass.
pub fn kick_network_pass(scheduler: Arc<MediaScheduler>, volume_id: String) {
    spawn_pass(scheduler, volume_id, PassKind::Network);
}

/// Request a coalesced enrichment pass and, if this request starts it, drive it
/// (plus any coalesced re-run) on a blocking background task — never on the IPC
/// thread, and on a dedicated worker (SQLite + backend), not rayon.
fn spawn_pass(scheduler: Arc<MediaScheduler>, volume_id: String, kind: PassKind) {
    if scheduler.coordinator.request(&volume_id) == BeginOutcome::Coalesced {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            let result = tauri::async_runtime::spawn_blocking(move || match kind {
                PassKind::Local => sched.run_pass_blocking(&vid),
                // Unify the return shape (the network pass reports via its own logs).
                PassKind::Network => sched.run_network_pass_blocking(&vid).map(|()| 0usize),
            })
            .await;
            match result {
                Ok(Ok(count)) => log::debug!(
                    target: "media_index",
                    "enrichment of '{volume_id}' ({kind:?}) enriched {}",
                    crate::pluralize::pluralize(count as u64, "image")
                ),
                Ok(Err(e)) => log::warn!(target: "media_index", "enrichment of '{volume_id}' failed: {e}"),
                Err(e) => log::warn!(target: "media_index", "enrichment task for '{volume_id}' panicked: {e}"),
            }
            if scheduler.coordinator.finish(&volume_id) == FinishOutcome::Done {
                break;
            }
        }
    });
}
