//! The typed message-part model the `AgentLlm` seam speaks.
//!
//! The make-or-break invariant this file encodes: an
//! assistant turn is an ordered list of **typed parts** (text, tool call,
//! tool result, reasoning), and opaque reasoning state is **provider-tagged and
//! rides on the part that owns it** — never flattened to `content: String +
//! reasoning: String`. That lossy shape is exactly what breaks a multi-step tool
//! loop: the provider blob has to be re-attached to
//! the right structural position, so it cannot be concatenated into a text field.
//! See `DETAILS.md` for the mapping table and the per-provider reasoning round-trip.
//!
//! These types carry `serde` for DB persistence (`main.db` `content_blocks`)
//! and are pure data — no dependency on `genai` or `crate::ai`. The genai
//! coupling lives entirely in `genai_impl.rs`. The reasoning `blob` is a
//! **backend-only** value: it is persisted and replayed, but never crosses to the
//! frontend (the wire `MessageView` carries display parts only).

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The role of a message in the conversation. DB token form is snake_case (the
/// `messages.role` column); IPC uses the wire `MessageView`, not this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    System,
    User,
    Assistant,
    Tool,
}

impl AgentRole {
    /// The stable DB token for the `messages.role` column. Snake_case, the
    /// one place the enum ↔ storage mapping lives; renaming a token is a schema
    /// change, renaming a variant is free.
    pub fn as_token(self) -> &'static str {
        match self {
            AgentRole::System => "system",
            AgentRole::User => "user",
            AgentRole::Assistant => "assistant",
            AgentRole::Tool => "tool",
        }
    }

    /// Parse a stored token back to the variant, or `None` if unknown (a row
    /// written by a newer schema, or corruption — the reader surfaces a typed
    /// store error rather than guessing).
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "system" => Some(AgentRole::System),
            "user" => Some(AgentRole::User),
            "assistant" => Some(AgentRole::Assistant),
            "tool" => Some(AgentRole::Tool),
            _ => None,
        }
    }
}

/// Which provider a reasoning blob came from. The tag is descriptive: it records
/// where the opaque `blob` originated so a replay re-attaches it in the shape that
/// provider expects. Distinguishes the two OpenAI surfaces because their reasoning
/// round-trip differs (chat-completions is stateless; Responses carries encrypted
/// items).
///
/// Two-way split, like the operation log's `token_enum!` types: the serde/specta
/// wire form is camelCase (for IPC + `bindings.ts` — the `cost_meter` provider on
/// the wire, the per-thread cost breakdown), while [`as_token`](Self::as_token) is the
/// stable snake_case DB token (the `cost_meter.provider` column). The reasoning
/// `blob` in `content_blocks` persists this via serde and round-trips it untouched;
/// its exact string form is backend-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderTag {
    Anthropic,
    OpenAi,
    OpenAiResponses,
    Gemini,
    Local,
}

impl ProviderTag {
    /// Every variant, for exhaustive tests (token uniqueness, round-trip).
    pub const ALL: [ProviderTag; 5] = [
        ProviderTag::Anthropic,
        ProviderTag::OpenAi,
        ProviderTag::OpenAiResponses,
        ProviderTag::Gemini,
        ProviderTag::Local,
    ];

    /// The stable snake_case DB token for the `cost_meter.provider` column. The one
    /// place enum ↔ storage mapping lives; separate from the camelCase serde wire
    /// form so neither can drift the other.
    pub fn as_token(self) -> &'static str {
        match self {
            ProviderTag::Anthropic => "anthropic",
            ProviderTag::OpenAi => "openai",
            ProviderTag::OpenAiResponses => "openai_responses",
            ProviderTag::Gemini => "gemini",
            ProviderTag::Local => "local",
        }
    }

    /// Parse a stored token back to the variant, or `None` if unknown.
    pub fn from_token(token: &str) -> Option<Self> {
        match token {
            "anthropic" => Some(ProviderTag::Anthropic),
            "openai" => Some(ProviderTag::OpenAi),
            "openai_responses" => Some(ProviderTag::OpenAiResponses),
            "gemini" => Some(ProviderTag::Gemini),
            "local" => Some(ProviderTag::Local),
            _ => None,
        }
    }
}

/// How hard the model should think before it answers: a per-call input to
/// [`AgentLlm::respond`](super::AgentLlm::respond), mapped onto each provider's own knob in
/// `genai_impl.rs` (Anthropic `output_config.effort`, OpenAI `reasoning.effort` /
/// `reasoning_effort`, Gemini `thinkingLevel` / `thinkingBudget`, DeepSeek `thinking.type`).
/// The mapping table and its caveats: `DETAILS.md` § Reasoning effort.
///
/// An explicit level sent to a model that doesn't reason (OpenAI `gpt-4.1`, most local models) is
/// a 400 there, so whatever offers the other levels must know the model reasons first. For a
/// model known not to reason, [`CallSettings::model_reasons`] overrides even `ProviderDefault`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReasoningEffort {
    /// Send no effort and let the provider decide, except on the OpenAI Responses API, where
    /// it means `medium` (OpenAI's documented default; `crate::ai` would otherwise pick `low`).
    #[default]
    ProviderDefault,
    /// Ask for no reasoning. Models that can't stop thinking (Gemini Pro, Claude Fable) keep
    /// their default.
    Off,
    Low,
    Medium,
    High,
    /// The provider's highest level; where there is none above `high`, `high`.
    Max,
}

/// What one [`AgentLlm::respond`](super::AgentLlm::respond) call asks of the model beyond the
/// prompt: how hard to think, and the output ceiling that thinking shares with the reply and
/// its tool calls. Resolved per slot by `chat::effort::resolve_effort`, which never pairs a
/// level with a model that can't take it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CallSettings {
    pub effort: ReasoningEffort,
    pub max_output_tokens: u32,
    /// Whether the model reasons, when something knows (`chat::effort::EffortOptions::reasons`).
    /// `Some(false)` makes the call ask for no reasoning at all, whatever `effort` says: no
    /// effort, and no reasoning summary or encrypted items back. On the OpenAI Responses API a
    /// model that doesn't reason 400s on either ("Unsupported parameter: 'reasoning.effort'" /
    /// "'reasoning.summary'", verified on `gpt-4.1-mini` and `gpt-4o`, `curl`, 2026-10-27).
    pub model_reasons: Option<bool>,
}

impl Default for CallSettings {
    /// The provider decides, inside the base reply slot.
    fn default() -> Self {
        Self {
            effort: ReasoningEffort::ProviderDefault,
            max_output_tokens: crate::agent::chat::budget::AGENT_MAX_OUTPUT_TOKENS,
            model_reasons: None,
        }
    }
}

/// Opaque, provider-tagged reasoning state. The `blob` shape is owned solely by
/// the genai adapter mapping (`genai_impl.rs`); everything else treats it as
/// opaque and must persist and replay it untouched. Never inspect or reshape it
/// outside the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningState {
    pub provider: ProviderTag,
    pub blob: serde_json::Value,
}

/// The typed name of a tool, generated from the tool table. Re-exported here because a tool call
/// is part of this message model.
pub use crate::tools::ToolId;

/// A single tool invocation the model emitted. `reasoning` carries any opaque
/// state that provider attaches to the call itself (e.g. Gemini's per-`functionCall`
/// `thoughtSignature`), which must ride here to survive replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolCall {
    pub call_id: String,
    pub tool: ToolId,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reasoning: Option<ReasoningState>,
}

impl AgentToolCall {
    /// The call that actually runs: for a well-formed `use_tool`, the tool it names with that tool's
    /// arguments; for anything else, this call. Every reader of "what ran" goes through this (the gate,
    /// the repeat breaker, Stop, the ledgers, the rail), so none of them knows `use_tool` exists. What
    /// the provider sees is the stored call, which never changes: this is derived, and ❌ never
    /// persisted.
    ///
    /// A malformed `use_tool` stays itself, so it reaches its own handler and hears why
    /// ([`use_tool_target`](crate::tools::meta::use_tool_target)).
    pub fn effective(&self) -> Cow<'_, AgentToolCall> {
        if self.tool != ToolId::UseTool {
            return Cow::Borrowed(self);
        }
        match crate::tools::meta::use_tool_target(&self.arguments) {
            Ok((tool, arguments)) => Cow::Owned(AgentToolCall {
                call_id: self.call_id.clone(),
                tool,
                arguments,
                // Reasoning replays with the stored call, and nothing that reads the effective one
                // ever sends it anywhere.
                reasoning: None,
            }),
            Err(_) => Cow::Borrowed(self),
        }
    }

    /// What makes two calls "the same call": the tool's wire name plus its whole arguments
    /// object, keys sorted at every depth ([`canonical_json`]), so two calls that mean the same
    /// thing render the same bytes whatever order the provider emitted their keys in. The call id
    /// is NOT part of it (ids collide across rounds). One definition for the repeat breaker
    /// (`chat/runtime/repeats.rs`) and the rail's repeat mark (`agent/line/`).
    pub fn identity_key(&self) -> String {
        // A unit separator, so no tool name plus arguments can collide with another pair.
        format!("{}\u{1f}{}", self.tool.as_wire_name(), canonical_json(&self.arguments))
    }
}

/// `value` with every object's keys sorted, at every depth. Not `serde_json`'s own order: genai
/// turns on `preserve_order` for the whole build, so a `Map` iterates as it was filled.
fn canonical_json(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<_> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), canonical_json(value)))
                    .collect(),
            )
        }
        serde_json::Value::Array(items) => serde_json::Value::Array(items.iter().map(canonical_json).collect()),
        other => other.clone(),
    }
}

/// The result of executing a tool, fed back to the model. `elided` records that
/// the runtime collapsed this result to a stub for the context budget; the
/// content already reflects the elision when set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub call_id: String,
    pub content: serde_json::Value,
    pub elided: bool,
}

impl AgentToolResult {
    /// True when this is a real answer rather than a refusal or a handler problem. Reads OUR
    /// OWN typed result keys (`available` / `problem`), never external wording.
    ///
    /// It lives on the type, not with any one caller, because four of them now judge the same
    /// question: the turn loop (whether to record a failed call), the wake watcher (whether a
    /// proposal landed), the repeat breaker's contract, and the tools view (whether to stamp a
    /// propose refusal's `readyForReview: false`). A second copy of these key names is a second
    /// thing to forget when a result shape grows.
    pub fn reports_a_problem(&self) -> bool {
        let refused = self.content.get("available") == Some(&serde_json::Value::Bool(false));
        refused || self.content.get("problem").is_some()
    }
}

/// What a wake noticed, as DATA: one entry per folder that earned its own line, plus
/// the rolled-up remainder.
///
/// ⚠️ **A wake's first message is structured, never prose.** It is persisted as the
/// thread's user-role message and lives in `main.db` for as long as the thread does, so
/// a rendered English sentence there would be untranslated UI copy frozen where no later
/// locale pass can reach it. The rail localizes these numbers itself; [`render`] exists
/// for the provider, which reads English either way.
///
/// [`render`]: WakeDigest::render
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeDigest {
    pub folders: Vec<WakeDigestFolder>,
    pub rollups: Vec<WakeDigestRollup>,
}

/// One folder the digest names outright, with what happened in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeDigestFolder {
    pub folder: String,
    pub created: u32,
    pub modified: u32,
    pub removed: u32,
    pub renamed: u32,
}

/// Folders that did not earn a line, summarized under a shared ancestor. The COUNT is the
/// point: it is how the reader knows the size of what it is not being shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WakeDigestRollup {
    pub ancestor: String,
    pub folders: u32,
    /// Every change across those folders. `u64` because it is a SUM of four `u32` counters
    /// over an unbounded number of folders, and the one number here that can overflow.
    pub changes: u64,
}

impl WakeDigest {
    /// The path a wake thread is named after: the first folder the digest names, else the first
    /// rollup's ancestor. `None` for an empty digest.
    pub fn place(&self) -> Option<&str> {
        self.folders
            .first()
            .map(|folder| folder.folder.as_str())
            .or_else(|| self.rollups.first().map(|rollup| rollup.ancestor.as_str()))
    }

    /// The thread title a wake gives the place at `path`: its last component. Data, not voice:
    /// a backend English sentence would be untranslated copy frozen into `main.db`.
    pub fn title_for(path: &str) -> &str {
        let trimmed = path.trim_end_matches('/');
        match trimmed.rfind('/') {
            Some(cut) => &trimmed[cut + 1..],
            None => trimmed,
        }
    }

    /// Every folder the digest covers, the rolled-up ones included.
    pub fn folder_count(&self) -> u64 {
        self.folders.len() as u64 + self.rollups.iter().map(|rollup| u64::from(rollup.folders)).sum::<u64>()
    }

    /// The digest as the MODEL reads it. An empty digest renders an empty string, never a
    /// header saying there is nothing to report: that would spend budget to say nothing.
    ///
    /// ❌ Never render this into the UI. It is deliberately English, deliberately terse,
    /// and deliberately shaped for a prompt.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for folder in &self.folders {
            out.push_str(&folder.render());
        }
        for rollup in &self.rollups {
            out.push_str(&rollup.render());
        }
        out
    }

    /// Every path the digest mentions, in the order it mentions them. The message's FTS
    /// text: paths are the user's own data rather than authored copy, so they are the one
    /// part of a wake's first message that belongs in a search index.
    pub fn paths(&self) -> Vec<&str> {
        self.folders
            .iter()
            .map(|folder| folder.folder.as_str())
            .chain(self.rollups.iter().map(|rollup| rollup.ancestor.as_str()))
            .collect()
    }
}

impl WakeDigestFolder {
    /// One folder line for the prompt: where, and what happened there.
    pub fn render(&self) -> String {
        format!("{}: {}\n", self.folder, self.counts_text())
    }

    /// Only the kinds that actually happened: a line of zeroes is budget spent on nothing.
    fn counts_text(&self) -> String {
        let parts = [
            (self.created, "new"),
            (self.modified, "changed"),
            (self.removed, "removed"),
            (self.renamed, "renamed"),
        ];
        let text = parts
            .iter()
            .filter(|(count, _)| *count > 0)
            .map(|(count, label)| format!("{count} {label}"))
            .collect::<Vec<_>>()
            .join(", ");
        if text.is_empty() {
            "no changes".to_string()
        } else {
            text
        }
    }
}

impl WakeDigestRollup {
    /// One rollup line for the prompt. The folder count is what tells the model the size of
    /// what it is not being shown.
    pub fn render(&self) -> String {
        format!(
            "+ {} more folders under {}: {} changes\n",
            self.folders, self.ancestor, self.changes
        )
    }
}

/// One ordered content part of a message. The variant keys serialize snake_case
/// (`text`, `tool_call`, `tool_result`, `reasoning`, `wake_digest`,
/// `proposal_outcomes`) for a plainly-inspectable DB `content_blocks` JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPart {
    Text(String),
    ToolCall(AgentToolCall),
    ToolResult(AgentToolResult),
    /// Opaque reasoning state; persisted and replayed untouched, never shown.
    Reasoning(ReasoningState),
    /// What a wake noticed, structured. The provider reads [`WakeDigest::render`]; the rail
    /// reads the numbers and says them in the user's own language.
    WakeDigest(WakeDigest),
    /// What the user did with a sweep's proposals, structured. Opens the follow-up turn the
    /// same way a digest opens a wake, and for the same reason: a rendered English sentence
    /// would freeze one locale's copy in `main.db` where no later locale pass can reach it.
    ProposalOutcomes(crate::agent::types::ProposalOutcomes),
    /// The context envelope a user turn was sent with, rendered once at send
    /// (`chat::context::render_envelope`) and replayed verbatim on every later call, so the
    /// thread's earlier messages stay byte-identical and the provider's prompt cache holds.
    /// The provider reads it as text; the rail never shows it and search never indexes it.
    Envelope(String),
    /// The agent's memory files that changed since the thread last carried them, captured at
    /// send and persisted with the user turn. The provider reads [`MemoryNotes::render`] (the
    /// fenced, announced block, `chat::context::memory_notes`); the rail never shows it and
    /// search never indexes it.
    MemoryNotes(MemoryNotes),
}

// The agent's memory part: see the module's own header.
mod memory_notes;
pub use memory_notes::{MemoryNote, MemoryNotes};

/// A full message: a role, its ordered typed parts, and its timestamp. Every
/// message carries `at` (unix secs) so the model can reason about gaps ("this
/// morning") across a thread's history (spec §5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub parts: Vec<AgentPart>,
    pub at: i64,
}

/// A tool the agent declares to the provider. Built from a [`ToolId`] plus its
/// description and JSON schema. NEVER emitted with `strict: true` (
/// OpenAI strict mode also demands all-required, which genai does not enforce, so
/// an optional prop 400s); the genai mapping leaves strict unset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDeclaration {
    pub name: ToolId,
    pub description: String,
    pub schema: serde_json::Value,
}

/// Why an assistant turn ended. The known reasons are unit variants (the provider's
/// raw string is dropped; the classification is typed); `Other` keeps the raw
/// string for an unmapped provider reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStopReason {
    Completed,
    ToolCall,
    MaxTokens,
    ContentFilter,
    StopSequence,
    Other(String),
}

/// Normalized per-call token usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AgentUsage {
    /// The whole input, the cached share included.
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    /// Of `prompt_tokens`, read back from the provider's prompt cache (billed at a fraction of
    /// input, `agent::pricing`).
    #[serde(default)]
    pub cache_read_tokens: u32,
    /// Of `prompt_tokens`, written to the provider's prompt cache for the next call (billed
    /// above input on Claude and GPT-5.6+).
    #[serde(default)]
    pub cache_write_tokens: u32,
}

/// A streamed increment of an assistant turn. The UI renders text as it arrives,
/// shows "thinking…" on a `ReasoningTick` (the reasoning content is never
/// surfaced), and a "looked at X" line on `ToolCallStarted`. The terminal `End`
/// carries the fully-assembled final [`AgentMessage`] (with any opaque state) plus
/// the stop reason and usage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentDelta {
    Text(String),
    /// Opaque reasoning progressed; the UI shows "thinking…", content never surfaced.
    ReasoningTick,
    /// The provider said it's still working without saying anything yet (Anthropic's `ping`, a
    /// Gemini status update, the stream opening). Never shown; it only proves the call isn't
    /// stalled (`chat::context::STREAM_STALL_TIMEOUT`), so a model thinking quietly between its
    /// provider's keep-alives isn't ended as one.
    KeepAlive,
    ToolCallStarted {
        call_id: String,
        tool: ToolId,
    },
    End {
        stop: AgentStopReason,
        usage: AgentUsage,
        message: AgentMessage,
    },
}

/// The typed error surface of the `AgentLlm` seam. Provider transport details are
/// classified by HTTP status upstream (`crate::ai`'s `ai_error_for_status`), never
/// by message-string matching; the `String` payloads carry the
/// provider's own wording for display only, never for control flow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentLlmError {
    /// No API key configured for the selected provider.
    NoKey,
    /// The provider/model slot is not configured.
    NotConfigured,
    /// The provider is unreachable (DNS / connect refused / no route).
    Unavailable,
    /// The request timed out.
    Timeout,
    /// The provider rejected the API key (HTTP 401 / 403); the string is for display only.
    AuthFailed(String),
    /// The provider is rate-limiting or the account is out of quota (HTTP 429); the
    /// string is for display only.
    RateLimited(String),
    /// The account's plan doesn't include this model (a 429 with a zero request limit, as a
    /// free Mistral workspace gets on `mistral-small-latest`). Retrying won't help; another
    /// model or plan does. The string is for display only.
    ModelNotInPlan(String),
    /// The provider won't take a request this big (HTTP 413), typically an account plan
    /// whose per-minute token cap is below one prompt (Groq's free tier). Retrying won't help.
    /// The string is for display only.
    RequestTooLarge(String),
    /// A per-message budget (tool turns / wall time / tokens) was exhausted.
    BudgetExhausted,
    /// Any other provider-side failure; the string is for display only.
    Provider(String),
    /// The organization's managed policy refused the request: the LLM client's per-request
    /// backstop, when a policy arrives after the slot resolved. Not a provider failure.
    Managed(crate::managed_policy::ManagedAiRefusal),
}

impl AgentLlmError {
    /// The provider-authored detail, where the variant carries one. The UI shows it
    /// under the typed headline so the user sees what to fix (a retired model slug, a
    /// quota reset time); it never drives control flow.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::AuthFailed(detail)
            | Self::RateLimited(detail)
            | Self::ModelNotInPlan(detail)
            | Self::RequestTooLarge(detail)
            | Self::Provider(detail) => Some(detail),
            _ => None,
        }
    }
}

impl std::fmt::Display for AgentLlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoKey => write!(f, "no API key configured"),
            Self::NotConfigured => write!(f, "the AI provider is not configured"),
            Self::Unavailable => write!(f, "the AI provider is unavailable"),
            Self::Timeout => write!(f, "the AI request timed out"),
            Self::AuthFailed(detail) => write!(f, "the AI provider rejected the API key: {detail}"),
            Self::RateLimited(detail) => write!(f, "the AI provider is rate-limiting or out of quota: {detail}"),
            Self::ModelNotInPlan(detail) => write!(f, "the AI provider's plan doesn't include this model: {detail}"),
            Self::RequestTooLarge(detail) => write!(f, "the AI provider won't take a request this big: {detail}"),
            Self::BudgetExhausted => write!(f, "the message budget was exhausted"),
            Self::Provider(detail) => write!(f, "the AI provider returned a problem: {detail}"),
            Self::Managed(refusal) => write!(f, "the organization's policy refuses this AI request ({refusal:?})"),
        }
    }
}

impl std::error::Error for AgentLlmError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detail_returns_the_provider_wording_only_where_the_variant_carries_it() {
        // Display-only: callers show this under the typed headline, never branch on it.
        use AgentLlmError as E;
        assert_eq!(E::AuthFailed("bad key".into()).detail(), Some("bad key"));
        assert_eq!(E::RateLimited("slow down".into()).detail(), Some("slow down"));
        assert_eq!(E::ModelNotInPlan("limit 0".into()).detail(), Some("limit 0"));
        assert_eq!(
            E::Provider("HTTP 404: model gone".into()).detail(),
            Some("HTTP 404: model gone")
        );
        assert_eq!(E::NoKey.detail(), None);
        assert_eq!(E::NotConfigured.detail(), None);
        assert_eq!(E::Unavailable.detail(), None);
        assert_eq!(E::Timeout.detail(), None);
        assert_eq!(E::BudgetExhausted.detail(), None);
    }

    fn use_tool(arguments: serde_json::Value) -> AgentToolCall {
        AgentToolCall {
            call_id: "call-1".to_string(),
            tool: ToolId::UseTool,
            arguments,
            reasoning: Some(ReasoningState {
                provider: ProviderTag::Anthropic,
                blob: json!("opaque"),
            }),
        }
    }

    #[test]
    fn a_well_formed_use_tool_is_the_tool_it_names_under_the_same_call_id() {
        let call = use_tool(json!({ "name": "tag", "params": { "pane": "left" } }));
        let effective = call.effective();
        assert_eq!(effective.call_id, "call-1");
        assert_eq!(effective.tool, ToolId::Tag);
        assert_eq!(effective.arguments, json!({ "pane": "left" }));
        assert_eq!(effective.identity_key(), {
            let direct = AgentToolCall {
                call_id: "other".to_string(),
                tool: ToolId::Tag,
                arguments: json!({ "pane": "left" }),
                reasoning: None,
            };
            direct.identity_key()
        });
    }

    #[test]
    fn no_params_or_null_params_mean_no_arguments() {
        for arguments in [
            json!({ "name": "nav_back" }),
            json!({ "name": "nav_back", "params": null }),
        ] {
            let call = use_tool(arguments);
            assert_eq!(call.effective().tool, ToolId::NavBack);
            assert_eq!(call.effective().arguments, json!({}));
        }
    }

    #[test]
    fn a_name_outside_the_table_is_unrecognized_the_way_any_provider_name_is() {
        let call = use_tool(json!({ "name": "format_drive" }));
        assert_eq!(call.effective().tool, ToolId::Unrecognized("format_drive".into()));
    }

    /// Stays `use_tool`, so its own handler says why: nesting would hide the call from everything
    /// reading the effective one, and a stray key would be dropped and acted on without.
    #[test]
    fn anything_malformed_stays_itself() {
        for arguments in [
            json!({}),
            json!("use_tool"),
            json!({ "name": "" }),
            json!({ "name": "   " }),
            json!({ "name": 3 }),
            json!({ "name": "use_tool", "params": { "name": "delete" } }),
            json!({ "name": "tool_details", "params": { "names": ["delete"] } }),
            json!({ "name": "list_dir", "params": "{\"path\":\"/tmp\"}" }),
            json!({ "name": "list_dir", "params": ["/tmp"] }),
            json!({ "name": "tag", "pane": "left" }),
        ] {
            let call = use_tool(arguments.clone());
            let effective = call.effective();
            assert_eq!(effective.tool, ToolId::UseTool, "{arguments}");
            assert!(matches!(effective, Cow::Borrowed(_)), "{arguments}");
        }
    }

    #[test]
    fn every_other_call_is_itself() {
        let call = AgentToolCall {
            call_id: "c".to_string(),
            tool: ToolId::ListDir,
            arguments: json!({ "name": "delete" }),
            reasoning: None,
        };
        assert!(matches!(call.effective(), Cow::Borrowed(_)));
    }

    #[test]
    fn tool_id_serializes_as_bare_wire_name() {
        assert_eq!(serde_json::to_string(&ToolId::AppState).unwrap(), "\"app_state\"");
        assert_eq!(
            serde_json::to_string(&ToolId::Unrecognized("format_drive".into())).unwrap(),
            "\"format_drive\""
        );
    }

    #[test]
    fn tool_id_from_wire_name_gate_shape() {
        // A table name resolves to its variant, whoever may run it; any other is `Unrecognized`.
        assert_eq!(ToolId::from_wire_name("app_state"), ToolId::AppState);
        assert!(ToolId::from_wire_name("app_state").is_known());
        assert_eq!(ToolId::from_wire_name("delete"), ToolId::Delete);
        assert_eq!(
            ToolId::from_wire_name("format_drive"),
            ToolId::Unrecognized("format_drive".into())
        );
        assert!(!ToolId::from_wire_name("format_drive").is_known());
        // Round-trips through serde as the same bare string.
        let round: ToolId = serde_json::from_str("\"copy\"").unwrap();
        assert_eq!(round, ToolId::Copy);
    }

    #[test]
    fn tool_id_known_wire_names_round_trip() {
        // Every KNOWN variant round-trips through its wire name, and none collapses
        // to Unrecognized.
        for tool in ToolId::KNOWN {
            let name = tool.as_wire_name();
            assert_eq!(ToolId::from_wire_name(name), tool);
            assert!(tool.is_known());
        }
    }

    #[test]
    fn agent_message_serde_round_trip_preserves_reasoning_blob() {
        // DB persistence fidelity: a tool-call-with-reasoning survives serialize →
        // parse without losing the opaque provider blob. This is the invariant the
        // typed-parts model exists to protect (never flatten to content+reasoning).
        let msg = AgentMessage {
            role: AgentRole::Assistant,
            parts: vec![AgentPart::ToolCall(AgentToolCall {
                call_id: "call-1".into(),
                tool: ToolId::AppState,
                arguments: json!({ "path": "/Users/x" }),
                reasoning: Some(ReasoningState {
                    provider: ProviderTag::Gemini,
                    blob: json!({ "thought_signatures": ["sig-abc"] }),
                }),
            })],
            at: 1_000,
        };
        let encoded = serde_json::to_string(&msg).unwrap();
        let decoded: AgentMessage = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, msg);
    }
}
