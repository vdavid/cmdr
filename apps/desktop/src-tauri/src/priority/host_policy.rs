//! The app's answers to "may background work run right now?" and "which folders
//! matter to this user?", for the two subsystems that can't reach `crate::priority`
//! to work them out themselves.
//!
//! The index subsystems live in a crate of their own and ask through
//! `indexing::host::policy::HostPolicy`; a storage backend asks the narrower
//! `cmdr_fs::volume::host::activity::UserActivity`. Both implementations here are
//! pure adapters: the priority order, the scopes, the signals, and the walk order
//! all still live in this module's siblings.

use std::path::PathBuf;
use std::time::Duration;

use cmdr_fs::volume::host::activity::{UserActivity, volume_busy_for_user};
use cmdr_index::host::policy::{HostPolicy, OpenListing, WorkClearance};

use super::{foreground, roots, transfers};

/// Answers the index from the app's real priority signals.
pub struct AppHostPolicy;

impl HostPolicy for AppHostPolicy {
    fn clearance(&self, volume_id: &str, idle_threshold: Duration) -> WorkClearance {
        WorkClearance {
            app_idle: foreground::global().idle_for(idle_threshold),
            // Through the same composed rule the storage backends read, so the two
            // background consumers can never disagree about whether the user is
            // waiting on a volume. A listing in flight counts, which is why a scan
            // now narrows for the listing's whole duration and not just for the
            // idle window after it started.
            volume_idle: !volume_busy_for_user(&AppUserActivity, volume_id, idle_threshold),
            transfer_active: transfers::transfer_active(volume_id),
        }
    }

    fn open_listings(&self) -> Vec<OpenListing> {
        crate::file_system::listing::caching::snapshot_listings()
            .into_iter()
            .map(|listing| OpenListing {
                volume_id: listing.volume_id,
                path: listing.path,
            })
            .collect()
    }

    fn priority_roots(&self, volume_id: &str) -> Vec<PathBuf> {
        roots::priority_roots(volume_id)
    }
}

/// Answers a storage backend's "is the user busy on this volume?" from the same
/// foreground signal.
///
/// Per volume, and only per volume: a transfer off a NAS is work the user asked
/// for and is watching a progress bar for, so it stands aside for contention on
/// the volume it's competing with and for nothing else. The THRESHOLD stays with
/// the caller, which is why none appears here.
pub struct AppUserActivity;

impl UserActivity for AppUserActivity {
    fn volume_idle_for(&self, volume_id: &str, threshold: Duration) -> bool {
        foreground::global().idle_for_volume(volume_id, threshold)
    }

    fn volume_foreground_leases(&self, volume_id: &str) -> usize {
        foreground::global().volume_lease_count(volume_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The adapter has to carry BOTH foreground scopes through, not collapse them.
    /// Browsing one share must leave another's scan at full speed, which is the whole
    /// reason `volume_idle` exists next to `app_idle`.
    #[test]
    fn a_scoped_note_marks_the_app_busy_but_only_its_own_volume() {
        let browsed = "test://host_policy/browsed";
        let quiet = "test://host_policy/quiet";
        let window = Duration::from_secs(30);

        foreground::note_foreground_activity_on(browsed);

        let browsed_clearance = AppHostPolicy.clearance(browsed, window);
        assert!(!browsed_clearance.app_idle, "any activity makes the app busy");
        assert!(!browsed_clearance.volume_idle, "the browsed volume is busy");

        assert!(
            AppHostPolicy.clearance(quiet, window).volume_idle,
            "a volume nobody browsed stays idle"
        );
    }

    /// Transfers come through on the per-volume scope, and don't leak onto other
    /// volumes.
    #[test]
    fn a_transfer_is_reported_only_for_its_own_volume() {
        let busy = "test://host_policy/transferring".to_string();
        let window = Duration::from_secs(30);

        transfers::note_transfer_started(std::slice::from_ref(&busy));
        assert!(AppHostPolicy.clearance(&busy, window).transfer_active);
        assert!(
            !AppHostPolicy
                .clearance("test://host_policy/idle", window)
                .transfer_active
        );
        transfers::note_transfer_finished(std::slice::from_ref(&busy));
        assert!(!AppHostPolicy.clearance(&busy, window).transfer_active);
    }

    /// A backend yields to browsing on ITS volume and to nothing else. Collapsing
    /// this to the app-wide signal would park a NAS copy every time the user
    /// scrolls a local folder.
    #[test]
    fn a_backend_sees_only_its_own_volume_as_busy() {
        let browsed = "test://user_activity/browsed";
        let quiet = "test://user_activity/quiet";
        let window = Duration::from_secs(30);

        foreground::note_foreground_activity_on(browsed);

        assert!(!AppUserActivity.volume_idle_for(browsed, window));
        assert!(
            AppUserActivity.volume_idle_for(quiet, window),
            "a volume nobody browsed is one nobody is waiting on"
        );
    }

    /// The lease reaches BOTH consumers through this adapter: a listing in flight
    /// makes the volume busy for a storage backend AND un-clear for the index, even
    /// once the timestamp half has decayed.
    ///
    /// This is the whole of the scan's behavior change. The rest of its guarantee is
    /// pinned where it belongs: `scan_pace::tests::the_budget_is_never_zero_for_any_input`
    /// proves no `WorkClearance` (this one included) can stop the walk.
    #[test]
    fn a_listing_in_flight_makes_the_volume_busy_for_both_consumers() {
        let listed = "test://host_policy/leased";
        // A window of zero means the timestamp half reads idle immediately, so
        // whatever answers "busy" below can only be the lease.
        let decayed = Duration::ZERO;

        let listing = foreground::lease_foreground_on(listed);
        assert!(
            AppUserActivity.volume_idle_for(listed, decayed),
            "the timestamp half is idle at a zero window, which is what isolates the lease"
        );
        assert_eq!(AppUserActivity.volume_foreground_leases(listed), 1);
        assert!(
            !AppHostPolicy.clearance(listed, decayed).volume_idle,
            "the index must narrow for the listing's whole duration"
        );

        drop(listing);
        assert_eq!(AppUserActivity.volume_foreground_leases(listed), 0);
        assert!(
            !AppHostPolicy.clearance(listed, Duration::from_secs(30)).volume_idle,
            "the listing stamped the volume, so the debounce keeps the index narrowed after it"
        );
    }

    /// A lease is per volume, so a listing on one share leaves another share's scan
    /// and transfers at full speed.
    #[test]
    fn a_lease_on_one_volume_leaves_another_clear() {
        let listed = "test://host_policy/leased_elsewhere";
        let quiet = "test://host_policy/untouched";
        let _listing = foreground::lease_foreground_on(listed);
        assert!(AppHostPolicy.clearance(quiet, Duration::ZERO).volume_idle);
        assert!(AppUserActivity.volume_idle_for(quiet, Duration::ZERO));
        assert_eq!(AppUserActivity.volume_foreground_leases(quiet), 0);
    }
}
