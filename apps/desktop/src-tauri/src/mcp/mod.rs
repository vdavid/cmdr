//! MCP (Model Context Protocol) server module.
//!
//! Provides an HTTP+SSE server that exposes cmdr functionality as MCP tools,
//! enabling AI agents to control the file manager.

mod config;
mod executor;
pub mod pane_state;
mod protocol;
mod server;
mod tools;

#[cfg(test)]
mod tests;

pub use config::McpConfig;
pub use pane_state::PaneStateStore;
pub use server::start_mcp_server;
