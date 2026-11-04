//! Turns a failed provider call into a typed [`AiError`]: by HTTP status, plus the response's
//! request-limit headers for a 429, never by the provider's wording. Pure functions;
//! `client_unit_test.rs` pins them without a network.

use std::fmt::Display;

use serde_json::Value;

use super::AiError;

/// Classifies a provider HTTP error status into the right [`AiError`] so the frontend can
/// show a specific toast (key rejected vs. out of quota vs. generic server error). Branches
/// on the numeric status, never the message body. 429 covers both rate-limiting and
/// OpenAI's `insufficient_quota`; 413 is a request past what the provider (or the account's
/// plan) takes in one go; 401/403 is a rejected key; 404 is a model id the provider
/// doesn't serve (a typo, or one it decommissioned) or a base URL with the wrong path.
pub(super) fn ai_error_for_status(status: u16, detail: String) -> AiError {
    match status {
        401 | 403 => AiError::AuthFailed(detail),
        404 => AiError::NotFound(detail),
        413 => AiError::RequestTooLarge(detail),
        429 => AiError::RateLimited(detail),
        _ => AiError::ServerError(detail),
    }
}

/// [`ai_error_for_status`], plus the one case the status alone can't tell: a 429 whose
/// response says the account may send NO requests to this model ([`plan_allows_no_requests`]),
/// which is a plan without the model rather than a busy provider or a spent quota.
fn ai_error_for_response(status: u16, headers: &reqwest::header::HeaderMap, detail: String) -> AiError {
    if status == 429 && plan_allows_no_requests(headers) {
        return AiError::ModelNotInPlan(detail);
    }
    ai_error_for_status(status, detail)
}

/// Request-limit headers that state the account's per-window CAP (not what's left of it).
/// Mistral sends `x-ratelimit-limit-req-minute`; OpenAI, Groq, and most OpenAI-compatible
/// hosts send `x-ratelimit-limit-requests`.
const REQUEST_LIMIT_HEADERS: [&str; 2] = ["x-ratelimit-limit-req-minute", "x-ratelimit-limit-requests"];

/// Whether a failed response says the account's request limit for this model is zero. A free
/// Mistral workspace answers `mistral-small-latest` (and `mistral-medium-*`, `magistral-small-*`,
/// `devstral-small-*`) with a 429 carrying `x-ratelimit-limit-req-minute: 0` (verified by `curl`
/// on a free key, 2026-11-03). A zero `remaining` count is a used-up limit, so it's not read.
fn plan_allows_no_requests(headers: &reqwest::header::HeaderMap) -> bool {
    REQUEST_LIMIT_HEADERS.iter().any(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.trim().parse::<u64>().ok())
            == Some(0)
    })
}

/// Builds the display detail for a failed provider response: `HTTP <status>: <message>`.
/// The message is the JSON body's `error.message` when present (OpenAI, OpenRouter,
/// Anthropic, and Gemini all put the human sentence there), else the raw body, capped so
/// an HTML error page (a proxy, Cloudflare) can't flood the UI or the logs. Display only,
/// never control flow: classification stays on the numeric status (`ai_error_for_status`).
pub(super) fn provider_error_detail(status: impl Display, body: &str) -> String {
    format!("HTTP {status}: {}", provider_error_message(body))
}

/// The human sentence in a failed provider response: the JSON body's `error.message`, else the
/// raw body, capped at 400 chars. Shared with the key check, which shows it as a detail line.
pub(in crate::ai) fn provider_error_message(body: &str) -> String {
    const MAX_CHARS: usize = 400;
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| Some(value.get("error")?.get("message")?.as_str()?.to_string()))
        .unwrap_or_else(|| body.to_string());
    let message = message.trim();
    if message.chars().count() > MAX_CHARS {
        let capped: String = message.chars().take(MAX_CHARS).collect();
        format!("{capped}…")
    } else {
        message.to_string()
    }
}

/// Maps `genai`'s rich error tree to our flat [`AiError`]. Pattern-matches on the
/// known transport variants instead of grepping the `Display` output. Shared with
/// the agent LLM (`crate::agent::llm`), which maps `AiError` on to its own typed
/// error, so the status-based classification lives in one place.
pub(crate) fn map_genai_error(e: genai::Error) -> AiError {
    use genai::Error as G;
    use genai::webc::Error as W;

    // A failure on the STREAMING path arrives differently: `genai` wraps the response error in
    // `WebStream { error: BoxError }`, where the box holds a `genai::Error::HttpError` carrying
    // the status. Without this arm every streaming failure fell through to the catch-all below,
    // so a rejected key or an exhausted quota reached the UI as a generic server error — on the
    // path the agent and folder suggestions actually use.
    if let G::WebStream { error, .. } = &e
        && let Some(G::HttpError {
            status, body, headers, ..
        }) = error.downcast_ref::<G>()
    {
        return ai_error_for_response(status.as_u16(), headers, provider_error_detail(status, body));
    }

    let webc = match &e {
        G::WebAdapterCall { webc_error, .. } | G::WebModelCall { webc_error, .. } => Some(webc_error),
        _ => None,
    };

    if let Some(webc) = webc {
        match webc {
            W::Reqwest(req) if req.is_timeout() => return AiError::Timeout,
            W::Reqwest(req) if req.is_connect() => return AiError::Unavailable,
            W::Reqwest(req) => return AiError::ServerError(format!("network error: {req}")),
            W::ResponseFailedStatus { status, body, headers } => {
                return ai_error_for_response(status.as_u16(), headers, provider_error_detail(status, body));
            }
            W::ResponseFailedNotJson { content_type, body } => {
                return AiError::ParseError(format!(
                    "expected JSON response, got content-type {content_type}: {body}"
                ));
            }
            W::ResponseFailedInvalidJson { body, cause } => {
                return AiError::ParseError(format!("invalid JSON ({cause}): {body}"));
            }
            W::JsonValueExt(err) => {
                return AiError::ParseError(format!("JSON shape mismatch: {err}"));
            }
        }
    }

    AiError::ServerError(e.to_string())
}
