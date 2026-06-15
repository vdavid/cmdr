# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates immediately to
business-logic modules. **No business logic here**: if you're adding branching or data transformation, it belongs in the
relevant subsystem module.

## Module map (one file per domain)

- `mod.rs`: re-exports + platform gates. `util.rs`: shared timeout helpers (see "Timeouts" below).
- `file_system/`: directory listing/streaming, path queries, type-to-jump, Brief column widths, create/copy/move/delete,
  scan preview, conflict resolution, drag, `stat.rs` (batched "is dir?" probe), `e2e_support.rs`.
- `volumes.rs` (macOS) / `volumes_linux.rs` (Linux): list, default, space, path→volume resolution.
- `mtp.rs`, `network.rs` (SMB discovery/mount/reconnect/disconnect, saved-password borrow, lazy-startup hooks),
  `eject.rs`, `clipboard.rs`, `rename.rs`, `icons.rs`, `favorites.rs`, `font_metrics.rs`, `restricted_paths.rs`,
  `file_viewer.rs`, `ui.rs`, `settings.rs`, `mcp.rs`, `licensing.rs`, `indexing.rs`, `search.rs`, `sync_status.rs`,
  `crash_reporter.rs`, `error_reporter.rs`, `beta_signup.rs`.
- AI and space-poller commands register DIRECTLY from their own modules (`ai::manager`/`suggestions`/`api_keys`,
  `space_poller.rs`); there is intentionally no `commands/ai.rs` or `commands/space_poller.rs`.

Per-file function inventories and decision rationale: [DETAILS.md](DETAILS.md).

## Must-knows

- **Every filesystem-touching command is `async` + timeout-wrapped** (`statfs`/`readdir`/`metadata`/NSURL/`realpath`
  block 30-120s on hung mounts; a hung sync command stalls the whole IPC thread). Tiers: 2s reads, 5s writes
  (`create_directory`, `rename_file`), 15s trash, 30s recursive scans. Three helpers in `util.rs`:
  - `blocking_with_timeout_flag(dur, fallback, closure)` → `TimedOut<T>` for `Vec`/`HashMap`/`Option`/`()` returns
    (frontend checks `.timedOut`). **Prefer this** over the bare `blocking_with_timeout`, whose timeout is
    indistinguishable from the fallback.
  - `blocking_result_with_timeout(dur, closure)` → `Result<T, IpcError>` for commands already returning `Result`
    (timeout → `Err(IpcError { timedOut: true })`). For hand-rolled `tokio::time::timeout`, map `Elapsed` to
    `IpcError::timeout()`.
  - Matching TS types live in `$lib/tauri-commands/ipc-types.ts`. `path_exists` returns `TimedOut<bool>` and is
    SMB-aware: a `Disconnected` `SmbVolume` returns immediate `false`, so the command re-checks
    `volume.smb_connection_state()` and reports `timedOut: true` (so the frontend won't evict users from a network folder
    on a transient blip).
- **`expand_tilde` is conditional.** For `list_directory` it's gated on `volume_id == "root"`; for write operations
  (copy, move, delete, scan preview) it's always applied. MTP and network volume paths must NEVER be tilde-expanded.
- **`create_directory_core` / `create_file_core` error on an unregistered volume; NO `std::fs` fallback.** "root" and
  every mount is registered in `VolumeManager` at startup, so an unregistered `volume_id` means a race (unmount mid-op).
  A bare `std::fs` fallback had no timeout and broke the "every FS-touching command is timed" contract on a hung mount;
  don't re-add it. Unit tests register a real local "root" via `ensure_root_volume()` (they never call
  `init_volume_manager`).
- **Platform gates at the module level in `mod.rs`, not per-function.** `volumes` is macOS-only; `mtp` and `network` are
  macOS+Linux; `volumes_linux` is Linux-only. Compiler excludes the whole surface (so an unsupported command isn't even
  registered). Individual functions use `#[cfg]` only where behavior differs (for example `sync_status`).
- **`delete_files` and `rename_file` accept `volume_id`.** Non-root → `delete_files` routes to the volume-aware delete
  and skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`); `rename_file` passes `volume_id`
  through for MTP and skips permission checks for non-root volumes. `rename_file` calls `notify_mutation` after success.
- **`start_selection_drag` / `start_drag_paths` require the main thread** (via `run_drag_on_main_thread`). Each derives
  the session locality (`locality_for_volume`, keyed on `Volume::supports_local_fs_access()`) for
  `native_drag::start_drag`: a LOCAL session gets file-URL + legacy filenames per item (matching Finder, no path text,
  which broke browser uploads, issue #28); a VIRTUAL session (MTP, direct SMB, search-results) gets no legacy types plus
  an `NSFilePromiseProvider` per item. Composition is the pure `native_drag::type_plan::plan_pasteboard_items`.
- **Close tab (⌘W): `CLOSE_TAB_ID` is the one menu item NOT disabled when the main window loses focus.** On focus loss,
  `activate_window_menu("other")` disables all non-App items except this one, because on macOS ⌘W must keep closing the
  front window (Settings, viewer, debug) via the `on_menu_event` exception. Disabling it would stop its accelerator
  firing and break ⌘W in non-main windows. See `menu/DETAILS.md`.
- **`list_shares_with_credentials` carries `#[allow(clippy::too_many_arguments)]`** because Tauri command params must be
  top-level args (no struct bundling).

Full details: [DETAILS.md](DETAILS.md).
