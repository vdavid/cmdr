//! Staying responsive to the person watching: frontier roots walked several to a
//! call, the folder they just opened taken between roots, and the progress pump
//! that outlives the walks it reports on.
//!
//! The two event sinks at the bottom are what makes that observable — one holds a
//! between-walks gap open from inside the sink, the other asks the status response
//! from inside an announcement, which is the one moment the answer is checkable.

use super::*;

/// Frontier roots that cost nothing are walked several to a `cover()` call.
///
/// The sizing RULE is `grouping.rs`'s own tests; this is the wiring, and the
/// property that pays for it: a resumed run's frontier is thousands of roots
/// holding two entries each, and one call apiece meant the claim, the branch
/// bracket, and the walk thread WERE the cost (185 s against 26 s over the
/// benchmark's tree, `tests::resume_bench`). ⚠️ It has to stay a wiring test —
/// asserting a particular group SIZE would pin the machine's speed on whatever
/// hardware runs the suite.
#[test]
fn tiny_frontier_roots_are_walked_several_to_a_call() {
    let drive = Drive::new(
        "phased-groups",
        |root| {
            for index in 0..40 {
                std::fs::create_dir_all(root.join(format!("tiny-{index:02}"))).expect("dirs");
                std::fs::write(root.join(format!("tiny-{index:02}/leaf.txt")), "x").expect("file");
            }
        },
        &[],
    );

    drive.start();
    drive.wait_for_the_machine();

    let biggest_group = drive
        .events
        .events()
        .into_iter()
        .filter_map(|event| match event {
            crate::indexing::events::IndexEvent::CoverageBranchStarted { volume_id, roots }
                if volume_id == drive.volume_id =>
            {
                Some(roots.len())
            }
            _ => None,
        })
        .max()
        .unwrap_or(0);
    assert!(
        biggest_group > 1,
        "roots this cheap are walked in groups, not one call each"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and grouping them covers the drive exactly as walking them one at a time did"
    );
}

/// A folder the user has open when indexing starts is covered as its own phase,
/// ahead of the rest of the drive. The rank ORDER itself is pinned by the queue's
/// own tests; this is the wiring: the poll reaches the machine, and the machine
/// gives what it finds a turn.
#[test]
fn a_visited_root_is_taken_between_frontier_roots() {
    let build = |root: &Path| {
        for name in ["a", "b", "zzz-visited"] {
            std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
        }
    };
    let phases_without_a_visit = {
        let drive = Drive::new("phased-no-visit", build, &[]);
        drive.start();
        drive.wait_for_the_machine();
        drive.phase_changes()
    };

    let drive = Drive::with_host(
        "phased-with-visit",
        build,
        |host, root| {
            host.note_open_listing("phased-with-visit", root.join("zzz-visited"));
        },
        &[],
        true,
    );
    drive.start();
    drive.wait_for_the_machine();

    assert_eq!(
        drive.phase_changes(),
        phases_without_a_visit + 1,
        "the folder the user is looking at earns a phase of its own"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the rest of the drive still gets covered"
    );
}

/// The pump the machine owns for its whole lifetime, doing both its jobs: it
/// reports progress, and it is the one legal place the machine hears where the
/// user is looking (the seam's contract is "the 500 ms tick, ❌ nothing faster",
/// and frontier-root boundaries are far faster than that).
#[test]
fn the_progress_pump_reports_and_polls_where_the_user_is_looking() {
    let _serialized = crate::indexing::handle::test_lock();
    let dir = tempfile::tempdir().expect("temp db dir");
    let db_path = dir.path().join("pump-test.db");
    IndexStore::open(&db_path).expect("open store");
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn writer");

    let host = crate::indexing::host::policy::FakeHostPolicy::shared();
    host.note_open_listing("pump-volume", "/somewhere/the/user/is");
    let (_index, _installed) = crate::indexing::handle::Index::builder()
        .data_dir(dir.path())
        .host(std::sync::Arc::clone(&host) as std::sync::Arc<_>)
        .install_for_test();

    let events = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let progress = std::sync::Arc::new(crate::indexing::scanner::ScanProgress::new());
    progress.entries_scanned.fetch_add(7, Ordering::Relaxed);
    let visits = std::sync::Arc::new(VisitLog::new());
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::indexing::lifecycle::progress_reporter::ScanProgressReporter::new(
        std::sync::Arc::clone(&progress),
        writer.clone(),
        std::sync::Arc::clone(&events) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        "pump-volume".to_string(),
        crate::indexing::writer::AggSource::Sql,
    )
    .noting_visits(std::sync::Arc::clone(&visits))
    .spawn(std::sync::Arc::clone(&done));

    cmdr_fs::testing::wait_until(std::time::Duration::from_secs(5), "the pump to tick", || {
        !events.kinds_for("pump-volume").is_empty()
    });
    done.store(true, Ordering::Relaxed);

    assert!(
        events
            .kinds_for("pump-volume")
            .contains(&crate::indexing::events::IndexEventKind::ScanProgress),
        "the progress stream is alive"
    );
    assert_eq!(
        visits.take(),
        Some(PathBuf::from("/somewhere/the/user/is")),
        "and the machine hears where the user is looking"
    );
    writer.shutdown();
}

/// The same pump, over a REAL machine run, answering the question the isolated
/// test above can't: whose lifetime is it?
///
/// It is the machine's, and everything riding the 500 ms tick depends on that —
/// the progress stream, the `open_listings` poll, and mid-scan partial
/// aggregation, which is what makes a size appear for the folder somebody is
/// looking at while the walker is deep inside a different frontier root. One
/// reporter per walk would die and restart 50–150 times a phase and tick almost
/// never: a walk over a frontier root usually finishes in milliseconds, well
/// inside the reporter's first sleep.
///
/// The gap between two frontier roots is where that difference shows, so the test
/// holds one open from inside the sink and watches what still arrives.
#[test]
fn the_progress_pump_outlives_the_walks_it_reports_on() {
    let recorder = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let watcher = std::sync::Arc::new(PauseBetweenWalks::new(
        "phased-pump-outlives",
        std::sync::Arc::clone(&recorder),
    ));
    let drive = Drive::assembled(
        "phased-pump-outlives",
        |root| {
            // Three frontier roots under the volume root, so there are two gaps
            // between walks and the first one has walks after it.
            for name in ["a", "b", "c"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::clone(&watcher) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        recorder,
        crate::indexing::host::policy::FakeHostPolicy::shared(),
    );

    drive.start();
    drive.wait_for_the_machine();

    assert!(
        watcher.held_a_gap_open(),
        "precondition: a walk ended and this test held the moment after it open"
    );
    assert!(
        watcher.ticks_in_the_gap() > 0,
        "❌ the pump died with the walk it was reporting on: nothing ticked in {:?} between frontier roots, \
         so nothing would refresh the size of the folder the user is looking at until the run ends",
        THE_GAP
    );
    assert!(
        watcher.walks_after_the_gap() > 0,
        "and the gap was BETWEEN frontier roots, not after the last one"
    );
}

/// How long one between-walks gap is held open. Three of the reporter's 500 ms
/// ticks fit, so a machine-lifetime pump lands at least two of them here however
/// the sleeps happen to line up.
const THE_GAP: std::time::Duration = std::time::Duration::from_millis(1_500);

/// A window that reloads mid-index joins a run already in progress, and the phase
/// event is transition-only: on the whole-volume phase the next one is the end of
/// the run, so a status response that can't name the running phase leaves that
/// window with no header for minutes.
///
/// Asked from INSIDE the announcement, which is the only moment the answer is
/// known to be checkable: the machine emits synchronously on its own thread, so
/// while `emit` runs the phase it just announced is the phase it is on.
#[test]
fn a_window_joining_mid_run_reads_the_running_phase_off_the_status() {
    let recorder = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let asker = std::sync::Arc::new(AsksTheStatusOnEveryPhase::new(
        "phased-status-phase",
        std::sync::Arc::clone(&recorder),
    ));
    let drive = Drive::assembled(
        "phased-status-phase",
        |root| {
            for name in ["a", "b"] {
                std::fs::create_dir_all(root.join(name).join("inner")).expect("dirs");
            }
        },
        |_, _| {},
        &["a"],
        true,
        std::sync::Arc::clone(&asker) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        recorder,
        crate::indexing::host::policy::FakeHostPolicy::shared(),
    );

    drive.start();
    drive.wait_for_the_machine();

    let answers = asker.answers();
    assert!(
        answers.len() >= 2,
        "precondition: this drive runs several phases, or the check below proves little ({answers:?})"
    );
    for (announced, from_status) in &answers {
        assert_eq!(
            from_status.as_ref(),
            Some(announced),
            "a window joining here would render the wrong phase, or none at all ({answers:?})"
        );
    }
    assert_eq!(
        drive
            .index
            .status(drive.volume_id)
            .expect("the volume answers for its own status")
            .coverage_phase,
        None,
        "and a machine with no work left reports no phase rather than the one it ended on"
    );
}

/// Asks the status response which phase is running, from inside each phase
/// announcement, and keeps both answers side by side.
struct AsksTheStatusOnEveryPhase {
    volume_id: &'static str,
    recorder: std::sync::Arc<crate::indexing::events::RecordingSink>,
    answers: std::sync::Mutex<
        Vec<(
            crate::indexing::events::CoveragePhase,
            Option<crate::indexing::events::CoveragePhase>,
        )>,
    >,
}

impl AsksTheStatusOnEveryPhase {
    fn new(volume_id: &'static str, recorder: std::sync::Arc<crate::indexing::events::RecordingSink>) -> Self {
        Self {
            volume_id,
            recorder,
            answers: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn answers(
        &self,
    ) -> Vec<(
        crate::indexing::events::CoveragePhase,
        Option<crate::indexing::events::CoveragePhase>,
    )> {
        use cmdr_fs::ignore_poison::IgnorePoison;
        self.answers.lock_ignore_poison().clone()
    }
}

impl crate::indexing::events::EventSink for AsksTheStatusOnEveryPhase {
    fn emit(&self, event: crate::indexing::events::IndexEvent) {
        use cmdr_fs::ignore_poison::IgnorePoison;
        let announced = match &event {
            crate::indexing::events::IndexEvent::CoveragePhaseStarted { volume_id, phase, .. }
                if volume_id == self.volume_id =>
            {
                Some(*phase)
            }
            _ => None,
        };
        self.recorder.emit(event);
        if let Some(announced) = announced {
            let from_status = crate::indexing::read::queries::get_status(self.volume_id)
                .ok()
                .and_then(|status| status.coverage_phase);
            self.answers.lock_ignore_poison().push((announced, from_status));
        }
    }
}

/// Holds the machine still in the gap after its FIRST walk, and counts what
/// arrives while it waits.
///
/// The machine emits on its own thread, synchronously, so blocking inside `emit`
/// IS a between-frontier-roots moment: the walk has finished, `walking` is already
/// false, and the next walk can't start until this returns. Anything whose
/// lifetime was that walk is gone by now; anything whose lifetime is the machine
/// keeps going, and that is what the counters below tell apart.
struct PauseBetweenWalks {
    volume_id: &'static str,
    recorder: std::sync::Arc<crate::indexing::events::RecordingSink>,
    /// Set while the gap is being held open, so a tick landing in it is counted.
    holding: std::sync::atomic::AtomicBool,
    /// One gap is enough, and holding every one of them would only make the test
    /// slower.
    held: std::sync::atomic::AtomicBool,
    ticks: std::sync::atomic::AtomicUsize,
    walks_after: std::sync::atomic::AtomicUsize,
}

impl PauseBetweenWalks {
    fn new(volume_id: &'static str, recorder: std::sync::Arc<crate::indexing::events::RecordingSink>) -> Self {
        Self {
            volume_id,
            recorder,
            holding: std::sync::atomic::AtomicBool::new(false),
            held: std::sync::atomic::AtomicBool::new(false),
            ticks: std::sync::atomic::AtomicUsize::new(0),
            walks_after: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn held_a_gap_open(&self) -> bool {
        self.held.load(Ordering::Relaxed)
    }

    /// Progress events that arrived while no walk was running.
    fn ticks_in_the_gap(&self) -> usize {
        self.ticks.load(Ordering::Relaxed)
    }

    /// Walks that started after the gap, which is what makes it a gap rather than
    /// the end of the run.
    fn walks_after_the_gap(&self) -> usize {
        self.walks_after.load(Ordering::Relaxed)
    }
}

impl crate::indexing::events::EventSink for PauseBetweenWalks {
    fn emit(&self, event: crate::indexing::events::IndexEvent) {
        use crate::indexing::events::IndexEventKind;
        let mine = event.volume_id() == Some(self.volume_id);
        let kind = event.kind();
        self.recorder.emit(event);
        if !mine {
            return;
        }
        match kind {
            IndexEventKind::ScanProgress if self.holding.load(Ordering::Relaxed) => {
                self.ticks.fetch_add(1, Ordering::Relaxed);
            }
            IndexEventKind::CoverageBranchStarted if self.held.load(Ordering::Relaxed) => {
                self.walks_after.fetch_add(1, Ordering::Relaxed);
            }
            IndexEventKind::CoverageBranchEnded if !self.held.swap(true, Ordering::Relaxed) => {
                self.holding.store(true, Ordering::Relaxed);
                // allowed-test-sleep: the gap IS the thing under test. Nothing the
                // machine does between two frontier roots can be waited on, because
                // the property is that something keeps happening while it does
                // nothing at all.
                std::thread::sleep(THE_GAP);
                self.holding.store(false, Ordering::Relaxed);
            }
            _ => {}
        }
    }
}

/// How many directories the big sibling holds. Enough that the walker is still
/// inside it several batches after it starts, so "the machine stopped it" and
/// "the machine finished it" are different observable outcomes.
const A_BIG_SIBLING: usize = 2_000;

/// How long the sink holds the machine still so the reporter's `open_listings`
/// poll lands before the walk starts. Two of its 500 ms ticks fit.
const UNTIL_THE_POLL_LANDS: std::time::Duration = std::time::Duration::from_millis(1_200);

/// The headline of this milestone: a folder somebody opens while a big sibling is
/// being walked doesn't wait for that sibling to finish.
///
/// The gap BETWEEN groups was never fine enough grain — `~/projects-git` is 1.58M
/// entries on a real machine and 97% of it is a single child, so no stitch depth
/// splits it and "whatever you open gets indexed next" meant "in forty seconds"
/// (`docs/specs/phased-indexing-plan.md` § "Interleaving without preemption").
/// So the walk itself is stopped, the folder is covered, and the sibling's
/// leftovers come back as frontier — which is what the last assertion is: ground
/// under the big sibling being walked AFTER the folder somebody opened.
#[test]
fn a_folder_the_user_opens_stops_the_walk_of_a_big_sibling() {
    let recorder = std::sync::Arc::new(crate::indexing::events::RecordingSink::new());
    let host = crate::indexing::host::policy::FakeHostPolicy::shared();
    let opener = std::sync::Arc::new(OpensAFolderMidWalk::new(
        "phased-preemption",
        std::sync::Arc::clone(&recorder),
        std::sync::Arc::clone(&host),
        "zzz-visited",
    ));
    let drive = Drive::assembled(
        "phased-preemption",
        |root| {
            for index in 0..A_BIG_SIBLING {
                std::fs::create_dir_all(root.join(format!("big-a/sub-{index:04}"))).expect("dirs");
            }
            std::fs::create_dir_all(root.join("big-b/inner")).expect("dirs");
            std::fs::create_dir_all(root.join("zzz-visited/inner")).expect("dirs");
        },
        |_, _| {},
        &[],
        true,
        std::sync::Arc::clone(&opener) as std::sync::Arc<dyn crate::indexing::events::EventSink>,
        recorder,
        host,
    );

    drive.start();
    drive.wait_for_the_machine();

    let branches = drive.walked_branches();
    let big = drive.path("big-a");
    let visited = drive.path("zzz-visited");
    assert!(
        branches.first().is_some_and(|roots| roots.contains(&big)),
        "precondition: the machine started on the big sibling ({branches:?})"
    );
    let covered_the_folder = branches
        .iter()
        .position(|roots| roots.iter().any(|root| root.starts_with(&visited)))
        .expect("the folder somebody opened was covered");
    let back_inside_the_sibling = branches
        .iter()
        .skip(covered_the_folder + 1)
        .position(|roots| roots.iter().any(|root| root.starts_with(&big)));

    assert!(
        back_inside_the_sibling.is_some(),
        "the big sibling ran to the end before the folder somebody opened got a walk, \
         so nothing was preempted: {branches:?}"
    );
    assert!(
        drive.frontier(&drive.path("")).is_empty(),
        "and the walk that was stopped resumed: the drive still ends covered"
    );
}

/// Opens a folder from inside the first walk's announcement, and holds the
/// machine still long enough for the reporter's poll to hear about it.
///
/// The announcement goes out on the machine's own thread just BEFORE the walk
/// starts, so what this arranges is the case the interlude between groups can't
/// take: the folder arrives once the walk is already reading.
struct OpensAFolderMidWalk {
    volume_id: &'static str,
    recorder: std::sync::Arc<crate::indexing::events::RecordingSink>,
    host: std::sync::Arc<crate::indexing::host::policy::FakeHostPolicy>,
    folder: &'static str,
    opened: std::sync::atomic::AtomicBool,
}

impl OpensAFolderMidWalk {
    fn new(
        volume_id: &'static str,
        recorder: std::sync::Arc<crate::indexing::events::RecordingSink>,
        host: std::sync::Arc<crate::indexing::host::policy::FakeHostPolicy>,
        folder: &'static str,
    ) -> Self {
        Self {
            volume_id,
            recorder,
            host,
            folder,
            opened: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl crate::indexing::events::EventSink for OpensAFolderMidWalk {
    fn emit(&self, event: crate::indexing::events::IndexEvent) {
        let starting = match &event {
            crate::indexing::events::IndexEvent::CoverageBranchStarted { volume_id, roots }
                if volume_id == self.volume_id =>
            {
                roots.first().cloned()
            }
            _ => None,
        };
        self.recorder.emit(event);
        let Some(first_root) = starting else { return };
        if self.opened.swap(true, Ordering::Relaxed) {
            return;
        }
        let opened = Path::new(&first_root)
            .parent()
            .expect("the frontier root sits under the tree")
            .join(self.folder);
        self.host.note_open_listing(self.volume_id, opened);
        // allowed-test-sleep: the machine hears where the user is looking on the
        // reporter's own 500 ms tick and ❌ nothing faster (`visits.rs`), so the
        // only way to arrange "the folder arrives while the walk is reading" is to
        // let that tick happen. Nothing observable fires when a poll lands.
        std::thread::sleep(UNTIL_THE_POLL_LANDS);
    }
}
