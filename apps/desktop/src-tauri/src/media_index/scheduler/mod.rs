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

use tauri::AppHandle;

use super::backend::VisionBackend;
use super::gate;
use super::network;
use super::network::enrich::{NetworkEnrichCtx, NetworkPassOutcome, PauseReason, enrich_network_and_gc};
use super::network::fetch::FsByteFetcher;
use super::network::policy::ConservativeFetchPolicy;
use crate::ignore_poison::IgnorePoison;

pub(crate) mod enrich;
use enrich::{EnrichGates, GcScope, PassHooks, enrich_and_gc_scoped, load_statuses, walk_image_entries};

mod live;

use super::events::{EnrichTerminalGuard, MediaEnrichTerminalReason, TauriEnrichEmitter};
use super::progress::{EnrichProgressSink, NoopProgressSink};

mod reclaim;
pub use reclaim::{PruneOutcome, StoredCoverage, StoredCoverageCounts};

mod coordinator;
use coordinator::{BeginOutcome, FinishOutcome, PassCoordinator};

mod lifecycle;
use lifecycle::local_should_enrich;
pub use lifecycle::{kick_all_ready_passes, kick_all_ready_passes_with, kick_network_pass, start};
// Re-exported into the scheduler namespace so the sibling `kick_tests` module reaches
// them through its `use super::*` (they're otherwise only called within `lifecycle`).
#[cfg(test)]
use lifecycle::{kick_ready_passes_from, wire_volume};

#[cfg(test)]
mod coalescing_tests;
#[cfg(test)]
mod enrich_tests;
#[cfg(test)]
mod kick_tests;
#[cfg(test)]
mod reclaim_tests;

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
            // Stop between images on the memory-watchdog emergency stop OR a master-toggle
            // OFF, so disabling image indexing halts an in-flight pass promptly (§ gate).
            cancel: &gate::should_stop,
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
        // Stop on the watchdog emergency stop OR a master-toggle OFF (§ gate), so
        // disabling image indexing halts a running NAS pass promptly.
        let cancel = || gate::should_stop();
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
