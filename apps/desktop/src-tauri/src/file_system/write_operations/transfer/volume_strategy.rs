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

use std::future::Future;
use std::pin::Pin;

use super::super::conflict::ApplyToAll;
use super::super::state::WriteOperationState;
use super::super::types::{OperationEventSink, VolumeCopyConfig, WriteOperationError};
use super::volume_conflict::{ResolvedConflict, resolve_volume_conflict};
use super::volume_preflight::SourceHint;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{Volume, VolumeError, VolumeReadStream};
use crate::ignore_poison::IgnorePoison;

/// Wraps a source read stream so a between-chunk cooperative checkpoint runs
/// once per chunk for the cross-volume streaming path, where the per-chunk
/// progress callback (`on_progress`) is sync and so can't `.await` to park or
/// yield.
///
/// Before delegating each `next_chunk().await`, the wrapper:
///
/// 1. **Handles pause**, in one of two ways depending on the source backend.
///    **Release-on-pause** (MTP, `pause_releases_read_stream() == true`): close the
///    in-flight source stream so its scarce resource is freed — for MTP the
///    one-per-device PTP session, so the phone can be listed/navigated while
///    paused — then on resume reopen the source at the byte count already
///    delivered (`open_read_stream_at_offset`) and continue (the "navigate while
///    paused" feature). **Park-in-place** (local FS, SMB, in-memory): just block on
///    `pause_gate.wait_while_paused_async` holding the open stream, since those
///    streams hold nothing a pause needs to free. Either way the checkpoint runs
///    at a chunk boundary — the previous chunk is fully written and the next is
///    not yet read — so a paused op holds only its in-flight `.cmdr-tmp-<uuid>`,
///    never a torn target. Resume or cancel unblocks it
///    (`wait_while_paused_async` returns the instant cancel is observed;
///    cancellation wins over pause).
/// 2. **Auto-yields the device to foreground work** (release-on-pause backends
///    that opt in via `Volume::supports_foreground_yield()`, i.e. MTP). When a
///    foreground op (a listing / nav on the phone) is pending on the source
///    device, the checkpoint behaves like the index scan's
///    `background_yield_point`: it RELEASES the in-flight download (freeing the
///    PTP session so the listing's device ops get the lock), parks until
///    foreground drains plus a short debounce window, then reopens at
///    `bytes_yielded` and resumes — all WITHOUT a user pause, and the op stays
///    `Running` (this is a transient device yield, not user intent). A
///    minimum-progress floor (`min_progress_floor`) gates the arm so continuous
///    foreground nav can't starve the copy to zero throughput: after a resume the
///    transfer must move at least `min_progress_floor` bytes before honoring the
///    next yield. The debounce (`foreground_debounce`) collapses a burst of
///    listings into ONE release/reopen instead of one per chunk.
/// 3. **Yields cooperatively** (`tokio::task::yield_now`) so a long transfer
///    doesn't starve foreground tasks (listings, navigation, progress emits) on
///    the runtime.
///
/// **Byte exactness across a release/reopen.** The wrapper counts `bytes_yielded`
/// and reopens at exactly that offset, so the source delivers `[bytes_yielded,
/// size)` with no gap or overlap. The destination's `write_from_stream` writes
/// every yielded chunk sequentially to its temp, so the temp's length always
/// equals `bytes_yielded`; appending the reopened tail reconstructs the file
/// exactly. Safe-replace temp+rename on the destination is untouched — the source
/// reopen is invisible to it (it sees one continuous chunk stream).
///
/// Cancellation is NOT enforced here: the backend's existing `on_progress`
/// `is_cancelled` check after each write owns the cancel-then-cleanup ordering
/// (drop the handle, remove the partial). On the release-on-pause path a cancel
/// observed while parked reopens the source and lets the next chunk flow through
/// to that same `on_progress` check, so the cancel/cleanup contract is identical
/// to the park-in-place path. The wrapper never drops, double-writes, or reorders
/// bytes — it passes each inner chunk through untouched.
/// Debounce window for the foreground auto-yield: after foreground work drains,
/// the checkpoint stays parked until the device has been quiet for this long
/// before reopening, so a BURST of listings (e.g. arrow-keying down a folder
/// tree) is served as ONE release/reopen instead of one per chunk. Each reopen
/// pays a real `GetPartialObject64` re-setup, so collapsing the burst matters.
/// ~400 ms balances nav responsiveness (the copy is suspended the whole window)
/// against reopen thrash; tuned on real hardware in M2.
const FOREGROUND_YIELD_DEBOUNCE: Duration = Duration::from_millis(400);

/// Minimum-progress floor for the foreground auto-yield: after a resume, the
/// transfer must move at least this many bytes before it will honor the next
/// foreground yield. Without it, a continuous stream of foreground ops would
/// release the session every single chunk and starve the copy to zero
/// throughput. 4 MiB ≈ a handful of `GetPartialObject64` chunks — small enough
/// that nav still feels responsive (the copy yields again within a few chunks),
/// large enough that the copy always makes visible forward progress between
/// yields. Tuned on real hardware in M2.
const MIN_PROGRESS_FLOOR_BYTES: u64 = 4 * 1024 * 1024;

/// The (debounce, min-progress-floor) pair a freshly-built `CheckpointStream`
/// uses. Production always returns the named constants. Tests override both
/// (debounce ≈ 0, a tiny floor) via [`set_auto_yield_tuning_for_test`] so the
/// auto-yield arm is deterministic without real device latency or megabytes of
/// synthetic data. The stream construction lives behind `copy_single_path`, so a
/// thread-local override is how a test reaches it without widening the public
/// copy API.
fn auto_yield_tuning() -> (Duration, u64) {
    #[cfg(test)]
    {
        if let Some(t) = tests::auto_yield_tuning_override() {
            return t;
        }
    }
    (FOREGROUND_YIELD_DEBOUNCE, MIN_PROGRESS_FLOOR_BYTES)
}

struct CheckpointStream {
    /// The open source stream, or `None` while released during a pause OR a
    /// foreground auto-yield (both the release-on-pause and auto-yield paths set
    /// this to `None`; reopened at `bytes_yielded` by `ensure_open`).
    inner: Option<Box<dyn VolumeReadStream>>,
    state: Arc<WriteOperationState>,
    /// Bytes this wrapper has yielded so far == the destination temp's length ==
    /// the offset to reopen at after a release.
    bytes_yielded: u64,
    /// Reopen context, `Some` only when the source backend releases on pause
    /// (MTP). `None` ⇒ park-in-place (the original behavior).
    reopen: Option<CheckpointReopen>,
    /// `bytes_yielded` at the last resume (initial open = 0, then each reopen).
    /// The min-progress floor is measured from here: the auto-yield arm only
    /// fires once `bytes_yielded - last_resume_offset >= min_progress_floor`, so
    /// continuous foreground nav can't starve the copy to zero throughput.
    last_resume_offset: u64,
    /// Quiet window the auto-yield waits for before reopening. Field (not a bare
    /// constant) so tests can set it ≈ 0 for determinism; defaults to
    /// [`FOREGROUND_YIELD_DEBOUNCE`].
    foreground_debounce: Duration,
    /// Bytes the transfer must advance after a resume before honoring the next
    /// foreground yield. Field (not a bare constant) so tests can set a small
    /// floor for determinism; defaults to [`MIN_PROGRESS_FLOOR_BYTES`].
    min_progress_floor: u64,
}

/// What `CheckpointStream` needs to close the source on pause and reopen it at
/// the kept offset on resume. Present only for release-on-pause backends.
struct CheckpointReopen {
    source_volume: Arc<dyn Volume>,
    source_path: PathBuf,
    total_size: u64,
}

impl VolumeReadStream for CheckpointStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        Box::pin(async move {
            self.checkpoint().await;
            // After the checkpoint, a release-on-pause stream may need reopening.
            // If reopen failed (device gone), surface it as a normal transient
            // error so the copy aborts and the partial is handled like any other
            // mid-stream failure.
            let inner = match self.ensure_open().await {
                Ok(inner) => inner,
                Err(e) => return Some(Err(e)),
            };
            match inner.next_chunk().await {
                Some(Ok(chunk)) => {
                    self.bytes_yielded += chunk.len() as u64;
                    Some(Ok(chunk))
                }
                other => other,
            }
        })
    }

    fn total_size(&self) -> u64 {
        match &self.reopen {
            // While released, `inner` is None; report the cached full size.
            Some(r) => r.total_size,
            None => self.inner.as_ref().map_or(0, |s| s.total_size()),
        }
    }

    fn bytes_read(&self) -> u64 {
        self.bytes_yielded
    }
}

impl CheckpointStream {
    /// Run the between-chunk pause checkpoint and the foreground auto-yield, then
    /// yield cooperatively.
    async fn checkpoint(&mut self) {
        if self.reopen.is_some() {
            // Release-on-pause path. Only release if we're actually paused AND
            // there's remaining data worth holding the resource for: at EOF the
            // next `next_chunk` returns None and the copy completes regardless.
            let paused = self.state.pause_gate.is_paused();
            let cancelled = super::super::state::is_cancelled(&self.state.intent);
            let has_remaining = self.reopen.as_ref().is_some_and(|r| self.bytes_yielded < r.total_size);
            if paused && !cancelled && has_remaining {
                // Close the in-flight stream NOW so the scarce resource (MTP
                // session) is freed for the whole pause, then park. We reopen at
                // `bytes_yielded` in `ensure_open` after the park returns.
                if let Some(mut stream) = self.inner.take() {
                    stream.cancel_and_release().await;
                    // `stream` (and its session) is dropped here.
                }
            }
        }
        // Park between chunks while paused (returns immediately on cancel).
        self.state.pause_gate.wait_while_paused_async(&self.state.intent).await;

        // Foreground auto-yield: when a foreground op (listing / nav on the
        // phone) is pending on the source device, briefly release the session so
        // it gets the device, then reopen and resume — WITHOUT a user pause, op
        // stays Running. This is a transfer becoming a yielding background user of
        // the device gate, exactly like the index scan, except a transfer holds an
        // open download transaction, so "yield" means release + reopen.
        self.auto_yield_to_foreground().await;

        // Yield so foreground tasks get scheduled during a long copy.
        tokio::task::yield_now().await;
    }

    /// The foreground auto-yield arm (step 2 in the wrapper's doc). No-op unless
    /// the source opts in (`supports_foreground_yield()`, MTP only) and a
    /// foreground op is actually pending on its device.
    async fn auto_yield_to_foreground(&mut self) {
        // Clone the source handle + cache the total up front so the `self.reopen`
        // borrow ends before the later `self.inner.take()` (mutable) borrow.
        let (source_volume, total_size) = match self.reopen.as_ref() {
            Some(r) => (Arc::clone(&r.source_volume), r.total_size),
            None => return, // park-in-place backends never auto-yield
        };
        if !source_volume.supports_foreground_yield() {
            return;
        }
        if super::super::state::is_cancelled(&self.state.intent) {
            return; // cancel owns teardown; never start a yield while cancelled
        }
        // At EOF there's nothing left to hold the session for; let the copy finish.
        if self.bytes_yielded >= total_size {
            return;
        }
        // Min-progress floor: after a resume, transfer at least `min_progress_floor`
        // bytes before honoring the next yield, so continuous foreground nav can't
        // starve the copy to zero throughput.
        if self.bytes_yielded.saturating_sub(self.last_resume_offset) < self.min_progress_floor {
            return;
        }
        // Cheap probe (an atomic load behind the device gate); skip the release
        // entirely when nothing foreground is waiting.
        if !source_volume.foreground_pending().await {
            return;
        }

        // Release the in-flight stream NOW so the foreground listing's device ops
        // get the PTP session. `ensure_open` reopens at `bytes_yielded` after we
        // return — byte-exact, same as the release-on-pause path.
        if let Some(mut stream) = self.inner.take() {
            stream.cancel_and_release().await;
            // `stream` (and its session) is dropped here.
        }

        // Debounce: wait for foreground to drain, then a quiet window; if a new
        // foreground op arrives during the window, re-park. This collapses a
        // BURST of listings into ONE suspension instead of one release/reopen per
        // chunk. The whole loop is cancel-aware (a cancel breaks out promptly and
        // lets `ensure_open` + the backend's `on_progress` own cleanup).
        loop {
            if super::super::state::is_cancelled(&self.state.intent) {
                break;
            }
            // Park until the device is clear of foreground work, but RACE it
            // against cancellation: `wait_until_foreground_idle` only returns once
            // foreground drains, and a cancel doesn't clear the foreground signal,
            // so without this race a cancel-while-yielding would hang until the
            // (unrelated) foreground op happens to finish.
            tokio::select! {
                () = source_volume.wait_until_foreground_idle() => {}
                () = poll_until_cancelled(&self.state.intent) => break,
            }
            if super::super::state::is_cancelled(&self.state.intent) {
                break;
            }
            // Stay parked for the quiet window. If foreground becomes pending
            // again before it elapses (a burst), loop and re-drain.
            if sleep_cancel_aware(&self.state.intent, self.foreground_debounce).await {
                break; // cancelled during the wait
            }
            if !source_volume.foreground_pending().await {
                break; // quiet for the full window ⇒ resume
            }
        }
        // `ensure_open` reopens at `bytes_yielded` next and resets the floor
        // baseline (`last_resume_offset`), so the min-progress floor restarts.
    }

    /// Ensure `self.inner` is open, reopening at the kept offset if it was
    /// released during a pause. Returns the open stream.
    async fn ensure_open(&mut self) -> Result<&mut Box<dyn VolumeReadStream>, VolumeError> {
        if self.inner.is_none() {
            let reopen = self
                .reopen
                .as_ref()
                .expect("inner is only ever None on the release-on-pause path, which sets `reopen`");
            let stream = reopen
                .source_volume
                .open_read_stream_at_offset(&reopen.source_path, self.bytes_yielded)
                .await?;
            self.inner = Some(stream);
            // Reopening (after a pause-resume or a foreground auto-yield) restarts
            // the min-progress floor from the offset we resumed at, so the next
            // auto-yield can only fire after another `min_progress_floor` bytes.
            self.last_resume_offset = self.bytes_yielded;
        }
        Ok(self.inner.as_mut().expect("just ensured Some"))
    }
}

/// Sleep for `dur`, returning early if the operation is cancelled. Returns
/// `true` if it bailed on a cancel, `false` if the full window elapsed.
///
/// Cancel-awareness matters: a cancel during the auto-yield debounce wait must
/// not be slept through. We slice the sleep and re-check `is_cancelled` between
/// slices, so a cancel is observed within at most one slice. (A zero/near-zero
/// debounce — what tests inject — returns at the first check, before any sleep.)
///
/// Free function (not a method) so it borrows only the cancel atomic, not
/// `&CheckpointStream`: a `&self` held across an `.await` would force the
/// wrapper's `next_chunk` future to require `CheckpointStream: Sync`, which it
/// isn't (it holds a non-`Sync` `dyn VolumeReadStream`).
async fn sleep_cancel_aware(intent: &std::sync::atomic::AtomicU8, dur: Duration) -> bool {
    const SLICE: Duration = Duration::from_millis(20);
    let mut remaining = dur;
    loop {
        if super::super::state::is_cancelled(intent) {
            return true;
        }
        if remaining.is_zero() {
            return false;
        }
        let step = remaining.min(SLICE);
        tokio::time::sleep(step).await;
        remaining = remaining.saturating_sub(step);
    }
}

/// Resolves once the operation is cancelled; otherwise never. Used to RACE the
/// auto-yield's `wait_until_foreground_idle` against a cancel via `select!`: the
/// device gate only releases when foreground drains, and a cancel doesn't clear
/// the foreground signal, so a cancel-while-parked needs a separate waker. We
/// poll the intent (a cheap relaxed atomic load) on a short tick rather than
/// reach into the gate's internals; the latency is bounded by the tick.
async fn poll_until_cancelled(intent: &std::sync::atomic::AtomicU8) {
    const TICK: Duration = Duration::from_millis(20);
    while !super::super::state::is_cancelled(intent) {
        tokio::time::sleep(TICK).await;
    }
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
#[derive(Default)]
pub(super) struct CreatedPaths {
    pub files: Mutex<Vec<PathBuf>>,
    pub dirs: Mutex<Vec<PathBuf>>,
}

impl CreatedPaths {
    fn record_file(&self, path: PathBuf) {
        self.files.lock_ignore_poison().push(path);
    }

    fn record_dir(&self, path: PathBuf) {
        self.dirs.lock_ignore_poison().push(path);
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
#[allow(
    clippy::too_many_arguments,
    reason = "Cross-volume copy needs source/dest volumes, paths, the source type hint, the size hint, shared state, the rollback ledger, and two progress callbacks. Bundling into a struct adds ceremony without cleaning anything up."
)]
pub(super) async fn copy_single_path(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_is_directory: bool,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
    // `Some` ⇒ deep clashes inside a merged directory honor the user's file
    // policy (Stop-wait, latch, conditional reduce, type mismatches). `None` ⇒
    // no per-child conflict resolution (the cross-volume move's copy phase,
    // where the dest is a fresh staging area, and tests that don't merge).
    merge: Option<&MergeCtx<'_>>,
) -> Result<u64, VolumeError> {
    // Check cancellation up front.
    if super::super::state::is_cancelled(&state.intent) {
        return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
    }

    if source_is_directory {
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
        )
        .await?;
        on_file_complete();
        Ok(bytes)
    }
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
async fn stream_pipe_file(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    source_size_hint: Option<u64>,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
) -> Result<u64, VolumeError> {
    log::debug!("stream_pipe_file: {} -> {}", source_path.display(), dest_path.display());

    // Register the destination with the downloads watcher's ignore set
    // when the destination is local-FS-backed (the only case where the
    // watcher could otherwise fire). Covers MTP→Local and SMB→Local
    // imports that land in ~/Downloads.
    note_pending_for_local_dest(dest_volume, dest_path);

    // One-shot retry on a stale destination handle. A destination backend (MTP)
    // can reject the write because the cached handle for the destination folder
    // went stale — the device re-keyed its object handles since the folder was
    // last listed (Android MediaProvider rescans). The backend refreshes its
    // cache and returns `StaleDestinationHandle`; we re-open the source stream
    // and try once more with the now-fresh handle. Safe to restart the whole
    // file: the rejection lands at `SendObjectInfo`, before any source byte is
    // read or any destination byte is written, so no progress is double-counted
    // and no partial lingers.
    // Whether a pause should RELEASE this source's stream (free a scarce
    // resource, e.g. MTP's PTP session) and reopen at the kept offset on resume,
    // vs. park the open stream in place. Decided once per file from the source
    // backend; the source path is stable across the safe-replace retry below.
    let release_on_pause = source_volume.pause_releases_read_stream();

    let mut retried = false;
    loop {
        let stream = source_volume
            .open_read_stream_with_hint(source_path, source_size_hint)
            .await?;
        let size = stream.total_size();
        // Wrap so a paused op parks (and a long copy yields) between chunks.
        // `size` is read off the raw stream first — the wrapper forwards
        // `total_size()` unchanged, so the destination still sees the real size.
        // For a release-on-pause source, the wrapper also gets the context it
        // needs to reopen at the kept offset after releasing on pause.
        let reopen = release_on_pause.then(|| CheckpointReopen {
            source_volume: Arc::clone(source_volume),
            source_path: source_path.to_path_buf(),
            total_size: size,
        });
        let (foreground_debounce, min_progress_floor) = auto_yield_tuning();
        let stream: Box<dyn VolumeReadStream> = Box::new(CheckpointStream {
            inner: Some(stream),
            state: Arc::clone(state),
            bytes_yielded: 0,
            reopen,
            last_resume_offset: 0,
            foreground_debounce,
            min_progress_floor,
        });
        match dest_volume
            .write_from_stream(dest_path, size, stream, on_file_progress)
            .await
        {
            Err(VolumeError::StaleDestinationHandle(_)) if !retried => {
                retried = true;
                log::warn!(
                    "stream_pipe_file: destination handle for {} was stale; retrying once with the refreshed handle",
                    dest_path.display()
                );
                continue;
            }
            result => return result,
        }
    }
}

/// Resolve `dest_path` against `dest_volume.local_path()` and register it
/// with the downloads watcher's ignore set. Skips silently when
/// `dest_volume` isn't local-FS-backed (MTP, SMB, in-memory): those paths
/// would never trigger the watcher anyway, and synthesizing a non-local
/// path into the ignore set would just churn the map for no benefit.
fn note_pending_for_local_dest(dest_volume: &Arc<dyn Volume>, dest_path: &Path) {
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
    reason = "Mirrors copy_single_path's argument list plus the rollback ledger and merge context; bundling into a struct adds ceremony without cleaning anything up."
)]
async fn copy_directory_streaming(
    source_volume: &Arc<dyn Volume>,
    source_path: &Path,
    dest_volume: &Arc<dyn Volume>,
    dest_path: &Path,
    state: &Arc<WriteOperationState>,
    created: &CreatedPaths,
    on_file_progress: &(dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    on_file_complete: &(dyn Fn() + Sync),
    merge: Option<&MergeCtx<'_>>,
) -> Result<u64, VolumeError> {
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
            Err(e) => return Err(e),
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
                Err(e) => return Err(e),
            }
        }
    };

    // Build the dest name→entry map ONCE, only for a pre-existing (merging)
    // level. A freshly-created level can't clash, so we never list it.
    let dest_by_name: HashMap<String, FileEntry> = if level_pre_existed {
        dest_volume
            .list_directory(dest_path, None)
            .await?
            .into_iter()
            .map(|e| (e.name.clone(), e))
            .collect()
    } else {
        HashMap::new()
    };

    let entries = source_volume.list_directory(source_path, None).await?;
    let mut total_bytes = 0u64;

    for entry in &entries {
        if super::super::state::is_cancelled(&state.intent) {
            return Err(VolumeError::Cancelled("Operation cancelled by user".to_string()));
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
        if dest_hit.is_some()
            && let Some(ctx) = merge
        {
            match resolve_merge_child(ctx, source_volume, &child_source, entry, dest_volume, &child_dest).await? {
                MergeChildDecision::Skip => continue,
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
            ))
            .await?;
            continue;
        }

        let bytes = stream_pipe_file(
            source_volume,
            &child_source,
            entry.size,
            dest_volume,
            &write_dest,
            state,
            on_file_progress,
        )
        .await?;
        // Safe-replace finalize for a file→file Overwrite: the temp now holds
        // the complete new bytes; swap it over the original. On finalize error
        // the temp is preserved as committed data (see `finalize_safe_replace`).
        let recorded = match replace_after_write {
            Some(orig) => {
                super::volume_conflict::finalize_safe_replace(dest_volume, &write_dest, &orig).await?;
                orig
            }
            None => write_dest,
        };
        created.record_file(recorded);
        total_bytes += bytes;
        on_file_complete();
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
) -> Result<MergeChildDecision, VolumeError> {
    // Deep children aren't top-level sources, so no preflight hint exists for
    // them; the resolver falls back to trait calls. We DO know the source type
    // and size from the source listing entry we already have in hand, which
    // saves the resolver a redundant `is_directory` probe and seeds the dialog's
    // size annotation.
    let source_is_directory_hint = Some(entry.is_directory);
    let source_size_hint = if entry.is_directory { None } else { entry.size };
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
        None, // dest size hint: unknown here; the resolver stats only on the Stop path
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
#[path = "volume_strategy_tests.rs"]
mod tests;
