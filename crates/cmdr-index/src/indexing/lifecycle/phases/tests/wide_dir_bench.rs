//! What ONE very wide directory costs the first index.
//!
//! `#[ignore]`d: both arms build a synthetic tree per width and print numbers
//! rather than asserting.
//!
//! The question is the scaling SHAPE. A photo dump, a Maildir, or a downloads
//! folder really does hold tens of thousands of children in one directory, so a
//! first index that grows faster than the width is a "Cmdr never finishes"
//! report waiting to happen. Two arms, because the answer differs:
//!
//! - [`wide_dir_cost`] drives the REAL machine over the directory, uninterrupted.
//!   It splits the run into **build** (the fixture's cost, ❌ not the product's),
//!   **machine** (`start_volume` to no work left, which IS the product's),
//!   **walk** (up to the last moment the entry counter moved), and **tail**
//!   (everything after that) — so "the walk is slow" and "everything after the
//!   walk is slow" stop looking alike.
//! - [`wide_dir_rollup_cost`] covers the same ground with the directory already
//!   listed, which is what a stopped walk leaves. That is where the cost is.
//!
//! Results and the call they back: `docs/notes/wide-dir-scaling-2026-08-18.md`.

use std::io::Write;
use std::time::{Duration, Instant};

use super::*;
use crate::indexing::events::EventSink;

/// The widths to measure, smallest first.
fn widths() -> Vec<usize> {
    std::env::var("CMDR_WIDE_DIR_BENCH_WIDTHS")
        .ok()
        .map(|raw| raw.split(',').filter_map(|part| part.trim().parse().ok()).collect())
        .unwrap_or_else(|| vec![12_000, 20_000, 30_000, 40_000, 60_000])
}

/// What the wide directory is full of.
///
/// Both shapes exist because they cost different things: a pile of FILES is the
/// one a photo dump or a Maildir actually makes, and it puts every child in one
/// directory's row batch; a pile of DIRECTORIES is what the walker has to
/// enumerate, and it is the shape the preemption fixture builds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    /// One directory holding N subdirectories, each with one file in it.
    Subdirs,
    /// One directory holding N plain files.
    Files,
}

impl Shape {
    fn from_env() -> Self {
        match std::env::var("CMDR_WIDE_DIR_BENCH_SHAPE").as_deref() {
            Ok("files") => Self::Files,
            _ => Self::Subdirs,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Subdirs => "subdirs",
            Self::Files => "files",
        }
    }
}

/// How long one width's run may take before the bench gives up on it. Long on
/// purpose: the question this bench exists to answer is whether a run that blows
/// a 600 s budget is wedged or merely slow, and only a run allowed to finish
/// answers it.
fn patience() -> Duration {
    Duration::from_secs(
        std::env::var("CMDR_WIDE_DIR_BENCH_PATIENCE_S")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(1_800),
    )
}

#[test]
#[ignore = "benchmark over a synthetic tree; run manually with --nocapture"]
fn wide_dir_cost() {
    let mut out = std::io::stderr();
    let shape = Shape::from_env();
    let _ = writeln!(
        &mut out,
        "\n── a first index over one directory of N {} ──\n\
         {:>8}  {:>10}  {:>10}  {:>10}  {:>10}  {:>9}",
        shape.label(),
        "width",
        "build",
        "machine",
        "walk",
        "tail",
        "entries"
    );
    for width in widths() {
        let run = one_width(width, shape);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {:>10}  {:>10}  {:>10}  {:>10}  {:>9}",
            format!("{:.1?}", run.built),
            format!("{:.1?}", run.machine),
            format!("{:.1?}", run.walk),
            format!("{:.1?}", run.tail),
            run.entries,
        );
    }
}

/// What it costs to cover a wide directory's children ONE FRONTIER ROOT AT A
/// TIME, against covering the same ground in one walk.
///
/// That is not an exotic state: the moment anything stops the walk of a wide
/// directory after it has been listed — a quit, a search taking the ground, or
/// the machine stopping the group for a folder somebody opened — its frontier
/// stops being one root and becomes one root per unwalked child. Every one of
/// those roots ends its walk with a `ComputeSubtreeAggregates`, whose handler
/// rolls the ancestor chain up from the child's parent: the wide directory
/// itself, recomputed from all `width` of its children, once per root.
///
/// The ancestor roll-up is coalesced per burst now, so the piecemeal arm pays a
/// handful of ancestor walks rather than one per root and the ratio stays flat as
/// the width grows (`writer/pending_rollups.rs`). The ratio column is the finding;
/// ❌ read it rather than the absolute times, which move with machine load.
///
/// ⚠️ Both arms walk the way the PHASE MACHINE walks — the drain left to the
/// caller, settled once at the end — and ❌ not with a blocking flush per root.
/// A flush per root stops the walker and the writer overlapping at all, which is
/// exactly the state where nothing can coalesce, so measuring that way would
/// report a quadratic the machine doesn't have.
///
/// ```sh
/// CMDR_PHASES_TEST_TREE_DIR=/private/tmp CMDR_WIDE_DIR_BENCH_WIDTHS=500,1000,2000,4000 \
///   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
///   indexing::lifecycle::phases::tests::wide_dir_bench::wide_dir_rollup_cost
/// ```
#[test]
#[ignore = "benchmark over a synthetic tree; run manually with --nocapture"]
fn wide_dir_rollup_cost() {
    let mut out = std::io::stderr();
    let _ = writeln!(
        &mut out,
        "\n── covering a wide directory whole, against one child at a time ──\n\
         {:>8}  {:>12}  {:>14}  {:>10}  {:>12}",
        "width", "one walk", "per child", "ratio", "per root"
    );
    for width in widths() {
        let whole = cover_the_wide_directory_whole(width);
        let piecemeal = cover_the_wide_directory_child_by_child(width);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {:>12}  {:>14}  {:>10}  {:>12}",
            format!("{whole:.1?}"),
            format!("{piecemeal:.1?}"),
            format!(
                "{:.0}x",
                piecemeal.as_secs_f64() / whole.as_secs_f64().max(f64::MIN_POSITIVE)
            ),
            format!("{:.2?}", piecemeal / width.max(1) as u32),
        );
    }
}

/// The uninterrupted shape: the wide directory is one frontier root, so one walk
/// covers it and one ancestor roll-up follows.
fn cover_the_wide_directory_whole(width: usize) -> Duration {
    let fixture = Tree::new();
    build_wide(&fixture.root().join("big"), width, Shape::Subdirs);
    let started = Instant::now();
    fixture.cover_leaving_the_drain_to_the_caller(&fixture.path("big"));
    fixture.settle();
    started.elapsed()
}

/// The interrupted shape: the wide directory is already listed, so every one of
/// its children is a frontier root of its own.
fn cover_the_wide_directory_child_by_child(width: usize) -> Duration {
    let fixture = Tree::new();
    build_wide(&fixture.root().join("big"), width, Shape::Subdirs);
    // Exactly what a stopped walk leaves: the wide directory listed, every child
    // of it a row nothing has walked.
    stitch::down_to(&fixture.space, &fixture.writer, Path::new(&fixture.path("big")));
    let roots = fixture.frontier(&fixture.path("big"));
    assert_eq!(roots.len(), width, "the stitch leaves every child on the frontier");

    let started = Instant::now();
    for root in &roots {
        fixture.cover_leaving_the_drain_to_the_caller(root);
    }
    // The roll-ups the burst queued are part of what covering this ground costs, so
    // the timed region ends where the writer is genuinely done, ❌ not where the
    // last walk returned.
    fixture.settle();
    started.elapsed()
}

/// One width's run, split into the parts that tell different stories.
struct Run {
    /// Making the tree. The fixture's cost, reported so it can be subtracted.
    built: Duration,
    /// `start_volume` to "no work left". The product's cost.
    machine: Duration,
    /// Up to the last moment the entry counter moved.
    walk: Duration,
    /// From there to the end: everything the machine does once the disk reading
    /// has stopped delivering rows.
    tail: Duration,
    /// What the index holds at the end.
    entries: u64,
}

fn one_width(width: usize, shape: Shape) -> Run {
    let built = std::sync::Mutex::new(Duration::ZERO);
    let drive = Drive::assembled(
        "wide-dir-bench",
        |root| {
            let started = Instant::now();
            build_wide(&root.join("big"), width, shape);
            use cmdr_fs::ignore_poison::IgnorePoison;
            *built.lock_ignore_poison() = started.elapsed();
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()) as std::sync::Arc<dyn EventSink>,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()),
        crate::indexing::host::policy::FakeHostPolicy::shared(),
    );

    let started = Instant::now();
    drive.start();
    let (_, last_moved) = wait_out(&drive, started, width);
    let machine = started.elapsed();

    use cmdr_fs::ignore_poison::IgnorePoison;
    Run {
        built: *built.lock_ignore_poison(),
        machine,
        walk: last_moved.duration_since(started),
        tail: machine.saturating_sub(last_moved.duration_since(started)),
        entries: drive.entry_count(),
    }
}

/// Poll the volume until the machine reports it has no work left, or the patience
/// budget runs out. Reports whether it got there, and the last moment the entry
/// counter moved — which is what separates a slow walk from a slow everything-
/// else.
fn wait_out(drive: &Drive, started: Instant, width: usize) -> (bool, Instant) {
    let mut last_moved = started;
    let mut last_count = 0;
    let mut next_report = Instant::now() + Duration::from_secs(10);
    let patience = patience();
    while started.elapsed() < patience {
        let Ok(status) = drive.index.status(drive.volume_id) else {
            return (false, last_moved);
        };
        if !status.scanning {
            return (true, last_moved);
        }
        if status.entries_scanned != last_count {
            last_count = status.entries_scanned;
            last_moved = Instant::now();
        }
        // A live trace, so a run that is going to blow the budget says WHERE it
        // stopped rather than only that it did.
        if Instant::now() >= next_report {
            let mut out = std::io::stderr();
            let _ = writeln!(
                &mut out,
                "    {width} @ {:.0?}: {} entries, phase {:?}, roots {:?}, last moved {:.0?} ago",
                started.elapsed(),
                status.entries_scanned,
                status.coverage_phase,
                status.walked_roots,
                last_moved.elapsed(),
            );
            next_report = Instant::now() + Duration::from_secs(10);
        }
        // allowed-test-sleep: the poll interval of a bench that has to outlive
        // `wait_until`'s panic-on-timeout — the whole question is what a run that
        // blows a budget does next, and it traces where it got while it waits.
        std::thread::sleep(Duration::from_millis(50));
    }
    (false, last_moved)
}

/// A directory holding `width` children, so the tree is wide rather than deep:
/// exactly one directory carries the whole width.
fn build_wide(root: &Path, width: usize, shape: Shape) {
    std::fs::create_dir_all(root).expect("the wide directory");
    for index in 0..width {
        match shape {
            Shape::Subdirs => {
                let dir = root.join(format!("sub-{index:06}"));
                std::fs::create_dir_all(&dir).expect("dirs");
                std::fs::write(dir.join("leaf.txt"), "x").expect("file");
            }
            Shape::Files => std::fs::write(root.join(format!("file-{index:06}.jpg")), "x").expect("file"),
        }
    }
}
