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
//! - **The lifecycle bus** ([`crate::indexing::lifecycle_bus`]): a `ScanCompleted`
//!   for a volume ⇒ enrich it. Consumed **edge-triggered** (`borrow_and_update` /
//!   `has_changed`), NEVER a `borrow()` poll — the `watch` retains the last
//!   `Completed` across a new scan's truncate window, so a poll could observe a
//!   stale `Completed` mid-truncate and GC live rows. The edge is the data-safety
//!   guarantee (plan Decision 3).
//! - **The startup registry sweep** ([`crate::indexing::ready_volumes_with_kind`]):
//!   a volume Fresh at launch never re-fires `ScanCompleted`, so it's enqueued once
//!   here (the common restart case).
//! - **The registration bus**: a volume registered after the sweep is wired then.
//!
//! Local volumes only in M1 (SMB/MTP enrichment is M1.5). The [`PassCoordinator`]
//! clone guarantees ONE pass per volume, folding a concurrent request into a single
//! re-run.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use super::backend::VisionBackend;
// The fake backend is production's fallback only off-macOS (macOS uses real Vision);
// tests import it themselves.
#[cfg(not(target_os = "macos"))]
use super::backend::fake::FakeVisionBackend;
use super::gate;
use crate::ignore_poison::IgnorePoison;
use crate::indexing::IndexVolumeKind;

mod enrich;
use enrich::{enrich_and_gc, load_statuses, walk_image_entries};

#[cfg(test)]
mod coalescing_tests;
#[cfg(test)]
mod enrich_tests;

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
    /// a subscription — M1's minimal per-volume enrichment signal).
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
}

impl MediaScheduler {
    /// Construct a scheduler over `data_dir` with the given inference backend.
    pub fn new(data_dir: PathBuf, backend: Arc<dyn VisionBackend>) -> Self {
        Self {
            coordinator: PassCoordinator::new(),
            data_dir,
            writers: super::writer_registry::WriterRegistry::new(),
            backend,
        }
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
        let summary = enrich_and_gc(&images, &statuses, self.backend.as_ref(), &writer, &|| {
            gate::is_cancelled()
        })?;

        log::info!(
            target: "media_index",
            "enrichment of '{volume_id}': {} of {} images enriched, {} rows GC'd",
            summary.enriched,
            images.len(),
            summary.gc_count,
        );
        Ok(summary.enriched)
    }
}

/// Wire the scheduler to the app: seed the master toggle from settings, register the
/// memory-watchdog stop hook, subscribe to registrations, sweep the registry for
/// already-ready volumes, and wire each Local volume's scan-completion subscription.
///
/// M1 is LOCAL-only: SMB/MTP enrichment (which must read bytes off the wire) is
/// M1.5. The fake backend is wired here as the M1 placeholder; the real
/// `objc2-vision` OCR backend is the next slice and drops in behind the same seam
/// with no change here.
pub fn start(app: &AppHandle) {
    let data_dir = match crate::config::resolved_app_data_dir(app) {
        Ok(d) => d,
        Err(e) => {
            log::warn!(target: "media_index", "media scheduler not started: {e}");
            return;
        }
    };

    // Seed the master toggle from settings (off by default; sparse-persisted, so an
    // absent key means off).
    gate::set_enabled(crate::settings::load_settings(app).image_index_enabled == Some(true));

    // Share the ONE resident-memory ceiling: the indexing memory watchdog's stop
    // action runs this hook, telling in-flight enrichment to yield — rather than a
    // second independent 16 GB ceiling over the same pool (plan Resources).
    crate::indexing::register_subsystem_stop_hook(Box::new(|| {
        gate::request_cancel();
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
    let scheduler = Arc::new(MediaScheduler::new(data_dir, backend));
    app.manage(Arc::clone(&scheduler));

    // Subscribe to registrations FIRST (before the sweep) so a volume registering in
    // the gap isn't dropped (plan M4 late-registering volumes).
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

    // Startup sweep: volumes Fresh at launch never re-fire ScanCompleted.
    for (volume_id, kind) in crate::indexing::ready_volumes_with_kind() {
        wire_volume(Arc::clone(&scheduler), volume_id, kind);
    }
}

/// Wire one volume into the scheduler by its typed kind. M1 handles LOCAL only:
/// SMB and MTP are skipped (their byte-fetch enrichment is M1.5). For a Local
/// volume, subscribe to its scan-completion bus and enrich on each completion edge,
/// plus once now if it's already `Completed` (the sweep / late-registration case).
fn wire_volume(scheduler: Arc<MediaScheduler>, volume_id: String, kind: IndexVolumeKind) {
    if kind != IndexVolumeKind::Local {
        log::debug!(target: "media_index", "media enrichment skips '{volume_id}' ({kind:?}): local-only in M1");
        return;
    }

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
            spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone());
        }
        while rx.changed().await.is_ok() {
            if matches!(
                *rx.borrow_and_update(),
                crate::indexing::lifecycle_bus::ScanState::Completed { .. }
            ) {
                spawn_pass(Arc::clone(&sub_scheduler), sub_volume.clone());
            }
        }
    });
}

/// Request a coalesced enrichment pass and, if this request starts it, drive it
/// (plus any coalesced re-run) on a blocking background task — never on the IPC
/// thread, and on a dedicated worker (SQLite + backend), not rayon.
fn spawn_pass(scheduler: Arc<MediaScheduler>, volume_id: String) {
    if scheduler.coordinator.request(&volume_id) == BeginOutcome::Coalesced {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let sched = Arc::clone(&scheduler);
            let vid = volume_id.clone();
            let result = tauri::async_runtime::spawn_blocking(move || sched.run_pass_blocking(&vid)).await;
            match result {
                Ok(Ok(count)) => log::debug!(
                    target: "media_index",
                    "enrichment of '{volume_id}' enriched {}",
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
