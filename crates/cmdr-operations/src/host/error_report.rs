//! What went wrong badly enough that a person should hear about it in an error
//! report, as typed facts the app re-raises.
//!
//! The engine never calls `log_error!`: that macro is the app's (it lives at the
//! app crate's root and carries the report dispatch), so the engine names what
//! happened and `EngineEvents::error_report` hands it over. Cmdr's answer
//! re-raises each one through `log_error!` with the target and words it always
//! had.

use std::path::PathBuf;
use std::time::Duration;

use cmdr_operation_log::types::OperationId;

use super::super::types::WriteOperationError;

/// One error-level fact, grouped by what it means for the person.
#[derive(Debug)]
pub enum OperationErrorReport {
    // An operation failed, and the person already sees it. The report is the
    // signal opt-in users send on "this didn't work". Raised only for a failure
    // that may be our defect: an expected refusal (`WriteOperationError::is_expected_refusal`)
    // logs at warn instead, as the frontend's `logLevelFor` does.
    /// A local copy failed and kept the `kept_files` it had landed.
    LocalCopyFailed {
        /// The operation that failed.
        operation_id: OperationId,
        /// Why it failed.
        error: WriteOperationError,
        /// How many files it had landed and kept.
        kept_files: usize,
    },
    /// A copy between volumes (Local, SMB, MTP, …) failed.
    VolumeCopyFailed {
        /// The operation that failed.
        operation_id: OperationId,
        /// Why it failed.
        error: WriteOperationError,
    },

    // A file of the person's is still safe, but not where they left it.
    /// A displaced original couldn't take a name back, so it stays on disk under
    /// its aside name, with its record, for the sweep to answer for.
    AsideStranded {
        /// Which attempt to put it back failed.
        attempt: AsideAttempt,
        /// Where the original sits now, under its aside name.
        aside: PathBuf,
        /// The OS's words for the refusal.
        error: String,
    },

    // A transfer is stuck on a backend that stopped answering.
    /// The probe gave up on a task that moved no bytes for `still_for_secs`:
    /// the write is abandoned, and the file runs again if it has attempts left.
    StalledTaskAbandoned {
        /// The operation the task belongs to.
        operation_id: OperationId,
        /// How long the task moved no bytes, in seconds.
        still_for_secs: u64,
        /// The task as the probe renders it.
        task: String,
    },
    /// A cancelled volume copy left `tasks` in-flight tasks behind that didn't
    /// wind down within `deadline`.
    WindDownAbandoned {
        /// The cancelled operation.
        operation_id: OperationId,
        /// How many tasks didn't wind down.
        tasks: usize,
        /// How long the wind-down waited for them.
        deadline: Duration,
        /// The probe's dump of the wedged tasks, when the op had a probe.
        probe_dump: Option<String>,
    },
    /// A volume copy whose first failure drained its window left `tasks`
    /// in-flight tasks behind that didn't stop within `deadline`.
    FailureDrainAbandoned {
        /// The failed operation.
        operation_id: OperationId,
        /// How many tasks didn't stop.
        tasks: usize,
        /// How long the drain waited for them.
        deadline: Duration,
        /// The probe's dump of the wedged tasks, when the op had a probe.
        probe_dump: Option<String>,
    },

    // A bug, contained: the operation carries on, or its lanes are freed.
    /// The transfer tree stopped for this operation; the transfer carries on.
    TreeAbandoned {
        /// Why the tree stopped.
        cause: TreeFailure,
    },
    /// The admission pass found a queued op with no deferred start, and freed
    /// its lanes.
    AdmittedWithoutStart {
        /// The queued operation whose lanes were freed.
        operation_id: OperationId,
    },

    // Startup.
    /// The engine claimed its in-flight ledger with no data dir to keep it in,
    /// so this session's leftovers live in memory only: a crash or a force-quit
    /// leaves them for the hour-gated directory scan instead of the next
    /// launch's sweep.
    LedgerUnpersisted,
}

/// Which attempt to give a displaced original a name back failed.
#[derive(Debug)]
pub enum AsideAttempt {
    /// Putting it back at `original` after a staged write didn't land
    /// (`stage_and_land_file`).
    LandingRestore {
        /// Where the original lived.
        original: PathBuf,
    },
    /// Putting it back at `original` (`DisplacedEntry::restore`).
    DisplacedRestore {
        /// Where the original lived.
        original: PathBuf,
    },
    /// Putting it back at `original` after a folder overwrite failed
    /// (`safe_overwrite_dir`).
    DirOverwriteRestore {
        /// Where the original lived.
        original: PathBuf,
    },
    /// Giving it a ` (recovered)` name beside the folder that took its own
    /// (`DisplacedEntry::keep_as_recovered_sibling`).
    RecoveredName,
}

/// Why a transfer tree stopped.
#[derive(Debug)]
pub enum TreeFailure {
    /// A progress report into a live tree panicked.
    ReportPanicked,
    /// Building the tree panicked during `step`.
    BuildPanicked {
        /// The build step that panicked.
        step: &'static str,
    },
}
