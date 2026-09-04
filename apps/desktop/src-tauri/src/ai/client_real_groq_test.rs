//! Real-API smoke test against Groq (OpenAI-compatible, free tier). The cheapest of the
//! real-provider gates: it exercises OUR `AiBackend::remote` + `chat_completion` code against
//! a live OpenAI-compatible endpoint, so a regression in adapter routing, auth, or response
//! parsing fails here instead of silently in production (the wiremock tests can't catch a
//! real-API contract drift, and they can't catch a provider retiring a model either).
//!
//! `#[ignore]`-gated: needs a valid `GROQ_API_KEY`. The `groq-smoke` check in the Go check
//! runner resolves the key (env var, else the sops `secret` helper) and runs this with
//! `--run-ignored only`, skipping cleanly when no key is available (contributors without a
//! key, CI without the secret).
//!
//! Endpoint and model id live in `smoke_providers.rs`, not here.
//!
//! Run manually:
//! ```sh
//! GROQ_API_KEY=$(secret GROQ_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_groq_test
//! ```

use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion_with_empty_retry};
use super::smoke_providers::{GROQ, api_key, expect_ok};

#[tokio::test]
#[ignore = "real API call: set GROQ_API_KEY to run"]
async fn smoke_groq_translate_shaped_completion() {
    let backend = AiBackend::remote(api_key(&GROQ), GROQ.base_url.to_string(), GROQ.model.to_string());

    // Mirror the translate commands' option shape (temperature + capped tokens + the
    // empty-retry wrapper), so this exercises the same path Search/Selection use. 200 tokens,
    // not 50: every Groq production chat model reasons now, and reasoning eats the budget
    // before any text appears. A bad draw still lands on `chat_completion_with_empty_retry`,
    // which is itself part of what we're testing.
    let options = ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(200)
        .with_top_p(0.9);

    let system = "You output one line in the form `keyword: value`. No prose.";
    let user = "files named report from last week";

    let response = expect_ok(
        &GROQ,
        GROQ.model,
        chat_completion_with_empty_retry(&backend, system, user, &options).await,
    );

    assert!(
        !response.trim().is_empty(),
        "Groq returned an empty completion: {response:?}"
    );
}
