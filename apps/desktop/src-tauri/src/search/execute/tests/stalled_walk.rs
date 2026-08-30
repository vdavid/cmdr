//! Waiting on somebody else's walk, and the one case where waiting has to stop.
//!
//! A run whose whole frontier belongs to another walk waits for it, because the
//! alternative is an empty answer (`live_e2e.rs` pins that). A walk IS bounded,
//! but the bound scales badly: a share that has stopped answering fails one
//! listing per 120 s `LIST_TIMEOUT`, and a share the user is browsing drops the
//! walk to one listing in flight, so 32 consecutive failures serialize into
//! roughly an hour — times the number of frontier roots. Waiting an hour for a
//! walk that will deliver nothing is what these tests remove.
//!
//! The signal is progress, never a deadline on the wait: a walk that keeps
//! starting directory reads is waited on however long it takes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::volume::Volume;
use cmdr_index::testing::host::{FakeVolumeProvider, test_lock};
use cmdr_index::{CoverageDimension, Index, NoopEventSink};

use super::super::live_run::{OtherWalk, StallWatch, compress_stall_for_test};
use super::live_drive::*;
use super::*;
use crate::ignore_poison::IgnorePoison;
use crate::search::live::events::CollectorSearchEventSink;
use crate::search::live::{self, SearchPhase, WalkEnding};

/// The real threshold, so what the unit tests below judge is the production
/// number rather than a shrunk one.
const STALL_AFTER: Duration = Duration::from_secs(30);

#[test]
fn a_walk_that_keeps_starting_directory_reads_is_waited_on_however_long_it_takes() {
    // The property that makes this safe to ship: the give-up is about SILENCE,
    // never about how long the wait has run. A cover walk keeps up to 64 listings
    // in flight, so even a walk grinding through a slow share keeps moving the
    // pulse while one read hangs — and a walk of a whole NAS runs for minutes.
    let start = Instant::now();
    let mut watch = StallWatch::new(STALL_AFTER, start);

    // Ten minutes of walking, a read started every 25 s: slower than the stall
    // threshold's own span between readings, and never stalled.
    for step in 1..=24u64 {
        let now = start + Duration::from_secs(25 * step);
        assert_eq!(
            watch.observe(step, now),
            OtherWalk::Released,
            "a walk that is still starting reads is still worth waiting for, {step} readings in"
        );
    }
}

#[test]
fn a_walk_that_starts_no_read_for_the_stall_threshold_is_given_up_on() {
    // The case this exists for: the pulse froze because the walk's concurrency
    // collapsed onto a mount that isn't answering. Nothing is coming, so the run
    // stops waiting and answers with what it has.
    let start = Instant::now();
    let mut watch = StallWatch::new(STALL_AFTER, start);

    assert_eq!(watch.observe(7, start), OtherWalk::Released, "the first reading");
    assert_eq!(
        watch.observe(7, start + STALL_AFTER - Duration::from_millis(1)),
        OtherWalk::Released,
        "a hair under the threshold is still a walk worth waiting for"
    );
    assert_eq!(
        watch.observe(7, start + STALL_AFTER),
        OtherWalk::Stalled,
        "and at the threshold the run gives up on it"
    );
}

#[test]
fn a_pulse_that_moves_again_starts_the_stall_clock_over() {
    // A walk that goes quiet for 29 s, starts one read, and goes quiet again has
    // NOT stalled: the clock measures the gap between readings, never the total
    // wait. Without the reset, every wait longer than the threshold would end in
    // a give-up whatever the walk was doing.
    let start = Instant::now();
    let mut watch = StallWatch::new(STALL_AFTER, start);
    let nearly = STALL_AFTER - Duration::from_secs(1);

    assert_eq!(watch.observe(1, start), OtherWalk::Released);
    assert_eq!(watch.observe(1, start + nearly), OtherWalk::Released);
    assert_eq!(
        watch.observe(2, start + nearly + Duration::from_millis(1)),
        OtherWalk::Released,
        "the walk started a read, so the clock starts over"
    );
    assert_eq!(
        watch.observe(2, start + nearly + nearly),
        OtherWalk::Released,
        "and the wait so far, which is now past the threshold, counts for nothing"
    );
    assert_eq!(
        watch.observe(2, start + nearly + STALL_AFTER + Duration::from_millis(1)),
        OtherWalk::Stalled,
        "only another full threshold of silence ends it"
    );
}

#[test]
fn a_run_stops_waiting_for_a_walk_that_has_stopped_moving_and_says_its_answer_is_short() {
    // End to end: the other walk parks on a listing that never returns — a dead
    // share, in the shape a test can hold open — and the run waiting for its
    // ground gives up rather than waiting out the walk's own bound.
    //
    // It answers through the SAME path a volume nobody can walk takes: what it
    // has, reported as a lower bound (`WalkEnding::Interrupted`). No new phase, no
    // new copy, and the other walk keeps going.
    let _serialized = test_lock();
    let _one_run_at_a_time = live::test_registry_lock();
    // 300 ms instead of 30 s: what this test pins is the give-up and what the run
    // reports after it, and the threshold itself is judged above, at full size.
    let _short_fuse = compress_stall_for_test(Duration::from_millis(300));
    let data = tempfile::tempdir().expect("index data dir");
    let _search_data = volumes::install_data_dir_for_test(data.path());
    let root = format!("{MOUNT_PREFIX}/{VOLUME_ID}");
    let scope = format!("{root}/a");

    let gated = Arc::new(GatedDrive::new(&root, &scope));
    let volumes = FakeVolumeProvider::shared();
    volumes
        .register(VOLUME_ID, Arc::clone(&gated) as Arc<dyn Volume>)
        .mark_network(&root);
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(NoopEventSink::shared())
        .install_for_test();

    // Somebody else's walk takes the scope and parks inside it, where it stays.
    let held = index
        .cover(
            VOLUME_ID,
            vec![scope.clone()],
            CoverageDimension::Listing,
            tokio_util::sync::CancellationToken::new(),
        )
        .expect("the drive is walkable");
    gated.wait_until_reached();

    let searched_scope = scope.clone();
    let watched = Arc::new(CollectorSearchEventSink::default());
    let sink = Arc::clone(&watched);
    let searcher = std::thread::spawn(move || search_watched("stalled-walk", &searched_scope, "txt", &sink));

    // The whole point: it ends on its own, without the gate ever opening.
    cmdr_fs::testing::wait_until(
        Duration::from_secs(10),
        "the run to give up on the walk that stopped moving",
        || !watched.complete.lock_ignore_poison().is_empty(),
    );
    let answer = searcher.join().expect("the waiting run finishes");

    assert_eq!(
        answer.walk,
        WalkEnding::Interrupted,
        "the answer says it is a lower bound, the same way a drive nobody can walk does"
    );
    assert!(
        answer.paths.is_empty() && answer.match_count == 0,
        "and it is honestly empty rather than silently complete: {:?}",
        answer.paths
    );
    assert!(
        answer.phases.contains(&SearchPhase::WaitingForAnotherWalk),
        "it waited first, rather than answering straight away: {:?}",
        answer.phases
    );
    assert!(
        !answer.phases.contains(&SearchPhase::Walking),
        "and never claimed to be walking, having walked nothing: {:?}",
        answer.phases
    );

    // Giving up on a walk is not stopping it: it still holds its ground, and its
    // rows still land in the index for the next search.
    assert_eq!(
        index
            .coverage(VOLUME_ID, &scope, CoverageDimension::Listing)
            .expect("the drive answers for its own coverage")
            .being_walked,
        vec![scope.clone()],
        "the other walk is still there"
    );

    gated.release();
    while held.next_batch().is_some() {}
    let _ = held.finish();
    volumes::forget_volume_for_test(VOLUME_ID);
    let _ = index.forget_volume(VOLUME_ID);
}
