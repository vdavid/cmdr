//! Two-round tool loops through the REAL genai adapters against a local server that plays
//! back recorded-shape provider streams, one per reasoning dialect.
//!
//! Round one streams reasoning plus a tool call; the test persists the `End` message the way
//! the store does (a serde round-trip), sends it back with a tool result, and asserts what
//! round two's request body carries. That body is the whole point: it's what each provider
//! validates, and a reasoning block that's missing or reshaped there is a 400 (Anthropic,
//! DeepSeek) or a model that forgets why it called the tool (OpenAI Responses, Gemini).
//!
//! The streams are trimmed from genai 0.7.1's own recordings of the live APIs
//! (`tests/data/yakbak/`), with our tool name swapped in. Shapes per provider:
//! `../../DETAILS.md` § Reasoning round-trip.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;
use futures_util::StreamExt;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::super::GenaiAgentLlm;
use crate::agent::llm::AgentLlm;
use crate::agent::llm::types::{
    AgentDelta, AgentMessage, AgentPart, AgentRole, AgentToolResult, ReasoningEffort, ToolDeclaration, ToolId,
};
use crate::ai::client::AiBackend;
use crate::ignore_poison::IgnorePoison;

const WIRE: &str = "app_state";

#[derive(Clone)]
struct Playback {
    responses: Arc<Vec<String>>,
    requests: Arc<Mutex<Vec<Value>>>,
}

async fn play(State(state): State<Playback>, body: Bytes) -> impl IntoResponse {
    let request: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let index = {
        let mut requests = state.requests.lock_ignore_poison();
        requests.push(request);
        requests.len() - 1
    };
    let stream = state.responses.get(index).cloned().unwrap_or_default();
    ([(header::CONTENT_TYPE, "text/event-stream")], stream)
}

/// Serves `responses` in order, one per POST, and returns the base URL plus every request body.
async fn serve(responses: Vec<String>) -> (String, Arc<Mutex<Vec<Value>>>) {
    let state = Playback {
        responses: Arc::new(responses),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let requests = state.requests.clone();
    let app = Router::new().fallback(play).with_state(state);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("addr").port();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://127.0.0.1:{port}/v1/"), requests)
}

fn tools() -> Vec<ToolDeclaration> {
    vec![ToolDeclaration {
        name: ToolId::AppState,
        description: "Read the panes.".into(),
        schema: json!({ "type": "object", "properties": {} }),
    }]
}

async fn respond(llm: &GenaiAgentLlm, messages: &[AgentMessage]) -> AgentMessage {
    let mut stream = llm
        .respond(
            "sys",
            &tools(),
            messages,
            crate::agent::llm::CallSettings {
                effort: ReasoningEffort::High,
                max_output_tokens: 32_000,
                model_reasons: None,
            },
            CancellationToken::new(),
        )
        .await
        .expect("the stream opens");
    while let Some(delta) = stream.next().await {
        if let AgentDelta::End { message, .. } = delta.expect("no stream error") {
            return message;
        }
    }
    panic!("the stream ended without an End");
}

/// Runs round one, persists its answer the way the store does, then round two with a tool
/// result, against `round_two_model` (the same model, unless a test switches it). Returns the
/// round-one message and round two's request body.
async fn two_rounds(model: &str, round_two_model: &str, streams: [String; 2]) -> (AgentMessage, Value) {
    two_rounds_at(model, round_two_model, streams, true).await
}

/// [`two_rounds`] against an endpoint that does (`chat_reasoning`) or doesn't take a
/// chat-completions turn's reasoning back. The native protocols ignore it.
async fn two_rounds_at(
    model: &str,
    round_two_model: &str,
    streams: [String; 2],
    chat_reasoning: bool,
) -> (AgentMessage, Value) {
    let (base_url, requests) = serve(streams.to_vec()).await;
    let user = AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text("Where am I?".into())],
        at: 1,
    };
    let backend = |model: &str| {
        AiBackend::remote_for_tests("k".into(), base_url.clone(), model.into())
            .with_chat_reasoning_replay(chat_reasoning)
    };
    let first = GenaiAgentLlm::new(backend(model));
    let answer = respond(&first, std::slice::from_ref(&user)).await;

    let persisted: AgentMessage =
        serde_json::from_value(serde_json::to_value(&answer).expect("serializes")).expect("deserializes");
    let call_id = persisted
        .parts
        .iter()
        .find_map(|part| match part {
            AgentPart::ToolCall(call) => Some(call.call_id.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("round one called no tool: {:?}", persisted.parts));
    let result = AgentMessage {
        role: AgentRole::Tool,
        parts: vec![AgentPart::ToolResult(AgentToolResult {
            call_id,
            content: json!({ "left": "/Users/x" }),
            elided: false,
        })],
        at: 3,
    };

    let second = GenaiAgentLlm::new(backend(round_two_model));
    respond(&second, &[user, persisted, result]).await;
    let body = requests
        .lock_ignore_poison()
        .get(1)
        .cloned()
        .expect("round two was sent");
    (answer, body)
}

fn sse(events: &[Value]) -> String {
    events
        .iter()
        .map(|event| match event.get("type").and_then(Value::as_str) {
            Some(name) => format!("event: {name}\ndata: {event}\n\n"),
            None => format!("data: {event}\n\n"),
        })
        .collect()
}

// region: --- Anthropic

fn anthropic_round_one() -> String {
    sse(&[
        json!({"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"model":"claude-opus-5-5","stop_reason":null,"usage":{"input_tokens":13,"output_tokens":0}}}),
        // Claude 5.x's default display: the thinking text is omitted, only the signature comes.
        json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"signature_delta","signature":"sig-first"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"redacted_thinking","data":"redacted-blob"}}),
        json!({"type":"content_block_stop","index":1}),
        json!({"type":"content_block_start","index":2,"content_block":{"type":"thinking","thinking":"","signature":""}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"thinking_delta","thinking":"Check the panes."}}),
        json!({"type":"content_block_delta","index":2,"delta":{"type":"signature_delta","signature":"sig-third"}}),
        json!({"type":"content_block_stop","index":2}),
        json!({"type":"content_block_start","index":3,"content_block":{"type":"tool_use","id":"toolu_1","name":WIRE,"input":{}}}),
        json!({"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"{}"}}),
        json!({"type":"content_block_stop","index":3}),
        json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
        json!({"type":"message_stop"}),
    ])
}

fn anthropic_round_two() -> String {
    sse(&[
        json!({"type":"message_start","message":{"id":"msg_2","type":"message","role":"assistant","content":[],"model":"claude-opus-5-5","stop_reason":null,"usage":{"input_tokens":40,"output_tokens":0}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"You're in /Users/x."}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":6}}),
        json!({"type":"message_stop"}),
    ])
}

/// The assistant turn round two sends back, by role.
fn assistant_turn<'a>(body: &'a Value, list: &str) -> &'a Value {
    body[list]
        .as_array()
        .expect("a message list")
        .iter()
        .find(|message| message["role"] == "assistant" || message["role"] == "model")
        .expect("the assistant turn is replayed")
}

#[tokio::test]
async fn anthropic_replays_signed_omitted_and_redacted_thinking_in_order() {
    let (_, body) = two_rounds(
        "claude-opus-5-5",
        "claude-opus-5-5",
        [anthropic_round_one(), anthropic_round_two()],
    )
    .await;
    let content = &assistant_turn(&body, "messages")["content"];
    assert_eq!(
        content[0],
        json!({"type":"thinking","thinking":"","signature":"sig-first"})
    );
    assert_eq!(content[1], json!({"type":"redacted_thinking","data":"redacted-blob"}));
    assert_eq!(
        content[2],
        json!({"type":"thinking","thinking":"Check the panes.","signature":"sig-third"})
    );
    assert_eq!(content[3]["type"], "tool_use");
    assert_eq!(content.as_array().map(Vec::len), Some(4), "{content}");
    assert_eq!(body["output_config"]["effort"], "high");
}

#[tokio::test]
async fn a_model_switch_drops_the_old_models_thinking() {
    let (_, body) = two_rounds(
        "claude-opus-5-5",
        "claude-sonnet-5-5",
        [anthropic_round_one(), anthropic_round_two()],
    )
    .await;
    let content = &assistant_turn(&body, "messages")["content"];
    assert_eq!(content.as_array().map(Vec::len), Some(1), "{content}");
    assert_eq!(content[0]["type"], "tool_use");
}

/// Anthropic caches only where a request says so. Two breakpoints: the system string (so the
/// tools and system prompt every thread shares stay cached across threads) and the end of the
/// last message (so the next call, the same bytes plus a round, reads the rest back cached).
/// Without them every call re-bills the whole thread at full price.
#[tokio::test]
async fn anthropic_marks_the_system_string_and_the_last_message_for_caching() {
    let (_, body) = two_rounds(
        "claude-opus-5-5",
        "claude-opus-5-5",
        [anthropic_round_one(), anthropic_round_two()],
    )
    .await;
    let ephemeral = json!({ "type": "ephemeral" });
    assert_eq!(
        body["system"],
        json!([{ "type": "text", "text": "sys", "cache_control": ephemeral }]),
        "the system string goes out as one marked block"
    );
    let messages = body["messages"].as_array().expect("a message list");
    let last_block = messages
        .last()
        .and_then(|message| message["content"].as_array())
        .and_then(|content| content.last())
        .expect("the last message has content blocks");
    assert_eq!(last_block["cache_control"], ephemeral, "{last_block}");
    let marked = body.to_string().matches("cache_control").count();
    assert_eq!(marked, 2, "two breakpoints, well under Anthropic's four: {body}");
}

// endregion: --- Anthropic

// region: --- OpenAI Responses

fn responses_round_one() -> String {
    let reasoning = json!({"id":"rs_1","type":"reasoning","summary":[],"encrypted_content":"enc-openai-1"});
    let call = json!({"id":"fc_1","type":"function_call","status":"completed","arguments":"{}","call_id":"call_1","name":WIRE});
    sse(&[
        json!({"type":"response.created","response":{"id":"resp_1","object":"response","model":"gpt-5.5","status":"in_progress","output":[]},"sequence_number":0}),
        json!({"type":"response.output_item.added","item":{"id":"rs_1","type":"reasoning","summary":[]},"output_index":0,"sequence_number":1}),
        json!({"type":"response.output_item.done","item":reasoning,"output_index":0,"sequence_number":2}),
        json!({"type":"response.output_item.added","item":{"id":"fc_1","type":"function_call","status":"in_progress","arguments":"","call_id":"call_1","name":WIRE},"output_index":1,"sequence_number":3}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc_1","output_index":1,"delta":"{}","sequence_number":4}),
        json!({"type":"response.function_call_arguments.done","item_id":"fc_1","output_index":1,"arguments":"{}","sequence_number":5}),
        json!({"type":"response.output_item.done","item":call,"output_index":1,"sequence_number":6}),
        json!({"type":"response.completed","response":{"id":"resp_1","object":"response","model":"gpt-5.5","status":"completed","output":[reasoning, call],"usage":{"input_tokens":20,"input_tokens_details":{"cached_tokens":0},"output_tokens":30,"output_tokens_details":{"reasoning_tokens":20},"total_tokens":50}},"sequence_number":7}),
    ])
}

fn responses_round_two() -> String {
    let message = json!({"id":"msg_1","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"You're in /Users/x.","annotations":[]}]});
    sse(&[
        json!({"type":"response.created","response":{"id":"resp_2","object":"response","model":"gpt-5.5","status":"in_progress","output":[]},"sequence_number":0}),
        json!({"type":"response.output_text.delta","item_id":"msg_1","output_index":0,"content_index":0,"delta":"You're in /Users/x.","sequence_number":1}),
        json!({"type":"response.completed","response":{"id":"resp_2","object":"response","model":"gpt-5.5","status":"completed","output":[message],"usage":{"input_tokens":60,"input_tokens_details":{"cached_tokens":0},"output_tokens":8,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":68}},"sequence_number":2}),
    ])
}

#[tokio::test]
async fn openai_responses_replays_the_encrypted_reasoning_item_before_the_call() {
    let (_, body) = two_rounds("gpt-5.5", "gpt-5.5", [responses_round_one(), responses_round_two()]).await;
    let input = body["input"].as_array().expect("an input list");
    let reasoning_at = input
        .iter()
        .position(|item| item["type"] == "reasoning")
        .expect("the reasoning item rides back");
    assert_eq!(input[reasoning_at]["encrypted_content"], "enc-openai-1");
    let call_at = input
        .iter()
        .position(|item| item["type"] == "function_call")
        .expect("the call rides back");
    assert!(reasoning_at < call_at, "reasoning precedes its call: {input:?}");
    assert_eq!(
        input.iter().filter(|item| item["type"] == "reasoning").count(),
        1,
        "replayed once, not once per copy"
    );
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(body["reasoning"]["effort"], "high");
}

// endregion: --- OpenAI Responses

// region: --- Gemini

fn gemini_round_one() -> String {
    sse(&[
        json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":WIRE,"args":{}},"thoughtSignature":"gsig-1"}],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"thoughtsTokenCount":15,"totalTokenCount":30},"modelVersion":"gemini-3.8-flash"}),
    ])
}

fn gemini_round_two() -> String {
    sse(&[
        json!({"candidates":[{"content":{"parts":[{"text":"You're in /Users/x."}],"role":"model"},"finishReason":"STOP","index":0}],"usageMetadata":{"promptTokenCount":40,"candidatesTokenCount":6,"totalTokenCount":46},"modelVersion":"gemini-3.8-flash"}),
    ])
}

#[tokio::test]
async fn gemini_replays_the_signature_on_its_function_call_once() {
    let (_, body) = two_rounds(
        "gemini-3.8-flash",
        "gemini-3.8-flash",
        [gemini_round_one(), gemini_round_two()],
    )
    .await;
    let parts = assistant_turn(&body, "contents")["parts"]
        .as_array()
        .expect("model parts");
    assert_eq!(parts.len(), 1, "{parts:?}");
    assert_eq!(parts[0]["functionCall"]["name"], WIRE);
    assert_eq!(parts[0]["thoughtSignature"], "gsig-1");
}

// endregion: --- Gemini

// region: --- Chat completions (DeepSeek, Groq, local servers)

fn chat_chunk(delta: Value, finish: Option<&str>) -> Value {
    json!({"id":"c1","object":"chat.completion.chunk","created":0,"model":"deepseek-v4-pro","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}

fn chat_round_one() -> String {
    let mut body = sse(&[
        chat_chunk(
            json!({"role":"assistant","content":null,"reasoning_content":"I should "}),
            None,
        ),
        chat_chunk(json!({"reasoning_content":"read the panes."}), None),
        chat_chunk(
            json!({"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":WIRE,"arguments":"{}"}}]}),
            None,
        ),
        chat_chunk(json!({}), Some("tool_calls")),
    ]);
    body.push_str("data: [DONE]\n\n");
    body
}

fn chat_round_two() -> String {
    let mut body = sse(&[
        chat_chunk(json!({"role":"assistant","content":"You're in /Users/x."}), None),
        chat_chunk(json!({}), Some("stop")),
    ]);
    body.push_str("data: [DONE]\n\n");
    body
}

#[tokio::test]
async fn chat_completions_replays_reasoning_content_beside_the_tool_call() {
    // DeepSeek's thinking mode 400s a tool loop whose assistant turns come back without
    // their `reasoning_content`. genai streams it outside the captured content, so this is
    // the path that has to rejoin it.
    let (answer, body) = two_rounds(
        "deepseek-v4-pro",
        "deepseek-v4-pro",
        [chat_round_one(), chat_round_two()],
    )
    .await;
    assert!(
        matches!(answer.parts.first(), Some(AgentPart::Reasoning(_))),
        "the reasoning leads the persisted turn: {:?}",
        answer.parts
    );
    let assistant = assistant_turn(&body, "messages");
    assert_eq!(assistant["reasoning_content"], "I should read the panes.");
    assert_eq!(assistant["tool_calls"][0]["function"]["name"], WIRE);
}

#[tokio::test]
async fn a_host_that_rejects_reasoning_content_gets_none_back() {
    // Groq 400s an assistant turn carrying `reasoning_content`, so a host not known to take it
    // keeps the no-reasoning replay it always had, and nothing is persisted to leak later.
    let (answer, body) = two_rounds_at(
        "openai/gpt-oss-120b",
        "openai/gpt-oss-120b",
        [chat_round_one(), chat_round_two()],
        false,
    )
    .await;
    assert!(
        !answer.parts.iter().any(|part| matches!(part, AgentPart::Reasoning(_))),
        "{:?}",
        answer.parts
    );
    let assistant = assistant_turn(&body, "messages");
    assert!(assistant.get("reasoning_content").is_none(), "{assistant}");
}

/// OpenRouter streams a signed Claude thinking block in pieces sharing `index: 0` (the
/// signature on a later piece) beside the plaintext `reasoning`, then an encrypted block
/// whole. The shapes follow OpenRouter's reasoning-tokens guide, "Streaming Response".
fn openrouter_round_one() -> String {
    let mut body = sse(&[
        chat_chunk(
            json!({"role":"assistant","content":"","reasoning":"Read ","reasoning_details":[
                {"type":"reasoning.text","text":"Read ","signature":null,"format":"anthropic-claude-v1","index":0}
            ]}),
            None,
        ),
        chat_chunk(
            json!({"reasoning":"the panes.","reasoning_details":[
                {"type":"reasoning.text","text":"the panes.","signature":null,"index":0}
            ]}),
            None,
        ),
        chat_chunk(
            json!({"reasoning_details":[
                {"type":"reasoning.text","signature":"sig-or-1","index":0},
                {"type":"reasoning.encrypted","data":"opaque-1","format":"anthropic-claude-v1","index":1}
            ]}),
            None,
        ),
        chat_chunk(
            json!({"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":WIRE,"arguments":"{}"}}]}),
            None,
        ),
        chat_chunk(json!({}), Some("tool_calls")),
    ]);
    body.push_str("data: [DONE]\n\n");
    body
}

#[tokio::test]
async fn openrouter_replays_streamed_reasoning_details_verbatim() {
    // OpenRouter carries a signed or encrypted model's continuity in `reasoning_details` and
    // wants the blocks back whole and in order. Upstream genai drops them when streaming; the
    // vendored patch (`vendor/genai/CMDR-PATCH.md`) merges the pieces into blocks.
    let (_, body) = two_rounds(
        "anthropic/claude-sonnet-5.5",
        "anthropic/claude-sonnet-5.5",
        [openrouter_round_one(), chat_round_two()],
    )
    .await;
    let assistant = assistant_turn(&body, "messages");
    assert_eq!(
        assistant["reasoning_details"],
        json!([
            {"type":"reasoning.text","text":"Read the panes.","signature":"sig-or-1","format":"anthropic-claude-v1","index":0},
            {"type":"reasoning.encrypted","data":"opaque-1","format":"anthropic-claude-v1","index":1}
        ])
    );
    assert_eq!(assistant["reasoning_content"], "Read the panes.");
}

/// Two signed Claude thinking blocks in one answer, every piece at `index: 0`, as some
/// providers stream them. Regression anchor: merging by `index` fused them into one block
/// with the first signature, so the next turn's signature check refused the replay.
fn openrouter_round_one_two_signed_blocks() -> String {
    let text = |text: &str| json!({"reasoning_details":[{"type":"reasoning.text","text":text,"signature":null,"format":"anthropic-claude-v1","index":0}]});
    let signature = |signature: &str| json!({"reasoning_details":[{"type":"reasoning.text","signature":signature,"format":"anthropic-claude-v1","index":0}]});
    let mut body = sse(&[
        chat_chunk(text("Read the panes."), None),
        chat_chunk(signature("sig-or-1"), None),
        chat_chunk(text("Then list them."), None),
        chat_chunk(signature("sig-or-2"), None),
        chat_chunk(
            json!({"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":WIRE,"arguments":"{}"}}]}),
            None,
        ),
        chat_chunk(json!({}), Some("tool_calls")),
    ]);
    body.push_str("data: [DONE]\n\n");
    body
}

#[tokio::test]
async fn openrouter_keeps_index_zero_signed_reasoning_blocks_apart() {
    let (_, body) = two_rounds(
        "anthropic/claude-sonnet-5.5",
        "anthropic/claude-sonnet-5.5",
        [openrouter_round_one_two_signed_blocks(), chat_round_two()],
    )
    .await;
    let assistant = assistant_turn(&body, "messages");
    assert_eq!(
        assistant["reasoning_details"],
        json!([
            {"type":"reasoning.text","text":"Read the panes.","signature":"sig-or-1","format":"anthropic-claude-v1","index":0},
            {"type":"reasoning.text","text":"Then list them.","signature":"sig-or-2","format":"anthropic-claude-v1","index":0}
        ])
    );
}

// endregion: --- Chat completions
