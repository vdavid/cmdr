//! What happens to a scan preview whose volume stops answering.
//!
//! The fixture is a volume whose scan future never resolves, which is what a
//! wedged mount looks like from here: no error, no cancel, no progress, no
//! return. Reproducing it with a real network drop isn't repeatable; a volume
//! that never answers is exactly repeatable and hits the same code.

use crate::Operations;
use crate::testing::TestOperations;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use uuid::Uuid;

use super::event_sinks::{CollectorScanPreviewSink, ScanPreviewEventSink};
use super::scan_cache::{ScanOutcome, ScanPreviewState};
use super::scan_preview::{run_scan_preview, run_volume_scan_preview};
use super::scan_watchdog::{ScanTally, ScanTarget, ScanWatchdog};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, WedgedVolume, wait_until_async};
use cmdr_fs::volume::Volume;

/// How long a test waits for the watchdog to publish. Generous against the
/// 200 ms limits below: these run on a loaded box.
const WAIT: Duration = Duration::from_secs(5);

/// The inactivity budget the fixtures use, short enough to keep the suite fast.
/// A cell that must NOT time out while feeding the clock needs a wider one: a
/// stalled test thread fakes a timeout at this size.
const TEST_LIMIT: Duration = Duration::from_millis(200);

/// A preview registered in flight on `engine`, with its cancel flag and progress interval.
fn in_flight_preview(engine: &TestOperations) -> (String, Arc<ScanPreviewState>) {
    let preview_id = format!("watchdog-{}", Uuid::new_v4());
    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(50),
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    });
    engine
        .stores()
        .previews()
        .register_preview(preview_id.clone(), Arc::clone(&state));
    (preview_id, state)
}

/// The bug this suite exists for: the walk never comes back, and before the
/// watchdog the preview stayed in flight forever, so the dialog spun with no
/// counts and no way out, and a confirmed transfer waiting on the same preview
/// waited with it.
#[tokio::test]
async fn a_volume_that_never_answers_still_settles_its_preview() {
    let engine = TestOperations::new();
    let (preview_id, state) = in_flight_preview(&engine);
    let sources = vec![PathBuf::from("/share/folder")];
    let events = Arc::new(CollectorScanPreviewSink::new());
    let sink: Arc<dyn ScanPreviewEventSink> = Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>;
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&sources, "wedged"),
        TEST_LIMIT,
        Arc::clone(&state),
        Arc::clone(&sink),
    );

    let walk = tokio::spawn(run_volume_scan_preview(
        engine.clone(),
        sink,
        preview_id.clone(),
        sources,
        Arc::new(WedgedVolume::new("Wedged")) as Arc<dyn Volume>,
        String::from("wedged"),
        state,
        crate::ScanPreviewPurpose::Transfer,
        watchdog,
    ));

    let id = preview_id.clone();
    wait_until_async(WAIT, "the preview to settle", || {
        matches!(engine.stores().previews().poll_claim(&id), Some(ScanOutcome::Error(_)))
    })
    .await;

    let errors = events.errors.lock_ignore_poison();
    assert_eq!(errors.len(), 1, "the dialog is told exactly once");
    assert!(
        errors[0].timed_out,
        "the dialog needs the typed flag to say 'not responding' rather than a bare failure"
    );

    walk.abort();
    engine.stores().previews().release_preview(&preview_id);
}

/// A slow-but-alive walk must NOT be killed: as long as entries keep landing,
/// the budget resets. This is the test that stops the bound from turning a big
/// SMB tree into a false timeout.
#[tokio::test]
async fn a_walk_that_keeps_counting_is_left_alone() {
    let engine = TestOperations::new();
    let (preview_id, state) = in_flight_preview(&engine);
    let events = Arc::new(CollectorScanPreviewSink::new());
    // Wider than `TEST_LIMIT`, and fed well inside it: this cell's feed is a test
    // thread, and at 200 ms fed every 100 ms, a 100 ms stall under load let the
    // watchdog fire between two counts.
    let budget = Duration::from_secs(1);
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&[PathBuf::from("/Volumes/a slow share")], "test"),
        budget,
        state,
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    // Three budgets' worth of wall clock, fed a count every tenth of a budget.
    for tick in 1..=30u64 {
        // allowed-test-sleep: the WAIT is the subject. The claim is that a walk
        // that keeps counting is never cut off, which can only be shown by letting
        // several budgets of real time pass between counts.
        tokio::time::sleep(budget / 10).await;
        watchdog.note_progress(tick as usize, 0, tick * 1_024);
    }

    assert!(
        engine.stores().previews().poll_claim(&preview_id).is_none(),
        "a walk that is still counting has not settled"
    );
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "and the dialog was told nothing"
    );

    engine.stores().previews().release_preview(&preview_id);
}

/// A PAUSED walk counts nothing, which looks exactly like a volume that stopped
/// answering. Without the parked flag, holding Pause for longer than the budget
/// would end the scan with "stopped responding" — the user's own click reported
/// back to them as a dead share.
#[tokio::test]
async fn a_walk_parked_on_a_pause_is_never_called_unresponsive() {
    let engine = TestOperations::new();
    let (preview_id, state) = in_flight_preview(&engine);
    let events = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&[PathBuf::from("/Volumes/a share someone paused")], "test"),
        TEST_LIMIT,
        state,
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    watchdog.note_progress(7, 1, 1_024);
    watchdog.note_parked();

    // allowed-test-sleep: the claim is that nothing happens across several
    // budgets' worth of silence, and silence is the whole input.
    tokio::time::sleep(TEST_LIMIT * 5).await;

    assert!(
        engine.stores().previews().poll_claim(&preview_id).is_none(),
        "a parked walk has not settled: the person is still deciding"
    );
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "and nothing told the dialog the volume died"
    );

    // Resuming restarts the clock rather than resuming it mid-flight, so the
    // pause itself is never charged against the budget.
    watchdog.note_resumed();
    // allowed-test-sleep: just under a budget, which must not fire on its own.
    tokio::time::sleep(TEST_LIMIT / 2).await;
    assert!(events.errors.lock_ignore_poison().is_empty());

    engine.stores().previews().release_preview(&preview_id);
}

/// The race: a walk that finishes at the same moment the watchdog gives up. One
/// of them publishes; the loser stays quiet. Here the WORKER wins, so no timeout
/// may reach the dialog afterwards.
#[tokio::test]
async fn a_worker_that_claims_first_keeps_the_watchdog_quiet() {
    let engine = TestOperations::new();
    let (preview_id, state) = in_flight_preview(&engine);
    let events = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&[PathBuf::from("/Volumes/a share that answered just in time")], "test"),
        TEST_LIMIT,
        state,
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    assert!(watchdog.claim_outcome(), "the worker takes the outcome first");

    // allowed-test-sleep: the point is that nothing happens across a span in which
    // the watchdog woke, saw the claim, and stopped. There's no condition to wait
    // on, because a correct watchdog produces no event at all.
    tokio::time::sleep(TEST_LIMIT * 4).await;

    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "the watchdog must not contradict an outcome the worker already owns"
    );
    assert!(
        !watchdog.claim_outcome(),
        "and the claim is one-shot, so nothing else can publish either"
    );

    engine.stores().previews().release_preview(&preview_id);
}

/// The settle line is the first thing anyone reads when triaging a scan, and it
/// reports the totals the walk really produced.
///
/// The watchdog's own counters come from the walk's progress tick, which only
/// fires on the `fileOperations.progressUpdateInterval` timer. A scan finishing
/// inside one interval never ticks, so reporting those counters at completion
/// said `0 files, 0 dirs, 0 bytes` over a fully walked tree — which is what
/// every scan preview in production logged, three-file folders included.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_preview_that_finishes_before_its_first_tick_still_reports_what_it_walked() {
    let engine = TestOperations::new();
    let dir = TestDir::new("watchdog_settle_counts");
    let tree = dir.join("tree");
    std::fs::create_dir(&tree).expect("the scratch tree");
    for (name, bytes) in [("a.txt", 11usize), ("b.txt", 22), ("c.txt", 33)] {
        std::fs::write(tree.join(name), vec![b'x'; bytes]).expect("a scratch file");
    }

    let preview_id = format!("watchdog-{}", Uuid::new_v4());
    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        // Far longer than the walk, so no progress tick can fire: the production
        // shape this covers.
        progress_interval: Duration::from_secs(600),
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    });
    engine
        .stores()
        .previews()
        .register_preview(preview_id.clone(), Arc::clone(&state));

    let sources = vec![tree.clone()];
    let events = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&sources, "root"),
        // Generous: this walk is meant to complete, not to be given up on.
        Duration::from_secs(600),
        Arc::clone(&state),
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    let walk_watchdog = Arc::clone(&watchdog);
    let walk_events: Arc<dyn ScanPreviewEventSink> = Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>;
    let walk_id = preview_id.clone();
    let walker = Operations::clone(&engine);
    tokio::task::spawn_blocking(move || {
        run_scan_preview(
            &walker,
            walk_events,
            walk_id,
            sources,
            state,
            crate::ScanPreviewPurpose::Transfer,
            walk_watchdog,
        );
    })
    .await
    .expect("the walk thread");

    let complete = events.complete.lock_ignore_poison();
    let [completed] = complete.as_slice() else {
        panic!("expected exactly one complete event, got {}", complete.len());
    };
    assert_eq!(completed.files_total, 3, "the walk really did count the tree");
    assert_eq!(
        watchdog.walked(),
        ScanTally {
            files: completed.files_total,
            dirs: completed.dirs_total,
            bytes: completed.bytes_total,
        },
        "the settle line reports what the dialog was told, not an untouched tick counter"
    );

    drop(complete);
    engine.stores().previews().release_preview(&preview_id);
}
