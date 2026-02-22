# Write operations

Copy, move, and delete with streaming progress, cancellation, conflict resolution, and rollback. macOS only.

## Purpose

Implements the three destructive file operations as background tasks that stream Tauri events to the frontend. Every
operation is cancellable, reports byte-level progress, and handles edge cases: symlink loops, same-inode overwrites,
network mounts, cross-filesystem moves, and name/path length limits.

## Files

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API: `copy_files_start`, `move_files_start`, `delete_files_start`. Validates inputs, creates `WriteOperationState`, spawns `tokio::spawn` + `spawn_blocking`. |
| `types.rs` | All serializable types: events, config, errors, results. `WriteOperationConfig`, `ConflictResolution`, `WriteOperationError`, `DryRunResult`, scan preview events. |
| `state.rs` | Two `LazyLock<RwLock<HashMap>>` caches (`WRITE_OPERATION_STATE`, `OPERATION_STATUS_CACHE`). `WriteOperationState`, `CopyTransaction`, `ScanResult`, `FileInfo`. |
| `helpers.rs` | Validation (`validate_sources`, `validate_destination_writable` via `libc::access`, `validate_disk_space` via `statvfs`). Conflict resolution (condvar wait for Stop mode). `safe_overwrite_file`/`safe_overwrite_dir` (temp+rename). `find_unique_name`. `run_cancellable`. `is_same_filesystem` (device IDs). |
| `scan.rs` | `scan_sources` (recursive walk, emits progress), `dry_run_scan`, scan preview subsystem (`start_scan_preview`, `cancel_scan_preview`). |
| `copy.rs` | `copy_files_with_progress`: scan → disk space check → per-file copy via `copy_single_item`. `CopyTransaction` for rollback. |
| `move_op.rs` | Same-fs: `fs::rename`. Cross-fs: copy to `.cmdr-staging-<uuid>`, atomic rename, delete sources. |
| `delete.rs` | Scan, delete files first, then directories in reverse/deepest-first order. Not rollbackable. |
| `copy_strategy.rs` | Strategy selection per file: network FS → chunked copy; overwrite → temp+rename; otherwise → macOS `copyfile(3)`. |
| `macos_copy.rs` | FFI to macOS `copyfile(3)`. Preserves xattrs, ACLs, resource forks, Finder metadata. Supports APFS `clonefile`. |
| `chunked_copy.rs` | 1 MB chunked read/write for network mounts. Checks cancellation between chunks. Copies xattrs, ACLs, timestamps. |
| `volume_copy.rs`, `volume_conflict.rs`, `volume_strategy.rs` | Volume-to-volume copy (Local↔MTP abstraction). Publicly re-exported from `mod.rs` and at least partially wired up. |
| `tests.rs`, `integration_test.rs` | Unit and integration tests. |

## Architecture / data flow

```
Frontend
  → validate (sources exist, dest writable, not same location, dest not inside source)
  → WriteOperationState created (AtomicBool cancelled, Condvar for Stop conflicts)
  → stored in WRITE_OPERATION_STATE + OPERATION_STATUS_CACHE
  → tokio::spawn (async wrapper)
      → tokio::task::spawn_blocking (all blocking I/O here)
          → scan phase: walk_dir_recursive, emit scan-progress events
          → disk space check (statvfs)
          → execute phase: per-file copy/delete
              → throttled write-progress events (200ms default)
          → success: CopyTransaction::commit(), emit write-complete
          → cancel: CopyTransaction::rollback(), emit write-cancelled
          → error: CopyTransaction::rollback(), emit write-error
  → state removed from both caches
```

## Key patterns and gotchas

**All blocking work in `spawn_blocking`.** Never call blocking I/O on the async executor.

**Two-layer cancellation.** `AtomicBool` for fast in-loop checks. `run_cancellable` wraps blocking operations (e.g.,
network-mount copies that may block indefinitely) in a separate thread, polling the flag every 100ms via `mpsc::channel`.

**`CopyTransaction` rollback.** Records created files and dirs in creation order. Rollback deletes files in reverse order
first, then dirs in reverse order (deepest first). `commit()` just drops the vecs. Delete operations are not rollbackable.

**Symlinks never dereferenced.** All stat calls use `symlink_metadata`. Symlink loop detection uses a `HashSet<PathBuf>`
of canonicalized paths.

**Safe overwrite: temp + backup + rename.** Steps: copy source → `dest.cmdr-tmp-<uuid>`, rename dest → `dest.cmdr-backup-<uuid>`,
rename temp → dest, delete backup. The original is intact until step 3 completes.

**Stop-mode conflict resolution.** Emits `write-conflict` event, then blocks on a `Condvar` with a 300s safety timeout.
Frontend calls `resolve_write_conflict(operation_id, resolution, apply_to_all)` which stores a `ConflictResolutionResponse`
and notifies the condvar. `cancel_write_operation` also notifies the condvar to unblock.

**`skip_rollback` is stored inverted.** `cancel_write_operation(rollback: bool)` stores `!rollback` in `skip_rollback`.

**Scan preview caching.** `start_scan_preview` runs a background scan, caches the result in `SCAN_PREVIEW_RESULTS`. The
actual `copy_files_start` can consume the cache via `preview_id` in `WriteOperationConfig`, skipping a redundant scan.

**Progress throttled to 200ms.** Each operation tracks `last_progress_time` and skips emitting if under the interval.

**Temp files use `.cmdr-` prefix.** Enables recoverability (recognizable leftover files after a crash).

**Move strategy.** Same filesystem detected via device ID comparison (`MetadataExt::dev`). Cross-filesystem move uses a
`.cmdr-staging-<uuid>` dir at the destination root, then atomic `rename` into place, then source deletion.

**Copy strategy selection** (`copy_strategy.rs`):
- Destination is a network mount → `chunked_copy_with_metadata` (macOS `copyfile` ignores `COPYFILE_QUIT` on network mounts)
- Needs safe overwrite → `safe_overwrite_file`
- Otherwise → `copy_single_file_native` (macOS `copyfile(3)`, supports `COPYFILE_CLONE` for APFS instant copies)

**Special files skipped.** Sockets, FIFOs, and device files are filtered out during scan.

**`volume_copy` path is incomplete.** The three `volume_*` files are Phase 5 work, but are publicly re-exported from `mod.rs` and at least partially wired up.

## Events emitted

| Event | Trigger |
|-------|---------|
| `write-progress` | Every ~200ms during copy/move/delete |
| `write-conflict` | Stop mode hit a conflicting destination file |
| `write-complete` | Operation finished successfully |
| `write-cancelled` | Operation cancelled (includes `rolled_back` flag) |
| `write-error` | Operation failed |
| `dry-run-complete` | `config.dry_run == true` (returns `DryRunResult`) |
| `scan-preview-progress` | During `start_scan_preview` |
| `scan-preview-complete` | Preview scan finished |
| `scan-preview-error` | Preview scan failed |
| `scan-preview-cancelled` | Preview scan cancelled |

## Dependencies

- `crate::file_system::volume` — `Volume` trait, `SpaceInfo`, `ScanConflict` (used by `volume_copy`)
- `crate::ignore_poison` — `IgnorePoison` extension for `RwLock`/`Mutex` to not panic on poisoned locks
- External: `tauri` (emit, AppHandle), `uuid` (operation IDs, temp names), `libc` (access, statvfs, sync), `xattr`, `exacl`, `filetime` (metadata preservation in `chunked_copy`)
