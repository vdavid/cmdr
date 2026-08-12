//! What the broadcast publishes when the local listing DOESN'T come back.
//!
//! The rule under test is the one whose absence stranded a user with a blank volume
//! picker and a refresh button that couldn't fix it: a failed listing carries the last
//! good set, and only a failure before any successful listing publishes nothing.

use super::{ListingOutcome, LocationInfo, publishable};

#[cfg(target_os = "macos")]
use crate::volumes::LocationCategory;
#[cfg(target_os = "linux")]
use crate::volumes_linux::LocationCategory;

fn volume(id: &str) -> LocationInfo {
    LocationInfo {
        id: id.to_string(),
        name: id.to_string(),
        path: format!("/Volumes/{id}"),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
        is_read_only: false,
        is_disk_image: false,
        fs_type: None,
        supports_trash: true,
        smb_connection_state: None,
        usb_speed: None,
    }
}

fn ids(volumes: &[LocationInfo]) -> Vec<&str> {
    volumes.iter().map(|v| v.id.as_str()).collect()
}

#[test]
fn a_successful_listing_publishes_itself_and_becomes_the_last_good_set() {
    let mut last_good = vec![volume("stale")];
    let (published, timed_out) = publishable(ListingOutcome::Listed(vec![volume("fresh")]), &mut last_good);

    assert_eq!(ids(&published), ["fresh"]);
    assert!(!timed_out);
    assert_eq!(ids(&last_good), ["fresh"], "the new listing replaces the last good set");
}

#[test]
fn a_timeout_carries_the_last_good_set_instead_of_blanking_the_picker() {
    // Pre-fix this published an empty list beside `timed_out: true`, so the picker read
    // "no volumes" and the refresh button re-ran the same timeout forever.
    let mut last_good = vec![volume("Macintosh HD"), volume("naspi")];
    let (published, timed_out) = publishable(ListingOutcome::TimedOut, &mut last_good);

    assert_eq!(ids(&published), ["Macintosh HD", "naspi"]);
    assert!(timed_out, "still flagged incomplete, so the UI keeps saying so");
    assert_eq!(ids(&last_good), ["Macintosh HD", "naspi"], "a timeout doesn't clear it");
}

#[test]
fn repeated_timeouts_keep_carrying_the_same_set() {
    // The refresh button's path: every retry that times out must still publish the
    // volumes, not erode them.
    let mut last_good = vec![volume("Macintosh HD")];
    for _ in 0..3 {
        let (published, timed_out) = publishable(ListingOutcome::TimedOut, &mut last_good);
        assert_eq!(ids(&published), ["Macintosh HD"]);
        assert!(timed_out);
    }
}

#[test]
fn a_panic_carries_the_last_good_set_but_isnt_flagged_as_slow() {
    let mut last_good = vec![volume("Macintosh HD")];
    let (published, timed_out) = publishable(ListingOutcome::Panicked, &mut last_good);

    assert_eq!(ids(&published), ["Macintosh HD"]);
    assert!(!timed_out, "a panic isn't a slow listing; the retry affordance is for slow");
}

#[test]
fn a_timeout_before_any_successful_listing_publishes_nothing() {
    // At startup there's genuinely nothing better to say, and inventing volumes would
    // be worse than an honest empty list flagged incomplete.
    let mut last_good = Vec::new();
    let (published, timed_out) = publishable(ListingOutcome::TimedOut, &mut last_good);

    assert!(published.is_empty());
    assert!(timed_out);
}

#[test]
fn an_unmount_shrinks_the_set_once_a_listing_succeeds_again() {
    // The staleness bound: carrying a gone volume is only ever until the next listing
    // that completes, which is what keeps the trade acceptable.
    let mut last_good = Vec::new();
    publishable(
        ListingOutcome::Listed(vec![volume("Macintosh HD"), volume("USB")]),
        &mut last_good,
    );
    publishable(ListingOutcome::TimedOut, &mut last_good);

    let (published, _) = publishable(ListingOutcome::Listed(vec![volume("Macintosh HD")]), &mut last_good);
    assert_eq!(ids(&published), ["Macintosh HD"], "the ejected volume is gone");
    assert_eq!(ids(&last_good), ["Macintosh HD"]);
}
