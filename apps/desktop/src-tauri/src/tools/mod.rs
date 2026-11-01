//! Every AI-callable tool, for both callers: the authored table, its handlers, the sensitive-content
//! gate, and the `cmdr://` resources. `mcp/` carries them to outside clients, and `agent/` runs them
//! in-process.
//!
//! - [`registry`] and `table.rs`: the mechanism and the data. One `mcp_tools!` table generates
//!   every view and the dispatch.
//! - `actions/`: the handlers behind most tools. `read/`, `propose/`, `suggestions/`, `memory.rs`,
//!   `connect.rs`, and `quiet.rs`: the handlers written for the agent.
//! - [`sensitive`]: the one gate every path yielding file contents passes.
//! - [`resources`]: the read-only `cmdr://` views.
//! - [`agent_tool_declarations`]: the `ToolDeclaration`s handed to the provider, built from the
//!   table's `agent_tool_view()`.
//!
//! See `CLAUDE.md` for the must-knows and `DETAILS.md` for the tool catalog.

mod actions;
pub mod connect;
pub mod error_codes;
pub mod memory;
pub mod params;
pub mod propose;
pub mod quiet;
pub mod read;
pub mod resources;
mod schemas;
pub mod sensitive;
pub mod suggestions;

// `table.rs` expands the `mcp_tools!` macro that `registry` defines, so `registry` comes first.
#[macro_use]
mod registry;
mod table;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod transcript_179_tests;

// The tool result types every handler returns, and the size fitting they share.
pub(crate) use actions::{ToolError, ToolResult, fit_to_result_budget, is_virtual_path};

// "Go there and point at that" is this module's frontend protocol (`mcp-tab` +
// `mcp-nav-to-path` + `mcp-move-cursor` + `mcp-select-names`), but it isn't only an
// agent's move: an OS reveal and the `cmdr` terminal command (`crate::reveal`,
// `crate::cli`) make exactly the same one. Exported as a named seam rather than by
// opening `actions` up, so the rest of the crate reaches this and nothing else.
//
// Gated because `crate::reveal` and `crate::cli` are the seam's only users and are
// themselves macOS-only: the in-module caller (`actions::downloads`) reaches `nav`
// directly, so off macOS this re-export has nobody, and `-D unused` is an error. CI lints
// on Linux and we lint on macOS, so a missing gate here compiles clean locally and breaks
// the whole build there.
#[cfg(target_os = "macos")]
pub(crate) use actions::nav::{GoToTarget, PaneChoice, go_to_in_pane};
pub(crate) use registry::{Access, Consumer, validate_params};
#[cfg(test)]
use registry::{Tool, tool_available_to};
pub(crate) use table::{agent_tool_view, execute_tool, get_all_tools, tool_access};
use table::{tool_consumers, tool_schema};

use crate::agent::llm::types::{ToolDeclaration, ToolId};

/// The tool declarations the agent hands the provider, one per `agent_tool_view()`
/// entry. Each name resolves to a typed [`ToolId`] (the 1:1 test pins that every
/// view entry maps to a known variant). Schemas are never `strict: true`
/// ([`ToolDeclaration`] carries no strict flag — spike Gap D).
pub fn agent_tool_declarations() -> Vec<ToolDeclaration> {
    agent_tool_view()
        .into_iter()
        .map(|t| ToolDeclaration {
            name: ToolId::from_wire_name(&t.name),
            description: t.description,
            schema: t.input_schema,
        })
        .collect()
}
