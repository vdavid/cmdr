//! The volume scan preview worker (MTP, SMB, archives, anything behind a
//! `Volume`), with the fresh-listing oracle in front of the backend's own
//! batch scan. The contract it owes: the parent module.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use super::super::event_sinks::ScanPreviewEventSink;
use super::super::scan::{SubtreeTotals, scan_subtree_with_oracle};
use super::super::scan_bridge::{ScanCounts, ScanPause, forward_scan_progress};
use super::super::scan_cache::{ScanOutcome, settle_preview};
use super::super::scan_watchdog::{ScanTally, ScanWatchdog};
use super::super::state::{CachedScanResult, ScanPreviewState};
use super::super::transfer::volume::{PathRole, map_volume_error};
use super::super::types::{
    ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewProgressEvent, WriteOperationError,
};
use super::settle_failed_preview;
use crate::file_system::listing::caching::try_get_authoritative_listing;
use crate::file_system::volume::{BatchScanResult, CopyScanResult, Volume};
use crate::ignore_poison::IgnorePoison;

/// The path a volume scan's failure names when the backend's error carries none
/// of its own (a `NotFound` does): the one source, or the folder a multi-item
/// selection shares.
fn scan_context_path(sources: &[PathBuf]) -> String {
    let context = match sources {
        [only] => Some(only.as_path()),
        [first, ..] => first.parent(),
        [] => None,
    };
    context.map(|p| p.display().to_string()).unwrap_or_default()
}

/// Runs a volume-based scan preview (for MTP and other non-local volumes).
///
/// Decision flow per parent group (sources sharing a parent directory):
/// - Fresh-listing oracle hit: cached entries supply size + `is_directory` for each selected child,
///   so the per-group `BatchScanResult` slice is built without any volume I/O for top-level files.
///   Top-level directories among the inputs recurse via `scan_subtree_with_oracle`, which
///   re-applies the oracle at every level (so a subfolder open in another pane is also
///   short-circuited).
/// - Oracle miss: falls through to `volume.scan_for_copy_batch_with_progress`, preserving the
///   cold-cache parent-grouping optimizations on MTP and the pipelined stat optimization on SMB.
///
/// Emits the same `scan-preview-progress` / `scan-preview-complete` events as
/// the pre-oracle code, so the FE dialog behavior is unchanged.
pub(in crate::file_system::write_operations) async fn run_volume_scan_preview(
    events: Arc<dyn ScanPreviewEventSink>,
    preview_id: String,
    sources: Vec<PathBuf>,
    volume: Arc<dyn Volume>,
    source_volume_id: String,
    state: Arc<ScanPreviewState>,
    watchdog: Arc<ScanWatchdog>,
) {
    // E2E only; see the local walk's matching delay.
    if let Some(ms) = crate::test_mode::e2e_scan_preview_delay_ms() {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    // Throttled progress emitter: the underlying MTP listing fires the callback
    // per entry (~60/s for 1047 files at ~17 ms each). We collapse those down to
    // ~5 events/s for the FE so the dialog's file count climbs smoothly without
    // flooding the IPC layer. Throttling lives in the closure rather than inside
    // each Volume impl so different backends share the same rate-limit policy.
    // The gate this walk parks on, at the same seams `is_cancelled` is polled.
    let pause = Arc::new(ScanPause::for_preview(
        preview_id.clone(),
        Arc::clone(&state),
        Arc::clone(&watchdog),
    ));

    let progress_state = Arc::new(std::sync::Mutex::new(Instant::now()));
    let pause_for_cb = Arc::clone(&pause);
    let state_for_cb = Arc::clone(&state);
    let events_for_cb = Arc::clone(&events);
    let watchdog_for_cb = Arc::clone(&watchdog);
    let preview_id_for_cb = preview_id.clone();
    let on_progress = move |p: crate::file_system::volume::ListingProgress| {
        if state_for_cb.cancelled.load(Ordering::Relaxed) {
            return;
        }
        // Fed BEFORE the 200 ms emit throttle: the watchdog asks whether the
        // volume is answering, and every entry the backend hands us answers
        // that, including the ones the UI throttle drops.
        watchdog_for_cb.note_progress(p.files, p.dirs, p.bytes);
        // A throttle timestamp: a poisoned one is still a timestamp.
        let mut last = progress_state.lock_ignore_poison();
        if last.elapsed() < Duration::from_millis(200) {
            return;
        }
        *last = Instant::now();
        drop(last);
        // Off the per-entry path (the throttle above owns that), which is where
        // the claim that names this walk's owner gets looked up.
        pause_for_cb.resolve_owner();
        events_for_cb.emit_progress(ScanPreviewProgressEvent {
            preview_id: preview_id_for_cb.clone(),
            files_found: p.files,
            dirs_found: p.dirs,
            bytes_found: p.bytes,
            current_path: None,
            current_dir: None,
            expected_files_total: None,
            expected_bytes_total: None,
            // See the matching note on this walk's complete event.
            online_only_found: false,
        });
        // Same counts under the owning operation's id; see the local walk's
        // matching forward.
        forward_scan_progress(
            &preview_id_for_cb,
            ScanCounts {
                files_found: p.files,
                dirs_found: p.dirs,
                bytes_found: p.bytes,
                ..ScanCounts::default()
            },
        );
    };

    // Cancellation predicate, captured by reference inside the async helpers.
    let state_for_cancel = Arc::clone(&state);
    let is_cancelled = move || state_for_cancel.cancelled.load(Ordering::Relaxed);

    let result: Result<BatchScanResult, WriteOperationError> = async {
        if state.cancelled.load(Ordering::Relaxed) {
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".to_string(),
            });
        }

        run_oracle_aware_batch_scan(
            volume.as_ref(),
            &source_volume_id,
            &sources,
            &is_cancelled,
            Some(&pause),
            &on_progress,
        )
        .await
        // The backend's typed error, kept typed: a `NotFound` names its own
        // path; anything without one names the first source.
        .map_err(|e| map_volume_error(&scan_context_path(&sources), PathRole::Source, e))
    }
    .await;

    // Extract stats from the result for the completion event
    let (total_files, total_dirs, total_bytes, dedup_bytes) = match &result {
        Ok(batch) => (
            batch.aggregate.file_count,
            batch.aggregate.dir_count,
            batch.aggregate.total_bytes,
            batch.aggregate.dedup_bytes,
        ),
        Err(_) => (0, 0, 0, 0),
    };

    // The cancel flag decides, not the result: a cancelled batch scan comes back
    // as `VolumeError::Cancelled` stringified into "Scan failed: …", so reading
    // the error would reach a waiting operation as a failure. Mirrors the local
    // walk's arms exactly.
    let cancelled = state.cancelled.load(Ordering::Relaxed);
    // See the local walk's matching guard: whoever claims the outcome first owns
    // publishing it, so a walk that comes back after the watchdog gave up stays
    // quiet instead of contradicting it.
    if !watchdog.claim_outcome() {
        return;
    }
    match result {
        _ if cancelled => {
            watchdog.note_settled("cancelled");
            settle_preview(&preview_id, ScanOutcome::Cancelled, None);
            events.emit_cancelled(ScanPreviewCancelledEvent { preview_id });
        }
        Ok(batch) => {
            // See the local walk's matching call: the batch's own totals, since a
            // backend that answers in one round trip never ticks progress.
            watchdog.note_completed(ScanTally {
                files: total_files,
                dirs: total_dirs,
                bytes: total_bytes,
            });
            // Cache results: volume scans don't produce per-file FileInfo, but
            // the cache stores aggregate stats AND per-path scan results so
            // copy_between_volumes can reuse both without re-statting.
            settle_preview(
                &preview_id,
                ScanOutcome::Complete,
                Some(
                    CachedScanResult::from_volume_batch(sources, total_files, total_bytes, dedup_bytes, batch.per_path)
                        .keeping_files(batch.files),
                ),
            );

            events.emit_complete(ScanPreviewCompleteEvent {
                preview_id,
                files_total: total_files,
                dirs_total: total_dirs,
                bytes_total: total_bytes,
                dedup_bytes_total: dedup_bytes,
                // Remote sources never sample: the estimate is suppressed.
                estimated_compressed_bytes: None,
                // `~/Library/CloudStorage` is local; a volume scan is MTP, SMB,
                // or an archive, where nothing is evicted to a File Provider.
                online_only_found: false,
            });
        }
        Err(error) => settle_failed_preview(&*events, &watchdog, preview_id, error),
    }
}

/// Oracle-aware batch scan: short-circuits parent directories that an open pane
/// is keeping watcher-fresh; falls through to the volume's own batch scan for
/// cold-cache parents. Builds a single merged `BatchScanResult` keyed back to
/// the caller's original `sources` slice (order matches input).
pub(in crate::file_system::write_operations) async fn run_oracle_aware_batch_scan(
    volume: &dyn Volume,
    volume_id: &str,
    sources: &[PathBuf],
    is_cancelled: &(dyn Fn() -> bool + Sync),
    pause: Option<&Arc<ScanPause>>,
    on_progress: &(dyn Fn(crate::file_system::volume::ListingProgress) + Sync),
) -> Result<BatchScanResult, crate::file_system::volume::VolumeError> {
    use crate::file_system::listing::FileEntry;
    use crate::file_system::volume::ListingProgress;
    use std::collections::HashMap;

    // Group sources by parent dir, preserving the input order of paths within
    // each group. The merged result puts the per-path entries back in original
    // input order (callers downstream don't currently depend on order, but it
    // matches `BatchScanResult::per_path`'s documented contract).
    let mut groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    let mut group_order: Vec<PathBuf> = Vec::new();
    for source in sources {
        let parent = source
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("/"));
        if !groups.contains_key(&parent) {
            group_order.push(parent.clone());
        }
        groups.entry(parent).or_default().push(source.clone());
    }

    let mut aggregate = CopyScanResult {
        file_count: 0,
        dir_count: 0,
        total_bytes: 0,
        dedup_bytes: 0,
        // Aggregate across multiple paths — meaningless, per the BatchScanResult contract.
        top_level_is_directory: false,
        top_level_modified_at: None,
    };
    let mut per_path_unordered: HashMap<PathBuf, CopyScanResult> = HashMap::new();
    // Every file's size and date, for a cost estimate, while every group came
    // back with them (a backend that keeps files, walked cold). One group that
    // didn't makes the whole list partial, and a partial list would price low.
    let mut files: Option<Vec<crate::file_system::volume::ScannedFile>> = Some(Vec::new());
    // Batch-scoped hardlink dedup for the source-footprint number. Shared
    // across all groups so a hardlink spanning two selected sources counts
    // once. Only `LocalPosixVolume` cached entries carry inodes; other
    // backends leave them `None` (no dedup, source == write footprint).
    let mut seen_inodes: HashSet<u64> = HashSet::new();

    for parent in &group_order {
        if is_cancelled() {
            return Err(crate::file_system::volume::VolumeError::Cancelled(
                "Operation cancelled by user".to_string(),
            ));
        }
        // The park points on the volume path are the ones cancel already has:
        // per entry inside `scan_subtree_with_oracle`, here per source group, and
        // — through the boundary handed to the cold-cache branch below — inside
        // the backend's own walk.
        if let Some(pause) = pause {
            pause.park_while_paused_async().await;
        }
        let paths_in_group = groups
            .get(parent)
            .expect("group_order tracks every parent inserted into groups");

        if let Some(cached_entries) = try_get_authoritative_listing(volume_id, parent) {
            files = None;
            log::debug!(
                "scan-preview: oracle hit for parent {} ({} cached entries, {} selected children)",
                parent.display(),
                cached_entries.len(),
                paths_in_group.len()
            );
            // Index cached entries by their last path component so we can resolve
            // selected paths without a per-path linear search. `path` on a cached
            // FileEntry is the absolute path string, so the file name component
            // is the disambiguator within this parent.
            let by_name: HashMap<String, &FileEntry> = cached_entries
                .iter()
                .filter_map(|e| {
                    PathBuf::from(&e.path)
                        .file_name()
                        .map(|n| (n.to_string_lossy().to_string(), e))
                })
                .collect();

            for source in paths_in_group {
                let name = source
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let Some(entry) = by_name.get(&name) else {
                    // Cache doesn't know this child. Could be a stale selection
                    // (entry deleted out-of-band) or a name encoding mismatch.
                    // Either way, fall through to a real stat for safety.
                    let scan = volume.scan_for_copy(source).await?;
                    aggregate.file_count += scan.file_count;
                    aggregate.dir_count += scan.dir_count;
                    aggregate.total_bytes += scan.total_bytes;
                    aggregate.dedup_bytes += scan.dedup_bytes;
                    per_path_unordered.insert(source.clone(), scan);
                    on_progress(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    });
                    continue;
                };

                if entry.is_directory && !entry.is_symlink {
                    // `scan_subtree_with_oracle` emits counts local to this
                    // subtree (starting at 1). Shift by the current aggregate
                    // so the FE display stays cumulative across multiple
                    // top-level dirs in this call — files, dirs, AND bytes.
                    // Scan-phase byte baseline is dedup'd (converges with the
                    // index estimate); the headline write footprint is tracked
                    // separately in `aggregate.total_bytes`.
                    let baseline = ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    };
                    let shifted = |p: ListingProgress| {
                        on_progress(ListingProgress {
                            files: baseline.files + p.files,
                            dirs: baseline.dirs + p.dirs,
                            bytes: baseline.bytes + p.bytes,
                        })
                    };
                    let subtree: SubtreeTotals = scan_subtree_with_oracle(
                        volume,
                        volume_id,
                        source,
                        is_cancelled,
                        pause.map(Arc::as_ref),
                        Some(&shifted),
                        &mut seen_inodes,
                    )
                    .await?;
                    aggregate.file_count += subtree.file_count;
                    // `scan_for_copy_batch`'s aggregate.dir_count counts descendants
                    // only, not the top-level path itself. Match that convention
                    // so the FE's "X dirs" number is consistent across paths.
                    aggregate.dir_count += subtree.dir_count;
                    aggregate.total_bytes += subtree.total_bytes;
                    aggregate.dedup_bytes += subtree.dedup_bytes;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: subtree.file_count,
                            dir_count: subtree.dir_count,
                            total_bytes: subtree.total_bytes,
                            dedup_bytes: subtree.dedup_bytes,
                            top_level_is_directory: true,
                            top_level_modified_at: entry.modified_at,
                        },
                    );
                } else {
                    let size = entry.size.unwrap_or(0);
                    // Top-level cached file: dedupe by inode for the source
                    // footprint. Single top-level files rarely collide, but a
                    // hardlink also selected inside a sibling dir would.
                    let dedup_contribution = match entry.inode {
                        Some(ino) if !seen_inodes.insert(ino) => 0,
                        _ => size,
                    };
                    aggregate.file_count += 1;
                    aggregate.total_bytes += size;
                    aggregate.dedup_bytes += dedup_contribution;
                    per_path_unordered.insert(
                        source.clone(),
                        CopyScanResult {
                            file_count: 1,
                            dir_count: 0,
                            total_bytes: size,
                            dedup_bytes: dedup_contribution,
                            top_level_is_directory: false,
                            top_level_modified_at: entry.modified_at,
                        },
                    );
                    on_progress(ListingProgress {
                        files: aggregate.file_count,
                        dirs: aggregate.dir_count,
                        bytes: aggregate.dedup_bytes,
                    });
                }
            }
        } else {
            // Cold cache for this parent. Delegate to the volume's own batch
            // scan: it preserves the MTP parent-grouping and SMB pipelined-stat
            // optimizations for cold paths.
            //
            // The volume's callback reports counts LOCAL to its current
            // `list_directory` call (starts at 1 for files, 0 for dirs/bytes
            // until entries are enumerated). Shift by the current aggregate
            // before forwarding so the FE display stays cumulative as we walk
            // multiple parent groups — without this, every new group's first
            // entry drops the visible counts back to local values, then climbs
            // to the group's local totals before the next group restarts.
            // Cold-cache backends (MTP, SMB) report dedup_bytes == total_bytes
            // (no hardlinks), so the dedup'd baseline matches their stream.
            let baseline = ListingProgress {
                files: aggregate.file_count,
                dirs: aggregate.dir_count,
                bytes: aggregate.dedup_bytes,
            };
            let shifted = |p: ListingProgress| {
                on_progress(ListingProgress {
                    files: baseline.files + p.files,
                    dirs: baseline.dirs + p.dirs,
                    bytes: baseline.bytes + p.bytes,
                })
            };
            // ❗ The boundary carries this walk's stop into the backend, so a
            // cold-cache group — one call that can be minutes over a sleeping
            // share — answers Cancel and Pause from inside rather than when it
            // ends. Both routes to "stop" ride along: the operation that claimed
            // this preview, and the preview's own flag (`ScanPause`).
            let mut group_boundary = crate::file_system::volume::ScanBoundary::new(Some(&shifted));
            if let Some(pause) = pause {
                group_boundary = group_boundary.stopping_at(crate::file_system::volume::ScanStop::new(
                    Arc::clone(pause) as Arc<dyn crate::file_system::volume::ScanStopSignal>,
                ));
            }
            let group_result = volume
                .scan_for_copy_batch_with_boundary(paths_in_group, &group_boundary)
                .await?;
            aggregate.file_count += group_result.aggregate.file_count;
            aggregate.dir_count += group_result.aggregate.dir_count;
            aggregate.total_bytes += group_result.aggregate.total_bytes;
            aggregate.dedup_bytes += group_result.aggregate.dedup_bytes;
            for (path, scan) in group_result.per_path {
                per_path_unordered.insert(path, scan);
            }
            files = match (files, group_result.files) {
                (Some(mut so_far), Some(more)) => {
                    so_far.extend(more);
                    Some(so_far)
                }
                _ => None,
            };
        }
    }

    // Rebuild per_path in caller's original source order. Missing entries
    // (shouldn't happen, but be defensive) are skipped silently.
    let per_path: Vec<(PathBuf, CopyScanResult)> = sources
        .iter()
        .filter_map(|src| per_path_unordered.remove(src).map(|scan| (src.clone(), scan)))
        .collect();

    Ok(BatchScanResult {
        aggregate,
        per_path,
        files,
    })
}
