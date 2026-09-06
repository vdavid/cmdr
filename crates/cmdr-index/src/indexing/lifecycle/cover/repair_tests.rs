//! The repair path: a frontier node the parallel walker refuses because the index
//! already holds rows under it.
//!
//! `ground.rs` routes that case to the serial reconcile, which compares by name.
//! What these pin is that the repair behaves like every other primitive: it keeps
//! the rows it didn't write, REPORTS the ones it did, survives the consumer
//! walking away, and calls a cancellation a cancellation. The rest of the driver
//! is `tests.rs`.

use super::ground::{RootOutcome, repair_non_virgin};
use super::test_support::{Fixture, drain};
use super::*;
use crate::indexing::writer::WriteMessage;

/// A frontier node the index already holds rows under is repaired by the serial
/// reconcile, which compares by name: the pre-existing rows keep their ids, the
/// new siblings arrive, and nothing is deleted.
///
/// The other half of
/// `scanner::convergence_tests::covering_a_frontier_node_never_removes_a_row_it_did_not_write`,
/// which pins that the parallel walker refuses this case rather than corrupting it.
#[test]
fn a_non_virgin_frontier_node_is_repaired_without_losing_rows() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    std::fs::write(root.join("F/G/kept.txt"), "kept").expect("file");
    std::fs::write(root.join("F/new.txt"), "new").expect("file");

    // What FSEvents verification leaves behind: G's row under an unlisted F.
    let f_id = f.seed_chain(&root.join("F"));
    f.writer
        .send(WriteMessage::UpsertEntryV2 {
            parent_id: f_id,
            name: "G".to_string(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: None,
            inode: None,
            nlink: None,
        })
        .expect("upsert G");
    f.writer.flush_blocking().expect("flush");
    // … and then G itself gets walked, so it holds rows F has no claim on.
    let g_walk = start(
        f.context(),
        vec![f.path("F/G")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    );
    drain(g_walk);
    f.writer.flush_blocking().expect("flush");

    let g_rows = f.child_ids(&f.path("F/G"));
    assert_eq!(g_rows.len(), 1, "precondition: G holds kept.txt");
    assert_eq!(f.listed_epoch(&f.path("F")), 0, "precondition: F is a frontier node");

    let walk = start(
        f.context(),
        vec![f.path("F")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    );
    let (_, outcome) = drain(walk);
    f.writer.flush_blocking().expect("flush");

    assert_eq!(outcome.roots_covered, 1, "the repair path covered it");
    assert_eq!(
        f.child_ids(&f.path("F/G")),
        g_rows,
        "the rows the walk did not write keep their ids"
    );
    assert!(f.listed_epoch(&f.path("F")) > 0, "and F is covered now");
    assert!(
        f.child_ids(&f.path("F")).len() >= 2,
        "with the sibling the repair discovered, alongside G"
    );
}

/// The repair path REPORTS what it wrote, to the consumer and in the totals.
///
/// A live search's answer is the index's covered half plus what the walk hands
/// back, and the covered half was read off an arena that predates the walk.
/// So a repair that writes rows silently makes the search that paid for it answer
/// as if that ground were empty — and, because the run still ends `Completed`,
/// call the short answer exhaustive. The shape is ordinary: search one folder,
/// then search its parent.
#[test]
fn a_repaired_frontier_node_reports_the_rows_it_wrote() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    std::fs::create_dir_all(root.join("F/sibling")).expect("dirs");
    std::fs::write(root.join("F/G/kept.txt"), "kept").expect("file");
    std::fs::write(root.join("F/new.txt"), "new").expect("file");
    std::fs::write(root.join("F/sibling/deep.txt"), "deep").expect("file");
    f.seed_chain(&root.join("F"));

    // The first search covers G, materializing F above it without listing it.
    drain(start(
        f.context(),
        vec![f.path("F/G")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    ));
    f.writer.flush_blocking().expect("flush");
    assert_eq!(f.listed_epoch(&f.path("F")), 0, "precondition: F is a frontier node");

    // The second search asks for F, which the parallel walker refuses.
    let (entries, outcome) = drain(start(
        f.context(),
        vec![f.path("F")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    ));
    f.writer.flush_blocking().expect("flush");

    let mut emitted: Vec<String> = entries.iter().map(|e| e.path.to_string_lossy().to_string()).collect();
    emitted.sort();
    assert_eq!(
        emitted,
        [f.path("F/new.txt"), f.path("F/sibling"), f.path("F/sibling/deep.txt")],
        "every row the repair wrote reached the consumer: nothing else will ever report them \
         to the search that asked for this walk"
    );
    assert_eq!(outcome.entries_found, 3, "and the totals count the same rows");
    assert_eq!(outcome.dirs_found, 1, "F/sibling among them");
}

/// A repair whose consumer walks away mid-stream still covers the ground.
///
/// The repair hands its rows over a BOUNDED channel ([`BATCH_QUEUE_DEPTH`]), so a
/// walk with more batches than slots parks on a full queue — and the search that
/// asked may be gone by then (a closed dialog, a superseded query). The send
/// failing is what frees it, and what it does next is keep going: walking is
/// coverage work (Decision 11), so the rows land and the frontier stops offering
/// this node whether or not anyone was still listening.
///
/// The sibling of `dropping_the_consumer_leaves_the_walk_running`, which pins the
/// same promise on the PARALLEL walker.
#[test]
fn a_repair_whose_consumer_left_still_covers_the_ground() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    std::fs::write(root.join("F/G/kept.txt"), "kept").expect("file");
    // More directories than the channel holds batches: the repair sends one batch
    // per directory it created rows in, so the walk is parked on a full queue by
    // the time the consumer goes.
    let wide = BATCH_QUEUE_DEPTH + 4;
    for i in 0..wide {
        std::fs::create_dir_all(root.join("F").join(format!("d{i}"))).expect("dirs");
        std::fs::write(root.join("F").join(format!("d{i}/leaf.txt")), "x").expect("file");
    }
    f.seed_chain(&root.join("F"));

    // Cover G first, which materializes F above it without listing it.
    drain(start(
        f.context(),
        vec![f.path("F/G")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    ));
    f.writer.flush_blocking().expect("flush");
    assert_eq!(f.listed_epoch(&f.path("F")), 0, "precondition: F is a frontier node");

    let walk = start(
        f.context(),
        vec![f.path("F")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    );
    // One batch, so the repair is provably under way and filling the queue behind
    // it; then walk away. `finish` drops the channel and waits the walk out.
    walk.next_batch().expect("the repair emits");
    let outcome = walk.finish();
    f.writer.flush_blocking().expect("flush");

    assert!(!outcome.cancelled, "nobody cancelled it");
    assert_eq!(
        outcome.entries_found,
        (wide * 2) as u64,
        "it wrote every directory and its file anyway"
    );
    assert!(
        f.listed_epoch(&f.path("F")) > 0,
        "and the coverage is durable: the next search doesn't walk F again"
    );
}

/// A repair moves the walk's PULSE, not only its batches.
///
/// `foldersFound` and the dialog's "N folders scanned" are read off
/// `WalkHeartbeat::dirs_scanned`, never off the entries — so a repair that fills a
/// list while the pulse sits at zero reports a walk that covered nothing. Worse, a
/// second run waiting on this ground judges the walk by that same number
/// (`search/execute/live_run.rs`'s `OTHER_WALK_STALL`) and gives up on one that is
/// working.
#[test]
fn a_repair_reports_the_directories_it_read() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    std::fs::create_dir_all(root.join("F/sibling")).expect("dirs");
    std::fs::write(root.join("F/sibling/deep.txt"), "deep").expect("file");
    f.seed_chain(&root.join("F"));

    // Cover G first, so F is a frontier node the parallel walker will refuse.
    drain(start(
        f.context(),
        vec![f.path("F/G")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    ));
    f.writer.flush_blocking().expect("flush");

    let walk = start(
        f.context(),
        vec![f.path("F")],
        CoverageDimension::Listing,
        CancellationToken::new(),
        WalkFor::TheIndex,
    );
    let pulse = walk.dirs_scanned_counter();
    drain(walk);

    // F, F/G, and F/sibling: every directory the repair read.
    assert_eq!(
        pulse.load(std::sync::atomic::Ordering::Relaxed),
        3,
        "the repair pulses per directory read, exactly as the parallel walker does"
    );
}

/// The repair path reports a cancellation as one, rather than as a covered node.
///
/// `reconcile_subtree` breaks out of its walk on cancel and returns `Ok`, so
/// without `ReconcileSummary.cancelled` this arm would count a stopped repair as
/// a finished one and the frontier would look smaller than it is.
#[test]
fn a_cancelled_repair_is_reported_as_cancelled_not_covered() {
    let f = Fixture::new();
    let root = f.tree.path();
    std::fs::create_dir_all(root.join("F/G")).expect("dirs");
    f.seed_chain(&root.join("F/G"));

    let cancel = CancellationToken::new();
    cancel.cancel();
    let (sender, _batches) = sync_channel(1);
    let heartbeat = WalkHeartbeat::new();
    let (summary, verdict) = repair_non_virgin(&f.context(), &root.join("F"), &sender, &cancel, &heartbeat);
    assert_eq!(
        verdict,
        RootOutcome::Cancelled,
        "a repair whose token had already fired covered nothing"
    );
    assert_eq!(summary.map(|s| s.total_entries), Some(0), "and wrote nothing to report");
}
