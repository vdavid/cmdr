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
        assert_eq!(remote_model_iden(m, None), m, "{m} should keep its inferred adapter");
    }
    // OpenAI-compatible BYOK models genai would mis-route to Ollama: forced to OpenAI.
    assert_eq!(remote_model_iden("deepseek-chat", None), "openai::deepseek-chat");
    assert_eq!(
        remote_model_iden("google/gemma-4-31b-it:free", None),
        "openai::google/gemma-4-31b-it:free"
    );
    assert_eq!(
        remote_model_iden("mistral-small-latest", None),
        "openai::mistral-small-latest"
    );
}

/// DeepSeek and Mistral speak chat-completions but carry reasoning their own way: DeepSeek
/// toggles thinking with a `thinking` body field and wants `reasoning_content` back, while
/// Mistral 422s on that field and wants a leading `thinking` chunk instead. genai's dedicated
/// adapters know this, so their hosts route there; the model name alone can't tell.
#[test]
fn deepseek_and_mistral_hosts_route_to_their_own_chat_dialects() {
    assert_eq!(
        remote_model_iden("deepseek-v4-pro", Some("api.deepseek.com")),
        "deepseek::deepseek-v4-pro"
    );
    assert_eq!(
        remote_model_iden("magistral-medium-latest", Some("api.mistral.ai")),
        "mistral::magistral-medium-latest"
    );
    // The same model id elsewhere (an aggregator) stays plain chat-completions.
    assert_eq!(
        remote_model_iden("deepseek-v4-pro", Some("api.together.xyz")),
        "openai::deepseek-v4-pro"
    );
    // A native family keeps its adapter on any host.
    assert_eq!(remote_model_iden("gpt-5.5", Some("api.mistral.ai")), "gpt-5.5");
}

/// Reasoning text goes back only to chat-completions hosts verified to take it: Groq 400s on
/// the field, and an unknown host keeps the no-reasoning replay it always had.
#[test]
fn only_verified_hosts_take_chat_reasoning_back() {
    for host in [
        "api.deepseek.com",
        "api.mistral.ai",
        "api.fireworks.ai",
        "openrouter.ai",
    ] {
        assert!(replays_chat_reasoning_for_host(host), "{host}");
    }
    for host in ["api.groq.com", "api.together.xyz", "127.0.0.1"] {
        assert!(!replays_chat_reasoning_for_host(host), "{host}");
    }
}

/// #801: only OpenRouter routing to a Claude model needs the request-level marker. Model ids
/// are OpenRouter's own (`anthropic/…`, and `~anthropic/…` for its "latest" aliases).
#[test]
fn only_openrouter_claude_asks_for_caching_at_the_top_level() {
    for model in [
        "anthropic/claude-sonnet-5.5",
        "anthropic/claude-opus-5",
        "~anthropic/claude-sonnet-latest",
    ] {
        assert!(wants_top_level_cache_marker("openrouter.ai", model), "{model}");
    }
    for (host, model) in [
        ("openrouter.ai", "deepseek/deepseek-v4.1-flash"),
        ("openrouter.ai", "openai/gpt-6.1-sol"),
        ("api.anthropic.com", "claude-sonnet-5-5"),
        ("api.together.xyz", "anthropic/claude-sonnet-5.5"),
    ] {
        assert!(!wants_top_level_cache_marker(host, model), "{host} {model}");
    }
}

/// A Gemini user who picked `models/gemini-…` from the old OpenAI-compatible list keeps a
/// working model once the preset moves to Google's native API.
#[test]
fn a_models_prefixed_gemini_id_routes_natively_as_its_bare_id() {
    assert_eq!(remote_model_iden("models/gemini-3.8-flash", None), "gemini-3.8-flash");
    assert_eq!(
        remote_model_iden("models/text-bison", None),
        "openai::models/text-bison",
        "only a Gemini id is unwrapped"
    );
}

/// The models the real-API smoke lanes call must land on the adapter their test expects,
/// or a lane goes green against the wrong protocol. Reads the ids from
/// `ai::smoke_providers` so a decommission stays a one-line fix over there.
#[test]
fn smoke_provider_models_route_to_their_intended_adapters() {
    use crate::ai::smoke_providers as sp;

    // The two native protocols keep theirs; anything else would test the wrong wire format.
    // Gemini especially: an `openai::` namespace here routes it to chat-completions, and the
    // whole `gemini-smoke` lane would then pass against a path it isn't meant to cover.
    assert_eq!(remote_model_iden(sp::ANTHROPIC.model, None), sp::ANTHROPIC.model);
    assert_eq!(remote_model_iden(sp::GEMINI.model, None), sp::GEMINI.model);
    // OpenAI proper is left alone so genai can auto-route Responses vs chat-completions.
    for model in [
        sp::OPENAI.model,
        sp::OPENAI_RESPONSES_MODEL,
        // No live lane since `o3` retires, but Azure OpenAI, OpenRouter, and proxies keep o-series ids.
        "o3",
    ] {
        assert_eq!(
            remote_model_iden(model, None),
            model,
            "{model} should keep its inferred adapter"
        );
    }
    // The OpenAI-compatible hosts must be forced onto `openai::`, slashed ids and all.
    for provider in [&sp::GROQ, &sp::FIREWORKS] {
        assert_eq!(
            remote_model_iden(provider.model, None),
            format!("openai::{}", provider.model),
            "{} would fall back to the Ollama adapter",
            provider.name
        );
    }
}

/// Only `gpt-5*` reaches the Responses API in `genai 0.6.5`. The two OpenAI smoke models
/// exist to cover one branch each (Responses, and plain chat-completions where `temperature`
/// survives), so a careless swap that collapses them onto the same adapter should fail here
/// rather than quietly halve the coverage.
#[test]
fn the_two_openai_smoke_models_cover_different_branches() {
    use crate::ai::smoke_providers as sp;

    assert!(
        sp::OPENAI_RESPONSES_MODEL.starts_with("gpt-5"),
        "the Responses-API smoke model needs the `gpt-5` prefix genai routes on"
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
    // 413: Groq's free tier answers it to any request past its tokens-per-minute cap.
    assert!(matches!(
        ai_error_for_status(413, "x".into()),
        AiError::RequestTooLarge(_)
    ));
    // 404: a model id the provider doesn't serve (decommissioned, or a typo), or a base
    // URL with the wrong path. The `<provider>-smoke` lanes branch on this variant.
    assert!(matches!(ai_error_for_status(404, "x".into()), AiError::NotFound(_)));
    // 503: a provider overloaded right now (Gemini's `UNAVAILABLE`, "high demand"), and 529:
    // Anthropic's `overloaded_error`. Both are worth a retry, unlike a plain 500.
    assert!(matches!(
        ai_error_for_status(503, "x".into()),
        AiError::Overloaded { retry_after: None, .. }
    ));
    assert!(matches!(
        ai_error_for_status(529, "x".into()),
        AiError::Overloaded { retry_after: None, .. }
    ));
    assert!(matches!(ai_error_for_status(502, "x".into()), AiError::ServerError(_)));
}

/// An overloaded provider's 503 reaches us on BOTH shapes genai uses, and its `Retry-After`
/// rides along, so the agent's retry waits as long as the provider asked. Classified from the
/// status and the header, never from the body's "high demand" wording.
#[test]
fn an_overloaded_provider_is_typed_with_its_retry_after() {
    fn headers(pairs: &[(&'static str, &'static str)]) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, reqwest::header::HeaderValue::from_static(value));
        }
        map
    }
    fn stream_503(pairs: &[(&'static str, &'static str)]) -> genai::Error {
        genai::Error::WebStream {
            model_iden: ModelIden::new(AdapterKind::Gemini, "gemini-3.8-flash"),
            cause: String::from("HTTP error"),
            error: Box::new(genai::Error::HttpError {
                status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
                canonical_reason: "Service Unavailable".to_string(),
                body: r#"{"error":{"code":503,"message":"The model is overloaded.","status":"UNAVAILABLE"}}"#
                    .to_string(),
                headers: Box::new(headers(pairs)),
            }),
        }
    }
    fn one_shot_503(pairs: &[(&'static str, &'static str)]) -> genai::Error {
        genai::Error::WebModelCall {
            model_iden: ModelIden::new(AdapterKind::Gemini, "gemini-3.8-flash"),
            webc_error: genai::webc::Error::ResponseFailedStatus {
                status: reqwest::StatusCode::SERVICE_UNAVAILABLE,
                body: "overloaded".to_string(),
                headers: Box::new(headers(pairs)),
            },
        }
    }

    match map_genai_error(stream_503(&[("retry-after", "7")])) {
        AiError::Overloaded { detail, retry_after } => {
            assert_eq!(retry_after, Some(Duration::from_secs(7)));
            assert!(
                detail.starts_with("HTTP 503"),
                "the detail keeps the provider's line: {detail}"
            );
        }
        other => panic!("expected Overloaded, got {other:?}"),
    }
    assert!(matches!(
        map_genai_error(one_shot_503(&[("retry-after", "3")])),
        AiError::Overloaded { retry_after: Some(d), .. } if d == Duration::from_secs(3)
    ));
    assert!(matches!(
        map_genai_error(stream_503(&[])),
        AiError::Overloaded { retry_after: None, .. }
    ));
    assert!(matches!(
        map_genai_error(stream_503(&[("retry-after", "soon")])),
        AiError::Overloaded { retry_after: None, .. }
    ));
}

/// `Retry-After` is either whole seconds or an HTTP date; a date already past means "now".
#[test]
fn retry_after_reads_seconds_and_http_dates() {
    use std::time::UNIX_EPOCH;
    let now = UNIX_EPOCH + Duration::from_secs(784_111_777); // Sun, 06 Nov 1994 08:49:37 GMT

    assert_eq!(parse_retry_after("12", now), Some(Duration::from_secs(12)));
    assert_eq!(parse_retry_after(" 0 ", now), Some(Duration::ZERO));
    assert_eq!(
        parse_retry_after("Sun, 06 Nov 1994 08:50:07 GMT", now),
        Some(Duration::from_secs(30))
    );
    assert_eq!(
        parse_retry_after("Sun, 06 Nov 1994 08:49:00 GMT", now),
        Some(Duration::ZERO)
    );
    assert_eq!(parse_retry_after("-5", now), None);
    assert_eq!(parse_retry_after("later", now), None);
}

/// A 429 whose request limit is ZERO isn't the provider being busy: the account's plan doesn't
/// include the model at all, so every call gets it and waiting changes nothing. Mistral's free
/// workspaces answer `mistral-small-latest` this way (`x-ratelimit-limit-req-minute: 0`).
/// Classified from the header, never from the body's wording.
#[test]
fn a_429_with_a_zero_request_limit_means_the_plan_excludes_the_model() {
    fn headers(pairs: &[(&'static str, &'static str)]) -> reqwest::header::HeaderMap {
        let mut map = reqwest::header::HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, reqwest::header::HeaderValue::from_static(value));
        }
        map
    }
    fn stream_429(pairs: &[(&'static str, &'static str)]) -> genai::Error {
        genai::Error::WebStream {
            model_iden: ModelIden::new(AdapterKind::OpenAI, "mistral-small-latest"),
            cause: String::from("HTTP error"),
            error: Box::new(genai::Error::HttpError {
                status: reqwest::StatusCode::TOO_MANY_REQUESTS,
                canonical_reason: "Too Many Requests".to_string(),
                body: r#"{"message":"Requests rate limit exceeded"}"#.to_string(),
                headers: Box::new(headers(pairs)),
            }),
        }
    }
    fn one_shot_429(pairs: &[(&'static str, &'static str)]) -> genai::Error {
        genai::Error::WebModelCall {
            model_iden: ModelIden::new(AdapterKind::OpenAI, "mistral-small-latest"),
            webc_error: genai::webc::Error::ResponseFailedStatus {
                status: reqwest::StatusCode::TOO_MANY_REQUESTS,
                body: "rate limited".to_string(),
                headers: Box::new(headers(pairs)),
            },
        }
    }

    // Mistral's own header, on both shapes genai uses.
    let zero = [("x-ratelimit-limit-req-minute", "0")];
    assert!(matches!(map_genai_error(stream_429(&zero)), AiError::ModelNotInPlan(_)));
    assert!(matches!(
        map_genai_error(one_shot_429(&zero)),
        AiError::ModelNotInPlan(_)
    ));
    // The OpenAI-style name other providers send means the same at zero.
    assert!(matches!(
        map_genai_error(one_shot_429(&[("x-ratelimit-limit-requests", "0")])),
        AiError::ModelNotInPlan(_)
    ));

    // A real limit, or no header at all, is an ordinary rate limit or spent quota.
    assert!(matches!(
        map_genai_error(stream_429(&[("x-ratelimit-limit-req-minute", "60")])),
        AiError::RateLimited(_)
    ));
    assert!(matches!(map_genai_error(stream_429(&[])), AiError::RateLimited(_)));
    // A zero REMAINING count is a limit used up, not a limit of zero.
    assert!(matches!(
        map_genai_error(stream_429(&[("x-ratelimit-remaining-req-minute", "0")])),
        AiError::RateLimited(_)
    ));
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
                headers: Box::default(),
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

fn target_for(adapter: AdapterKind, model: &str) -> ServiceTarget {
    ServiceTarget {
        endpoint: Endpoint::from_static("https://example.invalid/v1/"),
        auth: AuthData::from_single("k"),
        model: ModelIden::new(adapter, model),
    }
}

/// Every Claude from 4.7 on answers 400 to a non-default `temperature` or `top_p`, so the
/// Anthropic adapter sends neither, whatever the caller tuned. Regression anchor: suggestions
/// (0.6), search (0.3), and selection (0.2) all failed on `claude-*-5-5` while the smoke lane
/// sat on Haiku 4.5, which still took them.
#[test]
fn the_anthropic_adapter_sends_no_sampling_knobs() {
    let tuned = ChatOptions::default()
        .with_temperature(0.3)
        .with_top_p(0.9)
        .with_max_tokens(200);
    let sent = adjust_for_model(&tuned, &target_for(AdapterKind::Anthropic, "claude-opus-5-5"));
    assert_eq!(sent.temperature, None);
    assert_eq!(sent.top_p, None);
    assert_eq!(sent.max_tokens, Some(200), "everything else passes through");
}

/// The Responses API fills an unset effort with `low`, except for a caller that turned
/// reasoning capture off: that's the agent saying the model doesn't reason, and an effort sent
/// to such a model is a 400 (`agent::llm::CallSettings::model_reasons`).
#[test]
fn a_caller_opting_out_of_reasoning_gets_no_effort_filled_in() {
    let target = target_for(AdapterKind::OpenAIResp, "gpt-5.5");
    let filled = adjust_for_model(&ChatOptions::default(), &target);
    assert!(filled.reasoning_effort.is_some(), "an unset effort still gets `low`");

    let opted_out = adjust_for_model(&ChatOptions::default().with_capture_reasoning_content(false), &target);
    assert!(opted_out.reasoning_effort.is_none());
}

#[test]
fn an_openai_compatible_model_keeps_its_sampling_knobs() {
    let tuned = ChatOptions::default().with_temperature(0.3).with_top_p(0.9);
    let sent = adjust_for_model(&tuned, &target_for(AdapterKind::OpenAI, "gpt-4.1-mini"));
    assert_eq!(sent.temperature, Some(0.3));
    assert_eq!(sent.top_p, Some(0.9));
}
