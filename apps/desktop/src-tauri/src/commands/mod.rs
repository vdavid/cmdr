//! Tauri commands module.

pub mod agent;
pub mod analytics;
pub mod beta_signup;
pub mod child_window_state;
pub mod clipboard;
pub mod crash_reporter;
pub mod e2e;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod eject;
pub mod error_reporter;
pub mod favorites;
pub mod feedback;
pub mod file_actions;
pub mod file_system;
pub mod file_viewer;
pub mod font_metrics;
pub mod go_to_path;
pub mod icons;
pub mod importance;
pub mod indexing;
pub mod licensing;
pub mod logging;
pub mod mcp;
pub mod media_index;
pub mod menu;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod mtp;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod network;
pub mod operation_log;
pub mod quick_look;
pub mod rename;
pub mod restricted_paths;
pub mod search;
pub mod selection;
pub mod settings;
pub mod smb_diagnostics;
pub mod sync_status; // Has both macOS and non-macOS implementations
mod util;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod volumes;
/// The Linux name for the very same module: the volume commands are
/// cross-platform, and `ipc.rs` / `ipc_collectors.rs` register the Linux set
/// under `commands::volumes_linux`. An alias rather than a file, so there's no
/// second module for a reader to mistake for a Linux implementation, and nothing
/// to drift. Retire it when those registrations move to `volumes`.
#[cfg(target_os = "linux")]
pub use volumes as volumes_linux;
pub mod whats_new;
pub mod window_ordering;
