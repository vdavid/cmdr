//! MCP tool definition type.
//!
//! The `Tool` struct is the `tools/list` element (serde `camelCase`, so `input_schema`
//! serializes as `inputSchema`). The tools themselves are authored once in
//! [`super::tool_registry`]; `get_all_tools` is re-exported here so consumers
//! (`server.rs`, the test suite) keep importing it from `mcp::tools`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool definition for MCP.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub use super::tool_registry::get_all_tools;
