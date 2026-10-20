//! Staging for one cross-volume file write: bytes land on a `.cmdr-tmp-*`
//! sibling and take the file's final name only after the last one arrives.
//!
//! **The invariant.** A destination file must never carry its final name while
//! it is still being written. The 2026-07-31 wedge was force-quit mid-transfer
//! and left two phone backups at their final names — one at zero bytes, one
//! truncated at 4 MiB — indistinguishable from complete files
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Neither had a
//! conflict, so neither was staged; a fresh copy streamed straight to the
//! destination path. Staging every write closes that: whatever a crash leaves
//! behind wears a `.cmdr-tmp-*` name nobody mistakes for their data.
//!
//! **Who stages.** Every staged write stages HERE, a file→file Overwrite
//! included: the conflict layer only decides that the write replaces the user's
//! original ([`Replaces::Swap`]), and the write stages as
//! [`WriteStaging::StageOverOriginal`] and lands through the same landing as
//! any other: one replacing rename where the destination has one, else
//! through an aside (the original renamed aside, the temp onto its name, the
//! aside deleted last). So the temp has one owner, and `write_path` names the
//! file the user gets on every path. A write the DESTINATION lands in one
//! indivisible shot is
//! [`WriteStaging::SingleShot`] and needs no temp: there is no moment at which
//! the final name holds a partial. It has no landing either, so the no-replace
//! rule travels with it instead: a name the caller expected free is written
//! with [`WriteMode::CreateNew`], which the destination refuses atomically if
//! someone took it mid-upload ([`StagedWrite::write_mode`]). Every other write stages here
//! ([`WriteStaging::Stage`], or [`WriteStaging::StageOntoClaimedName`] when the
//! caller picked the final name itself).
//!
//! **Whose name it is.** The landing rename can answer `AlreadyExists`, and
//! what that means is the CALLER's to say ([`LandingName`]): a name it claimed
//! (a `Rename` pick's `O_EXCL` placeholder, a cross-type Overwrite's cleared
//! destination) may be cleared, a name it believed FREE may not. Without the
//! distinction a clash nobody resolved — a case-insensitive destination
//! answering for `Report.docx` with the user's `report.docx` — read as "clear
//! the way" and replaced a file under a Skip.
//!
//! **Cost.** One extra rename per staged file. On SMB that is one round trip,
//! which roughly doubles the wire cost of a file the compound
//! CREATE+WRITE+FLUSH+CLOSE fast path would otherwise finish in one — which is
//! why single-shot writes are exempt. That exemption is bought by
//! single-shot-ness, ❌ NEVER by smallness: the destination is asked
//! (`Volume::write_is_single_shot`), the caller never guesses from a size.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::host::Announced;
use super::super::in_flight_temps::{self, RecordHome};
use super::super::state::WriteOperationState;
#[cfg(test)]
use super::recovered_name::FinalizeFailure;
use super::transfer_probe::{TaskPhase, set_task_phase};
use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::staging::StagingTemp;
use cmdr_fs::volume::{Volume, VolumeError, WriteMode};

/// Who owns the `.cmdr-tmp-*` staging for one file write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WriteStaging {
    /// The path handed to the writer is the file's FINAL name: stage it here.
    /// The caller believes that name is FREE, so an `AlreadyExists` when the
    /// bytes come to take it is a clash nobody answered — see [`LandingName`].
    Stage,
    /// [`WriteStaging::Stage`] for a name inside a folder THIS operation created
    /// (the merge walker's `create_directory` succeeded), which the creation
    /// proved empty. Staged and landed exactly like `Stage`; a whole-publishing
    /// destination writes it with [`WriteMode::CreateNewInFreshFolder`], which
    /// lets an object store skip its per-object no-overwrite HEAD.
    StageInFreshFolder,
    /// Stage as [`WriteStaging::Stage`], but the final name is one the CALLER
    /// claimed or cleared: a `Rename` resolution's `O_EXCL` placeholder, or a
    /// cross-type Overwrite that removed what was there. Whatever the landing
    /// finds in the way is the caller's own doing, so it may clear it.
    StageOntoClaimedName,
    /// A file→file Overwrite ([`Replaces::Swap`]): the final name holds the
    /// user's ORIGINAL, which survives the whole write. Staged exactly like
    /// [`WriteStaging::Stage`], and landed over the original
    /// ([`LandingName::ReplacesOriginal`]).
    ///
    /// ❗ Never single-shot ([`resolve_staging`] leaves it alone): the
    /// original's survival is the stronger guarantee, and a single-shot write
    /// replaces it before the bytes are known to be whole.
    StageOverOriginal,
    /// No staging at all, because this destination can't land a staged write
    /// (it can't rename): the bytes go straight to the final name, with
    /// [`WriteMode::CreateOrReplace`], and a failed attempt's partial there is
    /// ours. Only `stream_pipe_file` picks it, as its fallback for a
    /// `NotSupported` landing of a write that replaces nothing. ❌ Never for a
    /// swap: it would write over the original in place.
    Unstaged,
    /// The DESTINATION lands this write in one indivisible shot (`Volume::
    /// write_is_single_shot`), or publishes every write whole by protocol
    /// (`Volume::publishes_writes_whole`, an object store), so the final name
    /// can never hold a partial and staging would buy nothing but a rename.
    /// Write straight to the final name.
    ///
    /// Carries the [`LandingName`] of the staged write it replaced, because
    /// with no landing rename to refuse a taken name, the write itself has to:
    /// an `ExpectedFree` name goes out as [`WriteMode::CreateNew`].
    SingleShot(LandingName),
}

/// What the caller believes sits at the final name when the staged bytes come
/// to take it, and therefore what an `AlreadyExists` from the landing rename
/// means.
///
/// The distinction is the difference between a late-detected conflict and a
/// deleted file. Only the caller knows which it is: the landing sees the same
/// `AlreadyExists` either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LandingName {
    /// Nobody resolved a conflict for this write: the name was free when the
    /// caller last looked. Something in the way now is a clash no policy and no
    /// person answered, so the landing REFUSES rather than clearing it.
    ExpectedFree,
    /// [`ExpectedFree`](Self::ExpectedFree), inside a folder this operation
    /// created and found empty. Means exactly the same to the landing (a clash
    /// nobody answered); only the write mode differs
    /// ([`WriteMode::CreateNewInFreshFolder`]).
    FreeInFreshFolder,
    /// The caller claimed or cleared this name and is entitled to what's in the
    /// way: a `Rename` pick reserved with an `O_EXCL` placeholder, a cross-type
    /// Overwrite whose destination delete has already run.
    ClaimedByTheCaller,
    /// The name holds the user's original, which this write replaces (a
    /// file→file Overwrite). The original is known to be in the way, so the
    /// landing replaces it in one forced rename where the destination's is one
    /// step (`Volume::supports_atomic_replace_rename`), and otherwise deletes it
    /// FIRST, then renames: never `rename(force)` there, which on MTP can make a
    /// duplicate. Only [`WriteStaging::StageOverOriginal`] lands with it.
    ReplacesOriginal,
}

impl LandingName {
    /// A name nothing resolved: [`FreeInFreshFolder`](Self::FreeInFreshFolder)
    /// in a folder this operation's own `create_directory` made (which proved
    /// it empty), else [`ExpectedFree`](Self::ExpectedFree). A merge into a
    /// folder that already existed keeps every no-overwrite check: that's
    /// where another writer's files plausibly land.
    pub(super) fn free(in_a_folder_this_operation_made: bool) -> Self {
        if in_a_folder_this_operation_made {
            Self::FreeInFreshFolder
        } else {
            Self::ExpectedFree
        }
    }
}

/// One file write's staging: where the bytes go, and how they get their final
/// name.
pub(crate) struct StagedWrite {
    /// `Some(temp)` for every staged write, `None` under
    /// [`WriteStaging::Unstaged`] and [`WriteStaging::SingleShot`].
    ///
    /// The guard also keeps the temp out of the pane while it's being written
    /// (`cmdr_fs::staging`). Dropping it un-hides the file, which is why
    /// `commit` and `abandon` hold it until the rename or delete is done — and
    /// why a landing that FAILS is right to let it go: a temp that survives one
    /// (a rescue or a discard that couldn't happen) is something the user may
    /// need to see.
    temp: Option<StagingTemp>,
    /// Where the file must end up: the user's original itself under
    /// [`WriteStaging::StageOverOriginal`].
    final_path: PathBuf,
    /// What the caller believes about that name, which is what [`land`] needs to
    /// tell a clash nobody answered from a name the caller itself claimed, and
    /// an original to replace from both.
    landing: LandingName,
    /// See [`write_mode`](Self::write_mode).
    write_mode: WriteMode,
    /// [`WriteStaging::Unstaged`]: the bytes go to the final name, so what a
    /// failed attempt leaves there is ours to clear before the next one
    /// ([`abandon_attempt`](Self::abandon_attempt)).
    unstaged: bool,
    /// The volume whose path space the temp is recorded in: the operation's
    /// destination (`in_flight_temps::dest_home`), or the one a caller
    /// writing to more than one volume names
    /// ([`begin_over_original`](Self::begin_over_original)).
    home_volume: Option<String>,
    state: Arc<WriteOperationState>,
}

impl StagedWrite {
    /// Begins a tracked sibling stage for validated generated output. A target
    /// observed absent stays `ExpectedFree`, so a racing file is never cleared.
    /// Announces like [`Self::begin`].
    pub(crate) fn begin_generated(
        state: &Arc<WriteOperationState>,
        dest_volume: &dyn Volume,
        final_path: Announced<'_>,
        replace_existing: bool,
    ) -> Self {
        let staging = if replace_existing {
            WriteStaging::StageOntoClaimedName
        } else {
            WriteStaging::Stage
        };
        Self::begin(state, dest_volume, final_path, staging)
    }

    /// Picks the staging path and, when we own it, records it as an in-flight
    /// partial on the operation so an abandoned task's litter can be found.
    ///
    /// The final name comes in announced ([`Announced`]); the write's own
    /// target (the temp, or the final name itself when single-shot) is announced
    /// here on `dest_volume`, before the write that creates it.
    pub(super) fn begin(
        state: &Arc<WriteOperationState>,
        dest_volume: &dyn Volume,
        final_path: Announced<'_>,
        staging: WriteStaging,
    ) -> Self {
        let staged = Self::begin_unannounced(state, final_path.path(), staging);
        state.scope().host().announce_on(dest_volume, staged.target());
        staged
    }

    /// Begins a file→file Overwrite of `final_path`, the user's file on
    /// `dest_volume`, staged and landed like any other
    /// ([`WriteStaging::StageOverOriginal`]), for a caller outside the transfer
    /// engine whose operation writes to more than one volume: a remote zip edit
    /// pulls the archive into a local scratch and pushes the edited one back, so
    /// the operation names no single destination. The temp is recorded in
    /// `dest_volume_id`'s path space, and so is the original's aside, which is
    /// what lets a swap it cuts off be kept for the volume's next arrival and
    /// settled by its sweep.
    pub(crate) fn begin_over_original(
        state: &Arc<WriteOperationState>,
        dest_volume: &dyn Volume,
        final_path: Announced<'_>,
        dest_volume_id: &str,
    ) -> Self {
        let staged = Self::begin_recorded_in(
            state,
            final_path.path(),
            WriteStaging::StageOverOriginal,
            Some(dest_volume_id.to_owned()),
        );
        state.scope().host().announce_on(dest_volume, staged.target());
        staged
    }

    /// [`Self::begin`] with nothing announced.
    fn begin_unannounced(state: &Arc<WriteOperationState>, final_path: &Path, staging: WriteStaging) -> Self {
        Self::begin_recorded_in(state, final_path, staging, state.dest_volume_id().map(str::to_owned))
    }

    /// [`Self::begin_unannounced`], recording the temp in `home_volume`'s path
    /// space.
    fn begin_recorded_in(
        state: &Arc<WriteOperationState>,
        final_path: &Path,
        staging: WriteStaging,
        home_volume: Option<String>,
    ) -> Self {
        let temp = match staging {
            WriteStaging::Stage
            | WriteStaging::StageInFreshFolder
            | WriteStaging::StageOntoClaimedName
            | WriteStaging::StageOverOriginal => {
                let staged = StagingTemp::mint(final_path, state.liveness_token());
                in_flight_temps::track_temp(state, home_of(home_volume.as_deref()), staged.path());
                Some(staged)
            }
            // Straight to the final name: no intermediate state to track, and
            // nothing to hide.
            WriteStaging::Unstaged | WriteStaging::SingleShot(_) => None,
        };
        Self {
            temp,
            final_path: final_path.to_path_buf(),
            // An unstaged write never lands and answers `ClaimedByTheCaller`
            // for the same reason it may write over the name: what's there is
            // its own attempt's. A single-shot write keeps the answer of the
            // staged write it replaced, which is what `write_mode` reads.
            landing: match staging {
                WriteStaging::Stage => LandingName::ExpectedFree,
                WriteStaging::StageInFreshFolder => LandingName::FreeInFreshFolder,
                WriteStaging::StageOntoClaimedName | WriteStaging::Unstaged => LandingName::ClaimedByTheCaller,
                WriteStaging::StageOverOriginal => LandingName::ReplacesOriginal,
                WriteStaging::SingleShot(landing) => landing,
            },
            write_mode: match staging {
                WriteStaging::SingleShot(LandingName::ExpectedFree) => WriteMode::CreateNew,
                WriteStaging::SingleShot(LandingName::FreeInFreshFolder) => WriteMode::CreateNewInFreshFolder,
                WriteStaging::SingleShot(LandingName::ClaimedByTheCaller | LandingName::ReplacesOriginal)
                | WriteStaging::Stage
                | WriteStaging::StageInFreshFolder
                | WriteStaging::StageOntoClaimedName
                | WriteStaging::StageOverOriginal
                | WriteStaging::Unstaged => WriteMode::CreateOrReplace,
            },
            unstaged: matches!(staging, WriteStaging::Unstaged),
            home_volume,
            state: Arc::clone(state),
        }
    }

    /// Where the streaming writer must put the bytes.
    pub(crate) fn target(&self) -> &Path {
        self.temp.as_ref().map_or(&self.final_path, StagingTemp::path)
    }

    /// What the write to [`target`](Self::target) may do to a file already
    /// there, for `Volume::write_from_stream`.
    ///
    /// ❗ `CreateNew` exactly when the bytes go straight to a FINAL name the
    /// caller expected free: a single-shot write with nothing resolved for it.
    /// With no staged landing to refuse a name someone took mid-upload, the
    /// destination's own create has to, atomically (SMB's `FileCreate`). A
    /// replacing write there reported success with the other writer's file
    /// gone. Pinned on a live share by
    /// `operations_host/backend_suites/smb_transfer_safety_test.rs::smb_integration_a_single_shot_upload_never_replaces_a_name_taken_mid_upload`.
    ///
    /// Everything else may replace: our own freshly minted `.cmdr-tmp-*` (a
    /// retried attempt writes over its predecessor), a final name the caller
    /// claimed, whose `O_EXCL` placeholder sits at the name and must be written
    /// over, or an unstaged write's own earlier attempt.
    pub(crate) fn write_mode(&self) -> WriteMode {
        self.write_mode
    }

    /// The write SUCCEEDED: give the bytes their final name.
    ///
    /// The temp stays in the in-flight set until [`land`] says it stopped being
    /// a removable copy of ours: it landed, it was taken away, or the landing is
    /// about to clear the name, from which point it may be the only complete
    /// copy at the destination, so ❗ nothing may sweep it and it's recorded as
    /// a landing instead (`in_flight_temps::hold_for_landing`). Until then a landing
    /// that never finishes (a cancel, the concurrent driver dropping its window,
    /// a crash) leaves it findable by the abandoned-write sweep and the startup
    /// sweep, which is safe: the name was never cleared, and the source still
    /// holds the bytes.
    ///
    /// A [`WriteStaging::StageOverOriginal`] write lands here too
    /// ([`LandingName::ReplacesOriginal`]): in one replacing rename where the
    /// destination has one, else through an aside: the original renamed aside
    /// under a record on disk (`in_flight_temps::hold_aside`), the temp renamed
    /// onto its name, the aside deleted last. A refused move aside is checked
    /// with one stat, and a refused landing puts the original back.
    ///
    /// `size` is the byte count the write confirmed. A landing that clears the
    /// way records the temp at that size (`ItemKind::Landing::expected_size`),
    /// which is how a later sweep tells a whole temp from a short one, and how
    /// a cut-off landing's settle tells a temp it can land without streaming
    /// the file again.
    ///
    /// ❗ A step that fails on the transport cuts the landing off
    /// ([`LandingError::CutOff`]) rather than acting on a guess: the caller
    /// waits for the destination and then resumes or gives up
    /// ([`CutOffLanding`]).
    ///
    /// `Failed` with `VolumeError::NotSupported` means this destination can't
    /// rename (or delete), so it can't stage at all; the caller may fall back to
    /// writing at the final name, and `land` has already taken the temp away. No
    /// production backend takes that branch.
    pub(crate) async fn commit(mut self, dest_volume: &Arc<dyn Volume>, size: u64) -> Result<(), LandingError> {
        let Some(temp) = self.temp.take() else {
            // Nothing to land: an unstaged or single-shot write already sits at
            // its final name.
            return Ok(());
        };
        // Landing is a device round trip of its own; a dump has to be able to
        // name it rather than showing a task still "streaming" at EOF.
        set_task_phase(TaskPhase::Finalizing);
        land(
            dest_volume,
            Landing::new(
                &self.state,
                temp,
                &self.final_path,
                self.landing,
                size,
                self.home_volume.clone(),
            ),
        )
        .await
    }

    /// [`commit`](Self::commit) with no outage wait, for a cell about the
    /// landing alone: a landing the transport cut off gives up at once, under
    /// the refusal rules (`CutOffLanding::give_up`).
    #[cfg(test)]
    pub(crate) async fn commit_without_a_wait(
        self,
        dest_volume: &Arc<dyn Volume>,
        size: u64,
    ) -> Result<(), FinalizeFailure> {
        match self.commit(dest_volume, size).await {
            Ok(()) => Ok(()),
            Err(LandingError::Failed(failure)) => Err(failure),
            Err(LandingError::CutOff(cut_off)) => cut_off.give_up(dest_volume).await,
        }
    }

    /// The write FAILED: the staged bytes are a partial, so remove them.
    ///
    /// Best-effort — the backend usually deleted its own partial already, and a
    /// leftover `.cmdr-tmp-*` is untidy rather than dangerous.
    ///
    /// A `SingleShot` write has nothing to remove: the destination promised the
    /// bytes either all landed or none did, and cleaning up after its own failed
    /// attempt is the backend's job (it is the only layer that can tell "the
    /// server created the file and then refused the bytes" from "the file was
    /// already there and we never touched it").
    pub(crate) async fn abandon(mut self, dest_volume: &Arc<dyn Volume>) {
        let Some(temp) = self.temp.take() else {
            return;
        };
        // Retired only once it's gone: a destination that turned the delete
        // down (a share that's out, a handle still open) leaves it listed, so
        // the stop's cleanup takes it later, or the ledger does on the share's
        // next reconnect. Retired first, it would be forgotten on the share.
        match dest_volume.delete(temp.path()).await {
            Ok(()) | Err(VolumeError::NotFound(_)) => self.retire_temp(temp.path()),
            Err(e) => log::debug!(
                target: "copy",
                "staged write: couldn't remove the partial {} after a failed write, leaving it listed: {e}",
                LogPrivate::file_path(temp.path()), e = LogError(&e)
            ),
        }
    }

    /// This ATTEMPT failed and another one is about to run the same file
    /// (`retry.rs`): clear whatever the failed attempt left at the write target,
    /// so the next attempt writes onto a clean path.
    ///
    /// Wider than [`abandon`](Self::abandon) by exactly one case, and the reason
    /// is that the next writer is US. Under [`WriteStaging::Unstaged`] the
    /// target is the final name, which `abandon` leaves alone (the terminal
    /// failure's cleanup is `stream_pipe_file`'s, which knows it wrote there),
    /// but between two attempts it holds nothing but the partial we just gave
    /// up on. Leaving it would make the next attempt's behavior depend on how
    /// each backend treats a write onto an existing path: `LocalPosixVolume`
    /// truncates, `InMemoryVolume` refuses with `AlreadyExists`, and MTP can
    /// happily make a second object with the same name.
    ///
    /// A `SingleShot` write is still left entirely to its backend: only the
    /// backend can tell "the server created the file and then refused the bytes"
    /// from "the file was already there and we never touched it", and that target
    /// is the user's real filename.
    pub(super) async fn abandon_attempt(self, dest_volume: &Arc<dyn Volume>) {
        if self.temp.is_some() {
            self.abandon(dest_volume).await;
            return;
        }
        if self.unstaged {
            let target = self.final_path.clone();
            if let Err(e) = dest_volume.delete(&target).await {
                log::debug!(
                    target: "copy",
                    "staged write: couldn't clear {} before the next attempt: {e}",
                    LogPrivate::file_path(&target), e = LogError(&e)
                );
            }
        }
    }

    fn retire_temp(&self, temp: &Path) {
        in_flight_temps::retire_temp(&self.state, home_of(self.home_volume.as_deref()), temp);
    }
}

/// The path space a staged write that names `home_volume` records its temp in.
fn home_of(home_volume: Option<&str>) -> Option<RecordHome> {
    home_volume.map(RecordHome::volume_space)
}

/// What one resolved write does to a FILE already holding the destination name,
/// as conflict resolution decided it.
///
/// A required, typed answer rather than a `bool` because a third shape exists
/// and each one drives a different landing, cleanup, and journal: confusing
/// them either deletes the user's original on a failed write or records an
/// overwrite as a fresh file, which an undo would then delete.
///
/// Under every shape the write's `write_path` is the name the user ends up
/// with, the original's own for a replace: ❌ no layer above [`StagedWrite`]
/// sees a temp-shaped destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Replaces {
    /// Nothing is replaced: a free name, a `Rename` pick, a merge into a
    /// folder, or a cross-type destination already set aside.
    Nothing,
    /// A file→file Overwrite by SAFE-REPLACE: the write stages on a temp
    /// sibling of the original ([`WriteStaging::StageOverOriginal`]), and its
    /// landing replaces the original in one step, or sets it aside, renames the
    /// temp onto its name, and deletes the aside last. The original survives
    /// the whole write, and the landing until the new bytes are at its name.
    Swap,
    /// A file→file Overwrite on a destination that publishes every write whole
    /// (`Volume::publishes_writes_whole`, an object store): the write goes to
    /// the original's own name with `CreateOrReplace`, and the destination
    /// swaps the content only when the write completes. Nothing to finalize,
    /// and ❗ a failed write leaves the ORIGINAL there, so nothing at the name
    /// is ours to clean.
    InPlace,
}

impl Replaces {
    /// Whether this write replaces a file the user had, which makes the
    /// operation not rollbackable (the original is gone).
    pub(crate) fn overwrites(&self) -> bool {
        !matches!(self, Self::Nothing)
    }

    /// The file this write replaces, whatever the mechanism: `write_path`
    /// itself, which names the original under both replacing shapes. A
    /// same-volume move asks this, because it renames the SOURCE onto that name
    /// either way.
    pub(crate) fn replaced_file(self, write_path: &Path) -> Option<PathBuf> {
        self.overwrites().then(|| write_path.to_path_buf())
    }
}

/// The staging every call site derives the same way: a swap over an original
/// ([`Replaces::Swap`]) stages over the original, an in-place replace on a
/// whole-publishing destination ([`Replaces::InPlace`]) writes the final name
/// over, and anything else stages by whose name it is.
///
/// `landing` is the OTHER thing only the caller knows: whether a conflict
/// resolution put this write at this name. It decides what the landing rename's
/// `AlreadyExists` means (see `staged_write.rs::land`), so ❌ never pass
/// `ClaimedByTheCaller` for a write nothing resolved — that is the reading that
/// clears the user's file.
///
/// `InPlace` answers `SingleShot(ClaimedByTheCaller)` here rather than waiting
/// for [`resolve_staging`]: the resolver chose it on the same whole-publish
/// answer, and the write must never fall back to a staged landing, whose
/// aside and renames are the round trips the in-place write exists to avoid.
pub(super) fn staging_for(replaces: &Replaces, landing: LandingName) -> WriteStaging {
    match (replaces, landing) {
        (Replaces::Swap, _) | (Replaces::Nothing, LandingName::ReplacesOriginal) => WriteStaging::StageOverOriginal,
        (Replaces::InPlace, _) => WriteStaging::SingleShot(LandingName::ClaimedByTheCaller),
        (Replaces::Nothing, LandingName::ExpectedFree) => WriteStaging::Stage,
        (Replaces::Nothing, LandingName::FreeInFreshFolder) => WriteStaging::StageInFreshFolder,
        (Replaces::Nothing, LandingName::ClaimedByTheCaller) => WriteStaging::StageOntoClaimedName,
    }
}

/// Whether a FILE write handed `dest_path` with `staging` can leave anything of
/// THIS operation at `dest_path` when it fails, which is what makes that path a
/// partial for the post-loop cleanup to remove.
///
/// - `Unstaged`: the bytes went to the name itself. Ours, though no driver
///   meets it (`stream_pipe_file` falls back to it and cleans up after it).
/// - `StageOntoClaimedName`: ❗ **never ours either**, though the caller claimed
///   the name. The bytes went to a temp sibling, so all that's ours at the name
///   is a `Rename` pick's `O_EXCL` placeholder, which `ClaimedNames` takes back
///   while it's still empty (`volume/naming.rs::take_back_unfilled_reservations`).
///   Once the landing cleared the name, what stands there is someone's save
///   after our delete, or our own landed file (a kept landing the reconnect
///   sweep finished before the post-loop ran, `#575`), and cleaning it deleted
///   that. Pinned by `copy_landing_race_tests.rs`.
/// - `StageOverOriginal`: ❗ **never ours.** `dest_path` is the user's
///   ORIGINAL, which survives a failed write; the temp is the write's own,
///   abandoned by the write, and once the landing has cleared the way it's a
///   landing no cleanup may touch.
/// - `Stage`: ❗ **never ours.** The bytes went to a `.cmdr-tmp-*` sibling that
///   `stream_pipe_file` abandons itself, and the name was FREE when the driver
///   looked. Anything there now is someone else's, and the likeliest reason the
///   write failed at all is that someone else took the name mid-upload (the
///   landing refuses, `staged_write.rs::land`). Cleaning `dest_path` then
///   deleted their file. Pinned by `copy_landing_race_tests.rs` and, on live
///   servers, `a_name_taken_mid_upload_is_never_replaced`.
/// - `SingleShot`: ❗ never ours either. A driver meets it for an in-place
///   replace on a whole-publishing destination ([`Replaces::InPlace`]), where
///   a failed write leaves the user's ORIGINAL at the name; an upgraded staged
///   write is resolved inside `stream_pipe_file`. Either way its failed attempt
///   is the backend's to clean.
pub(super) fn failed_write_leaves_ours_at(staging: WriteStaging) -> bool {
    match staging {
        WriteStaging::Unstaged => true,
        WriteStaging::Stage
        | WriteStaging::StageInFreshFolder
        | WriteStaging::StageOntoClaimedName
        | WriteStaging::StageOverOriginal
        | WriteStaging::SingleShot(_) => false,
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
/// destination can answer it: the caller asks `Volume::write_is_single_shot`
/// about the same `size` `write_from_stream` gets, off the same stream, and the
/// backend answers with the very condition its one-shot path branches on. A
/// caller-side size threshold would drift from that condition the day a backend
/// retunes it, and drifting apart means truncated files at real names again.
///
/// The answer arrives as a plain `bool` rather than being probed here, because
/// `stream_pipe_file`'s OTHER consumer needs the raw fact: the destination-side
/// foreground yield exempts a single-shot write from the min-progress floor, and
/// the returned enum can't tell it apart from a staged one. ❗ Ask ONCE per
/// write and share the answer; two probes could straddle a reconnect and
/// disagree.
///
/// `StageOverOriginal` is never touched: its staging keeps the ORIGINAL file in
/// place until the new bytes are complete, which is a stronger guarantee than
/// single-shot-ness. Nor is `Unstaged`, which has no stage to drop.
///
/// The single-shot answer keeps whose name it is: a `Stage` (name expected
/// free) becomes `SingleShot(ExpectedFree)`, which writes with
/// [`WriteMode::CreateNew`]; a `StageOntoClaimedName` becomes
/// `SingleShot(ClaimedByTheCaller)`, which may replace its own placeholder.
///
/// `lands_whole` is `write_is_single_shot` OR `Volume::publishes_writes_whole`:
/// an object store publishes every write whole by protocol, so it gets the same
/// unstaged write at the final name (`volume/DETAILS.md` § "Whole-publish
/// destinations"). Only the staging decision takes the OR; the yield floor and
/// the stall watchdog read the single-shot answer alone.
pub(super) fn resolve_staging(requested: WriteStaging, lands_whole: bool) -> WriteStaging {
    match requested {
        WriteStaging::Stage if lands_whole => WriteStaging::SingleShot(LandingName::ExpectedFree),
        WriteStaging::StageInFreshFolder if lands_whole => WriteStaging::SingleShot(LandingName::FreeInFreshFolder),
        WriteStaging::StageOntoClaimedName if lands_whole => WriteStaging::SingleShot(LandingName::ClaimedByTheCaller),
        other => other,
    }
}

#[path = "staged_write_landing.rs"]
mod landing;
pub(crate) use landing::{CutOffLanding, LandingError, LandingSource, Resumed};
use landing::{Landing, land};

#[path = "staged_write_publish.rs"]
mod publish;
pub(crate) use publish::{CutOffPublish, PublishError, PublishFailure, Republished};

#[cfg(test)]
impl LandingError {
    /// The failure of a landing a cell expects to END, never to be cut off.
    pub(crate) fn ended(self) -> FinalizeFailure {
        match self {
            Self::Failed(failure) => failure,
            Self::CutOff(cut_off) => panic!("the landing was cut off on the transport: {}", cut_off.error()),
        }
    }
}

#[cfg(test)]
#[path = "staged_write_tests.rs"]
mod tests;
