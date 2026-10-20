//! The pre-flight scan's answer to a read the dropped share cut off: the wait
//! the rest of a transfer takes (`../volume_outage.rs`), asked per read from
//! inside the backend's own walk (`cmdr_fs::volume::ScanOutages`).
//!
//! The scan is one backend call over every source, minutes of listings on a
//! big tree, so a drop in the middle of it used to fail the transfer before a
//! byte moved. Each read the walk sends now asks this owner, which waits with
//! the operation's budget, Cancel, and Pause, and has the read run again; the
//! walk carries on at the directory it was listing. A listing counts nothing
//! until it answers, so the read run again counts each entry once
//! (`cmdr_fs::volume::scan_outages`).
//!
//! No transfer probe is registered yet while the scan runs, so the wait is
//! announced on the operation's own channel (`wait_after_failure_announced`):
//! the scan's progress reads `Reconnecting` while it waits.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use super::super::super::state::WriteOperationState;
use super::super::volume_outage::{
    LostConnection, OutageAnnounce, OutageEnd, StartedOn, lost_connection, wait_after_failure_announced,
};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ScanOutages, ScanReadJudge, ScanReadNext, Volume, VolumeError};

/// One scan's owner, asked about every read its walk sends to `volume`.
pub(super) struct ScanWait<'a> {
    state: &'a Arc<WriteOperationState>,
    volume: &'a Arc<dyn Volume>,
    announce: OutageAnnounce<'a>,
    /// The read whose wait ended the scan without the share, and what it
    /// learned about the connection, for the error the person reads.
    lost: Mutex<Option<(PathBuf, LostConnection)>>,
}

impl<'a> ScanWait<'a> {
    pub(super) fn new(
        state: &'a Arc<WriteOperationState>,
        volume: &'a Arc<dyn Volume>,
        announce: OutageAnnounce<'a>,
    ) -> Self {
        Self {
            state,
            volume,
            announce,
            lost: Mutex::new(None),
        }
    }

    /// The lost server connection that ended the scan, with the path whose
    /// read it was; `None` when the scan ended any other way.
    pub(super) fn lost_connection(&self) -> Option<(PathBuf, LostConnection)> {
        self.lost.lock_ignore_poison().take()
    }
}

impl ScanOutages for ScanWait<'_> {
    fn read<'b>(&'b self, path: &'b Path) -> Box<dyn ScanReadJudge + Send + 'b> {
        Box::new(ScanRead {
            wait: self,
            path,
            // ❗ Now, before the read goes out (`StartedOn`).
            started_on: StartedOn::now(&[self.volume]),
            outages: 0,
        })
    }
}

/// One read of the walk, and the outages it has ridden out so far
/// (`MAX_OUTAGES_PER_ITEM`, per read, the way each merge level counts its own).
struct ScanRead<'b> {
    wait: &'b ScanWait<'b>,
    path: &'b Path,
    started_on: StartedOn,
    outages: u32,
}

impl ScanReadJudge for ScanRead<'_> {
    fn after_failure<'c>(
        &'c mut self,
        error: &'c VolumeError,
    ) -> Pin<Box<dyn Future<Output = ScanReadNext> + Send + 'c>> {
        Box::pin(async move {
            let wait = self.wait;
            let volumes = [wait.volume];
            let ended = wait_after_failure_announced(
                wait.state,
                error,
                &volumes,
                &self.started_on,
                &mut self.outages,
                &wait.announce,
            )
            .await;
            match ended {
                OutageEnd::Back => {
                    self.started_on = StartedOn::now(&volumes);
                    ScanReadNext::Again
                }
                OutageEnd::Cancelled => ScanReadNext::Stopped,
                OutageEnd::GaveUp { .. } | OutageEnd::NotWaitable => {
                    if let Some(lost) = lost_connection(ended, error, &volumes) {
                        *wait.lost.lock_ignore_poison() = Some((self.path.to_path_buf(), lost));
                    }
                    ScanReadNext::Fail
                }
            }
        })
    }
}
