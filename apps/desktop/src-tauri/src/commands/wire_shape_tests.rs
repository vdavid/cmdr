//! Every command family's refusal is distinguishable BY VARIANT, with its data
//! intact, once it crosses the wire.
//!
//! The families were flattened into `{ message, timed_out }` before, so a
//! `Busy` and an OS refusal arrived as two English sentences a caller could only
//! tell apart by reading them. These pin the shape the frontend matches on: one
//! `type` tag, camelCase fields, and no free-form text carrying the
//! classification. `DeadlineError`'s own cell lives with it, in
//! `cmdr_fs::deadline`.

use crate::file_system::volume::reconnect_error::ReconnectError;
use cmdr_listing::FuzzyJumpError;
use cmdr_operations::VolumeScanError;
use serde_json::json;

#[test]
fn a_reconnect_refusal_carries_the_volumes_own_answer_rather_than_a_sentence() {
    let json = serde_json::to_value(ReconnectError::Volume {
        error: cmdr_fs::volume::VolumeError::NotSupported,
    })
    .unwrap();
    assert_eq!(json["type"], "volume");
    // The nested `VolumeError` stays a VALUE, so the frontend's existing
    // volume factory words it rather than a second vocabulary being invented.
    assert!(json["error"].is_object() || json["error"].is_string());
}

#[test]
fn a_scan_refusal_says_whether_the_destination_is_gone_or_not_connected() {
    assert_eq!(
        serde_json::to_value(VolumeScanError::DestinationVolumeNotConnected {
            volume_id: "mtp-1:65537".to_string()
        })
        .unwrap(),
        json!({ "type": "destinationVolumeNotConnected", "volumeId": "mtp-1:65537" })
    );
    assert_eq!(
        serde_json::to_value(VolumeScanError::DestinationVolumeNotFound {
            volume_id: "smb-testshare".to_string()
        })
        .unwrap(),
        json!({ "type": "destinationVolumeNotFound", "volumeId": "smb-testshare" })
    );
}

#[test]
fn a_lost_listing_says_which_one() {
    assert_eq!(
        serde_json::to_value(FuzzyJumpError::ListingNotFound {
            listing_id: "listing-7".to_string()
        })
        .unwrap(),
        json!({ "type": "listingNotFound", "listingId": "listing-7" })
    );
}
