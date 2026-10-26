//! Gated live smokes for the genai-backed [`GenaiAgentLlm`], one per Tier-1 cloud
//! provider plus the local slot. Each drives ONE real streaming `respond` call
//! through the whole seam (build request → exec → map deltas → final message), so a
//! regression in adapter routing, tool-call mapping, or stream handling fails here
//! instead of silently in production. Anthropic also runs a real two-round tool loop
//! per current Claude model (`smoke_anthropic_tool_loop_*`), the case that proves a
//! thinking-on loop survives genai dropping the thinking blocks on replay.
//!
//! `#[ignore]`-gated on the matching env var, never in CI's critical path. Verify
//! current model ids from each provider's models endpoint at run time — never from
//! training data. Run manually, for example:
//!
//! ```sh
//! ANTHROPIC_API_KEY=$(secret ANTHROPIC_API_KEY_CMDR_DEV) \
//!   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test
//! OPENAI_API_KEY=$(secret OPENAI_API_KEY)   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_openai
//! GEMINI_API_KEY=$(secret GEMINI_API_KEY)   cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_gemini
//! # Local: point at a running Cmdr llama-server.
//! LLAMA_PORT=18437 cargo nextest run --lib --run-ignored only agent::llm::live_smoke_test::smoke_local
//! ```

use futures_util::stream::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::ai::client::AiBackend;

use super::AgentLlm;
use super::genai_impl::GenaiAgentLlm;
use super::types::{
    AgentDelta, AgentMessage, AgentPart, AgentRole, AgentStopReason, AgentToolResult, ToolDeclaration, ToolId,
};

fn env_or_skip(var: &str) -> String {
    match std::env::var(var) {
        Ok(value) if !value.trim().is_empty() => value,
        _ => panic!("{var} not set"),
    }
}

fn user_turn(text: &str) -> Vec<AgentMessage> {
    vec![AgentMessage {
        role: AgentRole::User,
        parts: vec![AgentPart::Text(text.to_string())],
        at: 0,
    }]
}

/// Drives one `respond` call to completion and returns the joined visible text of
/// the final assembled message. Asserts the stream ended with an `End` delta.
async fn run_one(backend: AiBackend, prompt: &str) -> String {
    let llm = GenaiAgentLlm::new(backend);
    let messages = user_turn(prompt);
    let stream = llm
        .respond(
            "You are a terse assistant. Answer in one short sentence.",
            &[],
            &messages,
            CancellationToken::new(),
        )
        .await
        .expect("respond should open a stream");

    let deltas: Vec<AgentDelta> = stream.map(|item| item.expect("no stream error")).collect().await;

    let Some(AgentDelta::End { message, .. }) = deltas.last() else {
        panic!("stream did not end with an End delta");
    };
    message
        .parts
        .iter()
        .filter_map(|part| match part {
            AgentPart::Text(text) => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// What one multi-step tool loop saw, for the per-model assertions.
struct LoopOutcome {
    tool_rounds: usize,
    /// `ReasoningTick` deltas across the loop: proof a thinking block came back, since
    /// Claude 5.x omits the thinking text and genai keeps no Anthropic signature.
    reasoning_ticks: usize,
    answer: String,
}

/// Canned `list_dir` answers. The root names three `reports-N` folders and only the
/// one the prompt's arithmetic picks holds the PDFs, so the loop needs at least two
/// tool rounds, and the arithmetic coaxes a thinking model into a thinking block that
/// the next request then replays (or drops) ahead of the tool result.
fn fake_list_dir(arguments: &serde_json::Value) -> serde_json::Value {
    let path = arguments.get("path").and_then(|p| p.as_str()).unwrap_or_default();
    let path = path.trim_end_matches('/');
    if path.ends_with("reports-1592") {
        serde_json::json!({ "entries": [
            { "name": "q1.pdf", "kind": "file", "size": 120_000 },
            { "name": "q2.pdf", "kind": "file", "size": 98_000 },
            { "name": "q3.pdf", "kind": "file", "size": 143_000 },
        ]})
    } else if path.ends_with("reports-1591") || path.ends_with("reports-1593") {
        serde_json::json!({ "entries": [] })
    } else {
        serde_json::json!({ "entries": [
            { "name": "reports-1591", "kind": "folder" },
            { "name": "reports-1592", "kind": "folder" },
            { "name": "reports-1593", "kind": "folder" },
            { "name": "notes.txt", "kind": "file", "size": 2_000 },
        ]})
    }
}

/// Drives a real tool loop through `GenaiAgentLlm` the way `run_turn` does: each
/// `End` message is replayed verbatim as the assistant turn (reasoning parts and
/// all), followed by a tool turn with the results, until the model answers in prose.
async fn run_tool_loop(backend: AiBackend) -> LoopOutcome {
    let llm = GenaiAgentLlm::new(backend);
    let tools = [ToolDeclaration {
        name: ToolId::ListDir,
        description: "List one directory's immediate children.".into(),
        schema: serde_json::json!({
            "type": "object",
            "properties": { "path": { "type": "string", "description": "Absolute folder path" } },
            "required": ["path"],
        }),
    }];
    let mut messages = user_turn(
        "Call list_dir on /Users/test/work. Then work out N = 37 * 43 + 1 and call \
         list_dir on the reports-N folder it shows. How many PDF files are in it? \
         Answer with the number.",
    );
    let mut outcome = LoopOutcome {
        tool_rounds: 0,
        reasoning_ticks: 0,
        answer: String::new(),
    };

    for _ in 0..5 {
        let stream = llm
            .respond(
                "You are a file assistant. Use the tools; never guess folder contents.",
                &tools,
                &messages,
                CancellationToken::new(),
            )
            .await
            .unwrap_or_else(|error| panic!("respond failed on step {}: {error:?}", outcome.tool_rounds + 1));
        let deltas: Vec<AgentDelta> = stream
            .map(|item| item.unwrap_or_else(|error| panic!("stream error: {error:?}")))
            .collect()
            .await;
        let Some(AgentDelta::End { stop, message, .. }) = deltas.last() else {
            panic!("stream did not end with an End delta");
        };

        outcome.reasoning_ticks += deltas
            .iter()
            .filter(|delta| matches!(delta, AgentDelta::ReasoningTick))
            .count();
        let results: Vec<AgentPart> = message
            .parts
            .iter()
            .filter_map(|part| match part {
                AgentPart::ToolCall(call) => Some(AgentPart::ToolResult(AgentToolResult {
                    call_id: call.call_id.clone(),
                    content: fake_list_dir(&call.arguments),
                    elided: false,
                })),
                _ => None,
            })
            .collect();
        messages.push(message.clone());

        if results.is_empty() {
            assert_eq!(*stop, AgentStopReason::Completed, "final turn should complete");
            outcome.answer = message
                .parts
                .iter()
                .filter_map(|part| match part {
                    AgentPart::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            return outcome;
        }
        outcome.tool_rounds += 1;
        messages.push(AgentMessage {
            role: AgentRole::Tool,
            parts: results,
            at: 0,
        });
    }
    panic!("the model never answered after {} tool rounds", outcome.tool_rounds);
}

fn anthropic_backend(model: &str) -> AiBackend {
    AiBackend::remote_for_tests(
        env_or_skip("ANTHROPIC_API_KEY"),
        // The app's own base URL (`cloud-providers.ts`); without `/v1/` genai 404s.
        "https://api.anthropic.com/v1/".into(),
        model.into(),
    )
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic() {
    let answer = run_one(anthropic_backend("claude-haiku-5-5"), "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

async fn assert_anthropic_tool_loop(model: &str) {
    let outcome = run_tool_loop(anthropic_backend(model)).await;
    assert!(
        outcome.tool_rounds >= 2,
        "{model}: expected 2+ tool rounds, got {}",
        outcome.tool_rounds
    );
    assert!(
        outcome.answer.contains('3'),
        "{model}: expected the answer 3, got {:?}",
        outcome.answer
    );
    // No assertion on `reasoning_ticks`: adaptive thinking may skip a turn. On
    // 2026-10-26 all three models thought and the thinking-dropped replay still passed.
    log::info!(
        target: "agent::llm",
        "{model}: {} tool rounds, {} reasoning ticks",
        outcome.tool_rounds,
        outcome.reasoning_ticks
    );
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_haiku() {
    assert_anthropic_tool_loop("claude-haiku-5-5").await;
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_sonnet() {
    assert_anthropic_tool_loop("claude-sonnet-5-5").await;
}

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_tool_loop_opus() {
    assert_anthropic_tool_loop("claude-opus-5-5").await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai() {
    let key = env_or_skip("OPENAI_API_KEY");
    let backend = AiBackend::remote_for_tests(key, "https://api.openai.com/v1/".into(), "gpt-4o-mini".into());
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini() {
    let key = env_or_skip("GEMINI_API_KEY");
    // The app's Gemini preset: the native `v1/` API (a version-less base 404s) and its default
    // model (`gemini-2.5-flash` is closed to new users, 2026-10-26).
    let backend = AiBackend::remote_for_tests(
        key,
        "https://generativelanguage.googleapis.com/v1/".into(),
        "gemini-3.8-flash".into(),
    );
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "local server: set LLAMA_PORT to a running Cmdr llama-server to run"]
async fn smoke_local() {
    let port: u16 = env_or_skip("LLAMA_PORT")
        .parse()
        .expect("LLAMA_PORT must be a port number");
    let backend = AiBackend::local_for_tests(port);
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}
