# Commands module

Thin Tauri IPC layer. Each file groups one domain's `#[tauri::command]` functions and delegates.
**No business logic here**: branching or transformation belongs in the subsystem.

## Module map

One file per domain plus `mod.rs` (re-exports + platform gates), `util.rs` (timeouts and budgets), and the directories
`agent/`, `file_system/`, and `media_index/`. `servers.rs` is the protocol-agnostic facade over `sftp.rs`, `webdav.rs`,
and `network.rs`. AI and space-poller commands register themselves; the index subsystems are the reverse, since they
can't carry `tauri::`.

## Must-knows

- **Every filesystem-touching command is `async` + timeout-wrapped**, as is any whose cost grows with the data: a sync
  `#[tauri::command]` runs on the MAIN thread (an in-memory scan of a 74k listing once stopped the app answering IPC).
  Tiers: 2 s reads, 5 s writes, 15 s trash, 30 s recursive scans.
- **Time out through `util.rs`, ❌ never a bare `tokio::time::timeout`**: it drops the future and wedges an MTP phone
  mid-transaction. Its helpers time out the JOIN HANDLE instead and mint the command's own error type. Multi-leg
  commands share ONE `Deadline`, ❌ never a fresh 30 s per leg. ❌ Don't wrap `sync_status`: it carries its own.
- **❌ Every command's `Err` is its own typed enum, never a shared message-carrying struct**, or typed refusals flatten
  into untranslated English sentences that reach users (a generic one had spread to 39 call sites). Reuse the family's
  vocabulary (`MutationError`, `ViewerError`, `VolumeError`) or add a small enum beside it.
  `docs/guides/error-handling.md`.
- **A command the FRONTEND can re-issue faster than it completes needs a `BlockingBudget` (`util.rs`)**, one `static`
  per family, SHARED across the commands contending for one resource. One unbounded command once took all 512 pool
  threads and froze the app.
- **`expand_tilde` is conditional**: gated on `volume_id == "root"` for listing, always applied for writes. ❌ Never
  tilde-expand an MTP or network path.
- **A path from the transfer dialog is VOLUME-RELATIVE and must be anchored where its volume is still known**
  (`write_operations::resolve_dest_path`, `path_exists`): handing `/photos` to a share unanchored breaks the write
  before any I/O. `../file_system/volume/CLAUDE.md`.
- **The create core errors on an unregistered volume; ❌ NO `std::fs` fallback** (it would carry no timeout). An
  unregistered id means an unmount race. Unit tests use `ensure_root_volume()`, ❌ never `init_volume_manager`.
- **Platform-gate at the module level in `mod.rs`, ❌ not per-function**, so an unsupported command isn't registered.
  Per-function `#[cfg]` only where behavior differs (`sync_status`).
- **Drag locality comes from `Volume::paths_are_os_visible()`, ❌ never `supports_local_fs_access()`**, and both drag
  commands run on the main thread.

Inventory, per-command decisions, and the detach rationale: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
