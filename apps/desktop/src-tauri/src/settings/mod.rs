//! Settings module: loads settings from tauri-plugin-store's JSON file at startup.

pub mod loader;

pub use loader::{
    FullDiskAccessChoice, RestrictedWindowSettings, early_load_global_go_to_latest_shortcut,
    early_load_max_log_storage_mb, early_load_verbose_logging, load_restricted_window_settings, load_settings,
};
