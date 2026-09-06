//! Real-API smoke test against Fireworks AI, the second OpenAI-compatible host we gate on.
//!
//! Why a second one next to Groq: its model ids carry an account path
//! (`accounts/fireworks/models/…`). That shape has to survive `remote_model_iden`'s `openai::`
//! namespacing and `genai`'s `Url::join` intact, and a bare Groq id never exercises it. It
//! also means a Groq-side outage doesn't leave us with zero live OpenAI-compatible coverage.
//!
//! `#[ignore]`-gated: needs a valid `FIREWORKS_AI_API_KEY`. The `fireworks-smoke` check
//! resolves the key (env var, else the sops `secret` helper) and skips cleanly without one.
//!
//! Endpoint and model id live in `smoke_providers.rs`, not here.
//!
//! Run manually:
//! ```sh
//! FIREWORKS_AI_API_KEY=$(secret FIREWORKS_AI_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_fireworks_test
//! ```

use futures_util::StreamExt;
use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion_stream, chat_completion_with_empty_retry};
use super::smoke_providers::{FIREWORKS, api_key, expect_ok};

fn backend() -> AiBackend {
    AiBackend::remote(
        api_key(&FIREWORKS),
        FIREWORKS.base_url.to_string(),
        FIREWORKS.model.to_string(),
    )
}

/// 200 tokens for the same reason as Groq: the model reasons before it answers.
fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(200)
        .with_top_p(0.9)
}

#[tokio::test]
#[ignore = "real API call: set FIREWORKS_AI_API_KEY to run"]
async fn smoke_fireworks_account_path_model_completion() {
    let response = expect_ok(
        &FIREWORKS,
        FIREWORKS.model,
        chat_completion_with_empty_retry(
            &backend(),
            "You answer in exactly one short sentence.",
            "Say the word 'pong'.",
            &opts(),
        )
        .await,
    );

    assert!(
        !response.trim().is_empty(),
        "Fireworks returned an empty completion: {response:?}"
    );
    log::info!(target: "ai_smoke", "{} → {response}", FIREWORKS.model);
}

#[tokio::test]
#[ignore = "real API call: set FIREWORKS_AI_API_KEY to run"]
async fn smoke_fireworks_stream() {
    let backend = backend();
    let mut stream = expect_ok(
        &FIREWORKS,
        FIREWORKS.model,
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
        text.push_str(&expect_ok(&FIREWORKS, FIREWORKS.model, item));
        chunks += 1;
    }

    assert!(chunks > 0, "expected at least one chunk");
    log::info!(
        target: "ai_smoke",
        "{} stream → {}, total: {text}",
        FIREWORKS.model,
        crate::pluralize::pluralize(chunks, "chunk")
    );
}
