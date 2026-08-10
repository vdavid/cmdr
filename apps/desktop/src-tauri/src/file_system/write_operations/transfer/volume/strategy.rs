//! Copy strategy routing for volume-to-volume operations.
//!
//! Since Phase 4, every cross-volume copy either (a) uses the APFS clonefile
//! fast path when both sides are `LocalPosixVolume` on the same APFS volume, or
//! (b) pipes bytes through `open_read_stream` + `write_from_stream`. The old
//! `export_to_local` / `import_from_local` short-circuits are gone.
//!
//! Directories are walked here (recursively) so the user can cancel between
//! files. Per-file transfers use the destination's `write_from_stream`.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use super::super::super::conflict::ApplyToAll;
use super::super::super::state::WriteOperationState;
use super::super::super::types::{OperationEventSink, VolumeCopyConfig, WriteOperationError};
use super::super::checkpoint_stream::CheckpointStream;
use super::super::staged_write::StagedWrite;
// Re-exported so the sibling test modules (and any future caller reached through
// this module's API) name the staging choice without a second import path.
use super::super::retry;
pub(super) use super::super::staged_write::WriteStaging;
use super::super::transfer_probe::{
    TaskPhase, arm_current_task_stall_abort, note_task_retry, set_task_bytes, set_task_phase,
};
use super::conflict::{ResolvedConflict, resolve_volume_conflict};
use super::preflight::SourceHint;
use super::transfer_error::{AtPath, PathedVolumeError};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;

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
/// so a floor ≥ a typical file size means the copy would NEVER yield to a
/// foreground op for files smaller than the floor — i.e. it would disable
/// navigate-during-transfer for normal files. If you want it to read as a real
/// multi-window guard, raise it to a small multiple of `MTP_READ_WINDOW` (e.g.
/// 2-4× = "N windows between yields"); that changes behavior, so re-verify on a
/// real device.
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
/// `checkpoint_stream.rs::dest_park_continues`.
const DEST_FOREGROUND_YIELD_HARD_CAP: Duration = Duration::from_secs(1);

/// The (debounce, min-progress-floor, dest-yield-hard-cap) tuple a freshly-built
/// `CheckpointStream` uses. Production always returns the named constants. Tests
/// override all three (debounce ≈ 0, a tiny floor, a short cap) via
/// [`AutoYieldTuningGuard`] so both the source and destination auto-yield arms are
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

/// Context threaded into the recursive merge walk so each pre-existing level can
/// resolve its clashing children through the same conflict machinery the
/// top-level copy uses (Stop-wait, the apply-to-all latch, conditional reduce,
/// type mismatches), without widening `copy_directory_streaming`'s already-long
/// argument list per item.
///
/// `None` means "no conflict resolution" — the caller is a path that streams a
/// directory into a brand-new destination where nothing can clash (the
/// cross-volume move's copy phase, or a plain non-merging copy). In that case
/// every `create_directory` either succeeds fresh or — if the dest happens to
/// already hold a same-named dir — the walk still merges structurally, but
/// per-child file clashes overwrite blindly (today's behavior for that path).
/// The volume copy/move pipelines pass `Some(_)` so deep clashes honor the
/// user's file policy.
pub(super) struct MergeCtx<'a> {
    pub events: &'a dyn OperationEventSink,
    pub operation_id: &'a str,
    pub config: &'a VolumeCopyConfig,
    /// The operation's shared state — carries the cancel `intent`, the
    /// `conflict_resolution_tx` oneshot slot, and the `conflict_dispatch_lock`
    /// the resolver uses to serialize the human across concurrent merges.
    pub state: &'a Arc<WriteOperationState>,
    /// Op-wide apply-to-all latch, shared between the top-level dispatch and
    /// every deep merge level so a "…all" choice applies everywhere. Held only
    /// briefly per resolve (copy out → run the async resolver on the stack local
    /// → store back), mirroring the serial top-level path; the `Cancelled`-safe
    /// serialization of the human is the `conflict_dispatch_lock`'s job, not
    /// this cell's.
    pub apply_to_all: &'a Mutex<ApplyToAll>,
    /// Per-source-path hints from the preflight scan. Deep merge children aren't
    /// top-level sources, so they never have a hint — the resolver falls back to
    /// trait calls for them (the size/mtime annotations come from `get_metadata`
    /// on the Stop path only, bounded by the user's click time).
    pub source_hints: &'a HashMap<PathBuf, SourceHint>,
}

/// Records exactly what a single `copy_single_path` call wrote to the
/// destination, so rollback can remove only what this operation created — never
/// dest-only files that pre-existed a merged destination directory.
///
/// A directory source merges into an existing dest directory ("Overwrite means
/// merge for dirs"), so recording the top-level dest directory and recursively
/// deleting it on rollback would destroy the user's untouched files. Instead we
/// record:
/// - `files`: every destination FILE path the copy streamed, in write order.
///   Rollback deletes these individually.
/// - `dirs`: every destination DIRECTORY this copy newly created (i.e. the
///   `create_directory` call returned `Ok`, not `AlreadyExists`), in
///   creation order (shallowest first). Rollback removes these with a
///   non-recursive delete (empty-only on real backends), deepest first, so a
///   directory that still holds a pre-existing sibling survives.
// DEFAULT-OK: an empty ledger is an operation that has created nothing yet, which is the
// one state where rollback correctly has nothing to undo.
#[derive(Default)]
pub(super) struct CreatedPaths {
    pub files: Mutex<Vec<PathBuf>>,
    pub dirs: Mutex<Vec<PathBuf>>,
    // Children a DEEP merge resolved to Skip (a conflict the user/policy
    // declined). Invisible to the top-level driver, so tallied here; the
    // move-out op reads the count to keep a not-fully-extracted source in the
    // archive (see `skipped_file_count`).
    pub skipped_files: std::sync::atomic::AtomicUsize,
    pub skipped_bytes: std::sync::atomic::AtomicU64,
    // The SOURCE path of each skipped child. A skipped child never landed at
    // the destination, so the source copy is the only one: a MOVE sweeps its
    // source folder while preserving exactly these (see
    // `skipped_source_paths`).
    pub skipped_sources: Mutex<Vec<PathBuf>>,
    // Deep-merge children that REPLACED an existing dest file. The operation-log
    // capture reads this per source (`overwrote_count`) so a copy / move whose
    // subtree overwrote anything finalizes `not_rollbackable` — deleting the
    // copies can't bring the overwritten originals back.
    pub overwrote_files: std::sync::atomic::AtomicUsize,
}

impl CreatedPaths {
    pub(super) fn record_file(&self, path: PathBuf) {
        self.files.lock_ignore_poison().push(path);
    }

    fn record_dir(&self, path: PathBuf) {
        self.dirs.lock_ignore_poison().push(path);
    }

    /// Tally one child a deep merge skipped (conflict resolved to Skip), and
    /// remember its SOURCE path so a move's source sweep can spare it.
    fn record_skip(&self, source: PathBuf, size: u64) {
        use std::sync::atomic::Ordering;
        self.skipped_files.fetch_add(1, Ordering::Relaxed);
        self.skipped_bytes.fetch_add(size, Ordering::Relaxed);
        self.skipped_sources.lock_ignore_poison().push(source);
    }

    /// Tally one child a deep merge overwrote (replaced an existing dest file).
    fn record_overwrite(&self) {
        self.overwrote_files.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Whether this copy overwrote any existing dest file in its subtree — the
    /// operation-log capture uses it (with the top-level file→file overwrite) to
    /// mark the op `not_rollbackable`.
    pub(super) fn any_overwrote(&self) -> bool {
        self.overwrote_files.load(std::sync::atomic::Ordering::Relaxed) > 0
    }

    /// How many children this copy skipped (deep merge Skips). `0` means the
    /// whole subtree landed; the move-out op keys its per-source archive delete
    /// on this.
    pub(super) fn skipped_file_count(&self) -> usize {
        self.skipped_files.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Total byte size of the skipped children, for folding into the op-wide
    /// skipped-bytes tally.
    pub(super) fn skipped_byte_count(&self) -> u64 {
        self.skipped_bytes.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// The source paths a deep merge skipped. A MOVE passes these to
    /// `remove_tree` so its source sweep spares the
    /// children that never landed at the destination — deleting them would
    /// destroy the user's only copy.
    pub(super) fn skipped_source_paths(&self) -> std::collections::HashSet<PathBuf> {
        self.skipped_sources.lock_ignore_poison().iter().cloned().collect()
    }
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
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, the size hint, shared state, the rollback ledger, and two progress callbacks. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: Option<bool>,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn(u64) + Sync),
    // `Some` ⇒ deep clashes inside a merged directory honor the user's file
    // policy (Stop-wait, latch, conditional reduce, type mismatches). `None` ⇒
    // no per-child conflict resolution (the cross-volume move's copy phase,
    // where the dest is a fresh staging area, and tests that don't merge).
    merge: Option<&MergeCtx<'_>>,
    // Whether `dest_path` is the file's final name (`Stage`) or a `.cmdr-tmp-*`
    // the caller already minted for a safe-replace and will land itself
    // (`AlreadyStaged`). Every call site derives it the same way:
    // `replace_after_write.is_some()`. Only the FILE branch reads it — a
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
                on_file_progress,
                on_file_complete,
                merge,
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
            on_file_progress,
            on_file_complete,
            merge,
            None,
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
        let bytes = stream_pipe_file(
            source_volume,
            source_path,
            source_size_hint,
            dest_volume,
            dest_path,
            state,
            on_file_progress,
            staging,
        )
        .await
        .at(source_path)?;
        on_file_complete(bytes);
        Ok(bytes)
    }
}

/// The staging every call site derives the same way: a conflict resolution that
/// handed back a temp to swap over an original (`Some(orig)`) already staged the
/// write, anything else is ours to stage.
pub(super) fn staging_for(replace_after_write: &Option<PathBuf>) -> WriteStaging {
    if replace_after_write.is_some() {
        WriteStaging::AlreadyStaged
    } else {
        WriteStaging::Stage
    }
}

/// Drops the staging for a write the DESTINATION lands in one indivisible shot.
///
/// Staging keeps a byte-incomplete file from wearing the user's real filename.
/// A single-shot write has no in-between state to protect against — it either
/// lands whole or leaves nothing — so the `.cmdr-tmp-*` and the rename that
/// lands it would buy nothing and cost a round trip per file (on SMB that
/// roughly doubles the wire cost of a file the compound fast path finishes in
/// one frame; on a 10k-tiny-file copy to a NAS that is the whole difference).
///
/// ❌ The question is single-shot-ness, never smallness, and only the
/// destination can answer it: `size` goes to `Volume::write_is_single_shot` (the
/// same number `write_from_stream` gets, off the same stream) and the backend
/// answers with the very condition its one-shot path branches on. A caller-side
/// size threshold would drift from that condition the day a backend retunes it,
/// and drifting apart means truncated files at real names again.
///
/// `AlreadyStaged` is never touched: the caller's temp keeps the ORIGINAL file
/// in place until the new bytes are complete, which is a stronger guarantee than
/// single-shot-ness and the caller's to land.
pub(super) async fn resolve_staging(requested: WriteStaging, dest_volume: &Arc<dyn Volume>, size: u64) -> WriteStaging {
    if requested == WriteStaging::Stage && dest_volume.write_is_single_shot(size).await {
        WriteStaging::SingleShot
    } else {
        requested
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
    let on_progress = |_written: u64, _total: u64| {
        if super::super::super::state::is_cancelled(&state.intent) {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    };
    let on_complete = |_bytes: u64| {};
    copy_single_path(
        source_volume,
        source_path,
        // The caller probed the source itself, so this is a known answer.
        Some(source_is_directory),
        // No size hint: the stream reports the REAL length, so a source whose
        // listed metadata size lies still pulls its true bytes.
        None,
        dest_volume,
        dest_path,
        state,
        &created,
        &on_progress,
        &on_complete,
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
async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    staging: WriteStaging,
) -> Result<u64, VolumeError> {
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
    let mut staging = staging;
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
        // that hangs before the first byte is half of the wedge shape. Nothing is
        // staged yet, so the abort just returns.
        set_task_phase(TaskPhase::OpeningSource);
        let stream = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => return Err(hard_abort_error(source_path)),
            opened = source_volume.open_read_stream_with_hint(source_path, source_size_hint) => opened?,
        };
        let size = stream.total_size();
        let resolved_staging = resolve_staging(staging, dest_volume, size).await;
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
        ));
        set_task_phase(TaskPhase::Streaming);
        set_task_bytes(0, size);
        // The watchdog ACTING (M4.2): a task that sits inside a backend call with
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
        let stall_abort = if resolved_staging == WriteStaging::SingleShot {
            None
        } else {
            arm_current_task_stall_abort()
        };
        //
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
        let write_fut = dest_volume.write_from_stream(staged.target(), size, stream, on_file_progress);
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
                return Err(hard_abort_error(dest_path));
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
                    return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
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
                return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
            }
            Err(e) => {
                // The staged bytes are a partial (a mid-stream failure, or the
                // cancel the backend turned into `Cancelled`); drop them.
                staged.abandon(dest_volume).await;
                if attempt > 1 {
                    log::warn!(
                        target: "copy",
                        "stream_pipe_file: giving up on {} after {attempt} attempt(s): {e}",
                        dest_path.display(),
                    );
                }
                return Err(e);
            }
        };

        // Past the last byte: give the file its final name.
        match staged.commit(dest_volume).await {
            Ok(()) => return Ok(bytes),
            Err(VolumeError::NotSupported) if staging == WriteStaging::Stage => {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: destination can't land a staged write for {}; falling back to writing at the final name",
                    dest_path.display()
                );
                staging = WriteStaging::AlreadyStaged;
                continue;
            }
            // The write SUCCEEDED and the landing didn't: the temp holds the only
            // complete copy of the new bytes, and `commit` already dropped it from
            // the in-flight set so nothing sweeps it. Surface the failure.
            Err(e) => return Err(e),
        }
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

/// What tier 2 reports when it ends a wait.
///
/// A `Cancelled`, deliberately, and it decides three things at once: `retry.rs`
/// never re-runs a cancel, the post-loop keys `write-cancelled` off a
/// `Cancelled`-shaped error (so an abort closes the dialog instead of logging a
/// failed transfer), and no caller mistakes it for a transport fault worth
/// reporting to the user.
fn hard_abort_error(path: &Path) -> VolumeError {
    VolumeError::Cancelled(format!("stopped waiting for {} so the app can quit", path.display()))
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

/// Resolve `dest_path` against `dest_volume.local_path()` and register it
/// with the downloads watcher's ignore set. Skips silently when
/// `dest_volume` isn't local-FS-backed (MTP, SMB, in-memory): those paths
/// would never trigger the watcher anyway, and synthesizing a non-local
/// path into the ignore set would just churn the map for no benefit.
pub(super) fn note_pending_for_local_dest(dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
    let Some(root) = dest_volume.local_path() else {
        return;
    };
    // Mirror `LocalPosixVolume::resolve`'s absolute-path handling so the
    // path we register matches the one `write_from_stream` will hit.
    let absolute = if dest_path.as_os_str().is_empty() || dest_path == Path::new(".") {
        root
    } else if dest_path.is_absolute() {
        if dest_path.starts_with(&root) || root == Path::new("/") {
            dest_path.to_path_buf()
        } else {
            root.join(dest_path.strip_prefix("/").unwrap_or(dest_path))
        }
    } else {
        root.join(dest_path)
    };
    crate::downloads::note_pending_write_for_cmdr(&absolute);
}

/// Recursively copies (merges) a directory tree from source to destination,
/// streaming each file through `write_from_stream`. Checks cancellation between
/// entries.
///
/// ## Scan-as-you-merge
///
/// The merge discovers deep conflicts inline, level by level, with no upfront
/// recursive pre-scan. The trigger is the destination directory's existence:
///
/// - `create_directory` returns `Ok(())` ⇒ WE created this level fresh. Nothing
///   inside it can clash, so we skip the dest listing entirely and stream every
///   source child straight in.
/// - `create_directory` returns `AlreadyExists` ⇒ we're MERGING into the user's
///   pre-existing directory. We list the dest level ONCE and build a
///   `name → FileEntry` map, then for each source child that hits the map we
///   dispatch through the conflict resolver (file policy: Stop-wait, latch,
///   conditional reduce, type mismatches) — EXCEPT dir-vs-dir, which recurses
///   unconditionally (a folder landing on a folder always merges, never
///   prompts). A child with no map hit is copied straight in. One listing per
///   level, in-memory lookups after — no per-child `get_metadata` probes.
///
/// The `Ok` vs `AlreadyExists` split also drives rollback: `Ok` records the dir
/// in `created` (rollback may remove it once empty); `AlreadyExists` does NOT,
/// so rollback never touches the user's pre-existing directory — only the files
/// we wrote into it. This is what keeps a merge from destroying dest-only files.
///
/// When `merge` is `None`, there's no per-child conflict resolution: a clashing
/// dest file is overwritten blindly (the cross-volume move's copy phase, where
/// the dest is fresh staging, plus tests that never merge). `Some` is what the
/// volume copy / cross-volume move pipelines pass so deep clashes honor policy.
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors copy_single_path's argument list plus the rollback ledger, merge context, and the sequential-extract plan sink; bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_directory_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn(u64) + Sync),
    merge: Option<&MergeCtx<'_>>,
    // `Some` ⇒ PLAN MODE for the one-pass sequential extractor: create the
    // destination directory structure and resolve every file's conflict as usual,
    // but instead of streaming each file's bytes, record its resolved destination
    // in the plan and leave the byte write to the caller's single decode pass.
    // `None` ⇒ normal streaming copy.
    plan: Option<&super::sequential_extract::ExtractPlan>,
) -> Result<u64, PathedVolumeError> {
    note_pending_for_local_dest(dest_volume, dest_path);

    // Ensure the destination directory exists, and learn whether THIS level
    // pre-existed (a merge) or we created it fresh.
    //
    // Every backend EXCEPT MTP surfaces "already exists" as
    // `VolumeError::AlreadyExists` (SMB needs smb2 ≥ 0.8.0 to typed-classify
    // STATUS_OBJECT_NAME_COLLISION). MTP's `create_directory` does NOT error on
    // a same-name dir — the MTP protocol allows same-name sibling objects, so a
    // blind `create_folder` would make a duplicate `photos` and the merge would
    // target the WRONG dir. So on MTP (and any backend whose `create_directory`
    // can't be trusted to error on collision) we pre-check existence with the
    // one listing the merge level pays anyway, and skip the create when present.
    let level_pre_existed = if backend_create_directory_detects_collisions(dest_volume) {
        match dest_volume.create_directory(dest_path).await {
            Ok(()) => {
                created.record_dir(dest_path.to_path_buf());
                false
            }
            Err(VolumeError::AlreadyExists(_)) => true,
            Err(VolumeError::NotSupported) => {
                // Backend can't create directories at all; assume
                // `write_from_stream` materializes parents on demand (LocalPosix
                // does via `create_dir_all` semantics). Treat as fresh.
                false
            }
            Err(e) => return Err(e).at(source_path),
        }
    } else {
        // Untrusted-collision backend (MTP): pre-check existence.
        if dest_volume.exists(dest_path).await {
            true
        } else {
            match dest_volume.create_directory(dest_path).await {
                Ok(()) => {
                    created.record_dir(dest_path.to_path_buf());
                    false
                }
                // A race created it between the check and the create; merge.
                Err(VolumeError::AlreadyExists(_)) => true,
                Err(VolumeError::NotSupported) => false,
                Err(e) => return Err(e).at(source_path),
            }
        }
    };

    // Build the dest name→entry map ONCE, only for a pre-existing (merging)
    // level. A freshly-created level can't clash, so we never list it.
    let dest_by_name: HashMap<String, FileEntry> = if level_pre_existed {
        dest_volume
            .list_directory(dest_path, None)
            .await
            .at(source_path)?
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect()
    } else {
        HashMap::new()
    };

    let entries = source_volume.list_directory(source_path, None).await.at(source_path)?;
    let mut total_bytes = 0u64;

    for entry in &entries {
        if super::super::super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string())).at(source_path);
        }

        let child_source = PathBuf::from(&entry.path);
        let child_dest = dest_path.join(&entry.name);
        let dest_hit = dest_by_name.get(&entry.name);

        if entry.is_directory {
            // Dir-vs-dir (and dir-into-nothing) always recurses to merge — no
            // resolver call for the folder itself. A dir landing on a same-named
            // FILE is a type mismatch, which the resolver (below) handles.
            let dir_clashes_with_file = dest_hit.is_some_and(|d| !d.is_directory);
            if !dir_clashes_with_file {
                total_bytes += Box::pin(copy_directory_streaming(
                    source_volume,
                    &child_source,
                    dest_volume,
                    &child_dest,
                    state,
                    created,
                    on_file_progress,
                    on_file_complete,
                    merge,
                    plan,
                ))
                .await?;
                continue;
            }
        }

        // At this point the child is either a FILE, or a directory clashing with
        // a same-named dest FILE (type mismatch). If there's a dest hit and we
        // have merge context, route it through the file-policy resolver.
        let mut write_dest = child_dest.clone();
        let mut replace_after_write: Option<PathBuf> = None;
        if let Some(hit) = dest_hit
            && let Some(ctx) = merge
        {
            match resolve_merge_child(ctx, source_volume, &child_source, entry, dest_volume, &child_dest, hit)
                .await
                .at(&child_source)?
            {
                MergeChildDecision::Skip => {
                    // A DEEP skip: record it so the caller knows this subtree did
                    // not extract in full (the move-out op must keep the source in
                    // the archive; deleting it would drop this un-landed child).
                    created.record_skip(child_source.clone(), entry.size.unwrap_or(0));
                    continue;
                }
                MergeChildDecision::Proceed { write_path, replace } => {
                    write_dest = write_path;
                    replace_after_write = replace;
                }
            }
        }

        if entry.is_directory {
            // Type-mismatch Overwrite/Rename that resolved to Proceed: the
            // resolver already cleared/relocated the dest file, so recurse into
            // `write_dest` as a fresh (or renamed) directory root.
            total_bytes += Box::pin(copy_directory_streaming(
                source_volume,
                &child_source,
                dest_volume,
                &write_dest,
                state,
                created,
                on_file_progress,
                on_file_complete,
                merge,
                plan,
            ))
            .await?;
            continue;
        }

        // PLAN MODE (one-pass sequential extract): the destination + conflict are
        // resolved; record the write and let the caller's single decode pass
        // stream the bytes. Don't stream, count, record, or emit progress here —
        // the data pass owns all of that. The directory structure and conflict
        // prompts still happened above, exactly as a streaming copy would.
        if let Some(plan) = plan {
            plan.record(
                child_source,
                super::sequential_extract::PlannedWrite {
                    dest_path: write_dest,
                    replace_after_write,
                },
            );
            continue;
        }

        // ❗ `.at(&child_source)` is the whole point: this is the deepest frame
        // that knows WHICH file failed. Report it one level up and the user gets
        // the name of the folder they selected instead of the file that broke.
        let bytes = stream_pipe_file(
            source_volume,
            &child_source,
            entry.size,
            dest_volume,
            &write_dest,
            state,
            on_file_progress,
            staging_for(&replace_after_write),
        )
        .await
        .at(&child_source)?;
        // Safe-replace finalize for a file→file Overwrite: the temp now holds
        // the complete new bytes; swap it over the original. On finalize error
        // the temp is preserved as committed data (see `finalize_safe_replace`).
        let recorded = match replace_after_write {
            Some(orig) => {
                super::conflict::finalize_safe_replace(dest_volume, &write_dest, &orig)
                    .await
                    .at(&child_source)?;
                // A deep-merge child that replaced an existing dest file: record
                // the overwrite so the operation-log eligibility is honest (a copy
                // / move that overwrote isn't rollbackable — the original is gone).
                created.record_overwrite();
                orig
            }
            None => write_dest,
        };
        created.record_file(recorded);
        total_bytes += bytes;
        on_file_complete(bytes);
    }

    Ok(total_bytes)
}

/// Whether this backend's `create_directory` reliably returns
/// `VolumeError::AlreadyExists` when a same-name directory already exists.
///
/// `true` for LocalPosix (`std::fs::create_dir` → `ErrorKind::AlreadyExists`),
/// SMB (smb2 typed STATUS_OBJECT_NAME_COLLISION), and InMemoryVolume's
/// merge-test variant. `false` for MTP: the protocol allows same-name sibling
/// objects and `create_folder` happily makes a duplicate, so the merge walker
/// must pre-check existence instead of trusting the create to error.
fn backend_create_directory_detects_collisions(volume: &Arc<dyn Volume>) -> bool {
    volume.create_directory_errors_on_existing_dir()
}

/// Outcome of resolving one clashing child inside a merge.
enum MergeChildDecision {
    /// Honor a Skip: do NOT touch the dest child at all.
    Skip,
    /// Proceed writing to `write_path`; `replace` is `Some(orig)` for a
    /// file→file safe-replace (write to a temp sibling, finalize after).
    Proceed {
        write_path: PathBuf,
        replace: Option<PathBuf>,
    },
}

/// Dispatches one clashing merge child through the volume conflict resolver,
/// reusing the op-wide apply-to-all latch so a "…all" choice from any level (top
/// or deep) applies here. Mirrors the serial top-level path's latch handling:
/// copy the latch out of the shared cell, run the async resolver on the stack
/// local, store it back. The `conflict_dispatch_lock` inside the resolver — not
/// this cell — is what serializes the human across concurrent merges.
async fn resolve_merge_child(
    ctx: &MergeCtx<'_>,
    source_volume: &Arc<dyn Volume>,
    child_source: &Path,
    entry: &FileEntry,
    dest_volume: &Arc<dyn Volume>,
    child_dest: &Path,
    dest_hit: &FileEntry,
) -> Result<MergeChildDecision, VolumeError> {
    // Deep children aren't top-level sources, so no preflight hint exists for
    // them; the resolver falls back to trait calls. We DO know both sides' type
    // and size from the listing entries already in hand — the source's from this
    // level's source listing, the destination's from the `dest_by_name` map the
    // caller built for the same level. That saves the resolver a redundant
    // `is_directory` probe and seeds the dialog's size annotations.
    //
    // ❗ The dest size matters beyond display: it's what `OverwriteSmaller`
    // compares against. Passing `None` here used to leave the resolver
    // fabricating a `0`, which made every destination look smaller.
    let source_is_directory_hint = Some(entry.is_directory);
    let source_size_hint = if entry.is_directory { None } else { entry.size };
    let dest_size_hint = if dest_hit.is_directory { None } else { dest_hit.size };
    let _ = ctx.source_hints; // hints are keyed by top-level source path; deep children never match

    let mut latched = *ctx.apply_to_all.lock_ignore_poison();
    let resolved = resolve_volume_conflict(
        source_volume,
        child_source,
        dest_volume,
        child_dest,
        ctx.config,
        ctx.events,
        ctx.operation_id,
        ctx.state,
        &mut latched,
        source_size_hint,
        dest_size_hint,
        source_is_directory_hint,
    )
    .await;
    *ctx.apply_to_all.lock_ignore_poison() = latched;

    match resolved {
        Ok(None) => Ok(MergeChildDecision::Skip),
        Ok(Some(ResolvedConflict {
            write_path,
            replace_after_write,
        })) => Ok(MergeChildDecision::Proceed {
            write_path,
            replace: replace_after_write,
        }),
        // The resolver returns a typed `WriteOperationError`; map cancellation
        // back to the `VolumeError::Cancelled` this function's callers expect so
        // the post-loop reclassifies it as a cancel, not a transport error.
        Err(WriteOperationError::Cancelled { .. }) => Err(VolumeError::Cancelled("Operation cancelled by user".into())),
        Err(other) => Err(VolumeError::IoError {
            message: format!("conflict resolution failed: {other:?}"),
            raw_os_error: None,
        }),
    }
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
#[path = "strategy_yield_tests.rs"]
mod yield_tests;
