//! HTTP client for AI chat completions.
//!
//! Wraps [`genai`](https://crates.io/crates/genai), which normalizes the wire format across
//! OpenAI / OpenAI Responses / Anthropic / Gemini / xAI / Groq / DeepSeek / Ollama / etc.
//!
//! Two backend constructors:
//! - [`AiBackend::local`]: forces the OpenAI adapter at `http://127.0.0.1:<port>/v1/`.
//! - [`AiBackend::remote`]: BYOK. `base_url` overrides the endpoint; the adapter comes from the
//!   model name via [`remote_model_iden`] — `claude-*`/`gemini-*` keep their native protocols,
//!   `gpt-*`/`o*`/`chatgpt-*` use OpenAI (incl. the Responses-API auto-routing), and every other
//!   OpenAI-compatible provider (Groq, OpenRouter, DeepSeek, …) is forced onto OpenAI
//!   chat-completions so `genai` doesn't mis-route it to Ollama.

use std::fmt::Display;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::stream::{BoxStream, StreamExt};
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatRole, ChatStreamEvent, ReasoningEffort, StopReason, StreamEnd,
};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{Client, ModelIden, ServiceTarget};
use serde_json::{Value, json};

use super::llm_log::{self, CallLog, Fidelity, LlmLogContext, RequestInfo, ResponseInfo};

/// A configured AI backend ready to receive [`chat_completion`] calls.
///
/// Bundles a long-lived [`genai::Client`] with the model name to pass it. Build once
/// (cheap, but still a few allocations + one resolver-closure box) and reuse for the
/// lifetime of the configured provider.
pub struct AiBackend {
    client: Client,
    model: String,
    /// The logging context for this backend's calls, or `None` to skip logging (the default;
    /// unit-test backends and any path that hasn't opted in). Set by the caller via
    /// [`AiBackend::with_log_context`] once it knows which feature (and session) the call
    /// serves. The actual write is still gated on the `logLlmCalls` setting inside
    /// [`crate::ai::llm_log`], so a context alone never forces a write.
    log_ctx: Option<LlmLogContext>,
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
            log_ctx: None,
        }
    }

    /// Remote / cloud provider. The adapter is chosen from the model name: `claude-*` and
    /// `gemini-*` use their native protocols, `gpt-*` / `o*` / `chatgpt-*` use OpenAI (with
    /// `genai`'s gpt-5*/codex/pro → Responses-API auto-routing), and EVERYTHING ELSE is forced
    /// onto the OpenAI chat-completions adapter (see [`remote_model_iden`]).
    pub fn remote(api_key: String, base_url: String, model: String) -> Self {
        // Without a trailing `/` the `Url::join` quirk above silently drops `/v1`.
        let endpoint = if base_url.ends_with('/') {
            base_url
        } else {
            format!("{base_url}/")
        };
        let resolver = make_resolver(endpoint, AuthData::from_single(api_key), ForceAdapter::None);
        let client = Client::builder().with_service_target_resolver(resolver).build();
        // The resolver only overrides endpoint + auth; adapter dispatch happens from the model
        // name BEFORE the resolver runs (same reason `local` uses the `openai::` namespace).
        Self {
            client,
            model: remote_model_iden(&model),
            log_ctx: None,
        }
    }

    /// Attaches a logging context so this backend's requests and responses are recorded to
    /// disk (subject to the `logLlmCalls` setting). Callers set it once they know which
    /// feature and session the call serves — the agent per conversation, the one-shot helpers
    /// per job. Without it, this backend logs nothing.
    pub fn with_log_context(mut self, ctx: LlmLogContext) -> Self {
        self.log_ctx = Some(ctx);
        self
    }

    /// Resolves the provider adapter for this backend's configured model, without a
    /// network call. The agent LLM (`crate::agent::llm`) uses it to pick a
    /// per-provider reasoning posture before building request options.
    pub(crate) async fn resolve_adapter(&self) -> Result<AdapterKind, AiError> {
        let target = self
            .client
            .resolve_service_target(&self.model)
            .await
            .map_err(map_genai_error)?;
        Ok(target.model.adapter_kind)
    }

    /// Runs a full tool-capable streaming chat request and returns the raw `genai`
    /// stream, so the caller maps the events into its own delta type. Applies the
    /// same per-model option fixups as [`chat_completion_stream`] (reasoning-class
    /// models get `temperature`/`top_p` stripped).
    ///
    /// The prompt-only helpers above can't express a multipart tool loop (tool
    /// calls, tool responses, reasoning parts), so the agent's `AgentLlm` genai impl
    /// builds the `ChatRequest` itself and drives it through here — reusing this
    /// backend's adapter routing and endpoint resolution rather than duplicating them.
    pub(crate) async fn exec_chat_stream_request(
        &self,
        request: ChatRequest,
        options: &ChatOptions,
    ) -> Result<BoxStream<'static, genai::Result<ChatStreamEvent>>, AiError> {
        let target = self
            .client
            .resolve_service_target(&self.model)
            .await
            .map_err(map_genai_error)?;
        let effective_options = adjust_for_model(options, &target);
        // Log the assembled request (system + tools + history + envelope) before dispatch,
        // then tap the returned stream to log the assembled response on `End`.
        let logged = self.begin_request_log(&target, &request, Fidelity::RequestStruct);
        let res = self
            .client
            .exec_chat_stream(&self.model, request, Some(&effective_options))
            .await
            .map_err(map_genai_error)?;
        Ok(wrap_stream_with_response_log(res.stream, logged))
    }

    /// Serializes and records an outgoing request when this backend has a logging context and
    /// the `logLlmCalls` setting is on. Returns a handle + start time for the matching response
    /// log, or `None` when nothing should be logged. Never fails the call.
    fn begin_request_log(
        &self,
        target: &ServiceTarget,
        request: &ChatRequest,
        fidelity: Fidelity,
    ) -> Option<(CallLog, Instant)> {
        let ctx = self.log_ctx.as_ref()?;
        let info = RequestInfo {
            provider: provider_label(target.model.adapter_kind),
            model: target.model.model_name.to_string(),
            adapter_kind: format!("{:?}", target.model.adapter_kind),
            fidelity,
            user_message: latest_user_message(request).unwrap_or_default(),
        };
        let body = serde_json::to_value(request).unwrap_or(Value::Null);
        let handle = llm_log::log_request(ctx, info, body)?;
        Some((handle, Instant::now()))
    }
}

// region: --- LLM call logging helpers

/// Wraps a genai chat stream so the assembled response (streamed text, or the captured
/// content when the caller set capture options, plus stop reason and usage) is logged on
/// `End`. When `logged` is `None` (logging off or no context), the stream is returned
/// untouched with zero overhead. A stream dropped before `End` (cancellation) writes no
/// response file, which honestly reflects that nothing came back.
fn wrap_stream_with_response_log(
    stream: genai::chat::ChatStream,
    logged: Option<(CallLog, Instant)>,
) -> BoxStream<'static, genai::Result<ChatStreamEvent>> {
    let Some((handle, started)) = logged else {
        return stream.boxed();
    };
    let mut handle = Some(handle);
    let mut accumulated = String::new();
    stream
        .inspect(move |item| {
            if let Ok(event) = item {
                match event {
                    ChatStreamEvent::Chunk(chunk) => accumulated.push_str(&chunk.content),
                    ChatStreamEvent::End(end) => {
                        if let Some(handle) = handle.take() {
                            handle.log_response(
                                response_info_from_end(end, started),
                                response_body_from_end(end, &accumulated),
                            );
                        }
                    }
                    _ => {}
                }
            }
        })
        .boxed()
}

/// The assistant's assembled reply for the response body: the captured content when present
/// (the agent sets capture options, so this holds text + tool calls), else the text we
/// accumulated from the chunk stream (the legacy streaming helper sets no capture options).
fn response_body_from_end(end: &StreamEnd, accumulated: &str) -> Value {
    let content = end
        .captured_content
        .as_ref()
        .and_then(|content| serde_json::to_value(content).ok())
        .unwrap_or_else(|| json!({ "text": accumulated }));
    json!({ "content": content, "response_id": end.captured_response_id })
}

fn response_info_from_end(end: &StreamEnd, started: Instant) -> ResponseInfo {
    ResponseInfo {
        prompt_tokens: end.captured_usage.as_ref().and_then(|u| u.prompt_tokens).map(clamp_u32),
        completion_tokens: end
            .captured_usage
            .as_ref()
            .and_then(|u| u.completion_tokens)
            .map(clamp_u32),
        stop_reason: end.captured_stop_reason.as_ref().map(stop_reason_label),
        latency_ms: started.elapsed().as_millis() as u64,
        fidelity: Fidelity::Assembled,
    }
}

fn response_info_from_chat_response(res: &genai::chat::ChatResponse, started: Instant) -> ResponseInfo {
    ResponseInfo {
        prompt_tokens: res.usage.prompt_tokens.map(clamp_u32),
        completion_tokens: res.usage.completion_tokens.map(clamp_u32),
        stop_reason: res.stop_reason.as_ref().map(stop_reason_label),
        latency_ms: started.elapsed().as_millis() as u64,
        fidelity: Fidelity::Parsed,
    }
}

/// The latest user turn's text, for the log slug. Reads our own assembled request, so this is
/// not error/state classification (no `no-string-matching` concern).
fn latest_user_message(request: &ChatRequest) -> Option<String> {
    request
        .messages
        .iter()
        .rev()
        .find(|message| matches!(message.role, ChatRole::User))
        .and_then(|message| message.content.joined_texts())
}

/// A stable provider label for the `gen_ai.system` metadata field.
fn provider_label(adapter: AdapterKind) -> String {
    match adapter {
        AdapterKind::Anthropic => "anthropic".to_string(),
        AdapterKind::Gemini => "gemini".to_string(),
        AdapterKind::OpenAIResp => "openai_responses".to_string(),
        AdapterKind::OpenAI => "openai".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

/// A stable, lowercase stop-reason label for metadata (a descriptive log field, not control
/// flow — the runtime classifies stop reasons on typed variants, not this string).
fn stop_reason_label(reason: &StopReason) -> String {
    match reason {
        StopReason::Completed(_) => "completed",
        StopReason::MaxTokens(_) => "max_tokens",
        StopReason::ToolCall(_) => "tool_call",
        StopReason::ContentFilter(_) => "content_filter",
        StopReason::StopSequence(_) => "stop_sequence",
        StopReason::Other(_) => "other",
    }
    .to_string()
}

fn clamp_u32(count: i32) -> u32 {
    count.max(0) as u32
}

// endregion: --- LLM call logging helpers

/// Maps a BYOK model name to the `genai` model identifier whose namespace picks the right adapter.
///
/// `genai` infers the adapter from the model name and falls back to **Ollama** for anything it
/// doesn't recognize — so a bare `llama-3.1-8b-instant` (Groq), `deepseek-chat`, or
/// `google/gemma-…:free` (OpenRouter) would POST to Ollama's `/api/chat` against an OpenAI endpoint
/// and 404. Every cloud provider we support except Anthropic and Gemini speaks the OpenAI
/// chat-completions wire format, so we force the `openai::` namespace for all of them. Anthropic
/// (`claude-*`) and Gemini (`gemini-*`) keep their native adapters; real OpenAI model families
/// (`gpt-*` / `o1*` / `o3*` / `o4*` / `chatgpt-*`) are left alone so `genai` can auto-route the
/// `gpt-5*` / `*-codex` / `*-pro` Responses-API models.
fn remote_model_iden(model: &str) -> String {
    let native_or_openai = model.starts_with("claude-")
        || model.starts_with("gemini-")
        || model.starts_with("gpt-")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
        || model.starts_with("chatgpt-");
    if native_or_openai {
        model.to_string()
    } else {
        format!("openai::{model}")
    }
}

#[derive(Debug, Clone)]
pub enum AiError {
    /// Server is unreachable (DNS / connect refused / no route).
    Unavailable,
    /// Request timed out (server too slow, or local server unhealthy).
    Timeout,
    /// The provider rejected the API key (HTTP 401 / 403).
    AuthFailed(String),
    /// The provider is rate-limiting requests or the account is out of quota (HTTP 429).
    RateLimited(String),
    /// The call succeeded but the model produced no visible text. Common on reasoning models
    /// when `max_tokens` is fully consumed by reasoning before any answer is emitted.
    EmptyResponse,
    /// Server returned some other HTTP error or otherwise misbehaved.
    ServerError(String),
    /// Couldn't parse the response body.
    ParseError(String),
}

impl Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => write!(f, "AI server unavailable"),
            Self::Timeout => write!(f, "AI request timed out"),
            Self::AuthFailed(msg) => write!(f, "AI provider rejected the API key: {msg}"),
            Self::RateLimited(msg) => write!(f, "AI provider is rate-limiting or out of quota: {msg}"),
            Self::EmptyResponse => write!(f, "AI returned no text"),
            Self::ServerError(msg) => write!(f, "AI server error: {msg}"),
            Self::ParseError(msg) => write!(f, "AI response parse error: {msg}"),
        }
    }
}

/// Sends a chat completion request to an AI backend.
///
/// `options` are the caller-supplied generation knobs. We auto-strip `temperature`/
/// `top_p` and substitute [`ReasoningEffort::Low`] when the resolved adapter+model
/// is reasoning-class (see [`is_openai_chat_reasoning_model`]).
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

    let logged = backend.begin_request_log(&target, &req, Fidelity::RequestStruct);
    let res = backend
        .client
        .exec_chat(&backend.model, req, Some(&effective_options))
        .await
        .map_err(map_genai_error)?;

    if let Some((handle, started)) = logged {
        handle.log_response(
            response_info_from_chat_response(&res, started),
            serde_json::to_value(&res).unwrap_or(Value::Null),
        );
    }

    let text = res
        .first_text()
        // Common on reasoning models (`gpt-5*`, `o3*`, `*-pro`) when `max_tokens` gets
        // fully consumed by reasoning before any `output_text` is emitted. The HTTP call
        // succeeded; there's just no visible answer to return. Typed so callers can tell
        // the user to pick a simpler model or raise the token budget.
        .ok_or(AiError::EmptyResponse)?
        .to_owned();

    log::trace!("AI chat_completion: extracted content: {text}");
    Ok(text)
}

/// Hard ceiling for the empty-response retry's token budget, so a pathological model can't make
/// us request an unbounded (and expensive) completion.
const EMPTY_RETRY_TOKEN_CEILING: u32 = 2000;
/// Multiplier applied to `max_tokens` on the empty-response retry.
const EMPTY_RETRY_TOKEN_FACTOR: u32 = 4;

/// Returns a clone of `options` with `max_tokens` multiplied by `factor` (capped at `ceiling`).
/// Pure, so the budget math is unit-tested. A missing `max_tokens` defaults to the ceiling on
/// retry — if the caller didn't cap it, the first attempt already had room, so go straight to the
/// ceiling rather than guessing a base.
fn with_bumped_max_tokens(options: &ChatOptions, factor: u32, ceiling: u32) -> ChatOptions {
    let mut opts = options.clone();
    let bumped = match opts.max_tokens {
        Some(current) => current.saturating_mul(factor).min(ceiling),
        None => ceiling,
    };
    opts.max_tokens = Some(bumped);
    opts
}

/// Like [`chat_completion`], but retries ONCE with a larger token budget when the model returns
/// no visible text ([`AiError::EmptyResponse`]).
///
/// This is the provider-agnostic guard against reasoning models (`gpt-5*`, `o*`, DeepSeek
/// `*-reasoner`, Qwen `qwq`, …) spending the whole budget on hidden reasoning before emitting an
/// answer. Rather than maintain a model-name list that's never complete, we react to the symptom:
/// an empty answer means "retry with room to think AND answer". One retry only — if it's still
/// empty, the budget isn't the problem and we surface `EmptyResponse` so the UI can suggest a
/// faster model. Every other error (and a success) passes straight through with no extra call.
pub async fn chat_completion_with_empty_retry(
    backend: &AiBackend,
    system_prompt: &str,
    user_prompt: &str,
    options: &ChatOptions,
) -> Result<String, AiError> {
    match chat_completion(backend, system_prompt, user_prompt, options).await {
        Err(AiError::EmptyResponse) => {
            let bumped = with_bumped_max_tokens(options, EMPTY_RETRY_TOKEN_FACTOR, EMPTY_RETRY_TOKEN_CEILING);
            log::info!(
                "AI chat_completion: empty response, retrying once with max_tokens={:?} (was {:?})",
                bumped.max_tokens,
                options.max_tokens
            );
            chat_completion(backend, system_prompt, user_prompt, &bumped).await
        }
        other => other,
    }
}

/// Streams a chat completion. Returns a boxed stream of content chunks.
///
/// Same per-model option fixups as [`chat_completion`] (reasoning models get
/// `temperature`/`top_p` stripped and `ReasoningEffort::Low` substituted). Reasoning,
/// thought-signature, and tool-call chunks are filtered out; callers only see the
/// visible text content. Stream ends when `genai` emits `End` or errors; an empty
/// stream (zero chunks) is valid and matches the same graceful-degradation contract
/// as `chat_completion`'s "AI returned no text" case.
///
/// Cancellation: drop the returned stream. The `genai::ChatStreamResponse`'s reqwest
/// body is closed, billing stops on cloud providers, local-LLM compute is freed.
pub async fn chat_completion_stream(
    backend: &AiBackend,
    system_prompt: &str,
    user_prompt: &str,
    options: &ChatOptions,
) -> Result<BoxStream<'static, Result<String, AiError>>, AiError> {
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
        "AI chat_completion_stream: opening stream (adapter={:?}, model={})",
        target.model.adapter_kind,
        &*target.model.model_name
    );

    let logged = backend.begin_request_log(&target, &req, Fidelity::RequestStruct);
    let res = backend
        .client
        .exec_chat_stream(&backend.model, req, Some(&effective_options))
        .await
        .map_err(map_genai_error)?;

    // Tap the response side (logs the assembled reply on `End`), then map
    // ChatStreamEvent → Option<String>: keep only visible content; drop reasoning,
    // thought-signature, tool-call chunks; pass through errors mapped to AiError.
    let stream = wrap_stream_with_response_log(res.stream, logged).filter_map(|item| async move {
        match item {
            Ok(ChatStreamEvent::Chunk(chunk)) => Some(Ok(chunk.content)),
            Ok(ChatStreamEvent::Start | ChatStreamEvent::End(_)) => None,
            Ok(ChatStreamEvent::ReasoningChunk(_) | ChatStreamEvent::ThoughtSignatureChunk(_)) => None,
            Ok(ChatStreamEvent::ToolCallChunk(_)) => None,
            Err(e) => Some(Err(map_genai_error(e))),
        }
    });

    Ok(stream.boxed())
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

/// Classifies a provider HTTP error status into the right [`AiError`] so the frontend can
/// show a specific toast (key rejected vs. out of quota vs. generic server error). Branches
/// on the numeric status, never the message body. 429 covers both rate-limiting and
/// OpenAI's `insufficient_quota`; 401/403 is a rejected key.
fn ai_error_for_status(status: u16, detail: String) -> AiError {
    match status {
        401 | 403 => AiError::AuthFailed(detail),
        429 => AiError::RateLimited(detail),
        _ => AiError::ServerError(detail),
    }
}

/// Builds the display detail for a failed provider response: `HTTP <status>: <message>`.
/// The message is the JSON body's `error.message` when present (OpenAI, OpenRouter,
/// Anthropic, and Gemini all put the human sentence there), else the raw body, capped so
/// an HTML error page (a proxy, Cloudflare) can't flood the UI or the logs. Display only,
/// never control flow: classification stays on the numeric status (`ai_error_for_status`),
/// per `no-string-matching`.
fn provider_error_detail(status: impl Display, body: &str) -> String {
    const MAX_CHARS: usize = 400;
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| Some(value.get("error")?.get("message")?.as_str()?.to_string()))
        .unwrap_or_else(|| body.to_string());
    let message = message.trim();
    if message.chars().count() > MAX_CHARS {
        let capped: String = message.chars().take(MAX_CHARS).collect();
        format!("HTTP {status}: {capped}…")
    } else {
        format!("HTTP {status}: {message}")
    }
}

/// Maps `genai`'s rich error tree to our flat [`AiError`]. Pattern-matches on the
/// known transport variants instead of grepping the `Display` output. Shared with
/// the agent LLM (`crate::agent::llm`), which maps `AiError` on to its own typed
/// error, so the status-based classification lives in one place.
pub(crate) fn map_genai_error(e: genai::Error) -> AiError {
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
                return ai_error_for_status(status.as_u16(), provider_error_detail(status, body));
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

/// Checks if the local llama-server is healthy. Pings `/health` directly; `genai`
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
        assert_eq!(AiError::EmptyResponse.to_string(), "AI returned no text");
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
        assert_eq!(
            remote_model_iden("llama-3.1-8b-instant"),
            "openai::llama-3.1-8b-instant"
        );
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
        assert!(matches!(ai_error_for_status(404, "x".into()), AiError::ServerError(_)));
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
}
