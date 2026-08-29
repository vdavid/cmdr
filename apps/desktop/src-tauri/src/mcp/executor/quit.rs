//! The `quit` tool and the two answers to the quit confirmation.
//!
//! **Quitting goes through the same gate ⌘Q does** (`crate::quit`). A straight
//! `app.exit(0)` here would kill a running transfer with no prompt and no
//! warning, where a person pressing ⌘Q gets a dialog and 15 seconds; an agent
//! must not have a quieter, more destructive exit than the keyboard.
//!
//! Thin adapter over the gate, the shape `queue.rs` and `conflicts.rs` follow:
//! there's no frontend action to dispatch, so there's no ack to invent. The
//! verdict is read straight out of the gate, synchronously, which is why every
//! answer here is backend truth rather than a hope. What this adds is honesty
//! about WHICH way it went: `held` and `quitting` are genuinely different
//! situations for a caller, and an agent that can't tell them apart walks away
//! from a countdown that ends in a stopped transfer.
//!
//! Both answers are reached through the `dialog` tool
//! (`confirm` / `close` on `quit-confirmation`, wired in `dialogs.rs`), so the
//! quit confirmation is answered the way every other dialog is.

use serde_json::json;
use tauri::{AppHandle, Runtime};

use crate::file_system::write_operations::OperationSnapshot;
use crate::quit::{QuitAnswer, QuitOutcome};

use super::{ToolError, ToolResult};

/// Execute the `quit` tool: ask to quit, and report whether the gate held it.
///
/// No ack on either path. Nothing is dispatched to a window here: the gate flips
/// its phase under a mutex before this returns, so the outcome is already true
/// when it's reported. On the way-through path there's also nothing left to wait
/// for, since the process is on its way out.
pub fn execute_quit<R: Runtime>(app: &AppHandle<R>) -> ToolResult {
    match crate::quit::request_quit(app) {
        QuitOutcome::Proceed => {
            // The gate is now `Quitting`, so this comes back through
            // `RunEvent::ExitRequested` and sails straight past it.
            app.exit(0);
            Ok(json!({
                "outcome": "quitting",
                "message": "Nothing that moves data was running, so Cmdr is on its way out.",
            }))
        }
        QuitOutcome::Held {
            operations,
            countdown_ms,
        } => held_report(operations, countdown_ms),
    }
}

/// `dialog confirm quit-confirmation`: quit now, stopping what's running.
pub fn confirm_quit() -> ToolResult {
    report_answer(Answer::Quit, crate::quit::gate().confirm())
}

/// `dialog close quit-confirmation`: call the quit off and keep working. Closing
/// this dialog means the same thing Escape and the × mean in the UI.
pub fn keep_working() -> ToolResult {
    report_answer(Answer::KeepWorking, crate::quit::gate().cancel())
}

/// Which answer was given, so one reporting path can word both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    Quit,
    KeepWorking,
}

/// The reply to a quit the gate is holding: what's holding it, how long is left,
/// and the two calls that answer it.
///
/// The countdown is named twice on purpose. `countdownMs` is the field a caller
/// branches on; the sentence is what stops it from treating a held quit as a
/// finished one and wandering off, because an unanswered countdown ends with the
/// app quitting and these operations stopped.
fn held_report(operations: Vec<OperationSnapshot>, countdown_ms: u32) -> ToolResult {
    let fitted = super::fit_to_result_budget(operations);
    let (total, returned, truncated) = (fitted.total, fitted.items.len(), fitted.truncated);
    let rows = serde_json::to_value(&fitted.items).map_err(|e| ToolError::internal(e.to_string()))?;
    Ok(json!({
        "outcome": "held",
        "countdownMs": countdown_ms,
        "operations": rows,
        "total": total,
        "returned": returned,
        "truncated": truncated,
        "message": format!(
            "Cmdr asked before quitting: it would interrupt {}, still moving data, and the quit confirmation is up with about {}s on the clock. \
             Quit now and stop them with `dialog {{\"action\": \"confirm\", \"type\": \"quit-confirmation\"}}`, \
             or leave them running with `dialog {{\"action\": \"close\", \"type\": \"quit-confirmation\"}}`. \
             Answer one way or the other: if the countdown runs out, Cmdr quits anyway and these operations stop.",
            crate::pluralize::pluralize(total as u64, "operation"),
            countdown_ms / 1_000,
        ),
    }))
}

/// What an answer to the confirmation did, from the gate's own verdict.
///
/// [`QuitAnswer::NoQuitPending`] is a refusal, the mapping `resolve_conflict`
/// follows: nothing happened, and the caller's picture of the app is wrong. It
/// either answered a quit that was already decided (the countdown ran out, or
/// another surface answered first) or one that was never asked.
fn report_answer(answer: Answer, outcome: QuitAnswer) -> ToolResult {
    match outcome {
        QuitAnswer::Answered => Ok(match answer {
            Answer::Quit => json!({
                "outcome": "quitting",
                "message": "Cmdr is stopping every running operation, keeping whatever already copied, and quitting within two seconds.",
            }),
            Answer::KeepWorking => json!({
                "outcome": "kept_working",
                "message": "The quit is off and the countdown is gone, not deferred. The operations are still running; call quit again once they're done.",
            }),
        }),
        QuitAnswer::NoQuitPending => Err(ToolError::invalid_params(match answer {
            Answer::Quit => {
                "No quit is waiting to be confirmed. Either nobody asked to quit, or the answer already landed: \
                 the countdown may have run out, or another surface may have answered first. Call quit to ask."
            }
            Answer::KeepWorking => {
                "No quit is waiting to be called off. Either nobody asked to quit, or the answer already landed: \
                 the countdown may have run out, or another surface may have answered first. \
                 Read cmdr://state to see whether the operations are still running."
            }
        })
        .with_data(json!({ "outcome": no_quit_pending_token() }))),
    }
}

/// The gate's own wire name for [`QuitAnswer::NoQuitPending`], so the token an
/// agent branches on can't drift from the one the frontend sees over IPC.
fn no_quit_pending_token() -> &'static str {
    "no_quit_pending"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::write_operations::{LifecycleStatus, WriteOperationType};

    fn operation(operation_type: WriteOperationType) -> OperationSnapshot {
        OperationSnapshot {
            operation_id: format!("op-{operation_type:?}"),
            operation_type,
            status: LifecycleStatus::Running,
            source: Some("Holiday.mov".to_string()),
            destination: Some("Backup".to_string()),
            supports_rollback: true,
            reverses: None,
            error: None,
        }
    }

    #[test]
    fn a_held_quit_reports_a_typed_outcome_and_what_is_holding_it() {
        // An agent must not have to read the sentence to learn that its quit
        // didn't happen; `no-error-string-match` applies to a sentence an agent
        // parses exactly as it does to our own code.
        let report = held_report(
            vec![operation(WriteOperationType::Copy), operation(WriteOperationType::Move)],
            15_000,
        )
        .expect("a held quit reports");

        assert_eq!(report["outcome"], "held");
        assert_eq!(report["countdownMs"], 15_000);
        assert_eq!(report["total"], 2);
        assert_eq!(report["returned"], 2);
        assert_eq!(report["truncated"], false);
        let operations = report["operations"].as_array().expect("the operations are a list");
        assert_eq!(operations.len(), 2);
        assert_eq!(operations[0]["operationType"], "copy");
    }

    #[test]
    fn a_held_quit_names_both_calls_that_answer_it() {
        // The only discovery path there is: the tool is what tells an agent the
        // confirmation exists and how to answer it either way.
        let report = held_report(vec![operation(WriteOperationType::Copy)], 15_000).expect("a held quit reports");
        let sentence = report["message"].as_str().expect("the report carries a message");
        assert!(sentence.contains("\"action\": \"confirm\""), "{sentence}");
        assert!(sentence.contains("\"action\": \"close\""), "{sentence}");
        assert!(sentence.contains("quit-confirmation"), "{sentence}");
        // And it says what silence costs, which is the whole risk of the flow.
        assert!(sentence.contains("countdown runs out"), "{sentence}");
    }

    #[test]
    fn the_count_reads_grammatically_at_one_and_at_many() {
        let one = held_report(vec![operation(WriteOperationType::Copy)], 15_000).expect("a held quit reports");
        let sentence = one["message"].as_str().expect("the report carries a message");
        assert!(sentence.contains("interrupt 1 operation,"), "{sentence}");

        let two = held_report(
            vec![operation(WriteOperationType::Copy), operation(WriteOperationType::Move)],
            15_000,
        )
        .expect("a held quit reports");
        let sentence = two["message"].as_str().expect("the report carries a message");
        assert!(sentence.contains("interrupt 2 operations,"), "{sentence}");
    }

    #[test]
    fn each_answer_reports_what_it_did_as_a_typed_outcome() {
        let quit = report_answer(Answer::Quit, QuitAnswer::Answered).expect("a confirmed quit is an OK");
        assert_eq!(quit["outcome"], "quitting");

        let kept = report_answer(Answer::KeepWorking, QuitAnswer::Answered).expect("calling it off is an OK");
        assert_eq!(kept["outcome"], "kept_working");
    }

    #[test]
    fn an_answer_that_found_no_quit_is_a_refusal_carrying_the_verdict() {
        // An OK here would leave an agent believing it stopped a quit that is
        // already tearing down, or called off one nobody asked for.
        for answer in [Answer::Quit, Answer::KeepWorking] {
            let error =
                report_answer(answer, QuitAnswer::NoQuitPending).expect_err("nothing happened, so it can't answer OK");
            let data = error.data.expect("the refusal carries data");
            assert_eq!(data["outcome"], "no_quit_pending", "for {answer:?}");
        }
    }

    #[test]
    fn the_refusal_token_matches_the_gates_own_wire_name() {
        // The frontend branches on the serde name over IPC. One name per
        // outcome, or the two surfaces disagree about what happened.
        let serde_name = serde_json::to_value(QuitAnswer::NoQuitPending).expect("the outcome serializes");
        assert_eq!(serde_name.as_str(), Some(no_quit_pending_token()));
    }
}
