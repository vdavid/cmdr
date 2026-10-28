//! The volume-aware delete (MTP, SMB, a `LocalPosixVolume` folder, anything
//! behind a `Volume`): an oracle-aware recursive listing, then `Volume::delete`
//! per file or `delete_files` per batch. The local-FS twin is `walker.rs`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use super::super::event_sinks::OperationEventSink;
use super::super::state::WriteOperationState;
use super::super::transfer::volume_outage::StartedOn;
use super::super::types::{
    CancelRollback, DryRunResult, SourceItemOutcome, WriteCancelledEvent, WriteCompleteEvent, WriteOperationConfig,
    WriteOperationError, WriteOperationPhase, WriteOperationType, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use super::outage::{EntryStop, Listed, delete_entry};
use super::volume_scan::{VolumeDeleteEntry, VolumeScanTracker, listed_in_parent_listing, scan_volume_recursive};
use cmdr_fs::volume::{EntryKind, Volume};

/// Counts each top-level source's entries down as the delete removes them, so
/// `write-source-item-done` fires once per source, after its LAST entry (a
/// folder's own, which goes after its files). The local walker's
/// `SourceItemTracker` does the same over its flat file list; this one keys on
/// the scan's entries, which include the folders.
///
/// ❗ Without it a volume delete reported no source at all, and a caller that
/// tracks outcomes per source (an approved AI suggestion's ops) read every one
/// as never reached.
struct SourceCountdown {
    remaining: std::collections::HashMap<PathBuf, usize>,
    /// Sources a folder of which the delete left standing (best effort: a
    /// folder still holding something refuses, and stays).
    kept: std::collections::HashSet<PathBuf>,
}

impl SourceCountdown {
    fn new(sources: &[PathBuf], entries: &[VolumeDeleteEntry]) -> Self {
        let mut remaining = std::collections::HashMap::new();
        for source in sources {
            remaining.insert(source.clone(), 0);
        }
        let mut counted = Self {
            remaining,
            kept: std::collections::HashSet::new(),
        };
        for entry in entries {
            if let Some(source) = counted.source_of(&entry.path)
                && let Some(count) = counted.remaining.get_mut(&source)
            {
                *count += 1;
            }
        }
        counted
    }

    /// The top-level source `path` sits under (or is): the nearest ancestor
    /// that's a source. Bounded by the path's depth, never by the source count.
    fn source_of(&self, path: &std::path::Path) -> Option<PathBuf> {
        path.ancestors()
            .find(|ancestor| self.remaining.contains_key(*ancestor))
            .map(std::path::Path::to_path_buf)
    }

    /// One entry of a source is done (`removed` false: a folder that stayed).
    /// Emits the source's event when that was its last entry.
    fn entry_done(
        &mut self,
        events: &dyn OperationEventSink,
        operation_id: &str,
        path: &std::path::Path,
        removed: bool,
    ) {
        let Some(source) = self.source_of(path) else {
            return;
        };
        if !removed {
            self.kept.insert(source.clone());
        }
        let Some(count) = self.remaining.get_mut(&source) else {
            return;
        };
        *count = count.saturating_sub(1);
        if *count > 0 {
            return;
        }
        self.remaining.remove(&source);
        events.emit_source_item_done(WriteSourceItemDoneEvent {
            operation_id: operation_id.to_string(),
            source_path: source.display().to_string(),
            // Delete has no Skip: every file under it is unlinked. Only its own
            // folder can have stayed.
            source_removed: !self.kept.contains(&source),
            outcome: SourceItemOutcome::Done,
        });
    }
}

/// Deletes `batch` in one `delete_files` call, then takes each file the call
/// couldn't delete into its own outage loop (`outage::delete_entry`), starting
/// at the wait: a file the drop cut off is re-checked and deleted once the
/// share is back. After the first failure for good, the rest keep the batch's
/// own answer: the delete ends there anyway.
async fn delete_batch(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    batch: &[&VolumeDeleteEntry],
) -> Vec<Result<(), EntryStop>> {
    let paths: Vec<PathBuf> = batch.iter().map(|entry| entry.path.clone()).collect();
    let started_on = StartedOn::now(&[volume]);
    let answers = volume.delete_files(&paths).await;
    let mut results = Vec::with_capacity(batch.len());
    let mut stopped = false;
    for (entry, answer) in batch.iter().zip(answers) {
        let result = match answer {
            Ok(()) => Ok(()),
            Err(error) if stopped => Err(EntryStop::Failed { error, lost: None }),
            Err(error) => {
                delete_entry(
                    volume,
                    state,
                    &entry.path,
                    entry.listed,
                    Some((error, started_on.clone())),
                )
                .await
            }
        };
        stopped |= result.is_err();
        results.push(result);
    }
    results
}

/// Emits `write-cancelled` when `result` is `Err(Cancelled)`. Use this after
/// every top-level `scan_volume_recursive` call so the FE sees the proper
/// terminal event before the error propagates up. See `scan_volume_recursive`'s
/// "Cancel contract" doc for why this is the caller's responsibility.
fn emit_cancelled_if_aborted(
    result: &Result<(), WriteOperationError>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    tracker: &VolumeScanTracker,
) {
    if matches!(result, Err(WriteOperationError::Cancelled { .. })) {
        events.emit_cancelled(WriteCancelledEvent {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_processed: tracker.files_so_far.load(std::sync::atomic::Ordering::Relaxed),
            rollback: CancelRollback::none(),
        });
    }
}

/// Deletes files on a non-local volume (like MTP) with progress reporting.
///
/// Uses `volume.list_directory()` for scanning, and `volume.delete()` per file or
/// `volume.delete_files()` per batch where the backend batches
/// (`Volume::delete_batch_size`), then `volume.delete_directory()` per folder.
/// Emits the same events as `delete_files_with_progress` so the frontend progress
/// dialog works unchanged.
#[allow(
    clippy::too_many_arguments,
    reason = "Matches the parameter pattern of other write operation functions"
)]
pub(crate) async fn delete_volume_files_with_progress_inner(
    volume: Arc<dyn Volume>,
    volume_id: &str,
    events: &dyn OperationEventSink,
    operation_id: &str,
    state: &Arc<WriteOperationState>,
    sources: &[PathBuf],
    config: &WriteOperationConfig,
) -> Result<(), WriteOperationError> {
    use std::sync::atomic::Ordering;

    // Phase 1: Scan. Recursively enumerate the tree via volume.list_directory().
    // Fast path: if the scan preview already enumerated the top-level shape (file
    // vs dir + size per source), reuse it. The per-source `top_level_is_directory`
    // and `total_bytes` flags come from `CachedScanResult::per_path` populated by
    // `run_volume_scan_preview`. We still need to recurse into top-level dirs to
    // get the per-file paths delete needs — but the recursion's walker is now
    // oracle-aware, so a watched subtree skips the `list_directory` round-trip.
    let previews = state.scope().stores().previews();
    let cached_scan = config
        .preview_id
        .as_deref()
        .and_then(|preview_id| previews.take_cached_scan_result(preview_id, sources));

    let mut entries: Vec<VolumeDeleteEntry> = Vec::new();
    let mut total_bytes = 0u64;
    // Operation-scoped hardlink dedup. Shared across all top-level sources
    // so a hardlink that spans two different sources still counts once
    // (matches the local-FS walker's contract — `seen_inodes` in
    // `scan.rs::walk_dir_recursive`). Empty for backends without inode info.
    let mut seen_inodes: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let tracker = Arc::new(VolumeScanTracker::new(state.progress_interval));
    let mut last_scan_emit = Instant::now();

    if let Some(scan) = cached_scan.as_ref() {
        log::debug!(
            "delete_volume_files_with_progress: reused cached scan for operation_id={}, files={}, bytes={}, per_path={}",
            operation_id,
            scan.file_count,
            scan.total_bytes,
            scan.per_path.len()
        );

        // Index cached per-path results by source path so we can look up
        // `top_level_is_directory` and `total_bytes` for each input source.
        let by_path: std::collections::HashMap<PathBuf, &cmdr_fs::volume::CopyScanResult> =
            scan.per_path.iter().map(|(p, r)| (p.clone(), r)).collect();

        for source in sources {
            if state.intent().is_cancelled() {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: 0,
                    rollback: CancelRollback::none(),
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".into(),
                });
            }

            match by_path.get(source) {
                Some(per_path_scan) if !per_path_scan.top_level_is_directory => {
                    // Top-level file: size comes straight from the cache. No
                    // `is_directory` probe and no `list_directory` round-trip.
                    // No inode info on the cache-hit path (`CopyScanResult`
                    // doesn't carry it), so each top-level cached file is
                    // treated as a unique inode. Cross-source hardlinks at
                    // the top level aren't dedup'd; within-source recursion
                    // still dedupes via the walker's `seen_inodes`.
                    let size = per_path_scan.total_bytes;
                    tracker.files_so_far.fetch_add(1, Ordering::Relaxed);
                    tracker.bytes_so_far.fetch_add(size, Ordering::Relaxed);
                    total_bytes += size;
                    entries.push(VolumeDeleteEntry {
                        path: source.to_path_buf(),
                        progress_bytes: size,
                        is_dir: false,
                        listed: Listed {
                            kind: EntryKind::File,
                            size: Some(size),
                        },
                    });
                }
                Some(_) => {
                    // Top-level dir per the preview: recurse via the oracle-aware
                    // walker. Any subtree that's open in another pane and
                    // watcher-fresh gets cache-fed inside the walker.
                    //
                    // ❌ Don't hand it `Some(Directory)` on the preview's word: a
                    // preview's top-level stat may follow a link (a copy's does,
                    // by design), so "directory" can be a link to one. The parent
                    // listing answers link-aware with no I/O; failing that, the
                    // walker asks `entry_kind`.
                    let result = scan_volume_recursive(
                        &volume,
                        volume_id,
                        source,
                        listed_in_parent_listing(state, volume.as_ref(), volume_id, source),
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
                None => {
                    // Scan preview was non-volume (local-FS) or didn't include
                    // this source. Fall back to the no-preview shape for this
                    // path: the oracle-aware walker resolves the type.
                    //
                    // ❌ Don't "fix" the `None` here into a default. It looks
                    // like the bug this module's guardrails are about and is the
                    // opposite: forwarding `hint: None` is what makes
                    // `scan_volume_recursive` PROPAGATE a failed probe
                    // (`map_err(...)?`) instead of guessing. A `Some(File)`
                    // here would be the guess.
                    let result = scan_volume_recursive(
                        &volume,
                        volume_id,
                        source,
                        None,
                        &mut entries,
                        &mut total_bytes,
                        &mut seen_inodes,
                        state,
                        events,
                        operation_id,
                        &tracker,
                    )
                    .await;
                    emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
                    result?;
                }
            }

            // Even on the all-files fast path, throttle a single scan-progress
            // emit per interval so the FE dialog shows movement during the
            // entry-list build. Without this the user sees "Scanning…" frozen
            // until the actual delete phase starts.
            if last_scan_emit.elapsed() >= state.progress_interval {
                let files_done = tracker.files_so_far.load(Ordering::Relaxed);
                let dirs_done = tracker.dirs_so_far.load(Ordering::Relaxed);
                let bytes_done = tracker.bytes_so_far.load(Ordering::Relaxed);
                state.emit_progress_via_sink(
                    events,
                    WriteProgressEvent::new(
                        operation_id.to_string(),
                        WriteOperationType::Delete,
                        WriteOperationPhase::Scanning,
                        None,
                        files_done,
                        0,
                        bytes_done,
                        0,
                    )
                    .with_scan_meta(Some(source.display().to_string()), dirs_done, None),
                );
                state.scope().status().update(
                    operation_id,
                    WriteOperationPhase::Scanning,
                    None,
                    files_done,
                    0,
                    bytes_done,
                    0,
                );
                last_scan_emit = Instant::now();
            }
        }
    } else {
        for source in sources {
            // Oracle-first parent lookup: if the parent's listing is watched
            // (open in some pane), take the entry's kind and size from there
            // instead of probing the volume, which on MTP lists the parent and
            // on S3 costs a HEAD the cost estimate never counted. Falls through
            // to the trait probe when the oracle can't answer.
            let parent_hint = listed_in_parent_listing(state, volume.as_ref(), volume_id, source);

            // Hand the hint (or the lack of one) straight to the walker: it
            // resolves an absent hint with a PROPAGATING probe and handles both
            // the directory and the top-level-file case. Resolving it here
            // instead would mean owning a second answer to the same question,
            // and the obvious way to write that second answer is
            // `.unwrap_or(false)`, which turns an unanswerable stat into a
            // confident "file". Then the entry goes in as `is_dir: false` with
            // zero bytes, so the dialog describes one file and no bytes for
            // what may be a whole tree, and the `delete` that follows acts on a
            // fact nobody established. A source that can't be stat'd fails that
            // item instead, which is the honest outcome and cheap to retry.
            let result = scan_volume_recursive(
                &volume,
                volume_id,
                source,
                parent_hint,
                &mut entries,
                &mut total_bytes,
                &mut seen_inodes,
                state,
                events,
                operation_id,
                &tracker,
            )
            .await;
            emit_cancelled_if_aborted(&result, events, operation_id, &tracker);
            result?;
        }
    }

    let file_count = entries.iter().filter(|e| !e.is_dir).count();
    let dirs_count = entries.iter().filter(|e| e.is_dir).count();

    // Emit final scan progress (scan complete: tallies == totals)
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            WriteOperationType::Delete,
            WriteOperationPhase::Scanning,
            None,
            file_count,
            file_count,
            total_bytes,
            total_bytes,
        )
        .with_scan_meta(None, dirs_count, None),
    );

    // Handle dry-run mode
    if config.dry_run {
        events.emit_dry_run_complete(DryRunResult {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Delete,
            files_total: file_count,
            bytes_total: total_bytes,
            conflicts_total: 0,
            conflicts: Vec::new(),
            conflicts_sampled: false,
        });
        return Ok(());
    }

    // The walk asked each directory's device as it went; the mount table, read
    // once more right before anything is removed, also names a drive mounted
    // since and a Linux bind mount, whose device matches its parent's.
    super::super::mount_boundary::refuse_deleting_a_volume_mount(&*volume, sources, state).await?;

    // Phase 2: Delete. Files first, then directories deepest-first.
    // entries are already in order: children before parents (due to recursive scan).
    // We process files first, then dirs in reverse order.
    let mut files_done = 0;
    let mut bytes_done = 0u64;
    let mut last_progress_time = Instant::now();
    let mut countdown = SourceCountdown::new(sources, &entries);

    // Delete files: one at a time, or in the backend's own batches
    // (`Volume::delete_batch_size`, an object store's `DeleteObjects`). A batch
    // goes by key with no per-file stat: these are files the scan just listed.
    let files: Vec<&VolumeDeleteEntry> = entries.iter().filter(|e| !e.is_dir).collect();
    let batch_size = volume.delete_batch_size();
    for batch in files.chunks(batch_size.map_or(1, std::num::NonZeroUsize::get)) {
        // The cooperative boundary, between files (or batches) in the
        // destructive phase. The scan recursion above is NOT gated, so pausing
        // mid-enumeration can't freeze a half-counted "Scanning…".
        if state.stop_or_park_async().await {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(),
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        // E2E throttle so cancel-during-delete tests on fast virtual MTP have a
        // deterministic window in which to click Cancel before all files are
        // gone. Reuses the copy throttle knob (`set_test_throttle` exposes a
        // generic per-step throttle). Zero-cost outside E2E.
        if let Some(pause) = state.scope().host().pacing().copy_throttle() {
            tokio::time::sleep(pause).await;
        }

        let results = match batch_size {
            Some(_) => delete_batch(&volume, state, batch).await,
            None => {
                let mut one = Vec::with_capacity(batch.len());
                for entry in batch {
                    one.push(delete_entry(&volume, state, &entry.path, entry.listed, None).await);
                }
                one
            }
        };

        // Every file the batch removed is journaled and counted, even when
        // another in the same batch failed; the FIRST failure then ends the
        // delete, reported against its own path.
        let mut first_failure: Option<WriteOperationError> = None;
        for (entry, result) in batch.iter().zip(results) {
            match result {
                Ok(()) => {}
                // Cancel landed mid-iteration (during the throttle sleep, an
                // outage wait, or inside delete_with_cancel itself). The
                // top-of-loop cancel check only catches cancels that land
                // between iterations; without this arm, the `?` propagation
                // would surface as a Cancelled error with no `write-cancelled`
                // event, breaking the settle contract (see the module
                // CLAUDE.md § "Settle contract").
                Err(EntryStop::Cancelled) => {
                    events.emit_cancelled(WriteCancelledEvent {
                        operation_id: operation_id.to_string(),
                        operation_type: WriteOperationType::Delete,
                        files_processed: files_done,
                        rollback: CancelRollback::none(),
                    });
                    return Err(WriteOperationError::Cancelled {
                        message: "Operation cancelled by user".into(),
                    });
                }
                Err(stop) => {
                    if first_failure.is_none() {
                        first_failure = Some(stop.into_error(&entry.path, volume.as_ref()));
                    }
                    continue;
                }
            }

            // Journal the deleted leaf under the REAL volume id so "when did I
            // delete dog.jpg" is searchable for SMB / MTP too. Delete is never
            // rollbackable (the op finalizes `permanent_delete`), so these rows
            // exist purely for search — one per leaf, deliberately.
            super::super::journal::record_volume_leaf(
                state.scope(),
                cmdr_operation_log::types::EntryType::File,
                volume_id,
                &entry.path,
                None,
                Some(entry.progress_bytes as i64),
                None,
                false,
                cmdr_operation_log::types::ItemOutcome::Done,
            );

            files_done += 1;
            // `progress_bytes` (not `size`) so the numerator stays in lockstep
            // with the dedup'd `total_bytes` denominator. See
            // `VolumeDeleteEntry::progress_bytes` for the contract.
            bytes_done += entry.progress_bytes;
            countdown.entry_done(events, operation_id, &entry.path, true);
        }
        if let Some(failure) = first_failure {
            return Err(failure);
        }

        if last_progress_time.elapsed() >= state.progress_interval
            && let Some(entry) = batch.last()
        {
            let current_file = entry.path.file_name().map(|n| n.to_string_lossy().to_string());
            state.emit_progress_via_sink(
                events,
                WriteProgressEvent::new(
                    operation_id.to_string(),
                    WriteOperationType::Delete,
                    WriteOperationPhase::Deleting,
                    current_file.clone(),
                    files_done,
                    file_count,
                    bytes_done,
                    total_bytes,
                ),
            );
            state.scope().status().update(
                operation_id,
                WriteOperationPhase::Deleting,
                current_file,
                files_done,
                file_count,
                bytes_done,
                total_bytes,
            );
            last_progress_time = Instant::now();
        }
    }

    // Delete directories (already in deepest-first order from scan_volume_recursive)
    for entry in entries.iter().filter(|e| e.is_dir) {
        // The cooperative boundary (delete phase, between dirs).
        if state.stop_or_park_async().await {
            events.emit_cancelled(WriteCancelledEvent {
                operation_id: operation_id.to_string(),
                operation_type: WriteOperationType::Delete,
                files_processed: files_done,
                rollback: CancelRollback::none(),
            });
            return Err(WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            });
        }

        match delete_entry(&volume, state, &entry.path, entry.listed, None).await {
            Ok(()) => countdown.entry_done(events, operation_id, &entry.path, true),
            Err(EntryStop::Cancelled) => {
                events.emit_cancelled(WriteCancelledEvent {
                    operation_id: operation_id.to_string(),
                    operation_type: WriteOperationType::Delete,
                    files_processed: files_done,
                    rollback: CancelRollback::none(),
                });
                return Err(WriteOperationError::Cancelled {
                    message: "Operation cancelled by user".into(),
                });
            }
            // Best effort, as it always was: a folder still holding something
            // the delete couldn't take refuses, and stays.
            Err(EntryStop::Failed { error, lost: None }) if error.transport_fault().is_none() => {
                countdown.entry_done(events, operation_id, &entry.path, false);
            }
            // An outage that ended it, or a folder someone else swapped for
            // something else meanwhile: reported, ❌ never swallowed into a
            // delete that reads as complete.
            Err(stop) => return Err(stop.into_error(&entry.path, volume.as_ref())),
        }
    }

    // Emit completion
    events.emit_complete(WriteCompleteEvent {
        operation_id: operation_id.to_string(),
        operation_type: WriteOperationType::Delete,
        files_processed: files_done,
        files_skipped: 0,
        bytes_processed: bytes_done,
        appeared_during_move: None,
        top_level_skipped: None,
        refused: None,
        metadata_left_behind: None,
        durability_unconfirmed: None,
        locked_kept: None,
    });

    Ok(())
}
