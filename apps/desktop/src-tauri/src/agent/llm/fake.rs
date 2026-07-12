//! A deterministic, zero-network [`AgentLlm`] for testing the entire runtime and UI.
//!
//! A [`FakeAgentLlm`] is scripted with a sequence of [`ScriptedTurn`]s, one consumed
//! per [`AgentLlm::respond`] call, and records every `messages` slice it was handed
//! so a test can assert the exact assembled prompts (prefix stability, elision, the
//! envelope) the runtime built — see [`FakeAgentLlm::calls_seen`].

use std::collections::VecDeque;
use std::sync::Mutex;

use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{self, StreamExt};
use tokio_util::sync::CancellationToken;

use crate::ignore_poison::IgnorePoison;

use super::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentUsage,
    ToolDeclaration, ToolId,
};
use super::{AgentDeltaStream, AgentLlm};

/// One scripted assistant turn.
pub enum ScriptedTurn {
    /// Stream a final answer as these text chunks, then complete.
    Say(Vec<String>),
    /// Emit these tool calls (typed), then stop with `ToolCall` and await results.
    CallTools(Vec<(ToolId, serde_json::Value)>),
    /// Emit a tool call under an arbitrary raw name (e.g. `"delete"`), to exercise
    /// the runtime's read-only parse gate (M4). The name resolves through
    /// [`ToolId::from_wire_name`], so an unknown name becomes [`ToolId::Unrecognized`].
    CallRawTool(String, serde_json::Value),
    /// Fail the call before streaming with this typed error (no key, provider down).
    Fail(AgentLlmError),
}

/// A scripted, deterministic agent LLM. Cheap to build; drive it with `respond`.
pub struct FakeAgentLlm {
    turns: Mutex<VecDeque<ScriptedTurn>>,
    calls_seen: Mutex<Vec<Vec<AgentMessage>>>,
}

impl FakeAgentLlm {
    /// Builds a fake that plays `turns` in order, one per `respond` call.
    pub fn script(turns: Vec<ScriptedTurn>) -> Self {
        Self {
            turns: Mutex::new(turns.into()),
            calls_seen: Mutex::new(Vec::new()),
        }
    }

    /// Every `messages` slice handed to `respond`, in call order. Asserts the exact
    /// assembled prompts the runtime built.
    pub fn calls_seen(&self) -> Vec<Vec<AgentMessage>> {
        self.calls_seen.lock_ignore_poison().clone()
    }
}

impl AgentLlm for FakeAgentLlm {
    fn respond<'a>(
        &'a self,
        _system: &'a str,
        _tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            self.calls_seen.lock_ignore_poison().push(messages.to_vec());

            let turn = self
                .turns
                .lock_ignore_poison()
                .pop_front()
                .ok_or_else(|| AgentLlmError::Provider("fake: script exhausted".to_string()))?;

            let deltas = match turn {
                ScriptedTurn::Fail(error) => return Err(error),
                ScriptedTurn::Say(chunks) => say_deltas(chunks),
                ScriptedTurn::CallTools(calls) => tool_call_deltas(calls.into_iter().collect()),
                ScriptedTurn::CallRawTool(name, args) => tool_call_deltas(vec![(ToolId::from_wire_name(&name), args)]),
            };

            let cancel_signal = cancel.clone();
            let stream = stream::iter(deltas)
                .take_until(async move { cancel_signal.cancelled().await })
                .boxed();
            Ok(stream)
        }
        .boxed()
    }
}

/// Streams the chunks as text deltas, then an `End` carrying the joined answer.
fn say_deltas(chunks: Vec<String>) -> Vec<Result<AgentDelta, AgentLlmError>> {
    let joined = chunks.concat();
    let mut deltas: Vec<Result<AgentDelta, AgentLlmError>> =
        chunks.into_iter().map(|chunk| Ok(AgentDelta::Text(chunk))).collect();
    let message = AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::Text(joined)],
        at: 0,
    };
    deltas.push(Ok(AgentDelta::End {
        stop: AgentStopReason::Completed,
        usage: AgentUsage::default(),
        message,
    }));
    deltas
}

/// Emits a `ToolCallStarted` per call, then an `End` (stop `ToolCall`) whose message
/// carries the tool-call parts for the runtime to dispatch (and refuse, for an
/// `Unrecognized` name).
fn tool_call_deltas(calls: Vec<(ToolId, serde_json::Value)>) -> Vec<Result<AgentDelta, AgentLlmError>> {
    let mut deltas: Vec<Result<AgentDelta, AgentLlmError>> = Vec::new();
    let mut parts = Vec::new();
    for (index, (tool, arguments)) in calls.into_iter().enumerate() {
        let call_id = format!("fake-call-{index}");
        deltas.push(Ok(AgentDelta::ToolCallStarted {
            call_id: call_id.clone(),
            tool: tool.clone(),
        }));
        parts.push(AgentPart::ToolCall(AgentToolCall {
            call_id,
            tool,
            arguments,
            reasoning: None,
        }));
    }
    let message = AgentMessage {
        role: AgentRole::Assistant,
        parts,
        at: 0,
    };
    deltas.push(Ok(AgentDelta::End {
        stop: AgentStopReason::ToolCall,
        usage: AgentUsage::default(),
        message,
    }));
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user_message(text: &str) -> AgentMessage {
        AgentMessage {
            role: AgentRole::User,
            parts: vec![AgentPart::Text(text.to_string())],
            at: 1,
        }
    }

    async fn collect(stream: AgentDeltaStream) -> Vec<AgentDelta> {
        stream
            .map(|item| item.expect("fake never streams errors"))
            .collect()
            .await
    }

    #[tokio::test]
    async fn sequences_turns_and_records_every_call() {
        let fake = FakeAgentLlm::script(vec![
            ScriptedTurn::CallTools(vec![(ToolId::Placeholder, json!({ "path": "/" }))]),
            ScriptedTurn::Say(vec!["Hello ".into(), "world".into()]),
        ]);
        let cancel = CancellationToken::new();

        let first_prompt = vec![user_message("what is big?")];
        let first = fake
            .respond("sys", &[], &first_prompt, cancel.clone())
            .await
            .expect("first turn starts");
        let first_deltas = collect(first).await;
        assert!(matches!(first_deltas[0], AgentDelta::ToolCallStarted { .. }));
        let AgentDelta::End { stop, message, .. } = first_deltas.last().unwrap() else {
            panic!("expected an End delta");
        };
        assert_eq!(*stop, AgentStopReason::ToolCall);
        assert!(matches!(message.parts[0], AgentPart::ToolCall(_)));

        let second_prompt = vec![user_message("what is big?"), user_message("and small?")];
        let second = fake
            .respond("sys", &[], &second_prompt, cancel.clone())
            .await
            .expect("second turn starts");
        let second_deltas = collect(second).await;
        assert_eq!(second_deltas[0], AgentDelta::Text("Hello ".into()));
        assert_eq!(second_deltas[1], AgentDelta::Text("world".into()));
        let AgentDelta::End { stop, message, .. } = second_deltas.last().unwrap() else {
            panic!("expected an End delta");
        };
        assert_eq!(*stop, AgentStopReason::Completed);
        assert_eq!(message.parts, vec![AgentPart::Text("Hello world".into())]);

        // calls_seen records each assembled prompt exactly, in order.
        let seen = fake.calls_seen();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], first_prompt);
        assert_eq!(seen[1], second_prompt);
    }

    #[tokio::test]
    async fn call_raw_tool_carries_unrecognized_tool_id() {
        // The seam that M4's read-only negative test builds on: a raw name the agent
        // doesn't know resolves to `Unrecognized`, which dispatch will refuse.
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::CallRawTool("delete".into(), json!({}))]);
        let stream = fake
            .respond("sys", &[], &[], CancellationToken::new())
            .await
            .expect("turn starts");
        let deltas = collect(stream).await;

        let AgentDelta::ToolCallStarted { tool, .. } = &deltas[0] else {
            panic!("expected a ToolCallStarted");
        };
        assert_eq!(*tool, ToolId::Unrecognized("delete".into()));
        assert!(!tool.is_known());

        let AgentDelta::End { message, .. } = deltas.last().unwrap() else {
            panic!("expected an End delta");
        };
        let AgentPart::ToolCall(call) = &message.parts[0] else {
            panic!("expected a tool call part");
        };
        assert_eq!(call.tool, ToolId::Unrecognized("delete".into()));
    }

    #[tokio::test]
    async fn fail_turn_returns_typed_error_before_streaming() {
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::Fail(AgentLlmError::NoKey)]);
        // Match on the result rather than `expect_err` — the Ok type is a boxed
        // stream, which isn't `Debug`.
        let result = fake.respond("sys", &[], &[], CancellationToken::new()).await;
        assert!(
            matches!(result, Err(AgentLlmError::NoKey)),
            "a Fail turn errors with its typed error"
        );
    }
}
