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

use super::context::{ContextEnvelope, EnvelopeAttachment, EnvelopeConnectivity, EnvelopeFreshness, EnvelopeVolume};
use super::runtime::now_secs;
use super::stream::AgentErrorKindView;
use super::{budget, effort};
use crate::agent::llm::AgentLlm;
use crate::agent::llm::fake::FakeAgentLlm;
use crate::agent::llm::genai_impl::GenaiAgentLlm;
use crate::agent::llm::types::{CallSettings, ProviderTag};
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

/// Who is asking for an LLM. The two slots share one provider, one key, and one model, so
/// this changes NOTHING on the real path.
///
/// ⚠️ It exists for the E2E fake, which must not answer a wake the way it answers the rail: the
/// rail's specs assert on the scripted reply's exact text, so one shared script would make a
/// wake's thread indistinguishable from a rail thread and would tie the two suites together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSlot {
    /// A message the user sent in the chat rail.
    Rail,
    /// A turn the wake loop started on its own.
    Wake,
}

/// Why the slot can't resolve to an LLM. Decided before a thread exists, so it never reaches the
/// turn runtime; the command layer maps it onto its wire refusal ([`SlotRefusal::view`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotRefusal {
    /// "AI off", a blank cloud key, a stopped local server, or an unknown provider: nothing is
    /// configured to talk to. The settings surface disambiguates.
    NotConfigured,
    /// Cloud AI is picked, and the user hasn't allowed it (`ai::cloud_consent`).
    NoCloudConsent,
    /// The organization's managed policy refuses the provider or its host.
    Managed(crate::managed_policy::ManagedAiRefusal),
}

impl SlotRefusal {
    /// The wire kind the rail renders. A method here rather than a `From` beside the view type:
    /// `stream` must not depend on `session`, or the two close a module cycle through `runtime`.
    pub fn view(self) -> AgentErrorKindView {
        match self {
            SlotRefusal::NotConfigured => AgentErrorKindView::NotConfigured,
            SlotRefusal::NoCloudConsent => AgentErrorKindView::NoCloudConsent,
            SlotRefusal::Managed(_) => AgentErrorKindView::ManagedByOrganization,
        }
    }

    /// The organization's reason, when the policy is what refused: the wire kind is one unit
    /// variant, so the reason rides beside it for the rail's copy map.
    pub fn managed(self) -> Option<crate::managed_policy::ManagedAiRefusal> {
        match self {
            SlotRefusal::Managed(refusal) => Some(refusal),
            SlotRefusal::NotConfigured | SlotRefusal::NoCloudConsent => None,
        }
    }
}

/// Why the rail's send gate refused: Ask Cmdr's own switch, or the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendGateRefusal {
    AskCmdrOff,
    Slot(SlotRefusal),
}

impl SendGateRefusal {
    /// The wire kind the rail renders.
    pub fn view(self) -> AgentErrorKindView {
        match self {
            SendGateRefusal::AskCmdrOff => AgentErrorKindView::AskCmdrOff,
            SendGateRefusal::Slot(refusal) => refusal.view(),
        }
    }

    /// The organization's reason, when the policy refused.
    pub fn managed(self) -> Option<crate::managed_policy::ManagedAiRefusal> {
        match self {
            SendGateRefusal::AskCmdrOff => None,
            SendGateRefusal::Slot(refusal) => refusal.managed(),
        }
    }
}

/// Resolve the Ask Cmdr interactive slot into a ready LLM. The slot layers a dedicated
/// model choice (`askCmdr.interactiveModel`, read fresh) OVER the shared `ai/` provider
/// config (agent decision D43): provider on/off, keys, and base URLs stay single-sourced in
/// `ai/`; only the model is slot-specific, so the bulk slot slots in later with no
/// migration (D49). An empty override uses the model the `ai/` provider is configured with.
/// Returns the backend plus the provider/model the cost meter records, or a typed refusal
/// when AI is off, unconfigured, or cloud AI isn't allowed.
pub fn resolve_agent_llm(
    app: &AppHandle,
    slot: AgentSlot,
) -> Result<(ResolvedAgentLlm, ProviderTag, String), SlotRefusal> {
    // E2E harness path: drive a deterministic scripted assistant with zero network, so the
    // rail's send-and-render can be tested without a provider. Guarded by an explicit env
    // flag so it never activates in a normal run.
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok((
            ResolvedAgentLlm::Fake(scripted_fake_llm(slot)),
            ProviderTag::Local,
            "fake".to_string(),
        ));
    }
    let model_override = crate::settings::load_ask_cmdr_interactive_model(app);
    let backend = slot_backend(crate::ai::manager::resolve_backend_with_model(
        app,
        model_override.as_deref(),
    ))?;
    let (provider, model) = provider_and_model(model_override.as_deref());
    Ok((ResolvedAgentLlm::Genai(backend), provider, model))
}

/// The slot's reading of a backend resolution. Pure, so the mapping is testable without an app.
fn slot_backend(resolution: crate::ai::manager::BackendResolution) -> Result<AiBackend, SlotRefusal> {
    use crate::ai::manager::BackendResolution;
    match resolution {
        BackendResolution::Ready(backend) => Ok(backend),
        BackendResolution::Off | BackendResolution::NotConfigured(_) | BackendResolution::UnknownProvider(_) => {
            Err(SlotRefusal::NotConfigured)
        }
        // Refused before a thread exists: nothing reaches a cloud service the user didn't allow.
        BackendResolution::NoCloudConsent => Err(SlotRefusal::NoCloudConsent),
        BackendResolution::Managed(refusal) => Err(SlotRefusal::Managed(refusal)),
    }
}

/// The rail's send gate, decided before a thread or an LLM exists: Ask Cmdr's own switch first,
/// then the slot (which carries the cloud consent gate, `ai::manager::resolve_backend`).
/// `resolve` never runs when the person switched Ask Cmdr off, so it never looks at the provider.
/// A managed off (`DisableAI` pins the switch) goes on to the slot, whose resolution refuses with
/// the organization's own reason: the person didn't turn Ask Cmdr off, so "Ask Cmdr is off" would
/// be the wrong answer. `ask_cmdr_send_message` is the caller; a wake has its own gates
/// (`wake::readiness`).
pub fn admit_send<T>(
    ask_cmdr: crate::settings::AskCmdrSwitch,
    resolve: impl FnOnce() -> Result<T, SlotRefusal>,
) -> Result<T, SendGateRefusal> {
    if ask_cmdr == crate::settings::AskCmdrSwitch::Off {
        return Err(SendGateRefusal::AskCmdrOff);
    }
    resolve().map_err(SendGateRefusal::Slot)
}

/// The scripted turn the E2E fake streams: a short multi-chunk reply, so the test sees
/// streamed text land and a `Done`. Kept trivially deterministic.
///
/// ⚠️ **One script per slot, and the RAIL's is load-bearing.** `ask-cmdr.spec.ts` counts
/// replies by matching "test assistant", so changing that phrasing breaks specs that have
/// nothing to do with the wake loop. A wake gets its own sentence instead, which is also what
/// lets a wake spec tell a thread the agent opened apart from one the user did.
///
/// The wake slot has THREE scripts, selected by `test_mode::wake_fake_script` (which only the
/// `playwright-e2e` force-wake command sets), because a wake ends three materially different
/// ways and two of them are decided by a TOOL CALL rather than by what it says: it can reply,
/// it can call `nothing_to_suggest` (and delete its own thread), or it can call
/// `propose_suggestions` (and leave something for the user to review, which is what raises the
/// toast). Nothing else can reach them, and the ordinary wake reply is what an untouched
/// selector plays.
fn scripted_fake_llm(slot: AgentSlot) -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::test_mode::WakeFakeScript;
    if slot == AgentSlot::Wake {
        match crate::test_mode::wake_fake_script() {
            WakeFakeScript::Quiet => {
                return FakeAgentLlm::script(vec![
                    ScriptedTurn::CallTools(vec![(
                        crate::agent::llm::types::ToolId::NothingToSuggest,
                        serde_json::json!({ "reason": "it was all scripted test churn" }),
                    )]),
                    ScriptedTurn::Say(vec!["Nothing worth raising.".to_string()]),
                ]);
            }
            WakeFakeScript::Propose => return proposing_wake_fake(),
            WakeFakeScript::Reply => {}
        }
    }
    match slot {
        AgentSlot::Rail => FakeAgentLlm::by_prompt(rail_fake_turn),
        AgentSlot::Wake => FakeAgentLlm::script(vec![say(&["I had ", "a look at ", "what changed."])]),
    }
}

/// What a rail message starting with this plays: [`rail_fake_turn`]'s tool-running turn.
const RAIL_FAKE_TOOLS_MARKER: &str = "[tools]";
/// A rail message `[every-tool] <folder>` plays [`every_tool_round`], then the answer.
const RAIL_FAKE_EVERY_TOOL_MARKER: &str = "[every-tool]";
/// A rail message `[rename] <folder>` stages [`rename_plan_call`]'s plan, then signs off.
const RAIL_FAKE_RENAME_MARKER: &str = "[rename]";
/// A rail message `[fail:<kind>]` fails its call with [`rail_fake_failure`]'s typed error.
const RAIL_FAKE_FAIL_MARKER: &str = "[fail:";

/// The rail fake's turn for the prompt it's handed. A message starting with
/// [`RAIL_FAKE_TOOLS_MARKER`] runs three rounds, so `ask-cmdr.spec.ts` can pin a tool-running
/// turn's view live and after a reload: narration plus `app_state`, `list_volumes`, and a
/// hallucinated `delete` (refused); narration plus `list_volumes` twice (both repeats of round
/// one's, and reusing its call ids); then the answer. Anything else gets the one-line reply.
///
/// The round comes from the prompt (`latest_user_turn`), never a counter, so a retry or a second
/// thread can't desync it. `app_state` and `list_volumes` read the machine's real state: a spec
/// asserts their shape, never their values.
///
/// The other markers exist for the i18n capture (`i18n-capture-ask-cmdr.ts`), which needs every
/// tool line, a staged rename plan, and the typed failures on screen. Each carries what it needs
/// in the message text, since a fake has no other way to learn the fixture tree's path.
fn rail_fake_turn(messages: &[crate::agent::llm::types::AgentMessage]) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::{ScriptedTurn, latest_user_turn};
    use crate::agent::llm::types::ToolId;
    let no_args = || serde_json::json!({});
    let (text, round) = latest_user_turn(messages).unwrap_or(("", 0));
    if let Some(folder) = text.strip_prefix(RAIL_FAKE_EVERY_TOOL_MARKER) {
        return match round {
            0 => every_tool_round(folder.trim()),
            _ => say(&["That's every tool, ", "from the test assistant."]),
        };
    }
    if let Some(folder) = text.strip_prefix(RAIL_FAKE_RENAME_MARKER) {
        return match round {
            0 => rename_plan_call(folder.trim()),
            _ => say(&["Here's the plan, ", "from the test assistant."]),
        };
    }
    if let Some(kind) = text
        .strip_prefix(RAIL_FAKE_FAIL_MARKER)
        .and_then(|rest| rest.split(']').next())
    {
        return ScriptedTurn::Fail(rail_fake_failure(kind));
    }
    match latest_user_turn(messages) {
        Some((text, round)) if text.starts_with(RAIL_FAKE_TOOLS_MARKER) => match round {
            0 => ScriptedTurn::SayThenCall {
                say: vec!["Let me look at ".into(), "your panes and drives.".into()],
                calls: vec![
                    (ToolId::AppState, no_args()),
                    (ToolId::ListVolumes, no_args()),
                    (ToolId::Unrecognized("delete".into()), no_args()),
                ],
            },
            1 => ScriptedTurn::SayThenCall {
                say: vec!["Checking the drives ".into(), "once more.".into()],
                calls: vec![(ToolId::ListVolumes, no_args()), (ToolId::ListVolumes, no_args())],
            },
            _ => say(&["That's all, ", "from the test assistant."]),
        },
        _ => say(&["Hi! ", "I'm the ", "test assistant."]),
    }
}

/// One round calling every read tool the `[tools]` script doesn't, plus the two memory tools, so
/// their tool lines render with their subjects. It stays harmless on a shared app: the memory calls
/// aim outside the jail and refuse, `connect_volume` names a place nobody saved, and the search is
/// scoped to `folder` so it can't walk the drive. Results read the machine's real state, so a spec
/// asserts their shape, never their values.
fn every_tool_round(folder: &str) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::agent::llm::types::ToolId;
    use serde_json::json;
    let file = format!("{folder}/file-a.txt");
    ScriptedTurn::SayThenCall {
        say: vec!["Let me use ".into(), "everything I have.".into()],
        calls: vec![
            (ToolId::ListDir, json!({ "path": folder })),
            (ToolId::ListPaneFiles, json!({})),
            (ToolId::ImportantFolders, json!({})),
            (ToolId::FolderImportance, json!({ "path": folder })),
            (ToolId::OperationsList, json!({})),
            (ToolId::OperationsGet, json!({ "operationId": "no-such-operation" })),
            (ToolId::Search, json!({ "pattern": "file", "scope": folder })),
            (ToolId::SearchPhotos, json!({ "query": "a red bicycle" })),
            (ToolId::InspectFile, json!({ "paths": [file.clone()] })),
            (ToolId::ImageFacts, json!({ "paths": [file] })),
            (ToolId::ListSuggestions, json!({})),
            (ToolId::GetSuggestionGroup, json!({ "groupId": "no-such-group" })),
            (
                ToolId::MemoryWrite,
                json!({ "path": "../outside.md", "content": "Refused." }),
            ),
            (
                ToolId::MemoryEdit,
                json!({ "path": "../outside.md", "oldString": "a", "newString": "b" }),
            ),
            (ToolId::ConnectVolume, json!({ "volumeId": "no-such-place" })),
        ],
    }
}

/// A plan renaming the E2E fixture tree's two files in `folder`, which has to be what the focused
/// pane shows or the tool refuses it. Names come from the file names (`filename` evidence), so no
/// content claim needs backing.
fn rename_plan_call(folder: &str) -> crate::agent::llm::fake::ScriptedTurn {
    use crate::agent::llm::fake::ScriptedTurn;
    use crate::agent::llm::types::ToolId;
    let row = |from: &str, to: &str| {
        serde_json::json!({
            "sourcePath": format!("{folder}/{from}"),
            "volumeId": "root",
            "destinationName": to,
            "evidence": { "source": "filename", "detail": format!("The old name, {from}") },
        })
    };
    ScriptedTurn::SayThenCall {
        say: vec!["I'd name ".into(), "these two like this.".into()],
        calls: vec![(
            ToolId::ProposeRenamePlan,
            serde_json::json!({ "renames": [row("file-a.txt", "notes-a.txt"), row("file-b.txt", "notes-b.txt")] }),
        )],
    }
}

/// The typed failure a `[fail:<kind>]` message asks for, by its `AskCmdrErrorKind` name, with
/// `provider` for anything else.
fn rail_fake_failure(kind: &str) -> crate::agent::llm::types::AgentLlmError {
    use crate::agent::llm::types::AgentLlmError;
    match kind {
        "noKey" => AgentLlmError::NoKey,
        "notConfigured" => AgentLlmError::NotConfigured,
        "unavailable" => AgentLlmError::Unavailable,
        "timeout" => AgentLlmError::Timeout,
        "authFailed" => AgentLlmError::AuthFailed("Scripted key refusal".to_string()),
        "rateLimited" => AgentLlmError::RateLimited("Scripted rate limit".to_string()),
        "budgetExhausted" => AgentLlmError::BudgetExhausted,
        _ => AgentLlmError::Provider("Scripted provider failure".to_string()),
    }
}

fn say(chunks: &[&str]) -> crate::agent::llm::fake::ScriptedTurn {
    crate::agent::llm::fake::ScriptedTurn::Say(chunks.iter().map(|chunk| (*chunk).to_string()).collect())
}

/// A wake that stages one group and signs off, so a spec can drive the path where the user is
/// told something is waiting.
///
/// ⚠️ **An explicit `paths` group, ❌ never a selector.** A selector resolves against the drive
/// index at creation, which makes what the fake stages depend on whether the E2E fixture tree
/// happens to be indexed yet. Explicit paths stage exactly one op, deterministically. Nothing
/// runs from a staged group — approval is the user's, and no test approves this one — so the
/// paths do not need to exist.
fn proposing_wake_fake() -> FakeAgentLlm {
    use crate::agent::llm::fake::ScriptedTurn;
    FakeAgentLlm::script(vec![
        ScriptedTurn::CallTools(vec![(
            crate::agent::llm::types::ToolId::ProposeSuggestions,
            serde_json::json!({
                "rationale": "scripted test churn",
                "groups": [{
                    "verb": "trash",
                    "sourceVolumeId": "root",
                    "displayName": "Scripted wake suggestion",
                    "paths": ["/Users/e2e/wake-staged/scripted.txt"],
                }],
            }),
        )]),
        ScriptedTurn::Say(vec!["I staged something for you.".to_string()]),
    ])
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
    (cloud_provider_tag(&model), model)
}

/// A cloud model's provider tag, by its name prefix, matching `ai::client`'s adapter routing.
pub fn cloud_provider_tag(model: &str) -> ProviderTag {
    if model.starts_with("claude-") {
        ProviderTag::Anthropic
    } else if model.starts_with("gemini-") {
        ProviderTag::Gemini
    } else {
        ProviderTag::OpenAi
    }
}

/// The window the configured cloud provider reports for `model`, if a report has arrived.
/// Never waits for one: a stale or missing catalog starts a background refresh and this answers
/// `None`, so the send sizes from the family table this once. A local model has no report.
pub fn reported_window_tokens(app: &AppHandle, provider: ProviderTag, model: &str) -> Option<usize> {
    if provider == ProviderTag::Local {
        return None;
    }
    crate::ai::model_capabilities::for_configured_cloud_model(app, model)?
        .context_window_tokens
        .map(|tokens| tokens as usize)
}

/// How hard the slot's model thinks and how much room its reply gets, gathered here and decided
/// in the pure [`effort`] module: the user's choice for this provider and model
/// (`ai.reasoningEfforts`, read fresh per send), and what the provider reports about the
/// model's reasoning and longest reply. A choice the model can't take is logged with what it
/// became. A local model and the E2E fake get the default: the provider decides.
pub fn resolve_call_settings(app: &AppHandle, provider: ProviderTag, model: &str) -> CallSettings {
    let pick =
        |provider_id: &str| effort::choice_for(&crate::settings::load_ai_reasoning_efforts(app), provider_id, model);
    call_settings_for(app, provider, model, pick)
}

/// [`resolve_call_settings`] for a WAKE: Recommended, whatever the rail's pick, so a background
/// sweep never inherits an expensive one, but sized like the rail's Recommended (Claude's
/// default is high, so it gets High's room rather than thinking inside the base slot).
pub fn resolve_wake_call_settings(app: &AppHandle, provider: ProviderTag, model: &str) -> CallSettings {
    call_settings_for(app, provider, model, |_| effort::EffortChoice::Recommended)
}

fn call_settings_for(
    app: &AppHandle,
    provider: ProviderTag,
    model: &str,
    choice: impl FnOnce(&str) -> effort::EffortChoice,
) -> CallSettings {
    if provider == ProviderTag::Local || crate::test_mode::ask_cmdr_fake_active() {
        return CallSettings::default();
    }
    let provider_id = crate::ai::state::get_cloud_provider_id();
    let reported = crate::ai::model_capabilities::for_configured_cloud_model(app, model);
    let options = effort::effort_options(provider, model, reported.as_ref().map(|c| &c.reasoning));
    let choice = choice(&provider_id);
    let resolved = effort::resolve_effort(&options, choice, reported.as_ref().and_then(|c| c.max_output_tokens));
    if let Some(adjusted_to) = resolved.adjusted_to {
        log::info!(
            target: LOG_TARGET,
            "reasoning effort {choice:?} isn't one {model} takes; sending {adjusted_to:?} instead" // allowed-pluralize-noun: `takes` is a verb here
        );
    }
    // Fit the reply to the window it shares with the prompt, so a high effort on a small window
    // can't starve the prompt or ask for more than prompt plus reply can hold.
    let window = budget::known_window(
        provider,
        model,
        crate::ai::state::get_local_context_size(),
        reported.and_then(|c| c.context_window_tokens).map(|t| t as usize),
    )
    .map(|w| w.tokens);
    let max_output_tokens = effort::fit_reply_to_window(resolved.max_output_tokens, window);
    if max_output_tokens < resolved.max_output_tokens {
        log::debug!(
            target: LOG_TARGET,
            "reply ceiling for {model} fitted to its window: {max_output_tokens} of {}",
            resolved.max_output_tokens
        );
    }
    CallSettings {
        effort: resolved.effort,
        max_output_tokens,
        model_reasons: options.reasons,
    }
}

/// The chat memory the slot-change timeline names (`budget::chat_memory_figure`): moves only when
/// the user's "Chat memory size" or the model does, never with an effort or a late report.
pub fn chat_memory_tokens(app: &AppHandle, provider: ProviderTag, model: &str) -> Result<usize, budget::BudgetRefusal> {
    if crate::test_mode::ask_cmdr_fake_active() {
        return Ok(budget::DEFAULT_PROMPT_TOKEN_BUDGET);
    }
    budget::chat_memory_figure(budget::BudgetInputs {
        provider,
        model,
        user_choice: crate::settings::load_ask_cmdr_chat_memory_size(app),
        local_context_tokens: crate::ai::state::get_local_context_size(),
        reported_window_tokens: reported_window_tokens(app, provider, model),
        reply_tokens: budget::AGENT_MAX_OUTPUT_TOKENS as usize,
    })
}

/// The assembled-prompt token budget for the resolved slot, gathered here and decided in the
/// pure [`budget`] module. This layer supplies the three values the core may not read itself:
/// the user's `askCmdr.chatMemorySize` choice (read fresh per turn, so a change applies to the
/// NEXT message and never to a turn already in flight), the local server's configured window,
/// and the window the provider reports ([`reported_window_tokens`]).
///
/// The resolution's source is logged, so a budget that came from a stale family table is
/// visible in the log rather than silently authoritative.
pub fn resolve_prompt_budget(
    app: &AppHandle,
    provider: ProviderTag,
    model: &str,
) -> Result<usize, budget::BudgetRefusal> {
    resolve_prompt_budget_for(app, provider, model, resolve_call_settings(app, provider, model))
}

/// [`resolve_prompt_budget`] for a slot whose [`CallSettings`] the caller already resolved, so a
/// send sizes its prompt against the SAME reply ceiling its calls then ask for.
pub fn resolve_prompt_budget_for(
    app: &AppHandle,
    provider: ProviderTag,
    model: &str,
    call: CallSettings,
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
        reported_window_tokens: reported_window_tokens(app, provider, model),
        reply_tokens: call.max_output_tokens as usize,
    })?;
    log::debug!(
        target: LOG_TARGET,
        "prompt budget for {model}: a {}-token ceiling, from {}",
        resolved.prompt_tokens,
        resolved.source.label() // allowed-log-private: a fixed budget-source phrase of our own
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
    let connectivity = match summary.connection_state {
        Some("direct") => Some(EnvelopeConnectivity::Direct),
        Some("os_mount") => Some(EnvelopeConnectivity::OsMount),
        Some("disconnected") => Some(EnvelopeConnectivity::Disconnected),
        Some("needs_sign_in") => Some(EnvelopeConnectivity::NeedsSignIn),
        Some("needs_host_key_approval") => Some(EnvelopeConnectivity::NeedsHostKeyApproval),
        Some("saved") => Some(EnvelopeConnectivity::Saved),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The rail fake's tool turn plays its rounds by what the prompt holds, and every other
    /// message keeps the one-line reply `ask-cmdr.spec.ts` counts.
    #[test]
    fn the_rail_fake_picks_its_round_from_the_prompt() {
        use crate::agent::llm::fake::ScriptedTurn;
        use crate::agent::llm::types::{AgentMessage, AgentPart, AgentRole, ToolId};
        let message = |role, text: &str| AgentMessage {
            role,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text(text.into())],
            at: 0,
        };
        let tools = message(AgentRole::User, "[tools] look around");
        let answer = message(AgentRole::Assistant, "…");
        let tools_of = |turn: ScriptedTurn| match turn {
            ScriptedTurn::SayThenCall { calls, .. } => calls.into_iter().map(|(tool, _)| tool).collect(),
            _ => Vec::new(),
        };

        assert_eq!(
            tools_of(rail_fake_turn(std::slice::from_ref(&tools))),
            [
                ToolId::AppState,
                ToolId::ListVolumes,
                ToolId::Unrecognized("delete".into())
            ]
        );
        assert_eq!(
            tools_of(rail_fake_turn(&[tools.clone(), answer.clone()])),
            [ToolId::ListVolumes, ToolId::ListVolumes]
        );
        assert!(matches!(
            rail_fake_turn(&[tools, answer.clone(), answer.clone()]),
            ScriptedTurn::Say(_)
        ));
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[message(AgentRole::User, "hello")]) else {
            panic!("a plain message gets the one-line reply");
        };
        assert_eq!(chunks.concat(), "Hi! I'm the test assistant.");
    }

    /// The capture's scripts: `i18n-capture-ask-cmdr.ts` sends these markers and waits on "test
    /// assistant" (`ask-cmdr.spec.ts` matches the same words).
    #[test]
    fn the_rail_fake_plays_the_capture_scripts() {
        use crate::agent::llm::fake::ScriptedTurn;
        use crate::agent::llm::types::{AgentLlmError, AgentMessage, AgentPart, AgentRole, ToolId};
        let message = |role, text: &str| AgentMessage {
            role,
            parts: vec![AgentPart::Text("<envelope>".into()), AgentPart::Text(text.into())],
            at: 0,
        };
        let answer = message(AgentRole::Assistant, "…");

        let rename = message(AgentRole::User, "[rename] /fixtures/left");
        let ScriptedTurn::SayThenCall { calls, .. } = rail_fake_turn(std::slice::from_ref(&rename)) else {
            panic!("a rename message opens with its plan");
        };
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, ToolId::ProposeRenamePlan);
        assert_eq!(calls[0].1["renames"][0]["sourcePath"], "/fixtures/left/file-a.txt");
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[rename, answer.clone()]) else {
            panic!("a rename message signs off after its plan");
        };
        assert!(chunks.concat().contains("test assistant"));

        let every = message(AgentRole::User, "[every-tool] /fixtures/left");
        let ScriptedTurn::SayThenCall { calls, .. } = rail_fake_turn(std::slice::from_ref(&every)) else {
            panic!("an every-tool message opens with its round");
        };
        let tools: Vec<ToolId> = calls.into_iter().map(|(tool, _)| tool).collect();
        assert!(tools.contains(&ToolId::MemoryWrite) && tools.contains(&ToolId::ConnectVolume));
        assert!(
            !tools.contains(&ToolId::ProposeRenamePlan) && !tools.contains(&ToolId::ProposeSuggestions),
            "nothing in this round may stage anything for the user to answer"
        );
        let ScriptedTurn::Say(chunks) = rail_fake_turn(&[every, answer]) else {
            panic!("an every-tool message signs off after its round");
        };
        assert!(chunks.concat().contains("test assistant"));

        let fail = message(AgentRole::User, "[fail:timeout] anything");
        assert!(matches!(
            rail_fake_turn(std::slice::from_ref(&fail)),
            ScriptedTurn::Fail(AgentLlmError::Timeout)
        ));
    }

    /// A slot refusal reaches the rail as its OWN wire kind: a missing cloud consent must not
    /// read as "set up a provider" to somebody whose provider is set up.
    /// Cloud without consent is its own refusal; every "nothing to talk to" case collapses to one.
    #[test]
    fn a_backend_resolution_maps_onto_the_slot() {
        use crate::ai::manager::BackendResolution;
        use crate::managed_policy::ManagedAiRefusal;
        assert!(matches!(
            slot_backend(BackendResolution::NoCloudConsent),
            Err(SlotRefusal::NoCloudConsent)
        ));
        assert!(matches!(
            slot_backend(BackendResolution::Managed(ManagedAiRefusal::HostNotAllowed)),
            Err(SlotRefusal::Managed(ManagedAiRefusal::HostNotAllowed))
        ));
        for unconfigured in [
            BackendResolution::Off,
            BackendResolution::NotConfigured("x"),
            BackendResolution::UnknownProvider("x".into()),
        ] {
            assert!(matches!(slot_backend(unconfigured), Err(SlotRefusal::NotConfigured)));
        }
    }

    /// The organization's reason rides along with the wire kind, so the rail words `cloudAiOff`
    /// as "only on-device AI", not as "doesn't allow this AI service".
    #[test]
    fn a_managed_refusal_keeps_its_reason_through_the_send_gate() {
        use crate::managed_policy::ManagedAiRefusal;
        use crate::settings::AskCmdrSwitch;
        let refused = admit_send(AskCmdrSwitch::On, || {
            Err::<(), _>(SlotRefusal::Managed(ManagedAiRefusal::CloudAiOff))
        })
        .expect_err("the slot refused");
        assert!(matches!(refused.view(), AgentErrorKindView::ManagedByOrganization));
        assert_eq!(refused.managed(), Some(ManagedAiRefusal::CloudAiOff));

        let off = admit_send(AskCmdrSwitch::Off, || Ok::<(), SlotRefusal>(())).expect_err("switched off");
        assert_eq!(off.managed(), None);
        assert_eq!(SlotRefusal::NoCloudConsent.managed(), None);
    }

    #[test]
    fn each_slot_refusal_maps_to_its_own_wire_kind() {
        assert_eq!(
            serde_json::to_value(SlotRefusal::NoCloudConsent.view()).expect("serializes"),
            "noCloudConsent"
        );
        assert_eq!(
            serde_json::to_value(SlotRefusal::NotConfigured.view()).expect("serializes"),
            "notConfigured"
        );
        assert_eq!(
            serde_json::to_value(SlotRefusal::Managed(crate::managed_policy::ManagedAiRefusal::CloudAiOff).view())
                .expect("serializes"),
            "managedByOrganization"
        );
    }
}
