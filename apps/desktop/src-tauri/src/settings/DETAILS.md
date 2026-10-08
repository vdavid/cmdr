# Settings (Rust) details

`CLAUDE.md` holds the must-knows. This file holds per-field notes, the restricted-window snapshot, the early-load
helpers, and the file format.

## Settings struct

Each field is `parse_settings`-extracted from a literal dot-notation key in `settings.json`, as STORED: no managed
lock applies here. The struct's doc comments in `loader.rs` are the complete list (the image-index, ADB, and
drive-indexing fields are documented there only); the notes below cover the fields with non-obvious behavior. Source
key noted where it differs from the field name.

- `show_hidden_files: bool` (default off). The three backend spellings of that default (the serde attribute,
  `Settings::default` for a missing/unreadable file, and `parse_settings` for a file without the key) all read one
  `DEFAULT_SHOW_HIDDEN_FILES` const, so they can't drift. The frontend registry
  (`src/lib/settings/definitions/appearance.ts`) carries the matching default; nothing checks the two sides
  mechanically, so tests on each side stand in for a parity check.
- `full_disk_access_choice`: consulted at launch by the FDA gate, via `read_fda_choice` (registry key, falling back to
  the pre-migration top-level name, and reporting a value it can't parse).
- `developer_mcp_enabled: Option<bool>`. Absent (the common case) → `None` → `mcp/config.rs` uses its
  env → setting → debug-build-on fallback. The FE settings store persists sparsely (only keys an actor explicitly
  set), so it doesn't write the registry-default `false` here as if it were a user choice. An older data dir may
  still carry a leaked explicit `false` (we don't rewrite existing files); the dev wrapper's `CMDR_MCP_ENABLED=1` export
  (`scripts/DETAILS.md`) neutralizes that for the dev workflow. See the FE `lib/settings/DETAILS.md` § "Sparse
  persistence".
- `developer_mcp_port: Option<u16>`.
- `indexing_enabled: Option<bool>`.
- `crash_reports_enabled: Option<bool>` (from `updates.crashReports`).
- `ai_provider: Option<String>` (from `ai.provider`, for crash reports).
- `verbose_logging: Option<bool>` (from `developer.verboseLogging`, for crash reports).
- `direct_smb_connection: Option<bool>` (from `network.directSmbConnection`).
- `show_safe_save_files: Option<bool>` (from `advanced.showSafeSaveFiles`, default on).
- `show_staging_temp_files: Option<bool>` (from `advanced.showStagingTempFiles`, default off).
- `mtp_enabled: Option<bool>` (from `fileOperations.mtpEnabled`).
- `disk_space_change_threshold_mb: Option<u64>` (from `advanced.diskSpaceChangeThreshold`).
- `appearance_file_size_format: Option<String>` (from `appearance.fileSizeFormat`; the disk-space poller rounds its emit
  gate in the same base, `space_poller::FileSizeFormat::from_setting`).
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

`early_load_global_go_to_latest_shortcut()` is a third early-load helper returning `Option<(bool, String)>` (enabled +
shortcut string) for the downloads global shortcut, read before the `AppHandle` is wired in.

## Restricted-window snapshot

`load_restricted_window_settings` + `RestrictedWindowSettings` back the `get_restricted_window_settings` command (in
`commands/settings.rs`): the typed read allowlist for windows without store capability (the viewer AND the Transfers
queue). Reads `settings.json` fresh per call. A setting missing from the struct reads as its registry default in those
windows, silently — which is how the queue rendered binary sizes while the copy dialog rendered SI, and how a pinned
`appearance.language` reached every window except those two. Adding one is a four-place change; the frontend half is
`apps/desktop/src/lib/settings/CLAUDE.md`.

## Early-load helpers

Two helpers in `loader.rs` read `settings.json` before the Tauri `AppHandle` is fully wired into `setup()`, used by the
`logging::dispatch` initializer: `early_load_max_log_storage_mb()` (`Option<u64>`, cap in MB, 0 = disabled) and
`early_load_verbose_logging()` (`Option<bool>`, sets the initial stdout threshold to Debug if true and `RUST_LOG` is
unset). Both resolve the data dir via `config::standalone_app_data_dir()` (`CMDR_DATA_DIR`, else the OS default for
`config::BUNDLE_ID`, kept in sync with `tauri.conf.json` → `identifier`).

## File format

`settings.json` is flat JSON with literal dot-notation string keys, written by `tauri-plugin-store`:

```json
{ "listing.showHiddenFiles": true, "developer.mcpEnabled": true, "developer.mcpPort": 0 }
```

These are top-level keys; the dot is part of the key name, not a nesting separator. `parse_settings` reads them manually
(serde can't express dot-notation field names as struct fields). `developer.mcpPort = 0` means "let the kernel pick an
ephemeral port"; any non-zero value pins.

## Dependencies

- External: none.
- Internal: `crate::config::resolved_app_data_dir` (app data directory with dev isolation).
