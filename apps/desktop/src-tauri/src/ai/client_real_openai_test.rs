//! Real-API smoke tests against OpenAI. **Not run in CI** — these are gated behind
//! the `#[ignore]` attribute and need a valid `OPENAI_API_KEY` env var.
//!
//! Run with:
//! ```sh
//! OPENAI_API_KEY=$(security find-generic-password -a "$USER" -s "OPENAI_API_KEY" -w) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_openai_test
//! ```
//!
//! These tests cost real money (a few cents per full run) and depend on OpenAI's
//! endpoints being up. They're for one-time confidence after refactors, not for
//! regression detection.

use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion};

const BASE_URL: &str = "https://api.openai.com/v1/";

fn api_key_or_skip() -> Option<String> {
    let key = std::env::var("OPENAI_API_KEY").ok()?;
    if key.trim().is_empty() {
        return None;
    }
    Some(key)
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

#[tokio::test]
#[ignore = "real API call — set OPENAI_API_KEY to run"]
async fn smoke_gpt_4o_mini_chat_completions() {
    let Some(api_key) = api_key_or_skip() else {
        panic!("OPENAI_API_KEY not set");
    };

    let backend = AiBackend::remote(api_key, String::from(BASE_URL), String::from("gpt-4o-mini"));

    let res = chat_completion(
        &backend,
        "You answer in exactly one short sentence.",
        "Say the word 'pong'.",
        &opts(),
    )
    .await
    .expect("real OpenAI call should succeed");

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke","gpt-4o-mini → {res}");
}

#[tokio::test]
#[ignore = "real API call — set OPENAI_API_KEY to run"]
async fn smoke_gpt_5_routes_through_responses_api() {
    // gpt-5* should route through `/v1/responses` and use reasoning_effort instead
    // of temperature. If our adjust_for_model is wrong, OpenAI returns HTTP 400.
    let Some(api_key) = api_key_or_skip() else {
        panic!("OPENAI_API_KEY not set");
    };

    let backend = AiBackend::remote(api_key, String::from(BASE_URL), String::from("gpt-5-mini"));

    let res = chat_completion(
        &backend,
        "You answer in exactly one short sentence.",
        "Say the word 'pong'.",
        &opts(),
    )
    .await
    .expect("gpt-5 should not 400 on temperature (we strip it)");

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke","gpt-5-mini → {res}");
}

#[tokio::test]
#[ignore = "real API call — set OPENAI_API_KEY to run"]
async fn smoke_o3_mini_omits_temperature() {
    // o3-mini stays on /v1/chat/completions but rejects custom temperature. Our
    // is_openai_chat_reasoning_model heuristic must catch this.
    let Some(api_key) = api_key_or_skip() else {
        panic!("OPENAI_API_KEY not set");
    };

    let backend = AiBackend::remote(api_key, String::from(BASE_URL), String::from("o3-mini"));

    let res = chat_completion(&backend, "Answer in one short sentence.", "What is 2+2?", &opts())
        .await
        .expect("o3-mini should not 400 on temperature (we strip it)");

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke","o3-mini → {res}");
}
