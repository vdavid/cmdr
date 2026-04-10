# Write operations

Copy, move, delete, and trash with streaming progress, cancellation, conflict resolution, and rollback. macOS and Linux.

## Purpose

Implements the four destructive file operations as background tasks that stream Tauri events to the frontend. Every
operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites,
network mounts, cross-filesystem moves, and name/path length limits.

## Files

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API: `copy_files_start`, `move_files_start`, `delete_files_start`, `trash_files_start`. Each delegates to `start_write_operation` which handles state creation, spawn lifecycle, cleanup, and error/panic recovery. Validation runs inside the handler closure on the blocking thread pool — never on the async executor. |
| `types.rs` | All serializable types: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events. |
| `state.rs` | Two `LazyLock<RwLock<HashMap>>` caches (`WRITE_OPERATION_STATE`, `OPERATION_STATUS_CACHE`). `WriteOperationState`, `CopyTransaction`, `ScanResult`, `FileInfo`. |
| `helpers.rs` | Validation (`validate_sources`, `validate_destination_writable` via `libc::access`, `validate_disk_space` via `statvfs`). Conflict resolution (condvar wait for Stop mode). `safe_overwrite_file`/`safe_overwrite_dir` (temp+rename). `find_unique_name`. `run_cancellable`. `is_same_filesystem` (device IDs). Background cleanup helpers: `remove_file_in_background`, `remove_dir_all_in_background`. |
| `scan.rs` | `scan_sources` (recursive walk, emits progress), `dry_run_scan`, scan preview subsystem (`start_scan_preview`, `cancel_scan_preview`). |
| `copy.rs` | `copy_files_with_progress`: scan → disk space check → per-file copy via `copy_single_item`. `CopyTransaction` for rollback. |
| `move_op.rs` | Same-fs: `fs::rename`. Cross-fs: copy to `.cmdr-staging-<uuid>`, atomic rename, delete sources. |
| `delete.rs` | Scan, delete files first, then directories in reverse/deepest-first order. Not rollbackable. Also contains `delete_volume_files_with_progress` for non-local volumes (MTP): scans via `volume.list_directory()`, deletes via `volume.delete()` per item. |
| `trash.rs` | `move_to_trash_sync()` (macOS: ObjC `trashItemAtURL`; Linux: `trash` crate; reused by `commands/rename.rs`) and `trash_files_with_progress()` (batch trash with per-item progress, cancellation, partial failure). Uses `symlink_metadata()` for existence checks (handles dangling symlinks). |
| `copy_strategy.rs` | Strategy selection per file: network FS → chunked copy; overwrite → temp+rename; macOS → `copyfile(3)`; Linux → `copy_file_range(2)`. |
| `macos_copy.rs` | FFI to macOS `copyfile(3)`. Preserves xattrs, ACLs, resource forks, Finder metadata. Supports APFS `clonefile`. |
| `linux_copy.rs` | Linux `copy_file_range(2)` with reflink support on btrfs/XFS. 4 MB chunks, cancellation between iterations. |
| `chunked_copy.rs` | 1 MB chunked read/write — the default copy method for all non-APFS-clonefile copies on macOS and network copies on Linux. Checks cancellation between chunks. Copies xattrs, ACLs, timestamps. |
| `volume_copy.rs`, `volume_conflict.rs`, `volume_strategy.rs` | Volume-to-volume copy/move (Local↔MTP abstraction). Handles conflict detection, resolution (Stop/Skip/Overwrite/Rename), progress, rollback (delete all copied files in reverse with progress), and partial-file cleanup on cancel. Wired into Tauri commands `copy_between_volumes` and `move_between_volumes`. |
| `tests.rs`, `integration_test.rs` | Unit and integration tests. |

## Architecture / data flow

```
Frontend
  → WriteOperationState created (AtomicBool cancelled, Condvar for Stop conflicts)
  → stored in WRITE_OPERATION_STATE + OPERATION_STATUS_CACHE
  → operationId returned to frontend immediately (dialog opens, cancel is possible)
  → tokio::spawn (async wrapper)
      → tokio::task::spawn_blocking (all blocking I/O here)
          → validate (sources exist, dest writable, not same location, dest not inside source)
          → scan phase: walk_dir_recursive, emit scan-progress events
          → disk space check (statvfs)
          → execute phase: per-file copy/delete
              → throttled write-progress events (200ms default)
          → success: CopyTransaction::commit(), emit write-complete
          → cancel (Stopped): CopyTransaction::commit(), emit write-cancelled (rolled_back: false)
          → cancel (RollingBack): rollback_with_progress() → emit write-progress (phase: rolling_back) → emit write-cancelled
          → error: CopyTransaction::rollback(), emit write-error
      → safety net: start_write_operation emits write-error for unhandled handler errors
  → state removed from both caches
```

## Key patterns and gotchas

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**`OperationIntent` state machine.** Replaces the old `cancelled: AtomicBool` + `skip_rollback: AtomicBool` pair with a
single `AtomicU8`-backed enum: `Running → RollingBack` (user clicks Rollback), `Running → Stopped` (user clicks Cancel
or teardown), `RollingBack → Stopped` (user cancels the rollback). `Stopped` is terminal. The `is_cancelled()` helper
returns true for both `RollingBack` and `Stopped`, so the 40+ cancellation check sites just call `is_cancelled(&state.intent)`.

**Cancel vs Rollback — distinct behaviors:**
- **Cancel (`Stopped`)**: Stop immediately. Keep all fully-copied files. Delete only the last *partial* file (a
  half-written file is corrupted data, not useful to keep). `rolled_back: false`.
- **Rollback (`RollingBack`)**: Stop copying, then delete ALL files copied so far in reverse order with progress
  events (`phase: RollingBack`). The progress bars go backwards. User can cancel the rollback (→ `Stopped`), which
  keeps whatever hasn't been deleted yet. `rolled_back: true`.
- Both are triggered from the same `cancel_write_operation` IPC call, distinguished by the `rollback` parameter.

**Two-layer cancellation.** `AtomicU8` for fast in-loop checks. `run_cancellable` wraps blocking operations (e.g.,
network-mount copies that may block indefinitely) in a separate thread, polling the flag every 100ms via `mpsc::channel`.

**`CopyTransaction` rollback: sync with progress.** `rollback()` (synchronous, for error paths) and tracked
`rollback_with_progress()` in `copy.rs` (for user-initiated rollback — emits `write-progress` events with
`phase: RollingBack`, checks for `Stopped` between file deletions so the user can cancel the rollback). Auto-rollback
via `Drop` remains as a panic safety net. Delete operations are not rollbackable.

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>`
of canonicalized paths.

**Safe overwrite: temp + backup + rename.** Steps: copy source → `dest.cmdr-tmp-<uuid>`, rename dest → `dest.cmdr-backup-<uuid>`,
rename temp → dest, delete backup. The original is intact until step 3 completes.

**Stop-mode conflict resolution.** Emits `write-conflict` event, then blocks on a `Condvar` with a 300s safety timeout.
Frontend calls `resolve_write_conflict(operation_id, resolution, apply_to_all)` which stores a `ConflictResolutionResponse`
and notifies the condvar. `cancel_write_operation` also notifies the condvar to unblock.

**`cancel_write_operation` does state transitions.** `rollback=true` → `Running → RollingBack`, `rollback=false` →
`Running → Stopped` or `RollingBack → Stopped`. First caller's decision wins — subsequent calls with different intent
are no-ops (unless transitioning from `RollingBack → Stopped`). `cancel_all_write_operations` always transitions to
`Stopped` (teardown should never silently roll back without visual feedback).

**Scan preview caching.** `start_scan_preview` runs a background scan, caches the result in `SCAN_PREVIEW_RESULTS`. The
actual `copy_files_start` can consume the cache via `preview_id` in `WriteOperationConfig`, skipping a redundant scan.

**Progress throttled to 200ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Move strategy.** Same filesystem detected via device ID comparison (`MetadataExt::dev`). Cross-filesystem move uses a
`.cmdr-staging-<uuid>` dir at the destination root, then atomic `rename` into place, then source deletion.

**Move rollback (same-FS).** `MoveTransaction` in `move_op.rs` tracks `(source, dest)` pairs for each rename. On
cancellation, renames are reversed in reverse order. Same-FS rename rollback is instant (just another rename), so it
runs synchronously. Cross-FS move rollback is handled by `CopyTransaction` (deletes the staging directory).

**Intentional duplication: `merge_move_directory` vs `copy_single_item`.** Both implement recursive merge with conflict
resolution, but differ in every detail: copy has progress tracking, symlink handling, byte counting, strategy selection,
and `CopyTransaction` recording. Move uses simple `fs::rename`. A shared abstraction would be forced and fragile.
Cross-references are in the doc comments of both functions.

**Copy strategy selection** (`copy_strategy.rs`):
- macOS, same APFS volume → `copyfile(3)` with `COPYFILE_CLONE` for instant clonefile
- macOS, everything else → `chunked_copy_with_metadata` (1 MB chunks, cancellation between chunks)
- Linux, network → `chunked_copy_with_metadata`
- Linux, local → `copy_single_file_linux` (`copy_file_range(2)`, supports reflink on btrfs/XFS)
- Other platforms → `std::fs::copy` fallback

**Trash has no scan phase.** `trashItemAtURL` is atomic per top-level item (the OS moves the entire tree), so trash
doesn't need the recursive scan that delete/copy use. Progress tracks top-level items, with optional byte-level progress
from pre-computed item sizes. Partial failure is supported: if some items fail, others still succeed. The core
`move_to_trash_sync()` is extracted to `trash.rs` and reused by `commands/rename.rs`.

**`cancel_all_write_operations` for teardown safety.** A `beforeunload` listener calls this to cancel all active
operations (with rollback) on hot-reload, tab close, window close, or navigation. Prevents orphaned background
operations when the frontend is destroyed.

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

**Validation runs inside `spawn_blocking`.** The `*_files_start` functions return an `operationId` immediately, before
any filesystem I/O. Validation (`validate_sources`, `validate_destination_writable`, etc.) runs inside the handler
closure on the blocking thread pool. This keeps the Tauri IPC handler non-blocking, so the frontend can always open
the progress dialog and offer cancel, even if a network mount is stalled.

**`start_write_operation` emits `write-error` for handler errors.** The spawn wrapper matches on the handler's
`Result`: `Ok(Ok(()))` and `Ok(Err(Cancelled))` are no-ops (handlers already emitted the right events), `Ok(Err(e))`
emits `write-error` as a safety net, and `Err(join_error)` handles panics. Double-emit is harmless because the
frontend's `handleError` removes all listeners on first receipt.

**Background cleanup is best-effort.** `remove_file_in_background` and `remove_dir_all_in_background` run on detached
threads (used for temp/backup file cleanup, not for user-visible rollback). If the network mount disconnects or the app
exits, partial files or staging directories may remain on disk. These use the `.cmdr-` prefix, so they're recognizable.

**`volume_copy` path is fully wired up.** The three `volume_*` files are re-exported from `mod.rs` and called by the `copy_between_volumes` and `move_between_volumes` Tauri commands. Both copy and move operations support conflict detection and resolution (Stop/Skip/Overwrite/Rename) for all volume combinations (Local↔MTP, MTP↔MTP). Volume copy supports rollback (delete all copied files in reverse order with progress events, matching the local copy's `rollback_with_progress` pattern) and cancel cleanup (delete only the last partial file). Rollback uses `delete_volume_path_recursive` which lists directory contents via `Volume::list_directory` and deletes children before parents.

## Events emitted

| Event | Trigger |
|-------|---------|
| `write-progress` | Every ~200ms during copy/move/delete/trash |
| `write-conflict` | Stop mode hit a conflicting destination file |
| `write-complete` | Operation finished successfully |
| `write-cancelled` | Operation cancelled (includes `rolled_back` flag) |
| `write-error` | Operation failed (emitted by handler and/or `start_write_operation` safety net) |
| `write-source-item-done` | All files for a top-level source item processed (for gradual deselection) |
| `dry-run-complete` | `config.dry_run == true` (returns `DryRunResult`) |
| `scan-preview-progress` | During `start_scan_preview` |
| `scan-preview-complete` | Preview scan finished |
| `scan-preview-error` | Preview scan failed |
| `scan-preview-cancelled` | Preview scan cancelled |

## Key decisions

**Decision**: `delete_files_start` routes to either `delete_files_with_progress` (local, uses `walkdir` + `fs::remove_file`) or `delete_volume_files_with_progress` (non-local, uses `Volume` trait) based on `volume_id`.
**Why**: MTP volumes can't use `walkdir` or `fs::remove_*`. Rather than refactoring the existing local delete to go through the Volume trait (which would add overhead for local ops), we keep the fast local path and add a parallel volume-aware path. Both emit identical events so the frontend progress dialog works unchanged.

**Decision**: Keep `exacl` crate for ACL copy in chunked copies (not custom FFI bindings).
**Why**: `exacl` adds zero new transitive dependencies (all of its deps — `bitflags`, `log`, `scopeguard`, `uuid` — are already in our tree). It provides cross-platform ACL support (macOS, Linux, FreeBSD) and full ACL parsing/manipulation for potential future UI features. The crate appears unmaintained (last release Feb 2024) but ACL APIs are stable and don't change. Our usage is best-effort with graceful fallback — if `exacl` ever breaks, files still copy, they just lose ACLs. MIT licensed (compatible with BSL).

**Decision**: On macOS, use `copyfile(3)` only for same-APFS-volume copies; use chunked copy for everything else.
**Why**: The only practical benefit of `copyfile(3)` is APFS clonefile (instant copy-on-write, zero extra disk usage),
which only works on the same APFS volume. We evaluated `copyfile` on other filesystems:
- **HFS+**: No clonefile. Marginal metadata edge (birthtime, file flags), but HFS+ is rare since Apple converted all
  Macs to APFS in 2017.
- **exFAT / FAT32**: No clonefile, no xattrs, no ACLs, no file flags — the metadata `copyfile` would preserve doesn't
  exist on these filesystems. No practical benefit.
- **NTFS-3G**: FUSE-based, so `copyfile` goes through userspace with the same I/O buffering issues as network mounts.
  `COPYFILE_QUIT` is unreliable. No benefit.
- **Network mounts (SMB, NFS, AFP, WebDAV)**: `copyfile` ignores `COPYFILE_QUIT` while draining buffered I/O, causing
  cancellation to take 30+ seconds or complete the copy entirely. This applies when *either* the source or destination
  is on a network mount (for example, NAS-to-local copies).
- **USB / external drives**: Typically exFAT or HFS+ — no clonefile. Different volume from the internal drive, so no
  same-volume benefits.

Our chunked copy (1 MB read/write chunks) provides: identical speed for non-clonefile copies, reliable cancellation
between chunks, and granular progress callbacks. It preserves xattrs (including resource forks), ACLs, timestamps, and
permissions. The only metadata it doesn't preserve is birthtime (creation date) and file flags (`chflags`), which
matter only on same-volume copies where we use `copyfile` anyway. Detection uses `st_dev` (device ID) for same-volume
and `statfs.f_fstypename` for APFS. See `copy_strategy.rs` for the implementation.

## Gotchas

**Gotcha**: On macOS, never use `statvfs` alone for disk space checks — use `NSURLVolumeAvailableCapacityForImportantUsageKey`
**Why**: `statvfs` reports only physically free blocks. On APFS, purgeable space (iCloud caches, APFS snapshots) can account for tens of GB that macOS will reclaim on demand. Using `statvfs` causes the "insufficient space" error to reject copies that would actually succeed, and shows a different available-space number than the status bar (which uses the NSURL API). `validate_disk_space` in `helpers.rs` calls `crate::volumes::get_volume_space()` on macOS and falls back to `statvfs` on Linux.

## Dependencies

- `crate::file_system::volume` — `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `volume_copy`)
- `crate::ignore_poison` — `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs, sync), `xattr`, `exacl`, `filetime` (metadata preservation in `chunked_copy`)
