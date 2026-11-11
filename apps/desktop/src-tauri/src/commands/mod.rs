//! Tauri commands module.

pub mod agent;
pub mod analytics;
pub mod beta_signup;
pub mod child_window_state;
pub mod clipboard;
pub mod confirm_dialog;
pub mod crash_reporter;
pub mod data_wipe;
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
pub mod menu_state;
pub mod mtp;
pub mod multi_rename;
pub mod network;
pub mod nfs;
pub mod operation_log;
pub mod quick_look;
pub mod rename;
pub mod restricted_paths;
pub mod search;
pub mod security_log;
pub mod selection;
pub mod state_recovery;
// `Cmdr > Services` has no counterpart off macOS, so there's no stub either: the
// frontend's wrapper no-ops elsewhere (`tauri-commands/app-state.ts`).
pub mod s3;
#[cfg(target_os = "macos")]
pub mod services_menu;
pub mod settings;
pub mod share_link;
// The protocol-agnostic server family, over the per-protocol wiring.
pub mod servers;
// The IPC side of a mutation's late end: the engine hands it to a callback, this broadcasts it.
pub(crate) mod settled;
pub mod sftp;
pub mod smb_diagnostics;
pub mod sync_status; // Has both macOS and non-macOS implementations
pub mod usage;
pub mod volume_sign_in; // Backend-neutral sign-in commands
pub mod volumes;
pub mod webdav;
pub mod whats_new;
pub mod window_ordering;

#[cfg(test)]
mod wire_shape_tests;
