//! User settings persistence.
//!
//! Reads settings from the tauri-plugin-store JSON file.
//! Used to initialize the menu with the correct checked state on startup.

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

/// User's choice regarding full disk access permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum FullDiskAccessChoice {
    /// User clicked "Open System Settings" (presumably granted)
    Allow,
    /// User clicked "Deny" - don't ask again
    Deny,
    /// First launch, haven't shown prompt yet
    #[default]
    NotAskedYet,
}

/// User settings structure, matching the frontend settings-store.ts
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub show_hidden_files: bool,
    #[serde(default)]
    #[allow(dead_code, reason = "Only used by frontend, backend just persists it")]
    pub full_disk_access_choice: FullDiskAccessChoice,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_hidden_files: true,
            full_disk_access_choice: FullDiskAccessChoice::NotAskedYet,
        }
    }
}

/// Loads settings from the persistent store file.
/// Returns defaults if the file doesn't exist or can't be parsed.
pub fn load_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Settings {
    // Get the app data directory (e.g., ~/Library/Application Support/com.veszelovszki.cmdr/)
    let Some(data_dir) = app.path().app_data_dir().ok() else {
        return Settings::default();
    };

    let settings_path: PathBuf = data_dir.join("settings.json");

    // Try to read and parse the settings file
    let Ok(contents) = fs::read_to_string(&settings_path) else {
        return Settings::default();
    };

    serde_json::from_str(&contents).unwrap_or_default()
}
