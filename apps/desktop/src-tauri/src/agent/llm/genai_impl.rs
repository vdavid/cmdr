//! The genai-backed [`AgentLlm`] over `crate::ai::AiBackend`.
//!
//! This is the ONE place `genai` types touch the agent. It maps the typed
//! [`AgentPart`] model to and from genai's `ContentPart`, runs a single streaming
//! call, and maps the stream events to [`AgentDelta`]s. The mapping table and the
//! reasoning-blob shapes it owns are documented in `DETAILS.md`.
//!
//! Reasoning round-trip: every reasoning part genai captures (Anthropic signed and
//! redacted thinking blocks, OpenAI Responses encrypted items, Gemini thought
//! signatures, chat-completions `reasoning_content`) is kept in wire order, stamped
//! with the model that produced it, and replayed verbatim to that same model only.
//! Per-provider requirements: `DETAILS.md` § Reasoning round-trip.

use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::StreamExt;
use genai::ModelIden;
use genai::adapter::AdapterKind;
use genai::chat::{
    CacheControl, ChatMessage, ChatOptions, ChatRequest, ChatRole, ChatStreamEvent, ContentPart, CustomPart,
    MessageContent, ReasoningEffort as GenaiReasoningEffort, StopReason as GenaiStopReason, StreamEnd,
    Tool as GenaiTool, ToolCall as GenaiToolCall, ToolResponse as GenaiToolResponse, Usage as GenaiUsage,
};
use tokio_util::sync::CancellationToken;

use crate::ai::client::{AiBackend, AiError, map_genai_error};

use super::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult,
    AgentUsage, CallSettings, ProviderTag, ReasoningEffort, ReasoningState, ToolDeclaration, ToolId,
};
use super::{AgentDeltaStream, AgentLlm};

const LOG_TARGET: &str = "agent::llm";

/// The genai-backed agent LLM. Wraps a configured [`AiBackend`] (the interactive
/// model slot; the slot is resolved in the command layer) and reuses its adapter routing and
/// stream-cancel model.
pub struct GenaiAgentLlm {
    backend: AiBackend,
}

impl GenaiAgentLlm {
    pub fn new(backend: AiBackend) -> Self {
        Self { backend }
    }
}

impl AgentLlm for GenaiAgentLlm {
    fn respond<'a>(
        &'a self,
        system: &'a str,
        tools: &'a [ToolDeclaration],
        messages: &'a [AgentMessage],
        call: CallSettings,
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            let model = self.backend.resolve_model().await.map_err(AgentLlmError::from)?;
            let adapter = model.adapter_kind;
            let origin = ReasoningOrigin::of(&model).with_chat_reasoning(self.backend.replays_chat_reasoning());

            let request = build_request(system, tools, messages, &origin);
            let options = build_options(adapter, call);

            log::debug!(
                target: LOG_TARGET,
                "respond: opening stream (adapter={adapter:?}, effort={:?}, max_output={}, tools={}, messages={})",
                call.effort,
                call.max_output_tokens,
                tools.len(),
                messages.len()
            );

            let response = self
                .backend
                .exec_chat_stream_request(request, &options)
                .await
                .map_err(AgentLlmError::from)?;

            // Cancellation: end the stream when the token fires, which drops the
            // underlying reqwest body (billing stops) — the same model `crate::ai`
            // relies on. `response` is the raw genai event stream (already tapped for
            // logging inside `AiBackend`).
            let cancel_signal = cancel.clone();
            let stream = map_stream(
                response.take_until(async move { cancel_signal.cancelled().await }),
                origin,
            );

            Ok(stream)
        }
        .boxed()
    }
}

/// Maps the raw genai event stream to agent deltas, announcing each tool call ONCE.
///
/// genai's OpenAI streamer emits a `ToolCallChunk` for every argument fragment, each
/// carrying the merged call's id, so a call whose arguments arrive in several fragments
/// would otherwise start several times. The rail keys its tool lines by call id, and a
/// repeated key took the whole window down (`ERR-FRMAR`, a local Ministral 3B on
/// llama-server).
fn map_stream<S>(events: S, origin: ReasoningOrigin) -> AgentDeltaStream
where
    S: futures_util::Stream<Item = genai::Result<ChatStreamEvent>> + Send + 'static,
{
    let mut started = std::collections::HashSet::<String>::new();
    events
        .filter_map(move |item| {
            let delta = map_stream_event(item, &origin).filter(|delta| match delta {
                Ok(AgentDelta::ToolCallStarted { call_id, .. }) => started.insert(call_id.clone()),
                _ => true,
            });
            futures_util::future::ready(delta)
        })
        .boxed()
}

// region: --- Request build (agent -> genai)

/// Builds the genai request from the assembled prompt. System and tools go into
/// their dedicated request slots; the message history maps part-for-part.
/// `origin` is the model this request goes to: reasoning that some other model produced stays
/// out of the replay ([`ReasoningOrigin::may_replay`]).
///
/// **Prompt-cache breakpoints** (`DETAILS.md` § Prompt caching): the last message is marked, so
/// a provider that caches only on request caches the whole prompt, and the next call (the same
/// bytes plus a little) reads it back at a tenth of the price. Anthropic also gets one on the
/// system string, so the tools and system prompt every thread shares stay cached across threads.
/// A provider that caches automatically (OpenAI before GPT-5.6, Gemini, DeepSeek) ignores both.
fn build_request(
    system: &str,
    tools: &[ToolDeclaration],
    messages: &[AgentMessage],
    origin: &ReasoningOrigin,
) -> ChatRequest {
    let replay = Replay {
        tool_names: tool_names_by_call_id(messages),
        origin,
    };
    let mut chat_messages: Vec<ChatMessage> = messages
        .iter()
        .map(|message| agent_message_to_genai(message, &replay))
        // A message whose every part was dropped would go out as an empty turn, which
        // adapters reject. It happens to the pair a nameless tool call leaves behind in a
        // thread persisted before the inbound guard existed.
        .filter(|message| !message.content.is_empty())
        .collect();
    if let Some(last) = chat_messages.pop() {
        chat_messages.push(last.with_options(CacheControl::Ephemeral));
    }
    let mut request = if origin.provider == ProviderTag::Anthropic {
        // genai can't mark the request's `system` slot, only a system-role message, which its
        // Anthropic adapter lifts into the same `system` field.
        let system = ChatMessage::system(system).with_options(CacheControl::Ephemeral);
        ChatRequest::new(std::iter::once(system).chain(chat_messages).collect())
    } else {
        ChatRequest::new(chat_messages).with_system(system)
    };
    if !tools.is_empty() {
        let genai_tools: Vec<GenaiTool> = tools.iter().map(tool_declaration_to_genai).collect();
        request = request.with_tools(genai_tools);
    }
    request
}

/// Capture everything needed to rebuild the final [`AgentMessage`] on stream end, plus the
/// call's reasoning effort and its output ceiling (`chat::effort::reply_room`, which thinking shares).
///
/// `capture_reasoning_content` is load-bearing, not just for display: it's genai's opt-in for
/// Anthropic's thinking-block signatures and the OpenAI Responses `reasoning.encrypted_content`
/// include, the halves a tool loop has to replay.
fn build_options(adapter: AdapterKind, call: CallSettings) -> ChatOptions {
    let options = ChatOptions::default()
        .with_max_tokens(call.max_output_tokens)
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_usage(true);

    // A model known not to reason gets asked for none: no effort, and no reasoning capture,
    // which on the Responses API would add a `reasoning.summary` it 400s on. The explicit
    // `false` is also what stops `crate::ai`'s `adjust_for_model` filling in an effort.
    if call.model_reasons == Some(false) {
        return options.with_capture_reasoning_content(false);
    }
    let options = options.with_capture_reasoning_content(true);
    match genai_effort(adapter, call.effort) {
        Some(effort) => options.with_reasoning_effort(effort),
        None => options,
    }
}

/// The agent's effort level as genai's, which each adapter then maps onto its provider's knob
/// (`DETAILS.md` § Reasoning effort). `None` sends nothing, so the provider decides.
fn genai_effort(adapter: AdapterKind, effort: ReasoningEffort) -> Option<GenaiReasoningEffort> {
    match effort {
        // `crate::ai`'s `adjust_for_model` fills an unset effort with `Low` on the Responses API,
        // which suits a one-line translation, not an agent loop. `Medium` is OpenAI's own default.
        ReasoningEffort::ProviderDefault if adapter == AdapterKind::OpenAIResp => Some(GenaiReasoningEffort::Medium),
        ReasoningEffort::ProviderDefault => None,
        ReasoningEffort::Off => Some(GenaiReasoningEffort::Zero),
        ReasoningEffort::Low => Some(GenaiReasoningEffort::Low),
        ReasoningEffort::Medium => Some(GenaiReasoningEffort::Medium),
        ReasoningEffort::High => Some(GenaiReasoningEffort::High),
        ReasoningEffort::Max => Some(GenaiReasoningEffort::Max),
    }
}

/// Never sets `strict: true`: OpenAI strict also demands all-required, which genai does not
/// enforce, so an optional prop 400s.
fn tool_declaration_to_genai(decl: &ToolDeclaration) -> GenaiTool {
    let mut tool = GenaiTool::new(decl.name.as_wire_name());
    if !decl.description.is_empty() {
        tool = tool.with_description(decl.description.clone());
    }
    tool.with_schema(decl.schema.clone())
}

fn agent_message_to_genai(message: &AgentMessage, replay: &Replay<'_>) -> ChatMessage {
    let role = match message.role {
        AgentRole::System => ChatRole::System,
        AgentRole::User => ChatRole::User,
        AgentRole::Assistant => ChatRole::Assistant,
        AgentRole::Tool => ChatRole::Tool,
    };
    let parts: Vec<ContentPart> = message
        .parts
        .iter()
        .flat_map(|part| agent_part_to_genai(part, replay))
        .collect();
    ChatMessage::new(role, MessageContent::from_parts(parts))
}

/// Wire tool name per `call_id`, resolved across the whole assembled transcript.
type ToolNames<'a> = std::collections::HashMap<&'a str, &'a str>;

/// What replaying one transcript needs beyond the part itself: the tool names results borrow,
/// and the model the request goes to, which decides whose reasoning may ride along.
struct Replay<'a> {
    tool_names: ToolNames<'a>,
    origin: &'a ReasoningOrigin,
}

impl Replay<'_> {
    /// The reasoning state as genai parts, or nothing when another model produced it.
    fn reasoning(&self, state: &ReasoningState) -> Vec<ContentPart> {
        if !self.origin.may_replay(state) {
            return Vec::new();
        }
        if let Some(signatures) = blob_thought_signatures(&state.blob) {
            signatures.into_iter().map(ContentPart::ThoughtSignature).collect()
        } else if let Some(reasoning) = blob_reasoning_content(&state.blob) {
            vec![ContentPart::ReasoningContent(reasoning)]
        } else if let Some(data) = blob_custom(&state.blob) {
            vec![ContentPart::Custom(CustomPart { model_iden: None, data })]
        } else {
            Vec::new()
        }
    }
}

/// Indexes every tool CALL in the transcript by `call_id`, so the tool RESULT that
/// answers it can be replayed carrying the originating function name. Gemini's
/// `functionResponse.name` keys on that name rather than the call ID, so an unnamed
/// result degrades tool calling there; the other adapters correlate by call ID and
/// ignore it. A result whose call is no longer in the window resolves to `None`
/// rather than to a guess.
fn tool_names_by_call_id(messages: &[AgentMessage]) -> ToolNames<'_> {
    messages
        .iter()
        .flat_map(|message| message.parts.iter())
        .filter_map(|part| match part {
            AgentPart::ToolCall(call) => Some((call.call_id.as_str(), call.tool.as_wire_name())),
            _ => None,
        })
        .collect()
}

/// Maps one agent part to zero-or-more genai parts. Reasoning (standalone, or on a tool call)
/// goes out only to the model that produced it.
fn agent_part_to_genai(part: &AgentPart, replay: &Replay<'_>) -> Vec<ContentPart> {
    let tool_names = &replay.tool_names;
    match part {
        // The envelope a user turn was sent with is prose to the provider, replayed verbatim.
        AgentPart::Text(text) | AgentPart::Envelope(text) => vec![ContentPart::Text(text.clone())],
        // The provider reads prose, so the structured digest renders on its way out. This is
        // the ONLY place it becomes English, which is what keeps the persisted row localizable.
        AgentPart::WakeDigest(digest) => vec![ContentPart::Text(digest.render())],
        // Same contract as the digest above: the only place a sweep's outcomes become English.
        AgentPart::ProposalOutcomes(outcomes) => vec![ContentPart::Text(outcomes.render())],
        AgentPart::MemoryNotes(notes) => vec![ContentPart::Text(notes.render())],
        // A nameless call never reaches the wire. Threads persisted before the inbound
        // guard still carry one, and every replay of it 400s, so this is what unwedges
        // them.
        AgentPart::ToolCall(call) if call.tool.is_nameless() => Vec::new(),
        AgentPart::ToolCall(call) => vec![ContentPart::ToolCall(GenaiToolCall {
            call_id: call.call_id.clone(),
            fn_name: call.tool.as_wire_name().to_string(),
            fn_arguments: object_arguments(&call.arguments),
            thought_signatures: call
                .reasoning
                .as_ref()
                .filter(|state| replay.origin.may_replay(state))
                .and_then(|state| blob_thought_signatures(&state.blob)),
        })],
        // The result answering a dropped call goes with it: an orphan tool response is
        // rejected in its own right.
        AgentPart::ToolResult(result) if answers_a_nameless_call(tool_names, &result.call_id) => Vec::new(),
        AgentPart::ToolResult(result) => vec![ContentPart::ToolResponse(GenaiToolResponse {
            call_id: result.call_id.clone(),
            fn_name: tool_names.get(result.call_id.as_str()).map(|name| (*name).to_string()),
            content: value_to_tool_content(&result.content),
        })],
        AgentPart::Reasoning(state) => replay.reasoning(state),
    }
}

/// True when the call this result answers is nameless, and so was itself dropped from
/// the replay. A result whose call has fallen out of the context window resolves to
/// `None` here and stays, carrying no function name — the pre-existing shape.
fn answers_a_nameless_call(tool_names: &ToolNames<'_>, call_id: &str) -> bool {
    tool_names.get(call_id).is_some_and(|name| name.trim().is_empty())
}

/// Tool arguments always go out as a JSON OBJECT. A provider that truncates a tool call
/// mid-stream leaves a bare string behind (`""` from Qwen via DashScope), and an
/// OpenAI-compatible endpoint rejects that outright ("the `function.arguments` parameter
/// must be in JSON format"), wedging the thread for good. An empty object reaches the
/// tool's own validation instead, which the model can recover from.
fn object_arguments(arguments: &serde_json::Value) -> serde_json::Value {
    if arguments.is_object() {
        arguments.clone()
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    }
}

// endregion: --- Request build

// region: --- Response parse (genai -> agent)

/// Rebuilds the final assistant message from the captured stream content.
///
/// A chat-completions stream (DeepSeek, Mistral, Groq, a local server) keeps its reasoning text
/// out of `captured_content`, in `captured_reasoning_content` only, so it joins the message
/// here, leading, the way it came, when the endpoint takes it back
/// ([`ReasoningOrigin::takes_reasoning`]). DeepSeek 400s a tool loop that doesn't send it. The
/// native adapters already put what they replay into the content; their
/// `captured_reasoning_content` is a display copy (Anthropic) or a summary nothing replays
/// (Gemini, OpenAI Responses).
fn build_agent_message_from_stream_end(end: &StreamEnd, origin: &ReasoningOrigin, at: i64) -> AgentMessage {
    let mut parts = end
        .captured_content
        .as_ref()
        .map(|content| genai_content_to_agent_parts(content, origin))
        .unwrap_or_default();
    let has_reasoning_text = end.captured_content.as_ref().is_some_and(|content| {
        content
            .parts()
            .iter()
            .any(|part| matches!(part, ContentPart::ReasoningContent(_)))
    });
    if origin.provider == ProviderTag::OpenAi
        && origin.takes_reasoning()
        && !has_reasoning_text
        && let Some(reasoning) = end.captured_reasoning_content.as_ref().filter(|r| !r.is_empty())
    {
        parts.insert(0, origin.reasoning_part(reasoning_content_blob(reasoning.clone())));
    }
    AgentMessage {
        role: AgentRole::Assistant,
        parts,
        at,
    }
}

/// Maps genai content back to typed agent parts, stamping any reasoning with `origin`.
///
/// Every reasoning part is kept, in wire order, and so is each tool call's own
/// `thought_signatures`, even where they repeat: genai mirrors the leading signatures onto
/// the first tool call, and each adapter picks its canonical copy on replay (Anthropic and
/// OpenAI Responses read the leading parts, Gemini the call's own and skips the repeat). A
/// dedupe here would have to know which one each adapter reads; replaying exactly what came
/// back doesn't.
fn genai_content_to_agent_parts(content: &MessageContent, origin: &ReasoningOrigin) -> Vec<AgentPart> {
    let mut out = Vec::new();
    for part in content.parts() {
        match part {
            ContentPart::Text(text) => out.push(AgentPart::Text(text.clone())),
            // A provider can finish with `tool_call` and still hand back a call carrying an
            // id and nothing else (Qwen via DashScope). Nothing can dispatch it, and once
            // it is in the transcript every replay 400s, so it never becomes a part.
            ContentPart::ToolCall(call) if call.fn_name.trim().is_empty() => {
                log::warn!(
                    target: LOG_TARGET,
                    "dropping a nameless tool call (call_id={}): the provider named no function",
                    call.call_id
                );
            }
            ContentPart::ToolCall(call) => out.push(AgentPart::ToolCall(genai_tool_call_to_agent(call, origin))),
            ContentPart::ToolResponse(response) => out.push(AgentPart::ToolResult(AgentToolResult {
                call_id: response.call_id.clone(),
                content: tool_content_to_value(&response.content),
                elided: false,
            })),
            ContentPart::ThoughtSignature(signature) => {
                out.push(origin.reasoning_part(thought_signatures_blob(vec![signature.clone()])));
            }
            ContentPart::ReasoningContent(reasoning) => {
                out.push(origin.reasoning_part(reasoning_content_blob(reasoning.clone())));
            }
            // The custom parts genai captures into an answer are provider continuation state:
            // Anthropic `redacted_thinking`, OpenRouter `reasoning_details`. They go back as is.
            ContentPart::Custom(custom) => out.push(origin.reasoning_part(custom_blob(custom.data.clone()))),
            ContentPart::Binary(_) => {
                // The agent's read-only toolset never produces these; drop them.
            }
        }
    }
    out
}

fn genai_tool_call_to_agent(call: &GenaiToolCall, origin: &ReasoningOrigin) -> AgentToolCall {
    // Capture any per-call reasoning (e.g. Gemini's `thoughtSignature`) into the
    // opaque blob so it survives persistence and rides back on replay, the whole
    // point of the typed-parts model.
    let reasoning = call
        .thought_signatures
        .as_ref()
        .filter(|signatures| !signatures.is_empty())
        .map(|signatures| origin.reasoning_state(thought_signatures_blob(signatures.clone())));
    AgentToolCall {
        call_id: call.call_id.clone(),
        tool: ToolId::from_wire_name(&call.fn_name),
        arguments: call.fn_arguments.clone(),
        reasoning,
    }
}

fn map_stream_event(
    item: genai::Result<ChatStreamEvent>,
    origin: &ReasoningOrigin,
) -> Option<Result<AgentDelta, AgentLlmError>> {
    match item {
        Ok(ChatStreamEvent::Start | ChatStreamEvent::Heartbeat) => None,
        Ok(ChatStreamEvent::Chunk(chunk)) => (!chunk.content.is_empty()).then_some(Ok(AgentDelta::Text(chunk.content))),
        Ok(ChatStreamEvent::ReasoningChunk(_) | ChatStreamEvent::ThoughtSignatureChunk(_)) => {
            Some(Ok(AgentDelta::ReasoningTick))
        }
        Ok(ChatStreamEvent::ToolCallChunk(chunk)) => Some(Ok(AgentDelta::ToolCallStarted {
            call_id: chunk.tool_call.call_id,
            tool: ToolId::from_wire_name(&chunk.tool_call.fn_name),
        })),
        Ok(ChatStreamEvent::End(end)) => {
            let stop = end
                .captured_stop_reason
                .clone()
                .map(map_stop_reason)
                .unwrap_or(AgentStopReason::Completed);
            let usage = end.captured_usage.clone().map(map_usage).unwrap_or_default();
            let message = build_agent_message_from_stream_end(&end, origin, now_secs());
            Some(Ok(AgentDelta::End { stop, usage, message }))
        }
        Err(error) => Some(Err(AgentLlmError::from(map_genai_error(error)))),
    }
}

fn map_stop_reason(reason: GenaiStopReason) -> AgentStopReason {
    match reason {
        GenaiStopReason::Completed(_) => AgentStopReason::Completed,
        GenaiStopReason::ToolCall(_) => AgentStopReason::ToolCall,
        GenaiStopReason::MaxTokens(_) => AgentStopReason::MaxTokens,
        GenaiStopReason::ContentFilter(_) => AgentStopReason::ContentFilter,
        GenaiStopReason::StopSequence(_) => AgentStopReason::StopSequence,
        GenaiStopReason::Other(other) => AgentStopReason::Other(other),
    }
}

/// Reasoning is billed as output, and genai's `completion_tokens` already counts it on every
/// adapter (Anthropic `output_tokens`, OpenAI `completion_tokens` / `output_tokens`, Gemini
/// candidates plus thoughts). The one gap: genai 0.7.1 leaves `completion_tokens` unset when
/// Gemini reports thoughts and no candidates, so the thoughts stand in rather than metering $0.
fn map_usage(usage: GenaiUsage) -> AgentUsage {
    let reasoning_tokens = usage
        .completion_tokens_details
        .as_ref()
        .and_then(|details| details.reasoning_tokens);
    AgentUsage {
        prompt_tokens: non_negative(usage.prompt_tokens),
        completion_tokens: non_negative(usage.completion_tokens.or(reasoning_tokens)),
    }
}

// endregion: --- Response parse

// region: --- Error + provider mapping

/// Maps `crate::ai`'s transport error to the agent seam's typed error. The status
/// classification (401/403 → auth, 429 → rate-limited) happened upstream by HTTP
/// status, so this stays a variant-to-variant mapping.
impl From<AiError> for AgentLlmError {
    fn from(error: AiError) -> Self {
        match error {
            AiError::Unavailable => AgentLlmError::Unavailable,
            AiError::Timeout => AgentLlmError::Timeout,
            AiError::AuthFailed(detail) => AgentLlmError::AuthFailed(detail),
            AiError::RateLimited(detail) => AgentLlmError::RateLimited(detail),
            AiError::NotFound(detail) => AgentLlmError::Provider(detail),
            AiError::EmptyResponse => AgentLlmError::Provider("the model returned no text".to_string()),
            AiError::ServerError(detail) => AgentLlmError::Provider(detail),
            AiError::ParseError(detail) => AgentLlmError::Provider(detail),
            // Only reachable mid-turn (a policy that arrived after the slot resolved): the turn
            // ends as the organization's "no", typed.
            AiError::Managed(refusal) => AgentLlmError::Managed(refusal),
        }
    }
}

/// Maps the resolved adapter to a descriptive provider tag for reasoning state. Every
/// chat-completions dialect (DeepSeek, Mistral, Groq, …) and the local llama-server tag as
/// `OpenAi`; the precise model rides in the blob's origin stamp ([`ReasoningOrigin`]), and
/// honest local labeling (free/on-device cost) is the slot's job.
fn provider_tag_for(adapter: AdapterKind) -> ProviderTag {
    match adapter {
        AdapterKind::Anthropic => ProviderTag::Anthropic,
        AdapterKind::OpenAIResp => ProviderTag::OpenAiResponses,
        AdapterKind::Gemini => ProviderTag::Gemini,
        _ => ProviderTag::OpenAi,
    }
}

/// The model a reasoning blob came from, and the model a request goes to. Reasoning replays
/// only to the model that produced it: a signature is a provider's own continuation token, so
/// an Anthropic signature sent to OpenAI as `encrypted_content` (or one Claude's to another)
/// is a 400, and DeepSeek's `reasoning_content` would 422 on Mistral. A thread that switches
/// model mid-way drops the old model's reasoning, which every provider accepts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ReasoningOrigin {
    provider: ProviderTag,
    /// The genai adapter, lowercase (`anthropic`, `deepseek`, `openai_resp`, …).
    adapter: &'static str,
    /// The bare model name, without a genai namespace.
    model: String,
    /// Whether a chat-completions endpoint takes reasoning text back (`AiBackend::
    /// replays_chat_reasoning`): Groq 400s on it, DeepSeek 400s without it. Not part of the
    /// stamp, since it's the endpoint's rule, not the reasoning's.
    chat_reasoning: bool,
}

impl ReasoningOrigin {
    fn of(model: &ModelIden) -> Self {
        Self {
            provider: provider_tag_for(model.adapter_kind),
            adapter: model.adapter_kind.as_lower_str(),
            model: model.model_name.namespace_and_name().1.to_string(),
            chat_reasoning: false,
        }
    }

    fn with_chat_reasoning(mut self, replays: bool) -> Self {
        self.chat_reasoning = replays;
        self
    }

    /// Whether this endpoint gets reasoning back at all. The native protocols always do; a
    /// chat-completions one only when it's known to take it.
    fn takes_reasoning(&self) -> bool {
        self.provider != ProviderTag::OpenAi || self.chat_reasoning
    }

    fn stamp(&self) -> serde_json::Value {
        serde_json::json!({ "adapter": self.adapter, "model": self.model })
    }

    /// `blob` stamped with this origin.
    fn reasoning_state(&self, mut blob: serde_json::Value) -> ReasoningState {
        if let Some(map) = blob.as_object_mut() {
            map.insert(ORIGIN_KEY.to_string(), self.stamp());
        }
        ReasoningState {
            provider: self.provider,
            blob,
        }
    }

    fn reasoning_part(&self, blob: serde_json::Value) -> AgentPart {
        AgentPart::Reasoning(self.reasoning_state(blob))
    }

    /// Whether `state` may ride a request to this origin's model. A blob stored before the
    /// stamp existed carries only its provider family, so it replays within that family, as
    /// it always did.
    fn may_replay(&self, state: &ReasoningState) -> bool {
        if !self.takes_reasoning() {
            return false;
        }
        match state.blob.get(ORIGIN_KEY) {
            Some(stamp) => *stamp == self.stamp(),
            None => state.provider == self.provider,
        }
    }
}

// endregion: --- Error + provider mapping

// region: --- Small helpers

/// The reasoning blob is opaque outside this module; these keys are its whole vocabulary.
/// Each blob holds ONE reasoning part, in the position it came: `thought_signatures` (a
/// signature: Anthropic's thinking-block half, OpenAI Responses `encrypted_content`, Gemini's
/// `thoughtSignature`), `reasoning_content` (reasoning text: Anthropic's thinking-block half,
/// chat-completions `reasoning_content`), or `custom` (a provider block genai keeps whole:
/// Anthropic `redacted_thinking`, OpenRouter `reasoning_details`), plus the `origin` stamp.
const ORIGIN_KEY: &str = "origin";

fn thought_signatures_blob(signatures: Vec<String>) -> serde_json::Value {
    serde_json::json!({ "thought_signatures": signatures })
}

fn reasoning_content_blob(reasoning: String) -> serde_json::Value {
    serde_json::json!({ "reasoning_content": reasoning })
}

fn custom_blob(data: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "custom": data })
}

fn blob_custom(blob: &serde_json::Value) -> Option<serde_json::Value> {
    blob.get("custom").cloned()
}

fn blob_thought_signatures(blob: &serde_json::Value) -> Option<Vec<String>> {
    let array = blob.get("thought_signatures")?.as_array()?;
    if array.is_empty() {
        return None;
    }
    Some(
        array
            .iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
    )
}

fn blob_reasoning_content(blob: &serde_json::Value) -> Option<String> {
    blob.get("reasoning_content")?.as_str().map(str::to_string)
}

/// genai's `ToolResponse.content` is a string. A structured result serializes to
/// JSON text; a bare string passes through unquoted so it round-trips cleanly.
fn value_to_tool_content(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn tool_content_to_value(content: &str) -> serde_json::Value {
    serde_json::from_str(content).unwrap_or_else(|_| serde_json::Value::String(content.to_string()))
}

fn non_negative(count: Option<i32>) -> u32 {
    count.unwrap_or(0).max(0) as u32
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// endregion: --- Small helpers

#[cfg(test)]
mod tests;
