//! Real-API smoke tests against Google Gemini.
//!
//! Why a file of its own: Gemini is the second NATIVE protocol we ship. `remote_model_iden`
//! leaves `gemini-*` un-namespaced, so `genai` routes it to its Gemini adapter, which POSTs
//! `{base}models/{model}:generateContent` (and `:streamGenerateContent?alt=sse` for streams)
//! rather than the `/chat/completions` shape every other lane exercises. `smoke_gemini_routes_to_the_native_adapter`
//! pins that, so a routing regression fails here rather than in a user's app.
//!
//! ## The free tier is genuinely flaky, and the lane has to survive it
//!
//! Within a few minutes on 2026-09-04 the identical request answered: HTTP 200 with real
//! text, then `503 UNAVAILABLE` ("experiencing high demand"), then HTTP 404 with a
//! **completely empty body**, then 503s for several minutes. So a plain `expect_ok` here
//! would cry "decommissioned!" at an outage, and the alarm would be worth nothing.
//!
//! Two failures have to stay apart, and the discriminator is STRUCTURAL, never the sentence
//! (`AGENTS.md`'s no-string-matching rule, enforced by `error-string-match`):
//!
//! - **The model is gone** — the event this lane exists to catch. Google answers 404 and
//!   fills in its JSON error envelope (`{"error": {"code", "message", "status"}}`), because it
//!   routed the request and has something to say about the model. That's a hard failure
//!   through `expect_ok`, naming the constant to edit. (verified on 2026-09-04: the withdrawn
//!   `gemini-2.5-flash-lite` answers exactly that, three times running, pointing at
//!   `gemini-3.5-flash-lite`.)
//! - **Google didn't route the request** — 404 with a zero-byte body, or 503, or a hung
//!   connection. Nothing is proven about our pin, so the lane retries with backoff and, if it
//!   still can't get through, reports itself INCONCLUSIVE: a lane WARN, never a green pass.
//!   See `smoke_providers::report_inconclusive`.
//!
//! ❗ A bodyless 404 also covers "Google doesn't serve this PATH" — the wrong `base_url`
//! (verified 2026-09-04: `/v1beta/openai/models/…:generateContent` and a version-less
//! `/models/…:generateContent` both answer it while `v1` is healthy). Both suspects live in
//! the same `GEMINI` constant, so a lane that stays inconclusive night after night means
//! "read that constant", not "wait it out".
//!
//! `#[ignore]`-gated: needs a valid `GEMINI_API_KEY`. The `gemini-smoke` check resolves the
//! key (env var, else the sops `secret` helper) and skips cleanly without one.
//!
//! Endpoint and model id live in `smoke_providers.rs`, not here.
//!
//! Free tier, so a full run costs nothing but Google's patience.
//!
//! Run manually:
//! ```sh
//! GEMINI_API_KEY=$(secret GEMINI_API_KEY) \
//!   cargo nextest run --lib --run-ignored only ai::client_real_gemini_test
//! ```

use std::future::Future;
use std::time::Duration;

use futures_util::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::ChatOptions;
use serde_json::{Value, json};

use super::client::{AiBackend, AiError, chat_completion, chat_completion_stream};
use super::smoke_providers::{GEMINI, api_key, expect_ok, report_inconclusive};

fn backend() -> AiBackend {
    AiBackend::remote(api_key(&GEMINI), GEMINI.base_url.to_string(), GEMINI.model.to_string())
}

/// Every Gemini model reasons before it answers, and a tight budget comes back as pure
/// reasoning with no text at all. 300 tokens leaves room for both halves.
fn opts() -> ChatOptions {
    ChatOptions::default()
        .with_temperature(0.3)
        .with_max_tokens(300)
        .with_top_p(0.9)
}

/// Waits between attempts. Four attempts in total, so ~35 s of waiting in the worst case —
/// comfortably inside the 150 s cap `.config/nextest.toml` grants this module, and long
/// enough to ride out the 503 bursts observed on 2026-09-04.
const BACKOFF: [Duration; 3] = [Duration::from_secs(5), Duration::from_secs(10), Duration::from_secs(20)];

/// Runs one smoke call until it either answers or the provider proves it can't, retrying the
/// unproven cases.
///
/// Returns `Some(value)` when Gemini answered, `None` when every attempt failed for reasons
/// that say nothing about our pin (the run is then recorded as inconclusive, and the caller
/// skips its assertions). It never returns for a genuine decommission: `expect_ok` panics
/// with the message naming `GEMINI` and the model-list URL.
async fn until_answered<T, F, Fut>(mut call: F) -> Option<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, AiError>>,
{
    let mut last_error = None;
    for attempt in 0..=BACKOFF.len() {
        match call().await {
            Ok(value) => return Some(value),
            Err(err) => {
                if model_is_gone(&err).await {
                    // Diverges. Routed through the shared helper so this lane's decommission
                    // message reads identically to every other provider's.
                    return Some(expect_ok(&GEMINI, GEMINI.model, Err(err)));
                }
                log::warn!(
                    target: "ai_smoke",
                    "{} attempt {} didn't get through: {err}",
                    GEMINI.name,
                    attempt + 1
                );
                last_error = Some(err);
            }
        }
        if let Some(pause) = BACKOFF.get(attempt) {
            // allowed-test-sleep: the wait IS the subject. There's no condition to poll — the
            // point is to give an overloaded provider time to recover before asking again.
            tokio::time::sleep(*pause).await;
        }
    }

    let attempts = crate::pluralize::pluralize(BACKOFF.len() as u64 + 1, "attempt");
    let detail = last_error.map_or_else(|| String::from("no error recorded"), |err| err.to_string());
    report_inconclusive(
        &GEMINI,
        &format!(
            "no usable answer from `{model}` in {attempts}, and Google never said the model is gone. \
             Last: {detail}. Either the free tier is overloaded (it flaps between 200, 503, and a bodyless 404 \
             within minutes) or `GEMINI.base_url` names a path Google doesn't route. If this repeats nightly, \
             read the `GEMINI` block in apps/desktop/src-tauri/src/ai/smoke_providers.rs.",
            model = GEMINI.model,
        ),
    );
    None
}

/// Asks Gemini whether it has anything to SAY about this model, and answers from the shape of
/// the reply rather than its wording.
///
/// Only a 404 is ambiguous — a 503, a 429, or a transport failure already carries its own
/// typed [`AiError`] variant and proves nothing about the pin — so this probe runs on that one
/// variant and skips the extra request otherwise.
///
/// The tell is whether Google filled in its error envelope. It routed the request and
/// deliberately answered "this model is withdrawn" ⇒ a JSON object with an `error` member. It
/// never routed the request at all (overload, or a path it doesn't serve) ⇒ zero bytes.
/// Reading for the presence of that member is a structural check on a documented envelope, not
/// a match on the human sentence inside it.
///
/// Anything the probe itself can't complete counts as "not proven gone": a smoke that can't
/// reach Google has no business declaring a model dead.
async fn model_is_gone(err: &AiError) -> bool {
    if !matches!(err, AiError::NotFound(_)) {
        return false;
    }

    let Ok(key) = std::env::var(GEMINI.env_var) else {
        return false;
    };
    let Ok(client) = reqwest::Client::builder().timeout(Duration::from_secs(30)).build() else {
        return false;
    };

    let url = format!(
        "{base}models/{model}:generateContent",
        base = GEMINI.base_url,
        model = GEMINI.model
    );
    let body = json!({
        "contents": [{ "role": "user", "parts": [{ "text": "Say the word 'pong'." }] }],
        "generationConfig": { "maxOutputTokens": 300 },
    });
    let Ok(response) = client.post(&url).header("x-goog-api-key", key).json(&body).send().await else {
        return false;
    };
    if response.status() != reqwest::StatusCode::NOT_FOUND {
        return false;
    }

    let Ok(text) = response.text().await else {
        return false;
    };
    let envelope = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|value| value.get("error").cloned());
    if let Some(envelope) = envelope {
        log::warn!(target: "ai_smoke", "{} says `{}` is gone: {envelope}", GEMINI.name, GEMINI.model);
        return true;
    }
    false
}

/// The lane is only worth running if it goes through Gemini's OWN protocol. A stray
/// `openai::` namespace, or a `genai` release that stops recognizing the prefix, would leave
/// every test below green against the wrong wire format.
///
/// Costs no network call: `resolve_adapter` runs the resolver alone.
/// `client_unit_test::smoke_provider_models_route_to_their_intended_adapters` guards the same
/// invariant without a key, so an ordinary test run catches it too.
#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_routes_to_the_native_adapter() {
    let adapter = backend().resolve_adapter().await;
    assert!(
        matches!(adapter, Ok(AdapterKind::Gemini)),
        "`{}` must reach Gemini's native adapter, got {adapter:?}",
        GEMINI.model
    );
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_chat() {
    let backend = backend();
    let opts = opts();
    let Some(res) = until_answered(|| {
        chat_completion(
            &backend,
            "You answer in exactly one short sentence.",
            "Say the word 'pong'.",
            &opts,
        )
    })
    .await
    else {
        return;
    };

    assert!(!res.trim().is_empty(), "response should be non-empty");
    log::info!(target: "ai_smoke", "{} → {res}", GEMINI.model);
}

/// Opens a stream AND drains it, as ONE retryable unit.
///
/// ❗ Draining has to be inside the retried closure, not after it. `chat_completion_stream`
/// hands back an `Ok` stream before the response status is known, so Gemini's 404 arrives as a
/// failing chunk — and a chunk unwrapped outside this function skips the triage entirely and
/// reports every outage as a decommission. That's not hypothetical: an earlier shape of this
/// test did exactly that against a deliberately wrong `base_url`.
async fn stream_one(backend: &AiBackend, options: &ChatOptions) -> Result<(String, u64), AiError> {
    let mut stream = chat_completion_stream(
        backend,
        "You answer in exactly one short sentence.",
        "Say the word 'pong'.",
        options,
    )
    .await?;

    let mut text = String::new();
    let mut chunks: u64 = 0;
    while let Some(item) = stream.next().await {
        text.push_str(&item?);
        chunks += 1;
    }
    Ok((text, chunks))
}

#[tokio::test]
#[ignore = "real API call: set GEMINI_API_KEY to run"]
async fn smoke_gemini_stream() {
    let backend = backend();
    let opts = opts();
    // Every Gemini response carries a `thoughtSignature` part beside the text one, so this
    // also proves `chat_completion_stream` keeps filtering those out of the visible chunks.
    let Some((text, chunks)) = until_answered(|| stream_one(&backend, &opts)).await else {
        return;
    };

    assert!(!text.trim().is_empty(), "expected non-empty assembled text");
    assert!(chunks > 0, "expected at least one chunk");
    log::info!(
        target: "ai_smoke",
        "{} stream → {}, total: {text}",
        GEMINI.model,
        crate::pluralize::pluralize(chunks, "chunk")
    );
}
