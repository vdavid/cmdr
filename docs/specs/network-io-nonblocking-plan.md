# Network I/O non-blocking plan

Fix two related bugs where slow network filesystem I/O (SMB, NFS, AFP) blocks the UI.

**Bug 1 — UI freeze on cancel.** Cancelling a chunked copy deletes the partial file synchronously inside
`copy_data_chunked`. On SMB, `remove_file` blocks for 30–60 seconds while the mount drains. SMB serializes all
operations on the same connection, so every other I/O to that mount (directory listings, watcher reconciliation) also
stalls. Result: rainbow spinner, entire app unresponsive.

**Bug 2 — Un-cancellable dialog on slow start.** `copy_files_start` (and `move_files_start`, `delete_files_start`,
`trash_files_start`) run validation (`validate_sources`, `validate_destination`, `validate_destination_writable`,
`validate_destination_not_inside_source`) **before** calling `start_write_operation`. These validators call `stat()`,
`exists()`, `canonicalize()`, and `libc::access()` — all blocking syscalls. On a stalled mount, the Tauri IPC handler
blocks for minutes, never returning the `operationId` the frontend needs to cancel.

## Design principle alignment

- "Blocking the UI or other actions is an absolute no-go" (design-principles.md)
- "All actions longer than ~1 second should be immediately cancelable" (design-principles.md)
- "All blocking work in `spawn_blocking`" (write_operations CLAUDE.md)

## Architecture

Two independent parts, same principle: **never do potentially-slow filesystem I/O on the path between the user's action
and the UI's response.**

### Part A — Move validation into the background task

**Intention:** The frontend must always get an `operationId` back immediately, regardless of how slow the filesystem is.
This makes every operation cancellable from the instant the dialog opens.

**Approach:** Move all validation calls from the `*_files_start` functions into the handler closures passed to
`start_write_operation`. Since `start_write_operation` runs handlers via `tokio::spawn` → `spawn_blocking`, validation
will run on the blocking thread pool instead of the async executor.

**Prerequisite: fix `start_write_operation` error handling.** Currently (mod.rs line 132), `start_write_operation` only
emits `write-error` for panics (`JoinError`). A handler returning `Err(validation_error)` is silently dropped — the
`spawn_blocking` result is `Result<Result<(), WriteOperationError>, JoinError>`, and only the outer `Err` is handled.
This must be fixed to handle `Ok(Err(...))` too:

```rust
match result {
    Ok(Ok(())) => {} // Handler already emitted write-complete or write-cancelled
    Ok(Err(ref e)) if matches!(e, WriteOperationError::Cancelled { .. }) => {
        // Handler already emitted write-cancelled
    }
    Ok(Err(e)) => {
        // Handler error — emit write-error as safety net
        let _ = app_for_error.emit("write-error", WriteErrorEvent {
            operation_id: operation_id_for_cleanup,
            operation_type,
            error: e,
        });
    }
    Err(join_error) => {
        // Panic/abort
        let _ = app_for_error.emit("write-error", WriteErrorEvent {
            operation_id: operation_id_for_cleanup,
            operation_type,
            error: WriteOperationError::IoError {
                path: String::new(),
                message: format!("Task failed: {}", join_error),
            },
        });
    }
}
```

**Why `Cancelled` is excluded:** The `*_with_progress` functions already emit `write-cancelled` before returning
`Err(Cancelled)`. Emitting `write-error` for `Cancelled` would send a conflicting event to the frontend.

**Double-emit for non-cancel errors is safe.** The `*_with_progress` functions also emit `write-error` before returning
their own errors. So `start_write_operation` may emit a second `write-error`. This is harmless: the frontend's
`handleError` calls `cleanup()` which removes all event listeners, so the second event arrives to no listener.

**Concrete backend changes (`mod.rs`):**

Each `*_files_start` function becomes a thin wrapper: log intent, call `start_write_operation`. Validation moves into
the handler closure. For example, `copy_files_start`:

```rust
pub async fn copy_files_start(
    app: tauri::AppHandle,
    sources: Vec<PathBuf>,
    destination: PathBuf,
    config: WriteOperationConfig,
) -> Result<WriteOperationStartResult, WriteOperationError> {
    log::info!("copy_files_start: sources={:?}, destination={:?}, dry_run={}", sources, destination, config.dry_run);
    start_write_operation(app, WriteOperationType::Copy, config.progress_interval_ms, move |app, op_id, state| {
        validate_sources(&sources)?;
        validate_destination(&destination)?;
        validate_destination_writable(&destination)?;
        validate_not_same_location(&sources, &destination)?;
        validate_destination_not_inside_source(&sources, &destination)?;
        copy_files_with_progress(&app, &op_id, &state, &sources, &destination, &config)
    }).await
}
```

Same pattern for `move_files_start`, `delete_files_start`, `trash_files_start`. For `delete_files_start`, also move the
`get_volume_manager().get()` call into the handler.

**Frontend:** No changes needed. `handleError` already processes `write-error` events correctly. The catch block in
`startOperation` stays for transport-level IPC failures.

**UX change:** Validation errors currently skip the progress dialog entirely (IPC throws, `onError` fires). After this
change, the dialog briefly shows "Scanning..." before the error event arrives and transitions to the error dialog. On
local FS this is a fraction of a second; on a stalled network FS, "Scanning..." is more honest than a frozen state.

### Part B — Non-blocking cancellation cleanup

**Intention:** When the user cancels, the UI must respond immediately. Background cleanup (deleting partial files) can
take arbitrarily long on network mounts and should never block the cancellation response.

**Core pattern: fire-and-forget cleanup helpers** in `helpers.rs` (alongside existing `spawn_async_sync()`):

```rust
/// Deletes a file on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_file_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}

/// Deletes a directory tree on a detached thread. Returns immediately. Best-effort.
pub(super) fn remove_dir_all_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = std::fs::remove_dir_all(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}
```

**Change 1: `chunked_copy.rs` — partial file cleanup on cancel.**

```rust
// Current (blocks on SMB for 30-60s):
drop(dst_file);
let _ = std::fs::remove_file(dest);

// New (returns immediately):
drop(dst_file);
remove_file_in_background(dest.to_path_buf());
```

Note: the cancelled file is NOT in `CopyTransaction.created_files` — `record_file` is called after successful copy
(copy.rs line 532), and the cancelled file's copy failed. So `remove_file_in_background` and `rollback_in_background`
(below) target disjoint file sets. No double-delete coordination needed.

**Change 2: `CopyTransaction::rollback_in_background` in `state.rs`.**

```rust
/// Rolls back on a detached thread. Returns immediately.
/// Use for user-initiated cancel where the calling thread must not block.
/// Best-effort: if the background thread fails, files remain on disk.
pub fn rollback_in_background(mut self) {
    let files = std::mem::take(&mut self.created_files);
    let dirs = std::mem::take(&mut self.created_dirs);
    self.committed = true; // Prevent Drop from synchronous double-rollback
    if files.is_empty() && dirs.is_empty() {
        return;
    }
    log::info!("rollback_in_background: cleaning up {} files and {} dirs", files.len(), dirs.len());
    std::thread::spawn(move || {
        for file in files.iter().rev() {
            if let Err(e) = std::fs::remove_file(file) {
                log::warn!("rollback: failed to remove {}: {}", file.display(), e);
            }
        }
        for dir in dirs.iter().rev() {
            let _ = std::fs::remove_dir(dir);
        }
    });
}
```

`committed = true` is the safety mechanism preventing `Drop` from synchronous rollback. `mem::take` moves ownership into
the thread. Both are needed. The synchronous `rollback()` stays for non-cancellation error paths.

**Change 3: `copy.rs` — use `rollback_in_background` on cancel.**

```rust
if skip_rollback {
    transaction.commit();
} else {
    transaction.rollback_in_background();
}
```

**Change 4: `move_op.rs` — non-blocking staging cleanup on failure.**

Two `remove_dir_all` calls on the staging directory block on network destinations:

- **Line 279** (copy phase fails): change to `remove_dir_all_in_background(staging_dir.clone())`.
- **Line 354** (rename phase fails): change to `remove_dir_all_in_background(staging_dir)`.

Line 370 (success path, empty staging dir) stays synchronous — empty dir removal is near-instant.

**Change 5: Fix `test_chunked_copy_cancellation` test.**

Remove `assert!(!dst.exists())` (cleanup is now async). Add a comment explaining why. The test still validates the
`Cancelled` error return, which is its actual purpose.

## Out of scope

**Scan phase blocking on stalled mounts.** `scan_sources` → `walk_dir_recursive` does `readdir`/`symlink_metadata` per
entry. On a stalled mount, individual I/O calls can block for minutes. Could be addressed with `run_cancellable`
wrappers but is a separate, larger change.

**`delete_sources_after_move` blocking on stalled source mount.** Does `remove_dir_all`/`remove_file` during normal
operation. The cancellation flag is checked between sources, so the user can cancel between files, but a single stalled
`remove_dir_all` can block. Separate concern.

## Milestones

### Milestone 1 — Part A: validation in background

1. Fix `start_write_operation` in `mod.rs` to emit `write-error` for `Ok(Err(non-cancelled))` handler results.
2. Move validation into handler closures for all four `*_files_start` functions. For `delete_files_start`, also move
   `get_volume_manager().get()`.
3. Run `./scripts/check.sh --check clippy --check rustfmt`.
4. Run tests: `cd apps/desktop/src-tauri && cargo nextest run`.
5. Manual test: copy with invalid destination → dialog opens briefly, error dialog appears via event.

### Milestone 2 — Part B: non-blocking cancel cleanup

1. Add `remove_file_in_background` and `remove_dir_all_in_background` to `helpers.rs`.
2. Add `rollback_in_background` to `CopyTransaction` in `state.rs`.
3. Change `chunked_copy.rs` `copy_data_chunked` to use `remove_file_in_background`.
4. Change `copy.rs` cancellation branch to use `rollback_in_background`.
5. Change `move_op.rs` lines 279 and 354 to use `remove_dir_all_in_background`.
6. Fix `test_chunked_copy_cancellation`: remove file-existence assertion, add comment.
7. Run `./scripts/check.sh --check clippy --check rustfmt`.
8. Run tests: `cd apps/desktop/src-tauri && cargo nextest run`.

### Milestone 3 — Docs and final checks

1. Update `write_operations/CLAUDE.md`:
    - Document async rollback pattern (`rollback_in_background` vs `rollback`).
    - Add gotcha: background cleanup is best-effort; files may remain if mount disconnects.
    - Note that `start_write_operation` now emits `write-error` for handler errors.
2. Run full `./scripts/check.sh`.
3. Manual test on NAS/SMB:
    - Copy NAS → local, cancel mid-copy → dialog closes instantly.
    - Start copy from NAS while mount is stalled → dialog opens, is cancellable.
    - Copy with invalid source → error dialog appears promptly.
    - Cross-FS move with cancel → staging directory cleaned up in background.
