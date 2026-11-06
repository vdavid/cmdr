//! The engine's hot paths, opened to the `transfer_benchmarks` Criterion bench
//! (`benches/transfer_benchmarks.rs`).
//!
//! A bench compiles against the crate as an EXTERNAL crate, so it sees neither
//! `#[cfg(test)]` nor `pub(crate)`. This module is its door, and every function
//! in it is a thin wrapper over a real internal entry point: the same calls a
//! started operation makes, with a no-op sink and a fresh, UNREGISTERED
//! `WriteOperationState` on an engine the bench owns ([`BenchEngine`], or
//! [`BenchJournal`] for the journaled case), so nothing one case does reaches
//! another. The bench gets `testing` through the crate's self dev-dependency.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use uuid::Uuid;

use crate::Operations;
use crate::conflict_slot::ArmedConflict;
use crate::delete::delete_files_with_progress_inner;
use crate::event_sinks::{OperationEventSink, ScanPreviewEventSink};
use crate::journal;
use crate::scan_cache::ScanPreviewState;
use crate::scan_preview::run_scan_preview;
use crate::scan_watchdog::{SCAN_INACTIVITY_LIMIT, ScanTarget, ScanWatchdog};
use crate::state::WriteOperationState;
use crate::transfer::copy::copy_files_with_progress_inner;
use crate::transfer::volume::copy_volumes_with_progress;
use crate::types::{
    ConflictInfo, DryRunResult, ScanPreviewCancelledEvent, ScanPreviewCompleteEvent, ScanPreviewErrorEvent,
    ScanPreviewProgressEvent, ScanPreviewPurpose, ScanProgressEvent, VolumeCopyConfig, WriteCancelledEvent,
    WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationConfig, WriteOperationError,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_fs::volume::{DEFAULT_VOLUME_ID, Volume};
use cmdr_local::LocalPosixVolume;
use cmdr_operation_log::OperationLog;
use cmdr_operation_log::capture::WriterJournal;
use cmdr_operation_log::store::{OperationLogStoreError, open_read_connection, read_operation_items};
use cmdr_operation_log::types::{EntryType, ExecutionStatus, Initiator, OpKind};
use cmdr_operation_log::writer::OperationLogWriter;

/// A sink that drops every event: the bench measures the engine, not a
/// collector's `Vec` growing or a webview's IPC.
struct DiscardEvents;

impl OperationEventSink for DiscardEvents {
    fn emit_progress(&self, _event: WriteProgressEvent) {}
    fn emit_complete(&self, _event: WriteCompleteEvent) {}
    fn emit_cancelled(&self, _event: WriteCancelledEvent) {}
    fn emit_error(&self, _event: WriteErrorEvent) {}
    fn emit_conflict(&self, _event: ArmedConflict) {}
    fn emit_conflict_resolved(&self, _event: WriteConflictResolvedEvent) {}
    fn emit_source_item_done(&self, _event: WriteSourceItemDoneEvent) {}
    fn emit_scan_progress(&self, _event: ScanProgressEvent) {}
    fn emit_scan_conflict(&self, _conflict: ConflictInfo) {}
    fn emit_dry_run_complete(&self, _result: DryRunResult) {}
    fn emit_settled(&self, _event: WriteSettledEvent) {}
}

/// The scan preview's own four events, dropped the same way.
struct DiscardPreviewEvents;

impl ScanPreviewEventSink for DiscardPreviewEvents {
    fn emit_progress(&self, _event: ScanPreviewProgressEvent) {}
    fn emit_complete(&self, _event: ScanPreviewCompleteEvent) {}
    fn emit_error(&self, _event: ScanPreviewErrorEvent) {}
    fn emit_cancelled(&self, _event: ScanPreviewCancelledEvent) {}
}

/// An operation's state the way `start_write_operation` builds it on `engine`,
/// minus the registration: the progress interval the frontend sends by default.
fn fresh_state(engine: &Operations) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(
        WriteOperationConfig::default().progress_interval_ms,
    )))
}

/// The engine a bench drives: one of its own, with every seam answering
/// nothing and no journal. Build it once per case, untimed.
pub struct BenchEngine {
    engine: Operations,
}

impl Default for BenchEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchEngine {
    /// A detached engine of the bench's own.
    pub fn new() -> Self {
        Self {
            engine: Operations::detached(),
        }
    }

    /// A local same-volume copy of `sources` into `destination`, through
    /// `copy_files_with_progress_inner` with no journal.
    pub fn copy_local(&self, sources: &[PathBuf], destination: &Path) -> Result<(), WriteOperationError> {
        let op_id = cmdr_operation_log::new_operation_id();
        copy_files_with_progress_inner(
            &DiscardEvents,
            &op_id,
            &fresh_state(&self.engine),
            sources,
            destination,
            &WriteOperationConfig::default(),
        )
    }

    /// A local delete of `sources`, through `delete_files_with_progress_inner`.
    pub fn delete_local(&self, sources: &[PathBuf]) -> Result<(), WriteOperationError> {
        let op_id = cmdr_operation_log::new_operation_id();
        delete_files_with_progress_inner(
            &DiscardEvents,
            &op_id,
            &fresh_state(&self.engine),
            sources,
            &WriteOperationConfig::default(),
        )
    }

    /// A local volume over `root`, the destination a cross-volume copy streams
    /// into, on the host the engine itself runs on.
    pub fn local_volume(&self, root: impl Into<PathBuf>) -> Arc<dyn Volume> {
        Arc::new(LocalPosixVolume::local_folder(
            "Transfer bench",
            root,
            self.engine.host().volume_host().clone(),
        ))
    }

    /// A cross-volume copy through `copy_volumes_with_progress`: the chunked
    /// `open_read_stream` → `write_from_stream` path. Needs a multi-threaded
    /// Tokio runtime, like every volume backend.
    pub async fn copy_volumes(
        &self,
        source_volume: Arc<dyn Volume>,
        sources: &[PathBuf],
        dest_volume: Arc<dyn Volume>,
        dest_path: &Path,
    ) -> Result<(), WriteOperationError> {
        let op_id = cmdr_operation_log::new_operation_id();
        copy_volumes_with_progress(
            Arc::new(DiscardEvents),
            &op_id,
            &fresh_state(&self.engine),
            source_volume,
            sources,
            dest_volume,
            dest_path,
            &VolumeCopyConfig::default(),
        )
        .await
        .map_err(|failure| failure.error)
    }

    /// The local scan-preview worker over `sources`, run inline on the calling
    /// thread the way its spawned thread runs it, then the cached result consumed
    /// the way a confirmed operation consumes it. Returns the file count the walk
    /// cached, or `None` when it settled without a result. The preview's
    /// watchdog spawns its heartbeat on the engine's runtime, so no runtime
    /// needs to be entered.
    pub fn scan_preview_local(&self, sources: Vec<PathBuf>) -> Option<usize> {
        let engine = &self.engine;
        let preview_id = Uuid::new_v4().to_string();
        let state = Arc::new(ScanPreviewState {
            cancelled: AtomicBool::new(false),
            progress_interval: Duration::from_millis(WriteOperationConfig::default().progress_interval_ms),
            mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
        });
        let events: Arc<dyn ScanPreviewEventSink> = Arc::new(DiscardPreviewEvents);
        let watchdog = ScanWatchdog::start(
            engine,
            preview_id.clone(),
            ScanTarget::of(&sources, DEFAULT_VOLUME_ID),
            SCAN_INACTIVITY_LIMIT,
            Arc::clone(&state),
            Arc::clone(&events),
        );
        run_scan_preview(
            engine,
            events,
            preview_id.clone(),
            sources.clone(),
            state,
            ScanPreviewPurpose::Transfer,
            watchdog,
        );
        engine
            .stores()
            .previews()
            .take_cached_scan_result(&preview_id, &sources)
            .map(|result| result.file_count)
    }
}

/// An engine of the bench's own journaling into a temp-DB operation log, so a
/// local copy journals the way a started one does. The journal lives and dies
/// with it, so the unjournaled cases never see it.
pub struct BenchJournal {
    engine: Operations,
    db_path: PathBuf,
}

impl BenchJournal {
    /// Spawns the writer over `db_path` (created if missing) and builds an
    /// engine journaling into it.
    pub fn open(db_path: &Path) -> Result<Self, OperationLogStoreError> {
        let writer = OperationLogWriter::spawn(db_path)?;
        let log = OperationLog::detached();
        log.set_journal(Some(Arc::new(WriterJournal::new(writer))));
        Ok(Self {
            engine: Operations::detached_journaling_into(log),
            db_path: db_path.to_path_buf(),
        })
    }

    /// [`BenchEngine::copy_local`], bracketed by the journal's open and finalize
    /// exactly as `start_write_operation` brackets it. Returns the operation's
    /// id, for [`Self::journaled_files`].
    pub fn copy_local(&self, sources: &[PathBuf], destination: &Path) -> Result<String, WriteOperationError> {
        let op_id = cmdr_operation_log::new_operation_id();
        let state = fresh_state(&self.engine);
        journal::open_local_op(
            state.scope(),
            &op_id,
            OpKind::Copy,
            Initiator::User,
            sources.len() as u64,
            Some(DEFAULT_VOLUME_ID),
        );
        let result = copy_files_with_progress_inner(
            &DiscardEvents,
            &op_id,
            &state,
            sources,
            destination,
            &WriteOperationConfig::default(),
        );
        let execution_status = match &result {
            Ok(()) => ExecutionStatus::Done,
            Err(WriteOperationError::Cancelled { .. }) => ExecutionStatus::Canceled,
            Err(_) => ExecutionStatus::Failed,
        };
        journal::finalize_op(state.scope(), OpKind::Copy, execution_status);
        result.map(|()| op_id)
    }

    /// How many file rows the journal holds for `op_id`: the proof a bench
    /// iteration journaled at all.
    pub fn journaled_files(&self, op_id: &str) -> Result<usize, OperationLogStoreError> {
        let conn = open_read_connection(&self.db_path)?;
        let items = read_operation_items(&conn, op_id, u32::MAX)?;
        Ok(items.iter().filter(|item| item.entry_type == EntryType::File).count())
    }
}
