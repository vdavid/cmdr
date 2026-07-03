# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates immediately to
business-logic modules. **No business logic here**: branching or data transformation belongs in the relevant subsystem
module.

## Module map

One file per domain (`network.rs`, `mtp.rs`, `clipboard.rs`, etc.), plus `mod.rs` (re-exports + platform gates),
`util.rs` (timeout helpers, see Must-knows), and `file_system/` (listing, path queries, create/copy/move/delete, scan
preview, conflict resolution, drag, stat probe). AI and space-poller commands register DIRECTLY from their own modules
(`ai::*`, `space_poller.rs`): there is intentionally no `commands/ai.rs` or `commands/space_poller.rs`. Per-file
inventory and decision rationale: [DETAILS.md](DETAILS.md).

## Must-knows

- **Every filesystem-touching command is `async` + timeout-wrapped.** `statfs`/`readdir`/`metadata`/NSURL/`realpath`
  block 30-120s on hung mounts, and a hung sync command stalls the whole IPC thread. Tiers: 2s reads, 5s writes
  (`create_directory`, `rename_file`), 15s trash, 30s recursive scans. Three helpers in `util.rs`:
  - `blocking_with_timeout_flag` → `TimedOut<T>` for `Vec`/`HashMap`/`Option`/`()` returns (frontend checks
    `.timedOut`). **Prefer this** over the bare `blocking_with_timeout`, whose timeout is indistinguishable from the
    fallback.
  - `blocking_result_with_timeout` → `Result<T, IpcError>` for commands already returning `Result`. For hand-rolled
    `tokio::time::timeout`, map `Elapsed` to `IpcError::timeout()`.
  - Matching TS types live in `$lib/tauri-commands/ipc-types.ts`. `path_exists` is SMB-aware: a disconnected SMB volume
    returns immediate `false`, so it re-checks `smb_connection_state()` and reports `timedOut: true` instead, so a
    transient blip won't evict the user from a network folder.
- **`expand_tilde` is conditional.** For listing it's gated on `volume_id == "root"`; for write operations (copy, move,
  delete, scan preview) it's always applied. NEVER tilde-expand MTP or network volume paths.
- **`create_directory` / `create_file` / `rename_file` are thin: the logic + the managed instant op live in
  `file_system::write_operations::{create,rename}`.** These commands only expand tilde (root), resolve `volume_id`, wrap
  the module entry in the write timeout, and map to `IpcError`. The mutation runs via `manager::run_instant` (busy-marks
  the volume, appears briefly in the queue, still inline + result-returning). The validity/permission checks
  (`check_rename_validity`, `check_rename_permission`) stay UNMANAGED — the snappy read-only path.
- **The create core errors on an unregistered volume; NO `std::fs` fallback.** "root" and every mount is registered in
  `VolumeManager` at startup, so an unregistered `volume_id` means a race (unmount mid-op). A bare `std::fs` fallback has
  no timeout and breaks the "every FS-touching command is timed" contract on a hung mount; don't re-add it. Unit tests
  register a real local "root" via `ensure_root_volume()` (never `init_volume_manager`).
- **Platform gates at the module level in `mod.rs`, not per-function**, so the compiler excludes the whole surface and an
  unsupported command isn't even registered. `volumes` is macOS-only; `mtp`/`network`/`eject` are macOS+Linux;
  `volumes_linux` is Linux-only. Use per-function `#[cfg]` only where behavior differs (for example `sync_status`).
- **`delete_files` and `rename_file` accept `volume_id`.** Non-root → `delete_files` uses the volume-aware delete and
  skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`); `rename_file` passes `volume_id` through
  for MTP and skips permission checks for non-root. The rename mutation notifies the listing cache after success (the
  local branch via `notify_rename_in_listing`, the volume branch via the volume's own `notify_mutation`).
- **`start_selection_drag` / `start_drag_paths` require the main thread** (`run_drag_on_main_thread`). Each derives
  session locality (`locality_for_volume`, keyed on `Volume::supports_local_fs_access()`): a LOCAL session gets file-URL
  + legacy filenames per item (matching Finder, no path text, which broke browser uploads, issue #28); a VIRTUAL session
  (MTP, direct SMB, search-results) gets no legacy types plus an `NSFilePromiseProvider` per item.
- **⌘W: `CLOSE_TAB_ID` is the one menu item NOT disabled when the main window loses focus.** On focus loss,
  `activate_window_menu("other")` disables all non-App items except this one, because on macOS ⌘W must keep closing the
  front window (Settings, viewer, debug). Disabling it would stop its accelerator firing in non-main windows. See
  `menu/DETAILS.md`.
- **`list_shares_with_credentials` carries `#[allow(clippy::too_many_arguments)]`**: Tauri command params must be
  top-level args (no struct bundling).
