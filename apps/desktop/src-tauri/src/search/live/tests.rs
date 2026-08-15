//! What a live run does with what the walk hands it: the union, the batching,
//! the cap, and every terminal state.
//!
//! The walk itself isn't here — it's driven through a plain channel, which is
//! exactly the seam `drive_walk` puts between the walk's thread and the run's.
//! Real walks over real ground are `crates/cmdr-index/src/indexing/lifecycle/cover/`
//! and the end-to-end test in `search/execute/tests.rs`.

use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, sync_channel};

use super::events::CollectorSearchEventSink;
use super::*;
use crate::search::types::PatternType;

// ── Fixtures ─────────────────────────────────────────────────────────

/// A run built without the registry, so a test never supersedes another's.
/// Registry behavior has its own tests, which take [`test_registry_lock`].
fn run_for_test(volume_id: &str) -> Arc<LiveRun> {
    Arc::new(LiveRun {
        run_id: format!("run-{volume_id}"),
        volume_id: volume_id.to_string(),
        origin: RunOrigin::Dialog,
        cancel: CancellationToken::new(),
        superseded: AtomicBool::new(false),
    })
}

/// A plain substring query for `stem`, with the system-directory tier off unless
/// a test asks for it.
fn query(stem: &str) -> SearchQuery {
    SearchQuery {
        name_pattern: Some(stem.to_string()),
        pattern_type: PatternType::Glob,
        min_size: None,
        max_size: None,
        modified_after: None,
        modified_before: None,
        is_directory: None,
        include_paths: None,
        exclude_dir_names: None,
        include_path_ids: None,
        count_only: false,
        limit: 30,
        case_sensitive: Some(false),
        exclude_system_dirs: Some(false),
        sort_by: None,
    }
}

fn covered_file(path: &str) -> CoveredEntry {
    CoveredEntry {
        path: PathBuf::from(path),
        is_directory: false,
        is_symlink: false,
        logical_size: Some(10),
        physical_size: Some(10),
        modified_at: Some(1),
    }
}

fn indexed_row(path: &str) -> SearchResultEntry {
    SearchResultEntry {
        name: path.rsplit('/').next().unwrap_or(path).to_string(),
        path: path.to_string(),
        parent_path: "/".to_string(),
        is_directory: false,
        size: Some(10),
        modified_at: Some(1),
        icon_id: "file".to_string(),
        entry_id: 7,
    }
}

/// An outcome from a walk that covered everything it took.
fn covered_everything(roots: usize) -> CoverOutcome {
    CoverOutcome {
        entries_found: 0,
        dirs_found: 0,
        roots_covered: roots,
        cancelled: false,
        abandoned_ground: false,
    }
}

/// An outcome from a walk that covered every root it took but gave up on ground
/// inside them: the "completed, and still short" case (Accepted difference 9).
fn covered_but_gave_up(roots: usize) -> CoverOutcome {
    CoverOutcome {
        abandoned_ground: true,
        ..covered_everything(roots)
    }
}

/// Drive `messages` through a run, and report what the sink saw.
struct Driven {
    ending: WalkEnding,
    /// Whether the walk gave up on ground it started, independent of `ending`.
    abandoned_ground: bool,
    sink: Arc<CollectorSearchEventSink>,
}

/// Everything a walked entry is judged by, built for one query.
struct Judged {
    compiled: CompiledQuery,
    excludes: ExcludeRules,
}

impl Judged {
    fn new(query: &SearchQuery) -> Self {
        let compiled = CompiledQuery::compile(query, crate::search::matcher::Evaluator::LiveWalk)
            .expect("the test queries all narrow something");
        let excludes = ExcludeRules::from_query(query, compiled.case_insensitive());
        Self { compiled, excludes }
    }

    fn judge(&self) -> WalkJudge<'_> {
        WalkJudge {
            compiled: &self.compiled,
            excludes: &self.excludes,
            volume_root: None,
            home_dir: None,
        }
    }
}

/// Run the pump over a channel a test feeds, with `indexed` standing in for the
/// covered half.
fn drive(
    run: &Arc<LiveRun>,
    query: &SearchQuery,
    indexed: Vec<SearchResultEntry>,
    indexed_total: u32,
    feed: impl FnOnce(&SyncSender<WalkMsg>) + Send + 'static,
) -> Driven {
    let sink = Arc::new(CollectorSearchEventSink::default());
    let judged = Judged::new(query);
    let (tx, rx) = sync_channel(8);
    let feeder = std::thread::spawn(move || feed(&tx));

    let mut stream = ResultStream::new(run, sink.as_ref(), query);
    stream.add_indexed(indexed, indexed_total, 0);
    let walked = pump(&rx, 1, &judged.judge(), &mut stream, &WalkPulse::default());
    let coverage = SearchRunCoverage {
        walk: walked.ending,
        kind: CoverageKind::Live,
        permission_denied: Vec::new(),
        declined: Vec::new(),
        still_covering: Vec::new(),
        unresolved_scopes: Vec::new(),
        abandoned_ground: walked.abandoned_ground,
        abandoned_locations: 0,
        capped: stream.capped(),
        target_volume_id: run.volume_id.clone(),
        hidden_by_excludes: 0,
    };
    stream.finish(coverage);
    feeder.join().expect("the feeder thread");
    Driven {
        ending: walked.ending,
        abandoned_ground: walked.abandoned_ground,
        sink,
    }
}

// ── The union ────────────────────────────────────────────────────────

#[test]
fn the_two_halves_make_one_answer_with_nothing_shown_twice() {
    // The covered half comes from the index and the rest from the walk, and the
    // partition is what keeps them apart. The race it can't rule out — a file
    // indexed between the frontier query and the walk reaching it — would show
    // the same file twice and count it twice, which is what the bounded seen-set
    // insures against.
    let run = run_for_test("union");
    let q = query("report");
    let driven = drive(&run, &q, vec![indexed_row("/covered/report-1.pdf")], 1, |tx| {
        tx.send(WalkMsg::Batch(vec![
            covered_file("/walked/report-2.pdf"),
            // The same file the index already answered for.
            covered_file("/covered/report-1.pdf"),
            covered_file("/walked/notes.txt"),
        ]))
        .expect("send");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    assert_eq!(driven.ending, WalkEnding::Completed);
    let paths: Vec<String> = driven.sink.rows().into_iter().map(|row| row.path).collect();
    assert_eq!(
        paths,
        vec!["/covered/report-1.pdf", "/walked/report-2.pdf"],
        "the index half first, then what the walk added, each once"
    );
    let complete = driven.sink.complete.lock_ignore_poison();
    assert_eq!(
        complete[0].match_count, 2,
        "and the count is the union's, not the sum of the two halves"
    );
    assert!(
        !complete[0].coverage.capped,
        "a run that showed everything it found doesn't claim it was cut short"
    );
}

#[test]
fn a_walked_entry_is_judged_by_the_matcher_and_the_exclusions_alike() {
    // The live half applies BOTH: the compiled query (`matcher.rs`) and the scope
    // exclusions (`excludes.rs`), so a drive being unindexed can't turn a filter
    // off. Here the system-directory tier is on, so the match inside
    // `node_modules` is out and the one beside it is in.
    let run = run_for_test("judged");
    let q = SearchQuery {
        exclude_system_dirs: Some(true),
        ..query("report")
    };
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![
            covered_file("/p/node_modules/report.pdf"),
            covered_file("/p/src/report.pdf"),
            covered_file("/p/src/unrelated.txt"),
        ]))
        .expect("send");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    let paths: Vec<String> = driven.sink.rows().into_iter().map(|row| row.path).collect();
    assert_eq!(paths, vec!["/p/src/report.pdf"]);
}

// ── Superseding (Decision 11) ────────────────────────────────────────

#[test]
fn a_query_refined_mid_walk_drops_the_batches_and_keeps_the_walk() {
    // The TDD anchor for Decision 11, in the shape it happens: a walk is running
    // and streaming when the user refines the query. Everything already sent
    // stands; everything found after belongs to a question nobody is asking any
    // more; and the WALK — coverage work, not query work — runs to the end, so the
    // refined query finds that ground in the index rather than walking it again.
    //
    // Draining after the supersede is not politeness: the channel is bounded, so a
    // run that stopped reading would park the walk it isn't allowed to stop.
    let _serialized = test_registry_lock();
    let first = register("run-1", "supersede-volume", RunOrigin::Dialog);
    let q = SearchQuery {
        limit: 1000,
        ..query("report")
    };
    let sink = Arc::new(CollectorSearchEventSink::default());
    let judged = Judged::new(&q);
    let (tx, rx) = sync_channel(8);

    let watcher = Arc::clone(&sink);
    let refined = Arc::clone(&first);
    let feeder = std::thread::spawn(move || {
        // A full batch, so it goes out on the row rule rather than the timer.
        let found: Vec<CoveredEntry> = (0..BATCH_ROWS)
            .map(|i| covered_file(&format!("/walked/report-before-{i:03}.pdf")))
            .collect();
        tx.send(WalkMsg::Batch(found)).expect("send");
        // Refine only once those results are genuinely on screen, or the test
        // would be asserting about a batch the run never got to see.
        cmdr_fs::testing::wait_until(Duration::from_secs(5), "the first batch to reach the frontend", || {
            watcher.rows().len() == BATCH_ROWS
        });

        // The user types. A new run registers, superseding this one.
        let _second = register("run-2", "supersede-volume", RunOrigin::Dialog);
        assert!(!refined.wants_events(), "the newer run supersedes the older one");
        assert!(!refined.is_cancelled(), "❌ and does NOT stop its walk");

        tx.send(WalkMsg::Batch(vec![covered_file("/walked/report-after.pdf")]))
            .expect("a superseded run still reads its walk");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    let mut stream = ResultStream::new(&first, sink.as_ref(), &q);
    let walked = pump(&rx, 1, &judged.judge(), &mut stream, &WalkPulse::default());
    let ending = walked.ending;
    feeder.join().expect("the feeder thread");

    assert_eq!(ending, WalkEnding::Completed, "the walk ran to the end");
    let paths: Vec<String> = sink.rows().into_iter().map(|row| row.path).collect();
    assert_eq!(paths.len(), BATCH_ROWS, "what it found before the refinement stands");
    assert!(
        !paths.iter().any(|path| path.contains("after")),
        "and what came after is dropped: {paths:?}"
    );
    stream.finish(SearchRunCoverage {
        walk: ending,
        kind: CoverageKind::Live,
        permission_denied: Vec::new(),
        declined: Vec::new(),
        still_covering: Vec::new(),
        unresolved_scopes: Vec::new(),
        abandoned_ground: false,
        abandoned_locations: 0,
        capped: false,
        target_volume_id: "supersede-volume".to_string(),
        hidden_by_excludes: 0,
    });
    assert!(
        sink.complete.lock_ignore_poison().is_empty(),
        "and a superseded run has no last word either"
    );
    deregister("run-1");
    deregister("run-2");
}

#[test]
fn a_walk_that_wrote_rows_marks_its_volume_for_the_next_query() {
    // The other half of Decisions 11 and 12 together: the ground this walk covered
    // is recovered from the INDEX by the next query, which only works if that
    // query rebuilds its arena first. The mark is what tells it to, and a
    // superseded run has to keep marking — its walk is still writing.
    let _serialized = test_registry_lock();
    let run = register("run-marks", "marked-volume", RunOrigin::Dialog);
    let second = register("run-marks-2", "marked-volume", RunOrigin::Dialog);
    let q = query("report");
    volumes::take_walked_behind("marked-volume");

    drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![covered_file("/walked/report.pdf")]))
            .expect("send");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    assert!(
        volumes::take_walked_behind("marked-volume"),
        "a superseded run's walk still moves the arena out from under the next query"
    );
    drop(second);
    deregister("run-marks");
    deregister("run-marks-2");
}

// ── Batching ─────────────────────────────────────────────────────────

#[test]
fn rows_go_out_in_batches_of_at_most_a_hundred() {
    let run = run_for_test("batching");
    let q = SearchQuery {
        limit: 1000,
        ..query("f")
    };
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        let batch: Vec<CoveredEntry> = (0..250).map(|i| covered_file(&format!("/w/f{i:04}.txt"))).collect();
        tx.send(WalkMsg::Batch(batch)).expect("send");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    assert_eq!(driven.sink.rows().len(), 250);
    let sizes: Vec<usize> = driven.sink.batch_sizes().into_iter().filter(|size| *size > 0).collect();
    assert_eq!(
        sizes,
        vec![100, 100, 50],
        "full batches while they fill, then the remainder — not 250 events of one row"
    );
}

#[test]
fn the_index_half_goes_out_in_batches_too() {
    // Same rule on the other half: an arena that answered with 250 rows sends
    // three events, not 250. The frontend appends per event, so a row-per-event
    // stream is 250 renders of a growing list.
    let run = run_for_test("indexed-batching");
    let q = SearchQuery {
        limit: 1000,
        ..query("f")
    };
    let sink = CollectorSearchEventSink::default();
    let mut stream = ResultStream::new(&run, &sink, &q);

    let rows: Vec<SearchResultEntry> = (0..250)
        .map(|i| indexed_row(&format!("/covered/f{i:04}.txt")))
        .collect();
    stream.add_indexed(rows, 250, 0);

    let sizes: Vec<usize> = sink.batch_sizes().into_iter().filter(|size| *size > 0).collect();
    assert_eq!(sizes, vec![100, 100, 50]);
}

#[test]
fn a_lone_row_does_not_wait_for_company() {
    // A walk grinding through folders that match nothing must not hold the one
    // match it did find until the next batch happens to arrive — that's the 100 ms
    // half of "100 rows or 100 ms, whichever comes first".
    let run = run_for_test("interval");
    let q = query("report");
    let started = Instant::now();
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![covered_file("/w/report.pdf")]))
            .expect("send");
        // Several flush intervals of silence, so both halves of the rule are
        // visible: the row went out during it, and progress kept arriving.
        // allowed-test-sleep: the silence IS the subject — a walk that says nothing for a while
        std::thread::sleep(Duration::from_millis(400));
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    let first_row_at = driven
        .sink
        .progress
        .lock_ignore_poison()
        .iter()
        .position(|event| !event.entries.is_empty())
        .expect("the row went out");
    assert!(first_row_at < driven.sink.batch_sizes().len() - 1, "before the end");
    assert!(
        started.elapsed() >= Duration::from_millis(400),
        "the walk really ran on"
    );
}

// ── The cap ──────────────────────────────────────────────────────────

#[test]
fn the_cap_stops_the_rows_and_never_the_walk() {
    // Convergence is the payoff: a walk stopped at the cap would leave the ground
    // it hadn't reached uncovered forever, and freeze "N so far" at a number that
    // never becomes true. So the rows stop, the count doesn't, and the walk runs
    // to the end.
    let run = run_for_test("capped");
    let q = SearchQuery { limit: 2, ..query("f") };
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        for i in 0..6 {
            tx.send(WalkMsg::Batch(vec![covered_file(&format!("/w/f{i}.txt"))]))
                .expect("the walk is never turned away at the cap");
        }
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    assert_eq!(driven.ending, WalkEnding::Completed);
    assert_eq!(driven.sink.rows().len(), 2, "two rows, as asked");
    let complete = driven.sink.complete.lock_ignore_poison();
    assert_eq!(complete[0].match_count, 6, "and a count that kept rising past them");
    assert!(complete[0].coverage.capped);
}

#[test]
fn the_cap_covers_the_index_half_too() {
    // The engine truncates its own slice, so this guard never fires in
    // production — which is exactly why it's asserted here. `add_indexed` is what
    // the covered half arrives through, and a stream that trusted its caller's
    // length would emit past the cap the moment one changed.
    let run = run_for_test("indexed-cap");
    let q = SearchQuery { limit: 2, ..query("f") };
    let sink = CollectorSearchEventSink::default();
    let mut stream = ResultStream::new(&run, &sink, &q);

    let rows: Vec<SearchResultEntry> = (0..5).map(|i| indexed_row(&format!("/covered/f{i}.txt"))).collect();
    stream.add_indexed(rows, 5, 0);

    assert_eq!(sink.rows().len(), 2, "two rows, as asked");
    assert!(stream.capped());
    assert_eq!(
        sink.progress.lock_ignore_poison().last().expect("an event").match_count,
        5,
        "and the count is the volume's, not the slice's"
    );
}

#[test]
fn a_count_only_run_counts_without_building_a_single_row() {
    let run = run_for_test("count-only");
    let q = SearchQuery {
        count_only: true,
        ..query("f")
    };
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![
            covered_file("/w/f1.txt"),
            covered_file("/w/f2.txt"),
        ]))
        .expect("send");
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });

    assert!(driven.sink.rows().is_empty());
    assert_eq!(driven.sink.complete.lock_ignore_poison()[0].match_count, 2);
}

// ── Progress ─────────────────────────────────────────────────────────

#[test]
fn progress_follows_the_walk_rather_than_the_batches_it_emits() {
    // The regression this guards: `dirs_found` and `current_path` used to be
    // derived from the batches the walk emitted, and a batch fills at 2 000
    // entries. So a walk grinding through a slow tree — or parked on a directory
    // that hangs — reported "0 folders scanned" and no path for as long as that
    // lasted, which reads as frozen. Here NOT ONE batch arrives, and the run still
    // reports where the walk is. No percentage and no ETA either way: the total is
    // unknown by definition (Decision 14).
    let run = run_for_test("progress");
    let q = query("report");
    let sink = Arc::new(CollectorSearchEventSink::default());
    let judged = Judged::new(&q);
    let (tx, rx) = sync_channel(4);

    let pulse = WalkPulse::default();
    pulse.dirs_scanned.store(41, Ordering::Relaxed);
    *pulse.current_dir.lock_ignore_poison() = Some("/w/deep/slow".to_string());
    tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    drop(tx);

    let mut stream = ResultStream::new(&run, sink.as_ref(), &q);
    pump(&rx, 1, &judged.judge(), &mut stream, &pulse);
    stream.finish(SearchRunCoverage {
        walk: WalkEnding::Completed,
        kind: CoverageKind::Live,
        permission_denied: Vec::new(),
        declined: Vec::new(),
        still_covering: Vec::new(),
        unresolved_scopes: Vec::new(),
        abandoned_ground: false,
        abandoned_locations: 0,
        capped: false,
        target_volume_id: run.volume_id.clone(),
        hidden_by_excludes: 0,
    });

    let progress = sink.progress.lock_ignore_poison();
    let last = progress.last().expect("at least one event");
    assert_eq!(last.dirs_found, 41, "the count comes off the walk, not the batches");
    assert_eq!(last.current_path.as_deref(), Some("/w/deep/slow"));
}

// ── Terminal states ──────────────────────────────────────────────────

#[test]
fn a_completed_walk_that_covered_its_ground_reads_as_complete() {
    let run = run_for_test("complete");
    let q = query("report");
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    });
    assert_eq!(driven.ending, WalkEnding::Completed);
    assert_eq!(driven.sink.complete.lock_ignore_poison().len(), 1);
    assert!(driven.sink.cancelled.lock_ignore_poison().is_empty());
}

#[test]
fn cancelling_stops_the_run_promptly_and_ends_it_as_cancelled() {
    // Escape, or the dialog closing. The run must not wait for a walk that may be
    // parked on a slow network read, so the wait is bounded by the flush interval.
    let run = run_for_test("cancel");
    let q = query("report");
    let sink = Arc::new(CollectorSearchEventSink::default());
    let judged = Judged::new(&q);
    // A walk that says nothing for a long time (a network read in flight): only
    // the cancel gets this run out, so the sender is deliberately never joined.
    let (tx, rx) = sync_channel::<WalkMsg>(1);
    std::thread::spawn(move || {
        // allowed-test-sleep: fake latency — a walk parked on a slow read, which only the cancel gets out of
        std::thread::sleep(Duration::from_secs(5));
        let _ = tx.send(WalkMsg::Ended(covered_everything(1)));
    });
    let cancel_at = run.cancel_token();
    std::thread::spawn(move || {
        // allowed-test-sleep: the canceller's head start, so the pump is waiting when it fires
        std::thread::sleep(Duration::from_millis(20));
        cancel_at.cancel();
    });

    let started = Instant::now();
    let mut stream = ResultStream::new(&run, sink.as_ref(), &q);
    let ending = pump(&rx, 1, &judged.judge(), &mut stream, &WalkPulse::default()).ending;
    let elapsed = started.elapsed();
    stream.finish(SearchRunCoverage {
        walk: ending,
        kind: CoverageKind::Live,
        permission_denied: Vec::new(),
        declined: Vec::new(),
        still_covering: Vec::new(),
        unresolved_scopes: Vec::new(),
        abandoned_ground: false,
        abandoned_locations: 0,
        capped: false,
        target_volume_id: run.volume_id.clone(),
        hidden_by_excludes: 0,
    });

    assert_eq!(ending, WalkEnding::Cancelled);
    assert!(
        elapsed < Duration::from_millis(500),
        "it stopped without waiting the walk out ({elapsed:?})"
    );
    let cancelled = sink.cancelled.lock_ignore_poison();
    assert_eq!(cancelled.len(), 1, "one terminal event, and it's the cancelled one");
    assert_eq!(cancelled[0].coverage.walk, WalkEnding::Cancelled);
    assert!(sink.complete.lock_ignore_poison().is_empty());
}

#[test]
fn a_walk_that_stopped_without_being_asked_reads_as_interrupted() {
    // The drive went away mid-walk. `CoverOutcome::cancelled` says the walk
    // stopped early; that we did NOT ask is what makes it a disconnect rather than
    // a cancel, and the two are different sentences in the UI.
    let run = run_for_test("disconnect");
    let q = query("report");
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![covered_file("/w/report.pdf")]))
            .expect("send");
        tx.send(WalkMsg::Ended(CoverOutcome {
            entries_found: 1,
            dirs_found: 0,
            roots_covered: 0,
            cancelled: true,
            abandoned_ground: false,
        }))
        .expect("send");
    });

    assert_eq!(driven.ending, WalkEnding::Interrupted);
    let complete = driven.sink.complete.lock_ignore_poison();
    assert_eq!(complete[0].coverage.walk, WalkEnding::Interrupted);
    assert_eq!(complete[0].match_count, 1, "what it did find is still real");
}

#[test]
fn a_walk_that_finished_having_given_up_on_ground_says_so_on_its_own_field() {
    // The quiet third way a run comes back short (Accepted difference 9). The
    // walk covered every root it took and nobody stopped it, so `walk` reads
    // `completed` — and the list is still a lower bound. Folding this into
    // `interrupted` would tell the user the drive went away, which isn't what
    // happened: those directories stay unlisted and the next search retries them.
    let run = run_for_test("gave-up");
    let q = query("report");
    let driven = drive(&run, &q, Vec::new(), 0, |tx| {
        tx.send(WalkMsg::Batch(vec![covered_file("/w/report.pdf")]))
            .expect("send");
        tx.send(WalkMsg::Ended(covered_but_gave_up(1))).expect("send");
    });

    assert_eq!(driven.ending, WalkEnding::Completed, "it did reach the end");
    assert!(driven.abandoned_ground, "and it read less than the tree holds");
    let complete = driven.sink.complete.lock_ignore_poison();
    assert!(
        complete[0].coverage.abandoned_ground,
        "the terminal event has to carry it, or the UI can't label the list short"
    );
}

#[test]
fn a_walk_that_left_a_root_uncovered_reads_as_interrupted() {
    // One frontier root that couldn't be walked doesn't stop the others, and it
    // doesn't get to look complete either: the root stays frontier, and the next
    // search asks for it again.
    let run = run_for_test("partial");
    let q = query("report");
    let sink = Arc::new(CollectorSearchEventSink::default());
    let judged = Judged::new(&q);
    let (tx, rx) = sync_channel(4);
    tx.send(WalkMsg::Ended(covered_everything(1))).expect("send");
    drop(tx);

    let mut stream = ResultStream::new(&run, sink.as_ref(), &q);
    // Two roots taken, one covered.
    let ending = pump(&rx, 2, &judged.judge(), &mut stream, &WalkPulse::default()).ending;
    assert_eq!(ending, WalkEnding::Interrupted);
}

#[test]
fn a_walk_that_never_reported_at_all_reads_as_interrupted() {
    // The reader thread died, or the process is coming down. Nothing may claim the
    // frontier was covered.
    let run = run_for_test("silent");
    let q = query("report");
    let driven = drive(&run, &q, Vec::new(), 0, |_tx| {});
    assert_eq!(driven.ending, WalkEnding::Interrupted);
}

#[test]
fn ending_of_puts_our_own_cancel_ahead_of_every_other_verdict() {
    // A cancelled walk reports `cancelled` whatever else happened, so the order of
    // these checks IS the difference between "you stopped it" and "the drive went
    // away".
    let stopped = CoverOutcome {
        entries_found: 0,
        dirs_found: 0,
        roots_covered: 0,
        cancelled: true,
        abandoned_ground: false,
    };
    assert_eq!(ending_of(Some(&stopped), 1, true), WalkEnding::Cancelled);
    assert_eq!(ending_of(Some(&stopped), 1, false), WalkEnding::Interrupted);
    assert_eq!(ending_of(Some(&covered_everything(2)), 2, false), WalkEnding::Completed);
    assert_eq!(ending_of(None, 0, true), WalkEnding::Cancelled);
    assert_eq!(ending_of(None, 0, false), WalkEnding::Interrupted);
}

#[test]
fn a_stopped_run_stops_counting_as_well_as_showing() {
    // Cancel's end state is "what was found before stopping", so the gate has to
    // cover the COUNT, not just the rows: a walk winding down still hands over
    // whatever batch it was mid-way through, and counting those would leave the
    // number on screen creeping up after the user pressed Escape.
    let run = run_for_test("stopped");
    run.cancel_token().cancel();
    let q = query("report");
    let sink = CollectorSearchEventSink::default();
    let judged = Judged::new(&q);
    let mut stream = ResultStream::new(&run, &sink, &q);

    judged
        .judge()
        .consume(vec![covered_file("/walked/report.pdf")], &mut stream);

    assert_eq!(stream.match_count, 0, "a match found after the stop is not this run's");
    assert!(sink.rows().is_empty());
    // Progress still moves: what the walk READ is true whoever asked for it, and
    // the terminal event reports it.
    assert_eq!(stream.dirs_found, 0);
}

#[test]
fn a_run_that_cannot_run_says_so_unless_nobody_is_listening() {
    // The refusal path is the one that tells someone WHY they got nothing — "add a
    // filename pattern" for a query too broad to walk with. It has to reach a live
    // run, and it has to stay quiet for a superseded one, whose id the frontend
    // dropped when it asked the next question.
    let q = query("report");
    let live_sink = CollectorSearchEventSink::default();
    let listening = run_for_test("failing");
    ResultStream::new(&listening, &live_sink, &q).fail(SearchRunError::Query, "Query too broad.".to_string());
    let errors = live_sink.errors.lock_ignore_poison();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].error, SearchRunError::Query);
    assert_eq!(errors[0].message, "Query too broad.");

    let gone_sink = CollectorSearchEventSink::default();
    let superseded = run_for_test("failing-superseded");
    superseded.superseded.store(true, Ordering::Relaxed);
    ResultStream::new(&superseded, &gone_sink, &q).fail(SearchRunError::Query, "Query too broad.".to_string());
    assert!(gone_sink.errors.lock_ignore_poison().is_empty());
}

// ── The wire ─────────────────────────────────────────────────────────

#[test]
fn the_event_family_keeps_its_wire_names() {
    // The frontend listens by string, and a window's capability permission is
    // granted under that string, so a rename is a listener that silently never
    // fires. `#[tauri_specta(event_name = …)]` pins each one; this is what says so
    // out loud, on the side that emits them.
    use tauri_specta::Event;

    assert_eq!(SearchProgressEvent::NAME, "search-progress");
    assert_eq!(SearchCompleteEvent::NAME, "search-complete");
    assert_eq!(SearchCancelledEvent::NAME, "search-cancelled");
    assert_eq!(SearchErrorEvent::NAME, "search-error");
}

// ── The registry ─────────────────────────────────────────────────────

#[test]
fn cancelling_everything_stops_every_run_in_flight() {
    // What a quitting app calls. It reaches an agent's run too: nobody's search
    // outlives the process.
    let _serialized = test_registry_lock();
    let one = register("run-all-1", "volume-a", RunOrigin::Dialog);
    let two = register("run-all-2", "volume-b", RunOrigin::Agent);
    cancel_all_live_runs();
    assert!(one.is_cancelled() && two.is_cancelled());
    deregister("run-all-1");
    deregister("run-all-2");
}

#[test]
fn cancelling_a_run_nobody_registered_says_so_rather_than_pretending() {
    let _serialized = test_registry_lock();
    assert!(!cancel_live_run("a-run-that-never-was"));
    let run = register("run-cancel-one", "volume-c", RunOrigin::Dialog);
    assert!(cancel_live_run("run-cancel-one"));
    assert!(run.is_cancelled());
    deregister("run-cancel-one");
}

#[test]
fn an_agent_run_and_the_dialog_stay_out_of_each_others_way() {
    // Superseding exists because ONE dialog asks one question at a time. An MCP
    // call is its own asker with its own caller waiting, so neither run may
    // silence the other: a person typing mid-agent-search would otherwise watch
    // their results stop arriving, and an agent's answer would come back empty
    // because somebody touched the keyboard.
    let _serialized = test_registry_lock();
    let dialog = register("run-dialog", "volume-d", RunOrigin::Dialog);
    let agent = register("run-agent", "volume-d", RunOrigin::Agent);
    assert!(dialog.wants_events(), "an agent's search doesn't supersede the dialog");
    assert!(agent.wants_events());

    // And the dialog asking its next question supersedes only its own.
    let refined = register("run-dialog-2", "volume-d", RunOrigin::Dialog);
    assert!(!dialog.wants_events(), "the dialog's own previous run is superseded");
    assert!(agent.wants_events(), "the agent's is not");
    assert!(refined.wants_events());

    deregister("run-dialog");
    deregister("run-dialog-2");
    deregister("run-agent");
}

#[test]
fn closing_the_dialog_leaves_an_agents_search_running() {
    // `release_search_index` means "nobody is watching the dialog any more",
    // which says nothing about an MCP call still waiting on its answer.
    let _serialized = test_registry_lock();
    let dialog = register("run-close-dialog", "volume-e", RunOrigin::Dialog);
    let agent = register("run-close-agent", "volume-e", RunOrigin::Agent);
    cancel_dialog_runs_except(None);
    assert!(dialog.is_cancelled());
    assert!(
        !agent.is_cancelled(),
        "the agent's run is none of the dialog's business"
    );
    deregister("run-close-dialog");
    deregister("run-close-agent");
}
