//! The `read_resource` tool: the `cmdr://` views as a tool result, for a caller with no
//! `resources/read` (the agent). It adds only a size bound: the text comes from the same
//! [`read_resource`](super::read_resource) an outside client's `resources/read` calls, byte for byte.

use serde::Serialize;
use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use crate::agent::chat::budget::{CHARS_PER_TOKEN_ESTIMATE, MAX_TOOL_RESULT_TOKENS};
use crate::tools::sensitive::Protected;
use crate::tools::{ToolError, ToolResult};

/// Why a view didn't read. Typed, so nobody reads a sentence to find out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceError {
    UnknownUri(String),
    /// `cmdr://logs`: what the log holds stays home.
    Protected(Protected),
    /// `cmdr://settings` is built by the webview, which didn't answer in time.
    WindowSilent,
    /// A store or file behind the view isn't there. The message says which.
    Unavailable(String),
}

impl From<ResourceError> for ToolError {
    fn from(error: ResourceError) -> Self {
        match error {
            ResourceError::UnknownUri(uri) => ToolError::invalid_params(format!(
                "Unknown resource URI: {uri}. It takes cmdr://state, cmdr://settings, cmdr://logs, cmdr://indexing, cmdr://importance, or cmdr://dialogs/available."
            ))
            .with_data(json!({ "reason": "unknownResource" })),
            ResourceError::Protected(reason) => ToolError::invalid_params(
                "Cmdr's recent log holds text that looks like a private key, so Cmdr doesn't hand it to AI tools. \
                 Tell the user; asking again won't change it until the log moves on.",
            )
            .with_data(json!({ "refusal": "protected", "reason": reason })),
            ResourceError::WindowSilent => ToolError::invalid_params(
                "Cmdr's window didn't answer in time, so this can't be read right now. Say so rather than guessing at it.",
            )
            .with_data(json!({ "reason": "windowSilent" })),
            ResourceError::Unavailable(detail) => ToolError::invalid_params(detail),
        }
    }
}

/// One stretch of a view. `next_offset` is where the rest starts, present when there is a rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceWindow {
    uri: String,
    text: String,
    truncated: bool,
    returned_chars: usize,
    total_chars: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_offset: Option<usize>,
}

/// What a result's fields other than `text` and `uri` can spend.
const ENVELOPE_BYTES: usize = 120;

/// What `c` costs in the result's JSON, which is what a provider reads and what the budget counts.
fn serialized_width(c: char) -> usize {
    match c {
        '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
        c if c < ' ' => 6,
        c => c.len_utf8(),
    }
}

/// The stretch of `text` from `offset` that fits [`MAX_TOOL_RESULT_TOKENS`], ending on a whole line
/// where it can, so a row a reader acts on is never half a row. Always moves on when anything's left,
/// so paging ends.
fn window_of(uri: &str, text: &str, offset: usize) -> ResourceWindow {
    let total_chars = text.chars().count();
    let budget = (MAX_TOOL_RESULT_TOKENS * CHARS_PER_TOKEN_ESTIMATE).saturating_sub(ENVELOPE_BYTES + uri.len() * 2);

    let mut spent = 0;
    let mut taken = String::new();
    let mut taken_chars = 0;
    let mut last_line_end = None;
    for c in text.chars().skip(offset) {
        spent += serialized_width(c);
        if spent > budget && taken_chars > 0 {
            break;
        }
        taken.push(c);
        taken_chars += 1;
        if c == '\n' {
            last_line_end = Some((taken.len(), taken_chars));
        }
    }

    let reached = offset.saturating_add(taken_chars);
    if reached < total_chars
        && let Some((bytes, chars)) = last_line_end
    {
        taken.truncate(bytes);
        taken_chars = chars;
    }
    let reached = offset.saturating_add(taken_chars);
    let truncated = reached < total_chars;
    ResourceWindow {
        uri: uri.to_string(),
        text: taken,
        truncated,
        returned_chars: taken_chars,
        total_chars,
        next_offset: truncated.then_some(reached),
    }
}

/// `read_resource`: one view, bounded and honest about the bound.
pub async fn execute_read_resource<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::invalid_params("Missing 'uri' parameter"))?;
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let offset = usize::try_from(offset).unwrap_or(usize::MAX);

    let content = super::read_resource(app, uri).await?;
    serde_json::to_value(window_of(uri, &content.text, offset)).map_err(|e| ToolError::internal(e.to_string()))
}

#[cfg(test)]
#[path = "read_tool_tests.rs"]
mod tests;
