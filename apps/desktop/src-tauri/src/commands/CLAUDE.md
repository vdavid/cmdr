# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates
immediately to business-logic modules. No significant logic lives here.

## File map

| File | Domain | Notes |
|------|--------|-------|
| `mod.rs` | Re-exports | `mtp`, `network` gated behind `#[cfg(any(target_os = "macos", target_os = "linux"))]`; `volumes` behind `#[cfg(target_os = "macos")]`; `volumes_linux` behind `#[cfg(target_os = "linux")]` |
| `util.rs` | Shared helpers | `TimedOut<T>`, `IpcError`, `blocking_with_timeout`, `blocking_with_timeout_flag`, `blocking_result_with_timeout`. See "Timeout-aware return types" below. |
| `file_system/` | File listing & writes | Directory module split by operation type. `mod.rs` has `expand_tilde()`, re-exports, and tests. `listing.rs`: streaming + virtual-scroll listing API, path queries, benchmarking. `write_ops.rs`: create, copy, move, delete, trash, scan preview, conflict resolution, synthetic diff helpers. `volume_copy.rs`: cross-volume copy/move/scan, `SourceItemInput`. `drag.rs`: native drag, self-drag overlay. `e2e_support.rs`: feature-gated E2E/debug commands. |
| `volumes.rs` | Volume management (macOS) | `list_volumes`, `get_default_volume_id`, `get_volume_space`, `resolve_path_volume` (statfs-based, no volume enumeration) |
| `volumes_linux.rs` | Volume management (Linux) | Same interface as `volumes.rs`, delegates to `volumes_linux` module |
| `mtp.rs` | MTP devices | Full MTP command surface (connect, disconnect, list, download, upload, delete, rename, move, scan) |
| `network.rs` | SMB/network shares | Discovery, share listing, keychain, mounting, direct-connection upgrade. Upgrade business logic (address resolution, credential lookup, smb2 connection) lives in `network::smb_upgrade`; commands here are thin wrappers. |
| `font_metrics.rs` | Font metrics cache | `store_font_metrics`, `has_font_metrics` |
| `icons.rs` | File icons | `get_icons`, `refresh_directory_icons`, cache clear |
| `rename.rs` | Rename / trash | `move_to_trash` (delegates to `write_operations::trash::move_to_trash_sync`), `check_rename_permission`, `check_rename_validity`, `rename_file`. `rename_file` calls `notify_mutation` after success to update the listing cache (both local and volume-aware paths). |
| `file_viewer.rs` | File viewer | Session lifecycle, line search, word wrap, menu state |
| `ui.rs` | UI / menu | Context menu, Finder reveal, clipboard, Quick Look, Get Info, view mode, `set_menu_context` (enables/disables file-scoped menu items based on window focus) |
| `settings.rs` | Settings | Port availability check, watcher debounce, menu accelerator updates, and live-apply setters for `network.directSmbConnection`, `advanced.filterSafeSaveArtifacts`, `network.smbConcurrency` |
| `mcp.rs` | MCP server | `set_mcp_enabled`, `set_mcp_port` — live start/stop/port-change of the MCP server without app restart |
| `licensing.rs` | Licensing | Status query, activation, expiry, reminder, key validation |
| `indexing.rs` | Drive index | `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats`, `get_dir_stats_batch`, `clear_drive_index`, `set_indexing_enabled`, `get_index_debug_status` (dev-only extended stats). Uses `State<IndexManagerState>`. |
| `clipboard.rs` | Clipboard file ops | `copy_files_to_clipboard`, `cut_files_to_clipboard`, `read_clipboard_files`, `clear_clipboard_cut_state`. macOS uses NSPasteboard via `clipboard::pasteboard`; non-macOS stubs return errors. |
| `crash_reporter.rs` | Crash reporting | `check_pending_crash_report`, `dismiss_crash_report`, `send_crash_report`. Delegates to `crash_reporter` module. Send is skipped in dev/CI. |
| `search.rs` | Drive search | Thin IPC wrappers over `search` module. `resolve_ai_backend` for AI provider config. Post-filters directory sizes after `fill_directory_sizes`. |
| `sync_status.rs` | Cloud sync status | `get_sync_status` — macOS delegates to `file_system::sync_status`; non-macOS returns empty map via `#[cfg]` on the function itself (not the module). |

## Key decisions

**Decision**: One commands file per domain, with no business logic in commands.
**Why**: Tauri command functions are the IPC boundary -- they handle argument deserialization, state extraction, and error mapping. Mixing business logic here makes it untestable (Tauri commands need a running app to invoke). Keeping commands as thin pass-throughs means the real logic lives in subsystem modules that can be unit-tested independently.

**Decision**: Platform gating at the module level in `mod.rs`, not inside individual functions.
**Why**: Entire command surfaces (MTP, network, volumes) are platform-specific. Gating at the module level means the compiler excludes unused code entirely rather than compiling stub functions. This also prevents accidentally calling an unsupported command -- if the module doesn't exist on that platform, the Tauri command isn't registered at all.

**Decision**: `blocking_with_timeout` for all filesystem-touching commands, not just read-only ones.
**Why**: `spawn_blocking` alone doesn't protect against hung NFS/SMB mounts where even a simple `path.exists()` can block indefinitely. The timeout wrapper (2s for reads, 5s for writes, 15s for trash, 30s for recursive scans) returns a fallback value (or error for `Result`-returning commands) instead of freezing the IPC thread or exhausting the blocking pool. The helper lives in `util.rs` so all command files can share it. Commands that already use `spawn_blocking` (P2 commands like `rename_file`, `move_to_trash`) wrap the existing `spawn_blocking` with `tokio::time::timeout` instead.

**Decision**: Timeout-aware return types (`TimedOut<T>` and `IpcError`) for all timeout-protected commands.
**Why**: The original `blocking_with_timeout` returned a fallback value indistinguishable from a real empty/none result. The frontend couldn't tell "no volumes mounted" from "timed out before listing volumes." Two wrapper types solve this:
- `TimedOut<T>` (`{ data: T, timedOut: bool }`) — for commands that don't return `Result` (collections, `Option`, `()`). Use `blocking_with_timeout_flag` to produce these.
- `IpcError` (`{ message: String, timedOut: bool }`) — for commands returning `Result<T, _>`. Use `blocking_result_with_timeout` or map `tokio::time::timeout` errors to `IpcError::timeout()`.
The frontend has matching TypeScript types in `$lib/tauri-commands/ipc-types.ts` (`TimedOut<T>`, `IpcError`, `isIpcError`, `getIpcErrorMessage`). The old `blocking_with_timeout` is kept for `path_exists` and other commands where timeout distinction isn't needed.

**Decision**: JSON for all Tauri IPC, not binary formats (MessagePack, Protobuf).
**Why**: Benchmarked with real directory listings: MessagePack is 34-58% SLOWER than JSON despite being 17-19% smaller. Tauri serializes `Vec<u8>` as a JSON array of numbers, so binary data gets wrapped in JSON anyway, negating size benefits and adding extra decoding overhead. See [benchmark data](../../../../../docs/notes/json-ipc-benchmarks.md).

**Decision**: No `commands/ai.rs` file -- AI commands register directly from `ai::manager` and `ai::suggestions`.
**Why**: The AI subsystem has its own complex lifecycle (model loading, suggestion pipelines). Adding a thin wrapper in `commands/` would just be boilerplate forwarding. Registering directly keeps the AI command surface co-located with its implementation, which changes frequently.

**Decision**: No `commands/space_poller.rs` -- space poller commands register directly from `space_poller.rs`.
**Why**: Same reasoning as AI. The poller has its own lifecycle (init, start, watch/unwatch). Three commands: `watch_volume_space`, `unwatch_volume_space`, `set_disk_space_threshold`.

## Key patterns and gotchas

- **No business logic here.** If you find yourself adding branching or data transformation, move it to the relevant subsystem module.
- **`spawn_blocking` for filesystem I/O.** All blocking operations in async commands are wrapped in `tokio::task::spawn_blocking`.
- **Timeout-protected I/O with distinguishable results.** All filesystem-touching commands use timeout wrappers from `util.rs`. Timeouts: 2s for reads, 5s for writes (`create_directory`, `rename_file`), 15s for trash (`move_to_trash`), 30s for recursive scans. Three helpers:
  - `blocking_with_timeout(duration, fallback, closure)` — returns raw `T`, timeout indistinguishable from fallback. Used only where distinction isn't needed (like `path_exists`).
  - `blocking_with_timeout_flag(duration, fallback, closure)` → `TimedOut<T>` — for commands returning `Vec`, `HashMap`, `Option`, or `()`. Frontend unwraps `.data` and can check `.timedOut`.
  - `blocking_result_with_timeout(duration, closure)` → `Result<T, IpcError>` — for commands that already return `Result`. Timeout becomes `Err(IpcError { message, timedOut: true })`.
  - For P2 commands using raw `tokio::time::timeout`, map the `Elapsed` error to `IpcError::timeout()` and other errors to `IpcError::from_err(msg)`.
- **`expand_tilde`** is applied conditionally: for `list_directory` it's gated on `volume_id == "root"`, but for write operations (copy, move, delete, scan preview) it's always applied. MTP and network volume paths must never be tilde-expanded.
- **AI commands** are registered directly from `ai::manager` and `ai::suggestions` — there is no `commands/ai.rs` file.
- **Platform gates.** `volumes` is macOS-only; `mtp` and `network` are macOS+Linux; `volumes_linux` is Linux-only. Individual functions also use `#[cfg]` where behaviour differs (e.g., `sync_status`).
- **`delete_files` and `rename_file` accept `volume_id`.** When set to a non-root volume, `delete_files` routes to the volume-aware delete path and skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`). `rename_file` passes `volume_id` through for MTP rename support; permission checks are skipped for non-root volumes.
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
