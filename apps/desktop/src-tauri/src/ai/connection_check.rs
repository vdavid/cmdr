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
    probe_ai_endpoint(base_url, api_key, ModelListProtocol::for_provider(&provider_id)).await
}

/// How a provider's `GET /models` wants to be asked. Chat requests don't need this (`genai` picks
/// the adapter from the model name), but the probe runs before there's a model to go by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelListProtocol {
    /// `Authorization: Bearer <key>`, answering `{ "data": [{ "id": … }] }`. Every preset but one.
    OpenAiCompatible,
    /// Anthropic's native API: the key in `x-api-key`, plus a required `anthropic-version`
    /// header (without it: HTTP 400 "anthropic-version: header is required"). Same `data[].id`
    /// answer, but paged at 20 by default, so we ask for the maximum page.
    Anthropic,
}

/// The API version Anthropic's `GET /v1/models` is asked under. It answers identically on
/// `2023-06-01` and `2026-10-21` (Anthropic's migration guide limits that version's changes to
/// Messages, token counting, batches, and the Admin API).
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Anthropic's largest `/v1/models` page. The whole catalog is 14 models (2026-10-26), so one
/// page holds it with room to spare.
const ANTHROPIC_MODEL_PAGE_LIMIT: u32 = 1000;

impl ModelListProtocol {
    /// Keyed by the preset id the settings UI passes, the same id the key is stored under.
    fn for_provider(provider_id: &str) -> Self {
        match provider_id {
            "anthropic" => Self::Anthropic,
            _ => Self::OpenAiCompatible,
        }
    }
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
pub(super) async fn probe_ai_endpoint(
    base_url: String,
    api_key: String,
    protocol: ModelListProtocol,
) -> AiConnectionCheckResult {
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

    let models_url = format!("{}/models", base_url.trim_end_matches('/'));
    let url = match protocol {
        ModelListProtocol::OpenAiCompatible => models_url,
        ModelListProtocol::Anthropic => format!("{models_url}?limit={ANTHROPIC_MODEL_PAGE_LIMIT}"),
    };

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
    match protocol {
        ModelListProtocol::OpenAiCompatible => {
            if !api_key.is_empty() {
                request = request.header("Authorization", format!("Bearer {api_key}"));
            }
        }
        ModelListProtocol::Anthropic => {
            request = request.header("anthropic-version", ANTHROPIC_API_VERSION);
            if !api_key.is_empty() {
                request = request.header("x-api-key", api_key.as_str());
            }
        }
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

    // Other HTTP error: the provider's own sentence (`error.message`), not its raw JSON.
    let body = response.text().await.unwrap_or_default();
    AiConnectionCheckResult {
        connected: true,
        auth_error: false,
        models: vec![],
        error: Some(super::client::provider_error_detail(
            status,
            &scrub_secrets(&body, &api_key),
        )),
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

/// Takes the key out of an error body before it reaches the UI, as belt-and-suspenders in case a
/// misbehaving proxy reflects the request's headers back: the key itself wherever it appears
/// (it travels as `x-api-key` for Anthropic), plus any `Bearer <token>`. Capping the length is
/// `provider_error_detail`'s job.
fn scrub_secrets(body: &str, api_key: &str) -> String {
    let without_key = if api_key.is_empty() {
        Cow::Borrowed(body)
    } else {
        Cow::Owned(body.replace(api_key, "<redacted>"))
    };
    scrub_bearer_tokens(&without_key).into_owned()
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
        let result = probe_ai_endpoint(
            format!("{}/v1", allowed.uri()),
            String::new(),
            ModelListProtocol::OpenAiCompatible,
        )
        .await;

        assert!(result.models.is_empty(), "the probe stopped at the 302");
        let reached = elsewhere.received_requests().await.map_or(0, |r| r.len());
        assert_eq!(reached, 0, "the redirect target saw nothing");
    }

    #[test]
    fn with_cloud_consent_the_check_goes_ahead() {
        assert!(refuse_without_consent(true).is_none());
    }

    #[test]
    fn scrub_secrets_removes_bearer_tokens() {
        let out = scrub_secrets("error: invalid auth Bearer sk-abc123secret rejected", "");
        assert!(out.contains("Bearer <redacted>"), "got: {out}");
        assert!(!out.contains("sk-abc123secret"), "token leaked: {out}");
    }

    #[test]
    fn scrub_secrets_removes_the_key_wherever_it_appears() {
        let out = scrub_secrets(r#"{"echo":{"x-api-key":"sk-ant-secret"}}"#, "sk-ant-secret");
        assert!(!out.contains("sk-ant-secret"), "key leaked: {out}");
        assert!(out.contains("<redacted>"), "got: {out}");
    }

    /// Mounts a `/v1/models` answer on a fresh mock server and returns the server.
    async fn models_server(response: wiremock::ResponseTemplate) -> wiremock::MockServer {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .respond_with(response)
            .mount(&server)
            .await;
        server
    }

    fn only_request(requests: Option<Vec<wiremock::Request>>) -> wiremock::Request {
        let mut requests = requests.expect("request recording is on");
        assert_eq!(requests.len(), 1, "exactly one probe");
        requests.remove(0)
    }

    /// Regression anchor: the probe sent Anthropic only `Authorization: Bearer`, which it answers
    /// with 400 "anthropic-version: header is required", so every Anthropic user saw an error on
    /// a working key and never got a model list.
    #[tokio::test]
    async fn the_anthropic_probe_sends_its_native_headers_and_lists_models() {
        let body = r#"{"data":[{"id":"claude-opus-5-5","type":"model"},{"id":"claude-haiku-5-5","type":"model"}],"has_more":false}"#;
        let server = models_server(wiremock::ResponseTemplate::new(200).set_body_string(body)).await;

        let result = probe_ai_endpoint(
            format!("{}/v1/", server.uri()),
            "sk-ant-test".into(),
            ModelListProtocol::Anthropic,
        )
        .await;

        assert_eq!(result.models, ["claude-opus-5-5", "claude-haiku-5-5"]);
        assert_eq!(result.error, None);
        let request = only_request(server.received_requests().await);
        let header = |name: &str| {
            request
                .headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        assert_eq!(header("x-api-key").as_deref(), Some("sk-ant-test"));
        assert_eq!(header("anthropic-version").as_deref(), Some(ANTHROPIC_API_VERSION));
        assert_eq!(
            header("authorization"),
            None,
            "the key travels once, in its native header"
        );
        assert_eq!(
            request.url.query(),
            Some("limit=1000"),
            "one page holds the whole catalog"
        );
    }

    #[tokio::test]
    async fn an_openai_compatible_probe_sends_a_bearer_key() {
        let server =
            models_server(wiremock::ResponseTemplate::new(200).set_body_string(r#"{"data":[{"id":"gpt-4.1-mini"}]}"#))
                .await;

        let result = probe_ai_endpoint(
            format!("{}/v1", server.uri()),
            "sk-test".into(),
            ModelListProtocol::OpenAiCompatible,
        )
        .await;

        assert_eq!(result.models, ["gpt-4.1-mini"]);
        let request = only_request(server.received_requests().await);
        let auth = request.headers.get("authorization").and_then(|v| v.to_str().ok());
        assert_eq!(auth, Some("Bearer sk-test"));
        assert!(request.headers.get("x-api-key").is_none());
        assert!(request.headers.get("anthropic-version").is_none());
    }

    /// A refusal shows the provider's own sentence, never its JSON envelope, and never the key.
    #[tokio::test]
    async fn a_refused_probe_shows_the_providers_sentence() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"Something about sk-ant-test went wrong"}}"#;
        let server = models_server(wiremock::ResponseTemplate::new(400).set_body_string(body)).await;

        let result = probe_ai_endpoint(
            format!("{}/v1/", server.uri()),
            "sk-ant-test".into(),
            ModelListProtocol::Anthropic,
        )
        .await;

        assert_eq!(
            result.error.as_deref(),
            Some("HTTP 400 Bad Request: Something about <redacted> went wrong"),
        );
    }

    #[test]
    fn only_the_anthropic_preset_speaks_the_anthropic_protocol() {
        assert_eq!(
            ModelListProtocol::for_provider("anthropic"),
            ModelListProtocol::Anthropic
        );
        for id in ["openai", "google-gemini", "openrouter", "custom", "ollama"] {
            assert_eq!(
                ModelListProtocol::for_provider(id),
                ModelListProtocol::OpenAiCompatible,
                "{id}"
            );
        }
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
