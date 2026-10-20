//! The steps of [`stream_pipe_file`](super::stream_pipe_file)'s runner, in
//! pass order: settle a create the transport may have completed, finish a
//! landing the transport cut off, open the source, write, land. Each hands its
//! outcome back to the runner; a step that fails returns a [`Fault`] for
//! [`FileRun::on_fault`], ❌ never a wait of its own.

use std::sync::Arc;

use super::{Fault, FileRun, LandingToResume, Stop, auto_yield_tuning};
use crate::metadata_losses;
use crate::transfer::checkpoint_stream::CheckpointStream;
use crate::transfer::interrupted_landing::{create_went_unanswered, settle_unanswered_create};
use crate::transfer::recovered_name::FinalizeFailure;
use crate::transfer::retry;
use crate::transfer::staged_write::{LandingError, LandingSource, Resumed, StagedWrite, WriteStaging, resolve_staging};
use crate::transfer::transfer_probe::{TaskPhase, arm_current_task_stall_abort, set_task_bytes, set_task_phase};
use crate::transfer::volume::source_reads::{self, SourceReads};
use crate::transfer::volume::transfer_error::{AtPath, PathedVolumeError, hard_abort_error};
use crate::transfer::volume::{extract_quarantine, landed_flags};
use crate::transfer::volume_outage::{self, StartedOn};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, MetadataClasses};
use cmdr_fs::volume::{StreamWritten, VolumeError, VolumeReadStream};

/// The source, open and wrapped, with what was asked of it before the write
/// consumes it.
pub(super) struct Opened {
    stream: Box<dyn VolumeReadStream>,
    source_reads: SourceReads,
    size: u64,
    /// What the destination can't keep of what this file had worth keeping.
    lost: MetadataClasses,
    /// What can't go on until the file has its final name (Locked).
    after_landing: CarriedMetadata,
}

/// The write's last byte is down: what the landing needs.
pub(super) struct Written {
    staged: StagedWrite,
    written: StreamWritten,
    lost: MetadataClasses,
    after_landing: CarriedMetadata,
}

impl FileRun<'_> {
    /// Checkpoint zero: a single-shot create whose reply the transport lost
    /// settles BEFORE the next open. `Some` when the server had completed it;
    /// a stop during its byte compare ends the file as the stop it is.
    pub(super) async fn settle_unanswered_create(&mut self) -> Result<Option<u64>, Stop> {
        let Some((size, lost)) = self.unanswered_create.take() else {
            return Ok(None);
        };
        let settled = settle_unanswered_create(
            self.source_volume,
            self.source_path,
            size,
            self.dest_volume,
            self.dest_path,
            &self.state.stop_work,
        )
        .await;
        let Ok(settled) = settled else {
            return Err(volume_outage::cancelled())
                .at_destination(self.dest_path)
                .map_err(Stop::Over);
        };
        let Some(stamp) = settled else {
            return Ok(None);
        };
        log::info!(target: "copy", "stream_pipe_file: {} was written whole before the reply was lost", LogPrivate::file_path(self.dest_path));
        self.state.claimed_names.release_placeholder(self.dest_path);
        // The write's own stamp may be what the drop cut off (SMB stamps a
        // compound write in a later request), so it goes on again by path,
        // idempotent. What still won't go on is counted, ❌ never dropped
        // silently.
        let report = self
            .dest_volume
            .apply_metadata(self.dest_path, &stamp.carried, stamp.modified)
            .await;
        self.state
            .metadata_losses
            .note_file(lost.union(report.refused_classes()));
        Ok(Some(size))
    }

    /// Opens the source. A device round-trip on MTP / SMB that can hang on its
    /// own, so it has a phase of its own in a dump. It happens BEFORE the
    /// staging decision because that decision needs the stream's
    /// `total_size()`: the same number the destination gets, so the exemption
    /// is asked about exactly the write that will be performed. Nothing is
    /// staged yet, so a failure here has nothing to clean up.
    ///
    /// Raced against TIER 2 for the same reason the write is: on the SERIAL
    /// path nothing above this await can end it, and a device round trip that
    /// hangs before the first byte is half of the wedge shape. With nothing
    /// staged, the abort has nothing to do but return.
    pub(super) async fn open_source(&self) -> Result<Opened, Stop> {
        let state = self.state;
        let (source_volume, dest_volume) = (self.source_volume, self.dest_volume);
        set_task_phase(TaskPhase::OpeningSource);
        // Each step snapshots which session its volumes are on before it runs,
        // so a failure that turns out to be a replaced session's is run again
        // rather than reported (`volume_outage::StartedOn`).
        let started_on = StartedOn::now(&[source_volume, dest_volume]);
        let opened = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => {
                return Err(hard_abort_error(self.source_path)).at_source(self.source_path).map_err(Stop::Over);
            }
            opened = source_volume.open_read_stream_with_hint(self.source_path, self.source_facts.size) => opened,
        };
        let stream = opened.map_err(|error| Stop::Faulted(Box::new(Fault::OpenSource { error, started_on })))?;
        let Some(size) = stream.total_size().known() else {
            return Err(VolumeError::NotSupported)
                .at_source(self.source_path)
                .map_err(Stop::Over);
        };
        let length = cmdr_fs::volume::StreamLength::Known(size);
        // Holds the source to `size` too: one that ends short or runs long fails
        // the write as `SourceChanged` before anything lands (`source_reads.rs`).
        let (stream, source_reads) = source_reads::watch(stream, self.source_path, length);
        // An extract carries the archive's quarantine with the rest of what
        // the source reports, so the destination puts it on before the landing
        // like any other xattr (`extract_quarantine.rs`).
        let stream = extract_quarantine::quarantine_stream(state, source_volume, stream);
        // What the destination can't keep of what this file had worth keeping,
        // asked before the write consumes the stream and counted once it
        // lands: nothing at all for a pair that keeps everything.
        let lost = metadata_losses::file_losses(&**source_volume, &**dest_volume, &*stream);
        // What can't go on until the file has its final name (Locked), asked
        // before the write consumes the stream; empty for a plain file.
        let after_landing = landed_flags::after_landing(&**source_volume, &**dest_volume, &*stream);
        Ok(Opened {
            stream,
            source_reads,
            size,
            lost,
            after_landing,
        })
    }

    /// Stages this attempt's target and streams the source into it, raced
    /// against tier 2 and (for a staged write) the stall watchdog.
    pub(super) async fn write(&mut self, opened: Opened) -> Result<Written, Stop> {
        let state = self.state;
        let (source_volume, dest_volume, dest_path) = (self.source_volume, self.dest_volume, self.dest_path);
        let Opened {
            stream,
            source_reads,
            size,
            lost,
            after_landing,
        } = opened;
        let length = cmdr_fs::volume::StreamLength::Known(size);
        // ONE probe, two consumers: the staging decision below and the
        // destination-side foreground yield's floor exemption (handed to the
        // `CheckpointStream`). See `resolve_staging`.
        let write_is_single_shot = dest_volume.write_is_single_shot(length).await;
        // A whole-publishing destination (an object store) needs no staging
        // either, though it makes no single-shot promise: the yield floor and
        // the stall watchdog below keep reading the single-shot answer alone.
        let resolved_staging = resolve_staging(
            self.staging,
            write_is_single_shot || dest_volume.publishes_writes_whole(),
        );
        let staged = StagedWrite::begin(state, &**dest_volume, self.announced_dest, resolved_staging);
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
        // the transport error that produces feeds straight back into the retry.
        // It is the layer of last resort — every backend that can bound its
        // own waits already does, sooner — for the wedge shape that has no
        // deadline anywhere and left a user force-quitting the app. Re-armed
        // per attempt.
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
        let on_file_progress = self.on_file_progress;
        let write_progress =
            |progress: cmdr_fs::volume::StreamWriteProgress| on_file_progress(progress.bytes_written, size);
        let started_on = StartedOn::now(&[source_volume, dest_volume]);
        let write_fut =
            dest_volume.write_from_stream(staged.target(), staged.write_mode(), length, stream, &write_progress);
        let outcome = tokio::select! {
            biased;
            () = state.backend_abort.cancelled() => WriteAttemptOutcome::HardAborted,
            () = cancelled_or_never(stall_abort.as_ref()) => WriteAttemptOutcome::Finished(Err(
                VolumeError::ConnectionTimeout(cmdr_fs::error_detail::ErrorDetail::around(
                    "the write of ",
                    dest_path.to_string_lossy(),
                    " stopped moving and the transfer stopped waiting for it",
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
                    LogPrivate::file_path(dest_path),
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
                // dying produces. That holds because the frame never turns into a
                // stream at the real name: a source that broke its length fails
                // before anything is created (`source_reads.rs`, and SMB's own
                // drain check, #520).
                return Err(hard_abort_error(dest_path))
                    .at_destination(dest_path)
                    .map_err(Stop::Over);
            }
        };
        if create_went_unanswered(staged.write_mode(), &write_result) {
            self.unanswered_create = Some((size, lost));
        }
        match write_result {
            Ok(written) => {
                if self.attempt > 1 {
                    log::info!(
                        target: "copy",
                        "stream_pipe_file: {} landed on attempt {} of {}",
                        LogPrivate::file_path(dest_path),
                        self.attempt,
                        retry::MAX_ATTEMPTS,
                    );
                }
                Ok(Written {
                    staged,
                    written,
                    lost,
                    after_landing,
                })
            }
            Err(error) => Err(Stop::Faulted(Box::new(Fault::Write {
                error,
                started_on,
                staged,
                source_reads,
            }))),
        }
    }

    /// Past the last byte: give the file its final name. What the source
    /// carries beyond its bytes is already on it: the destination asked the
    /// stream and put it on the staged write (`Volume::write_from_stream`), all
    /// but the flags that wait for the final name. What it refused on the way
    /// joins what its caps couldn't hold, for the loss note.
    ///
    /// ❌ Not raced against tier 2: a task wedged here is ended by the driver's
    /// drain deadline rather than by an abort mid-rename.
    pub(super) async fn land(&mut self, written: Written) -> Result<u64, Stop> {
        let (dest_volume, dest_path) = (self.dest_volume, self.dest_path);
        let Written {
            staged,
            written,
            lost,
            after_landing,
        } = written;
        let bytes = written.bytes;
        let lost = lost.union(written.refused());
        let started_on = StartedOn::now(&[self.source_volume, dest_volume]);
        match staged.commit(dest_volume, bytes).await {
            Ok(()) => Ok(self.landed(bytes, lost, &after_landing).await),
            Err(LandingError::Failed(FinalizeFailure {
                error: VolumeError::NotSupported,
                ..
            })) if matches!(
                self.staging,
                WriteStaging::Stage | WriteStaging::StageInFreshFolder | WriteStaging::StageOntoClaimedName
            ) =>
            {
                log::warn!(
                    target: "copy",
                    "stream_pipe_file: destination can't land a staged write for {}; falling back to writing at the final name",
                    LogPrivate::file_path(dest_path)
                );
                self.staging = WriteStaging::Unstaged;
                self.writing_at_the_final_name = true;
                Err(Stop::RunAgain)
            }
            // A step the transport cut off, a swap's clearing delete included:
            // the outage wait and the settle decide (`on_fault`), ❌ never a
            // guess made on whatever session answers next.
            Err(LandingError::CutOff(cut_off)) => Err(Stop::Faulted(Box::new(Fault::Land {
                cut_off,
                started_on,
                bytes,
                lost,
                after_landing,
            }))),
            // The write SUCCEEDED and the landing didn't. When the landing had
            // cleared the way, the temp holds the only complete copy of the new
            // bytes: `land` got them out of temp space, and `new_data_at` says
            // where they are. A swap that couldn't clear the way (or can't tell)
            // reaches here too. Either way `land` has settled what the temp is
            // in the ledgers. Surface the failure.
            Err(LandingError::Failed(failure)) => Err(Stop::Over(PathedVolumeError::from_landing(failure, dest_path))),
        }
    }

    /// The checkpoint after an outage that cut a landing off: settles what the
    /// cut-off step did and lands what's left, ❌ without streaming the file
    /// again while its temp is whole (`staged_write::CutOffLanding::resume`).
    /// `Some` once the file is at its name; `None` runs the file again.
    pub(super) async fn resume_landing(&mut self) -> Result<Option<u64>, Stop> {
        let Some(LandingToResume {
            cut_off,
            bytes,
            lost,
            after_landing,
        }) = self.landing_to_resume.take()
        else {
            return Ok(None);
        };
        set_task_phase(TaskPhase::Finalizing);
        let started_on = StartedOn::now(&[self.source_volume, self.dest_volume]);
        let source = LandingSource {
            volume: self.source_volume,
            path: self.source_path,
        };
        match cut_off.resume(self.dest_volume, Some(source)).await {
            Resumed::Landed => Ok(Some(self.landed(bytes, lost, &after_landing).await)),
            Resumed::RunAgain(error) => {
                log::info!(
                    target: "copy",
                    "stream_pipe_file: nothing proves {} landed before the drop ({error}); running it again",
                    LogPrivate::file_path(self.dest_path), error = LogError(&error),
                );
                Ok(None)
            }
            Resumed::Failed(failure) => Err(Stop::Over(PathedVolumeError::from_landing(failure, self.dest_path))),
            Resumed::Stopped => Err(volume_outage::cancelled())
                .at_destination(self.dest_path)
                .map_err(Stop::Over),
            Resumed::CutOff(cut_off) => Err(Stop::Faulted(Box::new(Fault::Land {
                cut_off,
                started_on,
                bytes,
                lost,
                after_landing,
            }))),
        }
    }

    /// The file has its final name: what every landing that got there owes it.
    pub(super) async fn landed(&self, bytes: u64, lost: MetadataClasses, after_landing: &CarriedMetadata) -> u64 {
        // Landed, so a ` (N)` placeholder at the name is filled, not ours to
        // take back (`naming.rs::take_back_unfilled_reservations`).
        self.state.claimed_names.release_placeholder(self.dest_path);
        self.state.metadata_losses.note_file(lost);
        landed_flags::put_on(self.dest_volume, self.dest_path, after_landing, self.state).await;
        bytes
    }
}

/// The stall watchdog's abort for this attempt, or a future that never resolves
/// when it isn't armed (a single-shot write).
///
/// One `select!` arm covers both the armed and the unarmed case without a token
/// allocation, so an inert watchdog costs a poll of an already-live atomic.
async fn cancelled_or_never(token: Option<&tokio_util::sync::CancellationToken>) {
    match token {
        Some(token) => token.cancelled().await,
        None => std::future::pending().await,
    }
}

/// How one attempt at a file's write ended.
enum WriteAttemptOutcome {
    /// The destination's `write_from_stream` returned, one way or the other.
    Finished(Result<StreamWritten, VolumeError>),
    /// TIER 2 ended the wait: the write future was dropped mid-flight and the
    /// backend ran none of its own cleanup. ❌ Nothing may go back through that
    /// connection now; the staged partial is left to the sweep.
    HardAborted,
}
