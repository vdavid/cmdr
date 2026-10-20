//! A fresh compress's last step: the validated stage takes the archive's
//! name, in the mode the destination allows, riding out a dropped share.
//! Driven by `fresh_compress.rs`.
//!
//! ❗ A publish step the transport cut off may have been performed, so it's
//! settled once the share is back before anything acts on a guess
//! (`CutOffLanding` for a new archive, `CutOffPublish` for a replace).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::state::WriteOperationState;
use super::super::transfer::StagedWrite;
use super::super::transfer::retry::after_a_cut_off_landing_wait;
use super::super::transfer::staged_write::{
    CutOffLanding, CutOffPublish, LandingError, PublishError, Republished, Resumed,
};
use super::super::transfer::volume_outage::{
    LostConnection, OutageEnd, StartedOn, lost_connection, wait_after_failure,
};
use super::edit_error::EditError;
use super::outage_wait::{landing_failure, publish_failure};
use cmdr_fs::volume::{Volume, VolumeError};

/// Where the archive goes.
pub(super) struct Target {
    pub(super) volume: Arc<dyn Volume>,
    pub(super) archive_path: PathBuf,
    /// The compress replaces an archive already at the name.
    pub(super) replace_existing: bool,
}

/// What publishing the stage came to.
pub(super) enum Published {
    Yes,
    /// Nothing proves it published and nothing of it is left to publish:
    /// make the archive (or upload it) again.
    RunAgain,
    Ended(EditError),
}

/// Publishes the validated stage, in the mode the destination allows: a plain
/// landing for a new archive, one atomic replace, or the original set aside
/// for the rename. A step the transport cut off waits for the share
/// (`outages` is the compress's cap), then settles what the server did.
pub(super) async fn publish_stage(
    stage: StagedWrite,
    target: &Target,
    // The archive's size, as the validation just confirmed it.
    size: u64,
    state: &Arc<WriteOperationState>,
    outages: &mut u32,
) -> Published {
    let dest_volume = &target.volume;
    let archive_path = &target.archive_path;
    let volumes = [dest_volume];
    let mut started_on = StartedOn::now(&volumes);
    let first = if !target.replace_existing {
        Settled::of_landing(stage.commit(dest_volume, size).await, archive_path)
    } else if dest_volume.supports_atomic_replace_rename() {
        Settled::of_publish(stage.commit_atomic_replace(dest_volume, size).await, archive_path)
    } else {
        Settled::of_publish(
            stage.commit_with_displaced_original(dest_volume, size).await,
            archive_path,
        )
    };
    let mut cut_off = match first {
        Settled::Done(published) => return published,
        Settled::CutOff(cut_off) => cut_off,
    };
    let mut attempt = 1;
    loop {
        let ended = wait_after_failure(state, cut_off.error(), &volumes, &started_on, outages).await;
        let ended = after_a_cut_off_landing_wait(ended, cut_off.error(), &mut attempt, state).await;
        if ended != OutageEnd::Back {
            let lost = lost_connection(ended, cut_off.error(), &volumes);
            return cut_off
                .give_up(dest_volume, archive_path, ended == OutageEnd::Cancelled, lost)
                .await;
        }
        started_on = StartedOn::now(&volumes);
        match cut_off.resume(dest_volume, archive_path).await {
            Settled::Done(published) => return published,
            Settled::CutOff(again) => cut_off = again,
        }
    }
}

/// A publish step the transport cut off, whichever mode it published in.
enum CutOff {
    Landing(Box<CutOffLanding>),
    Publish(Box<CutOffPublish>),
}

/// Where a publish step stands.
enum Settled {
    Done(Published),
    CutOff(CutOff),
}

impl Settled {
    fn of_landing(landed: Result<(), LandingError>, archive_path: &Path) -> Self {
        match landed {
            Ok(()) => Self::Done(Published::Yes),
            Err(LandingError::Failed(failure)) => {
                Self::Done(Published::Ended(landing_failure(archive_path, failure, None)))
            }
            Err(LandingError::CutOff(cut_off)) => Self::CutOff(CutOff::Landing(cut_off)),
        }
    }

    fn of_publish(published: Result<(), PublishError>, archive_path: &Path) -> Self {
        match published {
            Ok(()) => Self::Done(Published::Yes),
            Err(PublishError::Failed(failure)) => {
                Self::Done(Published::Ended(publish_failure(archive_path, failure, None)))
            }
            Err(PublishError::CutOff(cut_off)) => Self::CutOff(CutOff::Publish(cut_off)),
        }
    }
}

impl CutOff {
    fn error(&self) -> &VolumeError {
        match self {
            Self::Landing(cut_off) => cut_off.error(),
            Self::Publish(cut_off) => cut_off.error(),
        }
    }

    /// The share is back: settle what the cut-off step did. A new archive's
    /// landing is rename-first, so its proof needs no source to compare
    /// against (`CutOffLanding::resume`).
    async fn resume(self, dest_volume: &Arc<dyn Volume>, archive_path: &Path) -> Settled {
        match self {
            Self::Landing(cut_off) => match cut_off.resume(dest_volume, None).await {
                Resumed::Landed => Settled::Done(Published::Yes),
                Resumed::RunAgain(_) => Settled::Done(Published::RunAgain),
                Resumed::Failed(failure) => {
                    Settled::Done(Published::Ended(landing_failure(archive_path, failure, None)))
                }
                Resumed::CutOff(again) => Settled::CutOff(Self::Landing(again)),
                Resumed::Stopped => Settled::Done(Published::Ended(EditError::Cancelled)),
            },
            Self::Publish(cut_off) => match cut_off.resume(dest_volume).await {
                Republished::Published => Settled::Done(Published::Yes),
                Republished::RunAgain => Settled::Done(Published::RunAgain),
                Republished::Failed(failure) => {
                    Settled::Done(Published::Ended(publish_failure(archive_path, failure, None)))
                }
                Republished::CutOff(again) => Settled::CutOff(Self::Publish(again)),
            },
        }
    }

    /// The wait ended without the share: the step meets the refusal rules. A
    /// cancel reads as the cancel it is, as a copy's does, unless an original
    /// the publish set aside couldn't go back to its name: where the user's
    /// archive is outranks.
    async fn give_up(
        self,
        dest_volume: &Arc<dyn Volume>,
        archive_path: &Path,
        cancelled: bool,
        lost: Option<LostConnection>,
    ) -> Published {
        match self {
            Self::Landing(cut_off) => match cut_off.give_up(dest_volume).await {
                Ok(()) => Published::Yes,
                Err(_) if cancelled => Published::Ended(EditError::Cancelled),
                Err(failure) => Published::Ended(landing_failure(archive_path, failure, lost)),
            },
            Self::Publish(cut_off) => {
                let failure = cut_off.give_up(dest_volume).await;
                if cancelled && failure.recovered.is_none() {
                    Published::Ended(EditError::Cancelled)
                } else {
                    Published::Ended(publish_failure(archive_path, failure, lost))
                }
            }
        }
    }
}
