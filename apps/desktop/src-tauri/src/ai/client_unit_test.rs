//! Unit tests for the pure helpers in `client.rs`: adapter routing, HTTP-status error
//! classification, the retry budget, and the provider-error detail string.
//!
//! A `#[path]` CHILD of `client`, not a sibling module, so these keep reaching its private
//! functions without widening any of them to `pub(super)` for the sake of test layout.

use super::*;

#[test]
fn test_ai_error_display() {
    assert_eq!(AiError::Unavailable.to_string(), "AI server unavailable");
    assert_eq!(AiError::Timeout.to_string(), "AI request timed out");
    assert_eq!(AiError::EmptyResponse.to_string(), "AI returned no text");
    assert_eq!(
        AiError::ServerError(String::from("bad")).to_string(),
        "AI server error: bad"
    );
    assert_eq!(
        AiError::ParseError(String::from("oops")).to_string(),
        "AI response parse error: oops"
    );
    assert_eq!(
        AiError::NotFound(String::from("HTTP 404: no such model")).to_string(),
        "AI provider has no such model or endpoint: HTTP 404: no such model"
    );
}

#[test]
fn remote_model_iden_forces_openai_for_compatible_providers() {
    // Native protocols + real OpenAI families: left untouched.
    for m in [
        "claude-sonnet-4-5",
        "gemini-2.5-flash",
        "gpt-4.1-mini",
        "gpt-5.5",
        "o3-mini",
        "chatgpt-4o-latest",
    ] {
        assert_eq!(remote_model_iden(m), m, "{m} should keep its inferred adapter");
    }
    // OpenAI-compatible BYOK models genai would mis-route to Ollama: forced to OpenAI.
    assert_eq!(remote_model_iden("deepseek-chat"), "openai::deepseek-chat");
    assert_eq!(
        remote_model_iden("google/gemma-4-31b-it:free"),
        "openai::google/gemma-4-31b-it:free"
    );
    assert_eq!(
        remote_model_iden("mistral-small-latest"),
        "openai::mistral-small-latest"
    );
}

/// The models the real-API smoke lanes call must land on the adapter their test expects,
/// or a lane goes green against the wrong protocol. Reads the ids from
/// `ai::smoke_providers` so a decommission stays a one-line fix over there.
#[test]
fn smoke_provider_models_route_to_their_intended_adapters() {
    use crate::ai::smoke_providers as sp;

    // Anthropic keeps its native protocol; anything else would test the wrong wire format.
    assert_eq!(remote_model_iden(sp::ANTHROPIC.model), sp::ANTHROPIC.model);
    // OpenAI proper is left alone so genai can auto-route Responses vs chat-completions.
    for model in [
        sp::OPENAI.model,
        sp::OPENAI_RESPONSES_MODEL,
        sp::OPENAI_CHAT_REASONING_MODEL,
    ] {
        assert_eq!(
            remote_model_iden(model),
            model,
            "{model} should keep its inferred adapter"
        );
    }
    // The OpenAI-compatible hosts must be forced onto `openai::`, slashed ids and all.
    for provider in [&sp::GROQ, &sp::FIREWORKS] {
        assert_eq!(
            remote_model_iden(provider.model),
            format!("openai::{}", provider.model),
            "{} would fall back to the Ollama adapter",
            provider.name
        );
    }
}

/// Only `gpt-5*` reaches the Responses API in `genai 0.6.5`, and only `o1`/`o3`/`o4`/
/// `chatgpt-` reach the chat-completions reasoning branch. The two OpenAI smoke models
/// exist to cover one branch each, so a careless swap that collapses them onto the same
/// adapter should fail here rather than quietly halve the coverage.
#[test]
fn the_two_openai_smoke_models_cover_different_branches() {
    use crate::ai::smoke_providers as sp;

    assert!(
        sp::OPENAI_RESPONSES_MODEL.starts_with("gpt-5"),
        "the Responses-API smoke model needs the `gpt-5` prefix genai routes on"
    );
    assert!(
        is_openai_chat_reasoning_model(sp::OPENAI_CHAT_REASONING_MODEL),
        "the chat-completions reasoning smoke model must match the heuristic it guards"
    );
    assert!(
        !is_openai_chat_reasoning_model(sp::OPENAI.model),
        "the plain chat-completions smoke model must NOT be reasoning-class, or it stops \
         proving that temperature survives for ordinary models"
    );
}

#[test]
fn with_bumped_max_tokens_multiplies_and_caps() {
    let base = ChatOptions::default().with_max_tokens(300);
    assert_eq!(with_bumped_max_tokens(&base, 4, 2000).max_tokens, Some(1200));
    // Caps at the ceiling.
    assert_eq!(with_bumped_max_tokens(&base, 100, 2000).max_tokens, Some(2000));
    // Saturating multiply can't overflow into a tiny value.
    let huge = ChatOptions::default().with_max_tokens(u32::MAX);
    assert_eq!(with_bumped_max_tokens(&huge, 4, 2000).max_tokens, Some(2000));
    // No prior cap → jump straight to the ceiling on retry.
    assert_eq!(
        with_bumped_max_tokens(&ChatOptions::default(), 4, 2000).max_tokens,
        Some(2000)
    );
}

#[test]
fn ai_error_for_status_classifies_by_code() {
    assert!(matches!(ai_error_for_status(401, "x".into()), AiError::AuthFailed(_)));
    assert!(matches!(ai_error_for_status(403, "x".into()), AiError::AuthFailed(_)));
    // 429 is both rate-limiting and OpenAI's `insufficient_quota`.
    assert!(matches!(ai_error_for_status(429, "x".into()), AiError::RateLimited(_)));
    assert!(matches!(ai_error_for_status(500, "x".into()), AiError::ServerError(_)));
    // 404: a model id the provider doesn't serve (decommissioned, or a typo), or a base
    // URL with the wrong path. The `<provider>-smoke` lanes branch on this variant.
    assert!(matches!(ai_error_for_status(404, "x".into()), AiError::NotFound(_)));
}

/// A streaming failure reaches us wrapped twice (`WebStream` holding a boxed `HttpError`),
/// and until that shape was unwrapped every one of them fell to the catch-all: a rejected
/// key showed as a generic server error on the very path the agent and folder suggestions
/// use. Classification must match the non-streaming path exactly.
#[test]
fn a_streaming_http_error_classifies_by_status_like_any_other() {
    fn stream_error(status: u16, body: &str) -> genai::Error {
        // `genai::Error::HttpError` carries a `reqwest::StatusCode`; we depend on the same
        // `reqwest` major, so this is the same type.
        let status = reqwest::StatusCode::from_u16(status).expect("valid status");
        genai::Error::WebStream {
            model_iden: ModelIden::new(AdapterKind::OpenAI, "some-model"),
            cause: String::from("HTTP error"),
            error: Box::new(genai::Error::HttpError {
                status,
                canonical_reason: status.canonical_reason().unwrap_or("Unknown").to_string(),
                body: body.to_string(),
            }),
        }
    }

    assert!(matches!(
        map_genai_error(stream_error(404, r#"{"error":{"message":"no such model"}}"#)),
        AiError::NotFound(_)
    ));
    assert!(matches!(
        map_genai_error(stream_error(401, "nope")),
        AiError::AuthFailed(_)
    ));
    assert!(matches!(
        map_genai_error(stream_error(429, "slow down")),
        AiError::RateLimited(_)
    ));
    assert!(matches!(
        map_genai_error(stream_error(500, "boom")),
        AiError::ServerError(_)
    ));

    // The provider's own sentence rides along for display, same as the non-streaming path.
    let AiError::NotFound(detail) = map_genai_error(stream_error(404, r#"{"error":{"message":"gone"}}"#)) else {
        panic!("404 should classify as NotFound");
    };
    assert_eq!(detail, "HTTP 404 Not Found: gone");
}

#[test]
fn provider_error_detail_extracts_the_json_error_message() {
    // OpenAI, OpenRouter, Anthropic, and Gemini all put the human sentence at
    // `error.message`; the rest of the body is noise for a user.
    let body = r#"{"error":{"message":"This model is unavailable for free.","code":404},"user_id":"u1"}"#;
    assert_eq!(
        provider_error_detail("404 Not Found", body),
        "HTTP 404 Not Found: This model is unavailable for free."
    );
}

#[test]
fn provider_error_detail_falls_back_to_the_raw_body() {
    assert_eq!(
        provider_error_detail("502 Bad Gateway", "upstream exploded"),
        "HTTP 502 Bad Gateway: upstream exploded"
    );
    // JSON without the well-known shape also falls back whole.
    assert_eq!(
        provider_error_detail("500", r#"{"oops":true}"#),
        r#"HTTP 500: {"oops":true}"#
    );
}

#[test]
fn provider_error_detail_truncates_a_huge_body() {
    // An HTML error page (a proxy, Cloudflare) must not flood the UI or the logs.
    let body = "x".repeat(5000);
    let detail = provider_error_detail("500", &body);
    assert!(detail.chars().count() < 450, "got {} chars", detail.chars().count());
    assert!(detail.ends_with('…'));
}

#[test]
fn test_is_openai_chat_reasoning_model() {
    assert!(is_openai_chat_reasoning_model("o1"));
    assert!(is_openai_chat_reasoning_model("o1-mini"));
    assert!(is_openai_chat_reasoning_model("o3-pro"));
    assert!(is_openai_chat_reasoning_model("o4-mini"));
    assert!(is_openai_chat_reasoning_model("chatgpt-4o-latest"));
    assert!(is_openai_chat_reasoning_model("gpt-5"), "defense-in-depth");
    assert!(is_openai_chat_reasoning_model("gpt-5.5"), "defense-in-depth");

    assert!(!is_openai_chat_reasoning_model("gpt-4o-mini"));
    assert!(!is_openai_chat_reasoning_model("gpt-4.1"));
    assert!(!is_openai_chat_reasoning_model("local-model"));
}
