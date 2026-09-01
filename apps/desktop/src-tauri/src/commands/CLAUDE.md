# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates.
**No business logic here**: branching or transformation belongs in the subsystem.

## Module map

One file per domain (`network.rs`, `sftp.rs`, `webdav.rs`, `mtp.rs`, `clipboard.rs`, …), plus `mod.rs` (re-exports +
platform gates), `util.rs` (timeout and budget helpers), `file_system/`, `media_index/`, and
`importance.rs`. AI and space-poller commands register from their own modules; the index subsystems are the reverse,
since they can't carry `tauri::`. Inventory and rationale: `DETAILS.md`; `sftp.rs`'s frontend contract is
`crates/cmdr-sftp/DETAILS.md` § "Connecting from the frontend"; `webdav.rs`'s, `crates/cmdr-webdav/DETAILS.md`.

## Must-knows

- **Every filesystem-touching command is `async` + timeout-wrapped**, as is any whose cost grows with the data: a sync
  `#[tauri::command]` runs on the MAIN thread, so an in-memory scan of a 74k listing once stopped the app answering
  IPC. Sync is for constants and flag flips. `statfs`/`readdir`/`metadata`/NSURL/`realpath` block 30-120 s on hung
  mounts. Tiers: 2 s reads, 5 s writes, 15 s trash, 30 s recursive scans. Helpers in `util.rs`:
  - `blocking_with_timeout_flag` → `TimedOut<T>` (**prefer it**: the bare `blocking_with_timeout`'s timeout is
    indistinguishable from its fallback).
  - `timeout_detached_typed` (async) / `blocking_typed_result_with_timeout` (pool) → **required when the work can reach
    a device backend**: they time out the JOIN HANDLE, never the work, and mint the command's OWN error type at the
    deadline. ❌ A bare `tokio::time::timeout` drops the future and wedges an MTP phone.
  - **Several legs take ONE `Deadline`, each leg `timeout_detached_within`**, ❌ never a fresh 30 s per leg (the
    command's promise becomes their sum, which no spinner can be sized against).
  - ❌ Don't wrap `sync_status`: it has its own deadline. DETAILS § "IPC deadlines detach, never drop".
- **❌ Every command's `Err` is its own typed enum, never a shared message-carrying struct.** A generic one existed and
  flattened 39 typed refusals into English sentences that reached users untranslated. Reuse the vocabulary the command
  belongs to (`MutationError`, `ViewerError`, `VolumeError` nested in either), or add a small enum beside the family.
  `docs/guides/error-handling.md`.
- **A command the FRONTEND can re-issue faster than it completes needs a `BlockingBudget` (`util.rs`)**, one `static`
  per family, SHARED across the commands contending for one resource. The pool caps at 512 threads; one unbounded
  command took all of it and froze the app until restart.
- **`expand_tilde` is conditional**: gated on `volume_id == "root"` for listing, always applied for writes. ❌ Never
  tilde-expand an MTP or network path.
- **A path from the transfer dialog is VOLUME-RELATIVE and must be anchored where its volume is still known**:
  `write_operations::resolve_dest_path` (every copy / move / compress / scan dest) and `path_exists`. Handing `/photos`
  to a share unanchored breaks the write before any I/O. `../file_system/volume/CLAUDE.md`.
- **Write commands stay thin over `file_system::write_operations`**, whose mutations run via `manager::run_instant`
  (busy-marks the volume, still inline). `check_rename_validity` / `check_rename_permission` stay UNMANAGED: the snappy
  read-only path. All four answer with `MutationError`.
- **The create core errors on an unregistered volume; ❌ NO `std::fs` fallback** (it would carry no timeout). An
  unregistered id means an unmount race. Unit tests use `ensure_root_volume()`, ❌ never `init_volume_manager`.
- **Platform-gate at the module level in `mod.rs`, ❌ not per-function**, so an unsupported command isn't registered.
  Per-function `#[cfg]` only where behavior differs (`sync_status`).
- **`delete_files` and `rename_file` accept a `volume_id`** and change behavior for non-root volumes. DETAILS § File inventory.
- **The drag commands need the main thread, and locality comes from `Volume::paths_are_os_visible()`, ❌ never
  `supports_local_fs_access()`.** DETAILS § "Drag session locality".
- **⌘W: `CLOSE_TAB_ID` is the one menu item NOT disabled when the main window loses focus.** `menu/DETAILS.md`.

Inventory, decisions, and the detach rationale: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
