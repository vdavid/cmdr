//! The local-filesystem scan preview worker: the walk, and the compress-size
//! sampler that rides alongside it. The contract it owes: the parent module.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use super::super::Operations;
use super::super::error_classification::classify_io_error;
use super::super::event_sinks::ScanPreviewEventSink;
use super::super::mount_boundary::refused_at;
use super::super::scan_bridge::{ScanCounts, ScanPause, forward_scan_progress};
use super::super::scan_cache::ScanOutcome;
use super::super::scan_walker::{OnlineOnlyWatch, WalkContext, walk_sources_with_per_path};
use super::super::scan_watchdog::{ScanTally, ScanWatchdog};
use super::super::state::{CachedScanResult, FileInfo, ScanPreviewState};
use super::super::transfer_tree::{TreeOrder, from_local_scan};
use super::super::types::{
    MountedVolumeAction, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewProgressEvent,
    ScanPreviewPurpose, WriteOperationError,
};
use super::settle_failed_preview;
use cmdr_fs::volume::CopyScanResult;

/// Internal function that runs the scan preview in a background thread.
///
/// A [`ScanPreviewPurpose::Transfer`] walk also builds the transfer's live tree
/// from what it found, after the walk and off it, so the operation consuming
/// the preview starts with it (and every `FileInfo` knows its node).
///
/// A [`ScanPreviewPurpose::Compress`] walk samples: a budget-capped worker
/// thread computes a compressed-size estimate off the walk thread: the walk
/// pushes `(path, size)` per regular file into a channel, the worker samples a
/// head window under a byte budget (see `compress_estimate`), and the estimate
/// rides the complete event. The worker is joined after the walk (usually
/// already done, since it ran concurrently) and cancels with the scan; a
/// sampling failure degrades to "no estimate" and never affects the scan.
pub(crate) fn run_scan_preview(
    engine: &Operations,
    events: Arc<dyn ScanPreviewEventSink>,
    preview_id: String,
    sources: Vec<PathBuf>,
    state: Arc<ScanPreviewState>,
    purpose: ScanPreviewPurpose,
    watchdog: Arc<ScanWatchdog>,
) {
    use super::super::compress_estimate::CompressEstimator;

    let sample_for_estimate = purpose == ScanPreviewPurpose::Compress;
    let host = engine.host();
    let previews = engine.stores().previews();

    // E2E only: a deterministic scanning window for the specs that have to act
    // while a transfer is still counting. Unset in production.
    if let Some(delay) = host.pacing().scan_preview_delay() {
        std::thread::sleep(delay);
    }

    let mut files: Vec<FileInfo> = Vec::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    // Write footprint (un-dedup'd) and `du`-equivalent source footprint. The
    // dialog shows the first as the headline transfer size, the second as
    // hardlink context. See `walk_dir_recursive`.
    let mut total_bytes = 0u64;
    let mut dedup_bytes = 0u64;
    let mut last_progress_time = Instant::now();
    let mut visited = HashSet::new();
    // Shared across sources so hardlinks crossing source roots count once,
    // matching `indexing/scanner.rs`'s dir_stats aggregation policy.
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    // Index-derived expected totals: lets the UI render a real progress bar
    // from the first scan event instead of an indeterminate spinner. `None`
    // when any source isn't covered by the index.
    let expected = host.index().expected_totals(&sources);

    // Online-only tally, armed only when the sources reach into a cloud drive we
    // know by name. A selected FOLDER can't carry `SF_DATALESS` itself, so this
    // walk is what tells the delete confirmation whether a trash there would pull
    // the whole subtree back down from the provider. Everywhere else it stays
    // `None` and the walk is byte-for-byte what it was.
    // See `delete/cloud_trash.rs` for why the download, not the refusal, is the reason.
    let online_only_seen = AtomicBool::new(false);
    let watch_online_only = super::super::delete::cloud_trash::selection_touches_known_cloud_drive(&sources);

    // Compress-size estimator: a budget-capped worker samples file heads OFF the walk
    // thread so the sampling CPU never lands on the scan's critical path. The
    // per-file hook below pushes `(path, size)` into the channel; the worker
    // deflates a head window under a byte budget and accumulates the estimate.
    // `None` for non-compress scans (the estimate is suppressed). Cancels with
    // the scan via the shared `cancelled` flag.
    let (estimate_tx, estimate_worker) = if sample_for_estimate {
        let (tx, rx) = std::sync::mpsc::channel::<(PathBuf, u64)>();
        let cancel = Arc::clone(&state);
        // The same panic a bare `thread::spawn` raises when the OS refuses a
        // thread; the name is what's new.
        let handle = std::thread::Builder::new()
            .name("cmdr-compress-estimate".to_string())
            .spawn(move || {
                let mut estimator = CompressEstimator::new();
                while let Ok((path, size)) = rx.recv() {
                    if cancel.cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    estimator.observe(&path, size);
                }
                estimator.finish()
            })
            .expect("the OS starts the compress estimate's thread");
        (Some(tx), Some(handle))
    } else {
        (None, None)
    };

    // The gate this walk parks on, once an operation claims the preview. Its
    // owner is looked up at the progress tick below, never per entry.
    let pause = ScanPause::for_preview(
        engine.clone(),
        preview_id.clone(),
        Arc::clone(&state),
        Arc::clone(&watchdog),
    );

    // One `CopyScanResult` per top-level source, filled by the walk below. The
    // copy engine reads it back as its `source_hints`, so a source that isn't in
    // here reaches the drivers as "unknown" and costs them a stat probe.
    let mut per_path: Vec<(PathBuf, CopyScanResult)> = Vec::new();

    let result: Result<(), WriteOperationError> = (|| {
        // Cheap per-file hook: a channel push, so it never delays the walk. A
        // dropped receiver (worker gone) just drops the sample (best-effort). The
        // channel is deliberately UNBOUNDED: a sync_channel would block the walk
        // when the sampler falls behind (e.g. the oracle-cached fast-walk case),
        // which violates the never-touch-the-critical-path contract. The queue is
        // small in practice — post-budget the worker drains via hashmap lookups.
        let send_sample = |path: &Path, size: u64| {
            if let Some(tx) = &estimate_tx {
                let _ = tx.send((path.to_path_buf(), size));
            }
        };
        // Given the walk's own `lstat` the flag is free; a cached entry from the
        // oracle has no metadata, so that one costs a stat of its own.
        let online_only_probe = |path: &Path, metadata: Option<&std::fs::Metadata>| match metadata {
            Some(metadata) => super::super::delete::cloud_trash::metadata_is_online_only(metadata),
            None => super::super::delete::cloud_trash::is_online_only(path),
        };
        let online_only_watch = OnlineOnlyWatch {
            probe: &online_only_probe,
            seen: &online_only_seen,
        };
        let sidecars = cmdr_fs::apple_double::SidecarRule::new();
        let refuse_mounted_volume = |path: &Path| refused_at(path, MountedVolumeAction::Delete);
        let ctx = WalkContext {
            sidecars: &sidecars,
            progress_interval: state.progress_interval,
            is_cancelled: &|| state.cancelled.load(Ordering::Relaxed),
            park_while_paused: &|| pause.park_while_paused(),
            // Typed and naming the entry, because a confirmed transfer waiting on
            // this preview reports exactly this as its own failure.
            on_io_error: &|path, e| classify_io_error(&e, path.display().to_string()),
            on_cancelled: &|| WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            },
            on_symlink_loop: &|path| WriteOperationError::SymlinkLoop {
                path: path.display().to_string(),
            },
            // A delete's preview stops where its delete would: the delete reuses
            // this walk's file list, so a list that crossed a mount would be
            // deleted as is.
            on_mount_boundary: (purpose == ScanPreviewPurpose::Delete).then_some(&refuse_mounted_volume),
            on_progress: &|files_found, dirs_found, bytes_found, current_path, current_dir| {
                // Proof the walk is alive, fed before the emits so a slow sink
                // can't read as a wedged volume.
                watchdog.note_progress(files_found, dirs_found, bytes_found);
                // The tick is the walk's one point off its per-entry path, so
                // it is where the claim (which may land mid-walk) is looked up.
                pause.resolve_owner();
                events.emit_progress(ScanPreviewProgressEvent {
                    preview_id: preview_id.to_string(),
                    files_found,
                    dirs_found,
                    bytes_found,
                    current_path: current_path.clone(),
                    current_dir: current_dir.clone(),
                    expected_files_total: expected.map(|e| e.files),
                    expected_bytes_total: expected.map(|e| e.bytes),
                    // Read per tick, not per entry: one hit is the whole answer,
                    // so the dialog can flip to the permanent delete long before
                    // a big tree finishes counting.
                    online_only_found: online_only_seen.load(Ordering::Relaxed),
                });
                // Same counts under the owning operation's id, so a confirmed
                // transfer's queue row, chip, and dialog stay live through the
                // walk instead of sitting at zero.
                forward_scan_progress(
                    engine,
                    &preview_id,
                    ScanCounts {
                        files_found,
                        dirs_found,
                        bytes_found,
                        current_path,
                        current_dir,
                        expected_files_total: expected.map(|e| e.files),
                        expected_bytes_total: expected.map(|e| e.bytes),
                    },
                );
            },
            on_file: sample_for_estimate.then_some(&send_sample as &dyn Fn(&Path, u64)),
            online_only: watch_online_only.then_some(&online_only_watch),
            listings: host.listings(),
        };
        // Local FS scan preview uses the "root" volume ID. The oracle short-circuits
        // any subtree currently open in a pane with a live FSEvents watcher.
        let volume_id = Some(cmdr_fs::volume::DEFAULT_VOLUME_ID);
        per_path = walk_sources_with_per_path(
            &sources,
            &mut files,
            &mut dirs,
            &mut total_bytes,
            &mut dedup_bytes,
            &mut last_progress_time,
            &mut visited,
            &mut seen_inodes,
            volume_id,
            &ctx,
        )?;
        Ok(())
    })();

    // Close the channel (drop the only sender) so the worker drains and returns,
    // then collect the estimate. The worker ran concurrently with the walk, so
    // this join is usually already done; a sampling panic degrades to `None`.
    drop(estimate_tx);
    let estimate = estimate_worker.and_then(|handle| handle.join().ok());

    // The cancel flag, not the shape of `result`, decides whether this was a
    // cancel: a cancelled walk unwinds through `on_cancelled` into the Err arm,
    // so reading the error would reach a waiting operation as a failure whose
    // message merely says "cancelled".
    let cancelled = state.cancelled.load(Ordering::Relaxed);
    // The watchdog may have already given this preview an outcome and told the
    // dialog about it. Publishing a second one would resurrect a spinner the
    // user has already been told is over, or overwrite a timeout with a result
    // nobody is waiting for any more.
    if !watchdog.claim_outcome() {
        return;
    }
    match result {
        _ if cancelled => {
            watchdog.note_settled("cancelled");
            previews.settle_preview(&preview_id, ScanOutcome::Cancelled, None);
            events.emit_cancelled(ScanPreviewCancelledEvent { preview_id });
        }
        Ok(()) => {
            // A tree bug builds none, and the operation builds its own (or none) later.
            let built = (purpose == ScanPreviewPurpose::Transfer)
                .then(|| {
                    let order = TreeOrder::active(host);
                    from_local_scan(
                        &sources,
                        &dirs,
                        files.iter().map(|file| (file.path.as_path(), file.size)),
                        &order,
                    )
                })
                .flatten();
            let tree = built.map(|(tree, nodes)| {
                for (file, node) in files.iter_mut().zip(nodes) {
                    file.tree_node = node;
                }
                tree
            });
            let file_count = files.len();
            let dirs_count = dirs.len();
            // The walk's own totals, not the watchdog's tick counters: a preview
            // that finishes inside one progress interval never fed those.
            watchdog.note_completed(ScanTally {
                files: file_count,
                dirs: dirs_count,
                bytes: total_bytes,
            });
            previews.settle_preview(
                &preview_id,
                ScanOutcome::Complete,
                Some(
                    CachedScanResult::from_local_walk(
                        purpose,
                        sources,
                        files,
                        dirs,
                        total_bytes,
                        dedup_bytes,
                        per_path,
                        estimate.clone(),
                    )
                    .with_tree(tree),
                ),
            );

            // Emit completion
            events.emit_complete(ScanPreviewCompleteEvent {
                preview_id,
                files_total: file_count,
                dirs_total: dirs_count,
                bytes_total: total_bytes,
                dedup_bytes_total: dedup_bytes,
                estimated_compressed_bytes: estimate,
                online_only_found: online_only_seen.load(Ordering::Relaxed),
            });
        }
        Err(error) => settle_failed_preview(previews, &*events, &watchdog, preview_id, error),
    }
}
