//! The app's answer to the index's "may background work run right now?" question.
//!
//! The index subsystems are being extracted into a crate that can't reach
//! `crate::priority`, so they ask through `indexing::host::policy::HostPolicy` and
//! this is the implementation the app installs at startup. It's a pure adapter: the
//! priority order, the scopes, and the signals all still live in this module's
//! siblings, and every decision the index used to make inline is made here now.

use std::time::Duration;

use cmdr_index::host::policy::{HostPolicy, OpenListing, WorkClearance};

use super::{foreground, transfers};

/// Answers the index from the app's real priority signals.
pub struct AppHostPolicy;

impl HostPolicy for AppHostPolicy {
    fn clearance(&self, volume_id: &str, idle_threshold: Duration) -> WorkClearance {
        WorkClearance {
            app_idle: foreground::global().idle_for(idle_threshold),
            volume_idle: foreground::global().idle_for_volume(volume_id, idle_threshold),
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
}
