//! The `memory_diagnostics` tool: what this process is holding right now.
//!
//! Thin adapter over the `get_memory_diagnostics` IPC command (smart backend / thin
//! frontend). It dispatches no FE action and invents no ack, so it returns the backend
//! payload directly — the `indexing` / `queue` adapter shape.
//!
//! It exists so the reading can be taken from OUTSIDE the app. The command ships in
//! release builds on purpose (the interesting numbers only appear in one under a real
//! workload), but until now nothing could call it there: a running instance could be asked
//! about its panes and its index, never about its own memory.
//!
//! How to read the payload, and which VM tags hold the Rust heap under each allocator:
//! `crate::commands::memory_diagnostics` module docs and `docs/tooling/memory-debugging.md`.

use serde_json::Value;

use super::{ToolError, ToolResult};

/// Histogram groups per tag when the caller doesn't say. Enough for the fingerprint
/// method (a repeated exact region size names the allocation) without spending the
/// caller's context on the tail; the command clamps the ceiling at 24.
#[cfg(target_os = "macos")]
const DEFAULT_SIZES_PER_TAG: u32 = 8;

#[cfg(target_os = "macos")]
pub async fn execute_memory_diagnostics(params: &Value) -> ToolResult {
    let sizes_per_tag = match params.get("sizesPerTag") {
        None | Some(Value::Null) => DEFAULT_SIZES_PER_TAG,
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| ToolError::invalid_params("'sizesPerTag' must be a whole number of 0 or more"))?,
    };

    let snapshot = crate::commands::memory_diagnostics::get_memory_diagnostics(sizes_per_tag).await;
    serde_json::to_value(snapshot).map_err(|e| ToolError::internal(format!("Couldn't serialize the snapshot: {e}")))
}

/// Everywhere else the Mach queries behind the snapshot don't exist, so the honest answer
/// is a refusal rather than a payload of zeros a reader would take for a measurement.
#[cfg(not(target_os = "macos"))]
pub async fn execute_memory_diagnostics(_params: &Value) -> ToolResult {
    Err(ToolError::internal(
        "Memory diagnostics are macOS only: the Mach queries the snapshot reads don't exist on this platform.",
    ))
}
