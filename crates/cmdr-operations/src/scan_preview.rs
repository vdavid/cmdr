//! Scan preview subsystem for the Copy dialog.
//!
//! Provides background scanning that feeds live stats to the frontend before
//! the actual copy starts. Results are cached so the copy can skip a redundant
//! scan.
//!
//! ## What a worker owes its preview
//!
//! Each worker ends by publishing a terminal OUTCOME through `settle_preview`
//! — complete with its result, errored with the typed failure a waiting
//! operation reports (`settle_failed_preview`), or cancelled — in the
//! same write that retires the in-flight state. An operation waiting on the preview may not
//! spawn for minutes behind a busy lane, so "look it up and find nothing" is
//! the common case rather than the rare one, and "nothing" would be ambiguous
//! four ways: complete-and-consumed, errored, cancelled, and never-existed.
//!
//! ⚠️ The `Cancelled` outcome comes from the worker's own cancel flag at its
//! exit, ❌ never from which event fired. A genuinely cancelled walk returns an
//! error (the local walk's `on_cancelled` string, the volume path's
//! `VolumeError::Cancelled`), so classifying on the event would reach the
//! operation as a FAILURE whose message happens to say "cancelled", and
//! recovering the truth from that message would be string-matching on the
//! control path.
//!
//! ## The progress bridge
//!
//! When an operation has claimed a preview, each progress tick is ALSO
//! republished under that operation's id as a scanning-phase `write-progress`
//! (`super::scan_bridge`). Both events keep firing: a pre-confirm dialog may
//! still be watching the same preview by `previewId`, and it has no operation
//! to watch instead.

//!
//! ## Layout
//!
//! This file holds the lifecycle API the IPC commands call (start, totals,
//! cancel) and the one settle both workers share on failure. The workers build
//! their own tallies and share nothing else: `local` walks the local filesystem
//! (with the compress-estimate sampler), and `volume` scans through a `Volume`
//! with the fresh-listing oracle in front.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use uuid::Uuid;

use super::Operations;
use super::event_sinks::ScanPreviewEventSink;
use super::scan_cache::{ScanOutcome, ScanPreviews};
use super::scan_watchdog::{SCAN_INACTIVITY_LIMIT, ScanTarget, ScanWatchdog};
use super::state::ScanPreviewState;
use super::types::{
    ScanPreviewErrorEvent, ScanPreviewPurpose, ScanPreviewStartResult, ScanPreviewTotals, WriteOperationError,
};
use cmdr_fs::volume::Volume;

#[cfg(not(target_family = "wasm"))]
mod local;
mod volume;

#[cfg(not(target_family = "wasm"))]
pub(super) use local::run_scan_preview;
#[cfg(test)]
pub(super) use volume::count_selected_links_as_links;
pub(super) use volume::entry_in_parent_listing;
#[cfg(test)]
pub(super) use volume::run_oracle_aware_batch_scan;
pub(super) use volume::run_volume_scan_preview;

impl Operations {
    /// Starts a scan preview for the Copy dialog, reporting through `events`.
    /// Returns a preview_id that can be used to cancel or to pass to copy_files.
    ///
    /// When `source_volume` is provided, uses `Volume::scan_for_copy()` instead of `std::fs`,
    /// enabling MTP and other non-local volumes to produce scan previews.
    ///
    /// `source_volume_id` identifies the volume the sources live on. It's used by the
    /// fresh-listing oracle (`Listings::authoritative_listing`) to short-circuit re-reading
    /// directories that an open pane is already keeping in sync. Pass `"root"` for
    /// local-FS scans.
    /// `purpose` says which dialog the walk serves. A transfer's walk also builds
    /// the operation's live tree (`super::transfer_tree`), carried to the
    /// operation in the cached result. A compress's LOCAL walk samples a
    /// compressed-size estimate (`compress_estimate`); a volume walk never samples.
    pub fn start_scan_preview(
        &self,
        events: Arc<dyn ScanPreviewEventSink>,
        sources: Vec<PathBuf>,
        source_volume: Option<Arc<dyn Volume>>,
        source_volume_id: String,
        progress_interval_ms: u64,
        purpose: ScanPreviewPurpose,
    ) -> ScanPreviewStartResult {
        let preview_id = Uuid::new_v4().to_string();
        let preview_id_clone = preview_id.clone();
        // `None` means "walk the local disk". A host with no local disk (the browser
        // demo) walks the volume its id names instead.
        let source_volume = source_volume.or_else(|| {
            let volumes = self.host().volumes();
            volumes
                .local_disk(&source_volume_id)
                .is_none()
                .then(|| volumes.get(&source_volume_id))
                .flatten()
        });

        let state = Arc::new(ScanPreviewState {
            cancelled: AtomicBool::new(false),
            progress_interval: Duration::from_millis(progress_interval_ms),
        });

        self.stores()
            .previews()
            .register_preview(preview_id.clone(), Arc::clone(&state));

        // Bounds the walk by INACTIVITY and narrates it in the log. Started here,
        // before either worker spawns, so a walk that wedges on its very first
        // syscall is still bounded — the workers only feed it.
        let watchdog = ScanWatchdog::start(
            self,
            preview_id.clone(),
            ScanTarget::of(&sources, &source_volume_id),
            SCAN_INACTIVITY_LIMIT,
            Arc::clone(&state),
            Arc::clone(&events),
        );

        // Spawn background task. The walk holds the engine until it settles.
        // A volume scan is async, so it runs on the engine's runtime; a local
        // scan walks `std::fs` on a thread of its own (no runtime needed).
        let engine = self.clone();
        if let Some(volume) = source_volume {
            self.host().runtime().spawn(async move {
                run_volume_scan_preview(
                    engine,
                    events,
                    preview_id_clone,
                    sources,
                    volume,
                    source_volume_id,
                    state,
                    purpose,
                    watchdog,
                )
                .await;
            });
        } else {
            #[cfg(target_family = "wasm")]
            {
                // No local disk here and no volume under the id: nothing to walk.
                if watchdog.claim_outcome() {
                    let path = sources.first().map(|p| p.display().to_string()).unwrap_or_default();
                    let error = WriteOperationError::SourceNoLongerConnected { path };
                    settle_failed_preview(self.stores().previews(), &*events, &watchdog, preview_id.clone(), error);
                }
            }
            #[cfg(not(target_family = "wasm"))]
            // The same panic a bare `thread::spawn` raises when the OS refuses a
            // thread; the name is what's new.
            std::thread::Builder::new()
                .name("cmdr-scan-preview".to_string())
                .spawn(move || {
                    run_scan_preview(&engine, events, preview_id_clone, sources, state, purpose, watchdog);
                })
                .expect("the OS starts the scan preview's thread");
        }

        ScanPreviewStartResult { preview_id }
    }

    /// Returns the cached totals from a completed scan preview, or `None` if the
    /// scan is still running, was cancelled, or errored. The FE uses this both to
    /// know whether the scan is done AND to recover its display state when the
    /// scan-preview events fired before listeners were attached (a real race
    /// surfaced by the watcher-backed oracle, which can complete a scan in
    /// ~5 ms — faster than the FE's `await startScanPreview()` IPC round-trip).
    pub fn get_scan_preview_totals(&self, preview_id: &str) -> Option<ScanPreviewTotals> {
        self.stores().previews().cached_scan_totals(preview_id)
    }

    /// Cancels a running scan preview AND frees any cached result.
    ///
    /// Cancelling sets the in-flight cancel flag (a still-running scan exits
    /// promptly). Freeing the cached result covers the dialog-dismissed-after-scan-
    /// completed case: the FE calls this on every dialog teardown, regardless of
    /// whether the scan was still running, so a completed-but-unconsumed
    /// `CachedScanResult` (tens of thousands of `FileInfo`) doesn't linger until
    /// quit. Consuming the result for a started op goes through
    /// `take_cached_scan_result` instead, which already removes it.
    ///
    /// ⚠️ A preview an OPERATION has claimed is left alone. Its owner decides when
    /// it ends (`cancel_operation` reaches it through the operation's own cancel
    /// token), and freeing it from a dialog teardown would pull the result out from
    /// under a transfer that is waiting on it.
    pub fn cancel_scan_preview(&self, preview_id: &str) {
        let previews = self.stores().previews();
        if let Some(owner) = previews.claimed_operation(preview_id) {
            log::debug!(
                target: "cmdr_operations",
                "scan preview {preview_id} is owned by operation {owner}; leaving it to its operation"
            );
            return;
        }
        if let Some(state) = previews.in_flight_state(preview_id) {
            state.cancelled.store(true, Ordering::Relaxed);
        }
        previews.release_preview(preview_id);
    }

    /// The cost facts of a settled preview, without consuming it. See
    /// [`ScanPreviews::cached_cost_facts`].
    pub fn cached_cost_facts(&self, preview_id: &str) -> Option<super::ScanCostFacts> {
        self.stores().previews().cached_cost_facts(preview_id)
    }
}

/// Publishes a walk that stopped on an error: the typed failure for a waiting
/// operation, and the dialog's notice. The dialog reads only `timed_out` off its
/// event, so the message there is the failure's technical form, for the log.
fn settle_failed_preview(
    previews: &ScanPreviews,
    events: &dyn ScanPreviewEventSink,
    watchdog: &ScanWatchdog,
    preview_id: String,
    error: WriteOperationError,
) {
    watchdog.note_settled("stopped");
    let message = format!("{error:?}");
    previews.settle_preview(&preview_id, ScanOutcome::Error(error), None);
    events.emit_error(ScanPreviewErrorEvent {
        preview_id,
        message,
        timed_out: false,
    });
}
