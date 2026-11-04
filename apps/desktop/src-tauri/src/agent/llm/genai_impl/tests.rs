use super::*;
use serde_json::json;

mod overloaded;
mod replay_fixtures;

fn origin(adapter: AdapterKind, model: &str) -> ReasoningOrigin {
    ReasoningOrigin::of(&ModelIden::new(adapter, model))
}

fn gemini() -> ReasoningOrigin {
    origin(AdapterKind::Gemini, "gemini-3.8-flash")
}

fn as_json(parts: Vec<ContentPart>) -> serde_json::Value {
    serde_json::to_value(parts).expect("genai parts serialize")
}

fn keyword(effort: Option<GenaiReasoningEffort>) -> Option<&'static str> {
    effort.map(|effort| effort.variant_name())
}

fn replay<'a>(messages: &'a [AgentMessage], origin: &'a ReasoningOrigin) -> Replay<'a> {
    Replay {
        tool_names: tool_names_by_call_id(messages),
        origin,
    }
}

fn gemini_tool_call_message() -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::ToolCall(AgentToolCall {
            call_id: "call-1".into(),
            tool: ToolId::AppState,
            arguments: json!({ "path": "/Users/x" }),
            reasoning: Some(gemini().reasoning_state(json!({ "thought_signatures": ["sig-abc"] }))),
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

    let gemini = gemini();
    let transcript = std::slice::from_ref(&original);
    let genai_message = agent_message_to_genai(&original, &replay(transcript, &gemini));
    let parts = genai_content_to_agent_parts(&genai_message.content, &gemini);
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
        blob_thought_signatures(&call.reasoning.as_ref().expect("reasoning must be preserved").blob),
        Some(vec!["sig-abc".to_string()])
    );
}

#[test]
fn every_reasoning_part_survives_in_wire_order_and_replays_identically() {
    // genai leads a captured answer with its reasoning blocks (signature, text, or a whole
    // provider block) and mirrors the signatures onto the first tool call. Each adapter
    // picks its own canonical copy on replay, so the parse keeps everything, in order, and
    // the replay hands genai back the exact parts it gave.
    let anthropic = origin(AdapterKind::Anthropic, "claude-opus-5-5");
    let content = MessageContent::from_parts(vec![
        ContentPart::ThoughtSignature("sig-first".into()),
        ContentPart::ReasoningContent(String::new()),
        ContentPart::Custom(CustomPart {
            model_iden: None,
            data: json!({ "type": "redacted_thinking", "data": "blob" }),
        }),
        ContentPart::Text("Let me look.".into()),
        ContentPart::ToolCall(GenaiToolCall {
            call_id: "call-1".into(),
            fn_name: ToolId::AppState.as_wire_name().into(),
            fn_arguments: json!({}),
            thought_signatures: Some(vec!["sig-first".into()]),
        }),
    ]);
    let parts = genai_content_to_agent_parts(&content, &anthropic);
    assert_eq!(parts.len(), 5, "nothing deduped away: {parts:?}");

    let message = AgentMessage {
        role: AgentRole::Assistant,
        parts,
        at: 1,
    };
    let transcript = std::slice::from_ref(&message);
    let replayed = agent_message_to_genai(&message, &replay(transcript, &anthropic));
    assert_eq!(
        serde_json::to_value(replayed.content.parts()).expect("serializes"),
        serde_json::to_value(content.parts()).expect("serializes")
    );
}

#[test]
fn reasoning_replays_only_to_the_model_that_produced_it() {
    // A signature is one provider's continuation token, and Claude's is useless (a 400) to
    // OpenAI or to another Claude. A thread that switches model drops the old reasoning.
    let opus = origin(AdapterKind::Anthropic, "claude-opus-5-5");
    let sonnet = origin(AdapterKind::Anthropic, "claude-sonnet-5-5");
    let part = opus.reasoning_part(thought_signatures_blob(vec!["sig".into()]));

    assert_eq!(
        as_json(agent_part_to_genai(&part, &replay(&[], &opus))),
        as_json(vec![ContentPart::ThoughtSignature("sig".into())])
    );
    assert!(agent_part_to_genai(&part, &replay(&[], &sonnet)).is_empty());

    // The same holds for a tool call's own signatures.
    let call = AgentPart::ToolCall(AgentToolCall {
        call_id: "c".into(),
        tool: ToolId::AppState,
        arguments: json!({}),
        reasoning: Some(opus.reasoning_state(thought_signatures_blob(vec!["sig".into()]))),
    });
    let sent = agent_part_to_genai(&call, &replay(&[], &sonnet));
    let [ContentPart::ToolCall(sent)] = sent.as_slice() else {
        panic!("expected the call");
    };
    assert_eq!(sent.thought_signatures, None);
}

#[test]
fn reasoning_stored_before_the_origin_stamp_replays_within_its_provider_family() {
    // Gemini threads persisted earlier carry `{thought_signatures}` with no stamp. They keep
    // replaying to Gemini, as they always did, and to nothing else.
    let legacy = AgentPart::Reasoning(ReasoningState {
        provider: ProviderTag::Gemini,
        blob: json!({ "thought_signatures": ["sig-old"] }),
    });
    assert_eq!(
        as_json(agent_part_to_genai(&legacy, &replay(&[], &gemini()))),
        as_json(vec![ContentPart::ThoughtSignature("sig-old".into())])
    );
    let openai = origin(AdapterKind::OpenAI, "gpt-4.1");
    assert!(agent_part_to_genai(&legacy, &replay(&[], &openai)).is_empty());
}

#[test]
fn effort_maps_to_genai_and_provider_default_sends_nothing() {
    assert_eq!(
        keyword(genai_effort(AdapterKind::Anthropic, ReasoningEffort::ProviderDefault)),
        None
    );
    assert_eq!(
        keyword(genai_effort(AdapterKind::DeepSeek, ReasoningEffort::ProviderDefault)),
        None
    );
    // `crate::ai` would fill `Low` here; an agent loop deserves OpenAI's own default.
    assert_eq!(
        keyword(genai_effort(AdapterKind::OpenAIResp, ReasoningEffort::ProviderDefault)),
        Some("medium")
    );
    assert_eq!(
        keyword(genai_effort(AdapterKind::Gemini, ReasoningEffort::Off)),
        Some("zero")
    );
    assert_eq!(
        keyword(genai_effort(AdapterKind::Anthropic, ReasoningEffort::Max)),
        Some("max")
    );
    let options = build_options(
        AdapterKind::Anthropic,
        CallSettings {
            effort: ReasoningEffort::High,
            max_output_tokens: 32_000,
            model_reasons: Some(true),
        },
        CachePlan::Breakpoints,
    );
    assert_eq!(keyword(options.reasoning_effort), Some("high"));
    assert_eq!(options.capture_reasoning_content, Some(true));
}

#[test]
fn a_model_known_not_to_reason_is_asked_for_no_reasoning_at_all() {
    // On the Responses API a model that doesn't reason 400s on `reasoning.effort` AND on the
    // `reasoning.summary` + encrypted include that reasoning capture turns on, so neither goes
    // out, whatever the effort says. `capture_reasoning_content: false` is also what keeps
    // `crate::ai`'s `adjust_for_model` from filling in its own effort.
    for effort in [ReasoningEffort::ProviderDefault, ReasoningEffort::High] {
        let options = build_options(
            AdapterKind::OpenAIResp,
            CallSettings {
                effort,
                model_reasons: Some(false),
                ..CallSettings::default()
            },
            CachePlan::Breakpoints,
        );
        assert_eq!(keyword(options.reasoning_effort), None, "{effort:?}");
        assert_eq!(options.capture_reasoning_content, Some(false), "{effort:?}");
    }
}

#[test]
fn a_nameless_tool_call_never_becomes_a_part() {
    // A provider can end a stream with `finish_reason: tool_call` yet hand back a
    // call carrying an id and nothing else (seen from Qwen via DashScope). It is
    // not an invocation: dispatch can't run it, and replaying it wedges the thread.
    let content = MessageContent::from_parts(vec![
        ContentPart::Text("here goes".into()),
        ContentPart::ToolCall(GenaiToolCall {
            call_id: "call-empty".into(),
            fn_name: String::new(),
            fn_arguments: json!(""),
            thought_signatures: None,
        }),
    ]);
    let parts = genai_content_to_agent_parts(&content, &origin(AdapterKind::OpenAI, "qwen3.7-plus"));
    assert_eq!(parts, vec![AgentPart::Text("here goes".into())]);
}

#[test]
fn a_nameless_call_and_the_result_answering_it_both_drop_out_of_the_replay() {
    // Threads persisted BEFORE the inbound guard still carry the broken pair, and
    // replaying it 400s forever. Dropping the call alone would leave an orphan tool
    // response, which adapters reject in turn, so the pair goes together — and a
    // message left with nothing in it goes with them.
    let transcript = vec![
        AgentMessage {
            role: AgentRole::User,
            parts: vec![AgentPart::Text("summarize this".into())],
            at: 1_000,
        },
        AgentMessage {
            role: AgentRole::Assistant,
            parts: vec![AgentPart::ToolCall(AgentToolCall {
                call_id: "call-empty".into(),
                tool: ToolId::from_wire_name(""),
                arguments: json!(""),
                reasoning: None,
            })],
            at: 1_001,
        },
        AgentMessage {
            role: AgentRole::Tool,
            parts: vec![AgentPart::ToolResult(AgentToolResult {
                call_id: "call-empty".into(),
                content: json!({ "available": false }),
                elided: false,
            })],
            at: 1_002,
        },
    ];

    let request = build_request("sys", &[], &transcript, &gemini(), CachePlan::Breakpoints);
    assert_eq!(request.messages.len(), 1, "only the user message survives the replay");
    assert_eq!(request.messages[0].role, ChatRole::User);
}

/// Regression anchor (#770, live on `gpt-6.1-terra`, 2026-10-29): a marked last message put
/// genai's GPT-5.6+ explicit mode on, and once that message was a tool result (no text block
/// to carry a breakpoint) the call cached nothing: call 2 of a tool loop read 0 and wrote 0.
/// Under the automatic plan nothing is marked, so OpenAI's own caching stays on.
#[test]
fn the_automatic_plan_marks_no_message() {
    let user = AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text("Where am I?".into())],
        at: 1,
    };
    let marked = |request: &ChatRequest| {
        request
            .messages
            .iter()
            .filter(|message| message.options.as_ref().is_some_and(|o| o.cache_control.is_some()))
            .count()
    };
    let responses = origin(AdapterKind::OpenAIResp, "gpt-6.1-terra");
    let automatic = build_request(
        "sys",
        &[],
        std::slice::from_ref(&user),
        &responses,
        CachePlan::Automatic,
    );
    assert_eq!(marked(&automatic), 0);
    let breakpoints = build_request("sys", &[], &[user], &responses, CachePlan::Breakpoints);
    assert_eq!(marked(&breakpoints), 1);
}

/// Unwrapping `use_tool` is the runtime's business, and it never reaches the provider: the model reads
/// back the call it sent, or a provider that signed it rejects the thread.
#[test]
fn a_use_tool_call_replays_as_the_call_the_provider_sent() {
    let arguments = json!({ "params": { "pane": "left" }, "name": "tag" });
    let call = AgentPart::ToolCall(AgentToolCall {
        call_id: "call-1".into(),
        tool: ToolId::UseTool,
        arguments: arguments.clone(),
        reasoning: None,
    });
    let parts = agent_part_to_genai(&call, &replay(&[], &gemini()));
    let [ContentPart::ToolCall(genai_call)] = parts.as_slice() else {
        panic!("expected one tool call part");
    };
    assert_eq!(genai_call.fn_name, "use_tool");
    assert_eq!(
        serde_json::to_string(&genai_call.fn_arguments).expect("json"),
        serde_json::to_string(&arguments).expect("json"),
    );
}

#[test]
fn tool_arguments_always_replay_as_a_json_object() {
    // A truncated tool call leaves a bare string behind. OpenAI-compatible endpoints
    // reject `"arguments": ""` outright ("must be in JSON format"), so an empty object
    // goes out instead: the tool's own validation answers it, and the model recovers.
    let call = AgentPart::ToolCall(AgentToolCall {
        call_id: "call-1".into(),
        tool: ToolId::InspectFile,
        arguments: json!(""),
        reasoning: None,
    });
    let parts = agent_part_to_genai(&call, &replay(&[], &gemini()));
    let [ContentPart::ToolCall(genai_call)] = parts.as_slice() else {
        panic!("expected one tool call part");
    };
    assert_eq!(genai_call.fn_arguments, json!({}));
}

#[test]
fn tool_result_carries_the_originating_tool_name() {
    // Gemini's `functionResponse.name` keys on the FUNCTION NAME, not the call ID,
    // so a result replayed without `fn_name` silently degrades tool calling there.
    // The name comes from the originating call earlier in the transcript.
    let transcript = vec![
        gemini_tool_call_message(),
        AgentMessage {
            role: AgentRole::Tool,
            parts: vec![AgentPart::ToolResult(AgentToolResult {
                call_id: "call-1".into(),
                content: json!({ "ok": true }),
                elided: false,
            })],
            at: 1_001,
        },
    ];

    let request = build_request("system", &[], &transcript, &gemini(), CachePlan::Breakpoints);
    let response = request
        .messages
        .iter()
        .flat_map(|message| message.content.parts())
        .find_map(|part| match part {
            ContentPart::ToolResponse(response) => Some(response.clone()),
            _ => None,
        })
        .expect("the tool result must map to a genai tool response");

    assert_eq!(response.call_id, "call-1");
    assert_eq!(response.fn_name.as_deref(), Some(ToolId::AppState.as_wire_name()));
}

#[test]
fn tool_result_without_a_matching_call_omits_the_tool_name() {
    // A result whose call fell out of the assembled window has no name to claim.
    // Sending a wrong one is worse than sending none, so it goes out unnamed.
    let transcript = vec![AgentMessage {
        role: AgentRole::Tool,
        parts: vec![AgentPart::ToolResult(AgentToolResult {
            call_id: "orphan".into(),
            content: json!({ "ok": true }),
            elided: false,
        })],
        at: 1_001,
    }];

    let request = build_request("system", &[], &transcript, &gemini(), CachePlan::Breakpoints);
    let response = request
        .messages
        .iter()
        .flat_map(|message| message.content.parts())
        .find_map(|part| match part {
            ContentPart::ToolResponse(response) => Some(response.clone()),
            _ => None,
        })
        .expect("the tool result must still map to a genai tool response");

    assert_eq!(response.fn_name, None);
}

#[test]
fn standalone_reasoning_survives_without_tool_calls() {
    // With no tool call to carry it, a standalone thought signature maps to a
    // `Reasoning` part rather than being dropped.
    let content = MessageContent::from_parts(vec![ContentPart::ThoughtSignature("sig-xyz".into())]);
    let gemini = gemini();
    let parts = genai_content_to_agent_parts(&content, &gemini);
    assert_eq!(
        parts,
        vec![gemini.reasoning_part(json!({ "thought_signatures": ["sig-xyz"] }))]
    );
}

fn tool_chunk(call_id: &str, fn_name: &str) -> genai::Result<ChatStreamEvent> {
    Ok(ChatStreamEvent::ToolCallChunk(genai::chat::ToolChunk {
        tool_call: GenaiToolCall {
            call_id: call_id.into(),
            fn_name: fn_name.into(),
            fn_arguments: json!(""),
            thought_signatures: None,
        },
    }))
}

#[tokio::test]
async fn a_tool_call_streamed_in_fragments_starts_once() {
    // Regression anchor for ERR-FRMAR: genai emits a `ToolCallChunk` per argument
    // fragment, all carrying the same id. Each became a `ToolCallStarted`, the rail
    // keyed two tool lines on one id, and Svelte's `each_key_duplicate` crashed the
    // window. A second, distinct call in the same reply still starts.
    let wire = ToolId::AppState.as_wire_name();
    let events = futures_util::stream::iter(vec![
        tool_chunk("abc", wire),
        tool_chunk("abc", wire),
        tool_chunk("abc", wire),
        tool_chunk("def", wire),
        tool_chunk("def", wire),
    ]);

    let started: Vec<String> = map_stream(events, origin(AdapterKind::OpenAI, "gpt-4.1-mini"))
        .filter_map(|delta| async move {
            match delta {
                Ok(AgentDelta::ToolCallStarted { call_id, .. }) => Some(call_id),
                _ => None,
            }
        })
        .collect()
        .await;

    assert_eq!(started, vec!["abc".to_string(), "def".to_string()]);
}

#[test]
fn agent_calls_leave_room_for_reasoning_and_a_tool_call() {
    // The ceiling is the call's own (`chat::effort::reply_room`), never a constant here.
    let options = build_options(
        AdapterKind::OpenAI,
        CallSettings {
            effort: ReasoningEffort::ProviderDefault,
            max_output_tokens: 32_000,
            model_reasons: None,
        },
        CachePlan::Breakpoints,
    );
    assert_eq!(options.max_tokens, Some(32_000));
}

#[test]
fn ai_error_maps_to_typed_agent_error() {
    // Status-classified upstream (by HTTP status), mapped variant-to-variant
    // here — never by matching the message string.
    assert_eq!(AgentLlmError::from(AiError::Unavailable), AgentLlmError::Unavailable);
    assert_eq!(AgentLlmError::from(AiError::Timeout), AgentLlmError::Timeout);
    // The provider's own wording rides along for display (never for control flow).
    assert_eq!(
        AgentLlmError::from(AiError::AuthFailed("bad key".into())),
        AgentLlmError::AuthFailed("bad key".into())
    );
    assert_eq!(
        AgentLlmError::from(AiError::RateLimited("slow down".into())),
        AgentLlmError::RateLimited("slow down".into())
    );
    assert_eq!(
        AgentLlmError::from(AiError::RequestTooLarge("TPM limit 8000".into())),
        AgentLlmError::RequestTooLarge("TPM limit 8000".into())
    );
    assert_eq!(
        AgentLlmError::from(AiError::ModelNotInPlan("rate limit exceeded".into())),
        AgentLlmError::ModelNotInPlan("rate limit exceeded".into())
    );
    // The provider's `Retry-After` survives the hop, so the runtime's retry can honor it.
    assert_eq!(
        AgentLlmError::from(AiError::Overloaded {
            detail: "HTTP 503: high demand".into(),
            retry_after: Some(std::time::Duration::from_secs(5)),
        }),
        AgentLlmError::Overloaded {
            detail: "HTTP 503: high demand".into(),
            retry_after: Some(std::time::Duration::from_secs(5)),
        }
    );
    assert!(matches!(
        AgentLlmError::from(AiError::EmptyResponse),
        AgentLlmError::Provider(_)
    ));
    assert!(matches!(
        AgentLlmError::from(AiError::NotFound("no such model".into())),
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
    // The client backstop's refusal mid-turn stays typed, never a provider sentence.
    use crate::managed_policy::ManagedAiRefusal;
    assert_eq!(
        AgentLlmError::from(AiError::Managed(ManagedAiRefusal::HostNotAllowed)),
        AgentLlmError::Managed(ManagedAiRefusal::HostNotAllowed)
    );
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
            ..AgentUsage::default()
        }
    );
}

/// The cost meter prices a cached prompt token at the cache rate (#770), so the read and
/// write counts genai reports have to survive the mapping, as shares of the whole prompt.
#[test]
fn usage_keeps_the_cache_reads_and_writes() {
    let usage = |read, write| GenaiUsage {
        prompt_tokens: Some(1_000),
        completion_tokens: Some(50),
        prompt_tokens_details: Some(genai::chat::PromptTokensDetails {
            cached_tokens: read,
            cache_creation_tokens: write,
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        map_usage(usage(Some(700), Some(200))),
        AgentUsage {
            prompt_tokens: 1_000,
            completion_tokens: 50,
            cache_read_tokens: 700,
            cache_write_tokens: 200,
        }
    );
    // A provider that reports more cached than it read in all can't make the plain share negative.
    let overcounted = map_usage(usage(Some(900), Some(400)));
    assert_eq!(
        (overcounted.cache_read_tokens, overcounted.cache_write_tokens),
        (900, 100)
    );
}

#[test]
fn thoughts_alone_still_meter_as_output() {
    // genai leaves `completion_tokens` unset when Gemini reports thoughts but no candidates;
    // the thoughts are billed output all the same.
    let usage = GenaiUsage {
        prompt_tokens: Some(10),
        completion_tokens: None,
        completion_tokens_details: Some(genai::chat::CompletionTokensDetails {
            reasoning_tokens: Some(42),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(map_usage(usage).completion_tokens, 42);
}

#[test]
fn declared_tools_are_never_strict() {
    let decl = ToolDeclaration {
        name: ToolId::AppState,
        description: "look".into(),
        schema: json!({ "type": "object" }),
    };
    let tool = tool_declaration_to_genai(&decl);
    assert!(tool.strict.is_none(), "strict must stay unset (Gap D)");
}
