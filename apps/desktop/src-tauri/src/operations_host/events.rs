//! The write engine's engine-wide facts, as this app hears them.
//!
//! The engine reports facts through `EngineEvents`; [`TauriEngineEvents`] is
//! this app's answer and owns the policy on each: which ones become a Tauri
//! event, which feed indexing's "a transfer trumps me" gauge, and which become a
//! PostHog capture under which name.

use serde_json::{Value, json};
use tauri::AppHandle;
use tauri_specta::Event as _;

use super::wire;
use cmdr_fs::log_private::LogPrivate;
use cmdr_operation_log::rollback::RollbackReport;
use cmdr_operation_log::types::{Initiator, RollbackState};
use cmdr_operations::host::{
    AsideAttempt, EngineEvents, InstantMutationFact, InstantOp, InstantOutcome, InstantTarget, OperationErrorReport,
    TreeFailure,
};
use cmdr_operations::{LeftoversKeptEvent, OperationsChanged, WriteProgressEvent};

/// Typed `volumes-busy-changed` Tauri event. Wraps the busy volume-ID list in a
/// struct because `tauri_specta::Event` payloads must be named types (a bare
/// `Vec<String>` can't derive `Event`). The struct name kebab-cases to
/// `volumes-busy-changed`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct VolumesBusyChanged {
    /// IDs of volumes with an in-flight copy / move / delete operation (sorted).
    pub volume_ids: Vec<String>,
}

/// Emits through the app handle when there is one; a test binary has none and
/// still feeds the gauge and the analytics client, which work without an app.
pub(super) struct TauriEngineEvents {
    app: Option<AppHandle>,
}

impl TauriEngineEvents {
    pub(super) fn new(app: Option<AppHandle>) -> Self {
        Self { app }
    }
}

impl EngineEvents for TauriEngineEvents {
    fn operations_changed(&self, snapshot: OperationsChanged) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = wire::OperationsChanged(snapshot).emit(app) {
            log::warn!(target: "op_manager", "failed to emit operations-changed: {e}");
        }
    }

    fn volumes_busy_changed(&self, volume_ids: Vec<String>) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = (VolumesBusyChanged { volume_ids }).emit(app) {
            crate::log_error!(target: "eject", "Failed to emit volumes-busy-changed: {}", e);
        }
    }

    fn transfer_started(&self, volume_ids: &[String]) {
        crate::priority::transfers::note_transfer_started(volume_ids);
    }

    fn transfer_finished(&self, volume_ids: &[String]) {
        crate::priority::transfers::note_transfer_finished(volume_ids);
    }

    fn leftovers_kept(&self, notice: LeftoversKeptEvent) {
        let Some(app) = &self.app else {
            return;
        };
        if let Err(e) = wire::LeftoversKept(notice).emit(app) {
            log::warn!(target: "copy", "couldn't tell the frontend about the kept leftovers: {e}");
        }
    }

    fn instant_mutation_settled(&self, fact: InstantMutationFact) {
        let props = instant_props(fact);
        // The event name is a LITERAL in each arm, never a variable holding one: the
        // `analytics-event-catalog` check reads emitters by matching `capture("…")`, so
        // a name that arrives as a `&str` would be invisible to it and could drift from
        // the catalog forever. Worth the duplicated call. The two creates get separate
        // names because "do people make folders?" and "do people make files?" are
        // different questions with very different expected answers.
        match fact.op {
            InstantOp::Rename => crate::analytics::events::capture("rename_used", props),
            InstantOp::CreateFolder => crate::analytics::events::capture("folder_created", props),
            InstantOp::CreateFile => crate::analytics::events::capture("file_created", props),
        }
    }

    fn rollback_settled(&self, initiator: Initiator, narrowed: bool, report: &RollbackReport) {
        crate::analytics::events::capture("operation_rollback_run", rollback_props(initiator, narrowed, report));
    }

    fn error_report(&self, report: OperationErrorReport) {
        raise(report);
    }

    fn scan_wait_progress(&self, event: WriteProgressEvent) {
        let Some(app) = &self.app else {
            return;
        };
        let tick = wire::WriteProgress(event);
        if let Err(e) = tick.emit(app) {
            log::warn!(target: "op_manager", "failed to forward scan progress for op={}: {e}", tick.0.operation_id);
        }
    }
}

/// The engine's module paths, as the targets its reports always carried. The
/// log format and the report dispatch's per-chain filters key off a target, so
/// these stay put wherever the code that raises them lives.
const OVERWRITE_TARGET: &str = "cmdr_operations::overwrite";
const TRANSFER_TREE_TARGET: &str = "cmdr_operations::transfer_tree";
const LOCAL_COPY_TARGET: &str = "cmdr_operations::transfer::copy";
const VOLUME_COPY_TARGET: &str = "cmdr_operations::transfer::volume::copy";

/// Raises `report` through `log_error!`, in the words error-report bundles and
/// log greps already know.
fn raise(report: OperationErrorReport) {
    match report {
        OperationErrorReport::LocalCopyFailed {
            operation_id,
            error,
            kept_files,
        } => crate::log_error!(
            target: LOCAL_COPY_TARGET,
            "copy_files_with_progress: failed op={} detail={:?}, keeping {} completed files",
            operation_id,
            error.log_detail(),
            kept_files,
        ),
        OperationErrorReport::VolumeCopyFailed { operation_id, error } => crate::log_error!(
            target: VOLUME_COPY_TARGET,
            "copy_between_volumes: operation {} failed: detail={:?}",
            operation_id,
            error.log_detail(),
        ),
        OperationErrorReport::AsideStranded { attempt, aside, error } => match attempt {
            AsideAttempt::LandingRestore { original } => crate::log_error!(
                target: OVERWRITE_TARGET,
                "stage_and_land_file: failed to restore aside {} -> {}: {}",
                LogPrivate::file_path(&aside),
                LogPrivate::file_path(&original),
                error
            ),
            AsideAttempt::DisplacedRestore { original } => crate::log_error!(
                target: OVERWRITE_TARGET,
                "DisplacedEntry::restore: failed to put {} back at {}: {}",
                LogPrivate::path(&aside),
                LogPrivate::path(&original),
                error
            ),
            AsideAttempt::DirOverwriteRestore { original } => crate::log_error!(
                target: OVERWRITE_TARGET,
                "safe_overwrite_dir: failed to restore aside {} -> {}: {}",
                LogPrivate::dir_path(&aside),
                LogPrivate::dir_path(&original),
                error
            ),
            AsideAttempt::RecoveredName => crate::log_error!(
                target: OVERWRITE_TARGET,
                "DisplacedEntry::keep_as_recovered_sibling: couldn't give {} a real name: {}",
                LogPrivate::path(&aside),
                error
            ),
        },
        OperationErrorReport::StalledTaskAbandoned {
            operation_id,
            still_for_secs,
            task,
        } => crate::log_error!(
            target: "copy",
            "transfer probe: op={op} ending the wait on a task that has moved no bytes for {secs}s. \
             The write is abandoned and the file runs again if it has attempts left.\n  {task}",
            op = operation_id,
            secs = still_for_secs,
            task = task,
        ),
        OperationErrorReport::WindDownAbandoned {
            operation_id,
            tasks,
            deadline,
            probe_dump,
        } => crate::log_error!(
            target: "copy",
            "copy_volumes_with_progress: op={} abandoning {} task(s) that did not wind down within {:?} of the cancel. \
             Their handles are left for the backend to reap; their staged partials are cleaned up below.{}",
            operation_id,
            tasks,
            deadline,
            probe_dump.map(|dump| format!("\n{dump}")).unwrap_or_default(),
        ),
        OperationErrorReport::FailureDrainAbandoned {
            operation_id,
            tasks,
            deadline,
            probe_dump,
        } => crate::log_error!(
            target: "copy",
            "copy_volumes_with_progress: op={} abandoning {} task(s) that did not stop within {:?} of its first failure. \
             Their handles are left for the backend to reap; their staged partials are cleaned up below.{}",
            operation_id,
            tasks,
            deadline,
            probe_dump.map(|dump| format!("\n{dump}")).unwrap_or_default(),
        ),
        OperationErrorReport::TreeAbandoned { cause } => match cause {
            TreeFailure::ReportPanicked => crate::log_error!(
                target: TRANSFER_TREE_TARGET,
                "transfer tree: a report panicked, so this operation shows no tree from here on (the transfer carries on)"
            ),
            TreeFailure::BuildPanicked { step } => crate::log_error!(
                target: TRANSFER_TREE_TARGET,
                "transfer tree: building it panicked ({step}), so this operation shows no tree (the transfer carries on)"
            ),
        },
        OperationErrorReport::AdmittedWithoutStart { operation_id } => crate::log_error!(
            target: "op_manager",
            "admitted op={operation_id} had no deferred start; releasing lanes"
        ),
        OperationErrorReport::LedgerUnpersisted => crate::log_error!(
            target: "copy",
            "no app data dir for the in-flight temp ledger; a copy interrupted this session will leave its \
             leftovers for the next transfer into that directory to reap"
        ),
    }
}

/// The props every instant-op event carries, built once so they can't drift:
/// who asked (the person, the agent, or an AI client), where it landed, and
/// whether it worked. Never a name, a path, or the reason it failed.
fn instant_props(fact: InstantMutationFact) -> Value {
    let target = match fact.target {
        InstantTarget::Volume => "volume",
        InstantTarget::Archive => "archive",
    };
    let outcome = match fact.outcome {
        InstantOutcome::Done => "done",
        InstantOutcome::Failed => "failed",
    };
    json!({
        "initiator": fact.initiator.as_token(),
        "target": target,
        "outcome": outcome,
    })
}

/// `operation_rollback_run`'s props: who asked, whole or narrowed (`scope`), how many
/// items the run EXAMINED (reversed plus skipped, bucketed; rows a selection left out
/// were never looked at), and how it ended. A cancel wins; otherwise the state the
/// original op landed in decides, and a run that wasn't stopped yet left the op as it
/// was is `failed` (the journal couldn't hand over its rows). Reads only the report's
/// counts and states, ❌ never its skips' example file names.
fn rollback_props(initiator: Initiator, narrowed: bool, report: &RollbackReport) -> Value {
    let outcome = if report.canceled {
        "cancelled"
    } else {
        match report.final_state {
            RollbackState::RolledBack => "done",
            RollbackState::PartiallyRolledBack => "partial",
            RollbackState::Rollbackable | RollbackState::NotRollbackable | RollbackState::RollingBack => "failed",
        }
    };
    let examined = report.reversed.saturating_add(report.skipped);
    json!({
        "initiator": initiator.as_token(),
        "scope": if narrowed { "partial" } else { "all" },
        "item_count": crate::analytics::item_count_bucket(usize::try_from(examined).unwrap_or(usize::MAX)),
        "outcome": outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(reversed: u64, skipped: u64, canceled: bool, final_state: RollbackState) -> RollbackReport {
        RollbackReport {
            reversed,
            skipped,
            left_out: 0,
            skips: Vec::new(),
            canceled,
            final_state,
        }
    }

    /// Pinned word for word: the dashboard maps these tokens.
    #[test]
    fn rollback_props_carry_the_scope_bucket_and_outcome_tokens() {
        assert_eq!(
            rollback_props(
                Initiator::User,
                true,
                &report(2, 1, false, RollbackState::PartiallyRolledBack)
            ),
            json!({
                "initiator": Initiator::User.as_token(),
                "scope": "partial",
                "item_count": crate::analytics::item_count_bucket(3),
                "outcome": "partial",
            })
        );
        let outcome = |r: RollbackReport| rollback_props(Initiator::User, false, &r)["outcome"].clone();
        assert_eq!(outcome(report(3, 0, false, RollbackState::RolledBack)), "done");
        assert_eq!(
            outcome(report(1, 0, true, RollbackState::PartiallyRolledBack)),
            "cancelled"
        );
        assert_eq!(outcome(report(0, 0, false, RollbackState::Rollbackable)), "failed");
        assert_eq!(
            rollback_props(Initiator::Agent, false, &report(1, 0, false, RollbackState::RolledBack))["scope"],
            "all"
        );
    }

    /// The props are what the dashboards query on, so their keys and tokens are
    /// pinned word for word.
    #[test]
    fn instant_props_carry_the_initiator_target_and_outcome_tokens() {
        let fact = InstantMutationFact {
            op: InstantOp::Rename,
            initiator: Initiator::User,
            target: InstantTarget::Archive,
            outcome: InstantOutcome::Failed,
        };
        assert_eq!(
            instant_props(fact),
            json!({ "initiator": Initiator::User.as_token(), "target": "archive", "outcome": "failed" })
        );
        let landed = InstantMutationFact {
            target: InstantTarget::Volume,
            outcome: InstantOutcome::Done,
            ..fact
        };
        assert_eq!(
            instant_props(landed),
            json!({ "initiator": Initiator::User.as_token(), "target": "volume", "outcome": "done" })
        );
    }

    /// The engine's transfer pair raises and lowers indexing's "a transfer
    /// trumps me" gauge for the volumes it names. The engine's half (one finish
    /// per start, however an operation ends) is `status_cache_tests.rs`'s.
    #[test]
    fn the_transfer_pair_raises_and_lowers_the_indexing_gauge() {
        use crate::priority::transfers::transfer_active;

        // The gauge is process-wide, so the volume is unique to this test.
        let volume = vec![format!("gauge-adapter-{}", uuid::Uuid::new_v4())];
        let events = TauriEngineEvents::new(None);

        events.transfer_started(&volume);
        assert!(transfer_active(&volume[0]), "a started transfer marks its volume");
        events.transfer_finished(&volume);
        assert!(!transfer_active(&volume[0]), "its finish clears it");
    }
}
