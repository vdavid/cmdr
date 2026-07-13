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
    /// Drive-indexing freshness UX. All three gate FRONTEND behavior (the
    /// first-connect notification and the one-time stale dialog), so the FE owns
    /// reads/writes via the settings registry; they're parsed here for
    /// completeness and crash-report correlation, mirroring
    /// `network.firstTriggerDone`. A missing key means the registry default
    /// (`ask_for_each_drive` ON, `stale_notify` ON).
    ///
    /// Gates the per-drive first-connect "Enable indexing?" notification (D6).
    #[serde(alias = "indexing.askForEachDrive", default)]
    #[allow(dead_code, reason = "FE-gating setting; parsed for completeness/crash correlation")]
    pub indexing_ask_for_each_drive: Option<bool>,
    /// Gates the one-time "a drive went stale" dialog (D2). The yellow stale
    /// badge shows regardless of this toggle.
    #[serde(alias = "indexing.staleNotify", default)]
    #[allow(dead_code, reason = "FE-gating setting; parsed for completeness/crash correlation")]
    pub indexing_stale_notify: Option<bool>,
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
    #[serde(alias = "behavior.fileSystemWatching.lowDiskSpaceNotifications", default)]
    pub low_disk_space_notifications: Option<String>,
    #[serde(alias = "behavior.fileSystemWatching.lowDiskSpaceThresholdPercent", default)]
    pub low_disk_space_threshold_percent: Option<u64>,
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
    /// The analytics opt-out (tri-state). `None`/`Some(true)` → analytics on, `Some(false)` →
    /// opted out. The frontend store only persists non-default values, so an opted-in install has
    /// no key. See `analytics_consent_granted` and `analytics/CLAUDE.md` § "Consent is tri-state".
    #[serde(alias = "analytics.enabled", default)]
    pub analytics_enabled: Option<bool>,
    /// The master "Index image contents" toggle for the media-ML enrichment
    /// subsystem (`media_index`). Off by default and sparse-persisted, so an absent
    /// key means off. Seeded into `media_index::gate` at startup; live changes flow
    /// through `set_image_index_enabled`.
    #[serde(alias = "mediaIndex.enabled", default)]
    pub image_index_enabled: Option<bool>,
    /// Volume ids the user opted into background network (SMB) image enrichment
    /// (`media_index` network enrichment). Off by default per volume: enabling the master toggle
    /// does NOT auto-enrich network volumes. Seeded into
    /// `media_index::network::config` at startup; live changes flow through
    /// `set_media_index_network_volume_enabled`.
    #[serde(alias = "mediaIndex.networkVolumes", default)]
    pub media_index_network_volumes: Vec<String>,
    /// Volume ids marked "always index": enrich regardless of the importance
    /// threshold (a rarely-browsed NAS scores low on navigation-based importance, so
    /// its photos would otherwise defer forever). Seeded + live-applied like the
    /// opt-in.
    #[serde(alias = "mediaIndex.alwaysIndexVolumes", default)]
    pub media_index_always_index_volumes: Vec<String>,
    /// Absolute folder paths (OS-mount form) marked "always index": every image at or
    /// under one enriches regardless of importance. Seeded + live-applied like the
    /// opt-in.
    #[serde(alias = "mediaIndex.alwaysIndexFolders", default)]
    pub media_index_always_index_folders: Vec<String>,
    /// The lowest folder-importance level (`0.0..=1.0`) to image-index — the importance
    /// settings slider. Absent means the default (enrich every scored folder). Seeded
    /// into `media_index::gate` at startup; live changes flow through
    /// `set_media_index_importance_threshold`.
    #[serde(alias = "mediaIndex.importanceThreshold", default)]
    pub media_index_importance_threshold: Option<f64>,
    /// Absolute folder paths the user EXCLUDED from photo-search indexing (the privacy
    /// complement to the opt-in). Seeded + live-applied like the opt-in.
    #[serde(alias = "mediaIndex.excludedFolders", default)]
    pub media_index_excluded_folders: Vec<String>,
}

fn default_show_hidden() -> bool {
    true
}

impl Settings {
    /// Whether the low-disk-space warning is on: any mode except `"off"`.
    /// A missing key means the registry default (`"in-app"`), so enabled.
    pub fn low_disk_space_enabled(&self) -> bool {
        self.low_disk_space_notifications.as_deref() != Some("off")
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_hidden_files: true,
            full_disk_access_choice: FullDiskAccessChoice::NotAskedYet,
            developer_mcp_enabled: None,
            developer_mcp_port: None,
            indexing_enabled: None,
            indexing_ask_for_each_drive: None,
            indexing_stale_notify: None,
            crash_reports_enabled: None,
            ai_provider: None,
            verbose_logging: None,
            direct_smb_connection: None,
            filter_safe_save_artifacts: None,
            mtp_enabled: None,
            disk_space_change_threshold_mb: None,
            low_disk_space_notifications: None,
            low_disk_space_threshold_percent: None,
            smb_concurrency: None,
            max_log_storage_mb: None,
            error_reports_enabled: None,
            show_virtual_git_portal: None,
            network_enabled: None,
            network_first_trigger_done: None,
            analytics_enabled: None,
            image_index_enabled: None,
            media_index_network_volumes: Vec::new(),
            media_index_always_index_volumes: Vec::new(),
            media_index_always_index_folders: Vec::new(),
            media_index_importance_threshold: None,
            media_index_excluded_folders: Vec::new(),
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
    let indexing_ask_for_each_drive = json.get("indexing.askForEachDrive").and_then(|v| v.as_bool());
    let indexing_stale_notify = json.get("indexing.staleNotify").and_then(|v| v.as_bool());

    let crash_reports_enabled = json.get("updates.crashReports").and_then(|v| v.as_bool());
    let ai_provider = json.get("ai.provider").and_then(|v| v.as_str()).map(String::from);
    let verbose_logging = json.get("developer.verboseLogging").and_then(|v| v.as_bool());
    let direct_smb_connection = json.get("network.directSmbConnection").and_then(|v| v.as_bool());
    let filter_safe_save_artifacts = json.get("advanced.filterSafeSaveArtifacts").and_then(|v| v.as_bool());
    let mtp_enabled = json.get("fileOperations.mtpEnabled").and_then(|v| v.as_bool());
    let disk_space_change_threshold_mb = json.get("advanced.diskSpaceChangeThreshold").and_then(|v| v.as_u64());
    let low_disk_space_notifications = json
        .get("behavior.fileSystemWatching.lowDiskSpaceNotifications")
        .and_then(|v| v.as_str())
        .map(String::from);
    let low_disk_space_threshold_percent = json
        .get("behavior.fileSystemWatching.lowDiskSpaceThresholdPercent")
        .and_then(|v| v.as_u64());
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
    let analytics_enabled = json.get("analytics.enabled").and_then(|v| v.as_bool());
    let image_index_enabled = json.get("mediaIndex.enabled").and_then(|v| v.as_bool());
    let media_index_network_volumes = parse_string_array(&json, "mediaIndex.networkVolumes");
    let media_index_always_index_volumes = parse_string_array(&json, "mediaIndex.alwaysIndexVolumes");
    let media_index_always_index_folders = parse_string_array(&json, "mediaIndex.alwaysIndexFolders");
    let media_index_importance_threshold = json.get("mediaIndex.importanceThreshold").and_then(|v| v.as_f64());
    let media_index_excluded_folders = parse_string_array(&json, "mediaIndex.excludedFolders");

    Ok(Settings {
        show_hidden_files,
        full_disk_access_choice,
        developer_mcp_enabled,
        developer_mcp_port,
        indexing_enabled,
        indexing_ask_for_each_drive,
        indexing_stale_notify,
        crash_reports_enabled,
        ai_provider,
        verbose_logging,
        direct_smb_connection,
        filter_safe_save_artifacts,
        mtp_enabled,
        disk_space_change_threshold_mb,
        low_disk_space_notifications,
        low_disk_space_threshold_percent,
        smb_concurrency,
        max_log_storage_mb,
        error_reports_enabled,
        show_virtual_git_portal,
        network_enabled,
        network_first_trigger_done,
        analytics_enabled,
        image_index_enabled,
        media_index_network_volumes,
        media_index_always_index_volumes,
        media_index_always_index_folders,
        media_index_importance_threshold,
        media_index_excluded_folders,
    })
}

/// Parse a JSON array of strings at `key` into a `Vec<String>` (non-string elements
/// dropped). A missing key or a non-array value yields an empty vec, so an absent
/// setting reads as "none opted in / no overrides" (the sparse-store default).
fn parse_string_array(json: &serde_json::Value, key: &str) -> Vec<String> {
    json.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

/// The settings a restricted-capability window (the viewer) reads at startup via
/// `get_restricted_window_settings`. The viewer has no `tauri-plugin-store`
/// capability by security design (see `capabilities/CLAUDE.md` § viewer), so it
/// can't load `settings.json` itself; this typed allowlist is its read surface.
/// Field names spell out the full setting id so the FE mapping is mechanical.
///
/// Every field is `Option`: `None` means "not persisted" and the frontend falls
/// back to the registry default, exactly like the store-backed path.
#[derive(Debug, Clone, Default, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestrictedWindowSettings {
    pub viewer_word_wrap: Option<bool>,
    pub file_viewer_suppress_binary_warning: Option<bool>,
    pub appearance_text_size: Option<f64>,
    pub appearance_app_color: Option<String>,
}

/// Reads the [`RestrictedWindowSettings`] allowlist from `settings.json`.
///
/// Reads the file fresh on every call (a viewer can open at any point in the
/// session). The on-disk value lags the main window's in-memory cache by the
/// store's 500 ms save debounce; live updates after open flow through the
/// cross-window `settings:changed` event instead, so the brief staleness only
/// affects the initial paint.
pub fn load_restricted_window_settings<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> RestrictedWindowSettings {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return RestrictedWindowSettings::default();
    };
    let settings_path = data_dir.join("settings.json");
    let Ok(contents) = fs::read_to_string(&settings_path) else {
        return RestrictedWindowSettings::default();
    };
    parse_restricted_window_settings(&contents)
}

/// Pure parse step for [`load_restricted_window_settings`], split out for unit tests.
fn parse_restricted_window_settings(contents: &str) -> RestrictedWindowSettings {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(contents) else {
        return RestrictedWindowSettings::default();
    };
    RestrictedWindowSettings {
        viewer_word_wrap: json.get("viewer.wordWrap").and_then(|v| v.as_bool()),
        file_viewer_suppress_binary_warning: json.get("fileViewer.suppressBinaryWarning").and_then(|v| v.as_bool()),
        appearance_text_size: json.get("appearance.textSize").and_then(|v| v.as_f64()),
        appearance_app_color: json
            .get("appearance.appColor")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
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

/// The Ask Cmdr interactive-slot model override, read fresh from `settings.json` each
/// send (so a settings change takes effect on the next message, no restart).
///
/// Empty/absent ⇒ use the model the shared `ai/` provider is already configured with (the
/// v1 default, zero extra config). A non-empty value is a dedicated model id for Ask Cmdr,
/// layered OVER the shared `ai/` provider config (agent-spec D43: two slots, interactive +
/// a later bulk slot). The bulk slot slots in beside this as its own additive key
/// (`askCmdr.bulkModel`), no migration. Only the model is slot-specific; provider on/off,
/// keys, and base URLs stay single-sourced in the `ai/` config (D49: extend, don't fork).
pub fn load_ask_cmdr_interactive_model<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<String> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let contents = fs::read_to_string(data_dir.join("settings.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&contents).ok()?;
    json.get("askCmdr.interactiveModel")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
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

/// Reads the `behavior.fileSystemWatching.globalGoToLatestShortcut.{enabled,binding}`
/// pair from disk before the AppHandle is wired up. Used by the focus-event
/// global-shortcut refresh hook in `downloads::runtime`. Returns `None` when
/// the settings file is missing OR the user hasn't customized either key (in
/// which case the registry defaults apply — `enabled = true`,
/// `binding = "⌃⌥⌘J"`).
pub fn early_load_global_go_to_latest_shortcut() -> Option<(bool, String)> {
    /// Bundle id from `tauri.conf.json`. Keep in sync if it ever changes.
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

    // The FE settings-store only persists non-default values, so a missing
    // key means "use the documented default." We return `None` only when the
    // settings file itself is missing; partial reads (one key set, one not)
    // fall back to defaults per-field.
    let enabled = json
        .get("behavior.fileSystemWatching.globalGoToLatestShortcut.enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let binding = json
        .get("behavior.fileSystemWatching.globalGoToLatestShortcut.binding")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| String::from("\u{2303}\u{2325}\u{2318}J"));
    Some((enabled, binding))
}

/// The operation log's default size budget: 3 GB (D10). Applied when
/// `operationLog.maxSize` isn't persisted.
pub const DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES: u64 = 3 * 1024 * 1024 * 1024;

/// The operation log's retention limits, read fresh from `settings.json` each call
/// so a retention settings change takes effect on the next retention tick. Defaults
/// (D10): age = forever (`None`), size = 3 GB.
#[derive(Debug, Clone, Copy)]
pub struct OperationLogRetentionLimits {
    /// Prune ops older than this many seconds. `None` = keep forever (the default,
    /// and the "Forever" sentinel the age setting stores as `0`).
    pub max_age_secs: Option<u64>,
    /// Prune oldest ops until the DB fits this many bytes. `None` only if the user
    /// picks an explicit `0` (unlimited); absent ⇒ the 3 GB default.
    pub max_size_bytes: Option<u64>,
}

impl Default for OperationLogRetentionLimits {
    fn default() -> Self {
        Self {
            max_age_secs: None,
            max_size_bytes: Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES),
        }
    }
}

/// Reads the operation-log retention limits from `settings.json`.
///
/// Contract with the retention settings UI: the age limit persists under
/// `operationLog.maxAge` as a duration in **milliseconds** (`0` = the "Forever"
/// sentinel ⇒ no age prune), and the size limit under `operationLog.maxSize` as a
/// byte count (absent ⇒ 3 GB default; `0` ⇒ unlimited). Retention reads these so
/// it works before the UI lands; the settings UI must persist these exact keys.
pub fn load_operation_log_retention_limits<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> OperationLogRetentionLimits {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return OperationLogRetentionLimits::default();
    };
    let settings_path = data_dir.join("settings.json");
    let Ok(contents) = fs::read_to_string(&settings_path) else {
        return OperationLogRetentionLimits::default();
    };
    parse_operation_log_retention_limits(&contents)
}

/// Pure parse step for [`load_operation_log_retention_limits`], split out for unit
/// tests.
fn parse_operation_log_retention_limits(contents: &str) -> OperationLogRetentionLimits {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(contents) else {
        return OperationLogRetentionLimits::default();
    };
    // Age: milliseconds; 0 (the "Forever" sentinel) or absent ⇒ no age prune.
    let max_age_secs = json
        .get("operationLog.maxAge")
        .and_then(|v| v.as_u64())
        .filter(|ms| *ms > 0)
        .map(|ms| ms / 1000);
    // Size: bytes; absent ⇒ 3 GB default; 0 ⇒ unlimited (no size prune).
    let max_size_bytes = match json.get("operationLog.maxSize").and_then(|v| v.as_u64()) {
        None => Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES),
        Some(0) => None,
        Some(bytes) => Some(bytes),
    };
    OperationLogRetentionLimits {
        max_age_secs,
        max_size_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restricted_window_settings_parse_set_values() {
        let json = r#"{
            "viewer.wordWrap": true,
            "fileViewer.suppressBinaryWarning": true,
            "appearance.textSize": 125,
            "appearance.appColor": "blue",
            "developer.mcpEnabled": true
        }"#;
        let parsed = parse_restricted_window_settings(json);
        assert_eq!(parsed.viewer_word_wrap, Some(true));
        assert_eq!(parsed.file_viewer_suppress_binary_warning, Some(true));
        assert_eq!(parsed.appearance_text_size, Some(125.0));
        assert_eq!(parsed.appearance_app_color.as_deref(), Some("blue"));
    }

    #[test]
    fn operation_log_retention_defaults_forever_and_3gb() {
        // Absent keys ⇒ forever age, 3 GB size.
        let limits = parse_operation_log_retention_limits("{}");
        assert_eq!(limits.max_age_secs, None);
        assert_eq!(limits.max_size_bytes, Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES));
        // Bad JSON ⇒ same defaults.
        let bad = parse_operation_log_retention_limits("not json");
        assert_eq!(bad.max_age_secs, None);
        assert_eq!(bad.max_size_bytes, Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES));
    }

    #[test]
    fn operation_log_retention_reads_persisted_values() {
        // Age in ms → seconds; size in bytes verbatim.
        let json = r#"{ "operationLog.maxAge": 90000, "operationLog.maxSize": 104857600 }"#;
        let limits = parse_operation_log_retention_limits(json);
        assert_eq!(limits.max_age_secs, Some(90), "90000 ms ⇒ 90 s");
        assert_eq!(limits.max_size_bytes, Some(104_857_600));
    }

    #[test]
    fn operation_log_retention_matches_frontend_registry_values() {
        // Round-trip guard: the exact values the settings registry
        // (`settings-registry.ts`) persists must produce the intended limits, so a
        // drift on either side (a changed preset ms/byte constant, or a renamed
        // key) fails here rather than silently mis-pruning. The `operationLog.maxAge`
        // "Forever" default (0) and the 3 GB `operationLog.maxSize` default are the
        // registry `default` values; the 30-day age preset and 1 GB size preset are
        // registry option values.
        let defaults =
            parse_operation_log_retention_limits(r#"{ "operationLog.maxAge": 0, "operationLog.maxSize": 3221225472 }"#);
        assert_eq!(
            defaults.max_age_secs, None,
            "the age default (0) is the Forever sentinel"
        );
        assert_eq!(
            defaults.max_size_bytes,
            Some(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES),
            "the 3 GB size default must equal the backend's byte constant"
        );
        assert_eq!(DEFAULT_OPERATION_LOG_MAX_SIZE_BYTES, 3_221_225_472);

        let presets = parse_operation_log_retention_limits(
            r#"{ "operationLog.maxAge": 2592000000, "operationLog.maxSize": 1073741824 }"#,
        );
        assert_eq!(presets.max_age_secs, Some(2_592_000), "30 days in ms ⇒ seconds");
        assert_eq!(presets.max_size_bytes, Some(1_073_741_824), "1 GB preset in bytes");
    }

    #[test]
    fn operation_log_retention_zero_sentinels_mean_unlimited() {
        // Age 0 = the "Forever" sentinel; size 0 = unlimited.
        let json = r#"{ "operationLog.maxAge": 0, "operationLog.maxSize": 0 }"#;
        let limits = parse_operation_log_retention_limits(json);
        assert_eq!(limits.max_age_secs, None);
        assert_eq!(limits.max_size_bytes, None);
    }

    #[test]
    fn restricted_window_settings_missing_keys_are_none() {
        let parsed = parse_restricted_window_settings("{}");
        assert_eq!(parsed.viewer_word_wrap, None);
        assert_eq!(parsed.file_viewer_suppress_binary_warning, None);
        assert_eq!(parsed.appearance_text_size, None);
        assert_eq!(parsed.appearance_app_color, None);
    }

    #[test]
    fn restricted_window_settings_bad_json_yields_defaults() {
        let parsed = parse_restricted_window_settings("not json at all");
        assert_eq!(parsed.viewer_word_wrap, None);
        assert_eq!(parsed.appearance_app_color, None);
    }

    #[test]
    fn restricted_window_settings_wrong_types_are_none() {
        let json = r#"{ "viewer.wordWrap": "yes", "appearance.textSize": "big" }"#;
        let parsed = parse_restricted_window_settings(json);
        assert_eq!(parsed.viewer_word_wrap, None);
        assert_eq!(parsed.appearance_text_size, None);
    }

    #[test]
    fn parses_drive_indexing_freshness_keys() {
        // The drive-indexing freshness toggles round-trip from their literal
        // dot-notation keys. A missing key stays `None` (the FE applies the
        // registry default: both ON).
        let json = r#"{ "indexing.askForEachDrive": false, "indexing.staleNotify": true }"#;
        let parsed = parse_settings(json).expect("valid settings json");
        assert_eq!(parsed.indexing_ask_for_each_drive, Some(false));
        assert_eq!(parsed.indexing_stale_notify, Some(true));

        let empty = parse_settings("{}").expect("empty settings json");
        assert_eq!(
            empty.indexing_ask_for_each_drive, None,
            "missing key → None (FE default)"
        );
        assert_eq!(empty.indexing_stale_notify, None, "missing key → None (FE default)");
    }
}
