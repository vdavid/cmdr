//! The `warn` while a share mount is stuck in the system's mount service.
//!
//! macOS mounts through NetFS, which waits on `NetAuthSysAgent`. After a network change
//! that agent can wedge until someone runs `killall NetAuthSysAgent`, and every mount
//! queues behind it with nothing in the log saying so. `mount_share`'s timeout gives up
//! on the CALLER after `DEFAULT_MOUNT_TIMEOUT_MS`, but the blocking call keeps waiting on
//! its thread, so the watch rides the blocking work itself: it speaks every
//! [`SLOW_MOUNT`] for as long as the call is out, and once more when it comes back.
//! Diagnostic only: ❌ never `error`, which files an error report.

use std::time::Duration;

use cmdr_fs::log_private::LogPrivate;
use tokio::sync::oneshot;
use tokio::time::Instant;

/// A mount still out after this is waiting on something (a healthy one takes a second or
/// two), and the watch repeats its line at this cadence while it stays out.
pub(super) const SLOW_MOUNT: Duration = Duration::from_secs(15);

/// Watches one mount call. Move it into the blocking work and [`finish`](Self::finish) it
/// there; dropping it (a panic) stops the watch quietly.
pub(super) struct MountWatch {
    server: String,
    share: String,
    started: Instant,
    threshold: Duration,
    /// Dropping it ends the watch's task.
    _stop: oneshot::Sender<()>,
}

impl MountWatch {
    /// Starts watching a mount of `share` on `server`. Must run inside a tokio runtime.
    pub(super) fn start(server: &str, share: &str, threshold: Duration) -> Self {
        let (stop, mut stopped) = oneshot::channel::<()>();
        let started = Instant::now();
        let (watched_server, watched_share) = (server.to_string(), share.to_string());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Resolves when the watch is finished or dropped: the sender never sends.
                    _ = &mut stopped => return,
                    () = tokio::time::sleep(threshold) => log::warn!(
                        "Mounting share={:?} on server={:?} is still waiting on the system's mount service after {} s",
                        LogPrivate::volume(&watched_share),
                        LogPrivate::host(&watched_server),
                        started.elapsed().as_secs()
                    ),
                }
            }
        });
        Self {
            server: server.to_string(),
            share: share.to_string(),
            started,
            threshold,
            _stop: stop,
        }
    }

    /// The call came back; logs how long it took when that was past the threshold.
    pub(super) fn finish(self, mounted: bool) {
        let took = self.started.elapsed();
        if took < self.threshold {
            return;
        }
        log::warn!(
            "Mounting share={:?} on server={:?} came back from the system's mount service after {} s ({})",
            LogPrivate::volume(&self.share),
            LogPrivate::host(&self.server),
            took.as_secs(),
            if mounted { "mounted" } else { "failed" }
        );
    }
}

#[cfg(test)]
#[path = "mount_watch_test.rs"]
mod mount_watch_test;
