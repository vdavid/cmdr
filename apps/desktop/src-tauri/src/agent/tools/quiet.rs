//! The `nothing_to_suggest` agent tool: how a wake says it found nothing worth raising.
//!
//! ## Why a tool rather than a phrasing
//!
//! A wake that looked and found nothing must leave no thread behind, or the rail fills with
//! "we had a look and it was fine" fifty times over. Deciding that from the model's WORDING
//! would be classifying control flow by text, which `error-string-match` forbids and which
//! breaks on the first copy edit or non-English reply. So the model says it with a typed call
//! and the wake reads [`ToolId::NothingToSuggest`](crate::agent::llm::types::ToolId).
//!
//! ## Why the handler does nothing
//!
//! ⚠️ **It is a pure signal**, `Access::Read`, mutating nothing. A handler that deleted the
//! thread would be `Access::Write` under the registry's tiebreaker and would fail
//! `test_agent_tool_view_never_writes` — and it would delete a USER's thread when the rail
//! called it, since there is one `agent_tool_view()` for both. The deletion belongs to the
//! wake path, after the turn (`agent/wake/`).

use serde_json::Value;
use tauri::{AppHandle, Runtime};

use crate::mcp::ToolResult;

/// The most of a `reason` worth keeping. It exists for the agent's own memory (a later
/// milestone), so a model that writes an essay into it is trimmed rather than obeyed.
pub const MAX_REASON_CHARS: usize = 280;

/// One required argument, a short reason. The schema rides in the cached prefix of every
/// turn the RAIL runs too, so it stays as small as it can be.
pub fn nothing_to_suggest_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "reason": {
                "type": "string",
                "description": "One short sentence on why none of it was worth raising."
            }
        },
        "required": ["reason"],
        "additionalProperties": false
    })
}

/// The reason the model gave, trimmed to [`MAX_REASON_CHARS`]. `None` when it gave none or
/// gave blank text: the call is the signal, and the reason is a bonus.
///
/// ❌ **Never log this verbatim.** `cmdr.log` ships inside error reports, including the
/// auto-dispatched ones the user never previews, and the redactor
/// (`redact::redact_line_salted`) is path-shaped, so it does nothing to prose. A sentence
/// about which of the user's folders were boring is exactly the thing that must not travel.
pub fn reason_of(arguments: &Value) -> Option<String> {
    let text = arguments.get("reason")?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    Some(text.chars().take(MAX_REASON_CHARS).collect())
}

/// Handler: acknowledge, and change nothing. The whole effect of this tool is that the call
/// happened, which the wake path observes typed.
pub async fn execute_nothing_to_suggest<R: Runtime>(_app: &AppHandle<R>, _params: &Value) -> ToolResult {
    Ok(serde_json::json!({ "acknowledged": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A reason is a short sentence or it is nothing: blank, absent, and wrong-typed all read
    /// as "no reason", and a long one is trimmed rather than carried whole.
    #[test]
    fn a_reason_is_short_or_absent() {
        assert_eq!(
            reason_of(&json!({ "reason": "  all cache churn  " })).as_deref(),
            Some("all cache churn")
        );
        assert_eq!(reason_of(&json!({ "reason": "   " })), None);
        assert_eq!(reason_of(&json!({})), None);
        assert_eq!(reason_of(&json!({ "reason": 7 })), None);

        let long = reason_of(&json!({ "reason": "x".repeat(MAX_REASON_CHARS + 50) })).expect("a reason");
        assert_eq!(long.chars().count(), MAX_REASON_CHARS);
    }
}
