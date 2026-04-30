//! Settings module — loads settings from tauri-plugin-store's JSON file at startup.

pub mod loader;

pub use loader::{FullDiskAccessChoice, early_load_max_log_storage_mb, early_load_verbose_logging, load_settings};
