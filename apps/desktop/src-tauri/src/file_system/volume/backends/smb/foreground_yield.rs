//! When a background SMB transfer should stand aside for the user's navigation.
//!
//! A copy and the pane's directory listings go over the SAME SMB session (every
//! `SmbVolume` clone multiplexes frames over one connection), so a running transfer
//! competes with every navigation on that share. `CheckpointStream`'s foreground
//! auto-yield already knows how to park between chunks; these two functions are the
//! probe it parks on, and they're what `SmbVolume`'s `Volume` foreground-yield
//! methods delegate to.
//!
//! MTP answers the same question from a per-device gate ("a foreground op is in
//! flight on the USB pipe RIGHT NOW"), because a PTP session is a single scarce
//! resource with an explicit holder. SMB has no such holder — frames just
//! interleave — so the signal here is time-based instead: the share counts as busy
//! for [`TRANSFER_FOREGROUND_IDLE_THRESHOLD`] after the last navigation on it
//! (`priority::foreground`'s per-volume timestamp).
//!
//! Scope is PER VOLUME on purpose. A transfer is work the user ASKED for and is
//! watching a progress bar for, so it must only stand aside for navigation on the
//! share it's actually competing with — browsing a local folder has no business
//! slowing a NAS copy. (The index scan makes the same call for the same reason; see
//! `indexing/network_scanner/scan_pace.rs`.)
//!
//! Starvation is handled one layer up and doesn't need a floor here:
//! `CheckpointStream` won't honor a yield until the transfer has moved
//! `min_progress_floor` bytes since its last resume, so continuous browsing slows a
//! copy but can never stop it.
//!
//! [`foreground_pending`] serves BOTH directions: a DOWNLOAD off this share
//! (source arm) and an UPLOAD to it (destination arm, gated by
//! `SmbVolume::supports_foreground_yield_as_destination`). The upload arm reads
//! `foreground_pending` but NOT [`wait_until_foreground_idle`]: it can't park
//! unbounded, because it holds an open write handle across the pause, so it caps
//! each park itself (`write_operations/transfer/checkpoint_stream.rs`).

use std::time::Duration;

use crate::priority::foreground;

/// How long after a navigation the share still counts as in use by the user.
///
/// Deliberately SHORT, unlike the index scan's window. A yield here PARKS the
/// transfer outright (the scan merely drops to one listing in flight), and
/// `CheckpointStream` adds its own quiet-window debounce on top, so a long window
/// would compound into a visibly stalled copy for a single arrow-key press.
pub(crate) const TRANSFER_FOREGROUND_IDLE_THRESHOLD: Duration = Duration::from_millis(500);

/// How often [`wait_until_foreground_idle`] re-checks. The signal is a timestamp,
/// not an event, so there's nothing to wake on; a tick well under the threshold
/// keeps the resume latency a small fraction of the window.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Whether the user is currently using `volume_id` in the foreground, so a
/// background transfer on it should stand aside.
pub(crate) fn foreground_pending(volume_id: &str) -> bool {
    !foreground::global().idle_for_volume(volume_id, TRANSFER_FOREGROUND_IDLE_THRESHOLD)
}

/// Park until `volume_id` has been quiet for [`TRANSFER_FOREGROUND_IDLE_THRESHOLD`].
/// Returns immediately when it already is.
///
/// The caller (`CheckpointStream::auto_yield_to_foreground`) races this against
/// cancellation, so it never needs its own cancel awareness.
pub(crate) async fn wait_until_foreground_idle(volume_id: &str) {
    while foreground_pending(volume_id) {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe the transfer parks on: navigating the share makes it pending, and
    /// an untouched share never is. Volume ids are unique per test so the
    /// process-global tracker can't cross-talk with tests running in parallel.
    #[test]
    fn navigating_a_share_makes_a_transfer_on_it_yield() {
        let browsed = "test://smb_yield/browsed";
        assert!(!foreground_pending(browsed), "nothing noted yet ⇒ nothing to yield to");
        foreground::note_foreground_activity_on(browsed);
        assert!(foreground_pending(browsed), "the user is browsing this share");
    }

    /// THE scope guarantee for transfers: a copy from the NAS must not park because
    /// the user is clicking around a LOCAL folder. That copy is work the user asked
    /// for; only contention on its own share earns a yield.
    #[test]
    fn navigating_a_different_volume_never_yields_this_transfer() {
        let copying_from = "test://smb_yield/copy_source";
        foreground::note_foreground_activity_on("test://smb_yield/some_other_place");
        assert!(!foreground_pending(copying_from));
    }

    /// Resume: the park ends on its own once the share goes quiet, with no
    /// re-arming and no dependence on another navigation arriving.
    #[tokio::test]
    async fn the_park_ends_once_the_share_goes_quiet() {
        let volume_id = "test://smb_yield/goes_quiet";
        foreground::note_foreground_activity_on(volume_id);
        assert!(foreground_pending(volume_id));

        tokio::time::timeout(Duration::from_secs(5), wait_until_foreground_idle(volume_id))
            .await
            .expect("the park must end on its own, not hang until the next navigation");
        assert!(!foreground_pending(volume_id));
    }
}
