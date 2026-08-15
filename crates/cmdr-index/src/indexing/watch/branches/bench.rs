//! What the branch set costs, on the two paths that touch it at scale.
//!
//! `#[ignore]`d: it prints numbers over synthetic path sets rather than
//! asserting. Nothing here touches the disk or a writer — the set is pure
//! in-memory bookkeeping, so the measurement is the bookkeeping and nothing else.
//!
//! Two arms, matching the two ways the set gets big:
//!
//! - **Registration**: one `begin_covering` over a whole frontier, then the
//!   matching `finish_covering`. This is what a phased index's group start pays,
//!   on the thread that asked for the walk, before any disk is read.
//! - **Admission**: `admit` per event against the set a mid-phase volume holds.
//!   This is the live hot path, once per filesystem event, under the set's lock.
//!
//! ```sh
//! cargo test -p cmdr-index --release --lib -- --ignored --nocapture branch_set_cost
//! ```
//!
//! Results and the call they back: `docs/notes/branch-set-cost-2026-08-15.md`.

use std::io::Write;
use std::time::{Duration, Instant};

use super::*;
use crate::indexing::watch::watcher::FsEventFlags;

/// Frontier widths to measure registration at. The real number that prompted
/// this was 2,503 roots on one cold-drive search; the rest bracket it so the
/// growth curve is visible rather than inferred from one point.
const WIDTHS: &[usize] = &[100, 500, 1_000, 2_500, 5_000];

/// How many events each admission arm pushes through `admit`. A churn burst on a
/// build directory reaches this in a second or two.
const CHURN_EVENTS: usize = 20_000;

#[test]
#[ignore = "benchmark over synthetic path sets; run manually with --nocapture"]
fn branch_set_cost() {
    let mut out = std::io::stderr();

    let _ = writeln!(&mut out, "\n── registering a frontier ──");
    let _ = writeln!(
        &mut out,
        "{:>7}  {:>12}  {:>12}  {:>12}  {:>10}",
        "roots", "begin", "finish", "total", "µs/root"
    );
    for &width in WIDTHS {
        let paths = frontier(width);
        let watch = BranchWatch::with_branches(Vec::new());

        let start = Instant::now();
        watch.begin_covering(&paths);
        let begin = start.elapsed();

        let start = Instant::now();
        watch.finish_covering(&paths, AfterWalk::Watch);
        let finish = start.elapsed();

        let total = begin + finish;
        let _ = writeln!(
            &mut out,
            "{width:>7}  {begin:>12.2?}  {finish:>12.2?}  {total:>12.2?}  {:>10.1}",
            total.as_secs_f64() * 1e6 / width as f64,
        );
    }

    let _ = writeln!(&mut out, "\n── admitting {CHURN_EVENTS} events ──");
    let _ = writeln!(
        &mut out,
        "{:>9}  {:>12}  {:>10}  {:>28}",
        "branches", "elapsed", "µs/event", "events/s"
    );
    for &width in WIDTHS {
        let (elapsed, admitted) = admission_cost(width);
        let _ = writeln!(
            &mut out,
            "{width:>9}  {elapsed:>12.2?}  {:>10.2}  {:>28.0}",
            elapsed.as_secs_f64() * 1e6 / CHURN_EVENTS as f64,
            CHURN_EVENTS as f64 / elapsed.as_secs_f64(),
        );
        assert_eq!(admitted, CHURN_EVENTS, "every event should have been processed");
    }
}

/// One `admit` per event against a settled set of `width` branches, with the
/// events spread evenly over them: the shape a churn burst has once a phase's
/// walks have finished and the volume is being kept current.
fn admission_cost(width: usize) -> (Duration, usize) {
    let paths = frontier(width);
    let watch = BranchWatch::with_branches(Vec::new());
    watch.begin_covering(&paths);
    watch.finish_covering(&paths, AfterWalk::Watch);

    // Built up front so the loop times admission and not string formatting.
    let events: Vec<FsChangeEvent> = (0..CHURN_EVENTS)
        .map(|i| FsChangeEvent {
            path: format!("{}/churn-{i}.o", paths[i % paths.len()]),
            event_id: i as u64,
            flags: FsEventFlags {
                item_modified: true,
                item_is_file: true,
                ..FsEventFlags::default()
            },
        })
        .collect();

    let mut admitted = 0;
    let start = Instant::now();
    for event in events {
        if let Admission::Process(events) = watch.admit(event, Reach::CoveredBranches) {
            admitted += events.len();
        }
    }
    (start.elapsed(), admitted)
}

/// `count` disjoint frontier roots, deep and sharing long prefixes: the shape a
/// resumed phased index leaves behind, where what's left to walk is a scatter of
/// small directories far down one user's tree. Long shared prefixes are the
/// honest case for a set that compares paths component by component.
fn frontier(count: usize) -> Vec<String> {
    (0..count)
        .map(|i| {
            format!(
                "/Users/someone/Library/Application Support/project-{}/node_modules/package-{}/dist/chunk-{i}",
                i % 32,
                i % 64,
            )
        })
        .collect()
}
