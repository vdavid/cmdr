# Settings (Rust)

Thin read-only settings loader used during Rust startup. The frontend owns all settings via `tauri-plugin-store`; this
module reads what was persisted so the backend can configure itself at launch.

Frontend counterpart: `apps/desktop/src/lib/settings/CLAUDE.md` owns the settings
store, the typed `getSetting` / `setSetting` wrapper, the Settings window UI, and the `settings-applier.ts` IPC pump that
satisfies the live-apply rule below.

## Module map

- `mod.rs`: re-exports `load_settings` from `loader`.
- `loader.rs`: `Settings` struct + `load_settings` (reads `settings.json`, falls back to `Default`); the `RestrictedWindowSettings`
  snapshot; the early-load helpers.

## Must-knows

- **Live-apply rule (MUST, no exceptions).** Every setting applies immediately without restart. When adding a setting the
  backend reads, also add (a) a Tauri command in `commands/settings.rs` that updates the relevant atomic/global
  (delegating to the owning subsystem's setter), and (b) a call from `settings-applier.ts` on `onSettingChange`.
  Startup-time seeding from `load_settings` stays (a sane initial value before any window opens), but every subsequent
  change is pushed via IPC. Restart-required is a bug. If a setting touches a TCP connection, thread pool, watcher, or
  server: reconnect / rebind / restart the thread / swap the pool, whatever it takes.
- **One-way read only.** This module never writes; all writes go through the frontend's settings store. The restricted
  window's write path (`persist_restricted_window_setting`) forwards to the main window's store rather than writing from
  Rust, keeping this invariant intact.
- **Dot-notation keys are literal, parsed manually.** `tauri-plugin-store` writes flat JSON with literal dot-notation
  string keys (`{ "showHiddenFiles": true, "developer.mcpEnabled": true }`): the dot is part of the key name, not a
  nesting separator. `parse_settings` reads them manually because serde can't express dot-notation field names as struct
  fields. Don't switch to serde auto-derivation.
- **Direct file reading is intentional.** Multiple backend systems (MCP server, hidden-files filter, indexing, crash
  reporter) need their config before any frontend window loads. Reading the file directly avoids a boot race.
- **`full_disk_access_choice` gates indexer auto-start.** Consulted at launch by the indexer FDA gate
  (`indexing::should_auto_start_indexing`) to defer the recursive `/` scan until the user decides about Full Disk Access.
  See `indexing/CLAUDE.md`.
- **`developer.mcpPort = 0` means "kernel picks an ephemeral port"** (the post-instance-isolation default); non-zero
  pins. See `mcp/DETAILS.md` § Server lifecycle and `docs/tooling/instance-isolation.md`.

## Restricted-window snapshot

`load_restricted_window_settings` + `RestrictedWindowSettings` back the `get_restricted_window_settings` command (in
`commands/settings.rs`): the typed read allowlist for windows without store capability (the viewer). Reads
`settings.json` fresh per call.

## Early-load helpers

Two helpers in `loader.rs` read `settings.json` before the Tauri `AppHandle` is fully wired into `setup()`, used by the
`logging::dispatch` initializer: `early_load_max_log_storage_mb()` (`Option<u64>`, cap in MB, 0 = disabled) and
`early_load_verbose_logging()` (`Option<bool>`, sets the initial stdout threshold to Debug if true and `RUST_LOG` is
unset). Both resolve the production default via `dirs::data_dir()` + a hard-coded bundle-id constant kept in sync with
`tauri.conf.json` → `identifier`.

The full `Settings` struct field list (every key, its source dot-path, and per-field notes) is in `DETAILS.md`.
