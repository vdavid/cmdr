//! The bearer-token gate dimension of the registry.

use serde_json::Value;

/// How a tool relates to the bearer-token gate. Pure, non-generic, and unit-testable — it
/// reproduces the previous `tool_call_requires_token` classification exactly, and `auth.rs`
/// reads it via [`tool_gate`](super::tool_gate). See `mcp/DETAILS.md` § Authentication for the
/// threat model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenGate {
    /// No token needed: reads, nav, search, and destructive ops that still prompt the user.
    Open,
    /// Always gated: config mutation that applies with no user confirmation (`set_setting`).
    Always,
    /// Gated iff `arguments.autoConfirm == true`: `copy` / `move` / `delete`.
    IfAutoConfirm,
    /// Gated iff `arguments.action == "confirm"`: the `dialog` tool.
    IfConfirmAction,
    /// Gated iff `arguments.rollback == true`: the `queue` tool's cancel action.
    /// Plain pause/resume/cancel are transient runtime actions (Open), but a
    /// rollback cancel actively DELETES already-copied files with no confirmation
    /// dialog — the same "auto-confirm a destructive thing" shape the token guards.
    IfRollback,
}

impl TokenGate {
    /// Whether a call with these `arguments` (the JSON-RPC `params.arguments` object) requires
    /// the bearer token. `IfConfirmAction` reads the tool's own typed `action` enum, not a
    /// message substring, so it's not a `no-string-matching` violation.
    pub fn requires_token(self, arguments: Option<&Value>) -> bool {
        match self {
            TokenGate::Open => false,
            TokenGate::Always => true,
            TokenGate::IfAutoConfirm => arguments
                .and_then(|a| a.get("autoConfirm"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            TokenGate::IfConfirmAction => {
                arguments.and_then(|a| a.get("action")).and_then(|v| v.as_str()) == Some("confirm")
            }
            TokenGate::IfRollback => arguments
                .and_then(|a| a.get("rollback"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }
}
