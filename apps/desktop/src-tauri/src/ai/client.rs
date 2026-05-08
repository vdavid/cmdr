//! HTTP client for AI chat completions.
//!
//! Wraps [`genai`](https://crates.io/crates/genai), which normalizes the wire format across
//! OpenAI / OpenAI Responses / Anthropic / Gemini / xAI / Groq / DeepSeek / Ollama / etc.
//!
//! Two backend constructors:
//! - [`AiBackend::local`]: forces the OpenAI adapter at `http://127.0.0.1:<port>/v1/`.
//! - [`AiBackend::remote`]: BYOK — the model name picks the adapter (e.g. `claude-*` →
//!   Anthropic native, `gemini-*` → Gemini native, `gpt-5*`/`*-pro`/`*-codex` → OpenAI
//!   Responses), and `base_url` overrides the endpoint.

use std::sync::Arc;
use std::time::Duration;

use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ReasoningEffort};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{Client, ModelIden, ServiceTarget};

/// A configured AI backend ready to receive [`chat_completion`] calls.
///
/// Bundles a long-lived [`genai::Client`] with the model name to pass it. Build once
/// (cheap, but still a few allocations + one resolver-closure box) and reuse for the
/// lifetime of the configured provider.
pub struct AiBackend {
    client: Client,
    model: String,
}

impl AiBackend {
    /// Local llama-server on `127.0.0.1:<port>`. Forces the OpenAI chat-completions
    /// adapter regardless of model name.
    pub fn local(port: u16) -> Self {
        // Trailing slash is required: `genai` calls `Url::join("chat/completions")`,
        // which strips the last path segment when the base lacks `/`.
        let endpoint = format!("http://127.0.0.1:{port}/v1/");
        let resolver = make_resolver(endpoint, AuthData::from_single(""), ForceAdapter::OpenAi);
        let client = Client::builder().with_service_target_resolver(resolver).build();
        // Force the `openai::` namespace so `genai`'s adapter inference doesn't fall
        // back to Ollama for the bare `local-model` name. The resolver replaces the
        // rest, but adapter dispatch happens before the resolver runs.
        Self {
            client,
            model: String::from("openai::local-model"),
        }
    }

    /// Remote / cloud provider. Adapter is chosen from the model name prefix
    /// (e.g. `claude-3-5-sonnet-latest` → Anthropic, `gemini-2.0-flash` → Gemini).
    pub fn remote(api_key: String, base_url: String, model: String) -> Self {
        // Without a trailing `/` the `Url::join` quirk above silently drops `/v1`.
        let endpoint = if base_url.ends_with('/') {
            base_url
        } else {
            format!("{base_url}/")
        };
        let resolver = make_resolver(endpoint, AuthData::from_single(api_key), ForceAdapter::None);
        let client = Client::builder().with_service_target_resolver(resolver).build();
        Self { client, model }
    }
}

#[derive(Debug, Clone)]
pub enum AiError {
    /// Server is unreachable (DNS / connect refused / no route).
    Unavailable,
    /// Request timed out (server too slow, or local server unhealthy).
    Timeout,
    /// Server returned an HTTP error or otherwise misbehaved.
    ServerError(String),
    /// Couldn't parse the response body.
    ParseError(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "AI server unavailable"),
            Self::Timeout => write!(f, "AI request timed out"),
            Self::ServerError(msg) => write!(f, "AI server error: {msg}"),
            Self::ParseError(msg) => write!(f, "AI response parse error: {msg}"),
        }
    }
}

/// Sends a chat completion request to an AI backend.
///
/// `options` are the caller-supplied generation knobs. We auto-strip `temperature`/
/// `top_p` and substitute [`ReasoningEffort::Low`] when the resolved adapter+model
/// is reasoning-class — see [`is_openai_chat_reasoning_model`].
pub async fn chat_completion(
    backend: &AiBackend,
    system_prompt: &str,
    user_prompt: &str,
    options: &ChatOptions,
) -> Result<String, AiError> {
    let target = backend
        .client
        .resolve_service_target(&backend.model)
        .await
        .map_err(map_genai_error)?;

    let effective_options = adjust_for_model(options, &target);

    let req = ChatRequest::new(vec![
        ChatMessage::system(system_prompt.to_owned()),
        ChatMessage::user(user_prompt.to_owned()),
    ]);

    log::debug!(
        "AI chat_completion: sending request (adapter={:?}, model={})",
        target.model.adapter_kind,
        &*target.model.model_name
    );

    let res = backend
        .client
        .exec_chat(&backend.model, req, Some(&effective_options))
        .await
        .map_err(map_genai_error)?;

    let text = res
        .first_text()
        .ok_or_else(|| {
            // Common on reasoning models (`gpt-5*`, `o3*`, `*-pro`) when `max_tokens`
            // gets fully consumed by reasoning before any `output_text` is emitted.
            // The HTTP call succeeded; there's just no visible answer to return.
            AiError::ParseError(String::from(
                "AI returned no text — likely max_tokens fully consumed by reasoning. Increase max_tokens.",
            ))
        })?
        .to_owned();

    log::trace!("AI chat_completion: extracted content: {text}");
    Ok(text)
}

/// Per-model option fixup: reasoning-class models reject `temperature`. Returns a
/// modified clone of `options` when needed; otherwise hands back a clone unchanged.
fn adjust_for_model(options: &ChatOptions, target: &ServiceTarget) -> ChatOptions {
    let model_name = &*target.model.model_name;
    let adapter = target.model.adapter_kind;

    let needs_reasoning_swap = matches!(adapter, AdapterKind::OpenAIResp)
        || (matches!(adapter, AdapterKind::OpenAI) && is_openai_chat_reasoning_model(model_name));

    if !needs_reasoning_swap {
        return options.clone();
    }

    let mut opts = options.clone();
    opts.temperature = None;
    opts.top_p = None;
    if opts.reasoning_effort.is_none() {
        opts.reasoning_effort = Some(ReasoningEffort::Low);
    }
    opts
}

/// `genai 0.6` already auto-routes `gpt-5*`, `*-codex`, `*-pro` to the Responses API
/// adapter, where temperature is omitted by [`adjust_for_model`]. This heuristic
/// catches the *remaining* OpenAI chat-completions models that reject custom
/// `temperature`: `o1*`, `o3*`, `o4*`, `chatgpt-*` (plus `gpt-5*` as defense in depth
/// in case `genai` ever changes its routing).
fn is_openai_chat_reasoning_model(model_name: &str) -> bool {
    model_name.starts_with("o1")
        || model_name.starts_with("o3")
        || model_name.starts_with("o4")
        || model_name.starts_with("chatgpt-")
        || model_name.starts_with("gpt-5")
}

/// Tells `genai`'s `ServiceTargetResolver` whether to force a specific adapter.
#[derive(Clone, Copy)]
enum ForceAdapter {
    None,
    OpenAi,
}

/// Builds a `ServiceTargetResolver` that overrides endpoint + auth, optionally also
/// pinning the adapter (for the local llama-server, where we want OpenAI chat
/// completions regardless of the model name passed in).
fn make_resolver(endpoint: String, auth: AuthData, force_adapter: ForceAdapter) -> ServiceTargetResolver {
    let endpoint: Arc<str> = Arc::from(endpoint);
    let auth = Arc::new(auth);
    ServiceTargetResolver::from_resolver_fn(move |st: ServiceTarget| -> genai::resolver::Result<ServiceTarget> {
        let model = match force_adapter {
            ForceAdapter::OpenAi => ModelIden::new(AdapterKind::OpenAI, "local-model"),
            ForceAdapter::None => st.model,
        };
        Ok(ServiceTarget {
            endpoint: Endpoint::from_owned(endpoint.clone()),
            auth: (*auth).clone(),
            model,
        })
    })
}

/// Maps `genai`'s rich error tree to our flat [`AiError`]. Pattern-matches on the
/// known transport variants instead of grepping the `Display` output.
fn map_genai_error(e: genai::Error) -> AiError {
    use genai::Error as G;
    use genai::webc::Error as W;

    let webc = match &e {
        G::WebAdapterCall { webc_error, .. } | G::WebModelCall { webc_error, .. } => Some(webc_error),
        _ => None,
    };

    if let Some(webc) = webc {
        match webc {
            W::Reqwest(req) if req.is_timeout() => return AiError::Timeout,
            W::Reqwest(req) if req.is_connect() => return AiError::Unavailable,
            W::Reqwest(req) => return AiError::ServerError(format!("network error: {req}")),
            W::ResponseFailedStatus { status, body, .. } => {
                return AiError::ServerError(format!("HTTP {status}: {body}"));
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

/// Checks if the local llama-server is healthy. Pings `/health` directly — `genai`
/// doesn't expose this, and it's not a chat call anyway.
pub async fn health_check(port: u16) -> bool {
    let url = format!("http://127.0.0.1:{port}/health");

    let client = match reqwest::Client::builder().timeout(Duration::from_secs(2)).build() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("AI health_check: failed to build client: {e}");
            return false;
        }
    };

    match client.get(&url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                true
            } else {
                let body = response.text().await.unwrap_or_default();
                log::debug!("AI health_check: HTTP {status}, body: {body}");
                false
            }
        }
        Err(e) => {
            log::trace!("AI health_check: connection error (expected during startup): {e}");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ai_error_display() {
        assert_eq!(AiError::Unavailable.to_string(), "AI server unavailable");
        assert_eq!(AiError::Timeout.to_string(), "AI request timed out");
        assert_eq!(
            AiError::ServerError(String::from("bad")).to_string(),
            "AI server error: bad"
        );
        assert_eq!(
            AiError::ParseError(String::from("oops")).to_string(),
            "AI response parse error: oops"
        );
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
}
