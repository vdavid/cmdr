//! Settings module for legacy settings loading.

mod legacy;
mod port_checker;

// Re-export legacy settings for backward compatibility
pub use legacy::{FullDiskAccessChoice, Settings, load_settings};

// Port checker is available for future use but not exposed as Tauri commands yet
#[allow(
    unused_imports,
    reason = "Port checker utilities kept for future MCP server configuration"
)]
use port_checker::{check_port_available, find_available_port};
