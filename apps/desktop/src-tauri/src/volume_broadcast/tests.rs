//! What the broadcast publishes when the local listing is slow or DOESN'T come back.
//!
//! Two rules under test:
//!
//! - A failed listing carries the last good set, and only a failure before any
//!   successful listing publishes nothing. Its absence stranded a user with a blank
//!   volume picker and a refresh button that couldn't fix it.
//! - A slow listing never holds back the server rows. A hung mount used to hold the
//!   whole list for 2 s, so a re-added server kept its OLD folder in the switcher and
//!   opening it said "Path not found".
//!
//! The round cells run on a paused clock: every `sleep` is an exact jump in virtual
//! time, so "the event went out at 100 ms" is a fact, not a race.

use super::round::{Broadcaster, LocalSnapshot};
use super::{LIST_TIMEOUT, PROVISIONAL_AFTER, VolumesChanged};
use crate::ignore_poison::IgnorePoison;
use crate::volume_listing::{ListingOutcome, LocationCategory, LocationInfo};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::Instant;

fn volume(id: &str) -> LocationInfo {
    LocationInfo {
        id: id.to_string(),
        name: id.to_string(),
        path: format!("/Volumes/{id}"),
        category: LocationCategory::MainVolume,
        icon: None,
        is_ejectable: false,
        mount_is_read_only: false,
        is_disk_image: false,
        is_cloud_mount: false,
        fs_type: None,
        supports_trash: true,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        root_label: None,
        mount_account: None,
    }
}

/// A server row: same id whatever its folder, which is what made a stale one bite.
fn server(folder: &str) -> LocationInfo {
    LocationInfo {
        path: format!("sftp://ada@nas.local:22{folder}"),
        category: LocationCategory::Network,
        ..volume("sftp-ada-nas")
    }
}

fn ids(volumes: &[LocationInfo]) -> Vec<&str> {
    volumes.iter().map(|v| v.id.as_str()).collect()
}

fn paths(volumes: &[LocationInfo]) -> Vec<&str> {
    volumes.iter().map(|v| v.path.as_str()).collect()
}

// ============================================================================
// The snapshot
// ============================================================================

/// Runs one discovery through the snapshot and reads what it would publish.
fn apply(snapshot: &mut LocalSnapshot, outcome: ListingOutcome) -> (Vec<LocationInfo>, bool) {
    let round = snapshot.begin_for_test();
    snapshot.finish(round, outcome);
    let (volumes, timed_out, _) = snapshot.local_part();
    (volumes, timed_out)
}

#[test]
fn a_successful_listing_publishes_itself_and_becomes_the_last_good_set() {
    let mut snapshot = LocalSnapshot::new();
    apply(&mut snapshot, ListingOutcome::Listed(vec![volume("stale")]));
    let (published, timed_out) = apply(&mut snapshot, ListingOutcome::Listed(vec![volume("fresh")]));

    assert_eq!(ids(&published), ["fresh"], "the new listing replaces the last good set");
    assert!(!timed_out);
}

#[test]
fn a_timeout_carries_the_last_good_set_instead_of_blanking_the_picker() {
    // Pre-fix this published an empty list beside `timed_out: true`, so the picker read
    // "no volumes" and the refresh button re-ran the same timeout forever.
    let mut snapshot = LocalSnapshot::new();
    apply(
        &mut snapshot,
        ListingOutcome::Listed(vec![volume("Macintosh HD"), volume("naspi")]),
    );
    let (published, timed_out) = apply(&mut snapshot, ListingOutcome::TimedOut);

    assert_eq!(ids(&published), ["Macintosh HD", "naspi"]);
    assert!(timed_out, "still flagged incomplete, so the UI keeps saying so");
}

#[test]
fn repeated_timeouts_keep_carrying_the_same_set() {
    // The refresh button's path: every retry that times out must still publish the
    // volumes, not erode them.
    let mut snapshot = LocalSnapshot::new();
    apply(&mut snapshot, ListingOutcome::Listed(vec![volume("Macintosh HD")]));
    for _ in 0..3 {
        let (published, timed_out) = apply(&mut snapshot, ListingOutcome::TimedOut);
        assert_eq!(ids(&published), ["Macintosh HD"]);
        assert!(timed_out);
    }
}

#[test]
fn a_panic_carries_the_last_good_set_but_isnt_flagged_as_slow() {
    let mut snapshot = LocalSnapshot::new();
    apply(&mut snapshot, ListingOutcome::Listed(vec![volume("Macintosh HD")]));
    let (published, timed_out) = apply(&mut snapshot, ListingOutcome::Panicked);

    assert_eq!(ids(&published), ["Macintosh HD"]);
    assert!(
        !timed_out,
        "a panic isn't a slow listing; the retry affordance is for slow"
    );
}

#[test]
fn a_timeout_before_any_successful_listing_publishes_nothing() {
    // At startup there's genuinely nothing better to say, and inventing volumes would
    // be worse than an honest empty list flagged incomplete.
    let mut snapshot = LocalSnapshot::new();
    let (published, timed_out) = apply(&mut snapshot, ListingOutcome::TimedOut);

    assert!(published.is_empty());
    assert!(timed_out);
}

#[test]
fn an_unmount_shrinks_the_set_once_a_listing_succeeds_again() {
    // The staleness bound: carrying a gone volume is only ever until the next listing
    // that completes, which is what keeps the trade acceptable.
    let mut snapshot = LocalSnapshot::new();
    apply(
        &mut snapshot,
        ListingOutcome::Listed(vec![volume("Macintosh HD"), volume("USB")]),
    );
    apply(&mut snapshot, ListingOutcome::TimedOut);
    let (published, _) = apply(&mut snapshot, ListingOutcome::Listed(vec![volume("Macintosh HD")]));

    assert_eq!(ids(&published), ["Macintosh HD"], "the ejected volume is gone");
}

#[test]
fn an_older_discovery_finishing_last_doesnt_roll_the_snapshot_back() {
    // Rounds overlap whenever a mount hangs. The one that STARTED first saw the older
    // world, whatever order they finish in.
    let mut snapshot = LocalSnapshot::new();
    let older = snapshot.begin_for_test();
    let newer = snapshot.begin_for_test();
    snapshot.finish(
        newer,
        ListingOutcome::Listed(vec![volume("Macintosh HD"), volume("USB")]),
    );
    let (_, _, pending) = snapshot.local_part();
    assert!(pending, "the older discovery is still running");

    snapshot.finish(older, ListingOutcome::Listed(vec![volume("Macintosh HD")]));
    let (published, timed_out, pending) = snapshot.local_part();

    assert_eq!(ids(&published), ["Macintosh HD", "USB"]);
    assert!(!timed_out);
    assert!(!pending);
}

// ============================================================================
// The round
// ============================================================================

/// Events one cell's rounds emitted, each with the virtual time it went out at.
type Events = Arc<Mutex<Vec<(Duration, VolumesChanged)>>>;

/// The server rows `complete` appends, as they are at EMIT time.
type Servers = Arc<Mutex<Vec<LocationInfo>>>;

/// Runs one round with a discovery that answers `outcome` after `delay`.
async fn round(
    broadcaster: &Broadcaster,
    servers: &Servers,
    events: &Events,
    start: Instant,
    delay: Duration,
    outcome: ListingOutcome,
) {
    let discovery = async move {
        // allowed-test-sleep: how long discovery takes, in virtual time on a start_paused runtime.
        tokio::time::sleep(delay).await;
        outcome
    };
    let complete = |local: Vec<LocationInfo>| {
        let servers = Arc::clone(servers);
        async move {
            let mut list = local;
            list.extend(servers.lock_ignore_poison().iter().cloned());
            list
        }
    };
    let events = Arc::clone(events);
    broadcaster
        .round(discovery, complete, move |payload| {
            events.lock_ignore_poison().push((start.elapsed(), payload));
        })
        .await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_hung_mount_doesnt_hold_back_a_re_added_servers_new_folder() {
    let broadcaster = Broadcaster::new();
    let servers: Servers = Arc::new(Mutex::new(vec![server("/A")]));
    let events: Events = Arc::default();
    let start = Instant::now();
    round(
        &broadcaster,
        &servers,
        &events,
        start,
        Duration::ZERO,
        ListingOutcome::Listed(vec![volume("Macintosh HD")]),
    )
    .await;
    events.lock_ignore_poison().clear();

    // The user forgets the server and re-adds it with folder B; a Tailscale share
    // hangs the next discovery until it times out.
    *servers.lock_ignore_poison() = vec![server("/B")];
    let start = Instant::now();
    round(
        &broadcaster,
        &servers,
        &events,
        start,
        LIST_TIMEOUT,
        ListingOutcome::TimedOut,
    )
    .await;

    let events = events.lock_ignore_poison();
    let (at, first) = &events[0];
    assert_eq!(
        *at, PROVISIONAL_AFTER,
        "the server rows go out as soon as discovery is late, not when it gives up"
    );
    assert_eq!(
        paths(&first.data),
        ["/Volumes/Macintosh HD", "sftp://ada@nas.local:22/B"],
        "fresh server row beside the cached local part"
    );
    assert!(
        first.discovery_pending,
        "the local part is standing in, and more follows"
    );
    assert!(
        !first.timed_out,
        "no verdict yet: the last finished listing was complete"
    );

    let (at, last) = &events[1];
    assert_eq!(*at, LIST_TIMEOUT);
    assert_eq!(
        paths(&last.data),
        ["/Volumes/Macintosh HD", "sftp://ada@nas.local:22/B"]
    );
    assert!(
        last.timed_out,
        "the discovery gave up, so the local part is the last good set"
    );
    assert!(!last.discovery_pending);
    assert_eq!(events.len(), 2);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_healthy_discovery_publishes_once() {
    let broadcaster = Broadcaster::new();
    let servers: Servers = Arc::new(Mutex::new(vec![server("/A")]));
    let events: Events = Arc::default();
    let start = Instant::now();
    round(
        &broadcaster,
        &servers,
        &events,
        start,
        Duration::from_millis(30),
        ListingOutcome::Listed(vec![volume("Macintosh HD")]),
    )
    .await;

    let events = events.lock_ignore_poison();
    assert_eq!(events.len(), 1, "no provisional event when discovery is on time");
    let (at, only) = &events[0];
    assert_eq!(*at, Duration::from_millis(30));
    assert_eq!(ids(&only.data), ["Macintosh HD", "sftp-ada-nas"]);
    assert!(!only.timed_out);
    assert!(!only.discovery_pending);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn a_round_lost_mid_discovery_doesnt_leave_every_later_event_pending() {
    let broadcaster = Broadcaster::new();
    let servers: Servers = Arc::default();
    let events: Events = Arc::default();
    let start = Instant::now();

    // Dropped while its discovery is still out, the way a cancelled task would be.
    let lost = round(
        &broadcaster,
        &servers,
        &events,
        start,
        Duration::from_secs(60),
        ListingOutcome::TimedOut,
    );
    let _ = tokio::time::timeout(PROVISIONAL_AFTER * 2, lost).await;

    round(
        &broadcaster,
        &servers,
        &events,
        start,
        Duration::ZERO,
        ListingOutcome::Listed(vec![volume("Macintosh HD")]),
    )
    .await;

    let events = events.lock_ignore_poison();
    let (_, last) = events.last().expect("the second round emitted");
    assert!(!last.discovery_pending, "the lost round no longer counts as running");
    assert_eq!(ids(&last.data), ["Macintosh HD"]);
}
