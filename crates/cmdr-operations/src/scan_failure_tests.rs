//! A scan preview that stops on a missing or unreadable item fails the
//! operation waiting on it with a typed error that NAMES that item.
//!
//! `ERR-VETBX`'s first attempt reached the user as `Copy error: Path: ; Error:
//! No such file or directory`: the walk knew which file it couldn't open, the
//! preview flattened that into a bare string, and the operation rebuilt an
//! `IoError` with an empty path around it. These drive the real preview workers
//! into the real scan-wait, so the whole hand-off is what's under test.

use crate::Operations;
use crate::testing::TestOperations;
use cmdr_fs::entry::FileEntry;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::event_sinks::{CollectorEventSink, CollectorScanPreviewSink, ScanPreviewEventSink};
use super::scan_bridge_tests::start_copy_awaiting_preview;
use super::scan_cache::ScanPreviewState;
use super::scan_preview::{run_scan_preview, run_volume_scan_preview};
use super::scan_watchdog::{ScanTarget, ScanWatchdog};
use super::types::WriteOperationError;
use cmdr_fs::testing::{WedgedVolume, wait_until_async};
use cmdr_fs::volume::{
    BatchScanResult, CopyScanResult, InMemoryVolume, ListingProgress, ScanBoundary, Volume, VolumeError,
};

const WAIT: Duration = Duration::from_secs(5);

/// A volume whose every source has vanished: each read answers `NotFound`
/// naming the path it was asked about, the way SMB and MTP do.
struct VanishedVolume {
    inner: InMemoryVolume,
}

impl Volume for VanishedVolume {
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
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { false })
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn scan_for_copy<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<CopyScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        _boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let first = paths.first().map(|p| p.display().to_string()).unwrap_or_default();
            Err(VolumeError::NotFound(first))
        })
    }
}

fn fresh_state() -> Arc<ScanPreviewState> {
    Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(50),
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    })
}

fn start_watchdog(
    engine: &TestOperations,
    preview_id: &str,
    sources: &[PathBuf],
    limit: Duration,
) -> Arc<ScanWatchdog> {
    ScanWatchdog::start(
        engine,
        preview_id.to_string(),
        ScanTarget::of(sources, "test"),
        limit,
        fresh_state(),
        Arc::new(CollectorScanPreviewSink::new()) as Arc<dyn ScanPreviewEventSink>,
    )
}

/// The one error the waiting copy reported.
async fn the_operations_error(events: &CollectorEventSink) -> WriteOperationError {
    wait_until_async(WAIT, "the write-error event", || {
        !events.errors.lock().expect("collector mutex").is_empty()
    })
    .await;
    let errors = events.errors.lock().expect("collector mutex");
    errors.first().expect("one error").error.clone()
}

/// The local walk: a selected item that's gone by the time the walk reaches it
/// is `SourceNotFound` for THAT item, so the user reads the friendly "couldn't
/// find" copy with the file named, not a bare errno sentence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_walk_that_finds_its_source_gone_names_it() {
    let engine = TestOperations::new();
    let (events, _op_id, preview_id, dir) = start_copy_awaiting_preview(&engine, "scanfail-local").await;
    let missing = dir.join("src").join("gone.jpg");
    let sources = vec![missing.clone()];
    let watchdog = start_watchdog(&engine, &preview_id, &sources, Duration::from_secs(600));

    let walk_id = preview_id.clone();
    let walker = Operations::clone(&engine);
    tokio::task::spawn_blocking(move || {
        run_scan_preview(
            &walker,
            Arc::new(CollectorScanPreviewSink::new()),
            walk_id,
            sources,
            fresh_state(),
            crate::ScanPreviewPurpose::Transfer,
            watchdog,
        );
    })
    .await
    .expect("the walk thread");

    let error = the_operations_error(&events).await;
    assert!(
        matches!(&error, WriteOperationError::SourceNotFound { path } if Path::new(path) == missing),
        "the operation must fail naming the item the walk couldn't find, got {error:?}"
    );
}

/// A volume walk: the backend's typed `NotFound` survives to the operation, with
/// the path the backend named, rather than being flattened into a string.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_walk_that_finds_its_source_gone_names_it() {
    let engine = TestOperations::new();
    let (events, _op_id, preview_id, _dir) = start_copy_awaiting_preview(&engine, "scanfail-volume").await;
    let missing = PathBuf::from("/share/fotók/retusált.jpg");
    let sources = vec![missing.clone()];
    let watchdog = start_watchdog(&engine, &preview_id, &sources, Duration::from_secs(600));

    run_volume_scan_preview(
        engine.clone(),
        Arc::new(CollectorScanPreviewSink::new()),
        preview_id,
        sources,
        Arc::new(VanishedVolume {
            inner: InMemoryVolume::new("Vanished"),
        }) as Arc<dyn Volume>,
        String::from("scanfail-vanished"),
        fresh_state(),
        crate::ScanPreviewPurpose::Transfer,
        watchdog,
    )
    .await;

    let error = the_operations_error(&events).await;
    assert!(
        matches!(&error, WriteOperationError::SourceNotFound { path } if Path::new(path) == missing),
        "the backend's typed miss must reach the operation with its path, got {error:?}"
    );
}

/// A walk the watchdog gave up on still names what it was walking, so the
/// failure says where the volume stopped answering.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_walk_that_stops_answering_names_what_it_was_walking() {
    let engine = TestOperations::new();
    let (events, _op_id, preview_id, _dir) = start_copy_awaiting_preview(&engine, "scanfail-wedged").await;
    let source = PathBuf::from("/share/folder");
    let sources = vec![source.clone()];
    let state = fresh_state();
    let sink: Arc<dyn ScanPreviewEventSink> = Arc::new(CollectorScanPreviewSink::new());
    let watchdog = ScanWatchdog::start(
        &engine,
        preview_id.clone(),
        ScanTarget::of(&sources, "wedged"),
        Duration::from_millis(200),
        Arc::clone(&state),
        Arc::clone(&sink),
    );
    let walk = tokio::spawn(run_volume_scan_preview(
        engine.clone(),
        sink,
        preview_id,
        sources,
        Arc::new(WedgedVolume::new("Wedged")) as Arc<dyn Volume>,
        String::from("wedged"),
        state,
        crate::ScanPreviewPurpose::Transfer,
        watchdog,
    ));

    let error = the_operations_error(&events).await;
    walk.abort();
    assert!(
        matches!(&error, WriteOperationError::IoError { path, .. } if Path::new(path) == source),
        "a timed-out walk must name what it was walking, got {error:?}"
    );
}
