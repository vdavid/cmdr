# Local delete: a per-file `remove_file` failure aborts the batch via `?` without emitting `write-error`, breaking the dialog close contract

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/delete/walker.rs:132-154` (file loop in `delete_files_with_progress_inner`)
`apps/desktop/src-tauri/src/file_system/write_operations/mod.rs:352-362` (the `start_write_operation` wrapper that catches the error)

## What
In the local delete loop, `fs::remove_file(&file_info.path).with_path(&file_info.path)?` uses `?` to propagate any error out of the function. The function has explicit `emit_cancelled` calls on the cancellation check (`is_cancelled` → emit + return Err(Cancelled)), but for a real I/O error (permission denied, device disconnected, file system busy), it just bails. The function does NOT emit `write-error` itself. The caller (`start_write_operation`) DOES emit `write-error` for unhandled handler errors as a safety net, so the user does see an error event — but the operation has already partially deleted files, and the failed file's state (and any files that would have been deleted after it) is left unresolved.

Looking at this more carefully: the error IS surfaced via the safety-net emit in `start_write_operation`. But the lack of an explicit `emit_cancelled` / `emit_complete` on this path means the operation status cache and the frontend dialog rely on the safety net's `write-error` emit. The frontend's progress dialog handles `write-error` correctly per the parent CLAUDE.md ("frontend's handleError removes all listeners on first receipt"). So from a user-visible standpoint, the error IS reported.

The real safety concern: after `?` propagation, no `transaction.commit()`-style finalization runs. There IS no transaction (delete has no rollback), but the partial deletion is now permanent. The user is told "delete failed at <path>" and may not realize that the 47 files BEFORE <path> in the scan order are already gone. The state.intent is still `Running`, so the settle guard fires correctly, but the dialog reports an error without acknowledging the partial success.

## Why it matters
User asks to delete a folder of 50 files. File 23 fails (locked by another process). User sees "delete failed: file23.txt: permission denied." They retry the delete; files 1-22 are already gone (the retry walks fewer files than originally selected). The user assumes the failure aborted the entire operation. They don't know the first 22 files were deleted. If those files were the user's keepers (selected by accident), they're gone with no recovery.

Per the AGENTS.md principle "Protect the user's data" and the `delete/CLAUDE.md` note "Not rollbackable. Once deleted, data is gone," this is the expected behavior — but the error event should communicate the partial-deletion state, not just the failing path.

## Evidence
`walker.rs:148-164`:
```rust
let progress_bytes = file_info.progress_bytes;

fs::remove_file(&file_info.path).with_path(&file_info.path)?;
//                                                            ⚠ Direct ? propagation; no
//                                                              emit_error with the count of
//                                                              already-deleted files.

files_done += 1;
bytes_done += progress_bytes;

if let Some(source_path) = tracker.record(file_info) {
    events.emit_source_item_done(WriteSourceItemDoneEvent { ... });
}
```

`mod.rs:115` documents the safety-net behavior:
```
// `start_write_operation` emits `write-error` for handler errors.
```

But the emitted error carries `WriteOperationError::IoError { path, message }` from `with_path`, with no `files_processed` count.

## Suggested fix
Before propagating the error, emit a `write-error` with structured partial-progress info: extend `WriteErrorEvent` with `files_processed: usize` and `bytes_processed: u64` (similar to `WriteCancelledEvent`). Replace the bare `?` with:

```rust
if let Err(e) = fs::remove_file(&file_info.path).with_path(&file_info.path) {
    events.emit_error(WriteErrorEvent::new_with_progress(
        operation_id.to_string(),
        WriteOperationType::Delete,
        e.clone(),
        files_done,
        bytes_done,
    ));
    return Err(e);
}
```

The FE then renders "Delete failed at file23.txt; 22 files were already deleted before this failure" instead of "Delete failed at file23.txt." Users understand the actual state and can decide whether to clean up the rest or restore from a Time Machine snapshot.

Same pattern applies to:
- The directory-deletion loop right below (`for dir in scan_result.dirs.iter().rev()` — currently `let _ =` silently swallows failures, which is even worse for visibility but less safety-critical because the files-first ordering means the failing dir is benign).
- The volume-delete equivalents at `walker.rs:803-826`.

## Notes
- AGENTS.md "communicate what's actually happening" applies directly.
- Related: the trash partial-failure finding (`medium-A-batch-trash-partial-failure-silently-emits-complete.md`) is the inverse — trash communicates SUCCESS while partial-failing; delete here communicates FAILURE while partial-succeeding. Both are facets of "we don't model partial outcomes in the event protocol."
