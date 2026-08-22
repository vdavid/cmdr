//! What a turn needs resolved from live app state before it can run: the LLM slot, the
//! prompt budget, and the context envelope.
//!
//! **Both the rail and a wake come through here.** The rail resolves per send
//! (`commands::agent::ask_cmdr_send_message`); a wake resolves the same way on its own thread
//! (`agent::wake::runner`). Keeping one copy is the point: the budget is read fresh per turn,
//! so a wake reading a stale one would think with a different window than the rail, silently,
//! and nothing about the resulting thread would say why.
//!
//! ❌ These may not live in `commands/agent/`, which sits ABOVE `agent/`: a wake would have to
//! import upward to reach them.

use chrono::{FixedOffset, Local};
use tauri::{AppHandle, Manager};

use super::budget;
use super::context::{ContextEnvelope, EnvelopeAttachment, EnvelopeConnectivity, EnvelopeFreshness, EnvelopeVolume};
use super::runtime::{AgentErrorKind, now_secs};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::fake::FakeAgentLlm;
use crate::agent::llm::genai_impl::GenaiAgentLlm;
use crate::agent::llm::types::ProviderTag;
use crate::ai::client::AiBackend;
use crate::ai::llm_log::LlmLogContext;
use crate::mcp::PaneStateStore;
use crate::mcp::resources::volumes::{VolumeSummary, snapshot_volumes};

const LOG_TARGET: &str = "agent::chat";

/// A resolved-but-not-yet-built agent LLM. Resolution happens up front (to fail fast before
/// creating a thread), but the concrete `AgentLlm` is built only once the conversation id is
/// known, so the real backend can carry an [`LlmLogContext`] keyed on that conversation.
pub enum ResolvedAgentLlm {
    /// The real genai-backed LLM over the resolved `ai/` backend.
    Genai(AiBackend),
    /// The deterministic E2E fake (zero network; never logs — the tap is at the genai seam).
    Fake(FakeAgentLlm),
}

impl ResolvedAgentLlm {
    /// Builds the boxed `AgentLlm`, attaching a per-conversation logging context to the real
    /// backend so its requests/responses land under `llm-logs/thread-{id}/` (subject to the
    /// `logLlmCalls` setting). The fake bypasses the genai seam, so it logs nothing.
    pub fn into_llm(self, conversation_id: i64) -> Box<dyn AgentLlm> {
        match self {
            ResolvedAgentLlm::Genai(backend) => Box::new(GenaiAgentLlm::new(
                backend.with_log_context(LlmLogContext::agent_chat(conversation_id)),
            )),
            ResolvedAgentLlm::Fake(fake) => Box::new(fake),
        }
    }
}

/// Resolve the Ask Cmdr interactive slot into a ready LLM. The slot layers a dedicated
/// model choice (`askCmdr.interactiveModel`, read fresh) OVER the shared `ai/` provider
/// config (agent-spec D43): provider on/off, keys, and base URLs stay single-sourced in
/// `ai/`; only the model is slot-specific, so the bulk slot slots in later with no
/// migration (D49). An empty override uses the model the `ai/` provider is configured with.
/// Returns the backend plus the provider/model the cost meter records, or a typed error
/// when AI is off/unconfigured.
pub fn resolve_agent_llm(app: &AppHandle) -> Result<(ResolvedAgentLlm, ProviderTag, String), AgentErrorKind> {
    // E2E harness path: drive a deterministic scripted assistant with zero network, so the
    // rail's send-and-render can be tested without a provider. Guarded by an explicit env
    // flag so it never activates in a normal run.
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok((
            ResolvedAgentLlm::Fake(scripted_fake_llm()),
            ProviderTag::Local,
            "fake".to_string(),
        ));
    }
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    use crate::ai::manager::BackendResolution;
    match crate::ai::manager::resolve_backend_with_model(model_override.as_deref()) {
        BackendResolution::Ready(backend) => {
            let (provider, model) = provider_and_model(model_override.as_deref());
            Ok((ResolvedAgentLlm::Genai(backend), provider, model))
        }
        // "AI off", a blank cloud key, or a stopped local server all read the same to the
        // rail: nothing is configured to talk to. The settings surface disambiguates.
        BackendResolution::Off | BackendResolution::NotConfigured(_) | BackendResolution::UnknownProvider(_) => {
            Err(AgentErrorKind::NotConfigured)
        }
    }
}

/// The scripted turn the E2E fake streams: a short multi-chunk reply, so the test sees
/// streamed text land and a `Done`. Kept trivially deterministic.
fn scripted_fake_llm() -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    FakeAgentLlm::script(vec![ScriptedTurn::Say(vec![
        "Hi! ".to_string(),
        "I'm the ".to_string(),
        "test assistant.".to_string(),
    ])])
}

/// The provider tag + effective model label for cost metering. The model is the
/// interactive slot's override when set, else the live `ai/` cloud model; a cloud model is
/// tagged by its name prefix, matching `ai::client`'s adapter routing. Local uses its fixed
/// model name.
pub fn provider_and_model(model_override: Option<&str>) -> (ProviderTag, String) {
    if crate::ai::state::get_provider() == "local" {
        return (
            ProviderTag::Local,
            crate::ai::manager::get_ai_runtime_status().model_name,
        );
    }
    let model = match model_override {
        Some(m) if !m.is_empty() => m.to_string(),
        _ => {
            let (_key, _base, ai_model) = crate::ai::state::get_cloud_config();
            ai_model
        }
    };
    let provider = if model.starts_with("claude-") {
        ProviderTag::Anthropic
    } else if model.starts_with("gemini-") {
        ProviderTag::Gemini
    } else {
        ProviderTag::OpenAi
    };
    (provider, model)
}

/// The assembled-prompt token budget for the resolved slot, gathered here and decided in the
/// pure [`budget`] module. This layer supplies the two values the core may not read itself:
/// the user's `askCmdr.chatMemorySize` choice (read fresh per turn, so a change applies to the
/// NEXT message and never to a turn already in flight) and the local server's configured
/// window.
///
/// The resolution's source is logged, so a budget that came from a stale family table is
/// visible in the log rather than silently authoritative.
pub fn resolve_prompt_budget(
    app: &AppHandle,
    provider: ProviderTag,
    model: &str,
) -> Result<usize, budget::BudgetRefusal> {
    // The E2E fake answers as a LOCAL provider with no local server behind it, so the real
    // resolution would size its budget from `ai.localContextSize` — a setting the harness has no
    // reason to touch, and whose value would then decide what the usage gauge shows in every E2E
    // run. Give the fake its own realistic budget instead: the harness keeps mirroring a real
    // user's settings, and the gauge under test shows what a normal user's would.
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok(budget::DEFAULT_PROMPT_TOKEN_BUDGET);
    }
    let resolved = budget::resolve_prompt_budget(budget::BudgetInputs {
        provider,
        model,
        user_choice: crate::settings::load_ask_cmdr_chat_memory_size(app),
        local_context_tokens: crate::ai::state::get_local_context_size(),
    })?;
    log::debug!(
        target: LOG_TARGET,
        "prompt budget for {model}: a {}-token ceiling, from {}",
        resolved.prompt_tokens,
        resolved.source.label()
    );
    if resolved.over_known_window {
        log::info!(
            target: LOG_TARGET,
            "the chat memory size ({} tokens) is above the {}-token window we believe {model} has; \
             using it anyway, the provider decides",
            resolved.prompt_tokens,
            resolved.known_window_tokens.unwrap_or(0)
        );
    }
    Ok(resolved.prompt_tokens)
}

/// Capture the context envelope from live app state (snapshot-at-send). Focused pane path
/// resolves from the focused SIDE's directory; volumes come from `snapshot_volumes`;
/// `attachments` are the references the caller attached for this turn (path + kind only).
///
/// With no main window (a wake on a routine-launched macOS app), `PaneStateStore` is absent
/// and the pane fields come back empty. That is the honest answer, ❌ not a reason to skip the
/// capture: the volume list and the clock are still worth having.
pub async fn capture_envelope<R: tauri::Runtime>(
    app: &AppHandle<R>,
    attachments: Vec<EnvelopeAttachment>,
    denied_names: Vec<String>,
    rename_batch_files: usize,
) -> ContextEnvelope {
    let (focused_pane_path, cursor_item, selection_count) = match app.try_state::<PaneStateStore>() {
        Some(store) => {
            let side = store.get_focused_pane();
            let pane = if side == "right" {
                store.get_right()
            } else {
                store.get_left()
            };
            let path = (!pane.path.is_empty()).then(|| pane.path.clone());
            let cursor = pane.files.get(pane.cursor_index).map(|f| f.name.clone());
            (path, cursor, pane.selected_indices.len() as u32)
        }
        None => (None, None, 0),
    };
    let volumes = snapshot_volumes().await.iter().map(to_envelope_volume).collect();
    ContextEnvelope {
        captured_at: now_secs(),
        focused_pane_path,
        cursor_item,
        selection_count,
        volumes,
        attachments,
        denied_names,
        rename_batch_files,
    }
}

/// Map a live volume summary to the envelope's pure mirror. The freshness/connectivity
/// values are OUR OWN stable tokens (the same ones `list_volumes` emits), parsed by exact
/// match like a `from_token` — not error/state-string classification.
fn to_envelope_volume(summary: &VolumeSummary) -> EnvelopeVolume {
    let freshness = match summary.index_status {
        Some("fresh") => EnvelopeFreshness::Fresh,
        Some("scanning") => EnvelopeFreshness::Scanning,
        Some("stale") => EnvelopeFreshness::Stale,
        _ => EnvelopeFreshness::Off,
    };
    let connectivity = match summary.smb_connection_state {
        Some("direct") => Some(EnvelopeConnectivity::Direct),
        Some("os_mount") => Some(EnvelopeConnectivity::OsMount),
        Some("disconnected") => Some(EnvelopeConnectivity::Disconnected),
        _ => None,
    };
    EnvelopeVolume {
        name: summary.name.clone(),
        freshness,
        connectivity,
    }
}

/// The local UTC offset now, for rendering timestamps in the user's timezone.
pub fn local_offset() -> FixedOffset {
    *Local::now().offset()
}
