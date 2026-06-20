//! Cloud-AI endpoint probing and BYOK-key safety.
//!
//! Self-contained, mostly pure: it checks connectivity to an OpenAI-compatible
//! `/models` endpoint and guards the user's API key against plaintext exfiltration
//! before any request carries it in an `Authorization: Bearer` header. No
//! `ManagerState` access — every input is an explicit argument.

use regex::Regex;
use std::borrow::Cow;
use std::sync::OnceLock;

/// Result of checking connectivity to an AI API endpoint.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionCheckResult {
    pub connected: bool,
    pub auth_error: bool,
    pub models: Vec<String>,
    pub error: Option<String>,
}

/// Checks connectivity to an AI API endpoint by calling GET {base_url}/models.
/// Returns connection status, auth status, and available model list.
#[tauri::command]
#[specta::specta]
pub async fn check_ai_connection(base_url: String, api_key: String) -> AiConnectionCheckResult {
    // Same plaintext-key guard as `configure_ai`: never send the BYOK key over
    // `http://` to a non-loopback host.
    if let Err(message) = validate_ai_base_url(&base_url, &api_key) {
        return AiConnectionCheckResult {
            connected: false,
            auth_error: false,
            models: vec![],
            error: Some(message),
        };
    }

    let url = format!("{}/models", base_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return AiConnectionCheckResult {
                connected: false,
                auth_error: false,
                models: vec![],
                error: Some(format!("Can't create HTTP client: {e}")),
            };
        }
    };

    let mut request = client.get(&url);
    if !api_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {api_key}"));
    }

    let response = match request.send().await {
        Ok(r) => r,
        Err(e) => {
            let msg = if e.is_timeout() {
                String::from("Can't reach server (timed out)")
            } else if e.is_connect() {
                String::from("Can't reach server")
            } else {
                format!("Can't reach server: {e}")
            };
            return AiConnectionCheckResult {
                connected: false,
                auth_error: false,
                models: vec![],
                error: Some(msg),
            };
        }
    };

    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return AiConnectionCheckResult {
            connected: true,
            auth_error: true,
            models: vec![],
            error: Some(String::from("API key is invalid")),
        };
    }

    if status == reqwest::StatusCode::OK {
        let body = response.text().await.unwrap_or_default();
        // Try parsing OpenAI-style response: { "data": [{ "id": "model-name" }, ...] }
        let models = parse_model_ids(&body);
        return AiConnectionCheckResult {
            connected: true,
            auth_error: false,
            models,
            error: None,
        };
    }

    // Other HTTP error
    let body = response.text().await.unwrap_or_default();
    let body_preview = truncate_body_preview(&body, 200);
    AiConnectionCheckResult {
        connected: true,
        auth_error: false,
        models: vec![],
        error: Some(format!("HTTP {status}: {body_preview}")),
    }
}

/// Validates a cloud-AI base URL before we attach the BYOK API key to a request.
///
/// We attach the key as an `Authorization: Bearer ...` header. Sending that over
/// plaintext `http://` to a host we don't control would leak the secret on the
/// wire, so we require `https://` unless the host is loopback. Loopback `http://`
/// stays allowed because the Ollama / LM Studio presets are `http://localhost:*`.
///
/// An empty `api_key` means there's no secret to leak, so plaintext to any host is
/// fine (used for local OpenAI-compatible servers that don't require auth).
///
/// Never logs `api_key`.
pub(super) fn validate_ai_base_url(url: &str, api_key: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| String::from("That endpoint URL doesn't look valid."))?;

    match parsed.scheme() {
        "https" => Ok(()),
        "http" => {
            if api_key.is_empty() || host_is_loopback(&parsed) {
                Ok(())
            } else {
                Err(String::from(
                    "We only send your API key over HTTPS — use https:// or clear the key first.",
                ))
            }
        }
        _ => Err(String::from("That endpoint URL doesn't look valid.")),
    }
}

/// True when the URL's host is a loopback address (`localhost`, `127.0.0.1`, `::1`).
fn host_is_loopback(parsed: &reqwest::Url) -> bool {
    let Some(host) = parsed.host_str() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // `host_str()` returns IPv6 hosts wrapped in brackets, e.g. `[::1]`.
    let bare = host.strip_prefix('[').and_then(|h| h.strip_suffix(']')).unwrap_or(host);
    bare.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/// Parses model IDs from an OpenAI-compatible /models response.
/// Returns empty vec on parse failure (connected but can't list models).
fn parse_model_ids(body: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        data: Vec<ModelEntry>,
    }
    #[derive(serde::Deserialize)]
    struct ModelEntry {
        id: String,
    }

    serde_json::from_str::<ModelsResponse>(body)
        .map(|r| r.data.into_iter().map(|m| m.id).collect())
        .unwrap_or_default()
}

/// Truncate an error-response body to at most `max` characters for a log/error preview,
/// appending `...` only when truncation actually happened.
///
/// Char-based (not byte-based): slicing `&body[..max]` panics when byte `max` lands inside
/// a multibyte UTF-8 sequence, which is trivially reachable with a non-ASCII error body from
/// a user-configured AI endpoint. We also scrub `Bearer <token>`-shaped substrings as
/// belt-and-suspenders in case a misbehaving proxy reflects the `Authorization` header back
/// in its error body.
fn truncate_body_preview(body: &str, max: usize) -> String {
    let scrubbed = scrub_bearer_tokens(body);
    let mut chars = scrubbed.chars();
    let preview: String = chars.by_ref().take(max).collect();
    // `chars` still has at least one item left iff the body was longer than `max` chars.
    if chars.next().is_some() {
        format!("{preview}...")
    } else {
        preview
    }
}

/// Replace the token in any `Bearer <token>` substring with `<redacted>`, leaving the rest
/// of the text intact. Case-insensitive on the `Bearer` keyword.
fn scrub_bearer_tokens(text: &str) -> Cow<'_, str> {
    static BEARER_RE: OnceLock<Regex> = OnceLock::new();
    let re = BEARER_RE.get_or_init(|| Regex::new(r"(?i)\bBearer\s+\S+").expect("valid bearer regex"));
    re.replace_all(text, "Bearer <redacted>")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_body_preview_is_char_safe_on_multibyte_boundary() {
        // '€' is 3 bytes in UTF-8. A string of 300 of them is 900 bytes; byte 200 lands
        // mid-codepoint (200 % 3 != 0), so the old `&body[..200]` form would panic here.
        let body = "€".repeat(300);
        // The point is simply that this does NOT panic.
        let out = truncate_body_preview(&body, 200);
        // 200 chars kept (each still a full '€'), plus the "..." marker.
        assert_eq!(out.chars().filter(|&c| c == '€').count(), 200);
        assert!(out.ends_with("..."));

        // '日' is also 3 bytes; same boundary hazard, different codepoint.
        let body = "日".repeat(300);
        let out = truncate_body_preview(&body, 200);
        assert_eq!(out.chars().filter(|&c| c == '日').count(), 200);
    }

    #[test]
    fn truncate_body_preview_truncates_ascii() {
        let body = "a".repeat(500);
        let out = truncate_body_preview(&body, 200);
        assert_eq!(out, format!("{}...", "a".repeat(200)));
    }

    #[test]
    fn truncate_body_preview_no_ellipsis_when_short() {
        assert_eq!(truncate_body_preview("short body", 200), "short body");
        // Exactly `max` chars: no truncation, so no ellipsis.
        let exact = "x".repeat(200);
        assert_eq!(truncate_body_preview(&exact, 200), exact);
    }

    #[test]
    fn truncate_body_preview_scrubs_bearer_tokens() {
        let body = "error: invalid auth Bearer sk-abc123secret rejected";
        let out = truncate_body_preview(body, 200);
        assert!(out.contains("Bearer <redacted>"), "got: {out}");
        assert!(!out.contains("sk-abc123secret"), "token leaked: {out}");
    }

    #[test]
    fn validate_url_allows_https() {
        assert!(validate_ai_base_url("https://api.openai.com/v1", "sk-secret").is_ok());
        assert!(validate_ai_base_url("https://api.openai.com/v1", "").is_ok());
    }

    #[test]
    fn validate_url_allows_http_loopback() {
        // Ollama / LM Studio presets, with or without a key.
        assert!(validate_ai_base_url("http://localhost:11434/v1", "key").is_ok());
        assert!(validate_ai_base_url("http://127.0.0.1:1234/v1", "key").is_ok());
        assert!(validate_ai_base_url("http://[::1]:8080/v1", "key").is_ok());
        assert!(validate_ai_base_url("http://localhost:11434/v1", "").is_ok());
    }

    #[test]
    fn validate_url_rejects_http_remote_with_key() {
        assert!(validate_ai_base_url("http://api.openai.com/v1", "sk-secret").is_err());
        assert!(validate_ai_base_url("http://10.0.0.5:1234/v1", "key").is_err());
    }

    #[test]
    fn validate_url_allows_http_remote_without_key() {
        // No secret to leak, so plaintext to a remote host is allowed.
        assert!(validate_ai_base_url("http://api.openai.com/v1", "").is_ok());
        assert!(validate_ai_base_url("http://10.0.0.5:1234/v1", "").is_ok());
    }

    #[test]
    fn validate_url_rejects_garbage() {
        assert!(validate_ai_base_url("not a url", "key").is_err());
        assert!(validate_ai_base_url("ftp://example.com", "key").is_err());
        assert!(validate_ai_base_url("", "key").is_err());
    }
}
