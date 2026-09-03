//! What Pause does to an operation that is still scanning.
//!
//! Pause exists for the moment somebody realizes they picked the wrong
//! destination, and that moment lands during the scan far more often than
//! during the write: the scan is the minutes-long part of a big transfer. So
//! the walk itself has to park, and these tests hold the three claims that make
//! the promise true — a paused scan stops counting, a resumed one carries on
//! from where it stopped rather than starting over, and a cancel reaches a
//! parked walk instead of leaving it hanging on the gate.
//!
//! Every test drives the REAL preview worker over a real tree, with a real
//! operation claiming the preview, because the join between the two (the claim,
//! the gate, the walk's cancel flag) is the whole subject.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use super::event_sinks::{CollectorEventSink, ScanPreviewEventSink};
use super::manager::{PauseOutcome, cancel_operation, manager, pause_operation, resume_operation};
use super::scan_cache::{ScanPreviewState, register_preview, release_preview};
use super::scan_preview::run_scan_preview;
use super::scan_watchdog::{SCAN_INACTIVITY_LIMIT, ScanWatchdog, scan_target_label};
use super::types::{
    LifecycleStatus, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, WriteOperationConfig, WriteOperationType,
};
use super::{OperationEventSink, copy_files_start};
use crate::file_system::listing::{SortColumn, SortOrder};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;
use crate::test_support::{TestDir, wait_until_async};

/// Deadline for every wait here. Generous on purpose: each wait has a real
/// condition, so a healthy run satisfies it in microseconds.
const WAIT: Duration = Duration::from_secs(5);

/// How many files the fixture tree holds. Big enough that a walk paused at its
/// first tick has plenty left to count, so "it stopped" and "it finished" can't
/// be confused for each other.
const TREE_FILES: usize = 400;

fn unique(label: &str) -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("scanpause-{label}-{n}-{:?}", std::thread::current().id())
}

/// A scan-preview sink that pauses the owning operation the first time the walk
/// counts anything.
///
/// Pausing from inside the walk's own progress callback is what makes these
/// tests deterministic: the request lands between two entries of a real walk,
/// which is exactly where a person's click lands, and the walk's very next
/// entry is the one that has to park.
struct PauseOnFirstTick {
    operation_id: String,
    fired: AtomicBool,
    progress: std::sync::Mutex<Vec<ScanPreviewProgressEvent>>,
    complete: std::sync::Mutex<Vec<ScanPreviewCompleteEvent>>,
    cancelled: std::sync::Mutex<Vec<ScanPreviewCancelledEvent>>,
}

impl PauseOnFirstTick {
    fn new(operation_id: String) -> Self {
        Self {
            operation_id,
            fired: AtomicBool::new(false),
            progress: std::sync::Mutex::new(Vec::new()),
            complete: std::sync::Mutex::new(Vec::new()),
            cancelled: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// The highest file count the walk has reported so far.
    fn files_counted(&self) -> usize {
        self.progress
            .lock_ignore_poison()
            .iter()
            .map(|p| p.files_found)
            .max()
            .unwrap_or(0)
    }

    fn is_complete(&self) -> bool {
        !self.complete.lock_ignore_poison().is_empty()
    }
}

impl ScanPreviewEventSink for PauseOnFirstTick {
    fn emit_progress(&self, event: ScanPreviewProgressEvent) {
        self.progress.lock_ignore_poison().push(event);
        if !self.fired.swap(true, Ordering::SeqCst) {
            assert_eq!(
                pause_operation(&self.operation_id),
                PauseOutcome::Applied,
                "an operation whose scan can park is an ordinary pause, not a special case"
            );
        }
    }
    fn emit_complete(&self, event: ScanPreviewCompleteEvent) {
        self.complete.lock_ignore_poison().push(event);
    }
    fn emit_error(&self, _event: ScanPreviewErrorEvent) {}
    fn emit_cancelled(&self, event: ScanPreviewCancelledEvent) {
        self.cancelled.lock_ignore_poison().push(event);
    }
}

/// A tree of [`TREE_FILES`] files across a handful of directories.
fn build_tree(root: &std::path::Path) {
    for bucket in 0..8 {
        let dir = root.join(format!("d{bucket}"));
        std::fs::create_dir_all(&dir).expect("create bucket");
        for n in 0..(TREE_FILES / 8) {
            std::fs::write(dir.join(format!("f{n}.bin")), b"xyz").expect("write file");
        }
    }
}

/// A confirmed copy whose preview is still walking, with the real local walk
/// running behind it and a sink that pauses the operation at the first tick.
///
/// Returns the operation id, the preview id, the sink, and the walk's thread
/// handle so a test can prove the walk actually ended.
struct RunningScan {
    operation_id: String,
    preview_id: String,
    sink: Arc<PauseOnFirstTick>,
    walk: std::thread::JoinHandle<()>,
    op_events: Arc<CollectorEventSink>,
    _dir: TestDir,
}

async fn start_copy_with_a_real_scan(label: &str) -> RunningScan {
    let dir = TestDir::new(label);
    let src = dir.join("src");
    let dst = dir.join("dst");
    build_tree(&src);
    std::fs::create_dir_all(&dst).expect("create dst");

    let preview_id = unique("preview");
    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        // Every entry ticks, so the pause lands at the walk's first entry and
        // the counts it reports are the walk's real progress rather than a
        // 200 ms sample of it.
        progress_interval: Duration::ZERO,
    });
    register_preview(preview_id.clone(), Arc::clone(&state));

    let op_events = Arc::new(CollectorEventSink::new());
    let start = copy_files_start(
        Arc::clone(&op_events) as Arc<dyn OperationEventSink>,
        vec![src.clone()],
        dst,
        WriteOperationConfig {
            preview_id: Some(preview_id.clone()),
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");
    let operation_id = start.operation_id.clone();

    let sink = Arc::new(PauseOnFirstTick::new(operation_id.clone()));
    let events: Arc<dyn ScanPreviewEventSink> = Arc::clone(&sink) as Arc<dyn ScanPreviewEventSink>;
    let watchdog = ScanWatchdog::start(
        preview_id.clone(),
        scan_target_label(std::slice::from_ref(&src), "root"),
        SCAN_INACTIVITY_LIMIT,
        Arc::clone(&state),
        Arc::clone(&events),
    );

    let walk_preview_id = preview_id.clone();
    let walk = std::thread::spawn(move || {
        run_scan_preview(
            events,
            walk_preview_id,
            vec![src],
            SortColumn::Name,
            SortOrder::Ascending,
            state,
            false,
            watchdog,
        );
    });

    RunningScan {
        operation_id,
        preview_id,
        sink,
        walk,
        op_events,
        _dir: dir,
    }
}

/// Waits for the walk to go quiet: two reads of the same count with a real
/// beat between them. A parked walk produces nothing, so the count stops
/// moving; a running one over 400 files moves within microseconds.
async fn wait_until_parked(sink: &PauseOnFirstTick) -> usize {
    let mut settled = 0usize;
    for _ in 0..100 {
        let before = sink.files_counted();
        // allowed-test-sleep: the claim is that the walk STOPS producing, and
        // "stopped" can only be read as "nothing more arrived across a span".
        tokio::time::sleep(Duration::from_millis(20)).await;
        if before == sink.files_counted() && before > 0 {
            settled = before;
            break;
        }
    }
    assert!(settled > 0, "the walk must have counted something before it parked");
    settled
}

/// The bug: pausing during the scan told the user it would take effect and then
/// never did, so an 80,000-file move ran to completion after the click. The
/// walk has to park where it stands.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_paused_scan_stops_counting() {
    let scan = start_copy_with_a_real_scan("scanpause-stops").await;

    let parked_at = wait_until_parked(&scan.sink).await;
    assert!(
        parked_at < TREE_FILES,
        "the walk must park mid-tree, not after counting all {TREE_FILES} files (stopped at {parked_at})"
    );
    assert!(
        !scan.sink.is_complete(),
        "a parked walk has not finished, so no preview result may be published"
    );
    assert_eq!(
        manager().lifecycle_status(&scan.operation_id),
        Some(LifecycleStatus::Paused),
        "and the record says Paused, because now it truly is"
    );

    resume_operation(&scan.operation_id);
    wait_until_async(WAIT, "the resumed scan to finish", || scan.sink.is_complete()).await;
    scan.walk.join().expect("the walk thread ends");
    release_preview(&scan.preview_id);
    cancel_operation(&scan.operation_id);
    wait_until_async(WAIT, "the operation to settle", || {
        manager().lifecycle_status(&scan.operation_id).is_none()
    })
    .await;
    drop(scan.op_events);
}

/// Resume carries on from where the walk stopped. A scan that restarted would
/// re-walk everything it had already counted, which on a slow share is the
/// whole cost of the pause paid twice — and it would show up as a count that
/// dropped back toward zero.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_resumed_scan_carries_on_instead_of_starting_over() {
    let scan = start_copy_with_a_real_scan("scanpause-resumes").await;
    let parked_at = wait_until_parked(&scan.sink).await;

    resume_operation(&scan.operation_id);
    wait_until_async(WAIT, "the resumed scan to finish", || scan.sink.is_complete()).await;
    scan.walk.join().expect("the walk thread ends");

    let counts: Vec<usize> = scan
        .sink
        .progress
        .lock_ignore_poison()
        .iter()
        .map(|p| p.files_found)
        .collect();
    assert!(
        counts.windows(2).all(|w| w[1] >= w[0]),
        "a resumed walk continues; a count that fell back means it started over: {counts:?}"
    );
    let completed = scan.sink.complete.lock_ignore_poison()[0].files_total;
    assert_eq!(
        completed, TREE_FILES,
        "and the finished scan describes the whole tree exactly once"
    );
    assert!(
        parked_at < completed,
        "the pause has to have interrupted real work for this to mean anything"
    );

    release_preview(&scan.preview_id);
    cancel_operation(&scan.operation_id);
    wait_until_async(WAIT, "the operation to settle", || {
        manager().lifecycle_status(&scan.operation_id).is_none()
    })
    .await;
    drop(scan.op_events);
}

/// Cancellation wins over pause, and a parked scan is the case where getting
/// that wrong hangs a thread forever: the walk sits on the gate, the cancel
/// never touches the pause flag, and nothing else ever wakes it. The operation
/// has to end and the walk thread has to come back.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancel_reaches_a_scan_parked_on_the_pause_gate() {
    let scan = start_copy_with_a_real_scan("scanpause-cancel").await;
    wait_until_parked(&scan.sink).await;

    cancel_operation(&scan.operation_id);

    wait_until_async(WAIT, "the cancelled operation to settle", || {
        !scan.op_events.cancelled.lock_ignore_poison().is_empty()
    })
    .await;
    let cancelled = scan.op_events.cancelled.lock_ignore_poison();
    assert_eq!(cancelled[0].operation_type, WriteOperationType::Copy);
    assert_eq!(
        cancelled[0].files_processed, 0,
        "a scan that never finished wrote nothing"
    );
    drop(cancelled);

    scan.walk
        .join()
        .expect("the walk thread must come back, not stay parked on a gate nobody will open");
    assert!(
        !scan.sink.is_complete(),
        "a cancelled walk publishes no result for an operation that is already over"
    );
    release_preview(&scan.preview_id);
}

/// An operation event sink that pauses the operation the first time it reports
/// scanning progress, and counts how far the scan got.
///
/// The distinction it exists to draw: a paused operation that finishes its scan
/// and then parks at the driver's between-files gate looks identical from the
/// outside to one whose scan parked. The scanning-phase counts are what tell
/// them apart.
struct PauseOnFirstScanTick {
    operation_id: std::sync::Mutex<Option<String>>,
    fired: AtomicBool,
    scanned: std::sync::Mutex<Vec<usize>>,
    complete: AtomicBool,
}

impl PauseOnFirstScanTick {
    fn new() -> Self {
        Self {
            operation_id: std::sync::Mutex::new(None),
            fired: AtomicBool::new(false),
            scanned: std::sync::Mutex::new(Vec::new()),
            complete: AtomicBool::new(false),
        }
    }

    fn files_scanned(&self) -> usize {
        self.scanned.lock_ignore_poison().last().copied().unwrap_or(0)
    }
}

impl OperationEventSink for PauseOnFirstScanTick {
    fn emit_progress(&self, event: super::types::WriteProgressEvent) {
        if event.phase != super::types::WriteOperationPhase::Scanning {
            return;
        }
        self.scanned.lock_ignore_poison().push(event.files_done);
        if self.fired.swap(true, Ordering::SeqCst) {
            return;
        }
        let id = self.operation_id.lock_ignore_poison().clone();
        if let Some(id) = id {
            assert_eq!(pause_operation(&id), PauseOutcome::Applied);
        }
    }
    fn emit_complete(&self, _event: super::types::WriteCompleteEvent) {
        self.complete.store(true, Ordering::SeqCst);
    }
    fn emit_cancelled(&self, _event: super::types::WriteCancelledEvent) {}
    fn emit_error(&self, _event: super::types::WriteErrorEvent) {}
    fn emit_conflict(&self, _event: super::types::WriteConflictEvent) {}
    fn emit_conflict_resolved(&self, _event: super::types::WriteConflictResolvedEvent) {}
    fn emit_source_item_done(&self, _event: super::types::WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _event: super::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _conflict: super::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _result: super::types::DryRunResult) {}
    fn emit_settled(&self, _event: super::types::WriteSettledEvent) {}
}

/// The scan an operation does for ITSELF (no preview to consume: an evicted id,
/// a stale one from a reloaded window, or a second operation over the same
/// sources) parks on the same gate. Without this the record would say Paused
/// while the operation walked the tree at full speed, which is the same lie in
/// a different place.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_operations_own_scan_parks_too() {
    let dir = TestDir::new("scanpause-ownscan");
    let src = dir.join("src");
    let dst = dir.join("dst");
    build_tree(&src);
    std::fs::create_dir_all(&dst).expect("create dst");

    let sink = Arc::new(PauseOnFirstScanTick::new());
    let start = copy_files_start(
        Arc::clone(&sink) as Arc<dyn OperationEventSink>,
        vec![src.clone()],
        dst,
        WriteOperationConfig {
            // No preview: the operation does its own walk. Every entry ticks,
            // so the pause lands between two entries of that walk.
            progress_interval_ms: 0,
            ..WriteOperationConfig::default()
        },
        vec![unique("vol")],
        None,
        Initiator::User,
        None,
    )
    .await
    .expect("copy starts");
    let op_id = start.operation_id.clone();
    *sink.operation_id.lock_ignore_poison() = Some(op_id.clone());

    // The walk stops climbing: two reads of the same count with a beat between
    // them, which a walk over 400 files would blow past in microseconds.
    let mut parked_at = 0usize;
    for _ in 0..100 {
        let before = sink.files_scanned();
        // allowed-test-sleep: "stopped" can only be read as "nothing more
        // arrived across a span".
        tokio::time::sleep(Duration::from_millis(20)).await;
        if before == sink.files_scanned() && before > 0 {
            parked_at = before;
            break;
        }
    }
    assert!(parked_at > 0, "the scan must have counted something before parking");
    assert!(
        parked_at < TREE_FILES,
        "the scan itself has to park mid-tree; finishing all {TREE_FILES} files and parking at the \
         driver's gate afterwards is the behavior this test exists to rule out (stopped at {parked_at})"
    );
    assert!(!sink.complete.load(Ordering::SeqCst));

    resume_operation(&op_id);
    wait_until_async(WAIT, "the resumed copy to finish", || {
        sink.complete.load(Ordering::SeqCst)
    })
    .await;
    let copied: Vec<PathBuf> = std::fs::read_dir(dir.join("dst"))
        .expect("dst readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    assert!(!copied.is_empty(), "and resuming copies the tree");
}
