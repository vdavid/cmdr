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

use cmdr_fs::pluralize::pluralize;

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
        let watch = BranchWatch::default();

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

    for landing in [Landing::InsideABranch, Landing::OutsideEveryBranch] {
        let _ = writeln!(
            &mut out,
            "\n── admitting {} {} ──",
            pluralize(CHURN_EVENTS as u64, "event"),
            landing.describe()
        );
        let _ = writeln!(
            &mut out,
            "{:>9}  {:>12}  {:>10}  {:>28}",
            "branches", "elapsed", "µs/event", "events/s"
        );
        for &width in WIDTHS {
            let elapsed = admission_cost(width, landing);
            let _ = writeln!(
                &mut out,
                "{width:>9}  {elapsed:>12.2?}  {:>10.2}  {:>28.0}",
                elapsed.as_secs_f64() * 1e6 / CHURN_EVENTS as f64,
                CHURN_EVENTS as f64 / elapsed.as_secs_f64(),
            );
        }
    }
}

/// Where a churn burst's events land relative to the covered ground. Both are
/// the live hot path, and on a branch-watched volume the second is the common
/// one: most of a drive is ground no walk went near.
#[derive(Clone, Copy)]
enum Landing {
    InsideABranch,
    OutsideEveryBranch,
}

impl Landing {
    fn describe(self) -> &'static str {
        match self {
            Self::InsideABranch => "inside the branches",
            Self::OutsideEveryBranch => "outside every branch",
        }
    }
}

/// One `admit` per event against a settled set of `width` branches: the shape a
/// churn burst has once a phase's walks have finished and the volume is being
/// kept current.
fn admission_cost(width: usize, landing: Landing) -> Duration {
    let paths = frontier(width);
    let watch = BranchWatch::default();
    watch.begin_covering(&paths);
    watch.finish_covering(&paths, AfterWalk::Watch);

    // Built up front so the loop times admission and not string formatting.
    let events: Vec<FsChangeEvent> = (0..CHURN_EVENTS)
        .map(|i| {
            let inside = &paths[i % paths.len()];
            let path = match landing {
                Landing::InsideABranch => format!("{inside}/churn-{i}.o"),
                // Same depth and the same long shared prefix, one component off
                // the covered ground.
                Landing::OutsideEveryBranch => inside.replace("/dist/", "/untouched/"),
            };
            FsChangeEvent {
                path,
                event_id: i as u64,
                flags: FsEventFlags {
                    item_modified: true,
                    item_is_file: true,
                    ..FsEventFlags::default()
                },
            }
        })
        .collect();

    let expected = match landing {
        Landing::InsideABranch => Admission::Process(Vec::new()),
        Landing::OutsideEveryBranch => Admission::Discarded,
    };
    let mut seen = 0;
    let start = Instant::now();
    for event in events {
        if std::mem::discriminant(&watch.admit(event, Reach::CoveredBranches)) == std::mem::discriminant(&expected) {
            seen += 1;
        }
    }
    let elapsed = start.elapsed();
    assert_eq!(
        seen, CHURN_EVENTS,
        "every event should have taken the arm being measured"
    );
    elapsed
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
