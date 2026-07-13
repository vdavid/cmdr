//! Real-API smoke test against Groq (OpenAI-compatible, free tier). This is the cheap
//! always-available real-provider gate for the AI translate pipeline: it exercises OUR
//! `AiBackend::remote` + `chat_completion` code against a live OpenAI-compatible endpoint, so a
//! regression in adapter routing, auth, or response parsing fails here instead of silently in
//! production (the wiremock tests can't catch a real-API contract drift).
//!
//! `#[ignore]`-gated: needs a valid `GROQ_API_KEY`. The `groq-smoke` check in the Go check runner
//! resolves the key (env var, else the sops `secret` helper) and runs this with `--run-ignored only`,
//! skipping cleanly when no key is available (contributors without a key, CI without the secret).
//!
//! Run manually:
//! ```sh
//! GROQ_API_KEY=$(secret GROQ_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_groq_test
//! ```

use genai::chat::ChatOptions;

use super::client::{AiBackend, chat_completion_with_empty_retry};

/// Groq's OpenAI-compatible base. Trailing slash required (see the `genai` `Url::join` gotcha).
const BASE_URL: &str = "https://api.groq.com/openai/v1/";
/// Smallest/fastest Groq model — cheapest smoke, non-reasoning so a tight budget is safe.
const MODEL: &str = "llama-3.1-8b-instant";

fn api_key_or_skip() -> Option<String> {
    let key = std::env::var("GROQ_API_KEY").ok()?;
    if key.trim().is_empty() {
        return None;
    }
    Some(key)
}

#[tokio::test]
#[ignore = "real API call: set GROQ_API_KEY to run"]
async fn smoke_groq_translate_shaped_completion() {
    let Some(api_key) = api_key_or_skip() else {
        panic!("GROQ_API_KEY not set");
    };

    let backend = AiBackend::remote(api_key, BASE_URL.to_string(), MODEL.to_string());

    // Mirror the translate commands' option shape (temperature + capped tokens + the empty-retry
    // wrapper), so this exercises the same path Search/Selection use.
    let options = ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(50)
        .with_top_p(0.9);

    let system = "You output one line in the form `keyword: value`. No prose.";
    let user = "files named report from last week";

    let response = chat_completion_with_empty_retry(&backend, system, user, &options)
        .await
        .expect("Groq chat completion should succeed");

    assert!(
        !response.trim().is_empty(),
        "Groq returned an empty completion: {response:?}"
    );
}
