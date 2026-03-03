# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates
immediately to business-logic modules. No significant logic lives here.

## File map

| File | Domain | Notes |
|------|--------|-------|
| `mod.rs` | Re-exports | `mtp`, `network` gated behind `#[cfg(any(target_os = "macos", target_os = "linux"))]`; `volumes` behind `#[cfg(target_os = "macos")]`; `volumes_linux` behind `#[cfg(target_os = "linux")]` |
| `util.rs` | Shared helpers | `blocking_with_timeout` — runs a blocking closure on the blocking thread pool with a timeout, returning a fallback value on timeout. Used by all filesystem-touching commands. |
| `file_system.rs` | File listing & writes | Largest file. Streaming + virtual-scroll listing API, write ops (copy, move, delete, trash), scan preview, conflict resolution, volume copy, native drag, self-drag overlay. Contains `expand_tilde()`. |
| `volumes.rs` | Volume management (macOS) | `list_volumes`, `get_default_volume_id`, `find_containing_volume`, `get_volume_space` |
| `volumes_linux.rs` | Volume management (Linux) | Same interface as `volumes.rs`, delegates to `volumes_linux` module |
| `mtp.rs` | MTP devices | Full MTP command surface (connect, disconnect, list, download, upload, delete, rename, move, scan) |
| `network.rs` | SMB/network shares | Discovery, share listing, keychain, mounting. |
| `font_metrics.rs` | Font metrics cache | `store_font_metrics`, `has_font_metrics` |
| `icons.rs` | File icons | `get_icons`, `refresh_directory_icons`, cache clear |
| `rename.rs` | Rename / trash | `move_to_trash` (delegates to `write_operations::trash::move_to_trash_sync`), `check_rename_permission`, `check_rename_validity`, `rename_file` |
| `file_viewer.rs` | File viewer | Session lifecycle, line search, word wrap, menu state |
| `ui.rs` | UI / menu | Context menu, Finder reveal, clipboard, Quick Look, Get Info, view mode, `set_menu_context` (enables/disables file-scoped menu items based on window focus) |
| `settings.rs` | Settings | Port availability check, watcher debounce setting, menu accelerator updates |
| `licensing.rs` | Licensing | Status query, activation, expiry, reminder, key validation |
| `indexing.rs` | Drive index | `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats`, `get_dir_stats_batch`, `prioritize_dir`, `cancel_nav_priority`, `clear_drive_index`, `set_indexing_enabled`. Uses `State<IndexManagerState>`. |
| `sync_status.rs` | Cloud sync status | `get_sync_status` — macOS delegates to `file_system::sync_status`; non-macOS returns empty map via `#[cfg]` on the function itself (not the module). |

## Key decisions

**Decision**: One commands file per domain, with no business logic in commands.
**Why**: Tauri command functions are the IPC boundary -- they handle argument deserialization, state extraction, and error mapping. Mixing business logic here makes it untestable (Tauri commands need a running app to invoke). Keeping commands as thin pass-throughs means the real logic lives in subsystem modules that can be unit-tested independently.

**Decision**: Platform gating at the module level in `mod.rs`, not inside individual functions.
**Why**: Entire command surfaces (MTP, network, volumes) are platform-specific. Gating at the module level means the compiler excludes unused code entirely rather than compiling stub functions. This also prevents accidentally calling an unsupported command -- if the module doesn't exist on that platform, the Tauri command isn't registered at all.

**Decision**: `blocking_with_timeout` for all filesystem-touching commands, not just read-only ones.
**Why**: `spawn_blocking` alone doesn't protect against hung NFS/SMB mounts where even a simple `path.exists()` can block indefinitely. The timeout wrapper (2s for reads, 5s for writes, 15s for trash, 30s for recursive scans) returns a fallback value (or error for `Result`-returning commands) instead of freezing the IPC thread or exhausting the blocking pool. The helper lives in `util.rs` so all command files can share it. Commands that already use `spawn_blocking` (P2 commands like `rename_file`, `move_to_trash`) wrap the existing `spawn_blocking` with `tokio::time::timeout` instead.

**Decision**: No `commands/ai.rs` file -- AI commands register directly from `ai::manager` and `ai::suggestions`.
**Why**: The AI subsystem has its own complex lifecycle (model loading, suggestion pipelines). Adding a thin wrapper in `commands/` would just be boilerplate forwarding. Registering directly keeps the AI command surface co-located with its implementation, which changes frequently.

## Key patterns and gotchas

- **No business logic here.** If you find yourself adding branching or data transformation, move it to the relevant subsystem module.
- **`spawn_blocking` for filesystem I/O.** All blocking operations in async commands are wrapped in `tokio::task::spawn_blocking`.
- **`blocking_with_timeout` for potentially slow I/O.** All filesystem-touching commands use either `blocking_with_timeout` (from `util.rs`) or `tokio::time::timeout` around `spawn_blocking`. Timeouts: 2s for reads, 5s for writes (`create_directory`, `rename_file`), 15s for trash (`move_to_trash` — macOS NSFileManager is slow on cold start), 30s for recursive scans (`scan_volume_for_copy`, `scan_volume_for_conflicts`). The helper returns a fallback value on timeout or `JoinError`.
- **`expand_tilde`** is applied conditionally: for `list_directory` it's gated on `volume_id == "root"`, but for write operations (copy, move, delete, scan preview) it's always applied. MTP and network volume paths must never be tilde-expanded.
- **AI commands** are registered directly from `ai::manager` and `ai::suggestions` — there is no `commands/ai.rs` file.
- **Platform gates.** `volumes` is macOS-only; `mtp` and `network` are macOS+Linux; `volumes_linux` is Linux-only. Individual functions also use `#[cfg]` where behaviour differs (e.g., `sync_status`).
- **`start_selection_drag`** requires the main thread. It uses `app.run_on_main_thread()` plus a `std::sync::mpsc` channel to return the result synchronously.
- **`list_shares_with_credentials`** has `#[allow(clippy::too_many_arguments)]` because Tauri command parameters must be top-level arguments — no struct bundling.
- **`set_menu_context` and Close tab (⌘W).** When the main window loses focus, `set_menu_context("other")` disables all
  non-App menu items — but `CLOSE_TAB_ID` is explicitly excluded. On macOS, ⌘W means "close the front window," and the
  `on_menu_event` close-tab exception handles this: if main is focused it closes a tab, otherwise it closes the focused
  non-main window (Settings, viewer, debug). If `CLOSE_TAB_ID` were disabled, its accelerator wouldn't fire and ⌘W would
  stop working in non-main windows. This is the only item that needs this exemption — all other non-App items are
  correctly disabled because they only make sense in the explorer.

## Dependencies

All major subsystems: `file_system`, `volumes`, `mtp`, `network`, `font_metrics`, `icons`,
`file_viewer`, `licensing`, `indexing`, `menu`, `rename`, `sync_status`, and Tauri's `AppHandle` / `State`.
