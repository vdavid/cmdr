# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates immediately.
**No business logic here**: branching or data transformation belongs in the relevant subsystem module.

## Module map

One file per domain (`network.rs`, `mtp.rs`, `clipboard.rs`, etc.), plus `mod.rs` (re-exports + platform gates),
`util.rs` (timeout helpers, see Must-knows), `file_system/`, `media_index/`, and `importance.rs`. AI and space-poller
commands register from their own modules instead, so there's intentionally no `ai` or `space_poller` here; the index
subsystems are the reverse, since they can't carry `tauri::`. Inventory and rationale: `DETAILS.md`.

## Must-knows

- **Every filesystem-touching command is `async` + timeout-wrapped.** `statfs`/`readdir`/`metadata`/NSURL/`realpath`
  block 30-120s on hung mounts, and a hung sync command stalls the whole IPC thread. Tiers: 2s reads, 5s writes
  (`create_directory`, `rename_file`), 15s trash, 30s recursive scans. Three helpers in `util.rs`:
  - `blocking_with_timeout_flag` → `TimedOut<T>` for `Vec`/`HashMap`/`Option`/`()` returns. **Prefer this** over the
    bare `blocking_with_timeout`, whose timeout is indistinguishable from the fallback.
  - `blocking_result_with_timeout` → `Result<T, IpcError>` for commands already returning `Result`. For hand-rolled
    `tokio::time::timeout`, map `Elapsed` to `IpcError::timeout()`.
  - `timeout_detached` → **required when the future can reach a device backend** (rename, conflict/copy scans): it
    times out the JOIN HANDLE, never the work. ❌ A bare `tokio::time::timeout` drops the future and wedges an MTP
    phone. `DETAILS.md` § "IPC deadlines detach, never drop".
  - ❌ Don't wrap `sync_status`: it applies its own deadline, keeping partial results and the still-running batch.
    `../file_system/sync_status/DETAILS.md`.
  - Matching TS types live in `$lib/tauri-commands/ipc-types.ts`. `path_exists` is SMB-aware: a disconnected SMB volume
    returns immediate `false`, so it re-checks `smb_connection_state()` and reports `timedOut: true` — a transient blip
    won't evict the user from a network folder.
- **`expand_tilde` is conditional.** For listing it's gated on `volume_id == "root"`; for write operations (copy, move,
  delete, scan preview) it's always applied. NEVER tilde-expand MTP or network volume paths.
- **`create_directory` / `create_file` / `rename_file` are thin: the logic + the managed instant op live in
  `file_system::write_operations::{create,rename}`.** They only expand tilde (root), resolve `volume_id`, apply the
  write timeout, and map to `IpcError`; the mutation runs via `manager::run_instant` (busy-marks the volume, appears
  briefly in the queue, still inline and result-returning). `check_rename_validity` / `check_rename_permission` stay
  UNMANAGED — the snappy read-only path.
- **The create core errors on an unregistered volume; NO `std::fs` fallback.** Every mount registers at startup, so an
  unregistered `volume_id` means a race (unmount mid-op), and a bare `std::fs` fallback has no timeout — don't re-add
  it. Unit tests register a real local "root" via `ensure_root_volume()` (never `init_volume_manager`).
- **Platform gates at the module level in `mod.rs`, not per-function**, so an unsupported command isn't even
  registered. `volumes` is macOS-only; `mtp`/`network`/`eject` are macOS+Linux;
  `volumes_linux` is Linux-only. Use per-function `#[cfg]` only where behavior differs (for example `sync_status`).
- **`delete_files` and `rename_file` accept `volume_id`.** Non-root → `delete_files` uses the volume-aware delete and
  skips local `validate_sources` (MTP virtual paths fail `symlink_metadata`); `rename_file` passes `volume_id` through
  and skips permission checks. The rename notifies the listing cache after success (local via
  `notify_rename_in_listing`, volume via its own `notify_mutation`).
- **`start_selection_drag` / `start_drag_paths` require the main thread** (`run_drag_on_main_thread`). Each derives
  session locality (`locality_for_volume`, keyed on `Volume::paths_are_os_visible()`, ❌ never
  `supports_local_fs_access()` — direct SMB says `false` there yet its mounted paths open anywhere): a LOCAL session
  gets file-URL + legacy filenames per item (matching Finder, no path text, which broke browser uploads); a VIRTUAL one
  (MTP, search-results, archive-inner) gets an `NSFilePromiseProvider` per item, which only Finder can read.
- **⌘W: `CLOSE_TAB_ID` is the one menu item NOT disabled when the main window loses focus** — disabling it stops the
  accelerator closing the front Settings/viewer/debug window. `menu/DETAILS.md`.
- **`list_shares_with_credentials` carries `#[allow(clippy::too_many_arguments)]`**: Tauri params must be top-level
  args.
