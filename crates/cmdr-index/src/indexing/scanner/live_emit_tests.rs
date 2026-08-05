//! When a live consumer sees the entries a cover walk found.
//!
//! The walk hands entries over in batches, and a batch fills at 2 000 of them. On
//! a sparse tree — one matching file per directory, which is what most searches
//! look like — that means the search dialog shows "0 matches so far" for as long
//! as the walk takes, then everything at once. The rows were found the whole time;
//! only the crossing was missing.
//!
//! So the cadence is the contract: a partial batch that has been sitting for
//! [`EMIT_INTERVAL`](super::live_emit::EMIT_INTERVAL) goes out on its own, and a
//! walk fast enough to fill a batch never pays for the clock.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::live_emit::EmitPacer;
use super::test_fixtures::{MockTree, ReadGate, dir, file, setup_writer};
use super::{ScanProgress, ScanRoot, WalkHeartbeat, WalkPolicy, run_scan};
use crate::indexing::IndexPathSpace;

/// Long enough that nothing in these tests meets it: the walk under test is
/// stopped by a gate, not by the walker's stall watchdog.
const NO_STALL: Duration = Duration::from_secs(30);

/// How long a test waits for something that should take ~100 ms. Generous, so a
/// loaded machine can't fail it; the assertion is "before the walk ends", and the
/// walk cannot end while the gate is closed.
const PATIENCE: Duration = Duration::from_secs(10);

/// Ten distinct directory names for the sparse tree below.
const LEAVES: [&str; 10] = ["d0", "d1", "d2", "d3", "d4", "d5", "d6", "d7", "d8", "d9"];

#[test]
fn a_sparse_walk_hands_over_its_rows_before_it_ends() {
    let root = PathBuf::from("/root");
    // Ten directories, one file each: 20 entries against a 2 000-entry batch, so
    // nothing here ever fills one.
    let mut tree = MockTree::new().dir_at(root.clone(), LEAVES.iter().map(|name| dir(name)).collect::<Vec<_>>());
    for name in LEAVES {
        tree = tree.dir_at(root.join(name), vec![file("found.txt", 10)]);
    }
    // The last directory parks, so the walk is provably still running below.
    let gate = ReadGate::closed();
    let tree = tree.gated_at(root.join(LEAVES[9]), &gate);

    let (entries, batches) = sync_channel(8);
    let cancel = CancellationToken::new();
    let reader = tree.reader(&cancel);
    let (writer, _db_path, _db_dir) = setup_writer();

    std::thread::scope(|scope| {
        let walking = scope.spawn(|| {
            run_scan(
                &root,
                &cancel,
                &Arc::new(ScanProgress::new()),
                &writer,
                2000,
                2,
                WalkPolicy::for_walk(ScanRoot::Volume, &IndexPathSpace::root(), &root),
                &IndexPathSpace::root(),
                reader,
                NO_STALL,
                Some(entries),
                Some(&WalkHeartbeat::new()),
            )
        });

        // Pre-fix this blocked until the walk ended, and the walk could not end:
        // every row sat in a batch that would never fill.
        let batch = batches
            .recv_timeout(PATIENCE)
            .expect("a walk that has read nine directories should have handed its rows over");
        assert!(
            !batch.is_empty(),
            "an empty batch says nothing; the point is the rows arriving"
        );

        gate.open();
        walking.join().expect("the walk thread").expect("the walk runs");
    });
    writer.shutdown();
}

#[test]
fn the_last_partial_batch_still_arrives_when_the_walk_ends() {
    let root = PathBuf::from("/root");
    let tree = MockTree::new()
        .dir_at(root.clone(), vec![dir("one")])
        .dir_at(root.join("one"), vec![file("leaf.txt", 4)]);

    let (entries, batches) = sync_channel(8);
    let cancel = CancellationToken::new();
    let reader = tree.reader(&cancel);
    let (writer, _db_path, _db_dir) = setup_writer();

    run_scan(
        &root,
        &cancel,
        &Arc::new(ScanProgress::new()),
        &writer,
        2000,
        2,
        WalkPolicy::for_walk(ScanRoot::Volume, &IndexPathSpace::root(), &root),
        &IndexPathSpace::root(),
        reader,
        NO_STALL,
        Some(entries),
        Some(&WalkHeartbeat::new()),
    )
    .expect("the walk runs");
    writer.shutdown();

    let found: Vec<_> = batches.iter().flatten().collect();
    assert_eq!(
        found.len(),
        2,
        "both the directory and its file should reach the consumer"
    );
}

// ── The pacer itself ─────────────────────────────────────────────────

#[test]
fn a_pacer_that_nothing_is_waiting_on_is_never_due() {
    let pacer = EmitPacer::with_interval(Duration::ZERO);
    assert!(
        !pacer.is_due(),
        "an idle pacer must not fire, or a full scan pays for a clock read per entry"
    );
}

#[test]
fn a_waiting_batch_is_due_once_its_interval_passes() {
    let mut pacer = EmitPacer::with_interval(Duration::ZERO);
    pacer.waiting();
    assert!(pacer.is_due(), "a batch past its interval goes out");

    pacer.sent();
    assert!(!pacer.is_due(), "a batch that went out starts no new clock");
}

#[test]
fn a_batch_inside_its_interval_waits_for_company() {
    let mut pacer = EmitPacer::with_interval(Duration::from_secs(60));
    pacer.waiting();
    assert!(
        !pacer.is_due(),
        "a fresh batch must keep filling; flushing every entry would be a per-entry chat"
    );
    // A second entry doesn't restart the clock: the oldest row is what's waiting.
    pacer.waiting();
    assert!(!pacer.is_due(), "the clock runs from the first row, not the last");
}
