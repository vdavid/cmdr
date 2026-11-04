//! The write, queue, and undo commands, answered the way the desktop's Tauri
//! commands answer them: the same names, the same camelCase arguments, the same
//! JSON back, through the same engine calls. One file per domain, as the
//! desktop's `commands/` splits them.
//!
//! Where the desktop's command does host work the page has none of (tilde
//! expansion, S3 cost pricing, the OS trash), the demo answers what a volume
//! without it would; `DETAILS.md` § "Where the demo differs from the app".

mod history;
mod instant;
mod queue;
mod transfers;

use serde::Deserialize;
use serde_json::Value;

use super::DemoOperations;

/// The arguments of every command that names one operation.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationIdArgs {
    operation_id: String,
}

/// The arguments of every command that names several.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationIdsArgs {
    operation_ids: Vec<String>,
}

/// Every command group, by its Tauri names.
const GROUPS: &[&[&str]] = &[transfers::NAMES, instant::NAMES, queue::NAMES, history::NAMES];

/// Whether [`DemoOperations::invoke`] answers `command`.
pub(super) fn handles(command: &str) -> bool {
    GROUPS.iter().any(|names| names.contains(&command))
}

/// Every command [`DemoOperations::invoke`] answers.
pub(super) fn names() -> impl Iterator<Item = &'static str> {
    GROUPS.iter().flat_map(|names| names.iter().copied())
}

impl DemoOperations {
    /// Runs `command` with Tauri-style `args` (camelCase keys): `Ok` is the
    /// command's answer, `Err` what the promise rejects with.
    pub async fn invoke(&self, command: &str, args: Value) -> Result<Value, Value> {
        if transfers::NAMES.contains(&command) {
            self.transfers(command, args).await
        } else if instant::NAMES.contains(&command) {
            self.instant(command, args).await
        } else if queue::NAMES.contains(&command) {
            self.queue(command, args).await
        } else if history::NAMES.contains(&command) {
            self.history(command, args).await
        } else {
            Err(Value::String(format!("the demo doesn't answer `{command}`")))
        }
    }
}
