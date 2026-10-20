//! The `queue` tool: control the operation queue (pause / resume / cancel, and
//! the global pause-all / resume-all).
//!
//! Thin adapter over the typed `cmdr-operations` manager functions (smart
//! backend / thin frontend). It dispatches no FE action and invents no ack — it
//! calls the backend directly and returns OK (the `connect_to_server` /
//! `indexing` precedent, so there is no FE action to ack). These are transient runtime actions on a
//! crash-safe pipeline. A `rollback: true` cancel DELETES already-copied files, so keep the tool
//! out of the in-process agent view even though authenticated external MCP clients may request it.
//!
//! Discovery of operation ids + their status lives in `cmdr://state` under
//! `operations:` (the two-source join in `resources/operations.rs`).

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::pluralize::pluralize;
use cmdr_operation_log::types::OperationId;
use cmdr_operations::{
    LifecycleStatus, LockedSourcesAnswer, LockedSourcesAnswerOutcome, OperationSnapshot, Operations, PauseAllOutcome,
    PauseOutcome, SourcesGoneAnswerOutcome,
};

/// The app's engine, or `None` before `install`, where every answer is what an
/// engine with nothing running gives.
fn engine() -> Option<&'static Operations> {
    crate::operations_host::operations().ok()
}

pub async fn execute_queue(params: &Value) -> ToolResult {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'action' parameter"))?;

    match action {
        "pause_all" => Ok(json!(pause_all_reply(
            engine().map(Operations::pause_all).unwrap_or_default()
        ))),
        "resume_all" => Ok(json!(resume_all_reply(
            engine().map(Operations::resume_all).unwrap_or_default()
        ))),
        "pause" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            pause_reply(
                &id,
                engine().map_or(PauseOutcome::NotApplicable, |ops| ops.pause_operation(&id)),
            )
        }
        "resume" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            resume_reply(
                &id,
                engine().map_or(PauseOutcome::NotApplicable, |ops| ops.resume_operation(&id)),
            )
        }
        "cancel" => execute_cancel(params),
        "continue_without_gone" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            continue_reply(
                &id,
                engine().map_or(SourcesGoneAnswerOutcome::UnknownOperation, |ops| {
                    ops.continue_without_gone_sources(&id)
                }),
            )
        }
        "move_locked" | "skip_locked" => {
            let id = require_operation_id(params)?;
            require_operation_exists(&id)?;
            let answer = if action == "move_locked" {
                LockedSourcesAnswer::MoveAnyway
            } else {
                LockedSourcesAnswer::Skip
            };
            locked_reply(
                &id,
                answer,
                engine().map_or(LockedSourcesAnswerOutcome::UnknownOperation, |ops| {
                    ops.answer_locked_sources(&id, answer)
                }),
            )
        }
        other => Err(ToolError::invalid_params(format!(
            "action must be 'pause', 'resume', 'cancel', 'continue_without_gone', 'move_locked', 'skip_locked', 'pause_all', or 'resume_all' (got '{other}')"
        ))),
    }
}

/// The agent-facing answer to a pause request, from what the manager actually
/// did with it.
///
/// An `OK` means the caller's intent holds. `Applied` earns one because the
/// operation has parked — mid-scan as much as mid-write, since the walk honors
/// the same gate; `AlreadyInState` earns one because the operation is sitting
/// exactly where the caller wants it. `NotApplicable` is a refusal, because
/// nothing changed and nothing is remembered: a `Queued` operation is the
/// everyday case (pause is documented to leave one alone), and an agent told
/// "OK: Paused …" for one goes on to act on a queue that never stopped.
fn pause_reply(operation_id: &OperationId, outcome: PauseOutcome) -> ToolResult {
    match outcome {
        PauseOutcome::Applied => Ok(json!(format!("OK: Paused operation {operation_id}."))),
        PauseOutcome::AlreadyInState => Ok(json!(format!("OK: Operation {operation_id} was already paused."))),
        PauseOutcome::NotApplicable => Err(ToolError::invalid_params(format!(
            "Operation {operation_id} isn't running, so there's nothing to pause: it's queued or already over. See cmdr://state operations for its current status."
        ))),
    }
}

/// The words that differ between the two sweeps. Everything else about the
/// answer (which counts get said, and in what order) is shared, so the two can't
/// drift into telling different stories about the same manager.
struct SweepWords {
    /// What the sweep did to the ones it flipped: "Paused" / "Resumed".
    did: &'static str,
    /// The request, as a noun: "pause" / "resume".
    request: &'static str,
    /// Why there was nothing to sweep at all.
    nothing_there: &'static str,
    /// The state an already-there operation was in: "paused" / "running".
    already: &'static str,
}

const PAUSE_WORDS: SweepWords = SweepWords {
    did: "Paused",
    request: "pause",
    nothing_there: "no operation is running",
    already: "paused",
};

const RESUME_WORDS: SweepWords = SweepWords {
    did: "Resumed",
    request: "resume",
    nothing_there: "no operation is paused",
    already: "running",
};

/// The agent-facing answer to `pause_all`, from what the sweep actually did.
///
/// A sweep has no single verdict to report, so it reports the counts. "Nothing
/// was running", "three parked", and "two were already paused" are three
/// different situations, and an agent told a flat "Paused every running
/// operation" for all three goes on to act on a queue that may never have
/// stopped.
fn pause_all_reply(outcome: PauseAllOutcome) -> String {
    sweep_reply(outcome, &PAUSE_WORDS)
}

/// The mirror of [`pause_all_reply`] for `resume_all`.
fn resume_all_reply(outcome: PauseAllOutcome) -> String {
    sweep_reply(outcome, &RESUME_WORDS)
}

fn sweep_reply(outcome: PauseAllOutcome, words: &SweepWords) -> String {
    if outcome.total() == 0 {
        return format!(
            "OK: Nothing to {}: {}. See cmdr://state operations.",
            words.request, words.nothing_there
        );
    }

    let mut sentences: Vec<String> = Vec::new();
    if outcome.applied > 0 {
        sentences.push(format!(
            "{} {}.",
            words.did,
            pluralize(outcome.applied as u64, "operation")
        ));
    }
    if outcome.already_in_state > 0 {
        sentences.push(format!(
            "{} {} already {}.",
            pluralize(outcome.already_in_state as u64, "operation"),
            was_were(outcome.already_in_state),
            words.already
        ));
    }
    if outcome.not_applicable > 0 {
        sentences.push(format!(
            "{} finished before the {} reached {}.",
            pluralize(outcome.not_applicable as u64, "operation"),
            words.request,
            it_them(outcome.not_applicable)
        ));
    }

    // Nothing flipped: whatever the counts say, the queue is exactly where it
    // was, and the answer has to open by saying so.
    let opener = if outcome.took_effect_anywhere() {
        String::new()
    } else {
        format!("Nothing to {}: ", words.request)
    };
    format!("OK: {opener}{}", sentences.join(" "))
}

fn was_were(count: usize) -> &'static str {
    if count == 1 { "was" } else { "were" }
}

fn it_them(count: usize) -> &'static str {
    if count == 1 { "it" } else { "them" }
}

/// The mirror of [`pause_reply`] for resume. An operation parked mid-scan
/// resumes exactly like one parked mid-write; anything that isn't parked has
/// nothing to resume.
fn resume_reply(operation_id: &OperationId, outcome: PauseOutcome) -> ToolResult {
    match outcome {
        PauseOutcome::Applied => Ok(json!(format!("OK: Resumed operation {operation_id}."))),
        PauseOutcome::AlreadyInState => Ok(json!(format!("OK: Operation {operation_id} was already running."))),
        PauseOutcome::NotApplicable => Err(ToolError::invalid_params(format!(
            "Operation {operation_id} isn't paused, so there's nothing to resume: it's queued or already over. See cmdr://state operations for its current status."
        ))),
    }
}

/// The agent-facing answer to "carry on without the sources that left". Only the
/// answer that reached the operation earns an `OK`.
fn continue_reply(operation_id: &OperationId, outcome: SourcesGoneAnswerOutcome) -> ToolResult {
    match outcome {
        SourcesGoneAnswerOutcome::Continued => Ok(json!(format!(
            "OK: Operation {operation_id} is carrying on with the sources that are still there."
        ))),
        SourcesGoneAnswerOutcome::NotAsking => Err(ToolError::invalid_params(format!(
            "Operation {operation_id} isn't asking about sources that left, so there's nothing to carry on: someone answered first, or it never asked. See pendingSourcesGone in cmdr://state operations."
        ))),
        SourcesGoneAnswerOutcome::UnknownOperation => Err(ToolError::invalid_params(format!(
            "Unknown operationId '{operation_id}': it settled before the answer arrived. See cmdr://state operations."
        ))),
    }
}

/// The agent-facing answer to "these items are Locked: move them anyway, or skip
/// them?". Only the answer that reached the operation earns an `OK`.
fn locked_reply(
    operation_id: &OperationId,
    answer: LockedSourcesAnswer,
    outcome: LockedSourcesAnswerOutcome,
) -> ToolResult {
    match outcome {
        LockedSourcesAnswerOutcome::Answered => Ok(json!(match answer {
            LockedSourcesAnswer::MoveAnyway => format!(
                "OK: Operation {operation_id} is moving its Locked items too, unlocking each for the move and locking it again where it lands."
            ),
            LockedSourcesAnswer::Skip =>
                format!("OK: Operation {operation_id} is leaving its Locked items where they are and moving the rest."),
        })),
        LockedSourcesAnswerOutcome::NotAsking => Err(ToolError::invalid_params(format!(
            "Operation {operation_id} isn't asking about Locked items, so there's nothing to answer: someone answered first, or it never asked. See pendingLockedSources in cmdr://state operations."
        ))),
        LockedSourcesAnswerOutcome::UnknownOperation => Err(ToolError::invalid_params(format!(
            "Unknown operationId '{operation_id}': it settled before the answer arrived. See cmdr://state operations."
        ))),
    }
}

/// Cancel one or several operations. `rollback: true` (single-op only) routes to
/// the rollback-capable cancel that deletes already-copied files; everything else
/// keeps partials.
fn execute_cancel(params: &Value) -> ToolResult {
    let rollback = params.get("rollback").and_then(|v| v.as_bool()).unwrap_or(false);

    // Multi-op cancel via `operationIds`. Rollback is single-op only (there's no
    // batch rollback backend), so combining the two is an honest error, not a
    // silent partial.
    if let Some(ids_value) = params.get("operationIds") {
        let ids: Vec<OperationId> = ids_value
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(OperationId::from)).collect())
            .unwrap_or_default();
        if ids.is_empty() {
            return Err(ToolError::invalid_params(
                "'operationIds' must be a non-empty array of operation id strings",
            ));
        }
        if rollback {
            return Err(ToolError::invalid_params(
                "rollback is only supported for a single operationId, not operationIds",
            ));
        }
        if let Some(ops) = engine() {
            ops.cancel_operations(&ids);
        }
        let summary = pluralize(ids.len() as u64, "operation");
        return Ok(json!(format!("OK: Cancelled {summary} (kept already-copied files).")));
    }

    let id = require_operation_id(params)?;
    require_operation_exists(&id)?;
    if rollback {
        if let Some(ops) = engine() {
            ops.cancel_write_operation(&id, true);
        }
        Ok(json!(format!(
            "OK: Cancelled operation {id} and rolled back (deleted already-copied files)."
        )))
    } else {
        if let Some(ops) = engine() {
            ops.cancel_operation(&id);
        }
        Ok(json!(format!(
            "OK: Cancelled operation {id} (kept already-copied files)."
        )))
    }
}

fn require_operation_id(params: &Value) -> Result<OperationId, ToolError> {
    params
        .get("operationId")
        .and_then(|v| v.as_str())
        .map(OperationId::from)
        .ok_or_else(|| ToolError::invalid_params("This action requires an 'operationId' parameter"))
}

/// Reject an id the queue can't act on, so an agent gets an honest refusal
/// instead of a silent no-op (the backend treats an unknown id as a no-op).
/// Benign race: an op that settles between this check and the call would have
/// been a no-op anyway.
fn require_operation_exists(operation_id: &OperationId) -> Result<(), ToolError> {
    let operations = engine().map(Operations::list_operations).unwrap_or_default();
    if is_controllable(&operations, operation_id.as_str()) {
        Ok(())
    } else {
        Err(ToolError::invalid_params(format!(
            "Unknown operationId '{operation_id}': it isn't a currently queued, running, or paused operation. See cmdr://state operations."
        )))
    }
}

/// Whether pause / resume / cancel can still do anything to this id.
///
/// Membership in `list_operations()` isn't the test: the snapshot also carries
/// RETAINED FAILURES, which the queue window keeps so it can say why an
/// operation stopped (`cmdr-operations` DETAILS § "Retained failures"). Those
/// ids are long over, so acting on one is precisely the silent no-op this guard
/// exists to turn into a refusal. Typed status match, never a message test.
fn is_controllable(operations: &[OperationSnapshot], operation_id: &str) -> bool {
    operations
        .iter()
        .any(|op| op.operation_id == operation_id && op.status != LifecycleStatus::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations_host::app_engine;
    use cmdr_operations::testing::QueuedOperationFixture;
    use cmdr_operations::{WriteOperationError, WriteOperationType};

    fn row(operation_id: &str, status: LifecycleStatus) -> OperationSnapshot {
        OperationSnapshot {
            operation_id: operation_id.to_string(),
            operation_type: WriteOperationType::Copy,
            status,
            source: Some("/Users/me/photos".to_string()),
            destination: Some("Naspolya".to_string()),
            rollback_support: cmdr_operations::testing::RollbackSupport::Supported,
            reverses: None,
            error: None,
            can_retry: false,
            items: None,
        }
    }

    /// A retained failure parked in the app's engine (the one the `queue` tool asks,
    /// which every app-side cell shares), dropped again on `Drop` so a failing
    /// assertion can't leave it behind for a sibling test.
    struct RetainedFailureGuard(OperationId);

    impl RetainedFailureGuard {
        fn record(operation_id: &str) -> Self {
            let operation_id = OperationId::from(operation_id);
            app_engine().retain_failure(
                &operation_id,
                WriteOperationType::Copy,
                &WriteOperationError::IoError {
                    path: "/Users/me/photos/a.raw".to_string(),
                    message: "disk went away".into(),
                },
            );
            Self(operation_id)
        }
    }

    impl Drop for RetainedFailureGuard {
        fn drop(&mut self) {
            app_engine().dismiss_failed_operation(&self.0);
        }
    }

    /// Carrying on is an answer to a question, so asking an operation that isn't
    /// asking one is refused, ❌ never an `OK` an agent would act on.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn continuing_an_operation_that_is_not_asking_is_refused() {
        let fixture = QueuedOperationFixture::park_on(app_engine(), "mcp-queue-continue");
        let result = execute_queue(&json!({
            "action": "continue_without_gone",
            "operationId": fixture.queued_id().as_str(),
        }))
        .await;
        assert!(result.is_err(), "nothing was asked, so nothing carried on: {result:?}");
    }

    /// The Locked-items answers are refused the same way when nothing asked.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn answering_locked_items_nobody_asked_about_is_refused() {
        let fixture = QueuedOperationFixture::park_on(app_engine(), "mcp-queue-locked");
        for action in ["move_locked", "skip_locked"] {
            let result = execute_queue(&json!({
                "action": action,
                "operationId": fixture.queued_id().as_str(),
            }))
            .await;
            assert!(result.is_err(), "{action}: nothing was asked: {result:?}");
        }
    }

    #[test]
    fn live_operations_are_controllable() {
        let operations = vec![
            row("op-queued", LifecycleStatus::Queued),
            row("op-running", LifecycleStatus::Running),
            row("op-paused", LifecycleStatus::Paused),
        ];
        for id in ["op-queued", "op-running", "op-paused"] {
            assert!(is_controllable(&operations, id), "{id} should still be actionable");
        }
    }

    #[test]
    fn a_retained_failure_is_not_controllable() {
        // It's in the snapshot so the queue window can show why it stopped, but
        // pausing or cancelling it does nothing at all.
        let operations = vec![row("op-failed", LifecycleStatus::Failed)];
        assert!(!is_controllable(&operations, "op-failed"));
    }

    /// A queued operation is live, so the id guard waves it through — and pause
    /// is a documented no-op for it (it isn't admitted, so there is no driver to
    /// park). An agent that reads "OK: Paused …" here goes on to act on a queue
    /// that never stopped.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pausing_a_queued_operation_is_never_reported_as_paused() {
        let fixture = QueuedOperationFixture::park_on(app_engine(), "mcp-queue-pause");
        let result = execute_queue(&json!({ "action": "pause", "operationId": fixture.queued_id().as_str() })).await;

        assert!(
            result.is_err(),
            "a pause that didn't happen must reach the agent as a refusal, got {result:?}"
        );
    }

    /// The whole mapping in one place: an `OK` means the caller's intent holds
    /// (the queue stopped, or already was stopped), and nothing else may be
    /// phrased as one.
    #[test]
    fn only_an_outcome_the_caller_asked_for_answers_ok() {
        for (outcome, expected_ok) in [
            (PauseOutcome::Applied, true),
            (PauseOutcome::AlreadyInState, true),
            (PauseOutcome::NotApplicable, false),
        ] {
            assert_eq!(
                pause_reply(&OperationId::from("op-1"), outcome).is_ok(),
                expected_ok,
                "pause reply for {outcome:?}"
            );
            assert_eq!(
                resume_reply(&OperationId::from("op-1"), outcome).is_ok(),
                expected_ok,
                "resume reply for {outcome:?}"
            );
        }
    }

    /// A pause during the scan reads exactly like any other pause, because it
    /// IS one: the walk parks on the same gate. The reply must not hedge — the
    /// hedged version told users a pause was coming that never arrived.
    #[test]
    fn a_pause_that_happened_says_so_without_hedging() {
        let reply = pause_reply(&OperationId::from("op-1"), PauseOutcome::Applied).expect("an applied pause is an OK");
        let text = reply.as_str().expect("the reply is a string");
        assert_eq!(text, "OK: Paused operation op-1.");
    }

    // ── The sweeps (`pause_all` / `resume_all`) ───────────────────────────────
    //
    // The sweep versions can't be driven end-to-end from a test: the manager is
    // process-global, so a real `pause_all()` here would park a sibling test's
    // operation. What IS testable is everything that decides the answer: the
    // fold from per-operation outcomes into counts (`PauseAllOutcome`, tested in
    // `crates/cmdr-operations/src/manager/tests.rs`) and the wording below.

    /// The defect this closes: an empty sweep used to answer "OK: Paused every
    /// running operation", and an agent acting on that believes the device went
    /// quiet.
    #[test]
    fn a_sweep_that_touched_nothing_never_claims_it_paused_anything() {
        let text = pause_all_reply(PauseAllOutcome::default());
        assert!(!text.contains("Paused"), "got {text}");
        assert!(text.contains("Nothing to pause"), "got {text}");

        let text = resume_all_reply(PauseAllOutcome::default());
        assert!(!text.contains("Resumed"), "got {text}");
        assert!(text.contains("Nothing to resume"), "got {text}");
    }

    #[test]
    fn a_sweep_counts_what_it_paused() {
        let text = pause_all_reply(PauseAllOutcome {
            applied: 3,
            ..PauseAllOutcome::default()
        });
        assert!(text.contains("Paused 3 operations"), "got {text}");
    }

    /// Every count in a mixed sweep gets said. An agent reconciles the answer
    /// against `cmdr://state`, so a reply that mentions only the flips reads as
    /// a sweep that did more than it did.
    #[test]
    fn a_sweep_says_what_happened_to_every_operation_it_asked() {
        let text = pause_all_reply(PauseAllOutcome {
            applied: 1,
            already_in_state: 2,
            not_applicable: 0,
        });
        assert!(text.contains("Paused 1 operation"), "got {text}");
        assert!(text.contains("2 operations were already paused"), "got {text}");
    }

    /// A settle race is the only way a sweep meets `NotApplicable`, and saying
    /// so beats a count the agent can't reconcile with `cmdr://state`.
    #[test]
    fn a_sweep_owns_up_to_the_ones_that_got_away() {
        let text = pause_all_reply(PauseAllOutcome {
            applied: 1,
            not_applicable: 1,
            ..PauseAllOutcome::default()
        });
        assert!(text.contains("finished"), "got {text}");
    }

    /// Every sweep reply names the intent that now holds, so nothing reads as a
    /// bare "done" (`docs/style-guide.md`: no "error" / "failed" either).
    ///
    /// The two `contains` below assert a COPY rule on our own English prose, not error
    /// classification: the wording IS what's under test, so there is no typed value to
    /// match instead. Same shape as the frontend's `operation-start-gate.test.ts`.
    #[test]
    fn no_sweep_reply_uses_the_words_this_app_refuses() {
        let shapes = [
            PauseAllOutcome::default(),
            PauseAllOutcome {
                applied: 2,
                already_in_state: 1,
                not_applicable: 1,
            },
            PauseAllOutcome {
                already_in_state: 4,
                ..PauseAllOutcome::default()
            },
        ];
        for shape in shapes {
            for text in [pause_all_reply(shape), resume_all_reply(shape)] {
                let lowered = text.to_lowercase();
                // allowed-error-string-match: a copy rule on our own prose, not classification
                assert!(!lowered.contains("error"), "got {text}");
                // allowed-error-string-match: a copy rule on our own prose, not classification
                assert!(!lowered.contains("failed"), "got {text}");
                assert!(text.starts_with("OK: "), "got {text}");
            }
        }
    }

    #[test]
    fn an_unknown_id_is_not_controllable() {
        assert!(!is_controllable(
            &[row("op-running", LifecycleStatus::Running)],
            "op-other"
        ));
    }

    #[test]
    fn the_guard_refuses_a_retained_failure_even_though_it_is_listed() {
        let id = format!("mcp-queue-guard-{}", std::process::id());
        let _guard = RetainedFailureGuard::record(&id);

        // The premise: the failure IS in `list_operations()`, so plain membership
        // would wave it through and the tool would answer "OK: Cancelled …" for an
        // operation that ended minutes ago.
        assert!(
            app_engine().list_operations().iter().any(|op| op.operation_id == id),
            "the retained failure should be on the snapshot"
        );
        assert!(
            require_operation_exists(&OperationId::from(id.as_str())).is_err(),
            "the guard must refuse it"
        );
    }
}
