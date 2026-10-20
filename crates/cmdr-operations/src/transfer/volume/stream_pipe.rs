//! The per-file streaming write: one file's bytes from `open_read_stream` into
//! the destination's `write_from_stream`, staged on a `.cmdr-tmp-*` sibling,
//! retried per file, parked and yielded between chunks, and raced against both
//! cancel tiers. `strategy.rs` dispatches here for every FILE source; `merge.rs`
//! for every file child of a walked tree.
//!
//! [`stream_pipe_file`] is a step runner: this file holds the runner and its one
//! fault decision ([`FileRun::on_fault`]), `stream_pipe_steps.rs` the steps.

use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use super::super::super::host::Announced;
use super::super::super::state::WriteOperationState;
use super::super::retry;
use super::super::staged_write::{CutOffLanding, StagedWrite, WriteStaging};
use super::super::transfer_probe::{TaskPhase, note_task_retry, set_task_phase};
use super::super::volume_outage::{
    self, LostConnection, OutageEnd, OutageFailure, StartedOn, lost_connection, wait_after_failure,
};
use super::preflight::SourceFileFacts;
use super::server_side_copy::{ServerSideCopy, try_server_side_copy};
use super::source_reads::SourceReads;
use super::transfer_error::{AtPath, PathRole, PathedVolumeError};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::volume::{Volume, VolumeError};

#[path = "stream_pipe_steps.rs"]
mod steps;

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
pub(super) const MIN_PROGRESS_FLOOR_BYTES: u64 = 4 * 1024 * 1024;

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
        if let Some(t) = super::strategy::test_support::auto_yield_tuning_override() {
            return t;
        }
    }
    (
        FOREGROUND_YIELD_DEBOUNCE,
        MIN_PROGRESS_FLOOR_BYTES,
        DEST_FOREGROUND_YIELD_HARD_CAP,
    )
}

/// Streams one file from source to destination via `open_read_stream` /
/// `write_from_stream`. Per-chunk progress and cancellation are enforced by
/// the destination's `write_from_stream` implementation, which calls
/// `on_progress` between chunks and returns `VolumeError::Cancelled` on
/// `ControlFlow::Break(())`.
///
/// The source stream is wrapped in a `CheckpointStream` so a between-chunk
/// cooperative checkpoint (park-while-paused, then `yield_now`) runs once per
/// chunk: that's what makes a paused op stop advancing MID-FILE (the sync
/// `on_progress` callback can't `.await` to park), and what keeps a long
/// single-file transfer from starving foreground tasks.
///
/// The bytes never touch `dest_path` until the last one has landed: they go to
/// a `.cmdr-tmp-*` sibling that is renamed into place at the end
/// (`staged_write.rs`), delete-first when `dest_path` is the user's original a
/// file→file Overwrite replaces (`WriteStaging::StageOverOriginal`).
///
/// A step runner: each pass walks the file's checkpoints in order (settle a
/// create the transport may have completed, finish a landing the transport cut
/// off, open the source, write, land), and a step that fails hands its
/// [`Fault`] to ONE decision, [`FileRun::on_fault`], which classifies it,
/// waits out an outage with the step's own generation
/// snapshot, and says whether the file runs again or is over. The landed flags
/// (`landed_flags.rs`) are a best-effort tail of the landing, outside the
/// runner: a flag a drop cut off is a counted, reported metadata loss.
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
) -> Result<u64, PathedVolumeError> {
    log::debug!(
        "stream_pipe_file: {} -> {}",
        LogPrivate::file_path(source_path),
        LogPrivate::file_path(dest_path)
    );

    // Register BOTH halves of the eventual rename with the downloads watcher's
    // ignore set when the destination is local-FS-backed (the only case where
    // the watcher could otherwise fire). Covers MTP→Local and SMB→Local imports
    // that land in ~/Downloads.
    let announced_dest = state.scope().host().announce_on(&**dest_volume, dest_path);

    // The server can copy for itself: the bytes never leave it. A pure
    // optimization, so anything short of a clean success falls through to the
    // streaming runner below rather than failing the file, and a landing the
    // transport cut off takes the runner's landing wait like a streamed one.
    let mut fault = match try_server_side_copy(
        source_volume,
        source_path,
        dest_volume,
        announced_dest,
        state,
        on_file_progress,
        staging,
    )
    .await
    .map_err(|f| PathedVolumeError::from_landing(f, dest_path))?
    {
        ServerSideCopy::Landed(bytes) => {
            // Landed, so a ` (N)` placeholder at the name is filled, not ours to take back.
            state.claimed_names.release_placeholder(dest_path);
            return Ok(bytes);
        }
        ServerSideCopy::Streams => None,
        ServerSideCopy::LandingCutOff {
            cut_off,
            started_on,
            bytes,
        } => Some(Fault::Land {
            cut_off,
            started_on,
            bytes,
            lost: MetadataClasses::NONE,
            after_landing: CarriedMetadata::default(),
        }),
    };

    let mut run = FileRun {
        source_volume,
        source_path,
        source_facts,
        dest_volume,
        dest_path,
        announced_dest,
        state,
        on_file_progress,
        staging,
        writing_at_the_final_name: false,
        attempt: 1,
        outages: 0,
        unanswered_create: None,
        landing_to_resume: None,
    };
    loop {
        let failed = match fault.take() {
            Some(fault) => fault,
            None => match run.checkpoints().await {
                Ok(bytes) => return Ok(bytes),
                Err(Stop::RunAgain) => continue,
                Err(Stop::Over(error)) => return Err(error),
                Err(Stop::Faulted(fault)) => *fault,
            },
        };
        if let ControlFlow::Break(result) = run.on_fault(failed).await {
            return result;
        }
    }
}

/// One file's run through [`stream_pipe_file`]'s checkpoints: what every step
/// reads, and what a file carries from one pass to the next.
struct FileRun<'a> {
    source_volume: &'a Arc<dyn Volume>,
    source_path: &'a Path,
    source_facts: SourceFileFacts,
    dest_volume: &'a Arc<dyn Volume>,
    dest_path: &'a Path,
    announced_dest: Announced<'a>,
    state: &'a Arc<WriteOperationState>,
    on_file_progress: &'a (dyn Fn(u64, u64) -> ControlFlow<()> + Sync),
    /// The staging the caller asked for, until a destination that can't land a
    /// staged write sends the file back to be written at its final name.
    ///
    /// A destination that can't rename can't stage. No production backend is
    /// in that position (Local, SMB, and MTP all rename), but a minimal `Volume`
    /// impl must stay usable as a copy destination, so a `NotSupported` landing
    /// re-runs the file unstaged (`WriteStaging::Unstaged`) instead of failing.
    /// ❌ Never a swap: an unstaged write over the original would replace it
    /// before the new bytes are whole, so a swap's `NotSupported` fails the file.
    staging: WriteStaging,
    /// Set once that fallback happened. From then on a failed attempt's partial
    /// sits AT the final name and is ours, so the terminal failure removes it
    /// here: the driver only knows the staging it asked for, and nothing else
    /// ever cleans a final name.
    writing_at_the_final_name: bool,
    /// Which attempt at THIS file we're on, 1-based. A transport blip
    /// (`VolumeError::transport_fault`) runs the file again from its first byte
    /// on a fresh source stream and a fresh staging temp, up to
    /// `retry::MAX_ATTEMPTS`; see `retry.rs` for the policy and why it lives
    /// here rather than any layer above. Restarting the whole file is what keeps
    /// the retry honest: nothing partial survives an attempt, so no byte is
    /// written twice and no ledger records a file twice.
    attempt: u32,
    /// How many outages THIS file has waited out, across all of its steps: a
    /// volume that went away entirely (`volume_outage.rs`), as opposed to a blip
    /// the attempts above ride. Each one that ends with the volume back gives
    /// the file a fresh set of attempts; the cap is what keeps a flapping link
    /// from holding it forever.
    outages: u32,
    /// Set when an attempt's create at the FINAL name (a single-shot write)
    /// failed on the transport: the server may have completed it, so the next
    /// pass settles that before opening the source again
    /// (`interrupted_landing.rs`).
    unanswered_create: Option<(u64, MetadataClasses)>,
    /// Set when a landing the transport cut off waited out its outage: the next
    /// pass settles and finishes it before anything streams again
    /// (`staged_write::CutOffLanding::resume`).
    landing_to_resume: Option<LandingToResume>,
}

/// A cut-off landing, with what its file's landed tail still needs.
struct LandingToResume {
    cut_off: Box<CutOffLanding>,
    bytes: u64,
    lost: MetadataClasses,
    after_landing: CarriedMetadata,
}

/// Why a pass over the checkpoints stopped short of a landed file.
enum Stop {
    /// A step failed: [`FileRun::on_fault`] decides what comes next. Boxed: a
    /// fault carries its step's whole state, and only a failure pays for that.
    Faulted(Box<Fault>),
    /// Run the file again from its first checkpoint, with no wait: a retry
    /// after its backoff, or the unstaged fallback.
    RunAgain,
    /// The file failed for good.
    Over(PathedVolumeError),
}

/// A step that failed, named by its checkpoint and carrying what resuming
/// there needs, plus the generations its call started on (`StartedOn`).
enum Fault {
    /// Opening the source. Nothing is staged yet, so a resume opens it again.
    OpenSource { error: VolumeError, started_on: StartedOn },
    /// The write into the staged target. A resume clears the attempt's partial
    /// first, then runs the file again.
    Write {
        error: VolumeError,
        started_on: StartedOn,
        staged: StagedWrite,
        source_reads: SourceReads,
    },
    /// A landing step the transport cut off (a rename, or a swap's clearing
    /// delete), which the server may or may not have performed. A resume
    /// settles what it did and lands what's left, streaming the file again only
    /// when nothing proves it landed and no whole temp is left to land.
    Land {
        cut_off: Box<CutOffLanding>,
        started_on: StartedOn,
        bytes: u64,
        lost: MetadataClasses,
        after_landing: CarriedMetadata,
    },
}

impl Fault {
    fn error(&self) -> &VolumeError {
        match self {
            Self::OpenSource { error, .. } | Self::Write { error, .. } => error,
            Self::Land { cut_off, .. } => cut_off.error(),
        }
    }

    fn started_on(&self) -> &StartedOn {
        match self {
            Self::OpenSource { started_on, .. } | Self::Write { started_on, .. } | Self::Land { started_on, .. } => {
                started_on
            }
        }
    }
}

impl FileRun<'_> {
    /// One pass over the checkpoints, in order. `Ok` is the landed file's byte
    /// count.
    async fn checkpoints(&mut self) -> Result<u64, Stop> {
        if let Some(bytes) = self.settle_unanswered_create().await? {
            return Ok(bytes);
        }
        if let Some(bytes) = self.resume_landing().await? {
            return Ok(bytes);
        }
        let opened = self.open_source().await?;
        let written = self.write(opened).await?;
        self.land(written).await
    }

    /// The ONE decision for a step that failed: wait out the outage when that's
    /// what the failure was (`volume_outage::wait_after_failure`, judged
    /// against the generations the step's call started on), then resume at the
    /// step's checkpoint or end the file. `Continue` runs the file again. A
    /// file that ends on a server whose connection is gone carries that
    /// (`volume_outage::lost_connection`), so the person hears the cause.
    async fn on_fault(&mut self, fault: Fault) -> ControlFlow<Result<u64, PathedVolumeError>> {
        let volumes = [self.source_volume, self.dest_volume];
        let ended = wait_after_failure(
            self.state,
            fault.error(),
            &volumes,
            fault.started_on(),
            &mut self.outages,
        )
        .await;
        let connection_lost = lost_connection(ended, fault.error(), &volumes);
        match fault {
            // A source share that's gone (a download from a NAS that dropped):
            // once it's back, open the file again. Nothing is staged yet, so
            // there's nothing to clear first.
            Fault::OpenSource { error, .. } => match ended {
                OutageEnd::Back => {
                    self.attempt = 1;
                    ControlFlow::Continue(())
                }
                OutageEnd::Cancelled => ControlFlow::Break(Err(volume_outage::cancelled()).at_source(self.source_path)),
                OutageEnd::GaveUp { .. } | OutageEnd::NotWaitable => {
                    ControlFlow::Break(Err(PathedVolumeError::new(PathRole::Source, self.source_path, error)
                        .with_lost_connection(connection_lost)))
                }
            },
            // A volume that went away entirely (the share dropped off the
            // network and its session reads `Disconnected`): once it's back, run
            // the file again from its first byte with a fresh set of attempts.
            // The failed attempt's partial is cleared AFTER the wait, when the
            // delete can reach the share. A wait that gives up, or a failure
            // that isn't an outage, takes the write's own failure handling; a
            // cancel during the wait reaches its cancel arm (`is_cancelled` is
            // set by then).
            Fault::Write {
                error,
                staged,
                source_reads,
                ..
            } => match ended {
                OutageEnd::Back => {
                    staged.abandon_attempt(self.dest_volume).await;
                    log::info!(
                        target: "copy",
                        "stream_pipe_file: running {} again now that its volume is back",
                        LogPrivate::file_path(self.dest_path),
                    );
                    self.attempt = 1;
                    ControlFlow::Continue(())
                }
                OutageEnd::Cancelled | OutageEnd::GaveUp { .. } | OutageEnd::NotWaitable => {
                    self.write_failed(error, staged, source_reads, connection_lost).await
                }
            },
            // A landing step the transport cut off: the destination went away
            // under it (or answered with a session someone else had already
            // replaced, or blipped with nothing to wait for), and the server
            // may or may not have performed it. Once the share is back, or
            // after the per-file retry's backoff for a blip
            // (`retry::after_a_landing_blip`), the next pass settles what it
            // did and lands what's left (`resume_landing`). A wait that ends
            // without the share meets the refusal rules
            // (`CutOffLanding::give_up`): nothing cleared leaves the temp to the
            // abandoned-write cleanup, and a cleared way rescues the new bytes
            // or leaves them recorded as a landing for the next sweep that
            // reaches the share, ❌ never cleaned. A cancel takes the same
            // give-up, and is reported as the cancel it is.
            Fault::Land {
                cut_off,
                bytes,
                lost,
                after_landing,
                ..
            } => {
                let to_resume = LandingToResume {
                    cut_off,
                    bytes,
                    lost,
                    after_landing,
                };
                let ended = retry::after_a_cut_off_landing_wait(
                    ended,
                    to_resume.cut_off.error(),
                    &mut self.attempt,
                    self.state,
                )
                .await;
                if ended == OutageEnd::Back {
                    self.landing_to_resume = Some(to_resume);
                    return ControlFlow::Continue(());
                }
                self.give_up_landing(to_resume, ended == OutageEnd::Cancelled, connection_lost)
                    .await
            }
        }
    }

    /// A cut-off landing whose wait ended without the destination: settled by
    /// the refusal rules (`CutOffLanding::give_up`), and reported as the stop
    /// when the work `stopped`, else as the failure, carrying the server
    /// connection that ended it (`lost`) when that's what did.
    async fn give_up_landing(
        &self,
        landing: LandingToResume,
        stopped: bool,
        lost: Option<LostConnection>,
    ) -> ControlFlow<Result<u64, PathedVolumeError>> {
        let LandingToResume {
            cut_off,
            bytes,
            lost: lost_metadata,
            after_landing,
        } = landing;
        set_task_phase(TaskPhase::Finalizing);
        match cut_off.give_up(self.dest_volume).await {
            Ok(()) => ControlFlow::Break(Ok(self.landed(bytes, lost_metadata, &after_landing).await)),
            Err(_) if stopped => ControlFlow::Break(Err(volume_outage::cancelled()).at_destination(self.dest_path)),
            Err(failure) => ControlFlow::Break(Err(
                PathedVolumeError::from_landing(failure, self.dest_path).with_lost_connection(lost)
            )),
        }
    }

    /// A write that failed with no outage to wait out (or one the wait gave up
    /// on): a blip runs the file again after its backoff, a cancel is reported
    /// as one, anything else ends the file, carrying the server connection
    /// that ended it (`lost`) when that's what did.
    async fn write_failed(
        &mut self,
        error: VolumeError,
        staged: StagedWrite,
        source_reads: SourceReads,
        lost: Option<LostConnection>,
    ) -> ControlFlow<Result<u64, PathedVolumeError>> {
        let (dest_volume, dest_path, attempt) = (self.dest_volume, self.dest_path, self.attempt);
        if retry::should_retry(&error, attempt, self.state) {
            // The staged bytes are a partial of an attempt we're abandoning;
            // clear them BEFORE the next attempt starts, so a retried file
            // never leaves a trail of `.cmdr-tmp-*` siblings and the next
            // write lands on a clean path (see `abandon_attempt` for why that
            // reaches one case further than the terminal `abandon`).
            staged.abandon_attempt(dest_volume).await;
            log::warn!(
                target: "copy",
                "stream_pipe_file: attempt {attempt} of {} for {} failed ({error}); running the file again in {:?}",
                retry::MAX_ATTEMPTS,
                LogPrivate::file_path(dest_path),
                retry::backoff_after(attempt), error = LogError(&error),
            );
            note_task_retry();
            set_task_phase(TaskPhase::WaitingToRetry);
            if !retry::wait_before_retry(self.state, attempt).await {
                // Cancelled during the backoff. Report it as the cancel it is
                // rather than the transport error that triggered the retry, so
                // the post-loop reclassifies it and emits `write-cancelled`.
                return ControlFlow::Break(
                    Err(VolumeError::Cancelled("Operation cancelled by user".into())).at_destination(dest_path),
                );
            }
            self.attempt += 1;
            return ControlFlow::Continue(());
        }
        // The staged bytes are a partial (a mid-stream failure, or the cancel
        // the backend turned into `Cancelled`); drop them.
        staged.abandon(dest_volume).await;
        remove_unstaged_partial(self.writing_at_the_final_name, dest_volume, dest_path).await;
        // A cancel landed while an attempt was failing on something we WOULD
        // have run again. The cancel is the reason this file stops here, so
        // report it as one: the post-loop keys `write-cancelled` off a
        // `Cancelled`-shaped error, and a transport error in its place would
        // log the user's own click as a failed transfer.
        if error.transport_fault().is_some() && self.state.work_stopped() {
            return ControlFlow::Break(
                Err(VolumeError::Cancelled("Operation cancelled by user".into())).at_destination(dest_path),
            );
        }
        if attempt > 1 {
            log::warn!(
                target: "copy",
                "stream_pipe_file: giving up on {} after {attempt} attempt(s): {error}",
                LogPrivate::file_path(dest_path), error = LogError(&error),
            );
        }
        ControlFlow::Break(Err(source_reads
            .label(error, self.source_path, dest_path)
            .with_lost_connection(lost)))
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
            LogPrivate::file_path(dest_path), e = LogError(&e)
        );
    }
}
