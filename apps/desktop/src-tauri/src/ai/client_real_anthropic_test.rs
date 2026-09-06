//! Real-API smoke tests against Anthropic.
//!
//! Why a separate file from `client_real_openai_test.rs`: Anthropic's native streaming
//! protocol differs from OpenAI's SSE shape (event names like `content_block_delta` vs
//! `data:` JSON envelopes). Without exercising it, we'd be testing only the OpenAI lineage
//! despite supporting Anthropic via `genai`.
//!
//! `#[ignore]`-gated: needs a valid `ANTHROPIC_API_KEY`. The `anthropic-smoke` check resolves
//! the key (env var, else the sops `secret` helper) and skips cleanly without one.
//!
//! Endpoint and model id live in `smoke_providers.rs`, not here — including why the pin stays
//! on Haiku 4.5 rather than a newer Claude.
//!
//! Costs ~$0.001 per full run.
//!
//! Run manually:
//! ```sh
//! ANTHROPIC_API_KEY=$(secret ANTHROPIC_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_anthropic_test
//! ```

use futures_util::StreamExt;
use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion, chat_completion_stream};
use super::smoke_providers::{ANTHROPIC, api_key, expect_ok};

fn backend() -> AiBackend {
    AiBackend::remote(
        api_key(&ANTHROPIC),
        ANTHROPIC.base_url.to_string(),
        ANTHROPIC.model.to_string(),
    )
}

fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(200)
        .with_top_p(0.9)
}

#[tokio::test]
#[ignore = "real API call; set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_chat() {
    let res = expect_ok(
        &ANTHROPIC,
        ANTHROPIC.model,
        chat_completion(
            &backend(),
            "You answer in exactly one short sentence.",
            "Say the word 'pong'.",
            &opts(),
        )
        .await,
    );

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "{} → {res}", ANTHROPIC.model);
}

#[tokio::test]
#[ignore = "real API call; set ANTHROPIC_API_KEY to run"]
async fn smoke_anthropic_stream() {
    let backend = backend();
    let mut stream = expect_ok(
        &ANTHROPIC,
        ANTHROPIC.model,
        chat_completion_stream(
            &backend,
            "You answer in exactly one short sentence.",
            "Say the word 'pong'.",
            &opts(),
        )
        .await,
    );

    let mut text = String::new();
    let mut chunks: u64 = 0;
    while let Some(item) = stream.next().await {
        text.push_str(&expect_ok(&ANTHROPIC, ANTHROPIC.model, item));
        chunks += 1;
    }

    assert!(!text.trim().is_empty(), "expected non-empty assembled text");
    assert!(chunks > 0, "expected at least one chunk");
    log::info!(
        target: "ai_smoke",
        "{} stream → {}, total: {text}",
        ANTHROPIC.model,
        crate::pluralize::pluralize(chunks, "chunk")
    );
}
