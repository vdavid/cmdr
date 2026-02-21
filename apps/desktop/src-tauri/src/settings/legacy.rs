//! Settings loading from tauri-plugin-store JSON file.
//!
//! Reads settings from the settings-v2.json file created by tauri-plugin-store.
//! Used to initialize app state (menu checkboxes, MCP config) on startup.

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
/// Note: Uses serde aliases to support both camelCase (settings-v2.json) and snake_case
#[derive(Debug, Deserialize)]
pub struct Settings {
    #[serde(alias = "showHiddenFiles", default = "default_show_hidden")]
    pub show_hidden_files: bool,
    #[serde(alias = "fullDiskAccessChoice", default)]
    #[allow(dead_code, reason = "Only used by frontend, backend just persists it")]
    pub full_disk_access_choice: FullDiskAccessChoice,
    #[serde(alias = "developer.mcpEnabled", default)]
    pub developer_mcp_enabled: Option<bool>,
    #[serde(alias = "developer.mcpPort", default)]
    pub developer_mcp_port: Option<u16>,
    #[serde(alias = "indexing.enabled", default)]
    pub indexing_enabled: Option<bool>,
}

fn default_show_hidden() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_hidden_files: true,
            full_disk_access_choice: FullDiskAccessChoice::NotAskedYet,
            developer_mcp_enabled: None,
            developer_mcp_port: None,
            indexing_enabled: None,
        }
    }
}

/// Loads settings from the persistent store file (settings-v2.json).
/// Returns defaults if the file doesn't exist or can't be parsed.
pub fn load_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Settings {
    // Get the app data directory (like ~/Library/Application Support/com.veszelovszki.cmdr/)
    let Some(data_dir) = app.path().app_data_dir().ok() else {
        return Settings::default();
    };

    // Try settings-v2.json first (new format from tauri-plugin-store)
    let settings_v2_path: PathBuf = data_dir.join("settings-v2.json");
    if let Ok(contents) = fs::read_to_string(&settings_v2_path)
        && let Ok(settings) = parse_settings_v2(&contents)
    {
        return settings;
    }

    // Fall back to legacy settings.json
    let settings_path: PathBuf = data_dir.join("settings.json");
    if let Ok(contents) = fs::read_to_string(&settings_path)
        && let Ok(settings) = serde_json::from_str(&contents)
    {
        return settings;
    }

    Settings::default()
}

/// Parse settings-v2.json which uses dot notation for keys (like "developer.mcpEnabled")
fn parse_settings_v2(contents: &str) -> Result<Settings, serde_json::Error> {
    // tauri-plugin-store uses flat JSON with dot notation keys
    let json: serde_json::Value = serde_json::from_str(contents)?;

    let show_hidden_files = json.get("showHiddenFiles").and_then(|v| v.as_bool()).unwrap_or(true);

    let full_disk_access_choice = json
        .get("fullDiskAccessChoice")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let developer_mcp_enabled = json.get("developer.mcpEnabled").and_then(|v| v.as_bool());

    let developer_mcp_port = json
        .get("developer.mcpPort")
        .and_then(|v| v.as_u64())
        .and_then(|v| u16::try_from(v).ok());

    let indexing_enabled = json.get("indexing.enabled").and_then(|v| v.as_bool());

    Ok(Settings {
        show_hidden_files,
        full_disk_access_choice,
        developer_mcp_enabled,
        developer_mcp_port,
        indexing_enabled,
    })
}
