//! The concurrent driver behind a volume-to-volume copy.
//!
//! A `FuturesUnordered` sliding window, one task per top-level source item,
//! sized by `transfer_concurrency`. It is the only driver in the transfer
//! subsystem that runs several sources at once (moves are serial, local-FS copy
//! is serial), which is why it lives here rather than inside
//! `transfer_driver/`: sharing one abstraction with the serial driver would mean
//! reconciling `Fn + Send + Sync` (what `FuturesUnordered` polling needs) with
//! the serial driver's per-call `FnMut`, for a 1-of-4 win.
//!
//! `copy_volumes_with_progress` owns everything around it: the phases before
//! (dest-dir creation, temp reap, preflight, space check), the shared counters
//! and rollback ledgers, and all post-loop bookkeeping. This module only drives
//! the window and reports what happened.
//!
//! Conflict resolution runs synchronously on the driver BEFORE a task is
//! spawned, so the whole batch blocks on a single Stop prompt instead of racing
//! per-task prompts. Semantics, the cancel-drain contract, and the merge
//! invariant: `DETAILS.md`.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use super::super::super::conflict::ApplyToAll;
use super::super::super::event_sinks::OperationEventSink;
use super::super::super::journal;
use super::super::super::state::{WriteOperationState, is_cancelled, load_intent, update_operation_status};
use super::super::super::types::{VolumeCopyConfig, WriteOperationPhase, WriteOperationType, WriteProgressEvent};
use super::super::dest_name_index::{DestLookup, DestNameIndex};
use super::super::transfer_driver::make_concurrent_per_file_progress;
use super::super::transfer_probe::OperationProbe;
use super::conflict::resolve_volume_conflict;
use super::copy::drain_deadline as drain_deadline_for;
use super::preflight::SourceHint;
use super::strategy::{copy_single_path, resolve_source_is_directory};
use super::transfer_error::{PathRole, WriteFailure, map_volume_error};
use crate::file_system::volume::{Volume, VolumeError};
use crate::ignore_poison::IgnorePoison;

/// Success payload for one concurrent copy task.
///
/// `partial_path` is the path the task pushed into `in_flight_partials` (the
/// temp sibling under safe-replace, else the dest) so the result handler can
/// remove the right entry. `recorded_path` is the top-level landed path (the
/// original after a safe-replace finalize, else the dest) — recorded for
/// rollback ONLY for a top-level file source. `created_files` / `created_dirs`
/// carry the per-file destinations and newly-created subdirectories from a
/// DIRECTORY source's recursive copy, so rollback removes exactly what the op
/// wrote into a (possibly pre-existing, merged) dest directory and never the
/// directory root.
struct CopyTaskSuccess {
    partial_path: PathBuf,
    recorded_path: PathBuf,
    source_is_dir: bool,
    bytes: u64,
    /// Whether this source replaced any existing dest file (a top-level file→file
    /// safe-replace, or a deep-merge child overwrite). Feeds the operation-log
    /// eligibility: a copy that overwrote isn't rollbackable (the original is gone).
    overwrote: bool,
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
    /// The top-level source this task copied, and how many children a deep
    /// merge skipped in its subtree. `skipped_count == 0` means the whole
    /// subtree landed durably, so the out-of-zip move op may drop it from the
    /// archive; any deep skip keeps it.
    source_path: PathBuf,
    skipped_count: usize,
    skipped_bytes: u64,
}

/// Failure payload for one concurrent copy task.
///
/// `failed_path` is the in-flight partial entry to remove from
/// `in_flight_partials` (the temp sibling under safe-replace, else the dest
/// item path). ❌ It is NOT the path to report: it names where the partial sits
/// on the DESTINATION, which for a directory source is the dest dir root.
/// `reported_path` is the SOURCE item the walker actually failed on (a file deep
/// inside the subtree, not the top-level item the user selected), and is the only
/// one of the two a user can act on. Keep them separate. `cleanup_temp` distinguishes a STREAM failure (`true` — the
/// dest/temp is a half-written partial and must be cleaned) from a FINALIZE
/// failure after a SUCCESSFUL write (`false` — the temp holds the only complete
/// copy of the new data and MUST be left on disk).
///
/// `source_is_dir` plus `created_files` / `created_dirs` carry the per-file
/// rollback ledger for a DIRECTORY source interrupted mid-stream. Without it the
/// post-loop cleanup/rollback would fall back to recursively deleting the dest
/// directory ROOT — which on a merge destroys pre-existing dest-only files. With
/// it, the partials are cleaned per-file and the newly-created dirs pruned
/// empty-only, so a merged dir holding a sentinel survives.
struct CopyTaskFailure {
    failed_path: PathBuf,
    reported_path: PathBuf,
    error: VolumeError,
    cleanup_temp: bool,
    source_is_dir: bool,
    created_files: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
}

/// Everything the concurrent driver needs from its caller.
///
/// The `Arc` fields are clones of `copy_volumes_with_progress`'s own shared
/// counters and ledgers, so the caller keeps reading and unwrapping them after
/// the driver returns.
pub(super) struct ConcurrentCopy<'a> {
    pub(super) events: Arc<dyn OperationEventSink>,
    pub(super) operation_id: &'a str,
    pub(super) state: &'a Arc<WriteOperationState>,
    pub(super) source_volume: Arc<dyn Volume>,
    pub(super) source_paths: &'a [PathBuf],
    pub(super) dest_volume: Arc<dyn Volume>,
    pub(super) dest_path: &'a Path,
    pub(super) config: &'a VolumeCopyConfig,
    /// Window width, from `transfer_concurrency`.
    pub(super) concurrency: usize,
    /// The operation's file-copy window, the same width as `concurrency` and
    /// shared with every merge walker under it. A top-level FILE task takes a
    /// permit for its own write, and a DIRECTORY task's leaves take theirs, so
    /// the operation never has more than `concurrency` files in flight no matter
    /// how the batch is shaped. ❌ Never a second, per-walker width: `W` sources
    /// × `W` leaves is `W²` files on one connection.
    pub(super) file_window: super::strategy::FileWindow,
    /// The destination directory was created by THIS operation (Phase 0.5), so
    /// nothing the user already had can be inside it and every pre-check is a
    /// guaranteed miss.
    pub(super) dest_dir_is_ours: bool,
    /// The one destination listing Phase 0.6 already paid for, indexed for name
    /// lookups. `None` ⇒ probe per file.
    pub(super) dest_index: &'a Option<DestNameIndex>,
    pub(super) pre_skip_paths: &'a HashSet<PathBuf>,
    pub(super) source_hints: &'a HashMap<PathBuf, SourceHint>,
    pub(super) total_files: usize,
    pub(super) total_bytes: u64,
    pub(super) progress_interval: Duration,
    /// Real (source, dest) volume ids for the operation-log journal. `None` for
    /// the both-local shortcut and in tests that install no journal.
    pub(super) journal_volumes: &'a Option<(String, String)>,
    pub(super) op_probe: &'a Option<Arc<OperationProbe>>,
    pub(super) files_done_atomic: Arc<AtomicUsize>,
    pub(super) atomic_bytes_done: Arc<AtomicU64>,
    pub(super) files_skipped_atomic: Arc<AtomicUsize>,
    pub(super) bytes_skipped_atomic: Arc<AtomicU64>,
    pub(super) last_progress_mutex: Arc<std::sync::Mutex<Instant>>,
    pub(super) apply_to_all_cell: Arc<std::sync::Mutex<ApplyToAll>>,
    pub(super) copied_paths: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) in_flight_partials: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    pub(super) deep_skipped_files: Arc<AtomicUsize>,
    pub(super) deep_skipped_bytes: Arc<AtomicU64>,
}

/// What the driver leaves for the caller's post-loop bookkeeping. The counters
/// aren't in here: they live in the shared atomics the caller already holds.
pub(super) struct ConcurrentOutcome {
    /// The single half-written partial of a FILE source whose stream failed, for
    /// the post-loop cleanup sweep. Stays `None` for a directory source, whose
    /// cleanup runs per-file off the `copied_paths` / `created_dirs` ledgers.
    pub(super) last_dest_path: Option<PathBuf>,
    /// The first task failure, if any. The driver stops pushing new work and
    /// drains on one.
    pub(super) copy_error: Option<WriteFailure>,
}

/// Bumps `files_done` and `bytes_done` for a skipped source and (throttled)
/// emits a `write-progress` event. Without this, a "Skip all" choice silently
/// runs through dozens of conflicts with the progress bar pinned at 0% — the
/// user expects the bar to reflect skipped files since the operation is in
/// fact processing them.
#[allow(
    clippy::too_many_arguments,
    reason = "Helper bundles all the per-emit context the surrounding loop already has on hand"
)]
fn account_skipped_file(
    source_path: &Path,
    source_hints: &HashMap<PathBuf, SourceHint>,
    files_done_atomic: &Arc<AtomicUsize>,
    atomic_bytes_done: &Arc<AtomicU64>,
    files_skipped_atomic: &Arc<AtomicUsize>,
    bytes_skipped_atomic: &Arc<AtomicU64>,
    last_progress_mutex: &Arc<std::sync::Mutex<Instant>>,
    progress_interval: Duration,
    state: &Arc<WriteOperationState>,
    events: &dyn OperationEventSink,
    operation_id: &str,
    total_files: usize,
    total_bytes: u64,
) {
    let hint_size = source_hints
        .get(source_path)
        .map(|h| if h.is_directory { 0 } else { h.size })
        .unwrap_or(0);
    let new_files = files_done_atomic.fetch_add(1, Ordering::Relaxed) + 1;
    let new_bytes = atomic_bytes_done.fetch_add(hint_size, Ordering::Relaxed) + hint_size;
    files_skipped_atomic.fetch_add(1, Ordering::Relaxed);
    bytes_skipped_atomic.fetch_add(hint_size, Ordering::Relaxed);

    let mut last = last_progress_mutex.lock_ignore_poison();
    if last.elapsed() >= progress_interval {
        *last = Instant::now();
        drop(last);
        state.emit_progress_via_sink(
            events,
            WriteProgressEvent::new(
                operation_id.to_string(),
                WriteOperationType::Copy,
                WriteOperationPhase::Copying,
                source_path.file_name().map(|n| n.to_string_lossy().to_string()),
                new_files,
                total_files,
                new_bytes,
                total_bytes,
            ),
        );
        update_operation_status(
            operation_id,
            WriteOperationPhase::Copying,
            source_path.file_name().map(|n| n.to_string_lossy().to_string()),
            new_files,
            total_files,
            new_bytes,
            total_bytes,
        );
    }
}

/// Runs the sliding window to completion, cancellation, or the first failure.
///
/// Returns `Err` only when conflict resolution itself fails; a failed transfer
/// comes back as `ConcurrentOutcome::copy_error` so the caller's post-loop still
/// runs its rollback and cleanup.
pub(super) async fn drive_transfer_concurrent(ctx: ConcurrentCopy<'_>) -> Result<ConcurrentOutcome, WriteFailure> {
    let ConcurrentCopy {
        events,
        operation_id,
        state,
        source_volume,
        source_paths,
        dest_volume,
        dest_path,
        config,
        concurrency,
        file_window,
        dest_dir_is_ours,
        dest_index,
        pre_skip_paths,
        source_hints,
        total_files,
        total_bytes,
        progress_interval,
        journal_volumes,
        op_probe,
        files_done_atomic,
        atomic_bytes_done,
        files_skipped_atomic,
        bytes_skipped_atomic,
        last_progress_mutex,
        apply_to_all_cell,
        copied_paths,
        created_dirs,
        in_flight_partials,
        deep_skipped_files,
        deep_skipped_bytes,
    } = ctx;

    // Handed back to the caller's post-loop; see `ConcurrentOutcome`.
    let mut last_dest_path: Option<PathBuf> = None;
    let mut copy_error: Option<WriteFailure> = None;

    // Concurrent path: FuturesUnordered-driven sliding window sized by
    // `concurrency`. Each task streams one top-level source item end-to-end.
    // Conflict resolution runs synchronously on this driver before the task
    // is spawned (F14) so the whole batch blocks on a single Stop prompt
    // instead of racing per-task prompts.
    // Ok payload is `(partial_path, recorded_path, bytes)`: `partial_path`
    // is the path the task pushed into `in_flight_partials` (the temp
    // sibling under safe-replace, else the dest itself) so the result
    // handler can remove the right entry; `recorded_path` is the final
    // landed path for rollback bookkeeping (the original after a
    // safe-replace finalize, else the dest).
    //
    // Err payload is a `CopyTaskFailure`: `cleanup_temp` distinguishes a
    // STREAM failure (`true` — the dest/temp is a partial and must be
    // cleaned) from a FINALIZE failure after a SUCCESSFUL write (`false` —
    // the temp now holds the only complete copy of the new data and MUST be
    // left on disk; the original was already deleted by
    // `finalize_safe_replace`'s delete step). Deleting the temp in the
    // finalize-failure case would be total data loss. The `created_*` ledger
    // carries a DIRECTORY source's per-file partials out of the error arm so
    // post-loop cleanup never recursively deletes a merged dest dir root.
    type CopyTaskFuture<'a> = Pin<Box<dyn Future<Output = Result<CopyTaskSuccess, CopyTaskFailure>> + Send + 'a>>;
    let mut in_flight: FuturesUnordered<CopyTaskFuture<'_>> = FuturesUnordered::new();

    // Inline helper: drains ONE future from `in_flight`, updates tracking.
    // Returns Err on the first task failure (caller breaks + stores copy_error).
    // `in_flight` is threaded through as a mutable borrow so the helper is
    // just a local lambda in shape, but we inline below for borrow clarity.

    let mut iter = source_paths.iter().enumerate();
    // Set once the driver has SEEN the user's cancel / rollback: from then on
    // it stops waiting indefinitely for its tasks and gives them a bounded
    // window to wind down. See `CANCEL_DRAIN_DEADLINE`.
    let mut drain_deadline: Option<tokio::time::Instant> = None;
    // Whether that window is the hard-abort tier's short one rather than the
    // cooperative cancel's. Latched, so the abort re-arms the deadline once.
    let mut drain_shortened = false;
    loop {
        // Keep pushing new tasks until either sources run out or the window is full.
        while in_flight.len() < concurrency {
            if is_cancelled(&state.intent) {
                break;
            }
            let Some((source_index, source_path)) = iter.next() else {
                break;
            };

            // Pre-known conflict already accounted upfront in the bulk skip.
            if pre_skip_paths.contains(source_path) {
                continue;
            }

            // Is this source a directory? Resolved ONCE per source, here, from
            // the preflight hint — or by probing when there is none (a LOCAL
            // scan preview completes with an empty `per_path`, so a real
            // directory can arrive hintless). Three things downstream read this
            // answer and all three break on a wrong one: the conflict resolver,
            // `copy_single_path`'s streaming branch, and — the data-safety one —
            // the `in_flight_partials` gate below, which keeps a merged
            // destination directory out of the post-loop's recursive sweep.
            let source_hint = source_hints.get(source_path).copied();
            let source_is_dir =
                resolve_source_is_directory(&source_volume, source_path, source_hint.map(|hint| hint.is_directory))
                    .await
                    .map_err(|e| {
                        WriteFailure::synthetic(map_volume_error(
                            &source_path.display().to_string(),
                            PathRole::Source,
                            e,
                        ))
                    })?;
            // Sizes stay hint-only: a missing size just means no SMB compound
            // fast path, never a wrong branch, so it isn't worth a probe.
            let source_size_hint = source_hint.and_then(|hint| (!hint.is_directory).then_some(hint.size));

            // Resolve destination path + conflict synchronously.
            let mut dest_item_path = if let Some(name) = source_path.file_name() {
                dest_path.join(name)
            } else {
                dest_path.to_path_buf()
            };
            // For a file→file Overwrite, conflict resolution hands back a
            // temp sibling to stream into plus the original path to swap in
            // after the write fully lands (safe-replace). `None` ⇒ write
            // `dest_item_path` directly.
            let mut replace_after_write: Option<PathBuf> = None;
            // The destination pre-check: something already at this name ⇒
            // resolve the conflict. Asked as a `get_metadata` per source it
            // is ONE ROUND TRIP PER FILE, serialized here on the driver, and
            // on a NAS at 3.7 ms RTT it measured 2.378 s of a 3.224 s best
            // run for 500 files — 74%, and no window width can overlap it
            // (`docs/notes/transfer-concurrency-window-bench-2026-08-02.md`).
            //
            // Two things answer it more cheaply, in order:
            //
            // 1. **A destination directory THIS OPERATION created** (Phase
            //    0.5): nothing the user already had can be inside a folder
            //    that didn't exist a moment ago, so every probe is a
            //    guaranteed miss and there's nothing even to index.
            //    ❌ Never widen this to "the destination is empty". A
            //    pre-existing empty directory can gain an entry from another
            //    process between any two instants; one we just created cannot
            //    have held anything BEFORE we made it. Only the second claim
            //    is safe, and the difference is silent when you get it wrong.
            // 2. **The destination listing Phase 0.6 already paid for**, for
            //    a merge into a pre-existing folder — the ordinary F5 copy.
            //    `DestNameIndex` answers `Absent` only when no name in that
            //    listing can resolve to this one on any backend; anything it
            //    can't settle comes back `Unknown` and falls through to the
            //    probe below, which stays authoritative.
            //
            // The listing is taken once, at the start: by the 400th file of a
            // large batch it can be minutes old, so a file that ARRIVES at
            // the destination mid-batch is missed and an Overwrite replaces
            // it with no prompt. That trade is deliberate and David chose it
            // (2026-08-02); ❌ don't answer it with re-listing, polling, or a
            // freshness window. `DETAILS.md` § "Answering the pre-check from
            // one listing".
            let existing_dest_meta = if dest_dir_is_ours {
                None
            } else {
                match dest_index.as_ref().map(|index| index.lookup(source_path.file_name())) {
                    Some(DestLookup::Absent) => None,
                    Some(DestLookup::Present(entry)) => Some(*entry),
                    // No index (a local destination, or a listing that
                    // failed), or a name only the backend can settle.
                    Some(DestLookup::Unknown) | None => {
                        // Record the pre-check BEFORE awaiting it. In the
                        // 2026-07-31 incident this destination `get_metadata`
                        // was the driver's last log line and nothing said
                        // whether it returned, so a dump has to be able to
                        // name it as the step in progress.
                        if let Some(probe) = op_probe.as_ref() {
                            probe.set_driver_phase(
                                super::super::transfer_probe::DriverPhase::PreparingNext,
                                &format!("#{source_index} {}", dest_item_path.display()),
                            );
                        }
                        dest_volume.get_metadata(&dest_item_path).await.ok()
                    }
                }
            };
            if let Some(dest_meta) = existing_dest_meta {
                // The type and size come from the scan (or the one probe above),
                // never a re-stat: an MTP `scan_for_copy` lists the parent dir,
                // ~18 s for 1046 photos on a cold cache.
                let source_is_directory_hint = Some(source_is_dir);
                let dest_size_hint = dest_meta.size;
                log::debug!(
                    "copy_volumes_with_progress: conflict detected at {} (source_is_dir={}, dest_is_dir={})",
                    dest_item_path.display(),
                    source_is_dir,
                    dest_meta.is_directory,
                );
                // Copy the op-wide latch out, run the resolver on the stack
                // local, store it back — mirroring the serial path. The
                // resolver's `conflict_dispatch_lock` (acquired inside) is
                // what serializes the human against in-flight deep merges
                // spawned by earlier loop iterations; this same lock is why a
                // top-level prompt and a deep prompt can't race the one
                // oneshot slot. The known acceptable residual: an already-
                // emitted prompt isn't retroactively resolved by another
                // task's "…all" latch — a rare extra prompt, never data loss.
                let mut latched = *apply_to_all_cell.lock_ignore_poison();
                let resolved = resolve_volume_conflict(
                    &source_volume,
                    source_path,
                    &dest_volume,
                    &dest_item_path,
                    config,
                    &*events,
                    operation_id,
                    state,
                    &mut latched,
                    source_size_hint,
                    dest_size_hint,
                    source_is_directory_hint,
                )
                .await
                .map_err(WriteFailure::synthetic);
                *apply_to_all_cell.lock_ignore_poison() = latched;
                let resolved = resolved?;
                match resolved {
                    None => {
                        log::debug!(
                            "copy_volumes_with_progress: skipping {} due to conflict resolution",
                            source_path.display()
                        );
                        account_skipped_file(
                            source_path,
                            source_hints,
                            &files_done_atomic,
                            &atomic_bytes_done,
                            &files_skipped_atomic,
                            &bytes_skipped_atomic,
                            &last_progress_mutex,
                            progress_interval,
                            state,
                            &*events,
                            operation_id,
                            total_files,
                            total_bytes,
                        );
                        continue;
                    }
                    Some(rc) => {
                        dest_item_path = rc.write_path;
                        replace_after_write = rc.replace_after_write;
                    }
                }
            }

            let file_name = source_path.file_name().map(|n| n.to_string_lossy().to_string());
            log::debug!(
                "copy_volumes_with_progress: spawning copy {} -> {}",
                source_path.display(),
                dest_item_path.display()
            );

            // Mark this destination as in-flight so cancel/error can clean it
            // up — but ONLY for a FILE source. A DIRECTORY source's dest is a
            // (possibly pre-existing, merged) dir whose cleanup path is
            // `remove_tree`; recording the dir ROOT here and
            // then recursively deleting it on keep-partials/rollback would
            // destroy pre-existing dest-only files (the merge invariant). A
            // directory source's cleanup is owned entirely by the per-file
            // `created`/`copied_paths` ledger threaded out of the task's
            // result arms. (A dir task dropped mid-flight on abort leaves its
            // in-flight `.cmdr-tmp-<uuid>` for the backend writer's abort to
            // clean; never the merged root.) Pinned by
            // `cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file`.
            if !source_is_dir {
                in_flight_partials.lock_ignore_poison().push(dest_item_path.clone());
            }

            let src_vol = Arc::clone(&source_volume);
            let dst_vol = Arc::clone(&dest_volume);
            let state_clone = Arc::clone(state);
            let events_task = Arc::clone(&events);
            let op_id = operation_id;
            let files_done_a = Arc::clone(&files_done_atomic);
            let bytes_done_a = Arc::clone(&atomic_bytes_done);
            let last_prog_a = Arc::clone(&last_progress_mutex);
            let source_owned = source_path.clone();
            let dest_owned = dest_item_path.clone();
            let replace_after_write_owned = replace_after_write.clone();
            let file_name_owned = file_name.clone();
            // Per-task merge context: deep file clashes inside a directory
            // source landing on a merged dest honor the file policy, sharing
            // the op-wide apply-to-all latch with every other task and the
            // top-level dispatch.
            let merge_config = config.clone();
            let merge_op_id = operation_id.to_string();
            let merge_apply_to_all = Arc::clone(&apply_to_all_cell);
            let merge_window = file_window.clone();
            let merge_probe = op_probe.clone();
            let leaf_window = file_window.clone();
            // Register before the task is pushed, so a task that never gets
            // polled still shows up in a dump as `spawned`.
            let task_probe = op_probe.as_ref().map(|probe| {
                probe.begin_task(
                    source_index,
                    &source_path.display().to_string(),
                    &dest_item_path.display().to_string(),
                )
            });

            in_flight.push(Box::pin(async move {
                // Held for the task's whole life; dropping it (completion,
                // abort, panic) removes the row from the in-flight table.
                let task_probe = task_probe;
                let probe_handle = task_probe
                    .as_ref()
                    .map(super::super::transfer_probe::TaskProbeHandle::probe);
                // Per-task `last_file_bytes` tracks bytes reported for the
                // file this task is copying; deltas roll up into the
                // shared `bytes_done_a` so the throttle emits an aggregate.
                // Owned by the task; the helper closure carries its own
                // Arc clone, the post-call compensation reads the same
                // counter to detect "volume never invoked on_progress."
                let last_file_bytes = Arc::new(AtomicU64::new(0));
                // Per-source rollback ledger: the files this task streams
                // and the dirs it newly creates inside a directory source.
                let created = super::strategy::CreatedPaths::default();
                // Deep merge children are never top-level sources, so the
                // resolver never keys into per-source hints for them — an
                // empty map is correct (and avoids capturing the function's
                // `source_hints` into the `'static` task).
                let merge_hints: HashMap<PathBuf, SourceHint> = HashMap::new();
                // A skipped child reports no chunks, so unlike a copied one its
                // bytes never reach `bytes_done_a` through the progress
                // callback: credit both axes here. `note_skipped` is what keeps
                // them off the rate.
                let on_file_skipped = {
                    let bytes_done_a = Arc::clone(&bytes_done_a);
                    let files_done_a = Arc::clone(&files_done_a);
                    let state_clone = Arc::clone(&state_clone);
                    move |leaf_bytes: u64| {
                        bytes_done_a.fetch_add(leaf_bytes, Ordering::Relaxed);
                        files_done_a.fetch_add(1, Ordering::Relaxed);
                        state_clone.note_skipped(1, leaf_bytes);
                    }
                };
                let merge_ctx = super::strategy::MergeCtx {
                    events: &*events_task,
                    operation_id: &merge_op_id,
                    config: &merge_config,
                    state: &state_clone,
                    apply_to_all: &merge_apply_to_all,
                    source_hints: &merge_hints,
                    on_file_skipped: &on_file_skipped,
                    window: merge_window,
                    op_probe: merge_probe,
                };
                let on_file_progress = make_concurrent_per_file_progress(
                    Arc::clone(&events_task),
                    Arc::clone(&state_clone),
                    op_id.to_string(),
                    WriteOperationType::Copy,
                    file_name_owned.clone(),
                    Arc::clone(&last_file_bytes),
                    Arc::clone(&bytes_done_a),
                    Arc::clone(&files_done_a),
                    total_files,
                    total_bytes,
                    Arc::clone(&last_prog_a),
                    progress_interval,
                );
                // The byte count is rolled into the aggregate by the progress
                // callback's per-chunk delta (and the post-task compensation),
                // so this only advances the leaf-file axis.
                let on_file_complete = |_leaf_bytes: u64| {
                    files_done_a.fetch_add(1, Ordering::Relaxed);
                };
                // A top-level FILE source IS a leaf, so it takes its slot from
                // the same op-wide window a directory's children take theirs
                // from. Otherwise a batch mixing files and folders would carry
                // `W` file tasks PLUS the walkers' `W` leaves — twice the width
                // the user's setting asked for, on one connection.
                //
                // ❌ A DIRECTORY source takes none. A walker that held a slot
                // while waiting for its own children to get theirs would
                // deadlock the operation outright at width 1.
                let _leaf_permit = if source_is_dir {
                    None
                } else {
                    leaf_window.reserve().await
                };
                let copy_fut = copy_single_path(
                    &src_vol,
                    &source_owned,
                    Some(source_is_dir),
                    source_size_hint,
                    &dst_vol,
                    &dest_owned,
                    &state_clone,
                    &created,
                    &on_file_progress,
                    &on_file_complete,
                    Some(&merge_ctx),
                    super::strategy::staging_for(&replace_after_write_owned),
                );
                // Bind this task's probe as a task-local for the whole copy, so
                // `stream_pipe_file` and `CheckpointStream` can record their
                // phases without threading a handle through every signature.
                let result = match probe_handle {
                    Some(probe) => {
                        super::super::transfer_probe::CURRENT_TASK_PROBE
                            .scope(probe, copy_fut)
                            .await
                    }
                    None => copy_fut.await,
                };
                let created_files = std::mem::take(&mut *created.files.lock_ignore_poison());
                let created_dirs = std::mem::take(&mut *created.dirs.lock_ignore_poison());
                // Deep-merge skips in this source's subtree; `0` means the
                // whole subtree landed durably (the move op may drop it from
                // the archive).
                let task_skipped_count = created.skipped_file_count();
                let task_skipped_bytes = created.skipped_byte_count();
                // Overwrote iff a top-level file→file safe-replace fires below,
                // OR a deep-merge child replaced an existing dest file. Computed
                // before `replace_after_write_owned` is consumed. Feeds the
                // operation-log eligibility (a copy that overwrote can't roll back).
                let task_overwrote = replace_after_write_owned.is_some() || created.any_overwrote();
                match result {
                    Ok(bytes) => {
                        // If the volume didn't call the progress callback,
                        // add bytes_copied to the aggregate so the total is
                        // right. Same compensation the sequential path does.
                        if last_file_bytes.load(Ordering::Relaxed) == 0 && bytes > 0 {
                            bytes_done_a.fetch_add(bytes, Ordering::Relaxed);
                        }
                        // Safe-replace finalize: the temp now holds the
                        // complete new data; delete the original and rename
                        // the temp into place. On finalize error, surface
                        // it as this file's failure with `cleanup_temp =
                        // false` — the write SUCCEEDED, so the temp is
                        // committed data (the only complete copy, since
                        // finalize's delete step may already have removed
                        // the original). It must survive as a recoverable
                        // `.cmdr-tmp-*` artifact, NOT be cleaned.
                        if let Some(orig) = replace_after_write_owned {
                            if let Err(e) = super::conflict::finalize_safe_replace(&dst_vol, &dest_owned, &orig).await {
                                // Finalize is file→file only (safe-replace),
                                // so there's no directory ledger to carry.
                                return Err(CopyTaskFailure {
                                    failed_path: dest_owned,
                                    reported_path: source_owned.clone(),
                                    error: e,
                                    cleanup_temp: false,
                                    source_is_dir: false,
                                    created_files,
                                    created_dirs,
                                });
                            }
                            // Landed at `orig`; the temp `dest_owned` is
                            // gone after the rename. Report the temp as the
                            // partial to remove and `orig` as the recorded
                            // path for rollback bookkeeping. Safe-replace is
                            // file→file only, so there are no created dirs.
                            return Ok(CopyTaskSuccess {
                                partial_path: dest_owned,
                                recorded_path: orig,
                                source_is_dir: false,
                                bytes,
                                overwrote: task_overwrote,
                                created_files,
                                created_dirs,
                                source_path: source_owned,
                                skipped_count: task_skipped_count,
                                skipped_bytes: task_skipped_bytes,
                            });
                        }
                        Ok(CopyTaskSuccess {
                            partial_path: dest_owned.clone(),
                            recorded_path: dest_owned,
                            source_is_dir,
                            bytes,
                            overwrote: task_overwrote,
                            created_files,
                            created_dirs,
                            source_path: source_owned,
                            skipped_count: task_skipped_count,
                            skipped_bytes: task_skipped_bytes,
                        })
                    }
                    // Stream failure (incl. mid-stream cancel): the dest/temp
                    // is a half-written partial → clean it
                    // (`cleanup_temp = true`). For a DIRECTORY source, carry
                    // the per-file ledger so the result handler records the
                    // individual partials instead of the dir root — the
                    // post-loop must never recursively delete a merged dest
                    // dir and destroy pre-existing dest-only files.
                    Err(e) => Err(CopyTaskFailure {
                        failed_path: dest_owned,
                        reported_path: e.path,
                        error: e.error,
                        cleanup_temp: true,
                        source_is_dir,
                        created_files,
                        created_dirs,
                    }),
                }
            }));
        }

        if in_flight.is_empty() {
            break;
        }

        // The await the driver parked on for 20 minutes in the 2026-07-31
        // incident. Naming it means a dump distinguishes "the driver is
        // waiting for tasks" from "the driver is stuck preparing the next
        // source", which the log could not tell apart.
        if let Some(probe) = op_probe.as_ref() {
            probe.set_driver_phase(super::super::transfer_probe::DriverPhase::AwaitingTasks, "");
        }

        // Observing the user's intent HERE is what makes Cancel and Rollback
        // work at all. The spawn loop above checks `is_cancelled` too, but a
        // driver whose window is full — or whose sources have run out — never
        // gets back to it while its tasks are parked, which is why Rollback
        // did nothing in the incident. `backend_cancel` fires on every
        // transition out of `Running`, so both Cancel and Rollback land here.
        let next = match drain_deadline {
            // Already winding down: bounded, so one task that never returns
            // can't hold the operation (and the user's dialog) open.
            //
            // The window stays re-negotiable while it runs. A quit cancels first
            // and aborts what's left a beat later, and by then this await is
            // already sitting on the cooperative deadline — so tier 2 gets its own
            // `select!` arm to shorten it. Guarded on `drain_shortened` because a
            // fired token is ready forever, and an unguarded arm would spin.
            Some(deadline) => tokio::select! {
                biased;
                () = state.backend_abort.cancelled(), if !drain_shortened => {
                    drain_shortened = true;
                    let shortened = drain_deadline_for(true);
                    log::info!(
                        target: "copy",
                        "copy_volumes_with_progress: op={operation_id} is no longer waiting for its {} in-flight task(s); \
                         cutting the wind-down short (to {shortened:?})",
                        in_flight.len(),
                    );
                    drain_deadline = Some(tokio::time::Instant::now() + shortened);
                    continue;
                }
                result = tokio::time::timeout_at(deadline, in_flight.next()) => match result {
                    Ok(next) => next,
                    Err(_) => {
                        crate::log_error!(
                            target: "copy",
                            "copy_volumes_with_progress: op={} abandoning {} task(s) that did not wind down within {:?} of the cancel. \
                             Their handles are left for the backend to reap; their staged partials are cleaned up below.{}",
                            operation_id,
                            in_flight.len(),
                            drain_deadline_for(drain_shortened),
                            op_probe
                                .as_ref()
                                .map(|p| format!("\n{}", p.render_dump("abandoning wedged tasks")))
                                .unwrap_or_default(),
                        );
                        break;
                    }
                },
            },
            None => {
                tokio::select! {
                    // Cancel wins the race: a task result that was also ready
                    // is re-polled on the next pass, so nothing is dropped.
                    biased;
                    () = state.backend_cancel.cancelled() => {
                        // An abort fires tier 1 first, so it can be what woke this
                        // arm; ask which window we're owed rather than assuming.
                        drain_shortened = state.backend_abort.is_cancelled();
                        let window = drain_deadline_for(drain_shortened);
                        log::info!(
                            target: "copy",
                            "copy_volumes_with_progress: op={} observed {:?} while awaiting {} in-flight task(s); \
                             winding them down (up to {window:?})",
                            operation_id,
                            load_intent(&state.intent),
                            in_flight.len(),
                        );
                        drain_deadline = Some(tokio::time::Instant::now() + window);
                        continue;
                    }
                    next = in_flight.next() => next,
                }
            }
        };
        match next {
            Some(Ok(CopyTaskSuccess {
                partial_path,
                recorded_path,
                source_is_dir,
                bytes,
                overwrote,
                created_files,
                created_dirs: task_created_dirs,
                source_path: done_source,
                skipped_count: task_skipped_count,
                skipped_bytes: task_skipped_bytes,
            })) => {
                // Fold this source's deep-merge skips into the op-wide tally,
                // and — when the source landed with ZERO skips — record it as
                // fully extracted so the out-of-zip move op may drop it.
                deep_skipped_files.fetch_add(task_skipped_count, Ordering::Relaxed);
                deep_skipped_bytes.fetch_add(task_skipped_bytes, Ordering::Relaxed);
                if task_skipped_count == 0 {
                    events.note_source_landed_clean(&done_source);
                }
                // Remove the in-flight partial (the temp under safe-replace,
                // else the dest) and record what the op wrote for rollback.
                let mut partials = in_flight_partials.lock_ignore_poison();
                if let Some(pos) = partials.iter().position(|p| p == &partial_path) {
                    partials.swap_remove(pos);
                }
                drop(partials);
                let file_name_done = recorded_path.file_name().map(|n| n.to_string_lossy().to_string());
                // Journal the per-leaf `rollback_unit` rows under the REAL
                // volume ids (a file source → one leaf at `recorded_path`; a dir
                // source → one leaf per `created_files` entry, source rebased
                // from `done_source`). Created dirs journal post-loop (after all
                // files, so their `seq` follows the contents). No-ops for the
                // both-local shortcut (that path journals via `copy_files_start`)
                // and in tests that don't set a journal target.
                if let Some((src_vol, dst_vol)) = journal_volumes.as_ref() {
                    journal::record_volume_transfer_source(
                        operation_id,
                        src_vol,
                        &done_source,
                        dst_vol,
                        &recorded_path,
                        source_is_dir,
                        &created_files,
                        (!source_is_dir).then_some(bytes as i64),
                        overwrote,
                    );
                }
                // For a DIRECTORY source, record the individual files and the
                // newly-created subdirs the op wrote — never the directory
                // root — so rollback can't recursively delete a merged
                // directory and destroy dest-only files. For a FILE source,
                // record the landed path (the original after a safe-replace
                // finalize, else the dest); `created_*` are empty.
                if source_is_dir {
                    copied_paths.lock_ignore_poison().extend(created_files);
                    created_dirs.lock_ignore_poison().extend(task_created_dirs);
                } else {
                    copied_paths.lock_ignore_poison().push(recorded_path);
                }
                // Per-file milestone emit. The task's `on_file_complete`
                // bumped `files_done_atomic`; the FE's files-axis needs
                // a Copying event that observes the bumped value, but
                // no chunked emit fires after `on_file_complete` (the
                // file's transfer is over). Mirrors the serial path's
                // milestone in `transfer_driver.rs::drive_transfer_serial_async`.
                *last_progress_mutex.lock_ignore_poison() = Instant::now();
                let current_files = files_done_atomic.load(Ordering::Relaxed);
                let current_bytes = atomic_bytes_done.load(Ordering::Relaxed);
                state.emit_progress_via_sink(
                    &*events,
                    WriteProgressEvent::new(
                        operation_id.to_string(),
                        WriteOperationType::Copy,
                        WriteOperationPhase::Copying,
                        file_name_done.clone(),
                        current_files,
                        total_files,
                        current_bytes,
                        total_bytes,
                    ),
                );
                update_operation_status(
                    operation_id,
                    WriteOperationPhase::Copying,
                    file_name_done,
                    current_files,
                    total_files,
                    current_bytes,
                    total_bytes,
                );
            }
            Some(Err(CopyTaskFailure {
                failed_path: failed_dest,
                reported_path,
                error: e,
                cleanup_temp,
                source_is_dir,
                created_files,
                created_dirs: task_created_dirs,
            })) => {
                // Remove from in-flight partials; this one's own partial
                // cleanup (if any) the post-loop logic will do.
                let mut partials = in_flight_partials.lock_ignore_poison();
                if let Some(pos) = partials.iter().position(|p| p == &failed_dest) {
                    partials.swap_remove(pos);
                }
                drop(partials);
                if source_is_dir {
                    // DIRECTORY source interrupted mid-stream. Record the
                    // per-file partials and newly-created subdirs the op
                    // wrote, NOT the dir root `failed_dest`. The post-loop
                    // then cleans/rolls back per-file (and prunes created
                    // dirs empty-only), so a merged dir holding a
                    // pre-existing dest-only file survives — recursively
                    // deleting the root would be silent data loss.
                    copied_paths.lock_ignore_poison().extend(created_files);
                    created_dirs.lock_ignore_poison().extend(task_created_dirs);
                } else if cleanup_temp {
                    // FILE source stream failure: `failed_dest` is the single
                    // half-written partial. Clean it.
                    //
                    // `cleanup_temp == false` ⇒ finalize failed AFTER a
                    // successful write: `failed_dest` is the temp holding the
                    // ONLY complete copy of the new data (finalize already
                    // deleted the original). Do NOT designate it for cleanup —
                    // leaving it on disk as a `.cmdr-tmp-*` artifact is the
                    // correct, safe outcome. Cleaning it would be total data
                    // loss.
                    last_dest_path = Some(failed_dest.clone());
                }
                copy_error = Some(WriteFailure::from_volume(&reported_path, PathRole::Source, e));
                // Drop remaining in-flight tasks; their streams close,
                // temp files get cleaned up by the per-backend write
                // abort + delete path. Partial cleanup is done below.
                break;
            }
            None => break,
        }
    }

    // Drain whatever's left on cancel/error. On success, `in_flight` is
    // already empty. On abort, drop cancels the remaining futures (F10).
    if let Some(probe) = op_probe.as_ref() {
        probe.set_driver_phase(
            super::super::transfer_probe::DriverPhase::PostLoop,
            "draining in-flight",
        );
    }
    drop(in_flight);

    Ok(ConcurrentOutcome {
        last_dest_path,
        copy_error,
    })
}

/// The driver's own contract, asserted on what it hands back rather than on the
/// files a finished operation left: the post-loop's delete-capability split is a
/// second defense of the same data, so an end-to-end assertion can't tell a
/// right rollback ledger from a wrong one.
#[cfg(test)]
#[path = "copy_concurrent_driver_tests.rs"]
mod driver_tests;
