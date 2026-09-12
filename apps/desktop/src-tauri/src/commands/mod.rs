//! Tauri commands module.

pub mod agent;
pub mod analytics;
pub mod beta_signup;
pub mod child_window_state;
pub mod clipboard;
pub mod crash_reporter;
// The macOS Dock. macOS only, mechanism and all (`../dock/`); no other platform has one to pin to.
#[cfg(target_os = "macos")]
pub mod dock;
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
#[cfg(target_os = "macos")]
pub mod memory_diagnostics;
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
// `Cmdr > Services` has no counterpart off macOS, so there's no stub either: the
// frontend's wrapper no-ops elsewhere (`tauri-commands/app-state.ts`).
#[cfg(target_os = "macos")]
pub mod services_menu;
pub mod settings;
// The protocol-agnostic server family, over the per-protocol wiring. Same gate
// as `sftp` and `webdav`, whose commands it faces.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod servers;
// SFTP works wherever the app does; the gate matches `network`, whose stores and
// wiring it reaches through. ❌ No stub counterpart: stubbing it would turn SFTP
// off on Linux, where the Docker E2E lane runs.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod sftp;
pub mod smb_diagnostics;
pub mod sync_status; // Has both macOS and non-macOS implementations
pub mod usage;
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod volumes;
// WebDAV: the same gate and the same reason as `sftp`. ❌ No stub counterpart.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub mod webdav;
pub mod whats_new;
pub mod window_ordering;
