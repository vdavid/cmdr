use super::*;
use serde_json::json;

fn gemini_tool_call_message() -> AgentMessage {
    AgentMessage {
        role: AgentRole::Assistant,
        parts: vec![AgentPart::ToolCall(AgentToolCall {
            call_id: "call-1".into(),
            tool: ToolId::AppState,
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

    let genai_message = agent_message_to_genai(&original, &tool_names_by_call_id(std::slice::from_ref(&original)));
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
    let parts = genai_content_to_agent_parts(&content, ProviderTag::OpenAi);
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

    let request = build_request("sys", &[], &transcript);
    assert_eq!(request.messages.len(), 1, "only the user message survives the replay");
    assert_eq!(request.messages[0].role, ChatRole::User);
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
    let parts = agent_part_to_genai(&call, &ToolNames::new());
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

    let request = build_request("system", &[], &transcript);
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

    let request = build_request("system", &[], &transcript);
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
    let parts = genai_content_to_agent_parts(&content, ProviderTag::Gemini);
    assert_eq!(
        parts,
        vec![AgentPart::Reasoning(ReasoningState {
            provider: ProviderTag::Gemini,
            blob: json!({ "thought_signatures": ["sig-xyz"] }),
        })]
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

    let started: Vec<String> = map_stream(events, ProviderTag::OpenAi)
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
    let options = build_options(AdapterKind::OpenAI);
    assert_eq!(options.max_tokens, Some(AGENT_MAX_OUTPUT_TOKENS));
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
        }
    );
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
