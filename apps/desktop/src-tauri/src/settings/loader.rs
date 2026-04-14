//! Settings loading from tauri-plugin-store JSON file.
//!
//! Reads settings from the settings.json file created by tauri-plugin-store.
//! The backend reads the file directly at startup so it can configure itself
//! (MCP server, hidden files, indexing, crash reporter) before the frontend loads.
//! The frontend owns all writes via tauri-plugin-store; this module is read-only.

use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

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
/// Note: Uses serde aliases to support both camelCase (settings.json) and snake_case
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
    #[serde(alias = "updates.crashReports", default)]
    #[allow(
        dead_code,
        reason = "Read by frontend; Rust Settings struct preserves it for crash reporter"
    )]
    pub crash_reports_enabled: Option<bool>,
    #[serde(alias = "ai.provider", default)]
    #[allow(dead_code, reason = "Included in crash reports for feature correlation")]
    pub ai_provider: Option<String>,
    #[serde(alias = "developer.verboseLogging", default)]
    #[allow(dead_code, reason = "Included in crash reports for feature correlation")]
    pub verbose_logging: Option<bool>,
    #[serde(alias = "network.directSmbConnection", default)]
    pub direct_smb_connection: Option<bool>,
    #[serde(alias = "advanced.filterSafeSaveArtifacts", default)]
    pub filter_safe_save_artifacts: Option<bool>,
    #[serde(alias = "fileOperations.mtpEnabled", default)]
    pub mtp_enabled: Option<bool>,
    #[serde(alias = "advanced.diskSpaceChangeThreshold", default)]
    pub disk_space_change_threshold_mb: Option<u64>,
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
            crash_reports_enabled: None,
            ai_provider: None,
            verbose_logging: None,
            direct_smb_connection: None,
            filter_safe_save_artifacts: None,
            mtp_enabled: None,
            disk_space_change_threshold_mb: None,
        }
    }
}

/// Loads settings from the persistent store file (settings.json).
/// Returns defaults if the file doesn't exist or can't be parsed.
pub fn load_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Settings {
    // Get the app data directory (like ~/Library/Application Support/com.veszelovszki.cmdr/)
    let Some(data_dir) = crate::config::resolved_app_data_dir(app).ok() else {
        return Settings::default();
    };

    let settings_path: PathBuf = data_dir.join("settings.json");
    if let Ok(contents) = fs::read_to_string(&settings_path)
        && let Ok(settings) = parse_settings(&contents)
    {
        return settings;
    }

    Settings::default()
}

/// Parse settings.json which uses dot notation for keys (like "developer.mcpEnabled")
fn parse_settings(contents: &str) -> Result<Settings, serde_json::Error> {
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

    let crash_reports_enabled = json.get("updates.crashReports").and_then(|v| v.as_bool());
    let ai_provider = json.get("ai.provider").and_then(|v| v.as_str()).map(String::from);
    let verbose_logging = json.get("developer.verboseLogging").and_then(|v| v.as_bool());
    let direct_smb_connection = json.get("network.directSmbConnection").and_then(|v| v.as_bool());
    let filter_safe_save_artifacts = json.get("advanced.filterSafeSaveArtifacts").and_then(|v| v.as_bool());
    let mtp_enabled = json.get("fileOperations.mtpEnabled").and_then(|v| v.as_bool());
    let disk_space_change_threshold_mb = json.get("advanced.diskSpaceChangeThreshold").and_then(|v| v.as_u64());

    Ok(Settings {
        show_hidden_files,
        full_disk_access_choice,
        developer_mcp_enabled,
        developer_mcp_port,
        indexing_enabled,
        crash_reports_enabled,
        ai_provider,
        verbose_logging,
        direct_smb_connection,
        filter_safe_save_artifacts,
        mtp_enabled,
        disk_space_change_threshold_mb,
    })
}
