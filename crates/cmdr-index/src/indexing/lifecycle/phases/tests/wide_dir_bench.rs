//! What ONE very wide directory costs the first index.
//!
//! `#[ignore]`d: it builds a synthetic tree per width and drives the REAL machine
//! over it, which takes minutes and prints numbers rather than asserting.
//!
//! The question is the scaling SHAPE. A photo dump, a Maildir, or a downloads
//! folder really does hold tens of thousands of children in one directory, so a
//! first index that grows faster than the width is a "Cmdr never finishes" report
//! waiting to happen. Each row separates the three things a run spends time on:
//!
//! - **build**: making the tree, which is the fixture's cost and ❌ not the
//!   product's.
//! - **machine**: `start_volume` to the machine reporting no work, which IS the
//!   product's.
//! - **tail**: how much of the machine's time came AFTER the walk stopped
//!   counting entries, which is what separates "the walk is slow" from "something
//!   after the walk is slow".
//!
//! ```sh
//! CMDR_PHASES_TEST_TREE_DIR=/private/tmp CMDR_WIDE_DIR_BENCH_WIDTHS=12000,20000,30000 \
//!   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
//!   indexing::lifecycle::phases::tests::wide_dir_bench::wide_dir_cost
//! ```
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

/// What it costs to RESUME a first index that stopped part way through the wide
/// directory.
///
/// This is the shape the uninterrupted run above never reaches, and the one a
/// real user gets for free: quit mid-index, open a folder while the machine is
/// walking (preemption stops the group on purpose), or let a search take the
/// ground. The wide directory itself is listed, its children are not, so the
/// frontier stops being one root and becomes one root PER unwalked child — all of
/// them sharing a parent that has `width` children.
///
/// ```sh
/// CMDR_PHASES_TEST_TREE_DIR=/private/tmp CMDR_WIDE_DIR_BENCH_WIDTHS=4000,8000,16000 \
///   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
///   indexing::lifecycle::phases::tests::wide_dir_bench::wide_dir_resume_cost
/// ```
#[test]
#[ignore = "benchmark over a synthetic tree; run manually with --nocapture"]
fn wide_dir_resume_cost() {
    let mut out = std::io::stderr();
    let shape = Shape::from_env();
    let _ = writeln!(
        &mut out,
        "\n── resuming a first index stopped inside one directory of N {} ──\n\
         {:>8}  {:>10}  {:>10}  {:>10}  {:>9}",
        shape.label(),
        "width",
        "frontier",
        "resume",
        "entries",
        "finished"
    );
    for width in widths() {
        let run = one_resume(width, shape);
        let _ = writeln!(
            &mut out,
            "{width:>8}  {:>10}  {:>10}  {:>10}  {:>9}",
            run.frontier,
            format!("{:.1?}", run.resumed),
            run.entries,
            run.finished,
        );
    }
}

/// One width's interrupted run and what resuming it cost.
struct Resume {
    /// How many frontier roots the interruption left. One is the healthy answer;
    /// anything near `width` is the shape this bench exists to catch.
    frontier: usize,
    /// The relaunch, from `start_volume` to "no work left" or the patience budget.
    resumed: Duration,
    entries: u64,
    /// Whether it actually got there, rather than running out of patience.
    finished: bool,
}

fn one_resume(width: usize, shape: Shape) -> Resume {
    let drive = Drive::assembled(
        "wide-dir-resume-bench",
        |root| build_wide(&root.join("big"), width, shape),
        |_, _| {},
        &[],
        true,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()) as std::sync::Arc<dyn EventSink>,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()),
        crate::indexing::host::policy::FakeHostPolicy::shared(),
    );

    // Stop the machine once it is well inside the wide directory but nowhere near
    // done, which is where a quit or a preemption lands.
    drive.start();
    let stop_after = (width / 5).max(1) as u64;
    let deadline = Instant::now() + patience();
    while Instant::now() < deadline {
        match drive.index.status(drive.volume_id) {
            Ok(status) if status.entries_scanned >= stop_after || !status.scanning => break,
            Ok(_) => {}
            Err(_) => break,
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    drive.stop();
    let frontier = frontier_size(&drive);

    let started = Instant::now();
    drive.start();
    let (finished, _) = wait_out(&drive, started, width);
    Resume {
        frontier,
        resumed: started.elapsed(),
        entries: drive.entry_count(),
        finished,
    }
}

/// How many frontier roots the volume root still names, read straight off the
/// database so it can be asked with the volume stopped.
fn frontier_size(drive: &Drive) -> usize {
    let Ok(conn) = IndexStore::open_read_connection(&drive.db_path()) else {
        return 0;
    };
    let root = drive.path("");
    let space = IndexPathSpace::mount_rooted(root.clone());
    let Some(index_path) = space.index_relative(&root) else {
        return 0;
    };
    coverage_for_scope(&conn, &index_path, &root, CoverageDimension::Listing)
        .map(|map| map.frontier.len())
        .unwrap_or(0)
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
