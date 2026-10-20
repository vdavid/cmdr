//! How an archive edit's steps on a remote volume ride out a dropped share:
//! the wait a copy takes (`transfer/volume_outage.rs`), and the error a step
//! ends with when the wait ends without the share. Shared by the remote edit
//! (`remote.rs`) and fresh compression (`fresh_compress.rs`), whose planning
//! walk (`fresh_plan.rs`) reads through [`read_through_outages`].

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::super::transfer::recovered_name::FinalizeFailure;
use super::super::transfer::staged_write::PublishFailure;
use super::super::transfer::volume::{PathRole, map_failure_after_wait, map_finalize_failure};
use super::super::transfer::volume_outage::{
    LostConnection, OutageAnnounce, OutageEnd, StartedOn, lost_connection, wait_after_failure,
    wait_after_failure_announced,
};
use super::super::types::WriteOperationError;
use super::edit_error::EditError;
use cmdr_fs::volume::{Volume, VolumeError};

/// Why one step stopped short.
pub(super) enum StepError {
    /// A volume's own error, which an outage wait judges. Boxed: only a
    /// failure pays for it.
    Fault(Box<Fault>),
    /// A failure on this side (a local file, a cancel, a mismatch), which ends
    /// the edit as it is.
    Ended(EditError),
}

/// A volume's error, and what to report when the wait doesn't ride it out and
/// no lost server connection explains it.
pub(super) struct Fault {
    pub(super) error: VolumeError,
    pub(super) otherwise: EditError,
}

impl StepError {
    /// A fault an outage wait judges, `otherwise` reported when it can't.
    pub(super) fn fault(error: VolumeError, otherwise: EditError) -> Self {
        Self::Fault(Box::new(Fault { error, otherwise }))
    }

    /// `error` from `role`'s volume at `path`, reported as itself when the wait
    /// doesn't ride it out.
    pub(super) fn at(path: &Path, role: PathRole, error: VolumeError) -> Self {
        let otherwise = EditError::Op(map_failure_after_wait(path, role, error.clone(), None));
        Self::fault(error, otherwise)
    }
}

/// What comes after a step's failure.
pub(super) enum Next {
    /// The share is back: run the step again.
    Again,
    Stop(EditError),
}

/// Waits out the outage behind `failed`, when that's what it was
/// (`volume_outage::wait_after_failure`, judged against the generations the
/// step started on, `outages` the item's cap). A wait that ends without the
/// share says "Lost the connection to {server}", `path` as context; any other
/// failure is reported as it always was, and a cancel as the cancel it is.
pub(super) async fn wait_out(
    state: &Arc<WriteOperationState>,
    failed: StepError,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
    path: &Path,
) -> Next {
    let at = FailedAt {
        path,
        role: PathRole::Destination,
    };
    wait_out_announcing(state, failed, volumes, started_on, outages, at, None).await
}

/// Where a step failed, for the error a wait that ends without the share reports.
#[derive(Clone, Copy)]
struct FailedAt<'a> {
    path: &'a Path,
    role: PathRole,
}

/// [`wait_out`], for a step on `at`'s side, announcing the wait on `announce`
/// when nothing else speaks for the operation yet
/// (`volume_outage::wait_after_failure_announced`).
async fn wait_out_announcing(
    state: &Arc<WriteOperationState>,
    failed: StepError,
    volumes: &[&Arc<dyn Volume>],
    started_on: &StartedOn,
    outages: &mut u32,
    at: FailedAt<'_>,
    announce: Option<&OutageAnnounce<'_>>,
) -> Next {
    let (error, otherwise) = match failed {
        StepError::Ended(ended) => return Next::Stop(ended),
        StepError::Fault(fault) => (fault.error, fault.otherwise),
    };
    let ended = match announce {
        Some(announce) => wait_after_failure_announced(state, &error, volumes, started_on, outages, announce).await,
        None => wait_after_failure(state, &error, volumes, started_on, outages).await,
    };
    if ended == OutageEnd::Back {
        return Next::Again;
    }
    if ended == OutageEnd::Cancelled || state.intent().is_cancelled() {
        return Next::Stop(EditError::Cancelled);
    }
    Next::Stop(match lost_connection(ended, &error, volumes) {
        Some(lost) => EditError::Op(map_failure_after_wait(at.path, at.role, error, Some(lost))),
        None => otherwise,
    })
}

/// Runs `read`, one read of `path` on the SOURCE volume of a walk no transfer
/// probe speaks for (a compress's planning walk), and when it fails on the
/// transport while the volume is out, waits for it (announced on `announce`)
/// and runs it again. `otherwise` words a failure the wait doesn't explain.
///
/// ❗ Only a read: it must be safe to send twice, and the walk takes what it
/// returns only once it answers, so a read run again plans each entry once.
pub(super) async fn read_through_outages<T, F, Fut>(
    state: &Arc<WriteOperationState>,
    volume: &Arc<dyn Volume>,
    path: &Path,
    announce: Option<&OutageAnnounce<'_>>,
    otherwise: fn(&Path, VolumeError) -> EditError,
    mut read: F,
) -> Result<T, EditError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, VolumeError>>,
{
    let volumes = [volume];
    let at = FailedAt {
        path,
        role: PathRole::Source,
    };
    let mut outages = 0;
    loop {
        let started_on = StartedOn::now(&volumes);
        let error = match read().await {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        let failed = StepError::fault(error.clone(), otherwise(path, error));
        match wait_out_announcing(state, failed, &volumes, &started_on, &mut outages, at, announce).await {
            Next::Again => {}
            Next::Stop(ended) => return Err(ended),
        }
    }
}

/// Runs `step`, which must be safe to run twice (a read, or a write that
/// starts over from its first byte), and when it fails on the transport while
/// one of `volumes` is out, waits for it and runs it again ([`wait_out`]).
pub(super) async fn rerun_through_outages<T, F, Fut>(
    state: &Arc<WriteOperationState>,
    volumes: &[&Arc<dyn Volume>],
    path: &Path,
    mut step: F,
) -> Result<T, EditError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, StepError>>,
{
    let mut outages = 0;
    loop {
        let started_on = StartedOn::now(volumes);
        let failed = match step().await {
            Ok(value) => return Ok(value),
            Err(failed) => failed,
        };
        match wait_out(state, failed, volumes, &started_on, &mut outages, path).await {
            Next::Again => {}
            Next::Stop(ended) => return Err(ended),
        }
    }
}

/// A landing that ended without the bytes at `path`: where the new bytes went
/// when the way was cleared (that outranks everything), else the lost
/// connection, else the destination's own refusal.
pub(super) fn landing_failure(path: &Path, failure: FinalizeFailure, lost: Option<LostConnection>) -> EditError {
    EditError::Op(if failure.aftermath.is_some() {
        map_finalize_failure(path, failure)
    } else {
        map_failure_after_wait(path, PathRole::Destination, failure.error, lost)
    })
}

/// A whole-publish that ended without the bytes at `path`: the original kept
/// beside its name when it couldn't go back (that outranks everything), with
/// the lost connection or the refusal as its cause.
pub(super) fn publish_failure(path: &Path, failure: PublishFailure, lost: Option<LostConnection>) -> EditError {
    let cause = map_failure_after_wait(path, PathRole::Destination, failure.error, lost);
    EditError::Op(match failure.recovered {
        Some(recovered) => WriteOperationError::OriginalsKeptAside {
            cause: Box::new(cause),
            recovered: vec![recovered],
        },
        None => cause,
    })
}
