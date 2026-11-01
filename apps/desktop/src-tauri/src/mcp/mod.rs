//! MCP (Model Context Protocol) server module.
//!
//! Provides a Streamable HTTP server that exposes cmdr functionality as MCP tools,
//! enabling AI agents to control the file manager.

mod auth;
pub mod config;
mod executor;
pub mod port_file;
mod protocol;
pub mod resources;
mod safe_headers;
mod server;
mod tool_registry;

#[cfg(test)]
mod tests;

pub use auth::current_mcp_token;
pub use config::McpConfig;

// The agent runtime (`crate::agent`) is the registry's second consumer (agent decision D49):
// it dispatches the read-only `Consumer::Agent` view in-process. These are the exact
// surface it needs — the dispatch entry, the agent view, the consumer/access tokens, and
// the tool result types its handlers return. Deliberately narrow so the agent can't reach
// the ai-client dispatch or the auth gate.
pub(crate) use executor::{ToolError, ToolResult, fit_to_result_budget, is_virtual_path};

// "Go there and point at that" is this module's frontend protocol (`mcp-tab` +
// `mcp-nav-to-path` + `mcp-move-cursor` + `mcp-select-names`), but it isn't only an
// agent's move: an OS reveal and the `cmdr` terminal command (`crate::reveal`,
// `crate::cli`) make exactly the same one. Exported as a named seam rather than by
// opening `executor` up, so the rest of the crate reaches this and nothing else.
//
// Gated because `crate::reveal` and `crate::cli` are the seam's only users and are
// themselves macOS-only: the in-module caller (`executor::downloads`) reaches `nav`
// directly, so off macOS this re-export has nobody, and `-D unused` is an error. CI lints
// on Linux and we lint on macOS, so a missing gate here compiles clean locally and breaks
// the whole build there.
#[cfg(target_os = "macos")]
pub(crate) use executor::nav::{GoToTarget, PaneChoice, go_to_in_pane};
pub use server::{
    McpServerOutcome, get_mcp_actual_port, is_mcp_running, rebind_interactive, start_mcp_server_background,
    stop_mcp_server, stop_mcp_server_and_wait,
};
pub(crate) use tool_registry::params;
pub(crate) use tool_registry::{Access, Consumer, agent_tool_view, execute_tool, tool_access, validate_params};
