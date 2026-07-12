//! MCP (Model Context Protocol) server module.
//!
//! Provides a Streamable HTTP server that exposes cmdr functionality as MCP tools,
//! enabling AI agents to control the file manager.

mod auth;
pub mod config;
pub mod dialog_state;
mod executor;
pub mod listing_errors;
pub mod pane_state;
pub mod port_file;
mod protocol;
pub mod resources;
mod server;
pub mod terminal_ops;
mod tool_registry;
mod tools;

#[cfg(test)]
mod tests;

pub use auth::current_mcp_token;
pub use config::McpConfig;
pub use dialog_state::SoftDialogTracker;
pub use pane_state::PaneStateStore;

// The agent runtime (`crate::agent`) is the registry's second consumer (agent-spec D49):
// it dispatches the read-only `Consumer::Agent` view in-process. These are the exact
// surface it needs — the dispatch entry, the agent view, the consumer/access tokens, and
// the tool result types its handlers return. Deliberately narrow so the agent can't reach
// the ai-client dispatch or the auth gate.
pub(crate) use executor::{ToolError, ToolResult};
pub use server::{
    McpServerOutcome, get_mcp_actual_port, is_mcp_running, rebind_interactive, start_mcp_server_background,
    stop_mcp_server, stop_mcp_server_and_wait,
};
pub(crate) use tool_registry::{Access, Consumer, agent_tool_view, execute_tool, tool_access};
