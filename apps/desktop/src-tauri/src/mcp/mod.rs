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
mod tool_registry;
mod tools;

#[cfg(test)]
mod tests;

pub use auth::current_mcp_token;
pub use config::McpConfig;
pub use dialog_state::SoftDialogTracker;
pub use pane_state::PaneStateStore;
pub use server::{
    McpServerOutcome, get_mcp_actual_port, is_mcp_running, rebind_interactive, start_mcp_server_background,
    stop_mcp_server, stop_mcp_server_and_wait,
};
