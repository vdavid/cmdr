//! Shared fixtures and doubles for the `volume::r#move` suites: the operation
//! state and the `InMemoryVolume` pair every test starts from, a sink that
//! cancels the operation the moment the first file lands, and a destination
//! volume whose `rename` always fails.
//!
//! The suites that reach for these: `volume/move_tests.rs` (cross-volume),
//! `volume/move_same_tests.rs` (same-volume rename),
//! `volume/move_cancel_tests.rs`, `volume/move_failure_tests.rs`,
//! `volume/move_progress_tests.rs`, and `volume/move_merge_tests.rs`.

use super::*;
// Named here rather than inherited through the glob above: the engine module
// beside this one uses neither, so a `use` there would be dead in a release build.
use std::pin::Pin;
use std::time::Duration;

use crate::file_system::volume::{InMemoryVolume, VolumeError};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{
    WriteConflictEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteProgressEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::{AtomicU8, Ordering};

pub(super) fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// State whose `progress_interval` mirrors what the production wrapper does:
/// derived from the config. Without this, tests that set
/// `config.progress_interval_ms = 0` would still see the default 50 ms throttle
/// (state ignores the config it didn't construct from).
pub(super) fn make_state_with_interval_ms(ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(ms)))
}

pub(super) fn make_volumes() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    (
        Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000)),
        Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
    )
}

pub(super) fn config_default() -> VolumeCopyConfig {
    VolumeCopyConfig::default()
}

/// Sink that flips intent to Stopped after one successful file moves.
/// Watches `emit_progress` events with `files_done >= 1`.
pub(super) struct CancelAfterFirstSink {
    pub(super) inner: CollectorEventSink,
    pub(super) intent: Arc<AtomicU8>,
}
impl OperationEventSink for CancelAfterFirstSink {
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::Copying && event.files_done >= 1 {
            self.intent.store(2, Ordering::Relaxed); // Stopped
        }
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: WriteConflictEvent) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// Sink that photographs the live in-flight table at the one moment it is
/// populated.
///
/// A `write-progress` event in the Copying phase is emitted from
/// `SerialLeafProgress::on_chunk`, which the destination calls from inside
/// `write_from_stream` — so it runs inside the transfer's `CURRENT_TASK_PROBE`
/// scope, with the task's row in the table and its phase set. Nothing outside
/// that window can see it: the row is dropped when the source finishes, and the
/// operation is deregistered when the transfer returns.
pub(super) struct SampleInFlightTableSink {
    pub(super) inner: CollectorEventSink,
    operation_id: String,
    dump: std::sync::Mutex<Option<String>>,
}

impl SampleInFlightTableSink {
    pub(super) fn new(operation_id: &str) -> Self {
        Self {
            inner: CollectorEventSink::new(),
            operation_id: operation_id.to_owned(),
            dump: std::sync::Mutex::new(None),
        }
    }

    /// The table as it looked mid-write, or `None` when the operation kept no
    /// in-flight table at all — which is exactly what a transfer that never
    /// registered a probe looks like from here.
    pub(super) fn in_flight_table(&self) -> Option<String> {
        self.dump.lock_ignore_poison().clone()
    }
}

impl OperationEventSink for SampleInFlightTableSink {
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::Copying {
            let mut slot = self.dump.lock_ignore_poison();
            if slot.is_none() {
                *slot = crate::file_system::write_operations::transfer::transfer_probe::render_live_dump(
                    &self.operation_id,
                    "mid-write sample",
                );
            }
        }
        self.inner.emit_progress(event);
    }
    fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: WriteConflictEvent) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
    fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
}

/// Wraps an `InMemoryVolume` destination whose `rename` ALWAYS fails: models a
/// disconnect at the exact instant `finalize_safe_replace` swaps the
/// fully-written temp over the original. Everything else delegates.
pub(super) struct MoveRenameFailsDestVolume {
    pub(super) inner: Arc<InMemoryVolume>,
}

impl Volume for MoveRenameFailsDestVolume {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn root(&self) -> &Path {
        self.inner.root()
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn supports_export(&self) -> bool {
        true
    }
    fn supports_streaming(&self) -> bool {
        true
    }
    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(crate::file_system::volume::ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }
    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }
    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }
    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }
    fn create_file<'a>(
        &'a self,
        path: &'a Path,
        content: &'a [u8],
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.create_file(path, content)
    }
    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }
    fn get_space_info<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::volume::SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }
    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn crate::file_system::volume::VolumeReadStream>, VolumeError>> + Send + 'a,
        >,
    > {
        self.inner.open_read_stream(path)
    }
    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn crate::file_system::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        self.inner.write_from_stream(dest, size, stream, on_progress)
    }
    fn rename<'a>(
        &'a self,
        _from: &'a Path,
        _to: &'a Path,
        _force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async {
            Err(VolumeError::IoError {
                message: "simulated disconnect during finalize rename".to_string(),
                raw_os_error: None,
            })
        })
    }
}
