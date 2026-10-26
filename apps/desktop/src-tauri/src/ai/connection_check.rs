//! Cloud-AI endpoint probing and BYOK-key safety.
//!
//! Self-contained, mostly pure: it asks a provider's model-list endpoint whether the key works
//! (and which models it serves), and guards the user's API key against plaintext exfiltration
//! before any request carries it. No `ManagerState` access — every input is an explicit argument.
//!
//! The probe carries no user data, but it does reach the service, so it's gated on cloud
//! consent (`super::cloud_consent`) like every LLM call: nothing reaches a cloud AI service
//! until the user allows it. It's gated whatever `ai.provider` says, since the only thing it
//! ever probes is a cloud endpoint being set up.

use regex::Regex;

use crate::managed_policy::{ManagedAiRefusal, ManagedPolicy};
use std::borrow::Cow;
use std::sync::OnceLock;

/// Result of checking connectivity to an AI API endpoint. Success is `problem: None` with no
/// refusal; `models` may still be empty when the provider has no usable list.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AiConnectionCheckResult {
    pub models: Vec<String>,
    /// What went wrong, typed so the UI renders its own translated copy.
    pub problem: Option<AiConnectionProblem>,
    /// The user hasn't allowed cloud AI, so nothing was sent. The other fields are empty.
    pub cloud_consent_missing: bool,
    /// The organization's policy refuses this endpoint, so nothing was sent. The other fields are
    /// empty. Decided before consent.
    pub managed: Option<ManagedAiRefusal>,
}

impl AiConnectionCheckResult {
    fn models(models: Vec<String>) -> Self {
        Self {
            models,
            problem: None,
            cloud_consent_missing: false,
            managed: None,
        }
    }

    fn problem(problem: AiConnectionProblem) -> Self {
        Self {
            models: vec![],
            problem: Some(problem),
            cloud_consent_missing: false,
            managed: None,
        }
    }
}

/// Why a key check didn't succeed. The UI owns the wording; only `ProviderRefused` carries text,
/// the provider's own sentence, for a secondary detail line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AiConnectionProblem {
    /// The endpoint isn't an `http(s)` URL.
    InvalidEndpoint,
    /// Plain `http://` to a non-loopback host while a key is set: [`validate_ai_base_url`].
    InsecureEndpoint,
    /// No answer: DNS, a refused connection, TLS, or a redirect the policy stopped.
    Unreachable,
    /// No answer within [`PROBE_TIMEOUT`].
    TimedOut,
    /// The provider says the key is wrong: 401 or 403, or a provider's own typed signal.
    KeyRejected,
    /// Any other refusal. `detail` is the provider's `error.message` (or raw body), key scrubbed.
    ProviderRefused { status: u16, detail: String },
}

/// Long enough for a slow provider, short enough that a dead one doesn't hold the row.
const PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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

/// How a provider wants its key checked and its models listed. Chat requests don't need this
/// (`genai` picks the adapter from the model name), but the probe runs before there's a model to
/// go by. Each non-default variant exists because the plain probe answered wrong for that provider
/// (verified live 2026-10-26, `curl` with a real key and a bad one).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelListProtocol {
    /// `GET {base}/models` with `Authorization: Bearer <key>`, answering `{ "data": [{ "id" }] }`.
    OpenAiCompatible,
    /// Anthropic's native API: the key in `x-api-key`, plus a required `anthropic-version`
    /// header (without it: HTTP 400 "anthropic-version: header is required"). Same `data[].id`
    /// answer, newest first, but paged at 20 by default, so we ask for the maximum page.
    Anthropic,
    /// Google's native API (`…/v1/`): the key in `x-goog-api-key`, answering
    /// `{ "models": [{ "name": "models/gemini-…", "supportedGenerationMethods" }] }`. A bad key is
    /// 400 `API_KEY_INVALID`, not 401.
    Gemini,
    /// OpenAI-shaped, but its `/models` is public and answers 200 to ANY key, so the key itself
    /// is checked at `GET {base}/key` first (401 for a bad one).
    OpenRouter,
    /// Its base has no `/v1`, so `{base}/models` is a 404; `{base}/v1/models` checks the key. That
    /// list names its agent API's models, which its chat endpoint doesn't serve, so none are offered.
    Perplexity,
}

/// The API version Anthropic's `GET /v1/models` is asked under. It answers identically on
/// `2023-06-01` and `2026-10-21` (Anthropic's migration guide limits that version's changes to
/// Messages, token counting, batches, and the Admin API).
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Anthropic's largest `/v1/models` page. The whole catalog is 14 models (2026-10-26), so one
/// page holds it with room to spare.
const ANTHROPIC_MODEL_PAGE_LIMIT: u32 = 1000;

/// Gemini's largest `/v1/models` page (26 models on 2026-10-26).
const GEMINI_MODEL_PAGE_SIZE: u32 = 1000;

impl ModelListProtocol {
    /// Keyed by the preset id the settings UI passes, the same id the key is stored under.
    fn for_provider(provider_id: &str) -> Self {
        match provider_id {
            "anthropic" => Self::Anthropic,
            "google-gemini" => Self::Gemini,
            "openrouter" => Self::OpenRouter,
            "perplexity" => Self::Perplexity,
            _ => Self::OpenAiCompatible,
        }
    }

    /// Puts the key where this provider looks for it, plus any header it requires.
    fn authorize(self, request: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
        match self {
            Self::Anthropic => {
                let request = request.header("anthropic-version", ANTHROPIC_API_VERSION);
                if api_key.is_empty() {
                    request
                } else {
                    request.header("x-api-key", api_key)
                }
            }
            Self::Gemini if !api_key.is_empty() => request.header("x-goog-api-key", api_key),
            Self::OpenAiCompatible | Self::OpenRouter | Self::Perplexity if !api_key.is_empty() => {
                request.header("Authorization", format!("Bearer {api_key}"))
            }
            _ => request,
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
        models: vec![],
        problem: None,
        cloud_consent_missing: false,
        managed: Some(refusal),
    })
}

/// The consent half of [`check_ai_connection`], pure so it's testable: `Some` answer when cloud
/// AI isn't allowed, `None` to go ahead and probe.
fn refuse_without_consent(cloud_consent: bool) -> Option<AiConnectionCheckResult> {
    (!cloud_consent).then(|| AiConnectionCheckResult {
        models: vec![],
        problem: None,
        cloud_consent_missing: true,
        managed: None,
    })
}

/// Asks the provider, the way `protocol` says, whether the key works and which models it serves.
/// Split from the command so it stays pure and testable.
pub(super) async fn probe_ai_endpoint(
    base_url: String,
    api_key: String,
    protocol: ModelListProtocol,
) -> AiConnectionCheckResult {
    // Same plaintext-key guard as `configure_ai`: never send the BYOK key over
    // `http://` to a non-loopback host.
    if let Err(rejection) = validate_ai_base_url(&base_url, &api_key) {
        return AiConnectionCheckResult::problem(rejection.into());
    }

    // The policy judged `base_url` above; the redirect guard judges every hop after it.
    let client = match cmdr_http::client_builder()
        .timeout(PROBE_TIMEOUT)
        .redirect(super::client::policy_guarded_redirects())
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            log::warn!("Couldn't build the AI key-check HTTP client: {e}");
            return AiConnectionCheckResult::problem(AiConnectionProblem::Unreachable);
        }
    };
    let base = base_url.trim_end_matches('/');
    let get = |url: String| protocol.authorize(client.get(url), &api_key);

    let listed = match protocol {
        ModelListProtocol::OpenAiCompatible => fetch(get(format!("{base}/models")), protocol, &api_key)
            .await
            .map(|body| sorted(openai_model_ids(&body))),
        ModelListProtocol::Anthropic => fetch(
            get(format!("{base}/models?limit={ANTHROPIC_MODEL_PAGE_LIMIT}")),
            protocol,
            &api_key,
        )
        .await
        .map(|body| openai_model_ids(&body)),
        ModelListProtocol::Gemini => fetch(
            get(format!("{base}/models?pageSize={GEMINI_MODEL_PAGE_SIZE}")),
            protocol,
            &api_key,
        )
        .await
        .map(|body| sorted(gemini_chat_model_ids(&body))),
        ModelListProtocol::OpenRouter => match fetch(get(format!("{base}/key")), protocol, &api_key).await {
            Ok(_) => fetch(get(format!("{base}/models")), protocol, &api_key)
                .await
                .map(|body| sorted(openai_model_ids(&body))),
            Err(problem) => Err(problem),
        },
        ModelListProtocol::Perplexity => fetch(get(format!("{base}/v1/models")), protocol, &api_key)
            .await
            .map(|_| vec![]),
    };
    match listed {
        Ok(models) => AiConnectionCheckResult::models(models),
        Err(problem) => AiConnectionCheckResult::problem(problem),
    }
}

/// Sends one probe request: the body on 200, else the typed problem.
async fn fetch(
    request: reqwest::RequestBuilder,
    protocol: ModelListProtocol,
    api_key: &str,
) -> Result<String, AiConnectionProblem> {
    let response = request.send().await.map_err(|e| {
        if e.is_timeout() {
            AiConnectionProblem::TimedOut
        } else {
            log::debug!("AI key check couldn't reach the server: {e}");
            AiConnectionProblem::Unreachable
        }
    })?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    match status {
        reqwest::StatusCode::OK => Ok(body),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN => Err(AiConnectionProblem::KeyRejected),
        _ if protocol == ModelListProtocol::Gemini && is_gemini_key_rejection(&body) => {
            Err(AiConnectionProblem::KeyRejected)
        }
        _ => Err(AiConnectionProblem::ProviderRefused {
            status: status.as_u16(),
            detail: super::client::provider_error_message(&scrub_secrets(&body, api_key)),
        }),
    }
}

/// Google answers a bad key with 400 `INVALID_ARGUMENT`, and says which in a typed field:
/// `error.details[].reason == "API_KEY_INVALID"` (google.rpc.ErrorInfo). Read that field, never
/// the sentence beside it.
fn is_gemini_key_rejection(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            Some(
                value
                    .get("error")?
                    .get("details")?
                    .as_array()?
                    .iter()
                    .any(|detail| detail.get("reason").and_then(|r| r.as_str()) == Some("API_KEY_INVALID")),
            )
        })
        .unwrap_or(false)
}

/// Why [`validate_ai_base_url`] won't send a key to an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EndpointRejection {
    /// Not an `http(s)` URL.
    Invalid,
    /// Plain `http://` to a non-loopback host while a key is set.
    Insecure,
}

impl From<EndpointRejection> for AiConnectionProblem {
    fn from(rejection: EndpointRejection) -> Self {
        match rejection {
            EndpointRejection::Invalid => Self::InvalidEndpoint,
            EndpointRejection::Insecure => Self::InsecureEndpoint,
        }
    }
}

impl std::fmt::Display for EndpointRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid => write!(f, "That endpoint URL doesn't look valid."),
            Self::Insecure => write!(
                f,
                "We only send your API key over HTTPS. Use https://, or clear the key first."
            ),
        }
    }
}

/// Validates a cloud-AI base URL before we attach the BYOK API key to a request.
///
/// Sending the key over plaintext `http://` to a host we don't control would leak the secret on
/// the wire, so we require `https://` unless the host is loopback. Loopback `http://` stays
/// allowed because the Ollama / LM Studio presets are `http://localhost:*`.
///
/// An empty `api_key` means there's no secret to leak, so plaintext to any host is
/// fine (used for local OpenAI-compatible servers that don't require auth).
///
/// Never logs `api_key`.
pub(super) fn validate_ai_base_url(url: &str, api_key: &str) -> Result<(), EndpointRejection> {
    let parsed = reqwest::Url::parse(url).map_err(|_| EndpointRejection::Invalid)?;

    match parsed.scheme() {
        "https" => Ok(()),
        "http" if api_key.is_empty() || host_is_loopback(&parsed) => Ok(()),
        "http" => Err(EndpointRejection::Insecure),
        _ => Err(EndpointRejection::Invalid),
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

/// Model ids from an OpenAI-shaped `{ "data": [{ "id" }] }` answer, in the provider's order.
/// Empty on a shape we don't know (connected, but nothing to suggest).
fn openai_model_ids(body: &str) -> Vec<String> {
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

/// The `gemini-*` chat models in a native Gemini list, as bare ids. A model must offer
/// `generateContent` (that drops embeddings and the like), and only `gemini-*` ids route to the
/// native adapter (`client.rs::remote_model_iden`); a `gemma-*` id would go out as OpenAI chat
/// completions and 404 on this base.
fn gemini_chat_model_ids(body: &str) -> Vec<String> {
    #[derive(serde::Deserialize)]
    struct ModelsResponse {
        #[serde(default)]
        models: Vec<ModelEntry>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ModelEntry {
        name: String,
        #[serde(default)]
        supported_generation_methods: Vec<String>,
    }

    serde_json::from_str::<ModelsResponse>(body)
        .map(|r| {
            r.models
                .into_iter()
                .filter(|m| m.supported_generation_methods.iter().any(|g| g == "generateContent"))
                .filter_map(|m| m.name.strip_prefix("models/").map(str::to_string))
                .filter(|id| id.starts_with("gemini-"))
                .collect()
        })
        .unwrap_or_default()
}

/// Sorted and deduplicated, for providers whose list comes in no useful order (OpenAI's ~150).
fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Takes the key out of an error body before it reaches the UI, as belt-and-suspenders in case a
/// misbehaving proxy reflects the request's headers back: the key itself wherever it appears
/// (it travels in `x-api-key` / `x-goog-api-key` for some providers), plus any `Bearer <token>`.
/// Capping the length is `provider_error_message`'s job.
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
#[path = "connection_check_test.rs"]
mod tests;
