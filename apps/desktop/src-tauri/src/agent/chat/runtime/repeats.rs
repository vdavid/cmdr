//! The turn's memory of tool calls that already came back with a problem, so an identical
//! retry doesn't buy the same answer at the price of a full provider round trip.
//!
//! A `propose_rename_plan` call missing `volumeId` was refused, and the model re-sent the
//! byte-identical call eight times until the round cap ended the turn: about 90 seconds
//! and eight round trips spent on one broken payload, and from the user's side the agent just
//! stopped answering.
//!
//! **Why it can't fire on a legitimate repeat.** Only a call that came back with a PROBLEM is
//! remembered (judged by [`dispatch_ok`](super::dispatch::dispatch_ok), which reads our own
//! typed result keys, never wording). So:
//!
//! - **Re-fetching an elided result** is a repeat of a call that SUCCEEDED, which was never
//!   recorded. The system prompt tells the model to make exactly that call again, so it runs again
//!   every time it's asked for.
//! - **The retry our own refusals ask for**: `copy` says to select something first, so `copy`,
//!   `select`, `copy` has to run. A successful non-read moves the turn on
//!   ([`FailedCalls::moved_on`]), and the ledger starts over from there.
//! - **Paging** varies `offset`, and the key is the tool name plus the whole arguments
//!   object, so a different page is a different call.
//!
//! **What it does cost.** A call that failed for a passing reason gets no second execution
//! until something changes. That is the deliberate trade: the model is handed the original
//! problem again rather than a silence, and varying the arguments at all (fewer paths, a
//! narrower window) dispatches normally. Against that, the turn a user is waiting on stops
//! burning its whole budget on one payload the provider will keep re-sending.
//!
//! **A person's "no" sticks for the whole turn.** A decline sits in its own ledger, which moving on
//! doesn't touch: a model that navigates somewhere and asks again would otherwise put the question
//! back in front of someone who already answered it.

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

/// The per-turn ledger, keyed by [`AgentToolCall::identity_key`]: the tool's wire name plus its
/// whole arguments object, the same key the rail's repeat mark uses.
#[derive(Default)]
pub(super) struct FailedCalls {
    /// Since the turn last moved on.
    seen: HashMap<String, Failure>,
    /// What a person said no to, for the whole turn.
    declined: HashMap<String, Failure>,
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
        let key = call.identity_key();
        if let Some(failure) = self.declined.get_mut(&key) {
            let content = annotate(&failure.content, DECLINED);
            if failure.warned {
                return Repeat::Stuck(content);
            }
            failure.warned = true;
            return Repeat::Refuse(content);
        }
        let Some(failure) = self.seen.get_mut(&key) else {
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
        let ledger = if a_person_declined(content) {
            &mut self.declined
        } else {
            &mut self.seen
        };
        ledger.insert(
            call.identity_key(),
            Failure {
                content: content.clone(),
                warned: false,
            },
        );
    }

    /// A call changed something, so what failed before it may answer differently now. A person's no
    /// stays.
    pub(super) fn moved_on(&mut self) {
        self.seen.clear();
    }
}

const REFUSED: &str = "This exact call already came back with this, and it answers the same way every time. Change the arguments \
     or tell the user what you couldn't do; sending it again does nothing.";

const DECLINED: &str = "The person already said no to this, so it won't run or ask them again this turn. Tell them what you'd \
     have done, and let them ask if they change their mind.";

const STUCK: &str =
    "This exact call has now been sent three times with nothing changed, so no further tool call will run this turn.";

/// Whether this result is a person's no, from the typed token `tools/ask.rs` answers with.
fn a_person_declined(content: &Value) -> bool {
    content.pointer("/data/refusal").and_then(Value::as_str) == Some("declined")
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
    fn moving_on_lets_a_failed_call_run_again() {
        let mut failed = FailedCalls::default();
        let call = call(ToolId::Copy, json!({ "pane": "left" }));
        failed.record_failure(&call, &problem());
        failed.moved_on();
        assert!(matches!(failed.judge(&call), Repeat::Fresh));
    }

    /// Moving on is what would otherwise put the question back in front of someone who answered it.
    #[test]
    fn a_persons_no_outlasts_moving_on_and_says_so() {
        let mut failed = FailedCalls::default();
        let call = call(ToolId::Quit, json!({}));
        let declined =
            json!({ "problem": "The person said no, so nothing happened.", "data": { "refusal": "declined" } });
        failed.record_failure(&call, &declined);
        failed.moved_on();

        let Repeat::Refuse(content) = failed.judge(&call) else {
            panic!("a declined call never runs again this turn");
        };
        assert_eq!(content["data"]["refusal"], "declined");
        assert_eq!(content["guidance"], DECLINED);
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
    fn nested_argument_key_order_does_not_dodge_the_repeat_breaker() {
        // serde_json keeps insertion order in this build (genai turns on `preserve_order`), so
        // the key sorts its arguments itself, at every depth. Regression anchor: with insertion
        // order, a model reshuffling an inner object's keys looped on a failing call unchecked.
        let mut failed = FailedCalls::default();
        let first = json!({ "paths": [{ "path": "/a", "volumeId": "root" }], "limit": 5 });
        failed.record_failure(&call(ToolId::ImageFacts, first), &problem());
        let reordered = json!({ "limit": 5, "paths": [{ "volumeId": "root", "path": "/a" }] });
        assert!(matches!(
            failed.judge(&call(ToolId::ImageFacts, reordered)),
            Repeat::Refuse(_)
        ));
    }

    #[test]
    fn two_tools_carrying_the_same_arguments_are_two_records() {
        let mut failed = FailedCalls::default();
        failed.record_failure(&call(ToolId::ListDir, json!({ "path": "/" })), &problem());
        let other = call(ToolId::FolderImportance, json!({ "path": "/" }));
        assert!(matches!(failed.judge(&other), Repeat::Fresh));
    }
}
