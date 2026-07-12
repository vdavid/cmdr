//! The genai-backed [`AgentLlm`] over `crate::ai::AiBackend`.
//!
//! This is the ONE place `genai` types touch the agent. It maps the typed
//! [`AgentPart`] model to and from genai's `ContentPart`, runs a single streaming
//! call, and maps the stream events to [`AgentDelta`]s. The mapping table and the
//! reasoning-blob shapes it owns are documented in `DETAILS.md`.
//!
//! Reasoning posture (spike verdict): reasoning is kept OFF on the Anthropic and
//! OpenAI-Responses paths in v1, because genai drops their reasoning state on
//! replay (Gaps A/B) so a reasoning-on multi-step loop breaks. Gemini round-trips
//! its per-`functionCall` `thoughtSignature` correctly, so its reasoning rides on
//! the tool call and is preserved end to end.

use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatRole, ChatStreamEvent, ContentPart, MessageContent, ReasoningEffort,
    StopReason as GenaiStopReason, StreamEnd, Tool as GenaiTool, ToolCall as GenaiToolCall,
    ToolResponse as GenaiToolResponse, Usage as GenaiUsage,
};
use tokio_util::sync::CancellationToken;

use crate::ai::client::{AiBackend, AiError, map_genai_error};

use super::types::{
    AgentDelta, AgentLlmError, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolCall, AgentToolResult,
    AgentUsage, ProviderTag, ReasoningState, ToolDeclaration, ToolId,
};
use super::{AgentDeltaStream, AgentLlm};

const LOG_TARGET: &str = "agent::llm";

/// The genai-backed agent LLM. Wraps a configured [`AiBackend`] (the interactive
/// model slot; slot resolution arrives in M8) and reuses its adapter routing and
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
        cancel: CancellationToken,
    ) -> BoxFuture<'a, Result<AgentDeltaStream, AgentLlmError>> {
        async move {
            let adapter = self.backend.resolve_adapter().await.map_err(AgentLlmError::from)?;
            let provider = provider_tag_for(adapter);

            let request = build_request(system, tools, messages);
            let options = build_options(adapter);

            log::debug!(
                target: LOG_TARGET,
                "respond: opening stream (adapter={adapter:?}, provider={provider:?}, tools={}, messages={})",
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
            // relies on.
            let cancel_signal = cancel.clone();
            let stream = response
                .stream
                .take_until(async move { cancel_signal.cancelled().await })
                .filter_map(move |item| async move { map_stream_event(item, provider) })
                .boxed();

            Ok(stream)
        }
        .boxed()
    }
}

// region: --- Request build (agent -> genai)

/// Builds the genai request from the assembled prompt. System and tools go into
/// their dedicated request slots; the message history maps part-for-part.
fn build_request(system: &str, tools: &[ToolDeclaration], messages: &[AgentMessage]) -> ChatRequest {
    let chat_messages: Vec<ChatMessage> = messages.iter().map(agent_message_to_genai).collect();
    let mut request = ChatRequest::new(chat_messages).with_system(system);
    if !tools.is_empty() {
        let genai_tools: Vec<GenaiTool> = tools.iter().map(tool_declaration_to_genai).collect();
        request = request.with_tools(genai_tools);
    }
    request
}

/// Capture everything needed to rebuild the final [`AgentMessage`] on stream end,
/// plus the per-provider reasoning posture.
fn build_options(adapter: AdapterKind) -> ChatOptions {
    let options = ChatOptions::default()
        .with_capture_content(true)
        .with_capture_tool_calls(true)
        .with_capture_usage(true)
        .with_capture_reasoning_content(true);

    match adapter {
        // Round-trip broken on these in v1 (spike Gaps A/B): keep reasoning off so
        // a multi-step tool loop does not 400 / silently degrade. `crate::ai`'s
        // `adjust_for_model` won't override an explicit effort.
        AdapterKind::Anthropic | AdapterKind::OpenAIResp => options.with_reasoning_effort(ReasoningEffort::None),
        _ => options,
    }
}

/// Never sets `strict: true` — spike Gap D (OpenAI strict also demands all-required,
/// which genai does not enforce, so an optional prop 400s).
fn tool_declaration_to_genai(decl: &ToolDeclaration) -> GenaiTool {
    let mut tool = GenaiTool::new(decl.name.as_wire_name());
    if !decl.description.is_empty() {
        tool = tool.with_description(decl.description.clone());
    }
    tool.with_schema(decl.schema.clone())
}

fn agent_message_to_genai(message: &AgentMessage) -> ChatMessage {
    let role = match message.role {
        AgentRole::System => ChatRole::System,
        AgentRole::User => ChatRole::User,
        AgentRole::Assistant => ChatRole::Assistant,
        AgentRole::Tool => ChatRole::Tool,
    };
    let parts: Vec<ContentPart> = message.parts.iter().flat_map(agent_part_to_genai).collect();
    ChatMessage::new(role, MessageContent::from_parts(parts))
}

/// Maps one agent part to zero-or-more genai parts. A reasoning part that genai
/// can't represent (e.g. an Anthropic thinking blob — Gap A) maps to nothing; v1
/// runs those paths reasoning-off, so it does not arise in practice.
fn agent_part_to_genai(part: &AgentPart) -> Vec<ContentPart> {
    match part {
        AgentPart::Text(text) => vec![ContentPart::Text(text.clone())],
        AgentPart::ToolCall(call) => vec![ContentPart::ToolCall(GenaiToolCall {
            call_id: call.call_id.clone(),
            fn_name: call.tool.as_wire_name().to_string(),
            fn_arguments: call.arguments.clone(),
            thought_signatures: call.reasoning.as_ref().and_then(|r| blob_thought_signatures(&r.blob)),
        })],
        AgentPart::ToolResult(result) => vec![ContentPart::ToolResponse(GenaiToolResponse {
            call_id: result.call_id.clone(),
            content: value_to_tool_content(&result.content),
        })],
        AgentPart::Reasoning(state) => {
            if let Some(signatures) = blob_thought_signatures(&state.blob) {
                signatures.into_iter().map(ContentPart::ThoughtSignature).collect()
            } else if let Some(reasoning) = blob_reasoning_content(&state.blob) {
                vec![ContentPart::ReasoningContent(reasoning)]
            } else {
                Vec::new()
            }
        }
    }
}

// endregion: --- Request build

// region: --- Response parse (genai -> agent)

/// Rebuilds the final assistant message from the captured stream content.
fn build_agent_message_from_stream_end(end: &StreamEnd, provider: ProviderTag, at: i64) -> AgentMessage {
    let parts = end
        .captured_content
        .as_ref()
        .map(|content| genai_content_to_agent_parts(content, provider))
        .unwrap_or_default();
    AgentMessage {
        role: AgentRole::Assistant,
        parts,
        at,
    }
}

/// Maps genai content back to typed agent parts, tagging any reasoning with
/// `provider`. genai attaches captured thought signatures BOTH to the first tool
/// call AND as leading standalone parts, so a standalone `ThoughtSignature` is
/// skipped when the message has tool calls (it would duplicate the tool call's
/// reasoning).
fn genai_content_to_agent_parts(content: &MessageContent, provider: ProviderTag) -> Vec<AgentPart> {
    let has_tool_calls = content.parts().iter().any(|part| part.is_tool_call());
    let mut out = Vec::new();
    for part in content.parts() {
        match part {
            ContentPart::Text(text) => out.push(AgentPart::Text(text.clone())),
            ContentPart::ToolCall(call) => out.push(AgentPart::ToolCall(genai_tool_call_to_agent(call, provider))),
            ContentPart::ToolResponse(response) => out.push(AgentPart::ToolResult(AgentToolResult {
                call_id: response.call_id.clone(),
                content: tool_content_to_value(&response.content),
                elided: false,
            })),
            ContentPart::ThoughtSignature(signature) => {
                if !has_tool_calls {
                    out.push(AgentPart::Reasoning(ReasoningState {
                        provider,
                        blob: thought_signatures_blob(vec![signature.clone()]),
                    }));
                }
            }
            ContentPart::ReasoningContent(reasoning) => out.push(AgentPart::Reasoning(ReasoningState {
                provider,
                blob: reasoning_content_blob(reasoning.clone()),
            })),
            ContentPart::Binary(_) | ContentPart::Custom(_) => {
                // The agent's read-only toolset never produces these; drop them.
            }
        }
    }
    out
}

fn genai_tool_call_to_agent(call: &GenaiToolCall, provider: ProviderTag) -> AgentToolCall {
    // Capture any per-call reasoning (e.g. Gemini's `thoughtSignature`) into the
    // opaque blob so it survives persistence and rides back on replay — the whole
    // point of the typed-parts model (spike Gaps A/B).
    let reasoning = call
        .thought_signatures
        .as_ref()
        .filter(|signatures| !signatures.is_empty())
        .map(|signatures| ReasoningState {
            provider,
            blob: thought_signatures_blob(signatures.clone()),
        });
    AgentToolCall {
        call_id: call.call_id.clone(),
        tool: ToolId::from_wire_name(&call.fn_name),
        arguments: call.fn_arguments.clone(),
        reasoning,
    }
}

fn map_stream_event(
    item: genai::Result<ChatStreamEvent>,
    provider: ProviderTag,
) -> Option<Result<AgentDelta, AgentLlmError>> {
    match item {
        Ok(ChatStreamEvent::Start) => None,
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
            let message = build_agent_message_from_stream_end(&end, provider, now_secs());
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

fn map_usage(usage: GenaiUsage) -> AgentUsage {
    AgentUsage {
        prompt_tokens: non_negative(usage.prompt_tokens),
        completion_tokens: non_negative(usage.completion_tokens),
    }
}

// endregion: --- Response parse

// region: --- Error + provider mapping

/// Maps `crate::ai`'s transport error to the agent seam's typed error. The status
/// classification (401/403 → auth, 429 → rate-limited) happened upstream by HTTP
/// status, so this stays a variant-to-variant mapping (`no-string-matching`).
impl From<AiError> for AgentLlmError {
    fn from(error: AiError) -> Self {
        match error {
            AiError::Unavailable => AgentLlmError::Unavailable,
            AiError::Timeout => AgentLlmError::Timeout,
            AiError::AuthFailed(_) => AgentLlmError::AuthFailed,
            AiError::RateLimited(_) => AgentLlmError::RateLimited,
            AiError::EmptyResponse => AgentLlmError::Provider("the model returned no text".to_string()),
            AiError::ServerError(detail) => AgentLlmError::Provider(detail),
            AiError::ParseError(detail) => AgentLlmError::Provider(detail),
        }
    }
}

/// Maps the resolved adapter to a descriptive provider tag for reasoning state.
/// The local llama-server forces the OpenAI chat-completions adapter, so it tags as
/// `OpenAi` here; honest local labeling (free/on-device cost) is the slot's job (M8).
fn provider_tag_for(adapter: AdapterKind) -> ProviderTag {
    match adapter {
        AdapterKind::Anthropic => ProviderTag::Anthropic,
        AdapterKind::OpenAIResp => ProviderTag::OpenAiResponses,
        AdapterKind::Gemini => ProviderTag::Gemini,
        _ => ProviderTag::OpenAi,
    }
}

// endregion: --- Error + provider mapping

// region: --- Small helpers

/// The reasoning blob is opaque outside this module; these keys are its whole
/// vocabulary. Gemini's per-`functionCall` signatures and standalone thought
/// signatures use `thought_signatures`; reasoning text uses `reasoning_content`.
fn thought_signatures_blob(signatures: Vec<String>) -> serde_json::Value {
    serde_json::json!({ "thought_signatures": signatures })
}

fn reasoning_content_blob(reasoning: String) -> serde_json::Value {
    serde_json::json!({ "reasoning_content": reasoning })
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
mod tests {
    use super::*;
    use serde_json::json;

    fn gemini_tool_call_message() -> AgentMessage {
        AgentMessage {
            role: AgentRole::Assistant,
            parts: vec![AgentPart::ToolCall(AgentToolCall {
                call_id: "call-1".into(),
                tool: ToolId::Placeholder,
                arguments: json!({ "path": "/Users/x" }),
                reasoning: Some(ReasoningState {
                    provider: ProviderTag::Gemini,
                    blob: json!({ "thought_signatures": ["sig-abc"] }),
                }),
            })],
            at: 1_000,
        }
    }

    #[test]
    fn tool_call_with_reasoning_survives_genai_round_trip() {
        // The make-or-break invariant (spike Gaps A/B): a tool-call carrying opaque
        // reasoning state must survive agent -> genai -> agent with the blob intact.
        // The typed-parts model exists precisely to prevent the flatten-and-lose
        // failure this asserts against.
        let original = gemini_tool_call_message();

        let genai_message = agent_message_to_genai(&original);
        let parts = genai_content_to_agent_parts(&genai_message.content, ProviderTag::Gemini);
        let rebuilt = AgentMessage {
            role: AgentRole::Assistant,
            parts,
            at: original.at,
        };

        assert_eq!(rebuilt, original, "the reasoning blob must round-trip untouched");

        let AgentPart::ToolCall(call) = &rebuilt.parts[0] else {
            panic!("expected a tool call part");
        };
        assert_eq!(
            call.reasoning.as_ref().expect("reasoning must be preserved").blob,
            json!({ "thought_signatures": ["sig-abc"] })
        );
    }

    #[test]
    fn standalone_thought_signature_dedupes_against_tool_call() {
        // genai's `StreamEnd` attaches captured thought signatures BOTH to the first
        // tool call AND as leading standalone parts. Mapping both would duplicate the
        // reasoning, so a standalone `ThoughtSignature` is dropped when the message
        // has tool calls — the tool call is the canonical home.
        let content = MessageContent::from_parts(vec![
            ContentPart::ThoughtSignature("sig-abc".into()),
            ContentPart::ToolCall(GenaiToolCall {
                call_id: "call-1".into(),
                fn_name: "placeholder".into(),
                fn_arguments: json!({}),
                thought_signatures: Some(vec!["sig-abc".into()]),
            }),
        ]);
        let parts = genai_content_to_agent_parts(&content, ProviderTag::Gemini);
        // Exactly one part (the tool call), carrying the reasoning; no standalone dup.
        assert_eq!(parts.len(), 1);
        let AgentPart::ToolCall(call) = &parts[0] else {
            panic!("expected only the tool call");
        };
        assert_eq!(
            call.reasoning.as_ref().expect("reasoning on the call").blob,
            json!({ "thought_signatures": ["sig-abc"] })
        );
    }

    #[test]
    fn standalone_reasoning_survives_without_tool_calls() {
        // With no tool call to carry it, a standalone thought signature maps to a
        // `Reasoning` part rather than being dropped.
        let content = MessageContent::from_parts(vec![ContentPart::ThoughtSignature("sig-xyz".into())]);
        let parts = genai_content_to_agent_parts(&content, ProviderTag::Gemini);
        assert_eq!(
            parts,
            vec![AgentPart::Reasoning(ReasoningState {
                provider: ProviderTag::Gemini,
                blob: json!({ "thought_signatures": ["sig-xyz"] }),
            })]
        );
    }

    #[test]
    fn ai_error_maps_to_typed_agent_error() {
        // Status-classified upstream (by HTTP status), mapped variant-to-variant
        // here — never by matching the message string.
        assert_eq!(AgentLlmError::from(AiError::Unavailable), AgentLlmError::Unavailable);
        assert_eq!(AgentLlmError::from(AiError::Timeout), AgentLlmError::Timeout);
        assert_eq!(
            AgentLlmError::from(AiError::AuthFailed("bad key".into())),
            AgentLlmError::AuthFailed
        );
        assert_eq!(
            AgentLlmError::from(AiError::RateLimited("slow down".into())),
            AgentLlmError::RateLimited
        );
        assert!(matches!(
            AgentLlmError::from(AiError::EmptyResponse),
            AgentLlmError::Provider(_)
        ));
        assert!(matches!(
            AgentLlmError::from(AiError::ServerError("boom".into())),
            AgentLlmError::Provider(_)
        ));
        assert!(matches!(
            AgentLlmError::from(AiError::ParseError("garbled".into())),
            AgentLlmError::Provider(_)
        ));
    }

    #[test]
    fn stop_reasons_map_to_typed_variants() {
        assert_eq!(
            map_stop_reason(GenaiStopReason::Completed("stop".into())),
            AgentStopReason::Completed
        );
        assert_eq!(
            map_stop_reason(GenaiStopReason::ToolCall("tool_calls".into())),
            AgentStopReason::ToolCall
        );
        assert_eq!(
            map_stop_reason(GenaiStopReason::MaxTokens("length".into())),
            AgentStopReason::MaxTokens
        );
        assert_eq!(
            map_stop_reason(GenaiStopReason::Other("weird".into())),
            AgentStopReason::Other("weird".into())
        );
    }

    #[test]
    fn usage_maps_and_clamps_negatives() {
        let usage = GenaiUsage {
            prompt_tokens: Some(120),
            completion_tokens: Some(-3),
            ..Default::default()
        };
        assert_eq!(
            map_usage(usage),
            AgentUsage {
                prompt_tokens: 120,
                completion_tokens: 0,
            }
        );
    }

    #[test]
    fn declared_tools_are_never_strict() {
        let decl = ToolDeclaration {
            name: ToolId::Placeholder,
            description: "look".into(),
            schema: json!({ "type": "object" }),
        };
        let tool = tool_declaration_to_genai(&decl);
        assert!(tool.strict.is_none(), "strict must stay unset (Gap D)");
    }
}
