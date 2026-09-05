//! What `MtpVolume` declares about itself through the `Volume` trait.
//!
//! The capability answers a caller routes on, and the watch-coverage gate the
//! app's fresh-listing oracle reads. The gate is the one that needs a device:
//! coverage is "is this storage's session live right now", so it can only be
//! asserted by connecting one and taking it away again.

use std::path::Path;
use std::sync::Arc;

use cmdr_fs::volume::{Volume, WatchCoverage};

use crate::testing::{connect_virtual_device, device_lock, test_connection_manager, volume_for};
use crate::volume::MtpVolume;

/// MTP volumes answer `false` because they have their own event loop, which
/// handles watching independently. The `can_watch_listings` question is the
/// local notify-based watcher's, and it doesn't work on MTP paths.
#[test]
fn can_watch_listings_is_false_because_the_event_loop_does_the_watching() {
    let volume = MtpVolume::new(Arc::clone(test_connection_manager()), "mtp-20-5", 65537, "Test");
    assert!(!volume.can_watch_listings());
}

/// Streaming is on, which is what makes a direct MTP-to-MTP transfer possible.
#[test]
fn supports_streaming_is_true() {
    let volume = MtpVolume::new(Arc::clone(test_connection_manager()), "mtp-20-5", 65537, "Test");
    assert!(volume.supports_streaming());
}

/// A volume whose device was never connected reports no coverage. The negative
/// arm needs no device, so it holds no lock and runs in every build.
#[test]
fn watch_coverage_is_none_for_a_device_that_was_never_connected() {
    let volume = MtpVolume::new(
        Arc::clone(test_connection_manager()),
        "mtp-never-connected-9999",
        65537,
        "Test",
    );
    assert_eq!(volume.listing_watch_coverage(Path::new("/DCIM")), WatchCoverage::None);
}

/// Connecting flips the gate to `EveryWriter` and disconnecting drops it again.
///
/// The app's fresh-listing oracle skips a device round trip on the strength of
/// this answer, so a gate stuck open serves a pane from a cache nothing is
/// refreshing any more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn watch_coverage_flips_with_the_connection() {
    let _guard = device_lock().await;
    let device = connect_virtual_device(test_connection_manager()).await;

    // Before connect, addressed by the same device id but a storage that was
    // never attached: still nothing.
    let unconnected = MtpVolume::new(Arc::clone(test_connection_manager()), "mtp-not-here-1", 65537, "Test");
    assert_eq!(
        unconnected.listing_watch_coverage(Path::new("/")),
        WatchCoverage::None,
        "a storage nothing connected has no coverage"
    );

    let volume = volume_for(test_connection_manager(), &device, None).await;
    assert_eq!(
        volume.listing_watch_coverage(Path::new("/")),
        WatchCoverage::EveryWriter,
        "a connected storage is covered"
    );

    device.teardown(test_connection_manager()).await;
    assert_eq!(
        volume.listing_watch_coverage(Path::new("/")),
        WatchCoverage::None,
        "and the coverage goes with the session"
    );
}
