//! This app, acting as the write engine's host.
//!
//! The mirror of `cmdr_operations::host`: that module declares
//! what the engine needs from the app around it, and this is where the app
//! answers, once, in `setup()`, by building the app's one engine
//! ([`operations`]).
//!
//! - runtime — the volume host's (`volume_host::host()`), which is the app's own
//!   tokio runtime, so an admitted operation never runs on a caller's
//! - the journal — the app's one `OperationLog`, handed in by `install`
//! - volumes — [`volumes::AppOperationVolumes`], the volume registry asked per
//!   call
//! - engine-wide events — [`events::TauriEngineEvents`]
//! - own writes — [`own_writes::AppOwnWrites`], the downloads watcher's ignore
//!   set
//! - the drive index — [`index::AppOperationIndex`], `index_host::index()` asked
//!   per call
//! - E2E pacing — [`pacing::AppPacing`], `test_mode`'s throttles
//! - name order — [`name_order::AppNameOrder`], the panes' collator
//! - one operation's or preview's events — [`TauriEventSink`] and
//!   [`TauriScanPreviewSink`] (`sinks.rs`), built at the IPC edge; a terminal
//!   event's app side effects ride along (`terminal_effects.rs`)
//! - the wire — `wire.rs` wraps each engine payload as the Tauri event it
//!   travels as

mod analytics;
// Every backend's cells through the app's real engine, and the scenarios they
// share: the Docker fixtures, virtual MTP, and the ADB fake need the app.
#[cfg(test)]
pub(crate) mod backend_suites;
mod events;
mod index;
mod name_order;
mod own_writes;
mod pacing;
mod sinks;
pub(crate) mod terminal_effects;
mod volumes;
// Which app side effects each terminal event fires, and in what order around its emit.
#[cfg(test)]
mod copy_snapshot_out_tests;
#[cfg(test)]
mod log_redaction_tests;
#[cfg(test)]
mod portal_rename_tests;
#[cfg(test)]
mod registry_routing_tests;
#[cfg(test)]
mod terminal_effects_tests;
pub(crate) mod wire;

use std::sync::{Arc, OnceLock};

use cmdr_operation_log::OperationLog;
use tauri::AppHandle;

use cmdr_operations::{OperationEventSink, Operations, OperationsBuilder};
pub(crate) use events::VolumesBusyChanged;
pub use sinks::{TauriEventSink, TauriScanPreviewSink};

/// The app's write engine, once [`install`] built it.
static OPERATIONS: OnceLock<Operations> = OnceLock::new();

/// Why [`operations`] has no engine to hand out: [`install`] hasn't run.
///
/// Unreachable in the app, which installs at the top of `setup()`; a caller
/// that meets it refuses rather than starting anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotInstalled;

/// Builds the app's write engine, wired to this app, journaling into
/// `operation_log` (the handle `operation_log_host::install` returned, never a
/// second `operation_log()` read).
///
/// Call once, in `setup()`, unconditionally: after `volume_host::install` (the
/// engine's runtime is the volume host's) and `operation_log_host::install`, and
/// before the leftover sweep, `init_volume_manager`, or anything that can start
/// an operation. A second call keeps the first engine.
pub fn install(app: &AppHandle, operation_log: OperationLog) {
    let sink_app = app.clone();
    let engine = app_answers(Some(app.clone()), operation_log)
        .operation_sinks(Box::new(move || {
            Arc::new(TauriEventSink::new(sink_app.clone())) as Arc<dyn OperationEventSink>
        }))
        .build();
    if OPERATIONS.set(engine).is_err() {
        crate::log_error!(target: "op_manager", "the write engine was installed twice; keeping the first");
    }
}

/// The app's write engine. Every app caller reaches the engine through this.
///
/// `Err(NotInstalled)` before [`install`], with an error report: the caller
/// refuses with a variant the frontend already handles (a start through
/// [`refuse_not_installed`], an instant op through [`refuse_instant_not_installed`])
/// or answers what an engine with nothing running would (no operations, nothing
/// busy, an unknown id). It never falls back to a detached engine, which would
/// write with nobody hearing. In a test binary nothing installs, so the first
/// read builds `test_engine`, which every app-side cell then shares.
pub(crate) fn operations() -> Result<&'static Operations, NotInstalled> {
    operations_in(&OPERATIONS)
}

/// The app's one engine in a test binary, which an app-side cell drives the
/// way the app's own callers reach it through [`operations`]. Every such cell
/// shares it, so each keeps its operation ids, lanes, and volume names unique.
#[cfg(test)]
pub(crate) fn app_engine() -> &'static Operations {
    operations().expect("a test binary always has its engine")
}

/// [`operations`] over a given slot, so a test can hand an empty one in.
pub(crate) fn operations_in(slot: &'static OnceLock<Operations>) -> Result<&'static Operations, NotInstalled> {
    if let Some(engine) = slot.get() {
        return Ok(engine);
    }
    #[cfg(test)]
    if std::ptr::eq(slot, &OPERATIONS) {
        return Ok(OPERATIONS.get_or_init(test_engine));
    }
    crate::log_error!(target: "op_manager", "the write engine was reached before it was installed; refused");
    Err(NotInstalled)
}

/// The app's write engine claims its in-flight ledger in `data_dir`: the one
/// claim that reads, truncates, and sweeps the ledger file, taken on the engine
/// [`install`] set and no other (a duplicate that lost the install was dropped
/// untouched).
///
/// Call once in `setup()`, unconditionally, after the instance lock claims the
/// data dir and before anything can start a copy. With no data dir the ledger
/// runs unpersisted and the engine raises an error report. The sweep's handle
/// is dropped on purpose: a recorded leftover can sit on a dead mount where
/// `unlink` blocks for minutes, and a launch must never wait on that.
pub fn claim_ledger(data_dir: Option<&std::path::Path>) {
    claim_ledger_in(&OPERATIONS, data_dir);
}

/// [`claim_ledger`] on a given slot, so a test can hand in its own.
pub(crate) fn claim_ledger_in(slot: &'static OnceLock<Operations>, data_dir: Option<&std::path::Path>) {
    // Before `install` there's no engine to claim for; `operations_in` reported it.
    let Ok(engine) = operations_in(slot) else {
        return;
    };
    match engine.claim_ledger(data_dir) {
        Ok(sweep) => drop(sweep),
        Err(cmdr_operations::AlreadyClaimed) => {
            crate::log_error!(target: "op_manager", "the leftover ledger was claimed twice; keeping the first claim");
        }
    }
}

/// What a start refused before `install` answers: the frontend's generic I/O
/// failure, which its error dialog renders without naming a file. The app
/// can't reach this after `setup()`.
pub(crate) fn refuse_not_installed(_: NotInstalled) -> cmdr_operations::WriteOperationError {
    cmdr_operations::WriteOperationError::IoError {
        path: String::new(),
        message: "the write engine isn't ready yet".into(),
    }
}

/// What an instant op (rename, a new folder or file, a single trash, a paste
/// as a file) refused before `install` answers: "Cmdr is still starting up",
/// the one existing refusal that says so. Its copy names archive edits, the
/// only way an instant op met it before; a new variant would change the wire
/// and owe new copy for a state the app can't reach.
pub(crate) fn refuse_instant_not_installed(_: NotInstalled) -> cmdr_operations::MutationError {
    cmdr_operations::MutationError::ArchiveEditNotReady
}

/// What a scan preview asked for before `install` answers: no walk, and the
/// dialog hears an error at once (the answer an engine gives a walk that
/// couldn't start), so it never spins.
pub(crate) fn refuse_scan_preview_not_installed(
    events: &dyn cmdr_operations::ScanPreviewEventSink,
) -> cmdr_operations::ScanPreviewStartResult {
    let preview_id = uuid::Uuid::new_v4().to_string();
    events.emit_error(cmdr_operations::ScanPreviewErrorEvent {
        preview_id: preview_id.clone(),
        message: "the write engine isn't ready yet".to_string(),
        timed_out: false,
    });
    cmdr_operations::ScanPreviewStartResult { preview_id }
}

/// The engine a test binary shares, since nothing there calls [`install`]: the
/// app's answers with no frontend behind them (the real indexing gauge,
/// analytics client, and error reporter), the test binary's shared operation
/// log, and no sink factory. A cell that needs the app's registry on an engine
/// of its own builds one per cell, too.
#[cfg(test)]
pub(crate) fn test_engine() -> Operations {
    app_answers(None, crate::operation_log_host::operation_log().clone()).build()
}

/// [`test_engine`], journaling into `log`: an app-side cell that reads back
/// what its operations recorded runs them on an engine of its own.
#[cfg(test)]
pub(crate) fn test_engine_journaling_into(log: &cmdr_operations::testing::TestOperationLog) -> Operations {
    app_answers(None, log.log().clone()).build()
}

/// Every seam the app answers the same with or without a frontend; `app` is
/// where the engine-wide events go out, if anywhere, and `operation_log` the
/// journal it records into.
fn app_answers(app: Option<AppHandle>, operation_log: OperationLog) -> OperationsBuilder {
    Operations::builder(
        Arc::new(volumes::AppOperationVolumes),
        Arc::new(events::TauriEngineEvents::new(app)),
    )
    .volume_host(crate::volume_host::host())
    .operation_log(operation_log)
    .own_writes(Arc::new(own_writes::AppOwnWrites))
    .index(Arc::new(index::AppOperationIndex))
    .pacing(Arc::new(pacing::AppPacing))
    .name_order(Arc::new(name_order::AppNameOrder))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use cmdr_fs::ignore_poison::IgnorePoison;
    use cmdr_operation_log::OperationLog;
    use cmdr_operation_log::capture::CapturingJournal;
    use cmdr_operation_log::types::{ExecutionStatus, Initiator, OpKind};
    use cmdr_operation_log::writer::OpenOperation;

    use cmdr_operations::Operations;
    use cmdr_operations::host::testing::{FakeVolumes, RecordingEngineEvents};

    fn header(op_id: &str) -> OpenOperation {
        OpenOperation {
            op_id: op_id.into(),
            kind: OpKind::Copy,
            initiator: Initiator::User,
            source_volume_id: None,
            dest_volume_id: None,
            item_count: 1,
            started_at: 0,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        }
    }

    /// Whether `engine` journals into `log`: an operation opened on the engine is
    /// heard by a journal installed on `log`, so the two are one handle.
    fn journals_into(engine: &Operations, log: &OperationLog) -> bool {
        let journal = Arc::new(CapturingJournal::new());
        log.set_journal(Some(journal.clone()));
        drop(engine.operation_log().journal_open(header("same-log-probe")));
        let heard = journal.opens.lock_ignore_poison().len() == 1;
        log.set_journal(None);
        heard
    }

    /// The engine `install` builds journals into the very handle
    /// `operation_log_host::install` returned, never a second one.
    #[test]
    fn the_installed_engine_journals_into_the_log_it_was_handed() {
        let log = OperationLog::detached();
        let engine = super::app_answers(None, log.clone()).build();
        assert!(journals_into(&engine, &log));
    }

    /// The app claims the installed engine's leftover ledger, and only once: a
    /// second claim is refused and leaves the ledger file as the first left it.
    #[test]
    fn the_app_claims_the_installed_engines_ledger_once() {
        static SLOT: std::sync::OnceLock<Operations> = std::sync::OnceLock::new();
        let engine = Operations::builder(
            Arc::new(FakeVolumes::default()),
            Arc::new(RecordingEngineEvents::default()),
        )
        .build();
        assert!(SLOT.set(engine.clone()).is_ok());
        let dir = crate::test_support::TestDir::new("app-ledger-claim");
        // The persisted file's name, which every build since the ledger existed reads.
        let ledger_file = dir.join("in-flight-temps.log");

        super::claim_ledger_in(&SLOT, Some(&*dir));
        assert!(ledger_file.exists(), "the claim opened the ledger in the data dir");
        std::fs::write(&ledger_file, b"+\"/a/live.cmdr-tmp-1\"\n").expect("a live record");
        super::claim_ledger_in(&SLOT, Some(&*dir));

        assert_eq!(
            std::fs::read(&ledger_file).expect("the ledger file"),
            b"+\"/a/live.cmdr-tmp-1\"\n",
            "a second claim neither truncates nor replays the ledger"
        );
        assert_eq!(
            engine.claim_ledger(Some(&*dir)).err(),
            Some(cmdr_operations::AlreadyClaimed),
            "the claim was the installed engine's"
        );
    }

    /// The app's engine paces each rollback item by `test_mode`'s throttle: the
    /// window an E2E spec presses Cancel in. The engine's half (the item loop
    /// asks its host's `Pacing`) is
    /// `rollback::planner_tests::tests::the_e2e_throttle_hook_paces_the_item_loop`.
    #[tokio::test]
    async fn the_app_engine_paces_rollbacks_from_test_mode() {
        let _pacing = crate::test_mode::pace_rollback_for_test(25).await;
        assert_eq!(
            super::test_engine().host().pacing().rollback_pace(),
            Some(std::time::Duration::from_millis(25))
        );
    }

    /// The test binary's shared engine journals into `operation_log()`, the app's
    /// one log, as the installed engine journals into the one `install` handed it.
    #[test]
    fn the_shared_test_engine_journals_into_operation_log() {
        let engine = super::operations().expect("a test binary always has its engine");
        assert!(journals_into(engine, crate::operation_log_host::operation_log()));
    }
}
