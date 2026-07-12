//! JSON input schemas for the `mcp_tools!` table, one `fn <tool>_schema() -> Value` per tool.
//!
//! Hoisted out of the table so the authored registry (`../mod.rs`) stays a lean declaration:
//! the schema blocks dominate the line count and read the same wherever they live. Each function
//! returns the exact `json!` block the wire fixture pins, so the split changes no bytes
//! (`tests/tool_snapshot_tests.rs`). Grouped by the table's tool categories.

use serde_json::{Value, json};

mod async_tools;
mod dialogs;
mod favorites;
mod file_ops;
mod indexing;
mod nav;
mod network;
mod operation_log;
mod queue;
mod search;
mod settings;
mod view;

pub use async_tools::*;
pub use dialogs::*;
pub use favorites::*;
pub use file_ops::*;
pub use indexing::*;
pub use nav::*;
pub use network::*;
pub use operation_log::*;
pub use queue::*;
pub use search::*;
pub use settings::*;
pub use view::*;

/// The empty input schema shared by every no-parameter tool.
pub fn no_params_schema() -> Value {
    json!({
        "type": "object",
        "properties": {},
        "required": []
    })
}
