# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates
immediately to business-logic modules. No significant logic lives here.

## File map

| File | Domain | Notes |
|------|--------|-------|
| `mod.rs` | Re-exports | `mtp`, `network`, `volumes` gated behind `#[cfg(target_os = "macos")]` |
| `file_system.rs` | File listing & writes | Largest file. Streaming + virtual-scroll listing API, write ops, scan preview, conflict resolution, volume copy, native drag, self-drag overlay. Contains `expand_tilde()`. |
| `volumes.rs` | Volume management | `list_volumes`, `get_default_volume_id`, `find_containing_volume`, `get_volume_space` |
| `mtp.rs` | MTP devices | Full MTP command surface (connect, disconnect, list, download, upload, delete, rename, move, scan) |
| `network.rs` | SMB/network shares | Discovery, share listing, keychain, mounting. Also hosts `fe_log` for frontend debug logging. |
| `font_metrics.rs` | Font metrics cache | `store_font_metrics`, `has_font_metrics` |
| `icons.rs` | File icons | `get_icons`, `refresh_directory_icons`, cache clear |
| `rename.rs` | Rename / trash | `move_to_trash` (NSFileManager), `check_rename_permission`, `check_rename_validity`, `rename_file` |
| `file_viewer.rs` | File viewer | Session lifecycle, line search, word wrap, menu state |
| `ui.rs` | UI / menu | Context menu, Finder reveal, clipboard, Quick Look, Get Info, view mode |
| `settings.rs` | Settings | Port availability check, watcher debounce setting, menu accelerator updates |
| `licensing.rs` | Licensing | Status query, activation, expiry, reminder, key validation |
| `indexing.rs` | Drive index | `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats`, `get_dir_stats_batch`, `prioritize_dir`, `cancel_nav_priority`, `clear_drive_index`, `set_indexing_enabled`. Uses `State<IndexManagerState>`. |
| `sync_status.rs` | Cloud sync status | `get_sync_status` — macOS delegates to `file_system::sync_status`; non-macOS returns empty map via `#[cfg]` on the function itself (not the module). |

## Key patterns and gotchas

- **No business logic here.** If you find yourself adding branching or data transformation, move it to the relevant subsystem module.
- **`spawn_blocking` for filesystem I/O.** All blocking operations in async commands are wrapped in `tokio::task::spawn_blocking`.
- **`expand_tilde`** is applied conditionally: for `list_directory` it's gated on `volume_id == "root"`, but for write operations (copy, move, delete, scan preview) it's always applied. MTP and network volume paths must never be tilde-expanded.
- **AI commands** are registered directly from `ai::manager` and `ai::suggestions` — there is no `commands/ai.rs` file.
- **Platform gates.** `mtp`, `network`, and `volumes` modules are macOS-only at the `mod.rs` level. Individual functions also use `#[cfg]` where behaviour differs (e.g., `sync_status`).
- **`start_selection_drag`** requires the main thread. It uses `app.run_on_main_thread()` plus a `std::sync::mpsc` channel to return the result synchronously.
- **`list_shares_with_credentials`** has `#[allow(clippy::too_many_arguments)]` because Tauri command parameters must be top-level arguments — no struct bundling.
- **`fe_log`** (in `network.rs`) receives log messages from the frontend and forwards them to `env_logger`. Kept in `network.rs` for historical reasons; it has nothing to do with networking.

## Dependencies

All major subsystems: `file_system`, `volumes`, `mtp`, `network`, `font_metrics`, `icons`,
`file_viewer`, `licensing`, `indexing`, `menu`, `rename`, `sync_status`, and Tauri's `AppHandle` / `State`.
