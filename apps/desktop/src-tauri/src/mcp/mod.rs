//! MCP (Model Context Protocol) server module.
//!
//! Provides a Streamable HTTP server that carries `crate::tools` to outside AI clients: binding,
//! auth, and what the official SDK (`rmcp`) calls. The tools themselves live in `crate::tools`.

mod auth;
pub mod config;
mod handler;
pub mod port_file;
mod safe_headers;
mod server;

#[cfg(test)]
mod tests;

pub use auth::current_mcp_token;
pub use config::McpConfig;
pub use server::{
    McpServerOutcome, get_mcp_actual_port, is_mcp_running, rebind_interactive, start_mcp_server_background,
    stop_mcp_server, stop_mcp_server_and_wait,
};
