//! A deterministic, zero-network [`AgentLlm`] for testing the entire runtime and UI.
//!
//! A [`FakeAgentLlm`] is scripted with a sequence of [`ScriptedTurn`]s, one consumed
//! per [`AgentLlm::respond`] call (or picks each from the prompt, [`FakeAgentLlm::by_prompt`]),
//! and records every `messages` slice it was handed
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
    /// Narrate, then call tools: the text chunks and call starts interleave on the wire, while the
    /// `End` message holds ONE text, then every call, the order genai's `StreamEnd` gives a real
    /// provider's round (`chat/DETAILS.md`).
    SayThenCall {
        say: Vec<String>,
        calls: Vec<(ToolId, serde_json::Value)>,
    },
    /// Emit a tool call under an arbitrary raw name (e.g. `"format_drive"`), to exercise
    /// the runtime's parse gate. The name resolves through
    /// [`ToolId::from_wire_name`], so an unknown name becomes [`ToolId::Unrecognized`].
    CallRawTool(String, serde_json::Value),
    /// Fail the call before streaming with this typed error (no key, provider down).
    Fail(AgentLlmError),
}

/// Picks a turn from the prompt the fake was handed.
type PickTurn = Box<dyn Fn(&[AgentMessage]) -> ScriptedTurn + Send + Sync>;

/// Where a fake's turns come from.
enum Script {
    /// Played in order, each with the usage its `End` reports.
    Queue(VecDeque<(ScriptedTurn, AgentUsage)>),
    /// Picked per call from the prompt, each reporting zero usage.
    ByPrompt(PickTurn),
}

/// A scripted, deterministic agent LLM. Cheap to build; drive it with `respond`.
pub struct FakeAgentLlm {
    script: Mutex<Script>,
    calls_seen: Mutex<Vec<Vec<AgentMessage>>>,
    pace: Option<Pace>,
}

/// How slowly a paced fake streams, so a streaming or stopped state can be held still for a
/// screenshot or a person's QA ([`FakeAgentLlm::paced`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pace {
    /// The wait before each text delta.
    pub per_delta: std::time::Duration,
    /// An extra wait before the LAST text delta, so the streaming state holds with text on
    /// screen and the end still to come.
    pub hold_before_last: std::time::Duration,
}

impl FakeAgentLlm {
    /// Builds a fake that plays `turns` in order, one per `respond` call, each reporting zero
    /// usage.
    pub fn script(turns: Vec<ScriptedTurn>) -> Self {
        Self::script_with_usage(turns.into_iter().map(|turn| (turn, AgentUsage::default())).collect())
    }

    /// The same, with the usage each turn's `End` reports, so what a turn spends adds up to real
    /// numbers.
    pub fn script_with_usage(turns: Vec<(ScriptedTurn, AgentUsage)>) -> Self {
        Self {
            script: Mutex::new(Script::Queue(turns.into())),
            calls_seen: Mutex::new(Vec::new()),
            pace: None,
        }
    }

    /// Builds a fake that picks each turn from the prompt it's handed, so what it plays depends
    /// only on the thread (see [`latest_user_turn`]): a retry or a second thread can't desync it
    /// the way a shared queue would. Each turn reports zero usage.
    pub fn by_prompt(pick: impl Fn(&[AgentMessage]) -> ScriptedTurn + Send + Sync + 'static) -> Self {
        Self {
            script: Mutex::new(Script::ByPrompt(Box::new(pick))),
            calls_seen: Mutex::new(Vec::new()),
            pace: None,
        }
    }

    /// The same fake, streaming at `pace`. Cancel still ends the stream at once, mid-wait.
    pub fn paced(mut self, pace: Pace) -> Self {
        self.pace = Some(pace);
        self
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
        _call: crate::agent::llm::CallSettings,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            self.calls_seen.lock_ignore_poison().push(messages.to_vec());

            let (turn, usage) = match &mut *self.script.lock_ignore_poison() {
                Script::Queue(turns) => turns
                    .pop_front()
                    .ok_or_else(|| AgentLlmError::Provider("fake: script exhausted".to_string()))?,
                Script::ByPrompt(pick) => (pick(messages), AgentUsage::default()),
            };

            let deltas = match turn {
                ScriptedTurn::Fail(error) => return Err(error),
                ScriptedTurn::Say(chunks) => say_deltas(chunks, usage),
                ScriptedTurn::CallTools(calls) => tool_call_deltas(Vec::new(), calls, usage),
                ScriptedTurn::SayThenCall { say, calls } => tool_call_deltas(say, calls, usage),
                ScriptedTurn::CallRawTool(name, args) => {
                    tool_call_deltas(Vec::new(), vec![(ToolId::from_wire_name(&name), args)], usage)
                }
            };

            let cancel_signal = cancel.clone();
            let stream = match self.pace {
                None => stream::iter(deltas).boxed(),
                Some(pace) => paced(deltas, pace),
            };
            Ok(stream
                .take_until(async move { cancel_signal.cancelled().await })
                .boxed())
        }
        .boxed()
    }
}

/// `deltas` with `pace`'s waits before each text delta, the longer one before the last.
fn paced(deltas: Vec<Result<AgentDelta, AgentLlmError>>, pace: Pace) -> AgentDeltaStream {
    let last_text = deltas
        .iter()
        .rposition(|delta| matches!(delta, Ok(AgentDelta::Text(_))));
    stream::iter(deltas.into_iter().enumerate())
        .then(move |(index, delta)| async move {
            if matches!(delta, Ok(AgentDelta::Text(_))) {
                let hold = if Some(index) == last_text {
                    pace.hold_before_last
                } else {
                    std::time::Duration::ZERO
                };
                tokio::time::sleep(pace.per_delta + hold).await;
            }
            delta
        })
        .boxed()
}

/// Streams the chunks as text deltas, then an `End` carrying the joined answer.
fn say_deltas(chunks: Vec<String>, usage: AgentUsage) -> Vec<Result<AgentDelta, AgentLlmError>> {
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
        usage,
        message,
    }));
    deltas
}

/// Emits a `ToolCallStarted` per call, each after the `say` chunk of the same index (so
/// narration and calls interleave on the wire), then an `End` (stop `ToolCall`) whose message
/// carries the joined text first, then the tool-call parts for the runtime to dispatch (and
/// refuse, for an `Unrecognized` name).
fn tool_call_deltas(
    say: Vec<String>,
    calls: Vec<(ToolId, serde_json::Value)>,
    usage: AgentUsage,
) -> Vec<Result<AgentDelta, AgentLlmError>> {
    let mut deltas: Vec<Result<AgentDelta, AgentLlmError>> = Vec::new();
    let joined = say.concat();
    let mut parts = Vec::new();
    if !joined.is_empty() {
        parts.push(AgentPart::Text(joined));
    }
    let mut chunks = say.into_iter();
    for (index, (tool, arguments)) in calls.into_iter().enumerate() {
        if let Some(chunk) = chunks.next() {
            deltas.push(Ok(AgentDelta::Text(chunk)));
        }
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
    deltas.extend(chunks.map(|chunk| Ok(AgentDelta::Text(chunk))));
    let message = AgentMessage {
        role: AgentRole::Assistant,
        parts,
        at: 0,
    };
    deltas.push(Ok(AgentDelta::End {
        stop: AgentStopReason::ToolCall,
        usage,
        message,
    }));
    deltas
}

/// The thread's latest user turn as a fake sees it: the person's own text (the LAST text part of
/// the last user message, since context assembly prepends the envelope to it) and how many
/// assistant messages follow it (the rounds this turn already played). `None` before any user
/// message.
pub fn latest_user_turn(messages: &[AgentMessage]) -> Option<(&str, usize)> {
    let at = messages.iter().rposition(|message| message.role == AgentRole::User)?;
    let text = messages[at].parts.iter().rev().find_map(|part| match part {
        AgentPart::Text(text) => Some(text.as_str()),
        _ => None,
    })?;
    let rounds = messages[at + 1..]
        .iter()
        .filter(|message| message.role == AgentRole::Assistant)
        .count();
    Some((text, rounds))
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

    /// A paced fake holds before its last text delta and still ends at once on a cancel, so a
    /// held streaming state can be stopped like a real one.
    #[tokio::test(start_paused = true)]
    async fn a_paced_fake_holds_before_its_last_delta_and_stops_on_cancel() {
        let pace = Pace {
            per_delta: std::time::Duration::from_millis(10),
            hold_before_last: std::time::Duration::from_secs(60),
        };
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::Say(vec!["One. ".into(), "Two.".into()])]).paced(pace);
        let cancel = CancellationToken::new();
        let mut stream = fake
            .respond(
                "sys",
                &[],
                &[],
                crate::agent::llm::CallSettings::default(),
                cancel.clone(),
            )
            .await
            .expect("starts");
        let started = tokio::time::Instant::now();
        let first = stream.next().await.expect("a delta").expect("no error");
        assert_eq!(first, AgentDelta::Text("One. ".into()));
        assert!(started.elapsed() >= pace.per_delta);
        let next = tokio::time::timeout(std::time::Duration::from_secs(30), stream.next()).await;
        assert!(next.is_err(), "the last delta is held");
        cancel.cancel();
        assert!(stream.next().await.is_none(), "a cancel ends the held stream");
    }

    #[tokio::test]
    async fn sequences_turns_and_records_every_call() {
        let fake = FakeAgentLlm::script(vec![
            ScriptedTurn::CallTools(vec![(ToolId::AppState, json!({ "path": "/" }))]),
            ScriptedTurn::Say(vec!["Hello ".into(), "world".into()]),
        ]);
        let cancel = CancellationToken::new();

        let first_prompt = vec![user_message("what is big?")];
        let first = fake
            .respond(
                "sys",
                &[],
                &first_prompt,
                crate::agent::llm::CallSettings::default(),
                cancel.clone(),
            )
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
            .respond(
                "sys",
                &[],
                &second_prompt,
                crate::agent::llm::CallSettings::default(),
                cancel.clone(),
            )
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
        // The seam the negative test builds on: a raw name the agent
        // doesn't know resolves to `Unrecognized`, which dispatch will refuse.
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::CallRawTool("format_drive".into(), json!({}))]);
        let stream = fake
            .respond(
                "sys",
                &[],
                &[],
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await
            .expect("turn starts");
        let deltas = collect(stream).await;

        let AgentDelta::ToolCallStarted { tool, .. } = &deltas[0] else {
            panic!("expected a ToolCallStarted");
        };
        assert_eq!(*tool, ToolId::Unrecognized("format_drive".into()));
        assert!(!tool.is_known());

        let AgentDelta::End { message, .. } = deltas.last().unwrap() else {
            panic!("expected an End delta");
        };
        let AgentPart::ToolCall(call) = &message.parts[0] else {
            panic!("expected a tool call part");
        };
        assert_eq!(call.tool, ToolId::Unrecognized("format_drive".into()));
    }

    /// genai's `StreamEnd` order: whatever the wire interleaved, the stored message holds ONE text,
    /// then every call.
    #[tokio::test]
    async fn say_then_call_interleaves_on_the_wire_but_ends_text_first() {
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::SayThenCall {
            say: vec!["Let me ".into(), "look.".into()],
            calls: vec![
                (ToolId::AppState, json!({})),
                (ToolId::Unrecognized("format_drive".into()), json!({})),
            ],
        }]);
        let stream = fake
            .respond(
                "sys",
                &[],
                &[],
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await
            .expect("turn starts");
        let deltas = collect(stream).await;

        let wire: Vec<String> = deltas
            .iter()
            .map(|delta| match delta {
                AgentDelta::Text(text) => format!("text:{text}"),
                AgentDelta::ToolCallStarted { call_id, .. } => format!("start:{call_id}"),
                AgentDelta::End { .. } => "end".to_string(),
                other => panic!("unexpected delta {other:?}"),
            })
            .collect();
        assert_eq!(
            wire,
            [
                "text:Let me ",
                "start:fake-call-0",
                "text:look.",
                "start:fake-call-1",
                "end"
            ]
        );
        let AgentDelta::End { stop, message, .. } = deltas.last().unwrap() else {
            panic!("expected an End delta");
        };
        assert_eq!(*stop, AgentStopReason::ToolCall);
        assert_eq!(message.parts[0], AgentPart::Text("Let me look.".into()));
        let tools: Vec<&ToolId> = message.parts[1..]
            .iter()
            .map(|part| match part {
                AgentPart::ToolCall(call) => &call.tool,
                other => panic!("expected a call, got {other:?}"),
            })
            .collect();
        assert_eq!(tools, [&ToolId::AppState, &ToolId::Unrecognized("format_drive".into())]);
    }

    /// A fake that picks each turn from the prompt it's handed, so a retry or a second thread can't
    /// desync it.
    #[tokio::test]
    async fn by_prompt_picks_from_the_latest_user_text_and_the_rounds_since() {
        let fake = FakeAgentLlm::by_prompt(|messages| {
            let (text, rounds) = latest_user_turn(messages).expect("a user turn");
            ScriptedTurn::Say(vec![format!("{text}/{rounds}")])
        });
        let assistant = AgentMessage {
            role: AgentRole::Assistant,
            parts: vec![AgentPart::Text("hm".into())],
            at: 2,
        };
        let envelope_then_text = AgentMessage {
            role: AgentRole::User,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text("second".into())],
            at: 3,
        };
        let prompt = vec![
            user_message("first"),
            assistant.clone(),
            envelope_then_text,
            assistant.clone(),
            assistant,
        ];
        let stream = fake
            .respond(
                "sys",
                &[],
                &prompt,
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await
            .expect("turn starts");
        let deltas = collect(stream).await;
        assert_eq!(deltas[0], AgentDelta::Text("second/2".into()));
        assert_eq!(latest_user_turn(&[]), None);
    }

    #[tokio::test]
    async fn fail_turn_returns_typed_error_before_streaming() {
        let fake = FakeAgentLlm::script(vec![ScriptedTurn::Fail(AgentLlmError::NoKey)]);
        // Match on the result rather than `expect_err` — the Ok type is a boxed
        // stream, which isn't `Debug`.
        let result = fake
            .respond(
                "sys",
                &[],
                &[],
                crate::agent::llm::CallSettings::default(),
                CancellationToken::new(),
            )
            .await;
        assert!(
            matches!(result, Err(AgentLlmError::NoKey)),
            "a Fail turn errors with its typed error"
        );
    }
}
