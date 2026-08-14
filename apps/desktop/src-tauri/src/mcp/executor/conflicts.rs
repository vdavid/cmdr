//! The `resolve_conflict` tool: answer ONE name clash a running operation is
//! parked on.
//!
//! The counterpart to `dialog confirm`'s bulk policies. Those decide every clash
//! an operation will ever hit before it starts; this decides the one it is
//! standing on, which is the only way to reach the state a person reaches by
//! leaving the policy on "Ask for each" and clicking Skip on one file.
//!
//! Thin adapter over `write_operations::resolve_write_conflict`, like `queue` is
//! over the manager: no FE action to dispatch, so no ack to invent. What it adds
//! is honesty about the ARBITRATION. The backend answers `Resolved` /
//! `AlreadyResolved` / `StaleAnswer` / `NoPendingConflict` / `UnknownOperation`,
//! and those mean genuinely different things to a caller — an agent that can't
//! tell "I answered it" from "somebody beat me to it" from "the question I
//! answered is over and there's a new one" will act on a transfer that isn't
//! where it thinks. So the outcome crosses the wire as a typed `outcome` field,
//! never only as prose (`no-error-string-match` applies to a sentence an agent
//! parses exactly as it does to our own code).
//!
//! Discovery — which operation is asking, about which file, under which
//! `conflictId` — is the `pendingConflict:` block in `cmdr://state` under
//! `operations:` (`resources/operations.rs`).

use serde_json::{Value, json};

use super::{ToolError, ToolResult};
use crate::file_system::write_operations::{
    ConflictId, ConflictResolution, ConflictResolutionOutcome, resolve_write_conflict,
};

pub async fn execute_resolve_conflict(params: &Value) -> ToolResult {
    let operation_id = params
        .get("operationId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ToolError::invalid_params("Missing 'operationId' parameter. See cmdr://state operations."))?;

    // Required, never "whatever is pending": naming the clash is what stops an
    // answer meant for one file from deciding the next one, which is the bug
    // this whole surface is shaped around.
    let conflict_id = params
        .get("conflictId")
        .and_then(Value::as_u64)
        .map(ConflictId)
        .ok_or_else(|| {
            ToolError::invalid_params(
                "Missing 'conflictId' parameter: answer the clash by the id on it, from the pendingConflict block in cmdr://state operations.",
            )
        })?;

    let resolution = parse_resolution(params.get("resolution").and_then(|v| v.as_str()))?;
    let apply_to_all = params.get("applyToAll").and_then(Value::as_bool).unwrap_or(false);

    let outcome = resolve_write_conflict(operation_id, conflict_id, resolution, apply_to_all);
    report(operation_id, conflict_id, outcome)
}

/// The answer an agent may give. Deliberately NOT `stop`: that's the policy that
/// raises the question, and offering it as an answer would park the operation on
/// the same clash forever.
fn parse_resolution(raw: Option<&str>) -> Result<ConflictResolution, ToolError> {
    match raw {
        Some("skip") => Ok(ConflictResolution::Skip),
        Some("overwrite") => Ok(ConflictResolution::Overwrite),
        Some("rename") => Ok(ConflictResolution::Rename),
        Some("overwrite_smaller") => Ok(ConflictResolution::OverwriteSmaller),
        Some("overwrite_older") => Ok(ConflictResolution::OverwriteOlder),
        Some(other) => Err(ToolError::invalid_params(format!(
            "resolution must be 'skip', 'overwrite', 'rename', 'overwrite_smaller', or 'overwrite_older' (got '{other}')"
        ))),
        None => Err(ToolError::invalid_params(
            "Missing 'resolution' parameter: one of 'skip', 'overwrite', 'rename', 'overwrite_smaller', 'overwrite_older'.",
        )),
    }
}

/// The agent-facing answer, from what the backend's arbitration actually did.
///
/// An `OK` means the clash is settled: either this answer is the one the
/// operation carried on with, or somebody answered the SAME clash first and it
/// carried on with theirs. Everything else is a refusal, because nothing
/// happened and the caller's picture of the operation is wrong — the mapping
/// `queue`'s `pause_reply` follows, for the same reason.
fn report(operation_id: &str, conflict_id: ConflictId, outcome: ConflictResolutionOutcome) -> ToolResult {
    let token = outcome_token(outcome);
    match outcome {
        ConflictResolutionOutcome::Resolved => Ok(json!({
            "outcome": token,
            "operationId": operation_id,
            "conflictId": conflict_id.0,
            "message": format!("Answered clash {} of operation {operation_id}; it carried on with your answer.", conflict_id.0),
        })),
        ConflictResolutionOutcome::AlreadyResolved => Ok(json!({
            "outcome": token,
            "operationId": operation_id,
            "conflictId": conflict_id.0,
            "message": format!(
                "Clash {} of operation {operation_id} was already answered by another surface, so the operation carried on with THAT answer, not yours.",
                conflict_id.0
            ),
        })),
        ConflictResolutionOutcome::StaleAnswer => Err(refusal(
            format!(
                "Operation {operation_id} has moved past clash {}, so this answer changed nothing. Re-read cmdr://state operations: it may be parked on a different clash now, with its own conflictId.",
                conflict_id.0
            ),
            operation_id,
            conflict_id,
            token,
        )),
        ConflictResolutionOutcome::NoPendingConflict => Err(refusal(
            format!(
                "Operation {operation_id} isn't waiting on a name clash: it hasn't raised one, or a cancel took the pending one away."
            ),
            operation_id,
            conflict_id,
            token,
        )),
        ConflictResolutionOutcome::UnknownOperation => Err(refusal(
            format!(
                "Unknown operationId '{operation_id}': it isn't a currently queued, running, or paused operation. See cmdr://state operations."
            ),
            operation_id,
            conflict_id,
            token,
        )),
    }
}

/// A refusal that carries the arbitration verdict as typed `data`, so the agent
/// branches on the outcome rather than on the sentence.
fn refusal(message: String, operation_id: &str, conflict_id: ConflictId, token: &str) -> ToolError {
    ToolError::invalid_params(message).with_data(json!({
        "outcome": token,
        "operationId": operation_id,
        "conflictId": conflict_id.0,
    }))
}

/// The outcome's own wire name, from its serde representation — the same token
/// the FE sees over IPC, so a new variant can't drift into a hand-written string
/// here.
fn outcome_token(outcome: ConflictResolutionOutcome) -> &'static str {
    match outcome {
        ConflictResolutionOutcome::Resolved => "resolved",
        ConflictResolutionOutcome::AlreadyResolved => "already_resolved",
        ConflictResolutionOutcome::StaleAnswer => "stale_answer",
        ConflictResolutionOutcome::NoPendingConflict => "no_pending_conflict",
        ConflictResolutionOutcome::UnknownOperation => "unknown_operation",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_outcome_token_matches_its_serde_name() {
        // The tokens are what an agent branches on, and the FE branches on the
        // serde ones over IPC. One name per outcome, or the two surfaces
        // disagree about what happened.
        for outcome in [
            ConflictResolutionOutcome::Resolved,
            ConflictResolutionOutcome::AlreadyResolved,
            ConflictResolutionOutcome::StaleAnswer,
            ConflictResolutionOutcome::NoPendingConflict,
            ConflictResolutionOutcome::UnknownOperation,
        ] {
            let serde_name = serde_json::to_value(outcome).expect("the outcome serializes");
            assert_eq!(serde_name.as_str(), Some(outcome_token(outcome)), "for {outcome:?}");
        }
    }

    #[test]
    fn only_a_settled_clash_answers_ok() {
        // An agent that reads OK goes on believing the transfer is moving. Only
        // the two outcomes where the clash IS decided may say so.
        for (outcome, expected_ok) in [
            (ConflictResolutionOutcome::Resolved, true),
            (ConflictResolutionOutcome::AlreadyResolved, true),
            (ConflictResolutionOutcome::StaleAnswer, false),
            (ConflictResolutionOutcome::NoPendingConflict, false),
            (ConflictResolutionOutcome::UnknownOperation, false),
        ] {
            assert_eq!(
                report("op-1", ConflictId(2), outcome).is_ok(),
                expected_ok,
                "report for {outcome:?}"
            );
        }
    }

    #[test]
    fn a_refusal_carries_the_verdict_as_typed_data() {
        let error = report("op-1", ConflictId(2), ConflictResolutionOutcome::StaleAnswer)
            .expect_err("a stale answer is a refusal");
        let data = error.data.expect("the refusal carries data");
        assert_eq!(data["outcome"], "stale_answer");
        assert_eq!(data["conflictId"], 2);
    }

    #[test]
    fn a_successful_answer_reports_the_outcome_as_a_field_not_only_prose() {
        let result = report("op-1", ConflictId(2), ConflictResolutionOutcome::AlreadyResolved)
            .expect("someone else answering the same clash still leaves it settled");
        assert_eq!(result["outcome"], "already_resolved");
        assert_eq!(result["conflictId"], 2);
    }

    #[test]
    fn stop_is_not_an_answer() {
        // It's the policy that RAISES the question. Accepting it as an answer
        // would park the operation on the same clash again.
        assert!(parse_resolution(Some("stop")).is_err());
        assert!(parse_resolution(None).is_err());
        assert_eq!(
            parse_resolution(Some("skip")).expect("skip parses"),
            ConflictResolution::Skip
        );
        assert_eq!(
            parse_resolution(Some("overwrite_older")).expect("the conditional policies parse too"),
            ConflictResolution::OverwriteOlder
        );
    }

    #[tokio::test]
    async fn an_answer_that_names_no_clash_is_refused_before_it_reaches_the_backend() {
        let result = execute_resolve_conflict(&json!({ "operationId": "op-1", "resolution": "skip" })).await;
        assert!(result.is_err(), "a missing conflictId can't be guessed: {result:?}");
    }
}
