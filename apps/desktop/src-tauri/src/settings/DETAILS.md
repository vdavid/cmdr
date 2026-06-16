# Settings (Rust) details

`CLAUDE.md` holds the must-knows. This file holds the full `Settings` struct field list and the file format.

## Settings struct

Each field is `parse_settings`-extracted from a literal dot-notation key in `settings.json`. Source key noted where it
differs from the field name.

- `show_hidden_files: bool` (default true).
- `full_disk_access_choice`: consulted at launch by the indexer FDA gate.
- `developer_mcp_enabled: Option<bool>`.
- `developer_mcp_port: Option<u16>`.
- `indexing_enabled: Option<bool>`.
- `crash_reports_enabled: Option<bool>` (from `updates.crashReports`).
- `ai_provider: Option<String>` (from `ai.provider`, for crash reports).
- `verbose_logging: Option<bool>` (from `developer.verboseLogging`, for crash reports).
- `direct_smb_connection: Option<bool>` (from `network.directSmbConnection`).
- `filter_safe_save_artifacts: Option<bool>` (from `advanced.filterSafeSaveArtifacts`).
- `mtp_enabled: Option<bool>` (from `fileOperations.mtpEnabled`).
- `disk_space_change_threshold_mb: Option<u64>` (from `advanced.diskSpaceChangeThreshold`).
- `low_disk_space_notifications: Option<String>` (from `behavior.fileSystemWatching.lowDiskSpaceNotifications`;
  `low_disk_space_enabled()` maps any mode but "off" (or missing) to enabled).
- `low_disk_space_threshold_percent: Option<u64>` (from `behavior.fileSystemWatching.lowDiskSpaceThresholdPercent`,
  default 5).
- `smb_concurrency: Option<u16>` (from `network.smbConcurrency`).
- `max_log_storage_mb: Option<u64>` (from `advanced.maxLogStorageMb`).
- `error_reports_enabled: Option<bool>` (from `updates.errorReports`; Flow B opt-in, default off).
- `show_virtual_git_portal: Option<bool>` (from `fileExplorer.git.showVirtualGitPortal`).
- `network_enabled: Option<bool>` (from `network.enabled`; default on, off renders the picker as "Network (disabled)").
- `network_first_trigger_done: Option<bool>` (from `network.firstTriggerDone`; hidden internal flag, true once the macOS
  Local Network prompt has fired).
- `analytics_enabled: Option<bool>` (from `analytics.enabled`; tri-state consent: None/Some(true) → on, Some(false) →
  opted out; see `analytics/CLAUDE.md`).

`early_load_global_go_to_latest_shortcut()` is a third early-load helper returning `Option<(bool, String)>` (enabled +
shortcut string) for the downloads global shortcut, read before the `AppHandle` is wired in.

## File format

`settings.json` is flat JSON with literal dot-notation string keys, written by `tauri-plugin-store`:

```json
{ "showHiddenFiles": true, "developer.mcpEnabled": true, "developer.mcpPort": 0 }
```

These are top-level keys; the dot is part of the key name, not a nesting separator. `parse_settings` reads them manually
(serde can't express dot-notation field names as struct fields). `developer.mcpPort = 0` means "let the kernel pick an
ephemeral port"; any non-zero value pins.

## Dependencies

- External: none.
- Internal: `crate::config::resolved_app_data_dir` (app data directory with dev isolation).
