//! What stopping a walk costs, and what it buys the person who opened a folder.
//!
//! `#[ignore]`d: it builds a synthetic tree and drives the REAL machine over it,
//! which takes a minute or two and prints numbers rather than asserting.
//!
//! Two arms, for the two numbers preemption owes:
//!
//! - **Handing ground over**: a real cover walk over a big frontier root, stopped
//!   part way, timed from the ask to the moment `finish()` returns. This is the
//!   floor under preemption whatever else changes, because the machine starts the
//!   next walk only after the previous one joins — the second of the two reasons
//!   preemption was ruled out, and the one atomic handoff does NOT fix.
//! - **Time to index a folder somebody opened**: how long a folder opened just as
//!   a big sibling's walk begins waits before it is covered. The "before" number
//!   is the same tree walked with nobody opening anything, because that IS what
//!   the folder used to wait for: the machine consulted its visit queue only
//!   between walks, so the wait was the rest of the sibling plus the folder's own
//!   walk.
//!
//! ```sh
//! CMDR_PHASES_TEST_TREE_DIR=/private/tmp \
//!   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
//!   indexing::lifecycle::phases::tests::preemption_bench::preemption_cost
//! ```
//!
//! Results and the call they back: `docs/notes/preemption-2026-08-18.md`.

use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use super::*;
use crate::indexing::events::{EventSink, IndexEvent};

/// How many directories the big sibling holds. Big enough that walking it takes
/// seconds — which is the wart being measured — and small enough to build.
///
/// ⚠️ The run builds THREE trees this size and building dominates it: 60,000
/// directories took about 35 minutes each on APFS under load, and the machine
/// then needed longer than the 600 s patience below to cover one. Raise it for a
/// headline number, and budget hours rather than minutes.
fn dir_budget() -> usize {
    std::env::var("CMDR_PREEMPTION_BENCH_DIRS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(12_000)
}

/// How many times the handover arm stops a walk. Each round needs a frontier root
/// of its own: a root walked once isn't virgin ground the second time, and the
/// repair path would be measuring something else.
const HANDOVER_ROUNDS: usize = 5;

/// How many entries a walk delivers before it is asked to stop, so the ask lands
/// with the walker deep in parallel work rather than still spinning up.
const WARMUP_ENTRIES: usize = 4_000;

/// How long the bench waits on the machine before giving up on it.
const PATIENCE: Duration = Duration::from_secs(600);

#[test]
#[ignore = "benchmark over a synthetic tree; run manually with --nocapture"]
fn preemption_cost() {
    let mut out = std::io::stderr();
    let dirs = dir_budget();

    let handovers = how_long_a_walk_takes_to_hand_its_ground_over(dirs);
    let _ = writeln!(
        &mut out,
        "\n── handing ground over ({dirs} dirs, {HANDOVER_ROUNDS} rounds) ──"
    );
    for (round, took) in handovers.iter().enumerate() {
        let _ = writeln!(&mut out, "  round {}: {took:.2?}", round + 1);
    }
    let worst = handovers.iter().max().copied().unwrap_or_default();
    let total: Duration = handovers.iter().sum();
    let _ = writeln!(
        &mut out,
        "  median {:.2?}, worst {worst:.2?}",
        median(&mut handovers.clone()),
    );
    let _ = writeln!(&mut out, "  (total {total:.2?} over {} rounds)", handovers.len());

    let before = how_long_the_big_sibling_takes(dirs);
    let after = how_long_a_folder_somebody_opens_waits(dirs);
    let _ = writeln!(&mut out, "\n── time to index a folder somebody opened ({dirs} dirs) ──");
    let _ = writeln!(&mut out, "  the big sibling's own walk: {:.2?}", before.sibling);
    let _ = writeln!(&mut out, "  the opened folder's own walk: {:.2?}", before.visited);
    let _ = writeln!(
        &mut out,
        "  before (waits out the sibling, then walks):     {:.2?}",
        before.sibling + before.visited
    );
    let _ = writeln!(&mut out, "  after  (opened → covered, poll included): {after:.2?}");
}

/// The middle value, for a handful of rounds where a mean would follow the one
/// round that hit a page-cache miss.
fn median(rounds: &mut [Duration]) -> Duration {
    rounds.sort_unstable();
    rounds.get(rounds.len() / 2).copied().unwrap_or_default()
}

/// Stop a real walk part way and time it to the join, the way the machine does:
/// its own flush left to the caller, its batches drained, one frontier root per
/// round.
fn how_long_a_walk_takes_to_hand_its_ground_over(dirs: usize) -> Vec<Duration> {
    let fixture = Tree::new();
    let per_round = (dirs / HANDOVER_ROUNDS).max(1);
    for round in 0..HANDOVER_ROUNDS {
        build_wide(&fixture.root().join(format!("big-{round}")), per_round);
    }

    let mut rounds = Vec::new();
    for round in 0..HANDOVER_ROUNDS {
        let cancel = CancellationToken::new();
        let walk = cover::start(
            fixture.context().leaving_the_flush_to_the_caller(),
            vec![fixture.path(&format!("big-{round}"))],
            CoverageDimension::Listing,
            cancel.clone(),
            cover::WalkFor::TheIndex,
        );
        let mut seen = 0;
        while let Some(batch) = walk.next_batch() {
            seen += batch.len();
            if seen >= WARMUP_ENTRIES {
                break;
            }
        }
        let asked = Instant::now();
        cancel.cancel();
        while walk.next_batch().is_some() {}
        let outcome = walk.finish();
        rounds.push(asked.elapsed());
        assert!(outcome.cancelled, "the round measured a walk that was actually stopped");
    }
    rounds
}

/// A directory holding `dirs` children, each with one file, so a walk of it is
/// wide rather than deep and takes real time.
fn build_wide(root: &Path, dirs: usize) {
    for index in 0..dirs {
        let dir = root.join(format!("sub-{index:06}"));
        std::fs::create_dir_all(&dir).expect("dirs");
        std::fs::write(dir.join("leaf.txt"), "x").expect("file");
    }
}

/// What the two roots cost on their own, with nobody opening anything: the
/// "before" number is the sum, because the machine used to consult its visit
/// queue only between walks.
struct WalkedAlone {
    sibling: Duration,
    visited: Duration,
}

fn how_long_the_big_sibling_takes(dirs: usize) -> WalkedAlone {
    let timeline = std::sync::Arc::new(Timeline::new("preemption-bench-before"));
    let drive = Drive::assembled(
        "preemption-bench-before",
        |root| {
            build_wide(&root.join("big"), dirs);
            std::fs::create_dir_all(root.join("zzz-visited/inner")).expect("dirs");
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::clone(&timeline) as std::sync::Arc<dyn EventSink>,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()),
        crate::indexing::host::policy::FakeHostPolicy::shared(),
    );
    drive.start();
    wait_for(&drive);
    WalkedAlone {
        sibling: timeline.branch_time(&drive.path("big")),
        visited: timeline.branch_time(&drive.path("zzz-visited")),
    }
}

/// From the moment somebody opens the folder to the moment its walk ends.
///
/// The folder is opened the instant the big sibling's walk is announced, which is
/// the worst case: the whole of that walk is still ahead. The poll that carries it
/// to the machine runs on the reporter's 500 ms tick, and that latency is INSIDE
/// this number — it is what the person actually waits.
fn how_long_a_folder_somebody_opens_waits(dirs: usize) -> Duration {
    let host = crate::indexing::host::policy::FakeHostPolicy::shared();
    let timeline = std::sync::Arc::new(
        Timeline::new("preemption-bench-after").opening("zzz-visited", std::sync::Arc::clone(&host)),
    );
    let drive = Drive::assembled(
        "preemption-bench-after",
        |root| {
            build_wide(&root.join("big"), dirs);
            std::fs::create_dir_all(root.join("zzz-visited/inner")).expect("dirs");
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::clone(&timeline) as std::sync::Arc<dyn EventSink>,
        std::sync::Arc::new(crate::indexing::events::RecordingSink::new()),
        host,
    );
    drive.start();
    wait_for(&drive);
    timeline.wait_for_the_opened_folder(&drive.path("zzz-visited"))
}

fn wait_for(drive: &Drive) {
    cmdr_fs::testing::wait_until(PATIENCE, "the phases to finish", || {
        !drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning)
    });
}

/// When each walk started and ended, and (optionally) when a folder was opened.
///
/// A sink rather than a reader of `RecordingSink`, because what this arm measures
/// is WHEN, and the recorder keeps only what.
struct Timeline {
    volume_id: &'static str,
    branches: std::sync::Mutex<Vec<Branch>>,
    /// The folder to open the moment the first walk is announced, and the host to
    /// tell about it.
    opens: Option<(
        &'static str,
        std::sync::Arc<crate::indexing::host::policy::FakeHostPolicy>,
    )>,
    opened_at: std::sync::Mutex<Option<Instant>>,
    open_done: AtomicBool,
}

/// One walk, from its announcement to its end. `ended` stays `None` while it runs.
struct Branch {
    roots: Vec<String>,
    started: Instant,
    ended: Option<Instant>,
}

impl Timeline {
    fn new(volume_id: &'static str) -> Self {
        Self {
            volume_id,
            branches: std::sync::Mutex::new(Vec::new()),
            opens: None,
            opened_at: std::sync::Mutex::new(None),
            open_done: AtomicBool::new(false),
        }
    }

    fn opening(
        mut self,
        folder: &'static str,
        host: std::sync::Arc<crate::indexing::host::policy::FakeHostPolicy>,
    ) -> Self {
        self.opens = Some((folder, host));
        self
    }

    /// How long the walk over `root` took, start to end.
    fn branch_time(&self, root: &str) -> Duration {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.branches
            .lock_ignore_poison()
            .iter()
            .find(|branch| branch.ended.is_some() && branch.roots.iter().any(|walked| walked == root))
            .and_then(|branch| branch.ended.map(|end| end.duration_since(branch.started)))
            .unwrap_or_default()
    }

    /// From opening the folder to the end of the walk that covered it.
    fn wait_for_the_opened_folder(&self, root: &str) -> Duration {
        use cmdr_fs::ignore_poison::IgnorePoison;
        let opened = self.opened_at.lock_ignore_poison().expect("the folder was opened");
        self.branches
            .lock_ignore_poison()
            .iter()
            .find(|branch| branch.ended.is_some() && branch.roots.iter().any(|walked| walked.starts_with(root)))
            .and_then(|branch| branch.ended.map(|end| end.duration_since(opened)))
            .unwrap_or_default()
    }
}

impl EventSink for Timeline {
    fn emit(&self, event: IndexEvent) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        match event {
            IndexEvent::CoverageBranchStarted { volume_id, roots } if volume_id == self.volume_id => {
                self.branches.lock_ignore_poison().push(Branch {
                    roots: roots.clone(),
                    started: Instant::now(),
                    ended: None,
                });
                if let Some((folder, host)) = &self.opens
                    && !self.open_done.swap(true, Ordering::Relaxed)
                    && let Some(first) = roots.first()
                {
                    let opened = Path::new(first)
                        .parent()
                        .expect("a frontier root sits under the tree")
                        .join(folder);
                    *self.opened_at.lock_ignore_poison() = Some(Instant::now());
                    host.note_open_listing(self.volume_id, opened);
                }
            }
            IndexEvent::CoverageBranchEnded { volume_id, roots } if volume_id == self.volume_id => {
                let mut branches = self.branches.lock_ignore_poison();
                if let Some(open) = branches
                    .iter_mut()
                    .rev()
                    .find(|branch| branch.ended.is_none() && branch.roots == roots)
                {
                    open.ended = Some(Instant::now());
                }
            }
            _ => {}
        }
    }
}
