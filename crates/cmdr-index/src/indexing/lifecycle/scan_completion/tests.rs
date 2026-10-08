//! What the post-scan completion task does with each of its three outcomes,
//! against a real writer over a real DB and a `RecordingSink` in place of the app.

use super::unfinished::scan_failure_is_vanished_volume;
use super::*;
use crate::indexing::events::{IndexEventKind, RecordingSink};
use crate::indexing::hold;
use crate::indexing::lifecycle::freshness::Freshness;

/// Everything `run_scan_completion` needs, with a real writer over a real DB
/// and a `RecordingSink` in place of the app, so the assertions can read the
/// meta table the handler actually wrote.
struct Fixture {
    writer: IndexWriter,
    db_path: std::path::PathBuf,
    events: Arc<RecordingSink>,
    freshness: Arc<std::sync::Mutex<Option<Freshness>>>,
    /// Held so the watcher channel stays open for the duration of the test.
    _event_tx: tokio::sync::mpsc::UnboundedSender<FsChangeEvent>,
    event_rx: Option<tokio::sync::mpsc::UnboundedReceiver<FsChangeEvent>>,
    _dir: tempfile::TempDir,
}

impl Fixture {
    fn new(volume_id: &str) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join(format!("{volume_id}.db"));
        IndexStore::open(&db_path).expect("open store");
        let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("spawn the writer");
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        Self {
            writer,
            db_path,
            events: Arc::new(RecordingSink::new()),
            // A scan is in flight when the handler runs, so start from `Scanning`
            // (what `ScanStarted` left behind).
            freshness: Arc::new(std::sync::Mutex::new(Some(Freshness::Scanning))),
            _event_tx: event_tx,
            event_rx: Some(event_rx),
            _dir: dir,
        }
    }

    /// Build the handler's inputs around a walk thread that resolves to `result`.
    fn completion(&mut self, volume_id: &str, result: Result<ScanSummary, ScanError>) -> ScanCompletion {
        ScanCompletion {
            join_handle: std::thread::spawn(move || result),
            scan_done: Arc::new(AtomicBool::new(false)),
            ground_in_flux: Arc::new(AtomicBool::new(true)),
            // The ground a real scan hands over, taken the way `start_scan`
            // takes it so the handler releases something real.
            ground: cover::Claim::take(volume_id, vec!["/".to_string()], cover::Holder::Rewriting),
            event_rx: self.event_rx.take().expect("one completion per fixture"),
            watcher_overflow_flag: None,
            volume_id: volume_id.to_string(),
            space: IndexPathSpace::root(),
            events: Arc::clone(&self.events) as Arc<dyn EventSink>,
            writer: self.writer.clone(),
            freshness: Arc::clone(&self.freshness),
            live_event_task_slot: Arc::new(std::sync::Mutex::new(None)),
            scan_start_event_id: 0,
            calibration_kind: ScanCalibrationKind::FullWalk,
            work: VolumeWork::for_test(volume_id),
        }
    }

    /// The value of `meta.scan_completed_at` once the writer has drained.
    async fn completion_marker(&self) -> Option<String> {
        self.writer.flush().await.expect("flush the writer");
        let conn = IndexStore::open_read_connection(&self.db_path).expect("read connection");
        IndexStore::get_meta(&conn, "scan_completed_at").expect("read the meta table")
    }

    fn freshness_now(&self) -> Option<Freshness> {
        *self.freshness.lock_ignore_poison()
    }
}

fn summary(entries: u64) -> ScanSummary {
    ScanSummary {
        total_entries: entries,
        total_dirs: 1,
        total_physical_bytes: 4096,
        duration_ms: 12,
    }
}

/// A walk that ran to the end stamps the completion marker, so the next launch
/// loads the index instead of rescanning the whole volume.
#[tokio::test(flavor = "multi_thread")]
async fn a_completed_scan_writes_the_completion_marker() {
    let mut fx = Fixture::new("done-clean");
    let params = fx.completion("done-clean", Ok(summary(42)));
    run_scan_completion(params).await;

    assert!(
        fx.completion_marker().await.is_some(),
        "a clean completion must stamp `scan_completed_at`"
    );
    assert_eq!(
        fx.freshness_now(),
        Some(Freshness::Fresh),
        "a clean completion is authoritative"
    );
}

/// The volume's ground comes back when the scan does, on every outcome.
///
/// This is the whole reason the claim travels into this task instead of living
/// in `start_scan`'s frame: the scan outlives that call, so nothing else CAN
/// release it. Left held, the drive would refuse every later rescan and every
/// search walk for the rest of the session — the wedged-ground failure, which
/// no amount of retrying gets out of.
#[tokio::test(flavor = "multi_thread")]
async fn the_volume_ground_comes_back_when_the_scan_ends() {
    for (volume_id, result) in [
        ("ground-clean", Ok(summary(42))),
        ("ground-cancelled", Err(ScanError::Cancelled(summary(7)))),
        ("ground-failed", Err(ScanError::RootUnlistable)),
    ] {
        let mut fx = Fixture::new(volume_id);
        let params = fx.completion(volume_id, result);
        assert!(
            cover::Claim::take(volume_id, vec!["/".to_string()], cover::Holder::Rewriting)
                .mine()
                .is_empty(),
            "precondition: the scan holds '{volume_id}' while it runs"
        );

        run_scan_completion(params).await;

        let next = cover::Claim::take(volume_id, vec!["/".to_string()], cover::Holder::Rewriting);
        assert_eq!(next.mine(), ["/"], "'{volume_id}' is walkable again");
    }
}

/// A rescan queued behind this scan is RUN by it.
///
/// The queue only works if every whole-volume holder carries the request out on
/// its way, and this is the local scan's half of that. Miss it and a user who
/// pressed "Rescan now" mid-scan is promised a walk that never comes.
#[tokio::test(flavor = "multi_thread")]
async fn a_rescan_queued_behind_a_scan_runs_when_the_scan_ends() {
    let mut fx = Fixture::new("owed-after-scan");
    let params = fx.completion("owed-after-scan", Ok(summary(42)));
    cover::remember_rescan("owed-after-scan");

    run_scan_completion(params).await;

    // The volume isn't registered in this fixture, so the scan attempt itself
    // goes nowhere; what this pins is that the request was carried out of the
    // task rather than left waiting for a holder that is never coming back.
    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the queued rescan to be taken up",
        || !cover::a_rescan_can_start("owed-after-scan"),
    );
}

/// And it runs AFTER the handoff, not where the ground goes back.
///
/// Fired right after the join, the rescan would truncate while this task is
/// still reconciling buffered events and stamping `scan_completed_at` — landing
/// this scan's completion marker on the new scan's half-built index, which is
/// exactly the state that makes the next launch skip the healing rescan. So the
/// whole handoff runs with the request still waiting.
#[tokio::test(flavor = "multi_thread")]
async fn the_handoff_finishes_before_the_queued_rescan_may_truncate() {
    let mut fx = Fixture::new("owed-ordering");
    let params = fx.completion("owed-ordering", Ok(summary(42)));
    cover::remember_rescan("owed-ordering");

    finish_the_scan(params).await;

    assert!(
        fx.completion_marker().await.is_some(),
        "precondition: the handoff wrote the marker"
    );
    assert!(
        cover::take_rescan("owed-ordering"),
        "and the request was still waiting while it did, so nothing could truncate under it"
    );
}

/// The one that strands an index if it ever goes wrong: a stopped scan holds
/// only partial totals, so marking it complete would make the next launch skip
/// the healing rescan and serve a permanently half-built index.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_scan_writes_no_completion_marker() {
    let mut fx = Fixture::new("done-cancelled");
    let params = fx.completion("done-cancelled", Err(ScanError::Cancelled(summary(7))));
    run_scan_completion(params).await;

    assert_eq!(
        fx.completion_marker().await,
        None,
        "a cancelled scan must leave `scan_completed_at` absent so it heals on restart"
    );
}

/// A walk that ran to the end remembers how long each step after it took, in
/// its own walk kind's bucket, so the next run of that kind can show an overall
/// "~X left". The full aggregate the walk queued is the compute step; what's left
/// of the post-walk flush is the save step.
#[tokio::test(flavor = "multi_thread")]
async fn a_completed_scan_remembers_its_step_durations() {
    let mut fx = Fixture::new("steps-clean");
    // What a real walk queues as its last act: the full aggregate.
    fx.writer
        .send(WriteMessage::ComputeAllAggregates {
            source: crate::indexing::writer::AggSource::Sql,
        })
        .expect("queue the aggregate");
    let params = fx.completion("steps-clean", Ok(summary(42)));
    run_scan_completion(params).await;

    fx.writer.flush().await.expect("flush the writer");
    let conn = IndexStore::open_read_connection(&fx.db_path).expect("read connection");
    let steps = IndexStore::read_step_durations(&conn, ScanCalibrationKind::FullWalk).expect("read the steps");
    assert!(
        steps.save_ms.is_some(),
        "the save step's timing must be remembered: {steps:?}"
    );
    assert!(
        steps.compute_ms.is_some(),
        "the compute step's timing must be remembered: {steps:?}"
    );
    assert!(
        steps.catch_up_ms.is_some(),
        "the catch-up step's timing must be remembered: {steps:?}"
    );
    assert_eq!(
        IndexStore::read_step_durations(&conn, ScanCalibrationKind::ChangeCheck).expect("read the steps"),
        StepDurations::default(),
        "a full walk's timings never land in the change check's bucket"
    );
}

/// A stopped walk's save and compute covered a partial tree, so remembering
/// them would undersell the next full run.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_scan_remembers_no_step_durations() {
    let mut fx = Fixture::new("steps-cancelled");
    fx.writer
        .send(WriteMessage::ComputeAllAggregates {
            source: crate::indexing::writer::AggSource::Sql,
        })
        .expect("queue the aggregate");
    let params = fx.completion("steps-cancelled", Err(ScanError::Cancelled(summary(7))));
    run_scan_completion(params).await;

    fx.writer.flush().await.expect("flush the writer");
    let conn = IndexStore::open_read_connection(&fx.db_path).expect("read connection");
    assert_eq!(
        IndexStore::read_step_durations(&conn, ScanCalibrationKind::FullWalk).expect("read the steps"),
        StepDurations::default()
    );
}

/// Splitting the post-walk wait: the aggregate is the compute step, and what's
/// left of the flush is the save step. With no aggregate timing there's nothing
/// to split by, so neither is claimed.
#[test]
fn the_post_walk_wait_splits_into_save_and_compute() {
    assert_eq!(
        split_save_and_compute(60_000, Some(19_000)),
        StepDurations {
            save_ms: Some(41_000),
            compute_ms: Some(19_000),
            catch_up_ms: None,
        }
    );
    // The aggregate can start before the flush does (the writer caught up while
    // the walk was still ending), so the save step bottoms out at zero.
    assert_eq!(split_save_and_compute(5_000, Some(19_000)).save_ms, Some(0));
    assert_eq!(split_save_and_compute(60_000, None), StepDurations::default());
}

/// Cancelled is its own outcome, distinguishable from BOTH neighbours. It
/// isn't a completion (never `Fresh`, no marker) and it isn't a failure
/// (freshness untouched, no `ScanAborted`), and the post-scan handoff still
/// runs so the rows the walk did write are reconciled and served.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancelled_scan_is_neither_a_completion_nor_a_failure() {
    let mut fx = Fixture::new("cancel-not-fail");
    let params = fx.completion("cancel-not-fail", Err(ScanError::Cancelled(summary(7))));
    run_scan_completion(params).await;

    let kinds = fx.events.kinds_for("cancel-not-fail");
    assert!(
        !kinds.contains(&IndexEventKind::ScanAborted),
        "a user-stopped scan must not look like a vanished volume: {kinds:?}"
    );
    assert!(
        kinds.contains(&IndexEventKind::ScanComplete),
        "the post-scan handoff runs for a cancelled walk too (a failure's arm skips it): {kinds:?}"
    );
    // Not `Fresh` (that's a completion) and not `Stale` (that's a failure):
    // a stop leaves freshness exactly where `ScanStarted` put it.
    assert_eq!(
        fx.freshness_now(),
        Some(Freshness::Scanning),
        "a cancelled scan neither claims authority nor reports a failure"
    );
}

/// Failed is not cancelled and not complete: no marker, and freshness drops to
/// Stale so the badge offers a rescan instead of spinning forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_scan_writes_no_marker_and_reports_stale() {
    let mut fx = Fixture::new("done-failed");
    let params = fx.completion("done-failed", Err(ScanError::RootUnlistable));
    run_scan_completion(params).await;

    assert_eq!(
        fx.completion_marker().await,
        None,
        "a failed scan must leave `scan_completed_at` absent"
    );
    assert_eq!(
        fx.freshness_now(),
        Some(Freshness::Stale),
        "a failed scan is honest about being stale"
    );
    assert!(
        fx.events
            .kinds_for("done-failed")
            .contains(&IndexEventKind::ScanAborted),
        "a vanished root clears the stuck scanning row"
    );
}

/// The heal: a scan that runs to the end after a cancelled one stamps the
/// marker, so a stop is recoverable rather than a permanent state.
#[tokio::test(flavor = "multi_thread")]
async fn a_rescan_after_a_cancelled_scan_writes_the_marker() {
    let mut fx = Fixture::new("cancel-then-rescan");
    let cancelled = fx.completion("cancel-then-rescan", Err(ScanError::Cancelled(summary(7))));
    run_scan_completion(cancelled).await;
    assert_eq!(
        fx.completion_marker().await,
        None,
        "precondition: the cancelled scan left no marker"
    );

    let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
    fx.event_rx = Some(rx);
    let rescan = fx.completion("cancel-then-rescan", Ok(summary(42)));
    run_scan_completion(rescan).await;

    assert!(
        fx.completion_marker().await.is_some(),
        "a completed rescan heals the index that was left unmarked"
    );
}

/// The completion task reads the drive while it replays, so it holds its volume
/// from the moment it's spawned, and the live loop it starts holds it from there:
/// the drain never joins the task, and waits on the loop for only five seconds.
#[tokio::test(flavor = "multi_thread")]
async fn the_completion_task_and_its_live_loop_hold_the_volume() {
    let volume_id = "done-holds";
    let mut fx = Fixture::new(volume_id);
    let (finish_the_walk, the_walk_finishes) = std::sync::mpsc::channel::<()>();
    let volume = VolumeWork::for_test(volume_id);
    let mut params = fx.completion(volume_id, Ok(summary(1)));
    params.join_handle = std::thread::spawn(move || {
        let _ = the_walk_finishes.recv();
        Ok(summary(1))
    });
    params.work = volume.child(HoldKind::ScanCompletion);
    let slot = Arc::clone(&params.live_event_task_slot);

    let completion = crate::indexing::host::runtime::spawn(run_scan_completion(params));
    drop(volume);
    assert_eq!(
        hold::wait_until_released(volume_id, std::time::Duration::ZERO),
        hold::Release::StillHeld(vec![(HoldKind::ScanCompletion, 1)]),
        "the task holds the drive while it waits on the walk"
    );

    finish_the_walk.send(()).expect("the walk is still waiting");
    completion.await.expect("the completion task");
    assert_eq!(
        hold::wait_until_released(volume_id, std::time::Duration::ZERO),
        hold::Release::StillHeld(vec![(HoldKind::LiveLoop, 1)]),
        "and the live loop it started holds it from there"
    );

    let live = slot.lock_ignore_poison().take().expect("the task stored its live loop");
    // The fixture holds the watcher's end of the channel the loop reads.
    drop(fx);
    live.await.expect("the live loop ends once its channel closes");
    assert_eq!(
        hold::wait_until_released(volume_id, std::time::Duration::from_secs(5)),
        hold::Release::Released,
        "and lets go as it ends"
    );
}

/// A volume stopped while its scan was finishing gets no replay and no live loop:
/// both would read a drive whose manager is already gone, and nothing would ever
/// drain the loop.
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_volume_gets_no_replay_and_no_live_loop() {
    let volume_id = "done-stopped";
    let mut fx = Fixture::new(volume_id);
    let params = fx.completion(volume_id, Ok(summary(42)));
    let slot = Arc::clone(&params.live_event_task_slot);
    params.work.cancel.cancel();

    run_scan_completion(params).await;

    assert!(
        slot.lock_ignore_poison().is_none(),
        "no live loop was started for a stopped volume"
    );
    assert!(
        !fx.events.events().iter().any(|event| matches!(
            event,
            IndexEvent::PhaseChanged {
                phase: ActivityPhase::Live,
                ..
            }
        )),
        "and it never went live"
    );
    assert_eq!(
        hold::wait_until_released(volume_id, std::time::Duration::ZERO),
        hold::Release::Released,
        "so nothing holds the drive"
    );
}

/// ❗ The completion gate. A walk can run to the END and its drive still be gone by
/// the time anything would be stamped: the walk answers for the reads it made, and
/// this answers for the claims those reads would support.
///
/// Everything the task still claims rides on one presence read — the stamp, the
/// freshness flip, the phase, the live loop — so an index whose drive left comes
/// back unmarked and heals on the next mount, instead of carrying a completion
/// marker over rows a leaving drive may have taken.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide volume-provider slot, and the completion task reads that slot from inside the await: holding it across one IS the point"
)]
async fn a_scan_whose_drive_left_stamps_nothing_and_reports_the_vanish() {
    let _serialized = crate::indexing::handle::test_lock();
    let drive = crate::indexing::host::volumes::MountIdentity::from_raw(0x0100_0077);
    let provider = crate::indexing::host::volumes::FakeVolumeProvider::shared();
    provider.mount("/Volumes/CompletionVanish", drive);
    let _installed = crate::indexing::host::volumes::install_for_test(Arc::clone(&provider) as _);

    let volume_id = "done-vanished";
    let mut fx = Fixture::new(volume_id);
    let mut params = fx.completion(volume_id, Ok(summary(42)));
    // The walk's own generation, captured the way a real `LocalExternal` start
    // captures it — and then the drive goes.
    params.work = VolumeWork::for_test_on(volume_id, drive);
    provider.mark_unmounted("/Volumes/CompletionVanish");

    run_scan_completion(params).await;

    assert_eq!(
        fx.completion_marker().await,
        None,
        "a walk whose drive left must leave `scan_completed_at` absent, whatever its totals say"
    );
    assert_eq!(
        fx.freshness_now(),
        Some(Freshness::Stale),
        "and say so, rather than claiming the index is authoritative"
    );
    let kinds = fx.events.kinds_for(volume_id);
    assert!(
        kinds.contains(&IndexEventKind::ScanAborted),
        "the stuck scanning row comes off something other than a completion: {kinds:?}"
    );
    assert!(
        !kinds.contains(&IndexEventKind::ScanComplete),
        "❌ and nothing announces a scan that completed over a drive that isn't there: {kinds:?}"
    );
}

/// The abort decision fires ONLY for a vanished volume (`RootUnlistable`), so a
/// yanked drive clears its stuck "scanning" row — but a legitimately empty root
/// or a walk panic does NOT abort (the prior index stays visible-stale, no
/// spurious activity clear). Pins the distinguisher the completion arm relies on.
#[test]
fn only_a_vanished_root_triggers_the_scan_abort() {
    assert!(
        scan_failure_is_vanished_volume(&ScanError::RootUnlistable),
        "a vanished (unlistable) root must abort"
    );
    assert!(
        !scan_failure_is_vanished_volume(&ScanError::EmptyRoot),
        "a legitimately empty root must NOT abort"
    );
    assert!(
        !scan_failure_is_vanished_volume(&ScanError::Panicked("boom".to_string())),
        "a walk panic must NOT abort"
    );
    assert!(
        !scan_failure_is_vanished_volume(&ScanError::WriterSend("gone".to_string())),
        "a writer-send failure must NOT abort"
    );
}
