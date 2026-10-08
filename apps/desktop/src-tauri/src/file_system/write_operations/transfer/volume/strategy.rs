//! Copy strategy routing for volume-to-volume operations.
//!
//! Every cross-volume copy either (a) uses the APFS clonefile fast path when
//! both sides are `LocalPosixVolume` on the same APFS volume, or (b) pipes bytes
//! through `open_read_stream` + `write_from_stream`.
//!
//! Directories are walked here (recursively) so the user can cancel between
//! files. Per-file transfers use the destination's `write_from_stream`.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::super::state::WriteOperationState;
use super::super::checkpoint_stream::CheckpointStream;
use super::super::staged_write::StagedWrite;
// Re-exported so the sibling test modules (and any future caller reached through
// this module's API) name the staging choice without a second import path.
use super::super::recovered_name::FinalizeFailure;
use super::super::retry;
pub(super) use super::super::staged_write::{
    LandingName, Replaces, WriteStaging, failed_write_leaves_ours_at, note_pending_for_local_dest, resolve_staging,
    staging_for,
};
use super::super::transfer_driver::{LeafProgressLedger, SourceProgress};
use super::super::transfer_probe::{
    TaskPhase, arm_current_task_stall_abort, note_task_retry, set_task_bytes, set_task_phase,
};
use super::merge::copy_directory_streaming;
use super::merge_ctx::{CreatedPaths, MergeCtx};
use super::preflight::SourceFileFacts;
use super::server_side_copy::try_server_side_copy;
use super::transfer_error::{AtPath, PathedVolumeError, hard_abort_error};
use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};

/// Debounce window for the foreground auto-yield: after foreground work drains,
/// the checkpoint stays parked until the device has been quiet for this long
/// before starting the next window, so a BURST of listings (e.g. arrow-keying
/// down a folder tree) is served as ONE suspension instead of re-checking every
/// window. ~400 ms balances nav responsiveness (the copy is suspended the whole
/// window) against park thrash; a starting value, to be tuned on real hardware.
const FOREGROUND_YIELD_DEBOUNCE: Duration = Duration::from_millis(400);

/// Minimum-progress floor for the foreground auto-yield: after a resume, the
/// transfer must move at least this many bytes before it will honor the next
/// foreground yield. Without it, continuous foreground nav would park the copy
/// before every window and starve it to zero throughput.
///
/// At 4 MiB this is SMALLER than one bounded read window (`MTP_READ_WINDOW`,
/// 8 MiB), so in practice the floor resolves to "at least one full window between
/// yields" — the copy always reads one more 8 MiB window before it can yield
/// again. That's the intended guarantee; the 4 MiB value just never bites
/// distinctly from the window today (real-device verified to feel right).
///
/// ⚠️ Don't naively raise this to a "big" number to make it look meaningful: the
/// gate SKIPS the yield until this many bytes have moved since the last resume,
/// and the count starts at zero for EVERY file, so a floor ≥ a typical file size
/// means the copy never yields at all for files smaller than the floor — i.e. it
/// would disable navigate-during-transfer for normal files. If you want it to
/// read as a real multi-window guard, raise it to a small multiple of
/// `MTP_READ_WINDOW` (e.g. 2-4× = "N windows between yields"); that changes
/// behavior, so re-verify on a real device.
///
/// The destination arm exempts a SINGLE-SHOT write from the floor for exactly
/// that reason: such a write holds nothing open on the server while it drains, so
/// the floor has nothing to protect, and without the exemption a folder of photos
/// uploaded to a NAS stood aside for the user zero times. A streaming write under
/// the floor still never yields; that gap is deferred, see `transfer/DETAILS.md`
/// § "Foreground auto-yield".
const MIN_PROGRESS_FLOOR_BYTES: u64 = 4 * 1024 * 1024;

/// Hard cap on a SINGLE destination-side foreground park (uploads to SMB). Unlike
/// the SOURCE arm's `wait_until_foreground_idle` (unbounded, since a read holds
/// nothing scarce between windows), the destination arm holds an OPEN SMB write
/// handle across the pause, so it must resume and write at least this often even
/// under continuous browsing, keeping the handle warm so the server can't reap it
/// as idle. 1 s balances browsing responsiveness (the upload stands aside up to a
/// second at a time) against handle safety (a WRITE lands at least once a second).
/// The share's OWN session stays warm regardless (the user's navigation rides it),
/// so this cap protects only the write handle. ❌ Don't raise it toward any
/// server idle-timeout; keep it a small, safe fraction. Data-safety bound; see
/// `checkpoint_stream.rs::CheckpointStream::bounded_yield_to_dest_foreground`.
const DEST_FOREGROUND_YIELD_HARD_CAP: Duration = Duration::from_secs(1);

/// The (debounce, min-progress-floor, dest-yield-hard-cap) tuple a freshly-built
/// `CheckpointStream` uses. Production always returns the named constants. Tests
/// override all three (debounce ≈ 0, a tiny floor, a short cap) via
/// `AutoYieldTuningGuard` so both the source and destination auto-yield arms are
/// deterministic without real device latency or megabytes of synthetic data. The
/// stream construction lives behind `copy_single_path`, so a thread-local override
/// is how a test reaches it without widening the public copy API.
fn auto_yield_tuning() -> (Duration, u64, Duration) {
    #[cfg(test)]
    {
        if let Some(t) = test_support::auto_yield_tuning_override() {
            return t;
        }
    }
    (
        FOREGROUND_YIELD_DEBOUNCE,
        MIN_PROGRESS_FLOOR_BYTES,
        DEST_FOREGROUND_YIELD_HARD_CAP,
    )
}

/// Answers "is this top-level source a directory?" from the preflight hint,
/// probing the source volume only when there is no hint.
///
/// **A missing hint means UNKNOWN, never "file".** A completed scan preview can
/// carry no per-source data at all, and defaulting to `false` there streams a
/// directory as a file AND tells both drivers the destination path is a
/// sweepable partial — which, for a directory merged into the user's own dest
/// folder, means a recursive delete of their data on any failure.
///
/// ❌ Don't probe when a hint IS present: a hinted 15k-source MTP copy would pay
/// 15k parent listings (~2 minutes of stalled dialog) for an answer the scan
/// already has.
///
/// `Err` only when the source can't be stat'd at all (it's gone, or unreadable),
/// which the caller surfaces as that source's failure rather than guessing.
pub(super) async fn resolve_source_is_directory(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    hint: Option<bool>,
) -> Result<bool, VolumeError> {
    match hint {
        Some(known) => Ok(known),
        None => source_volume.is_directory(source_path).await,
    }
}

/// Copies a single path from source volume to destination volume.
///
/// Dispatches on two cases:
/// - Both volumes are `LocalPosixVolume` and the source/destination are on the same APFS volume →
///   delegate to the native `copy_files_start` path upstream (handled in `copy_between_volumes`;
///   this function isn't called for that case).
/// - Otherwise → generic streaming pipe via `open_read_stream` + `write_from_stream`, walking
///   directories recursively so the user can cancel between files.
///
/// `source_is_directory` is `None` when the caller has no preflight hint; see
/// [`resolve_source_is_directory`] for why that must not collapse to `false`.
#[allow(
    clippy::too_many_arguments,
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, the size hint, shared state, the rollback ledger, and the source's progress accounting. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: Option<bool>,
    source_facts: SourceFileFacts,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    // This source's view of the operation's leaf accounting. Every file that
    // streams below mints its own [`LeafProgress`] from it, so leaves that
    // overlap each other each hold their own share of the in-flight total.
    progress: &Arc<SourceProgress>,
    // `Some` ⇒ deep clashes inside a merged directory honor the user's file
    // policy (Stop-wait, latch, conditional reduce, type mismatches). `None` ⇒
    // no per-child conflict resolution (the cross-volume move's copy phase,
    // where the dest is a fresh staging area, and tests that don't merge).
    merge: Option<&MergeCtx<'_>>,
    // Whether `dest_path` is the file's final name (`Stage`) or a `.cmdr-tmp-*`
    // the caller already minted for a safe-replace and will land itself
    // (`AlreadyStaged`), or the final name an in-place replace writes over.
    // Every call site derives it the same way: `staging_for(&replaces,
    // landing)`. Only the FILE branch reads it — a
    // directory source's children each get their own staging decision inside the
    // merge walker — and a directory conflict never yields a caller temp, so
    // passing the same expression everywhere stays correct.
    staging: WriteStaging,
) -> Result<u64, PathedVolumeError> {
    // Check cancellation up front.
    if super::super::super::state::is_cancelled(&state.intent) {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string())).at(source_path);
    }

    let source_is_directory = resolve_source_is_directory(source_volume, source_path, source_is_directory)
        .await
        .at(source_path)?;

    if source_is_directory {
        // A sequential source (compressed tar / solid 7z) would re-decode the
        // whole prefix on every per-file `open_read_stream`, making a subtree
        // extract O(n²). Route it to the one-pass extractor instead, which decodes
        // the stream once. Random-access sources (a folder on any real FS, a plain
        // `.tar`, a zip) keep the per-entry walk below — zero regression.
        if source_volume.extraction_is_sequential(source_path) {
            return Box::pin(super::sequential_extract::extract_sequential_subtree(
                source_volume,
                source_path,
                dest_volume,
                dest_path,
                state,
                created,
                progress,
                merge,
                source_facts.modified_at,
            ))
            .await;
        }
        Box::pin(copy_directory_streaming(
            source_volume,
            source_path,
            dest_volume,
            dest_path,
            state,
            created,
            progress,
            merge,
            None,
            source_facts.modified_at,
        ))
        .await
    } else {
        // A top-level FILE source records nothing into `created` here: the
        // caller owns that path's rollback bookkeeping because it may be a
        // safe-replace temp sibling (`write_path`) that gets renamed onto the
        // original after the write lands — the caller records the ORIGINAL, not
        // the temp. `created` is for the directory-merge case, where the
        // recursive copy below is the only place that knows which files and
        // newly-created subdirs landed inside a (possibly pre-existing) dest
        // directory.
        // A top-level FILE source is a leaf like any other: it holds its own
        // share of the in-flight total while it streams, so a sibling task's
        // file finishing can't take this one's progress off the bar.
        let leaf = progress.begin_leaf();
        let on_chunk = |file_bytes_done: u64, _file_bytes_total: u64| leaf.on_chunk(file_bytes_done);
        let bytes = stream_pipe_file(
            source_volume,
            source_path,
            source_facts,
            dest_volume,
            dest_path,
            state,
            &on_chunk,
            staging,
        )
        .await
        .map_err(|f| PathedVolumeError::at_source_or_rescued_dest(f, source_path, dest_path))?;
        leaf.complete(bytes);
        Ok(bytes)
    }
}

/// Pulls one source path (a file or a whole subtree) from `source_volume` into
/// `dest_volume` at `dest_path` with NO conflict resolution — the destination is
/// assumed empty (a fresh scratch dir), so nothing is merged or overwritten.
/// Cancel and pause ride the op's `state` (checked per chunk); the transfer is
/// otherwise silent (no progress events). This is the seam the archive copy-into
/// flow uses to materialize a REMOTE source locally before ingesting it into a
/// zip, so it reuses the exact streaming, recursion, and cancel of the copy
/// engine without exposing the conflict machinery. Returns bytes transferred.
pub(in crate::file_system::write_operations) async fn pull_path_to_local(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: bool,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
) -> Result<u64, VolumeError> {
    // A throwaway rollback ledger: on any failure the caller discards the whole
    // scratch dir, so per-file rollback bookkeeping is moot.
    let created = CreatedPaths::default();
    // Nobody is watching this pull, so the accounting reports to no one; what it
    // still does is stop the stream the moment the op is cancelled.
    let progress = LeafProgressLedger::silent_source(Arc::clone(state));
    copy_single_path(
        source_volume,
        source_path,
        // The caller probed the source itself, so this is a known answer.
        Some(source_is_directory),
        // Nothing known about the file: the stream reports the REAL length, so a
        // source whose listed metadata size lies still pulls its true bytes, and
        // the scratch dir this lands in is repackaged rather than handed to
        // anyone, so its modes carry nothing.
        SourceFileFacts::default(),
        dest_volume,
        dest_path,
        state,
        &created,
        &progress,
        None,
        // Fresh scratch destination, no conflicts: nothing is pre-staged.
        WriteStaging::Stage,
    )
    .await
    // The scratch dir is discarded wholesale on any failure and this seam
    // reports no per-item path, so the originating path has no reader here.
    .map_err(|e| e.error)
}

/// Streams one file from source to destination via `open_read_stream` /
/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
/// the destination's `write_from_stream` implementation, which calls
/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
/// `ControlFlow::Break(())`.
///
/// The source stream is wrapped in a [`CheckpointStream`] so a between-chunk
/// cooperative checkpoint (park-while-paused, then `yield_now`) runs once per
/// chunk: that's what makes a paused op stop advancing MID-FILE (the sync
/// `on_progress` callback can't `.await` to park), and what keeps a long
/// single-file transfer from starving foreground tasks.
///
/// The bytes never touch `dest_path` until the last one has landed: unless the
/// caller already staged the write, they go to a `.cmdr-tmp-*` sibling that is
/// renamed into place at the end (`staged_write.rs`).
#[allow(
    clippy::too_many_arguments,
    reason = "One file's whole streaming context: both volumes, both paths, the size hint, shared state, the progress callback, and who staged the write."
)]
pub(super) async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_facts: SourceFileFacts,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    staging: WriteStaging,
) -> Result<u64, FinalizeFailure> {
    log::debug!("stream_pipe_file: {} -> {}", source_path.display(), dest_path.display());

    // Register BOTH halves of the eventual rename with the downloads watcher's
    // ignore set when the destination is local-FS-backed (the only case where
    // the watcher could otherwise fire). Covers MTP→Local and SMB→Local imports
    // that land in ~/Downloads.
    note_pending_for_local_dest(dest_volume, dest_path);

    // A destination that can't rename can't stage. No production backend is in
    // that position (Local, SMB, and MTP all rename), but a minimal `Volume`
    // impl must stay usable as a copy destination, so a `NotSupported` landing
    // re-runs the file unstaged — the pre-staging behavior — instead of failing.
    // The server can copy for itself: the bytes never leave it. A pure
    // optimization, so anything short of a clean success falls through to the
    // streaming loop below rather than failing the file.
    if let Some(bytes) = try_server_side_copy(
        source_volume,
        source_path,
        dest_volume,
        dest_path,
        state,
        on_file_progress,
        staging,
    )
    .await?
    {
        // Landed, so a ` (N)` placeholder at the name is filled, not ours to take back.
        state.claimed_names.release_placeholder(dest_path);
        return Ok(bytes);
    }

    let mut staging = staging;
    // Set once a destination that can't land a staged write sends the file back
    // to be written at its final name. From then on a failed attempt's partial
    // sits AT that name and is ours, so the terminal failure removes it here:
    // the driver can't, because it only knows the staging it asked for
    // (`failed_write_leaves_ours_at`).
    let mut writing_at_the_final_name = false;
    // Which attempt at THIS file we're on, 1-based. A transport blip
    // (`retry::is_retryable`) runs the file again from its first byte on a fresh
    // source stream and a fresh staging temp, up to `retry::MAX_ATTEMPTS`; see
    // `retry.rs` for the policy and why it lives here rather than any layer above.
    // Restarting the whole file is what keeps the retry honest: nothing partial
    // survives an attempt, so no byte is written twice and no ledger records a
    // file twice.
    let mut attempt: u32 = 1;
    loop {
        // Opening the source is a device round-trip on MTP / SMB and can hang on
        // its own; it needs to be distinguishable from streaming in a dump. It
        // happens BEFORE the staging decision because that decision needs the
        // stream's `total_size()` — the same number the destination gets, so the
        // exemption is asked about exactly the write that will be performed.
        // Nothing is staged yet, so a failure here has nothing to clean up.
        //
        // Raced against TIER 2 for the same reason the write below is: on the
        // SERIAL path nothing above this await can end it, and a device round trip
        // that hangs before the first byte is half of the wedge shape. With
        // nothing staged, the abort has nothing to do but return.
        set_task_phase(TaskPhase::OpeningSource);
        let stream = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => return Err(hard_abort_error(source_path).into()),
            opened = source_volume.open_read_stream_with_hint(source_path, source_facts.size) => opened?,
        };
        let size = stream.total_size().known().ok_or(VolumeError::NotSupported)?;
        let length = crate::file_system::volume::StreamLength::Known(size);
        // ONE probe, two consumers: the staging decision below and the
        // destination-side foreground yield's floor exemption (handed to the
        // `CheckpointStream`). See `resolve_staging`.
        let write_is_single_shot = dest_volume.write_is_single_shot(length).await;
        // A whole-publishing destination (an object store) needs no staging
        // either, though it makes no single-shot promise: the yield floor and
        // the stall watchdog below keep reading the single-shot answer alone.
        let resolved_staging = resolve_staging(staging, write_is_single_shot || dest_volume.publishes_writes_whole());
        let staged = StagedWrite::begin(state, dest_path, resolved_staging);
        note_pending_for_local_dest(dest_volume, staged.target());
        // Wrap so a paused op parks (and a long copy yields to foreground)
        // between bounded windows. `size` is read off the raw stream first — the
        // wrapper forwards `total_size()` unchanged, so the destination still sees
        // the real size. The wrapper carries BOTH volumes: the source drives the
        // read-side auto-yield (downloads), the destination drives the bounded
        // write-side yield (uploads to SMB). Each is a no-op unless its side opts
        // in (`supports_foreground_yield()` / `supports_foreground_yield_as_destination()`).
        let (foreground_debounce, min_progress_floor, dest_yield_hard_cap) = auto_yield_tuning();
        let stream: Box<dyn VolumeReadStream> = Box::new(CheckpointStream::new(
            stream,
            Arc::clone(state),
            Arc::clone(source_volume),
            Arc::clone(dest_volume),
            foreground_debounce,
            min_progress_floor,
            dest_yield_hard_cap,
            write_is_single_shot,
        ));
        set_task_phase(TaskPhase::Streaming);
        set_task_bytes(0, size);
        // The watchdog ACTING: a task that sits inside a backend call with
        // zero byte movement for `STALL_ABORT_AFTER` has its wait ended here, and
        // the transport error that produces feeds straight back into the retry
        // above. It is the layer of last resort — every backend that can bound its
        // own waits already does, sooner — for the wedge shape that has no
        // deadline anywhere and left a user force-quitting the app.
        //
        // ❌ Never armed for a SINGLE-SHOT write. Those land in one indivisible
        // frame at the file's FINAL name, and only the backend can tell "the
        // server created the file and then refused the bytes" from "the file was
        // already there and we never touched it" (`staged_write.rs` § abandon).
        // Abandoning one from out here would add a client-initiated instance of
        // the transport hazard that exemption already documents as unfixable.
        let stall_abort = if matches!(resolved_staging, WriteStaging::SingleShot(_)) {
            None
        } else {
            arm_current_task_stall_abort()
        };
        // TIER 2 (`state.backend_abort`) rides the same `select!`, and it is a
        // different animal from tier 1: the user's Cancel travels to the backend
        // through `on_file_progress` so the backend drops its own handle and
        // deletes its own partial, and that stays the default for every cancel.
        // This arm is the quit deadline saying it will not wait for a backend
        // that is not answering — armed for EVERY write, single-shot included
        // (dropping one indivisible frame is what the process dying would do
        // anyway, which `volume/DETAILS.md` § "The single-shot exemption" already
        // accounts for). ❌ Never fire it for anything a user clicked.
        //
        // Cost on the happy path: two already-live atomics polled per wakeup of
        // the write future. No allocation, no timer, no syscall, and no change to
        // any backend.
        let write_progress =
            |progress: crate::file_system::volume::StreamWriteProgress| on_file_progress(progress.bytes_written, size);
        let write_fut =
            dest_volume.write_from_stream(staged.target(), staged.write_mode(), length, stream, &write_progress);
        let outcome = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => WriteAttemptOutcome::HardAborted,
            () = cancelled_or_never(stall_abort.as_ref()) => WriteAttemptOutcome::Finished(Err(
                VolumeError::ConnectionTimeout(format!(
                    "the write of {} stopped moving and the transfer stopped waiting for it",
                    dest_path.display()
                )),
            )),
            result = write_fut => WriteAttemptOutcome::Finished(result),
        };
        let write_result = match outcome {
            WriteAttemptOutcome::Finished(result) => result,
            WriteAttemptOutcome::HardAborted => {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: stopped waiting for the write of {}; the app is shutting down. \
                     Its partial stays registered for the startup sweep.",
                    dest_path.display(),
                );
                // ❌ No `staged.abandon` here. The delete would go back through
                // the connection that just failed to answer, which is a second
                // hold on the very deadline this tier exists to keep. A staged
                // write's temp stays in `in_flight_temps` — in memory AND in the
                // persisted log — so `in_flight_temps::init_and_sweep` removes it
                // at the next launch, and nothing sits at a real name meanwhile.
                // A SINGLE-SHOT write has no temp and needs none: the destination
                // promised one indivisible frame, so dropping it leaves either the
                // whole file or nothing, which is the same outcome the process
                // dying produces.
                return Err(hard_abort_error(dest_path).into());
            }
        };
        let bytes = match write_result {
            Ok(bytes) => {
                if attempt > 1 {
                    log::info!(
                        target: "copy",
                        "stream_pipe_file: {} landed on attempt {attempt} of {}",
                        dest_path.display(),
                        retry::MAX_ATTEMPTS,
                    );
                }
                bytes
            }
            Err(e) if retry::should_retry(&e, attempt, state) => {
                // The staged bytes are a partial of an attempt we're abandoning;
                // clear them BEFORE the next attempt starts, so a retried file
                // never leaves a trail of `.cmdr-tmp-*` siblings and the next
                // write lands on a clean path (see `abandon_attempt` for why that
                // reaches one case further than the terminal `abandon`).
                staged.abandon_attempt(dest_volume).await;
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: attempt {attempt} of {} for {} failed ({e}); running the file again in {:?}",
                    retry::MAX_ATTEMPTS,
                    dest_path.display(),
                    retry::backoff_after(attempt),
                );
                note_task_retry();
                set_task_phase(TaskPhase::WaitingToRetry);
                if !retry::wait_before_retry(state, attempt).await {
                    // Cancelled during the backoff. Report it as the cancel it is
                    // rather than the transport error that triggered the retry, so
                    // the post-loop reclassifies it and emits `write-cancelled`.
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()).into());
                }
                attempt += 1;
                continue;
            }
            // A cancel landed while an attempt was failing on something we WOULD
            // have run again. The cancel is the reason this file stops here, so
            // report it as one: the post-loop keys `write-cancelled` off a
            // `Cancelled`-shaped error, and a transport error in its place would
            // log the user's own click as a failed transfer.
            Err(e) if retry::is_retryable(&e) && super::super::super::state::is_cancelled(&state.intent) => {
                staged.abandon(dest_volume).await;
                remove_unstaged_partial(writing_at_the_final_name, dest_volume, dest_path).await;
                return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()).into());
            }
            Err(e) => {
                // The staged bytes are a partial (a mid-stream failure, or the
                // cancel the backend turned into `Cancelled`); drop them.
                staged.abandon(dest_volume).await;
                remove_unstaged_partial(writing_at_the_final_name, dest_volume, dest_path).await;
                if attempt > 1 {
                    log::warn!(
                        target: "copy",
                        "stream_pipe_file: giving up on {} after {attempt} attempt(s): {e}",
                        dest_path.display(),
                    );
                }
                return Err(e.into());
            }
        };

        // Past the last byte, and BEFORE the rename: put the source's mode on
        // what was written, so the file never wears one mode under its real name
        // and then flips to another. A no-op unless the destination is a local
        // filesystem and the source had a mode to report (`landed_mode.rs`);
        // never fails the copy, whatever the destination thinks of `chmod`.
        super::landed_mode::apply_source_mode(
            source_volume,
            source_path,
            source_facts.mode,
            dest_volume,
            staged.target(),
        )
        .await;

        // Past the last byte: give the file its final name.
        match staged.commit(dest_volume).await {
            Ok(()) => {
                // Landed, so a ` (N)` placeholder at the name is filled, not ours
                // to take back (`naming.rs::take_back_unfilled_reservations`).
                state.claimed_names.release_placeholder(dest_path);
                return Ok(bytes);
            }
            Err(FinalizeFailure {
                error: VolumeError::NotSupported,
                ..
            }) if matches!(
                staging,
                WriteStaging::Stage | WriteStaging::StageInFreshFolder | WriteStaging::StageOntoClaimedName
            ) =>
            {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: destination can't land a staged write for {}; falling back to writing at the final name",
                    dest_path.display()
                );
                staging = WriteStaging::AlreadyStaged;
                writing_at_the_final_name = true;
                continue;
            }
            // The write SUCCEEDED and the landing didn't: the temp holds the only
            // complete copy of the new bytes, and `commit` already dropped it from
            // the in-flight set so nothing sweeps it. When the landing had already
            // cleared the way, `land` also got those bytes out of temp space and
            // `new_data_at` says where they are. Surface the failure.
            Err(e) => return Err(e),
        }
    }
}

/// Removes the partial a failed write left AT its final name, which happens only
/// after a destination that can't land a staged write sent the file back to be
/// written there unstaged. Best-effort: the write's own error is the one to
/// report.
async fn remove_unstaged_partial(writing_at_the_final_name: bool, dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
    if !writing_at_the_final_name {
        return;
    }
    if let Err(e) = dest_volume.delete(dest_path).await {
        log::debug!(
            target: "copy",
            "stream_pipe_file: couldn't remove the unstaged partial {}: {e}",
            dest_path.display()
        );
    }
}

/// TIER 2: the operation's hard-abort signal, or a future that never resolves.
///
/// One `select!` arm covers both the armed and the unarmed case without a token
/// allocation, so an inert tier costs a poll of an already-live atomic.
async fn cancelled_or_never(token: Option<&tokio_util::sync::CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

/// How one attempt at a file's write ended.
enum WriteAttemptOutcome {
    /// The destination's `write_from_stream` returned, one way or the other.
    Finished(Result<u64, VolumeError>),
    /// TIER 2 ended the wait: the write future was dropped mid-flight and the
    /// backend ran none of its own cleanup. ❌ Nothing may go back through that
    /// connection now; the staged partial is left to the sweep.
    HardAborted,
}

#[cfg(test)]
#[path = "strategy_abort_tests.rs"]
mod abort_tests;
#[cfg(test)]
#[path = "strategy_copy_tests.rs"]
mod copy_tests;
#[cfg(test)]
#[path = "strategy_dest_yield_tests.rs"]
mod dest_yield_tests;
#[cfg(test)]
#[path = "strategy_pause_tests.rs"]
mod pause_tests;
#[cfg(test)]
#[path = "strategy_retry_tests.rs"]
mod retry_tests;
#[cfg(test)]
#[path = "strategy_sequential_tests.rs"]
mod sequential_tests;
#[cfg(test)]
#[path = "strategy_server_side_copy_tests.rs"]
mod server_side_copy_tests;
#[cfg(test)]
#[path = "strategy_single_shot_tests.rs"]
mod single_shot_tests;
#[cfg(test)]
#[path = "strategy_stale_handle_tests.rs"]
mod stale_handle_tests;
// `pub(super)` so sibling test modules under `transfer` (notably
// `volume_move_failure_tests`) reuse the same doubles instead of hand-rolling
// their own.
#[cfg(test)]
#[path = "strategy_test_support.rs"]
pub(super) mod test_support;

#[cfg(test)]
#[path = "strategy_dest_yield_test_support.rs"]
pub(super) mod dest_yield_test_support;

#[cfg(test)]
#[path = "strategy_yield_tests.rs"]
mod yield_tests;
