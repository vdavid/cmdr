//! Real-API smoke tests against OpenAI.
//!
//! Unlike the other provider smokes, this one isn't a ping: OpenAI is where our
//! `adjust_for_model` fixups earn their keep, and each test below covers a different branch.
//! Flattening them into one call would drop the coverage that motivated the file.
//!
//! - plain chat-completions: `temperature`/`top_p` must survive.
//! - Responses API (`gpt-5*`): `genai` routes it to `/v1/responses`, where a custom
//!   `temperature` is a 400 unless we strip it and substitute a reasoning effort.
//! - chat-completions reasoning (`o1`/`o3`/`o4`/`chatgpt-*`): stays on
//!   `/v1/chat/completions` but rejects `temperature` all the same, which is exactly what
//!   `is_openai_chat_reasoning_model` exists to catch.
//!
//! `#[ignore]`-gated: needs a valid `OPENAI_API_KEY`. The `openai-smoke` check resolves the
//! key (env var, else the sops `secret` helper) and skips cleanly without one.
//!
//! Endpoint and the three model ids live in `smoke_providers.rs`, not here — including which
//! prefix each one has to keep for its branch to stay covered.
//!
//! These tests cost real money (a few cents per full run).
//!
//! Run manually:
//! ```sh
//! OPENAI_API_KEY=$(secret OPENAI_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_openai_test
//! ```

use futures_util::StreamExt;
use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion, chat_completion_stream};
use super::smoke_providers::{OPENAI, OPENAI_CHAT_REASONING_MODEL, OPENAI_RESPONSES_MODEL, api_key, expect_ok};

fn backend(model: &str) -> AiBackend {
    AiBackend::remote(api_key(&OPENAI), OPENAI.base_url.to_string(), model.to_string())
}

fn opts() -> ChatOptions {
    // 200 tokens, not 40: reasoning models consume the budget for thinking before
    // emitting any output_text. With Low effort and a short answer, 40 was sometimes
    // too tight and the model returned only reasoning, no text.
    ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(200)
        .with_top_p(0.9)
}

async fn say_pong(model: &str) -> String {
    expect_ok(
        &OPENAI,
        model,
        chat_completion(
            &backend(model),
            "You answer in exactly one short sentence.",
            "Say the word 'pong'.",
            &opts(),
        )
        .await,
    )
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_chat_completions() {
    let res = say_pong(OPENAI.model).await;
    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "{} → {res}", OPENAI.model);
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_responses_api_routing() {
    // `gpt-5*` should route through `/v1/responses` and use reasoning_effort instead
    // of temperature. If our adjust_for_model is wrong, OpenAI returns HTTP 400.
    let res = say_pong(OPENAI_RESPONSES_MODEL).await;
    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "{OPENAI_RESPONSES_MODEL} → {res}");
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_chat_reasoning_model_omits_temperature() {
    // Stays on /v1/chat/completions but rejects a custom temperature. Our
    // is_openai_chat_reasoning_model heuristic must catch this.
    let res = expect_ok(
        &OPENAI,
        OPENAI_CHAT_REASONING_MODEL,
        chat_completion(
            &backend(OPENAI_CHAT_REASONING_MODEL),
            "Answer in one short sentence.",
            "What is 2+2?",
            &opts(),
        )
        .await,
    );
    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "{OPENAI_CHAT_REASONING_MODEL} → {res}");
}

// --- Streaming smokes ---

async fn collect_stream(model: &str) -> String {
    let backend = backend(model);
    let mut stream = expect_ok(
        &OPENAI,
        model,
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
        text.push_str(&expect_ok(&OPENAI, model, item));
        chunks += 1;
    }
    log::info!(
        target: "ai_smoke",
        "{model} stream → {}, total: {text}",
        crate::pluralize::pluralize(chunks, "chunk")
    );
    text
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_chat_completions_stream() {
    let text = collect_stream(OPENAI.model).await;
    assert!(!text.trim().is_empty(), "expected non-empty assembled text");
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_responses_api_stream() {
    // Routes through the Responses API. Reasoning may eat the budget, so the assertion is
    // "the stream completed without error", not "non-empty": a reasoning model can
    // legitimately return zero output_text chunks when the budget is tight.
    let _text = collect_stream(OPENAI_RESPONSES_MODEL).await;
}

#[tokio::test]
#[ignore = "real API call: set OPENAI_API_KEY to run"]
async fn smoke_openai_chat_reasoning_model_stream() {
    // Same caveat as the Responses stream above.
    let _text = collect_stream(OPENAI_CHAT_REASONING_MODEL).await;
}
