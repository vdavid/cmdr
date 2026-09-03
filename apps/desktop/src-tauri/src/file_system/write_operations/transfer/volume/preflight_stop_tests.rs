//! Cancel and Pause during an operation's OWN volume scan.
//!
//! This is the scan a programmatic or MCP-started transfer takes: no dialog ran
//! ahead of it, so there is no cached preview to consume and `preflight.rs` walks
//! the sources itself. It used to be one `Volume` call over every source with no
//! seam of any kind, so a person watching "Scanning…" over a cold NAS share could
//! press Cancel and wait out the entire walk anyway — minutes, on the tree size
//! that makes anyone want to cancel.
//!
//! The cancel is wired to the SCAN's own progress rather than a wall clock, the
//! same way the rename-merge suites wire theirs: a `Volume` double fires
//! `cancel_write_operation` the instant the walk crosses its third entry, so the
//! walk is provably mid-flight when the boundary has to answer.

use super::preflight::scan_volume_sources;
use super::rename_merge_test_support::make_state;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{BatchScanResult, ScanBoundary, Volume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::{WriteOperationState, cancel_write_operation};
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    ConflictResolution, VolumeCopyConfig, WriteOperationError, WriteOperationType,
};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Entries the double's walk would cross if nothing stopped it. Far more than the
/// trip point, so "stopped where it was told" and "ran to the end" can't be
/// confused.
const ENTRIES: usize = 1_000;

/// The entry the double acts on. Not the first: a boundary that only works before
/// any work has happened is the check `preflight.rs` already had.
const TRIP_AT: usize = 3;

/// What the double does when its walk reaches [`TRIP_AT`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum TripAction {
    Cancel,
    Pause,
}

/// A backend whose batch scan walks [`ENTRIES`] entries through the boundary it
/// was handed, and cancels or pauses the operation as it crosses [`TRIP_AT`].
///
/// Every real backend's walk has a shape of its own (SMB recurses, MTP groups by
/// parent, local drives `WalkDir`), and each is pinned where it lives. What this
/// double stands for is the one thing they share and the one thing `preflight.rs`
/// owes them: a boundary that carries the operation's Cancel and Pause into
/// whatever the backend is doing.
struct BoundaryWalkingVolume {
    state: Arc<WriteOperationState>,
    operation_id: String,
    action: TripAction,
    walked: AtomicUsize,
}

impl BoundaryWalkingVolume {
    fn new(state: &Arc<WriteOperationState>, operation_id: &str, action: TripAction) -> Arc<Self> {
        Arc::new(Self {
            state: Arc::clone(state),
            operation_id: operation_id.to_string(),
            action,
            walked: AtomicUsize::new(0),
        })
    }

    fn walked(&self) -> usize {
        self.walked.load(Ordering::Acquire)
    }
}

impl Volume for BoundaryWalkingVolume {
    fn name(&self) -> &str {
        "boundary-walker"
    }

    fn root(&self) -> &Path {
        Path::new("/")
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move { Err(VolumeError::NotFound(path.display().to_string())) })
    }

    fn exists<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async { true })
    }

    fn is_directory<'a>(&'a self, _path: &'a Path) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(true) })
    }

    fn scan_for_copy_batch_with_boundary<'a>(
        &'a self,
        paths: &'a [PathBuf],
        boundary: &'a ScanBoundary<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<BatchScanResult, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            for _ in 0..ENTRIES {
                if self.walked.fetch_add(1, Ordering::AcqRel) + 1 == TRIP_AT {
                    match self.action {
                        // The real path, so the gate is woken the way a click
                        // wakes it. ❌ Never a bare `intent.store`.
                        TripAction::Cancel => cancel_write_operation(&self.operation_id, false),
                        TripAction::Pause => self.state.pause_gate.pause(),
                    }
                }
                boundary.file(1).await?;
            }
            let per_path = paths
                .iter()
                .map(|path| {
                    (
                        path.clone(),
                        crate::file_system::volume::CopyScanResult {
                            file_count: ENTRIES,
                            dir_count: 0,
                            total_bytes: ENTRIES as u64,
                            dedup_bytes: ENTRIES as u64,
                            top_level_is_directory: true,
                        },
                    )
                })
                .collect();
            Ok(cmdr_fs::volume::fold_batch(per_path))
        })
    }
}

fn config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        preview_id: None,
        ..VolumeCopyConfig::default()
    }
}

/// Cancel mid-scan stops the walk where it landed, ❌ not at the end of the tree.
#[tokio::test]
async fn a_cancel_during_the_operations_own_scan_stops_the_walk() {
    let state = make_state();
    let op = TestOperationGuard::register_state("preflight-scan-cancel", Arc::clone(&state));
    let volume = BoundaryWalkingVolume::new(&state, op.id(), TripAction::Cancel);
    let as_volume: Arc<dyn Volume> = Arc::clone(&volume) as Arc<dyn Volume>;
    let events = CollectorEventSink::new();

    let outcome = scan_volume_sources(
        &as_volume,
        &[PathBuf::from("/src")],
        &config(),
        op.id(),
        WriteOperationType::Copy,
        &state,
        &events,
    )
    .await;

    let failure = outcome.err().expect("a cancelled scan must not report totals");
    assert!(
        matches!(failure.error, WriteOperationError::Cancelled { .. }),
        "a cancelled scan surfaces as Cancelled, got {:?}",
        failure.error
    );
    assert!(
        volume.walked() <= TRIP_AT + 1,
        "the walk stopped where the cancel landed; it crossed {} of {ENTRIES} entries",
        volume.walked()
    );
    assert_eq!(
        events.cancelled.lock().expect("collector lock").len(),
        1,
        "the frontend closes its dialog on exactly one write-cancelled"
    );
}

/// Pause holds the walk where it is, and a resume lets it finish with the whole
/// tree's totals: Pause is ❌ not a slower Cancel.
#[tokio::test]
async fn a_pause_during_the_operations_own_scan_holds_it_until_resume() {
    let state = make_state();
    let op = TestOperationGuard::register_state("preflight-scan-pause", Arc::clone(&state));
    let volume = BoundaryWalkingVolume::new(&state, op.id(), TripAction::Pause);
    let as_volume: Arc<dyn Volume> = Arc::clone(&volume) as Arc<dyn Volume>;
    let events = CollectorEventSink::new();

    let resumer = Arc::clone(&state);
    let held_at = Arc::clone(&volume);
    let resumed = tokio::spawn(async move {
        // Long enough that an unparked walk would be finished and the assertion
        // below would read 1,000 rather than a handful.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let crossed = held_at.walked();
        resumer.pause_gate.resume();
        crossed
    });

    let preflight = scan_volume_sources(
        &as_volume,
        &[PathBuf::from("/src")],
        &config(),
        op.id(),
        WriteOperationType::Copy,
        &state,
        &events,
    )
    .await
    .expect("a resumed scan finishes");

    let crossed_while_paused = resumed.await.expect("the resumer task doesn't panic");
    assert!(
        crossed_while_paused <= TRIP_AT + 1,
        "the walk stood still while paused; it had crossed {crossed_while_paused} of {ENTRIES} entries"
    );
    assert_eq!(
        preflight.total_files, ENTRIES,
        "a resumed scan reports the whole tree, not what it had counted when it parked"
    );
    assert!(
        events.cancelled.lock().expect("collector lock").is_empty(),
        "a pause is not a cancel"
    );
}
