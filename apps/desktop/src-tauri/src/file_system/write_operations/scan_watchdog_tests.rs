//! What happens to a scan preview whose volume stops answering.
//!
//! The fixture is a volume whose scan future never resolves, which is what a
//! wedged mount looks like from here: no error, no cancel, no progress, no
//! return. Reproducing it with a real network drop isn't repeatable; a volume
//! that never answers is exactly repeatable and hits the same code.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use uuid::Uuid;

use super::event_sinks::{CollectorScanPreviewSink, ScanPreviewEventSink};
use super::scan_cache::{ScanOutcome, ScanPreviewState, poll_claim, register_preview, release_preview};
use super::scan_preview::run_volume_scan_preview;
use super::scan_watchdog::{ScanWatchdog, scan_target_label};
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ListingProgress, ScanConflict, SourceItemInfo, Volume, VolumeError,
};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until_async;

/// How long a test waits for the watchdog to publish. Generous against the
/// 200 ms limits below: these run on a loaded box.
const WAIT: Duration = Duration::from_secs(5);

/// The inactivity budget the fixtures use, short enough to keep the suite fast
/// and long enough that a stalled CI thread can't fake a timeout.
const TEST_LIMIT: Duration = Duration::from_millis(200);

/// A volume whose copy scan never answers: every scan future parks forever, the
/// way a syscall on a wedged mount does. Everything else delegates to an
/// in-memory volume so the fixture is a real `Volume`, not a panic trap.
struct WedgedVolume {
    inner: InMemoryVolume,
}

impl WedgedVolume {
    fn new() -> Self {
        Self {
            inner: InMemoryVolume::new("Wedged"),
        }
    }
}

impl Volume for WedgedVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = (path, on_progress);
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = path;
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = path;
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move {
            let _ = path;
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = path;
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn scan_for_copy_batch_with_progress<'a>(
        &'a self,
        paths: &'a [PathBuf],
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = (paths, on_progress);
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }

    fn scan_for_conflicts<'a>(
        &'a self,
        items: &'a [SourceItemInfo],
        dest: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ScanConflict>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let _ = (items, dest);
            std::future::pending::<()>().await;
            unreachable!("a wedged volume never answers")
        })
    }
}

/// A preview registered in flight, with its cancel flag and progress interval.
fn in_flight_preview() -> (String, Arc<ScanPreviewState>) {
    let preview_id = format!("watchdog-{}", Uuid::new_v4());
    let state = Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(50),
    });
    register_preview(preview_id.clone(), Arc::clone(&state));
    (preview_id, state)
}

/// The bug this suite exists for: the walk never comes back, and before the
/// watchdog the preview stayed in flight forever, so the dialog spun with no
/// counts and no way out, and a confirmed transfer waiting on the same preview
/// waited with it.
#[tokio::test]
async fn a_volume_that_never_answers_still_settles_its_preview() {
    let (preview_id, state) = in_flight_preview();
    let sources = vec![PathBuf::from("/share/folder")];
    let events = Arc::new(CollectorScanPreviewSink::new());
    let sink: Arc<dyn ScanPreviewEventSink> = Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>;
    let watchdog = ScanWatchdog::start(
        preview_id.clone(),
        scan_target_label(&sources, "wedged"),
        TEST_LIMIT,
        Arc::clone(&state),
        Arc::clone(&sink),
    );

    let walk = tokio::spawn(run_volume_scan_preview(
        sink,
        preview_id.clone(),
        sources,
        Arc::new(WedgedVolume::new()) as Arc<dyn Volume>,
        String::from("wedged"),
        state,
        watchdog,
    ));

    let id = preview_id.clone();
    wait_until_async(WAIT, "the preview to settle", move || {
        matches!(poll_claim(&id), Some(ScanOutcome::Error(_)))
    })
    .await;

    let errors = events.errors.lock_ignore_poison();
    assert_eq!(errors.len(), 1, "the dialog is told exactly once");
    assert!(
        errors[0].timed_out,
        "the dialog needs the typed flag to say 'not responding' rather than a bare failure"
    );

    walk.abort();
    release_preview(&preview_id);
}

/// A slow-but-alive walk must NOT be killed: as long as entries keep landing,
/// the budget resets. This is the test that stops the bound from turning a big
/// SMB tree into a false timeout.
#[tokio::test]
async fn a_walk_that_keeps_counting_is_left_alone() {
    let (preview_id, state) = in_flight_preview();
    let events = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        preview_id.clone(),
        String::from("a slow share"),
        TEST_LIMIT,
        state,
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    // Six budgets' worth of wall clock, fed a count every half budget.
    for tick in 1..=12u64 {
        tokio::time::sleep(TEST_LIMIT / 2).await;
        watchdog.note_progress(tick as usize, 0, tick * 1_024);
    }

    assert!(
        poll_claim(&preview_id).is_none(),
        "a walk that is still counting has not settled"
    );
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "and the dialog was told nothing"
    );

    release_preview(&preview_id);
}

/// The race: a walk that finishes at the same moment the watchdog gives up. One
/// of them publishes; the loser stays quiet. Here the WORKER wins, so no timeout
/// may reach the dialog afterwards.
#[tokio::test]
async fn a_worker_that_claims_first_keeps_the_watchdog_quiet() {
    let (preview_id, state) = in_flight_preview();
    let events = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        preview_id.clone(),
        String::from("a share that answered just in time"),
        TEST_LIMIT,
        state,
        Arc::clone(&events) as Arc<dyn ScanPreviewEventSink>,
    );

    assert!(watchdog.claim_outcome(), "the worker takes the outcome first");

    // Well past the budget: the watchdog has woken, seen the claim, and stopped.
    tokio::time::sleep(TEST_LIMIT * 4).await;

    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "the watchdog must not contradict an outcome the worker already owns"
    );
    assert!(
        !watchdog.claim_outcome(),
        "and the claim is one-shot, so nothing else can publish either"
    );

    release_preview(&preview_id);
}
