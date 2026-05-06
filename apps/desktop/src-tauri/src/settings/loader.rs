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
    #[serde(alias = "network.smbConcurrency", default)]
    pub smb_concurrency: Option<u16>,
    #[serde(alias = "advanced.maxLogStorageMb", default)]
    #[allow(
        dead_code,
        reason = "Read via early-load helper before plugin init; kept here for completeness"
    )]
    pub max_log_storage_mb: Option<u64>,
    #[serde(alias = "updates.errorReports", default)]
    pub error_reports_enabled: Option<bool>,
    #[serde(alias = "fileExplorer.git.showVirtualGitPortal", default)]
    pub show_virtual_git_portal: Option<bool>,
    #[serde(alias = "network.enabled", default)]
    pub network_enabled: Option<bool>,
    #[serde(alias = "network.firstTriggerDone", default)]
    pub network_first_trigger_done: Option<bool>,
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
            smb_concurrency: None,
            max_log_storage_mb: None,
            error_reports_enabled: None,
            show_virtual_git_portal: None,
            network_enabled: None,
            network_first_trigger_done: None,
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
    let smb_concurrency = json
        .get("network.smbConcurrency")
        .and_then(|v| v.as_u64())
        .and_then(|v| u16::try_from(v).ok());
    let max_log_storage_mb = json.get("advanced.maxLogStorageMb").and_then(|v| v.as_u64());
    let error_reports_enabled = json.get("updates.errorReports").and_then(|v| v.as_bool());
    let show_virtual_git_portal = json
        .get("fileExplorer.git.showVirtualGitPortal")
        .and_then(|v| v.as_bool());
    let network_enabled = json.get("network.enabled").and_then(|v| v.as_bool());
    let network_first_trigger_done = json.get("network.firstTriggerDone").and_then(|v| v.as_bool());

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
        smb_concurrency,
        max_log_storage_mb,
        error_reports_enabled,
        show_virtual_git_portal,
        network_enabled,
        network_first_trigger_done,
    })
}

/// Reads `advanced.maxLogStorageMb` from disk *before* the Tauri app handle is wired
/// into the rest of `setup()`.
///
/// The fern dispatch tree initializes inside `setup()` but before [`load_settings`] runs
/// (which itself depends on the resolved app data dir helper). We resolve the data dir
/// from `CMDR_DATA_DIR` (set by `tauri-wrapper.js` in dev / by E2E harnesses) and
/// otherwise fall back to the OS-default app-support dir for the `com.veszelovszki.cmdr`
/// bundle.
///
/// Returns `None` when the file is missing or the key is unset; the caller substitutes the
/// 200 MB default. Returns `Some(0)` for explicit "log storage disabled".
pub fn early_load_max_log_storage_mb() -> Option<u64> {
    /// Bundle id from `tauri.conf.json`. Mirrored here so this function works without the
    /// app handle. Keep in sync if the bundle id ever changes.
    const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

    let data_dir: PathBuf = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        let base = dirs::data_dir()?;
        base.join(BUNDLE_ID)
    };

    let settings_path = data_dir.join("settings.json");
    let contents = fs::read_to_string(&settings_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("advanced.maxLogStorageMb").and_then(|v| v.as_u64())
}

/// Reads `developer.verboseLogging` from disk *before* the Tauri app handle exists.
///
/// Mirrors [`early_load_max_log_storage_mb`]. Used by the logging dispatch builder so
/// the stdout threshold can start at Debug if the user persisted the verbose toggle.
/// Returns `None` when the file or key is missing.
pub fn early_load_verbose_logging() -> Option<bool> {
    /// Bundle id from `tauri.conf.json`. Mirrored here so this function works without
    /// the app handle. Keep in sync if the bundle id ever changes.
    const BUNDLE_ID: &str = "com.veszelovszki.cmdr";

    let data_dir: PathBuf = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        let base = dirs::data_dir()?;
        base.join(BUNDLE_ID)
    };

    let settings_path = data_dir.join("settings.json");
    let contents = fs::read_to_string(&settings_path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("developer.verboseLogging").and_then(|v| v.as_bool())
}
