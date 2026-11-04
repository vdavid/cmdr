//! The Tauri-shaped `invoke` plumbing every command family shares: camelCase args into a
//! typed request, and an answer or a rejection back as JSON. A leaf, so `commands.rs`
//! (which dispatches to every family) and the families it dispatches to (`viewer.rs`,
//! `operations_host/commands/`) all lean on it without leaning back on `commands.rs`.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// `args` as `T`, or the string Tauri rejects with when they don't parse.
pub(crate) fn parse<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, Value> {
    serde_json::from_value(args).map_err(|e| Value::String(format!("invalid args for command `{command}`: {e}")))
}

/// A command's answer as JSON.
pub(crate) fn ok(answer: impl Serialize) -> Result<Value, Value> {
    serde_json::to_value(answer).map_err(|e| Value::String(e.to_string()))
}

/// A fallible command's result: its answer, or its serialized error as the rejection.
pub(crate) fn answer<T: Serialize, E: Serialize>(result: Result<T, E>) -> Result<Value, Value> {
    match result {
        Ok(answer) => ok(answer),
        Err(error) => Err(ok(error)?),
    }
}
