//! Carrying an open handle across a reconnect: the patience a held reader
//! (`random_reader.rs`) and a streaming download (`read_stream.rs`) share
//! once the session their handle belonged to is gone.
//!
//! smb2 0.43 can pick a handle up again on a new session and prove the file is
//! still the version it was reading (`Tree::reopen_file_reader`,
//! `resume_file_download`), but it needs a NEW session to do it on, and
//! bringing one up is this crate's job: every client here runs
//! `auto_reconnect: false`. So the handle's owner reports its loss through the
//! funnel like any call, then waits here for a session newer than the one it
//! lost, through the ONE reconnect path (`do_attempt_reconnect`, single-flight,
//! so ten downloads meeting the same outage dial once).
//!
//! Bounded on purpose, and short: a copy that's still down after
//! [`REATTACH_BACKOFF`] reports the loss as before, and the transfer engine's
//! own outage wait (`cmdr-operations`'s `transfer/volume_outage.rs`, which has
//! the budget, the Cancel, the Pause, and the "waiting for the server" line)
//! takes over. Once the share is back, that wait carries the copy on from
//! where it was too (`Volume::resume_read_stream`, `read_stream.rs`), so this
//! only spares it the wait.

use std::time::Duration;

use cmdr_fs::log_error::LogError;
use log::debug;

use super::SmbVolumeInner;
use super::reconnect::reconnect_backoff_should_give_up;
use super::state::ConnectionState;
use std::sync::atomic::Ordering;

/// When a handle that lost its session tries again: at once, then after 1, 3,
/// and 10 s, the transfer session's own reconnect cadence
/// (`transfer_session.rs`). With a reconnect bounded at ~10 s a try, that's
/// well under a minute before the owner gives up.
pub(super) const REATTACH_BACKOFF: [Duration; 4] = [
    Duration::ZERO,
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
];

/// The tries a handle gets at a session newer than the one it lost. Each
/// [`next`](Self::next) waits out a step of [`REATTACH_BACKOFF`] and brings the
/// share back first when it reads `Disconnected`; the caller then leases and
/// reopens, and an error that ends THAT session goes through the funnel and
/// on to the next try.
pub(super) struct Reattach<'a> {
    share: &'a SmbVolumeInner,
    steps: std::slice::Iter<'static, Duration>,
}

/// Why a [`Reattach`] stopped trying.
#[derive(Debug)]
pub(super) enum GaveUp {
    /// Every step was spent and the share isn't usable yet.
    OutOfTime,
    /// The share can't come back on its own: it was unmounted, or the reconnect
    /// needs a person (a changed password, a refused encryption).
    Terminal,
}

impl<'a> Reattach<'a> {
    pub(super) fn new(share: &'a SmbVolumeInner) -> Self {
        Self {
            share,
            steps: REATTACH_BACKOFF.iter(),
        }
    }

    /// Waits for the next try. `Ok` means lease now: the share reads `Direct`
    /// (on the session that was there, or one this call just brought up).
    pub(super) async fn next(&mut self) -> Result<(), GaveUp> {
        loop {
            let Some(delay) = self.steps.next() else {
                debug!(
                    "smb reattach: '{}' still down after {} tries; the loss stands",
                    self.share.volume_id,
                    REATTACH_BACKOFF.len()
                );
                return Err(GaveUp::OutOfTime);
            };
            tokio::time::sleep(*delay).await;
            if self.share.unmounted.load(Ordering::Relaxed) {
                return Err(GaveUp::Terminal);
            }
            if self.share.connection_state() == ConnectionState::Direct {
                return Ok(());
            }
            match self.share.do_attempt_reconnect().await {
                Ok(()) => return Ok(()),
                Err(e) if reconnect_backoff_should_give_up(&e) => {
                    debug!(
                        "smb reattach: '{}' can't come back on its own ({}); the loss stands",
                        self.share.volume_id,
                        LogError(&e)
                    );
                    return Err(GaveUp::Terminal);
                }
                Err(_) => {}
            }
        }
    }
}

#[cfg(test)]
#[path = "resume_test.rs"]
mod resume_test;
