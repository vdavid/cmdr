//! Whether a tree somebody is writing to can keep a volume's first index from
//! ever saying it is done.
//!
//! `#[ignore]`d: it drives the REAL machine over a six-figure temp tree several
//! times while a thread writes into it, which takes minutes and prints numbers
//! rather than asserting.
//!
//! Completion is derived ("the frontier under the volume root is empty"), which is
//! immune to churn in the sense that matters — it can never claim ground nobody
//! walked — but says nothing about whether the frontier can be emptied at all while
//! somebody keeps adding to it. This bench asks that at four rates and one burst,
//! and then asks the two follow-ups a user cares about: does relaunching help while
//! the writing continues, and does the drive settle once it stops?
//!
//! Every arm writes into one folder that sorts first, so the walk covers and
//! watches it in the first group and everything after that lands on ground the
//! index already claims — a build directory on a real disk, and the only case where
//! churn can affect coverage at all.
//!
//! ```sh
//! CMDR_PHASES_TEST_TREE_DIR=/private/tmp \
//!   cargo test -p cmdr-index --release --lib -- --ignored --nocapture --exact \
//!   indexing::lifecycle::phases::tests::churn_bench::churn_against_completion
//! ```
//!
//! Results and the call they back:
//! `docs/notes/churn-against-completion-2026-08-15.md`.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::resume_bench::build_tree;
use super::*;

/// How many directories the underlying tree holds.
///
/// ⚠️ Big enough that a launch takes **tens of seconds**, and that is the whole
/// parameter. The live watcher is what turns a new folder into a row, and FSEvents
/// coalesces on its own latency: over a tree small enough to cover in a tenth of a
/// second, not one churn event is delivered before the machine stops, every arm
/// completes, and the bench reports that churn is harmless. 300,000 directories is
/// roughly 600,000 entries and ~15 s a launch on David's machine.
fn dir_budget() -> usize {
    std::env::var("CMDR_CHURN_BENCH_DIRS")
        .ok()
        .and_then(|raw| raw.parse().ok())
        .unwrap_or(300_000)
}

/// How long an arm waits for the machine to stop before giving up on it.
const PATIENCE: Duration = Duration::from_secs(600);

/// How many times each rate builds the index from nothing. Completion under churn
/// is a race, so one trial reports a coin toss as a law.
const TRIALS: usize = 3;

/// The cap on those trials when the rate is one that sometimes wins the race. An
/// arm keeps making first indexes past [`TRIALS`] until one of them ends unmarked,
/// because that state is the one worth interrogating — and the extra trials sharpen
/// the rate they are counted into rather than costing anything.
const TRIALS_MAX: usize = 6;

/// How many times an arm relaunches a volume that didn't finish, before calling it
/// a drive that won't settle while the writing lasts.
const RELAUNCHES: usize = 3;

/// The folder the churn lands in. It sorts ahead of `build_tree`'s `dir-N`, so the
/// walk covers it in the first group and everything written into it afterwards
/// lands on ground the index already claims — which is what a build directory on a
/// real disk is, and the only case where churn can affect coverage at all.
const CHURN_DIR: &str = "aaa-build";

/// How many files go in each directory the churn creates. A build writes a handful
/// of artifacts per unit; the count barely matters here, because a FILE under a
/// listed directory is reconciled in place and never becomes frontier.
const FILES_PER_CHURN_DIR: usize = 3;

#[test]
#[ignore = "benchmark over a six-figure synthetic tree; run manually with --nocapture"]
fn churn_against_completion() {
    let dirs = dir_budget();
    for shape in [
        ChurnShape::nothing(),
        ChurnShape::steady(20, "a compile: twenty new folders a second, all the way through"),
        ChurnShape::steady(60, "sixty new folders a second, all the way through"),
        ChurnShape::steady(
            200,
            "a package manager unpacking: two hundred a second, all the way through",
        ),
        ChurnShape::burst(
            2_000,
            "a build kicking off near the end: 2,000 folders at once, then quiet",
        ),
    ] {
        one_arm(dirs, shape).print();
    }
}

// ── One arm ──────────────────────────────────────────────────────────

/// What an arm writes into the tree while the machine covers it.
///
/// Two shapes, because the difference between them turned out to be the answer: a
/// finite burst, however big, is mopped up by the pass that follows it, and only
/// ground still ARRIVING during that pass survives it.
#[derive(Clone, Copy)]
enum Writing {
    Nothing,
    /// New folders at a fixed rate, for as long as the machine runs.
    Steady {
        dirs_per_second: u64,
    },
    /// One burst as fast as the disk takes it, deliberately timed to land while the
    /// machine is finishing, and then nothing.
    Burst {
        after: Duration,
        dirs: u64,
    },
}

#[derive(Clone, Copy)]
struct ChurnShape {
    writing: Writing,
    label: &'static str,
}

impl ChurnShape {
    fn nothing() -> Self {
        Self {
            writing: Writing::Nothing,
            label: "nobody is writing to the drive",
        }
    }

    fn steady(dirs_per_second: u64, label: &'static str) -> Self {
        Self {
            writing: Writing::Steady { dirs_per_second },
            label,
        }
    }

    /// A burst that starts once the machine is most of the way through its walk.
    /// Earlier than that and the first pass simply walks it as part of the tree,
    /// which measures nothing.
    fn burst(dirs: u64, label: &'static str) -> Self {
        Self {
            writing: Writing::Burst {
                after: Duration::from_secs(12),
                dirs,
            },
            label,
        }
    }
}

/// Cover the same tree from scratch a few times with somebody writing into it,
/// then keep relaunching, then let the writing stop.
///
/// Three questions, and only the last two would be bugs: whether a first index can
/// miss ground that appeared under it (honest, and exactly what the pass budget
/// says happens), whether relaunching keeps missing it for as long as the writing
/// lasts (a drive that never settles), and whether it stays stuck after the writing
/// stops (a wedge).
///
/// Repeated rather than sampled once, because completion here is a RACE and a
/// single run reports a coin toss as a law: the volume is done if some stock-take
/// finds the frontier empty, and whether one does depends on where the watcher's
/// next batch of new folders falls against the walk.
fn one_arm(dirs: usize, shape: ChurnShape) -> Arm {
    let drive = Drive::new(
        "phased-churn-bench",
        |root| {
            build_tree(root, dirs);
            std::fs::create_dir_all(root.join(CHURN_DIR)).expect("the churn folder");
        },
        &[],
    );

    // The same tree, indexed from nothing each time: `forget_volume` drops the
    // database, which is what makes the next `start` a first index rather than a
    // resume. Rebuilding the tree per trial would cost half a minute apiece and
    // measure nothing new. It stops at a trial that ended unmarked, because that is
    // the state the relaunches below have to start from.
    let mut first_indexes = Vec::new();
    while first_indexes.len() < TRIALS_MAX {
        if !first_indexes.is_empty() {
            drive.index.forget_volume(drive.volume_id).expect("the index goes");
        }
        let take = one_launch(&drive, shape.writing);
        let stuck = !take.completed;
        first_indexes.push(take);
        if stuck || (first_indexes.len() >= TRIALS && first_indexes.iter().all(|take| take.completed)) {
            break;
        }
    }

    // Relaunches, with whatever was writing still writing: the thing a user does
    // when the badge won't settle.
    let mut relaunches = Vec::new();
    while relaunches.len() < RELAUNCHES && !last_take(&first_indexes, &relaunches).completed {
        drive.stop();
        relaunches.push(one_launch(&drive, shape.writing));
    }

    // Everybody stops typing. A drive that doesn't settle HERE is wedged.
    //
    // ❌ Skipped when the drive already settled: relaunching a COMPLETED volume
    // reconciles rather than covers, and its marker lands after the machine reports
    // idle, so this take would read a healthy drive as unmarked.
    let quiet = if last_take(&first_indexes, &relaunches).completed {
        None
    } else {
        drive.stop();
        Some(one_launch(&drive, Writing::Nothing))
    };

    Arm {
        shape,
        first_indexes,
        relaunches,
        quiet,
    }
}

/// The state the drive is in right now: the last relaunch, or the last first index
/// when nothing has relaunched yet.
fn last_take<'a>(first_indexes: &'a [Take], relaunches: &'a [Take]) -> &'a Take {
    relaunches
        .last()
        .or_else(|| first_indexes.last())
        .expect("an arm always makes at least one first index")
}

/// Start the volume with somebody writing into it, wait for the machine to stop,
/// and take down what it left.
///
/// The writing starts and stops WITH the launch, so a burst arm gets its burst on
/// every launch rather than only the first, and nothing is written into the tree
/// while the volume is down (which no user's machine would do either — the drive is
/// only interesting while it's being indexed).
fn one_launch(drive: &Drive, writing: Writing) -> Take {
    let churn = Churn::start(drive.tree.path().join(CHURN_DIR), writing);
    let started = Instant::now();
    drive.start();
    let ran_for = wait_for_the_machine_to_stop(drive, started);
    let written = churn.created();
    drop(churn);
    Take::of(drive, ran_for, written)
}

/// Wait for the machine to report it has nothing left to do, and say how long that
/// took. `None` means it was still working when the patience ran out.
///
/// ⚠️ Waiting on the MACHINE rather than on `scan_completed_at` is the whole point.
/// A machine that stops without the marker will never write it, so a bench that
/// polls only the marker (`home_bench`) can't tell that apart from a slow run and
/// sits out its whole patience — which is exactly how one slow-looking run got
/// recorded with no explanation.
fn wait_for_the_machine_to_stop(drive: &Drive, started: Instant) -> Option<Duration> {
    loop {
        if !drive.index.status(drive.volume_id).is_ok_and(|status| status.scanning) {
            return Some(started.elapsed());
        }
        if started.elapsed() > PATIENCE {
            return None;
        }
        // allowed-test-sleep: sampling a running machine's state over minutes, and
        // reporting a miss instead of panicking on one, which is what the
        // wait-on-one-condition helper does.
        std::thread::sleep(Duration::from_millis(100));
    }
}

// ── What an arm reports ──────────────────────────────────────────────

struct Arm {
    shape: ChurnShape,
    /// One per trial: the same tree indexed from nothing, under the same churn.
    first_indexes: Vec<Take>,
    /// Relaunches of the last trial's volume, with the writing still going. Empty
    /// when that trial finished on its own.
    relaunches: Vec<Take>,
    /// One last launch with nothing writing. `None` when the drive had already
    /// settled and there was nothing left to ask.
    quiet: Option<Take>,
}

/// What one launch ended with.
struct Take {
    /// How long until the machine reported itself idle. `None` if it never did.
    ran_for: Option<Duration>,
    completed: bool,
    entries: u64,
    /// The frontier the moment the machine stopped. A snapshot rather than a
    /// steady state: under churn it keeps growing after the read.
    frontier: Vec<String>,
    churn_dirs: u64,
}

impl Take {
    fn of(drive: &Drive, ran_for: Option<Duration>, churn_dirs: u64) -> Self {
        Self {
            ran_for,
            // ⚠️ Read the moment the machine stops, with no grace period, and that
            // is sound for exactly this path: `take_stock` stamps AND flushes
            // before `finish` flips the machine to idle, so a machine that has
            // stopped without the marker will never write one. A relaunch of an
            // already-COMPLETED volume is a different path (it reconciles rather
            // than covering) and would need one — which is why no arm here
            // relaunches a volume that finished.
            completed: drive.meta("scan_completed_at").is_some(),
            entries: drive.entry_count(),
            frontier: drive.frontier(&drive.path("")),
            churn_dirs,
        }
    }

    fn print(&self, out: &mut impl Write, label: &str) {
        let churned: usize = self
            .frontier
            .iter()
            .filter(|root| root.contains(&format!("/{CHURN_DIR}/")))
            .count();
        let _ = writeln!(
            out,
            "  {label}: {}",
            match self.ran_for {
                Some(at) if self.completed => format!("covered end to end in {at:.1?}"),
                Some(at) => format!("the machine gave up after {at:.1?}, and nothing marks the drive done"),
                None => format!("still working when the {PATIENCE:?} patience ran out"),
            },
        );
        let _ = writeln!(
            out,
            "    {}, {} frontier roots left ({churned} of them new), {} folders written during it",
            cmdr_fs::pluralize::pluralize_with(self.entries, "entry", "entries"),
            self.frontier.len(),
            self.churn_dirs,
        );
    }
}

impl Arm {
    fn print(&self) {
        let mut out = std::io::stderr();
        let _ = writeln!(&mut out, "\n══ {} ══", self.shape.label);
        let finished = self.first_indexes.iter().filter(|take| take.completed).count();
        let _ = writeln!(
            &mut out,
            "  {finished} of {} first indexes said the drive was done",
            self.first_indexes.len()
        );
        for (trial, take) in self.first_indexes.iter().enumerate() {
            take.print(&mut out, &format!("first index {}", trial + 1));
        }
        for (attempt, take) in self.relaunches.iter().enumerate() {
            take.print(&mut out, &format!("relaunch {}, still being written to", attempt + 1));
        }
        if let Some(quiet) = &self.quiet {
            quiet.print(&mut out, "relaunched with the writing stopped");
        }
    }
}

// ── The churn ────────────────────────────────────────────────────────

/// A thread creating directories inside one folder at a fixed rate, the way a
/// build writes into its target directory. Stops when it is dropped, and the drop
/// joins, so nothing is still writing when the temp tree goes.
struct Churn {
    stop: Arc<AtomicBool>,
    created: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Churn {
    /// Nobody is writing anything.
    fn none() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            created: Arc::new(AtomicU64::new(0)),
            thread: None,
        }
    }

    fn start(root: PathBuf, writing: Writing) -> Self {
        if matches!(writing, Writing::Nothing) {
            return Self::none();
        }
        let stop = Arc::new(AtomicBool::new(false));
        let created = Arc::new(AtomicU64::new(0));
        let (stopping, counting) = (Arc::clone(&stop), Arc::clone(&created));
        let make_one = move |root: &Path, counting: &AtomicU64| {
            let n = counting.fetch_add(1, Ordering::Relaxed);
            let dir = root.join(format!("unit-{n}"));
            if std::fs::create_dir(&dir).is_ok() {
                for file in 0..FILES_PER_CHURN_DIR {
                    let _ = std::fs::write(dir.join(format!("out-{file}.o")), b"x");
                }
            }
        };
        let thread = std::thread::Builder::new()
            .name("churn".into())
            .spawn(move || match writing {
                Writing::Nothing => {}
                Writing::Steady { dirs_per_second } => {
                    // Paced in bursts rather than one folder per sleep: a sleep
                    // shorter than the timer's resolution turns every rate into "as
                    // fast as this thread goes", which would make three arms one.
                    let per_burst = dirs_per_second.div_ceil(10).max(1);
                    let interval = Duration::from_secs_f64(per_burst as f64 / dirs_per_second as f64);
                    while !stopping.load(Ordering::Relaxed) {
                        let started = Instant::now();
                        for _ in 0..per_burst {
                            make_one(&root, &counting);
                        }
                        if let Some(rest) = interval.checked_sub(started.elapsed()) {
                            // allowed-test-sleep: the sleep IS the rate this arm is
                            // measuring; there is no condition to wait on.
                            std::thread::sleep(rest);
                        }
                    }
                }
                Writing::Burst { after, dirs } => {
                    let started = Instant::now();
                    while started.elapsed() < after && !stopping.load(Ordering::Relaxed) {
                        // allowed-test-sleep: waiting for a moment in the machine's
                        // run, which is the arm's whole parameter.
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    for _ in 0..dirs {
                        if stopping.load(Ordering::Relaxed) {
                            return;
                        }
                        make_one(&root, &counting);
                    }
                }
            })
            .expect("the churn thread starts");
        Self {
            stop,
            created,
            thread: Some(thread),
        }
    }

    fn created(&self) -> u64 {
        self.created.load(Ordering::Relaxed)
    }
}

impl Drop for Churn {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
