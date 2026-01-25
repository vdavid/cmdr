//! Settings module for Tauri commands and port checking.

mod legacy;
mod port_checker;

// Re-export legacy settings for backward compatibility
pub use legacy::{load_settings, FullDiskAccessChoice, Settings};

// Re-export port checker commands
pub use port_checker::{check_port_available, find_available_port};
