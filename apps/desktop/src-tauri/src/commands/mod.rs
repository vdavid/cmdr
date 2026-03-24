//! Tauri commands module.

pub mod clipboard;
pub mod crash_reporter;
pub mod e2e;
pub mod file_system;
pub mod file_viewer;
pub mod font_metrics;
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
pub mod search;
pub mod settings;
pub mod sync_status; // Has both macOS and non-macOS implementations
pub mod ui;
mod util;
#[cfg(target_os = "macos")]
pub mod volumes;
#[cfg(target_os = "linux")]
pub mod volumes_linux;
