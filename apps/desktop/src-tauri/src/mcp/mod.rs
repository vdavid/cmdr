//! MCP (Model Context Protocol) server module.
//!
//! Provides a Streamable HTTP server that exposes cmdr functionality as MCP tools,
//! enabling AI agents to control the file manager.

mod config;
mod executor;
pub mod pane_state;
mod protocol;
mod resources;
mod server;
pub mod settings_state;
mod tools;

#[cfg(test)]
mod tests;

pub use config::McpConfig;
pub use pane_state::PaneStateStore;
pub use server::start_mcp_server;
pub use settings_state::SettingsStateStore;
