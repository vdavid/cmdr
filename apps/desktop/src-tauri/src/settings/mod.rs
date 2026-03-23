//! Settings module — loads settings from tauri-plugin-store's JSON file at startup.

mod loader;

pub use loader::load_settings;
