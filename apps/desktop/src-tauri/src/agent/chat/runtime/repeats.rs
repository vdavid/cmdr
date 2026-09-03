//! The turn's memory of tool calls that already came back with a problem, so an identical
//! retry doesn't buy the same answer at the price of a full provider round trip.
//!
//! A `propose_rename_plan` call missing `volumeId` was refused, and the model re-sent the
//! byte-identical call eight times until `MAX_TOOL_TURNS` ended the turn: about 90 seconds
//! and eight round trips spent on one broken payload, and from the user's side the agent just
//! stopped answering.
//!
//! **Why it can't fire on a legitimate repeat.** Only a call that came back with a PROBLEM is
//! remembered (judged by [`dispatch_ok`](super::dispatch::dispatch_ok), which reads our own
//! typed result keys, never wording). So:
//!
//! - **Re-fetching an elided result** is a repeat of a call that SUCCEEDED, which was never
//!   recorded. The system prompt tells the model to make exactly that call again, and every
//!   agent tool is an idempotent local read, so it runs again every time it's asked for.
//! - **Paging** varies `offset`, and the key is the tool name plus the whole arguments
//!   object, so a different page is a different call.
//!
//! **What it does cost.** A call that failed for a passing reason gets no second execution
//! inside the same turn. That is the deliberate trade: the model is handed the original
//! problem again rather than a silence, and varying the arguments at all (fewer paths, a
//! narrower window) dispatches normally. Against that, the turn a user is waiting on stops
//! burning its whole budget on one payload the provider will keep re-sending.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::agent::llm::types::AgentToolCall;

/// What the driver should do with one tool call.
pub(super) enum Repeat {
    /// Nothing identical has failed this turn: dispatch it.
    Fresh,
    /// Identical to a call that already came back with a problem. Hand `content` back without
    /// dispatching, and let the turn continue: the model has one chance to do something else.
    Refuse(Value),
    /// Identical to a call the model was ALREADY told not to repeat. Hand `content` back so
    /// the transcript stays well formed, then end the turn.
    Stuck(Value),
}

/// The per-turn ledger. Keyed by the tool's wire name plus its whole arguments object, both
/// serialized: `serde_json`'s `Map` is a `BTreeMap`, so two calls that mean the same thing
/// render the same bytes whatever order the provider emitted their keys in.
#[derive(Default)]
pub(super) struct FailedCalls {
    seen: HashMap<String, Failure>,
}

struct Failure {
    /// What the call came back with, handed to the model again so it still reads what to fix.
    content: Value,
    /// Whether the model has already been told that repeating this changes nothing.
    warned: bool,
}

impl FailedCalls {
    /// Whether `call` may run, and what to hand back if it may not.
    pub(super) fn judge(&mut self, call: &AgentToolCall) -> Repeat {
        let Some(failure) = self.seen.get_mut(&key(call)) else {
            return Repeat::Fresh;
        };
        if failure.warned {
            return Repeat::Stuck(annotate(&failure.content, STUCK));
        }
        failure.warned = true;
        Repeat::Refuse(annotate(&failure.content, REFUSED))
    }

    /// Remember a call that came back with a problem. Called only for a dispatched call, so a
    /// synthesized repeat can't deepen its own record.
    pub(super) fn record_failure(&mut self, call: &AgentToolCall, content: &Value) {
        self.seen.insert(
            key(call),
            Failure {
                content: content.clone(),
                warned: false,
            },
        );
    }
}

const REFUSED: &str = "This exact call already came back with this, and it answers the same way every time. Change the arguments \
     or tell the user what you couldn't do; sending it again does nothing.";

const STUCK: &str = "This exact call has now been sent three times with nothing changed, so the turn ends here.";

fn key(call: &AgentToolCall) -> String {
    // A unit separator, so no tool name plus arguments can collide with another pair.
    format!("{}\u{1f}{}", call.tool.as_wire_name(), call.arguments)
}

/// The failed result plus the two keys that make it a repeat notice. The original content is
/// kept whole, so `dispatch_ok` still reads it as a problem and the model still reads what to
/// fix; a non-object result (no tool returns one, but the type allows it) is carried as-is.
fn annotate(content: &Value, guidance: &str) -> Value {
    let mut annotated = content.clone();
    match annotated.as_object_mut() {
        Some(map) => {
            map.insert("repeatedCall".to_string(), json!(true));
            map.insert("guidance".to_string(), json!(guidance));
            annotated
        }
        None => json!({ "problem": content, "repeatedCall": true, "guidance": guidance }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::types::ToolId;

    fn call(tool: ToolId, arguments: Value) -> AgentToolCall {
        AgentToolCall {
            call_id: "call-1".to_string(),
            tool,
            arguments,
            reasoning: None,
        }
    }

    fn problem() -> Value {
        json!({ "problem": "list_dir needs path. It takes limit and path." })
    }

    #[test]
    fn a_failing_call_runs_once_is_refused_once_then_ends_the_turn() {
        let mut failed = FailedCalls::default();
        let call = call(ToolId::ListDir, json!({ "limit": 10 }));

        assert!(matches!(failed.judge(&call), Repeat::Fresh));
        failed.record_failure(&call, &problem());
        let Repeat::Refuse(content) = failed.judge(&call) else {
            panic!("the second identical call is refused, not dispatched");
        };
        assert_eq!(
            content["problem"],
            problem()["problem"],
            "the original problem survives"
        );
        assert_eq!(content["repeatedCall"], true);
        assert!(matches!(failed.judge(&call), Repeat::Stuck(_)));
    }

    #[test]
    fn the_key_covers_the_arguments_so_a_different_page_is_a_different_call() {
        let mut failed = FailedCalls::default();
        let first = call(ToolId::ListDir, json!({ "path": "/", "offset": 0 }));
        failed.record_failure(&first, &problem());
        let next_page = call(ToolId::ListDir, json!({ "path": "/", "offset": 50 }));
        assert!(matches!(failed.judge(&next_page), Repeat::Fresh));
    }

    #[test]
    fn argument_key_order_does_not_make_two_identical_calls_look_different() {
        // A provider is free to emit an object's keys in any order; the same call must land
        // on the same record either way.
        let mut failed = FailedCalls::default();
        failed.record_failure(&call(ToolId::ListDir, json!({ "a": 1, "b": 2 })), &problem());
        let reordered = call(ToolId::ListDir, json!({ "b": 2, "a": 1 }));
        assert!(matches!(failed.judge(&reordered), Repeat::Refuse(_)));
    }

    #[test]
    fn two_tools_carrying_the_same_arguments_are_two_records() {
        let mut failed = FailedCalls::default();
        failed.record_failure(&call(ToolId::ListDir, json!({ "path": "/" })), &problem());
        let other = call(ToolId::FolderImportance, json!({ "path": "/" }));
        assert!(matches!(failed.judge(&other), Repeat::Fresh));
    }
}
