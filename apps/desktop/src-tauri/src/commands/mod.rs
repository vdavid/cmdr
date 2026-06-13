//! Tauri commands module.

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
pub mod file_system;
pub mod file_viewer;
pub mod font_metrics;
pub mod go_to_path;
pub mod icons;
pub mod indexing;
pub mod licensing;
pub mod logging;
pub mod mcp;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod mtp;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod network;
pub mod rename;
pub mod restricted_paths;
pub mod search;
pub mod selection;
pub mod settings;
pub mod smb_diagnostics;
pub mod sync_status; // Has both macOS and non-macOS implementations
pub mod ui;
mod util;
#[cfg(target_os = "macos")]
pub mod volumes;
#[cfg(target_os = "linux")]
pub mod volumes_linux;
pub mod whats_new;
