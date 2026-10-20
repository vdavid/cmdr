//! A volume delete that rides out a server going away (`delete/DETAILS.md` §
//! "Waiting out a dropped share").
//!
//! The walk's reads are safe to run twice, so they go through
//! [`through_outages`] as they are. A delete isn't: a delete whose reply the
//! drop swallowed may have gone through, and during the outage another client
//! may have put a different file at that path. So each entry's delete waits on
//! its own loop here, and every attempt after a wait first re-checks the entry
//! against what the walk listed:
//!
//! - **Gone**: the delete that the drop cut off went through (or someone else
//!   removed it). It counts as deleted. ❗ Only after a wait: a first try's
//!   `NotFound` keeps meaning what it always did.
//! - **Not what the walk listed** (another kind, or a file of another size, or
//!   a size the walk never learned): someone else's file. It stays, and the
//!   delete stops on [`WriteOperationError::ChangedWhileDisconnected`].
//! - **The same**: deleted again with the same non-recursive `delete` (a
//!   folder's `delete_directory`), ❌ never escalated to a recursive one.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::super::transfer::volume::{PathRole, map_failure_after_wait};
use super::super::transfer::volume_outage::{
    LostConnection, OutageEnd, OutageFailure, StartedOn, cancelled, lost_connection, through_outages,
    wait_after_failure,
};
use super::super::types::WriteOperationError;
use cmdr_fs::volume::{EntryKind, Volume, VolumeError};

/// What the walk saw of an entry, which a delete re-run after a wait must
/// still see before it deletes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Listed {
    pub(super) kind: EntryKind,
    /// A file's size as listed. `None` when the walk never learned it, and
    /// then a re-run after a wait can't prove the file is the same one: it's
    /// left and reported, ❌ never deleted on its name alone.
    pub(super) size: Option<u64>,
}

/// Why one entry's delete stopped the operation.
#[derive(Debug)]
pub(super) enum EntryStop {
    /// The person cancelled while the delete waited for the share.
    Cancelled,
    /// After a wait, the entry wasn't the one the walk listed. Left alone.
    Changed,
    /// The delete failed for good: refused, or the outage outlasted the wait
    /// (`lost` says what the wait learned about the connection).
    Failed {
        error: VolumeError,
        lost: Option<LostConnection>,
    },
}

impl EntryStop {
    /// The typed error the delete reports for this stop at `path` on `volume`.
    pub(super) fn into_error(self, path: &Path, volume: &dyn Volume) -> WriteOperationError {
        match self {
            Self::Cancelled => WriteOperationError::Cancelled {
                message: "Operation cancelled by user".into(),
            },
            Self::Changed => WriteOperationError::ChangedWhileDisconnected {
                path: path.display().to_string(),
                volume_name: volume.name().to_owned(),
            },
            Self::Failed { error, lost } => map_failure_after_wait(path, PathRole::Source, error, lost),
        }
    }
}

/// Deletes the entry at `path`, which the walk listed as `listed`, waiting out
/// any outage of `volume` on the way.
///
/// `cut_off` is a failure this entry already met with no attempt of its own (a
/// batch's `delete_files` answer, from a call that started on the given
/// session): the loop starts at the wait instead of deleting blind.
pub(super) async fn delete_entry(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    path: &Path,
    listed: Listed,
    cut_off: Option<(VolumeError, StartedOn)>,
) -> Result<(), EntryStop> {
    let volumes = [volume];
    let mut outages = 0;
    let mut pending = cut_off;
    loop {
        let (error, started_on) = match pending.take() {
            Some(failure) => failure,
            None => {
                let started_on = StartedOn::now(&volumes);
                match attempt(volume, state, path, listed, outages > 0).await {
                    Ok(()) => return Ok(()),
                    Err(AttemptStop::Changed) => return Err(EntryStop::Changed),
                    Err(AttemptStop::Failed(error)) => (error, started_on),
                }
            }
        };
        if matches!(error, VolumeError::Cancelled(_)) {
            return Err(EntryStop::Cancelled);
        }
        match wait_after_failure(state, &error, &volumes, &started_on, &mut outages).await {
            OutageEnd::Back => {}
            OutageEnd::Cancelled => return Err(EntryStop::Cancelled),
            ended @ (OutageEnd::GaveUp { .. } | OutageEnd::NotWaitable) => {
                let lost = lost_connection(ended, &error, &volumes);
                return Err(EntryStop::Failed { error, lost });
            }
        }
    }
}

enum AttemptStop {
    Changed,
    Failed(VolumeError),
}

/// One try at the delete. `after_wait`: an outage came between this try and
/// the walk, so the entry is re-checked first and a `NotFound` means done.
async fn attempt(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    path: &Path,
    listed: Listed,
    after_wait: bool,
) -> Result<(), AttemptStop> {
    if after_wait {
        match still_as_listed(volume.as_ref(), path, listed).await {
            Ok(Recheck::Same) => {}
            Ok(Recheck::Gone) => return Ok(()),
            Ok(Recheck::Changed) => return Err(AttemptStop::Changed),
            Err(error) => return Err(AttemptStop::Failed(error)),
        }
    }
    // ❗ A walked folder goes as a folder: on an object store a markerless one
    // is gone with its last child, and a plain delete of its path would take
    // the file of its name instead (#510).
    let deleted = if listed.kind == EntryKind::Directory {
        volume.delete_directory(path, Some(&state.backend_cancel)).await
    } else {
        volume.delete_with_cancel(path, Some(&state.backend_cancel)).await
    };
    match deleted {
        Ok(()) => Ok(()),
        // The re-check saw it a moment ago, so the delete that took it is ours
        // or someone else's: either way, what the person asked for is done.
        Err(VolumeError::NotFound(_)) if after_wait => Ok(()),
        Err(error) => Err(AttemptStop::Failed(error)),
    }
}

enum Recheck {
    Same,
    Gone,
    Changed,
}

/// Whether the entry at `path` is still the one the walk listed.
async fn still_as_listed(volume: &dyn Volume, path: &Path, listed: Listed) -> Result<Recheck, VolumeError> {
    let kind = match volume.entry_kind(path).await {
        Ok(kind) => kind,
        Err(VolumeError::NotFound(_)) => return Ok(Recheck::Gone),
        Err(error) => return Err(error),
    };
    if kind != listed.kind {
        return Ok(Recheck::Changed);
    }
    if kind != EntryKind::File {
        // A folder's delete refuses while it holds anything, and a link's
        // removes the link alone: neither can take more than the walk saw.
        return Ok(Recheck::Same);
    }
    let Some(listed_size) = listed.size else {
        return Ok(Recheck::Changed);
    };
    match volume.get_metadata(path).await {
        Ok(entry) if entry.size == Some(listed_size) => Ok(Recheck::Same),
        Ok(_) => Ok(Recheck::Changed),
        Err(VolumeError::NotFound(_)) => Ok(Recheck::Gone),
        Err(error) => Err(error),
    }
}

/// A walk read's failure through [`through_outages`]: the error plus what the
/// wait learned about the connection when it ended the read.
#[derive(Debug)]
pub(super) struct ReadStop {
    error: VolumeError,
    lost: Option<LostConnection>,
}

impl ReadStop {
    /// The typed error the delete reports for a read of `path` that stopped it.
    pub(super) fn into_error(self, path: &Path) -> WriteOperationError {
        map_failure_after_wait(path, PathRole::Source, self.error, self.lost)
    }
}

impl From<VolumeError> for ReadStop {
    fn from(error: VolumeError) -> Self {
        Self { error, lost: None }
    }
}

impl OutageFailure for ReadStop {
    fn volume_error(&self) -> &VolumeError {
        &self.error
    }

    fn into_cancelled(self) -> Self {
        cancelled().into()
    }

    fn with_lost_connection(mut self, lost: Option<LostConnection>) -> Self {
        self.lost = lost;
        self
    }
}

/// Runs one of the walk's reads (a listing, a type probe), waiting out an
/// outage of `volume` and running it again: a read is safe to run twice.
pub(super) async fn read_through_outages<T, F, Fut>(
    volume: &Arc<dyn Volume>,
    state: &Arc<WriteOperationState>,
    mut read: F,
) -> Result<T, ReadStop>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, VolumeError>>,
{
    through_outages(state, &[volume], || {
        let running = read();
        async move { running.await.map_err(ReadStop::from) }
    })
    .await
}
