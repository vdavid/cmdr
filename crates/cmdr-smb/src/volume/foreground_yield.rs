//! When a background SMB transfer should stand aside for the user's navigation.
//!
//! A copy and the pane's directory listings go over the SAME SMB session (every
//! `SmbVolume` clone multiplexes frames over one connection), so a running transfer
//! competes with every navigation on that share. `CheckpointStream`'s foreground
//! auto-yield already knows how to park between chunks; these two functions are the
//! probe it parks on, and they're what `SmbVolume`'s `Volume` foreground-yield
//! methods delegate to.
//!
//! The share is busy while a foreground operation HOLDS A LEASE on it (a directory
//! listing takes one for its real duration), and for
//! [`TRANSFER_FOREGROUND_IDLE_THRESHOLD`] after the last one ended. The two halves
//! are composed once, in `cmdr_fs::volume::host::activity::volume_busy_for_user`.
//!
//! ❗ **The lease is what makes this exact, and it is not the connection.** The SMB
//! connection has no holder — every `SmbVolume` clone multiplexes frames over one
//! session, so there is nothing there to count, which is why this was time-based
//! alone and why a slow listing used to stop counting halfway through the user's
//! wait. A LISTING, though, is a scoped operation with a beginning and an end, so
//! it can hold a lease exactly the way an MTP foreground op holds its per-device
//! gate.
//!
//! The host reports the raw signals; the threshold stays here, because how long
//! counts as "busy" belongs to the work standing aside. A transfer parks outright,
//! so its window is short; an index scan that merely narrows wants far longer.
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

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::activity;

/// How long after a foreground operation ENDS the share still counts as in use by
/// the user. The window that debounces a burst of navigations into one park; the
/// operations themselves are covered exactly, by their leases.
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
///
/// True while any foreground operation holds a lease on the share, and for
/// [`TRANSFER_FOREGROUND_IDLE_THRESHOLD`] after the last one ends.
pub(crate) fn foreground_pending(host: &VolumeHost, volume_id: &str) -> bool {
    activity::volume_busy_for_user(host.activity(), volume_id, TRANSFER_FOREGROUND_IDLE_THRESHOLD)
}

/// Park until `volume_id` has been quiet for [`TRANSFER_FOREGROUND_IDLE_THRESHOLD`].
/// Returns immediately when it already is.
///
/// The caller (`CheckpointStream::auto_yield_to_foreground`) races this against
/// cancellation, so it never needs its own cancel awareness.
pub(crate) async fn wait_until_foreground_idle(host: &VolumeHost, volume_id: &str) {
    while foreground_pending(host, volume_id) {
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cmdr_fs::volume::host::activity::BusyVolumes;

    use super::*;

    /// A host whose only wired seam is the activity signal `busy` reports.
    fn host_watching(busy: Arc<BusyVolumes>) -> VolumeHost {
        VolumeHost::builder().activity(busy).build()
    }

    /// The probe the transfer parks on: navigating the share makes it pending, and
    /// an untouched share never is.
    #[test]
    fn navigating_a_share_makes_a_transfer_on_it_yield() {
        let browsed = "test://smb_yield/browsed";
        let idle = Arc::new(BusyVolumes::new());
        assert!(
            !foreground_pending(&host_watching(Arc::clone(&idle)), browsed),
            "nothing noted yet ⇒ nothing to yield to"
        );

        let busy = Arc::new(BusyVolumes::new().is_busy(browsed));
        assert!(
            foreground_pending(&host_watching(busy), browsed),
            "the user is browsing this share"
        );
    }

    /// The gap the lease closes: a listing that outlives the idle threshold keeps
    /// the share busy for its whole duration. With the timestamp alone, a 10 s
    /// listing was protected for its first half-second and the upload then
    /// competed for the rest of the user's wait.
    #[test]
    fn a_listing_in_flight_keeps_a_transfer_yielding_however_long_it_takes() {
        let browsed = "test://smb_yield/slow_listing";
        let listing = Arc::new(BusyVolumes::new().holds_a_lease(browsed));
        let host = host_watching(Arc::clone(&listing));
        assert!(
            host.activity()
                .volume_idle_for(browsed, TRANSFER_FOREGROUND_IDLE_THRESHOLD),
            "the timestamp half has already decayed, which is the case this covers"
        );
        assert!(foreground_pending(&host, browsed), "the listing is still running");

        listing.releases_a_lease(browsed);
        assert!(!foreground_pending(&host, browsed), "the listing finished");
    }

    /// THE scope guarantee for transfers: a copy from the NAS must not park because
    /// the user is clicking around a LOCAL folder. That copy is work the user asked
    /// for; only contention on its own share earns a yield.
    #[test]
    fn navigating_a_different_volume_never_yields_this_transfer() {
        let copying_from = "test://smb_yield/copy_source";
        let busy_elsewhere = Arc::new(BusyVolumes::new().is_busy("test://smb_yield/some_other_place"));
        assert!(!foreground_pending(&host_watching(busy_elsewhere), copying_from));
    }

    /// Resume: the park ends on its own once the share goes quiet, with no
    /// re-arming and no dependence on another navigation arriving.
    #[tokio::test]
    async fn the_park_ends_once_the_share_goes_quiet() {
        let volume_id = "test://smb_yield/goes_quiet";
        let busy = Arc::new(BusyVolumes::new().is_busy(volume_id));
        let host = host_watching(Arc::clone(&busy));
        assert!(foreground_pending(&host, volume_id));

        let quieting = Arc::clone(&busy);
        let volume = volume_id.to_string();
        tokio::spawn(async move {
            // allowed-test-sleep: the head start IS the subject — the park has to
            // already be running when the share goes quiet.
            tokio::time::sleep(Duration::from_millis(50)).await;
            quieting.goes_quiet(&volume);
        });

        tokio::time::timeout(Duration::from_secs(5), wait_until_foreground_idle(&host, volume_id))
            .await
            .expect("the park must end on its own, not hang until the next navigation");
        assert!(!foreground_pending(&host, volume_id));
    }
}
