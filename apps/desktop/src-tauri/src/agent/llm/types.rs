//! The typed message-part model the `AgentLlm` seam speaks.
//!
//! The genai spike proved the make-or-break invariant this file encodes: an
//! assistant turn is an ordered list of **typed parts** (text, tool call,
//! tool result, reasoning), and opaque reasoning state is **provider-tagged and
//! rides on the part that owns it** — never flattened to `content: String +
//! reasoning: String`. That lossy shape is exactly what breaks a multi-step tool
//! loop on step 3 (spike Gaps A/B): the provider blob has to be re-attached to
//! the right structural position, so it cannot be concatenated into a text field.
//! See `DETAILS.md` for the mapping table and the spike-gap rationale.
//!
//! These types carry `serde` for DB persistence (`main.db` `content_blocks`)
//! and are pure data — no dependency on `genai` or `crate::ai`. The genai
//! coupling lives entirely in `genai_impl.rs`. The reasoning `blob` is a
//! **backend-only** value: it is persisted and replayed, but never crosses to the
//! frontend (the wire `MessageView` carries display parts only).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    /// change, renaming a variant is free (`no-string-matching`).
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
/// items — spike Gap B).
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
    /// form so neither can drift the other (`no-string-matching`).
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

/// Opaque, provider-tagged reasoning state. The `blob` shape is owned solely by
/// the genai adapter mapping (`genai_impl.rs`); everything else treats it as
/// opaque and must persist and replay it untouched. Never inspect or reshape it
/// outside the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReasoningState {
    pub provider: ProviderTag,
    pub blob: serde_json::Value,
}

/// A typed identifier for a read-only agent tool.
///
/// The known variants map 1:1 onto the registry's `agent_tool_view()` entries (the
/// read-only families: live app state, drive-index listing + stats, importance,
/// operation-log search + detail, and the volume list). A structural test in
/// `agent/tools` pins that 1:1 mapping so a variant and its registry entry can't
/// drift.
///
/// [`ToolId::Unrecognized`] is the read-only choke point: a provider returns each
/// tool call's name as a raw string, and any name that is not a known read-only
/// tool resolves here. `Unrecognized` is never a member of `agent_tool_view()` and
/// carries no dispatch path, so the runtime refuses it before ever reaching
/// `execute_tool` — the gate is this typed parse step, never a string match on the
/// name (`no-string-matching`). It is deliberately excluded from [`ToolId::KNOWN`]
/// and the 1:1 test.
///
/// Serializes transparently as its wire name (a bare string) so the DB token, the
/// genai `fn_name`, and the IPC form are one identical value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolId {
    /// Live app-state snapshot: both panes' path/cursor/selection plus the volumes.
    AppState,
    /// One directory's immediate children plus its recursive size stats, from the
    /// drive index.
    ListDir,
    /// The largest subdirectories under a path, by recursive size (batches dir
    /// stats and sorts).
    LargestDirs,
    /// The most important folders across scored volumes (top-N or above a
    /// threshold), offline-capable.
    ImportantFolders,
    /// One folder's importance: scored (with the signal breakdown), floored, or
    /// unscored.
    FolderImportance,
    /// The volume list with per-volume index freshness and SMB connectivity.
    ListVolumes,
    /// Search the durable operation log (shared with the ai-client view).
    OperationsList,
    /// One logged operation's header plus a page of its item rows (shared).
    OperationsGet,
    /// A tool name the agent does not recognize (hallucinated, a typo, or a
    /// write/non-view tool). Carries the raw name for the transparent UI and the
    /// typed "tool not available" result; always refused by dispatch.
    Unrecognized(String),
}

impl ToolId {
    /// Every known read-only variant, in wire order. Excludes [`ToolId::Unrecognized`]
    /// by design (it's the refusal case, never a view entry). The 1:1 structural test
    /// asserts these map exactly onto `agent_tool_view()`.
    pub const KNOWN: [ToolId; 8] = [
        ToolId::AppState,
        ToolId::ListDir,
        ToolId::LargestDirs,
        ToolId::ImportantFolders,
        ToolId::FolderImportance,
        ToolId::ListVolumes,
        ToolId::OperationsList,
        ToolId::OperationsGet,
    ];

    /// The wire name for this tool: the genai `fn_name`, the DB token, and the IPC
    /// string, all one value.
    pub fn as_wire_name(&self) -> &str {
        match self {
            ToolId::AppState => "app_state",
            ToolId::ListDir => "list_dir",
            ToolId::LargestDirs => "largest_dirs",
            ToolId::ImportantFolders => "important_folders",
            ToolId::FolderImportance => "folder_importance",
            ToolId::ListVolumes => "list_volumes",
            ToolId::OperationsList => "operations_list",
            ToolId::OperationsGet => "operations_get",
            ToolId::Unrecognized(name) => name.as_str(),
        }
    }

    /// Resolves a raw provider-supplied tool name to a typed [`ToolId`]. Total: an
    /// unknown name resolves to [`ToolId::Unrecognized`] rather than failing, so the
    /// raw name stays representable for the transparent UI and the refusal result.
    /// The known set is exactly `agent_tool_view()`, pinned by the 1:1 test.
    pub fn from_wire_name(name: &str) -> Self {
        match name {
            "app_state" => ToolId::AppState,
            "list_dir" => ToolId::ListDir,
            "largest_dirs" => ToolId::LargestDirs,
            "important_folders" => ToolId::ImportantFolders,
            "folder_importance" => ToolId::FolderImportance,
            "list_volumes" => ToolId::ListVolumes,
            "operations_list" => ToolId::OperationsList,
            "operations_get" => ToolId::OperationsGet,
            other => ToolId::Unrecognized(other.to_string()),
        }
    }

    /// True when this is a recognized read-only tool. `Unrecognized` is the only
    /// false case. The authoritative gate is `agent_tool_view()` membership, which
    /// this shape mirrors.
    pub fn is_known(&self) -> bool {
        !matches!(self, ToolId::Unrecognized(_))
    }
}

impl Serialize for ToolId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_wire_name())
    }
}

impl<'de> Deserialize<'de> for ToolId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Ok(ToolId::from_wire_name(&name))
    }
}

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

/// The result of executing a tool, fed back to the model. `elided` records that
/// the runtime collapsed this result to a stub for the context budget; the
/// content already reflects the elision when set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolResult {
    pub call_id: String,
    pub content: serde_json::Value,
    pub elided: bool,
}

/// One ordered content part of a message. The variant keys serialize snake_case
/// (`text`, `tool_call`, `tool_result`, `reasoning`) for a plainly-inspectable DB
/// `content_blocks` JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentPart {
    Text(String),
    ToolCall(AgentToolCall),
    ToolResult(AgentToolResult),
    /// Opaque reasoning state; persisted and replayed untouched, never shown.
    Reasoning(ReasoningState),
}

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
/// description and JSON schema. NEVER emitted with `strict: true` (spike Gap D:
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
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
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
/// by message-string matching (`no-string-matching`); `Provider` carries a detail
/// string for display only, never for control flow.
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
    /// The provider rejected the API key (HTTP 401 / 403).
    AuthFailed,
    /// The provider is rate-limiting or the account is out of quota (HTTP 429).
    RateLimited,
    /// A per-message budget (tool turns / wall time / tokens) was exhausted.
    BudgetExhausted,
    /// Any other provider-side failure; the string is for display only.
    Provider(String),
}

impl std::fmt::Display for AgentLlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoKey => write!(f, "no API key configured"),
            Self::NotConfigured => write!(f, "the AI provider is not configured"),
            Self::Unavailable => write!(f, "the AI provider is unavailable"),
            Self::Timeout => write!(f, "the AI request timed out"),
            Self::AuthFailed => write!(f, "the AI provider rejected the API key"),
            Self::RateLimited => write!(f, "the AI provider is rate-limiting or out of quota"),
            Self::BudgetExhausted => write!(f, "the message budget was exhausted"),
            Self::Provider(detail) => write!(f, "the AI provider returned a problem: {detail}"),
        }
    }
}

impl std::error::Error for AgentLlmError {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tool_id_serializes_as_bare_wire_name() {
        assert_eq!(serde_json::to_string(&ToolId::AppState).unwrap(), "\"app_state\"");
        assert_eq!(
            serde_json::to_string(&ToolId::Unrecognized("delete".into())).unwrap(),
            "\"delete\""
        );
    }

    #[test]
    fn tool_id_from_wire_name_gate_shape() {
        // A known name resolves to its variant; an unknown one is `Unrecognized`,
        // never a valid view entry — the shape the read-only gate builds on.
        assert_eq!(ToolId::from_wire_name("app_state"), ToolId::AppState);
        assert!(ToolId::from_wire_name("app_state").is_known());
        assert_eq!(ToolId::from_wire_name("delete"), ToolId::Unrecognized("delete".into()));
        assert!(!ToolId::from_wire_name("delete").is_known());
        // Round-trips through serde as the same bare string.
        let round: ToolId = serde_json::from_str("\"copy\"").unwrap();
        assert_eq!(round, ToolId::Unrecognized("copy".into()));
    }

    #[test]
    fn tool_id_known_wire_names_round_trip() {
        // Every KNOWN variant round-trips through its wire name, and none collapses
        // to Unrecognized — the invariant the 1:1 registry test relies on.
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
