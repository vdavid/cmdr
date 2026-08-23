//! The two memory tools: the thin `AppHandle` half over the pure `MemoryStore`.
//!
//! Everything that decides anything — the jail, the caps, the edit's uniqueness rule — lives in
//! `agent/memory/` and is unit-tested against a `tempdir`. What is left here is resolving the
//! root out of the app data dir and shaping the answer, so the untestable half (there is no
//! Tauri mock runtime in the tree) holds no rules.
//!
//! ⚠️ **These are callable from the rail, not only from wakes.** That is intended: "remember
//! that I keep invoices by year" is exactly what the folder is for. It is also the mechanism
//! behind the injection risk `chat/context.rs` fences against, so it is said out loud here
//! rather than implied.
//!
//! ⚠️ **A refusal comes back as an `Ok` result carrying a typed token**, never as a `ToolError`.
//! `view::dispatch` flattens a `ToolError` to `{ "problem": <sentence> }`, and a model that has
//! to read prose to learn its memory is full will keep writing into a folder that is not saving
//! anything. `error-string-match` is the same rule pointed at the model.

use serde_json::{Value, json};
use tauri::{AppHandle, Runtime};

use crate::agent::memory::{MEMORY_DIR_MAX_BYTES, MemoryRefusal, MemoryWritten};
use crate::mcp::{ToolError, ToolResult};

/// `memory_write`'s arguments. Terse on purpose: every schema rides in the cached prefix of
/// every turn, the rail's included.
pub fn memory_write_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": {
                "type": "string",
                "description": "Relative .md path inside memory, usually AGENTS.md."
            },
            "content": { "type": "string", "description": "The file's full new text." }
        },
        "required": ["path", "content"],
        "additionalProperties": false
    })
}

/// `memory_edit`'s arguments.
pub fn memory_edit_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "path": { "type": "string", "description": "Relative .md path inside memory." },
            "oldString": { "type": "string", "description": "Text to replace. Must appear exactly once." },
            "newString": { "type": "string", "description": "What replaces it. Empty deletes it." }
        },
        "required": ["path", "oldString", "newString"],
        "additionalProperties": false
    })
}

/// Create or fully replace one memory file.
pub async fn execute_memory_write<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let path = string_arg(params, "path")?;
    let content = string_arg(params, "content")?;
    let Some(store) = crate::agent::memory::store_for(app) else {
        return Ok(unavailable());
    };
    Ok(answer(store.write(&path, &content)))
}

/// Replace one exact, unique occurrence inside one memory file.
pub async fn execute_memory_edit<R: Runtime>(app: &AppHandle<R>, params: &Value) -> ToolResult {
    let path = string_arg(params, "path")?;
    let old = string_arg(params, "oldString")?;
    let new = string_arg(params, "newString")?;
    let Some(store) = crate::agent::memory::store_for(app) else {
        return Ok(unavailable());
    };
    Ok(answer(store.edit(&path, &old, &new)))
}

/// One required string argument, or the invalid-params error the registry reports.
fn string_arg(params: &Value, name: &str) -> Result<String, ToolError> {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ToolError::invalid_params(format!("'{name}' is required and must be a string")))
}

/// The tool result for either tool: what landed, or the typed reason nothing did.
fn answer(outcome: Result<MemoryWritten, MemoryRefusal>) -> Value {
    match outcome {
        Ok(written) => json!({
            "saved": true,
            "path": written.path,
            "bytes": written.bytes,
            "remainingBytes": written.remaining_bytes,
        }),
        Err(refusal) => json!({
            "saved": false,
            "refused": refusal.token(),
            "detail": detail_of(&refusal),
        }),
    }
}

/// What the model is told, beside the token it actually branches on. Each one names the way
/// out, because a refusal with no next move gets retried verbatim.
fn detail_of(refusal: &MemoryRefusal) -> String {
    match refusal {
        MemoryRefusal::OutsideMemory => {
            "That path leaves your memory folder. Use a plain relative name like AGENTS.md.".to_string()
        }
        MemoryRefusal::NotMarkdown => "Memory holds Markdown only, so the name has to end in .md.".to_string(),
        MemoryRefusal::NoPath => "No file name was given.".to_string(),
        MemoryRefusal::DirectoryFull { used, wanted, .. } => format!(
            "Memory is full: {used} bytes of {MEMORY_DIR_MAX_BYTES} used, and this write wants {wanted}. \
             Prune what has gone stale with memory_edit, then write again."
        ),
        MemoryRefusal::NoSuchFile => "There is no such memory file yet. Use memory_write to start it.".to_string(),
        MemoryRefusal::NoMatch => {
            "That text is not in the file, so nothing was changed. Rewrite the file with memory_write instead."
                .to_string()
        }
        MemoryRefusal::NotUnique { matches } => format!(
            "That text appears {matches} times, so nothing was changed. Include enough surrounding lines to \
             pick out the one you mean."
        ),
        MemoryRefusal::Unwritable(detail) => format!("Memory could not be saved: {detail}"),
    }
}

/// The data dir would not resolve, so there is nowhere to remember anything. Shaped like every
/// other refusal so the model reads one contract.
fn unavailable() -> Value {
    json!({
        "saved": false,
        "refused": "unavailable",
        "detail": "Memory is not available in this session.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::memory::MemoryStore;

    /// The tool's whole job beyond resolving a root: turn a typed outcome into a shape the
    /// model can branch on. The refusal carries a TOKEN, so nothing downstream — the model
    /// included — has to read the sentence to know what happened.
    #[test]
    fn a_refusal_carries_a_token_and_a_way_out() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = MemoryStore::new(dir.path().join("memory"));

        let refused = answer(store.write("../escaped.md", "x"));

        assert_eq!(refused["saved"], false);
        assert_eq!(refused["refused"], "outsideMemory");
        assert!(
            refused["detail"].as_str().is_some_and(|d| d.contains("AGENTS.md")),
            "a refusal with no next move gets retried verbatim: {refused}"
        );
    }

    #[test]
    fn a_landed_write_reports_what_is_left() {
        let dir = tempfile::tempdir().expect("temp dir");
        let store = MemoryStore::new(dir.path().join("memory"));

        let saved = answer(store.write("AGENTS.md", "Keeps invoices by year."));

        assert_eq!(saved["saved"], true);
        assert_eq!(saved["bytes"], 23);
        assert!(saved["remainingBytes"].as_u64().is_some_and(|left| left > 0));
    }

    /// Every refusal has to name a next move, or the model tries the same call again.
    #[test]
    fn every_refusal_names_a_next_move() {
        for refusal in [
            MemoryRefusal::OutsideMemory,
            MemoryRefusal::NotMarkdown,
            MemoryRefusal::NoPath,
            MemoryRefusal::DirectoryFull {
                used: 1,
                cap: 2,
                wanted: 3,
            },
            MemoryRefusal::NoSuchFile,
            MemoryRefusal::NoMatch,
            MemoryRefusal::NotUnique { matches: 3 },
            MemoryRefusal::Unwritable("disk full".to_string()),
        ] {
            let detail = detail_of(&refusal);
            assert!(!detail.is_empty(), "{} has no sentence", refusal.token());
            assert!(
                !detail.contains("error") && !detail.contains("failed"),
                "{}: {detail}",
                refusal.token()
            );
        }
    }

    #[test]
    fn a_missing_argument_is_an_invalid_params_error_rather_than_a_panic() {
        assert!(string_arg(&json!({}), "path").is_err());
        assert!(string_arg(&json!({ "path": 7 }), "path").is_err());
        assert_eq!(
            string_arg(&json!({ "path": "AGENTS.md" }), "path").expect("a path"),
            "AGENTS.md"
        );
    }
}
