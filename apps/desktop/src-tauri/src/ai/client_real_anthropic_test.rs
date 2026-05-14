//! Real-API smoke tests against Anthropic. **Not run in CI**: gated behind `#[ignore]`,
//! requires a valid `ANTHROPIC_API_KEY` env var.
//!
//! Why a separate file from `client_real_openai_test.rs`: Anthropic's native streaming
//! protocol differs from OpenAI's SSE shape (event names like `content_block_delta`
//! vs `data:` JSON envelopes). Without exercising it, we'd be testing only the OpenAI
//! lineage despite supporting Anthropic via `genai`.
//!
//! Run with:
//! ```sh
//! ANTHROPIC_API_KEY=$(security find-generic-password -a "$USER" -s "ANTHROPIC_API_KEY" -w) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_anthropic_test
//! ```
//!
//! Costs ~$0.001 per full run.

use futures_util::StreamExt;
use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion, chat_completion_stream};

const BASE_URL: &str = "https://api.anthropic.com/v1/";

fn api_key_or_skip() -> Option<String> {
    let key = std::env::var("ANTHROPIC_API_KEY").ok()?;
    if key.trim().is_empty() {
        return None;
    }
    Some(key)
}

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(200)
        .with_top_p(0.9)
}

#[tokio::test]
#[ignore = "real API call; set ANTHROPIC_API_KEY to run"]
async fn smoke_claude_haiku_chat() {
    let Some(api_key) = api_key_or_skip() else {
        panic!("ANTHROPIC_API_KEY not set");
    };
    let backend = AiBackend::remote(api_key, String::from(BASE_URL), String::from("claude-3-5-haiku-latest"));

    let res = chat_completion(
        &backend,
        "You answer in exactly one short sentence.",
        "Say the word 'pong'.",
        &opts(),
    )
    .await
    .expect("real Anthropic call should succeed");

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "claude-3-5-haiku → {res}");
}

#[tokio::test]
#[ignore = "real API call; set ANTHROPIC_API_KEY to run"]
async fn smoke_claude_haiku_stream() {
    let Some(api_key) = api_key_or_skip() else {
        panic!("ANTHROPIC_API_KEY not set");
    };
    let backend = AiBackend::remote(api_key, String::from(BASE_URL), String::from("claude-3-5-haiku-latest"));

    let mut stream = chat_completion_stream(
        &backend,
        "You answer in exactly one short sentence.",
        "Say the word 'pong'.",
        &opts(),
    )
    .await
    .expect("stream open");

    let mut text = String::new();
    let mut chunks = 0;
    while let Some(item) = stream.next().await {
        let chunk = item.expect("chunk ok");
        text.push_str(&chunk);
        chunks += 1;
    }

    assert!(!text.trim().is_empty(), "expected non-empty assembled text");
    assert!(chunks > 0, "expected at least one chunk");
    log::info!(target: "ai_smoke", "claude-3-5-haiku stream → {chunks} chunks, total: {text}");
}
