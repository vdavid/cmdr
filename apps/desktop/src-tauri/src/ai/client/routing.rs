//! Which genai adapter a model and host get, and the per-model option fixups that ride along.
//! Pure functions: `AiBackend` calls them when it's built and on every request, and
//! `client_unit_test.rs` pins them without a network.

use genai::ServiceTarget;
use genai::adapter::AdapterKind;
use genai::chat::{ChatOptions, ReasoningEffort};

/// Maps a BYOK model name to the `genai` model identifier whose namespace picks the right adapter.
///
/// `genai` infers the adapter from the model name and falls back to **Ollama** for anything it
/// doesn't recognize — so a bare `openai/gpt-oss-20b` (Groq), `deepseek-chat`, or
/// `google/gemma-…:free` (OpenRouter) would POST to Ollama's `/api/chat` against an OpenAI endpoint
/// and 404. Every cloud provider we support except Anthropic and Gemini speaks the OpenAI
/// chat-completions wire format, so we force the `openai::` namespace for all of them. Anthropic
/// (`claude-*`) and Gemini (`gemini-*`) keep their native adapters; real OpenAI model families
/// (`gpt-*` / `o1*` / `o3*` / `o4*` / `chatgpt-*`) are left alone so `genai` can auto-route the
/// `gpt-5*` / `*-codex` / `*-pro` Responses-API models.
///
/// Two hosts get genai's dedicated chat-completions dialect instead of plain `openai::`, because
/// they carry reasoning differently and the model name can't tell (see [`chat_dialect_for_host`]).
pub(super) fn remote_model_iden(model: &str, endpoint_host: Option<&str>) -> String {
    // Google's OpenAI-compatible model list names its models `models/gemini-…`, which an earlier
    // key check offered verbatim. The bare id is what the native adapter routes on.
    let model = match model.strip_prefix("models/") {
        Some(bare) if bare.starts_with("gemini-") => bare,
        _ => model,
    };
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
        let namespace = endpoint_host.and_then(chat_dialect_for_host).unwrap_or("openai");
        format!("{namespace}::{model}")
    }
}

/// The genai namespace for an OpenAI-compatible host whose reasoning wire format differs from
/// plain chat-completions, else `None`. DeepSeek's adapter toggles thinking via a `thinking`
/// body field (its default is on, and a tool loop 400s unless `reasoning_content` comes back);
/// Mistral's 422s on `reasoning_content` and takes the reasoning back as a leading `thinking`
/// chunk. Matched on the endpoint host, never a model-name guess: the same open-weight model is
/// served by aggregators that speak plain chat-completions.
pub(super) fn chat_dialect_for_host(host: &str) -> Option<&'static str> {
    match host {
        "api.deepseek.com" => Some("deepseek"),
        "api.mistral.ai" => Some("mistral"),
        _ => None,
    }
}

/// Whether a chat-completions host takes an assistant turn's reasoning text back. Only hosts
/// verified to accept it, because the two ways to be wrong aren't equal: DeepSeek 400s a tool
/// loop that DOESN'T send it, and Groq 400s one that DOES ("property 'reasoning_content' is
/// unsupported", verified on `openai/gpt-oss-120b`, live tool loop, 2026-10-27). Every other
/// host keeps what it always got, no reasoning, which costs continuity, never the loop.
/// OpenRouter documents both `reasoning_content` and its `reasoning_details` blocks as accepted
/// back, and needs the blocks for a signed or encrypted model's continuity.
/// Evidence per host: `agent/llm/DETAILS.md` § Reasoning round-trip.
pub(super) fn replays_chat_reasoning_for_host(host: &str) -> bool {
    matches!(
        host,
        "api.deepseek.com" | "api.mistral.ai" | "api.fireworks.ai" | "openrouter.ai"
    )
}

/// Whether this host and model cache only when the request carries a top-level `cache_control`:
/// OpenRouter routing to a Claude model. Claude caches nothing unmarked, and genai's OpenAI
/// adapter drops the per-message marker, so without this every round re-bills the whole chat at
/// full price (#801). The top-level field is OpenRouter's automatic caching for Claude: the
/// breakpoint lands on the last cacheable block and moves forward as the thread grows (verified
/// on OpenRouter's prompt-caching guide, "Anthropic Claude", 2026-10-29). Other OpenRouter models
/// cache on their own or take per-block markers only, so the field stays off for them.
pub(super) fn wants_top_level_cache_marker(host: &str, model: &str) -> bool {
    host == "openrouter.ai" && model.trim_start_matches('~').starts_with("anthropic/")
}

/// Per-model option fixup: reasoning-class models reject `temperature`. Returns a
/// modified clone of `options` when needed; otherwise hands back a clone unchanged.
pub(super) fn adjust_for_model(options: &ChatOptions, target: &ServiceTarget) -> ChatOptions {
    let model_name = &*target.model.model_name;
    let adapter = target.model.adapter_kind;

    // Anthropic gets no sampling knobs at all. Every Claude from 4.7 on answers 400
    // "`temperature` is deprecated for this model" to a non-default `temperature` or `top_p`
    // (verified 2026-10-26, `curl` against `claude-{opus,sonnet,haiku}-5-5`), older ones 400 on
    // the pair, and API version `2026-10-21` drops both for every model. Older Claude models
    // lose a lower temperature, which is the trade Anthropic's own migration guide makes.
    if matches!(adapter, AdapterKind::Anthropic) {
        let mut opts = options.clone();
        opts.temperature = None;
        opts.top_p = None;
        return opts;
    }

    let needs_reasoning_swap = matches!(adapter, AdapterKind::OpenAIResp)
        || (matches!(adapter, AdapterKind::OpenAI) && is_openai_chat_reasoning_model(model_name));

    if !needs_reasoning_swap {
        return options.clone();
    }

    let mut opts = options.clone();
    opts.temperature = None;
    opts.top_p = None;
    // An explicit `capture_reasoning_content: false` is the caller saying the model doesn't
    // reason (the agent's `CallSettings::model_reasons`); an effort sent to it is a 400.
    if opts.reasoning_effort.is_none() && opts.capture_reasoning_content != Some(false) {
        opts.reasoning_effort = Some(ReasoningEffort::Low);
    }
    opts
}

/// `genai` already auto-routes `gpt-5*`, `*-codex`, `*-pro` to the Responses API
/// adapter, where temperature is omitted by [`adjust_for_model`]. This heuristic
/// catches the *remaining* OpenAI chat-completions models that reject custom
/// `temperature`: `o1*`, `o3*`, `o4*`, `chatgpt-*` (plus `gpt-5*` as defense in depth
/// in case `genai` ever changes its routing).
///
/// Keep the o-series arms even after OpenAI's own API drops them (`o3`, the last one, retires
/// 2026-12-11): Azure OpenAI deployments, OpenRouter, and OpenAI-compatible proxies keep
/// serving o-series ids long after, and a bare `o3*` id on any of them would get
/// `temperature` and a 400 without these arms. No live smoke covers them; the
/// offline `o3_chat_reasoning_model_omits_temperature` mock does.
pub(super) fn is_openai_chat_reasoning_model(model_name: &str) -> bool {
    model_name.starts_with("o1")
        || model_name.starts_with("o3")
        || model_name.starts_with("o4")
        || model_name.starts_with("chatgpt-")
        || model_name.starts_with("gpt-5")
}
