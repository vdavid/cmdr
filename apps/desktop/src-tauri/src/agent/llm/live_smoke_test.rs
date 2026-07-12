//! Gated live smokes for the genai-backed [`GenaiAgentLlm`], one per Tier-1 cloud
//! provider plus the local slot. Each drives ONE real streaming `respond` call
//! through the whole seam (build request → exec → map deltas → final message), so a
//! regression in adapter routing, tool-call mapping, or stream handling fails here
//! instead of silently in production. These are the M1 preview of M8's full
//! certification pass (a ≥3-step loop, reasoning on/off per provider).
//!
//! `#[ignore]`-gated on the matching env var, never in CI's critical path. Verify
//! current model ids from each provider's models endpoint at run time — never from
//! training data. Run manually, for example:
//!
//! ```sh
//! ANTHROPIC_API_KEY=$(secret ANTHROPIC_API_KEY) \
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
use super::types::{AgentDelta, AgentMessage, AgentPart, AgentRole};

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

#[tokio::test]
#[ignore = "real API call: set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic() {
    let key = env_or_skip("ANTHROPIC_API_KEY");
    // Reasoning is forced off for Anthropic in v1 (spike Gap A); a disablable model.
    let backend = AiBackend::remote(
        key,
        "https://api.anthropic.com/".into(),
        "claude-3-5-haiku-latest".into(),
    );
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai() {
    let key = env_or_skip("OPENAI_API_KEY");
    let backend = AiBackend::remote(key, "https://api.openai.com/v1/".into(), "gpt-4o-mini".into());
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini() {
    let key = env_or_skip("GEMINI_API_KEY");
    let backend = AiBackend::remote(
        key,
        "https://generativelanguage.googleapis.com/".into(),
        "gemini-2.5-flash".into(),
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
    let backend = AiBackend::local(port);
    let answer = run_one(backend, "Say hello.").await;
    assert!(!answer.trim().is_empty(), "expected a visible answer, got {answer:?}");
}
