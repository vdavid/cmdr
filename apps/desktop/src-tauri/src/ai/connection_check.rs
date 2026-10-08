//! Cloud-AI endpoint probing and BYOK-key safety.
//!
//! Self-contained, mostly pure: it checks connectivity to an OpenAI-compatible
//! `/models` endpoint and guards the user's API key against plaintext exfiltration
//! before any request carries it in an `Authorization: Bearer` header. No
//! `ManagerState` access — every input is an explicit argument.
//!
//! The probe carries no user data, but it does reach the service, so it's gated on cloud
//! consent (`super::cloud_consent`) like every LLM call: nothing reaches a cloud AI service
//! until the user allows it. It's gated whatever `ai.provider` says, since the only thing it
//! ever probes is a cloud endpoint being set up.

use regex::Regex;

use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};
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
    /// The user hasn't allowed cloud AI, so nothing was sent. The other fields are empty.
    pub cloud_consent_missing: bool,
    /// The organization's policy refuses this endpoint, so nothing was sent. The other fields are
    /// empty. Decided before consent.
    pub managed: Option<ManagedAiRefusal>,
}

/// Checks connectivity to the given provider's AI API endpoint.
///
/// Takes a provider ID, NOT a key: the key is read here from the OS secret store, so it never
/// travels through a webview. See `api_keys.rs`. A key that can't be read probes unauthenticated,
/// which surfaces as the auth error it effectively is.
#[tauri::command]
#[specta::specta]
pub async fn check_ai_connection(
    app: tauri::AppHandle,
    base_url: String,
    provider_id: String,
) -> AiConnectionCheckResult {
    if let Some(refused) = refuse_by_policy(&*crate::managed_policy::for_egress().await, &base_url) {
        return refused;
    }
    if let Some(refused) = refuse_without_consent(super::cloud_consent::cloud_consent_from_app(&app)) {
        return refused;
    }
    let (api_key, _) = super::api_keys::read_for_backend(&provider_id);
    probe_ai_endpoint(base_url, api_key).await
}

/// The policy half of [`check_ai_connection`], pure so it's testable: `Some` answer when the
/// organization refuses the endpoint (judged as the backend would send to it), `None` to go on.
fn refuse_by_policy(policy: &ManagedPolicy, base_url: &str) -> Option<AiConnectionCheckResult> {
    let refusal = policy
        .ai_destination(&super::client::remote_destination(base_url))
        .err()?;
    Some(AiConnectionCheckResult {
        connected: false,
        auth_error: false,
        models: vec![],
        error: None,
        cloud_consent_missing: false,
        managed: Some(refusal),
    })
}

/// The consent half of [`check_ai_connection`], pure so it's testable: `Some` answer when cloud
/// AI isn't allowed, `None` to go ahead and probe.
fn refuse_without_consent(cloud_consent: bool) -> Option<AiConnectionCheckResult> {
    (!cloud_consent).then(|| AiConnectionCheckResult {
        connected: false,
        auth_error: false,
        models: vec![],
        error: None,
        cloud_consent_missing: true,
        managed: None,
    })
}

/// Probes GET {base_url}/models with an explicit key. Returns connection status, auth status, and
/// the available model list. Split from the command so it stays pure and testable.
async fn probe_ai_endpoint(base_url: String, api_key: String) -> AiConnectionCheckResult {
    // Same plaintext-key guard as `configure_ai`: never send the BYOK key over
    // `http://` to a non-loopback host.
    if let Err(message) = validate_ai_base_url(&base_url, &api_key) {
        return AiConnectionCheckResult {
            connected: false,
            auth_error: false,
            models: vec![],
            error: Some(message),
            cloud_consent_missing: false,
            managed: None,
        };
    }

    let url = format!("{}/models", base_url.trim_end_matches('/'));

    // The policy judged `base_url` above; the redirect guard judges every hop after it.
    let client = match cmdr_http::client_builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(super::client::policy_guarded_redirects())
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return AiConnectionCheckResult {
                connected: false,
                auth_error: false,
                models: vec![],
                error: Some(format!("Can't create HTTP client: {e}")),
                cloud_consent_missing: false,
                managed: None,
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
                cloud_consent_missing: false,
                managed: None,
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
            cloud_consent_missing: false,
            managed: None,
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
            cloud_consent_missing: false,
            managed: None,
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
        cloud_consent_missing: false,
        managed: None,
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

    /// Without cloud consent the check answers at once, typed, and reaches nothing: no key read,
    /// no URL validation, no client, no request.
    #[test]
    fn without_cloud_consent_the_check_refuses_before_any_request() {
        let refused = refuse_without_consent(false).expect("no consent ⇒ a refusal");
        assert!(refused.cloud_consent_missing);
        assert!(!refused.connected);
        assert!(!refused.auth_error);
        assert!(refused.models.is_empty());
        assert_eq!(refused.error, None, "a typed flag, not a sentence");
    }

    /// The organization's policy refuses before consent, typed, and reaches nothing.
    #[test]
    fn a_refused_endpoint_is_answered_without_a_request() {
        use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS, DISABLE_CLOUD_AI};
        let cloud_off = testing::forcing(&[DISABLE_CLOUD_AI]);
        let refused = refuse_by_policy(&cloud_off, "http://localhost:11434/v1").expect("cloud off ⇒ a refusal");
        assert_eq!(refused.managed, Some(ManagedAiRefusal::CloudAiOff));
        assert!(!refused.connected && !refused.cloud_consent_missing && refused.error.is_none());

        let listed = testing::from_values(&[(
            ALLOWED_CLOUD_AI_HOSTS,
            plist::Value::Array(vec![plist::Value::String("api.openai.com".into())]),
        )]);
        assert_eq!(
            refuse_by_policy(&listed, "https://evil.example.com/v1").and_then(|r| r.managed),
            Some(ManagedAiRefusal::HostNotAllowed)
        );
        assert!(refuse_by_policy(&listed, "https://api.openai.com/v1").is_none());
        assert!(refuse_by_policy(&ManagedPolicy::default(), "https://evil.example.com/v1").is_none());
    }

    /// An allowed endpoint answering 302 must not carry the probe to a host the policy refuses:
    /// the probe follows redirects through the same guard as every LLM request.
    #[tokio::test]
    async fn the_probe_never_follows_a_redirect_to_a_refused_host() {
        use crate::managed_policy::testing::{self, ALLOWED_CLOUD_AI_HOSTS};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let elsewhere = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"data":[{"id":"m"}]}"#))
            .mount(&elsewhere)
            .await;
        let allowed = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(
                ResponseTemplate::new(302).insert_header("location", format!("{}/v1/models", elsewhere.uri())),
            )
            .mount(&allowed)
            .await;

        // Both servers listen on 127.0.0.1, so the list names the allowed one by its port.
        let entry = format!("127.0.0.1:{}", allowed.address().port());
        let _policy = testing::override_for_test(testing::from_values(&[(
            ALLOWED_CLOUD_AI_HOSTS,
            plist::Value::Array(vec![plist::Value::String(entry)]),
        )]));
        let result = probe_ai_endpoint(format!("{}/v1", allowed.uri()), String::new()).await;

        assert!(result.models.is_empty(), "the probe stopped at the 302");
        let reached = elsewhere.received_requests().await.map_or(0, |r| r.len());
        assert_eq!(reached, 0, "the redirect target saw nothing");
    }

    #[test]
    fn with_cloud_consent_the_check_goes_ahead() {
        assert!(refuse_without_consent(true).is_none());
    }

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
