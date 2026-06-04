# Settings (Rust)

Frontend counterpart: [`apps/desktop/src/lib/settings/CLAUDE.md`](../../../src/lib/settings/CLAUDE.md) owns the settings store (`tauri-plugin-store`), the typed `getSetting` / `setSetting` wrapper, the Settings window UI, and the `settings-applier.ts` IPC pump that satisfies the live-apply rule below.

## Live-apply rule

**Every setting MUST apply immediately without restart.** When adding a new setting that the backend reads, also add: (a) a Tauri command that updates the relevant atomic/global (live in `commands/settings.rs`, delegate to the owning subsystem's setter), and (b) a call from `settings-applier.ts` triggered by `onSettingChange`. Startup-time seeding from `load_settings` stays (it gives the backend a sane initial value before any window opens), but every subsequent change is pushed via IPC. Restart-required is a bug, not a design choice. If you're tempted to leave a setting as startup-only because it touches a TCP connection, a thread pool, a watcher, or a server: find a way. Reconnect, rebind, restart the thread, swap the worker pool, whatever it takes. **MUST.** No exceptions.

Thin read-only settings loader used during Rust startup. The frontend owns all settings via `tauri-plugin-store`; this module just reads what was persisted so the backend can configure itself at launch.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | Re-exports `load_settings` from `loader` |
| `loader.rs` | `Settings` struct + `load_settings`: reads `settings.json`, falls back to defaults |

## Settings struct

```rust
Settings {
    show_hidden_files: bool,           // default true
    full_disk_access_choice: ...,      // consulted at launch by indexer FDA gate
    developer_mcp_enabled: Option<bool>,
    developer_mcp_port: Option<u16>,
    indexing_enabled: Option<bool>,
    crash_reports_enabled: Option<bool>,  // from "updates.crashReports"
    ai_provider: Option<String>,           // from "ai.provider", for crash reports
    verbose_logging: Option<bool>,         // from "developer.verboseLogging", for crash reports
    direct_smb_connection: Option<bool>,   // from "network.directSmbConnection"
    mtp_enabled: Option<bool>,             // from "fileOperations.mtpEnabled"
    disk_space_change_threshold_mb: Option<u64>, // from "advanced.diskSpaceChangeThreshold"
    max_log_storage_mb: Option<u64>,               // from "advanced.maxLogStorageMb"
    error_reports_enabled: Option<bool>,           // from "updates.errorReports" (Flow B opt-in, default off)
    network_enabled: Option<bool>,                 // from "network.enabled" (default on; off renders picker as "Network (disabled)")
    network_first_trigger_done: Option<bool>,      // from "network.firstTriggerDone" (hidden internal flag; true if we've ever triggered the macOS Local Network prompt)
}
```

## Restricted-window snapshot

`load_restricted_window_settings` + the `RestrictedWindowSettings` struct back the `get_restricted_window_settings`
command (in `commands/settings.rs`): the typed read allowlist for windows without store capability (the viewer). Reads
`settings.json` fresh per call. Read-only like everything else here; the matching write path
(`persist_restricted_window_setting`) forwards to the main window's frontend store instead of writing from Rust, keeping
the one-way-read invariant below intact.

## Early-load helpers

Two helpers in `loader.rs` read `settings.json` *before* the Tauri `AppHandle` is fully
wired into `setup()`'s downstream calls, used by the `logging::dispatch` initializer so
the fern tree's keep-N value and stdout-threshold default both reflect persisted settings:

- `early_load_max_log_storage_mb()` → `Option<u64>` (cap in MB; 0 = disabled).
- `early_load_verbose_logging()` → `Option<bool>` (sets the initial stdout threshold to
  Debug if true and `RUST_LOG` is unset).

Both mirror the env-var precedence used by `resolved_app_data_dir` but resolve the
production default via `dirs::data_dir()` + a hard-coded bundle id constant (kept in
sync with `tauri.conf.json` → `identifier`).

## File format

**`settings.json`**: flat JSON with literal dot-notation string keys, written by `tauri-plugin-store`.

```json
{ "showHiddenFiles": true, "developer.mcpEnabled": true, "developer.mcpPort": 0 }
```

`developer.mcpPort = 0` means "let the kernel pick an ephemeral port" (the post-instance-isolation default). Any
non-zero value pins. See [`apps/desktop/src-tauri/src/mcp/CLAUDE.md`](../mcp/CLAUDE.md) § Server lifecycle and
[`docs/tooling/instance-isolation.md`](../../../../../docs/tooling/instance-isolation.md).

These are top-level keys; the dot is part of the key name, not a nesting separator. `parse_settings` reads them manually (serde can't express dot-notation field names as struct fields).

## Key decisions

**Decision**: Rust reads the settings file directly instead of receiving values via IPC from the frontend.
**Why**: Several backend systems (MCP server, hidden files filter, indexing) need their config *before* any frontend window loads. Waiting for the frontend to boot and push settings would create a race condition or require delaying backend initialization. Reading the file directly means the backend is configured immediately at launch.

**Decision**: Manual JSON field extraction in `parse_settings` instead of serde auto-derivation.
**Why**: `tauri-plugin-store` writes flat JSON with literal dot-notation string keys like `"developer.mcpEnabled"`. Serde's `rename` attribute can handle this per-field, but the dot is part of the key name, not a nesting separator. The manual extraction makes this non-obvious format explicit and avoids confusion about nested vs. flat structure.

## Key patterns

- **One-way read only.** This module never writes. All writes go through the frontend's settings store.
- Direct file reading is the correct design: multiple backend systems (MCP, indexing, crash reporter) need settings before the frontend loads.
- `full_disk_access_choice` is consulted at app launch by the indexer FDA gate (`indexing::should_auto_start_indexing`)
  to defer the recursive scan from `/` until the user has decided. See `indexing/CLAUDE.md` § "Defer indexer auto-start
  until the user decides about Full Disk Access".
- Falls back gracefully: missing file → use `Default`.

## Dependencies

External: none
Internal: `crate::config::resolved_app_data_dir` (for app data directory with dev isolation)
