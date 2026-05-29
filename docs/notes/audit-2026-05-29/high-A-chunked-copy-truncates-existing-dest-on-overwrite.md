# Chunked copy truncates existing destination before reading source, defeating safe-overwrite contract

**Severity:** high
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy_strategy.rs:108-158` (strategy dispatch)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/chunked_copy.rs:100-173` (`copy_data_chunked`)

## What
When a copy resolves a conflict as Overwrite, the strategy dispatcher (`copy_file_with_strategy`) is supposed to take the safe-overwrite path (temp+backup+rename) so the original file survives a mid-copy failure. On macOS this only happens for same-APFS-volume copies; for any other case (non-APFS local FS, USB exFAT/HFS+, network mounts) the strategy dispatcher routes to `chunked_copy_with_metadata`, which calls `std::fs::File::create(dest)`. That truncates the existing destination immediately — before a single source byte has been read. The same hole exists on Linux for network filesystems (`is_network_filesystem(...) → chunked_copy_with_metadata`, the `needs_safe_overwrite` arm is dead in that branch).

## Why it matters
A user copies a new version of a file (say, a 4 GB video edit) over the existing file on a USB drive or an SMB share, picks Overwrite at the conflict prompt, and the copy fails halfway through — power loss, USB yanked, SMB session dropped, mid-stream cancel. The pre-existing file at the destination was truncated to zero in the first millisecond of the operation, the streaming write only got partway, and the original is irretrievable. The destination now holds a corrupt partial file. This contradicts AGENTS.md's stated principle ("Use safe overwrite patterns like temp+rename") and the docstring on `safe_overwrite_file` ("If any step fails before step 3 completes, the original dest is intact"). The safe-overwrite path only protects same-APFS-volume copies; everything else silently bypasses it.

## Evidence
`copy_strategy.rs:108-136` (macOS):
```rust
if is_same_apfs_volume(source, dest) {
    let context = CopyProgressContext::with_cancellation(Arc::clone(cancelled));
    if needs_safe_overwrite {
        safe_overwrite_file(source, dest, Some(&context))
    } else {
        copy_single_file_native(source, dest, false, Some(&context))
    }
} else {
    // ⚠ needs_safe_overwrite is IGNORED here.
    chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
}
```

`copy_strategy.rs:145-158` (Linux):
```rust
if is_network_filesystem(source) || is_network_filesystem(dest) {
    // ⚠ needs_safe_overwrite is IGNORED here too.
    chunked_copy_with_metadata(source, dest, cancelled, progress_callback)
} else if needs_safe_overwrite {
    safe_overwrite_file(source, dest)
} else {
    copy_single_file_linux(source, dest, false, cancelled, progress_callback)
}
```

`chunked_copy.rs:112-115` inside `copy_data_chunked`:
```rust
let mut dst_file = std::fs::File::create(dest).map_err(|e| WriteOperationError::WriteError {
    path: dest.display().to_string(),
    message: format!("Failed to create destination file: {}", e),
})?;
```
`File::create` opens with `O_TRUNC`, so the user's existing file is wiped before the first `src_file.read(...)` runs.

## Suggested fix
Route `needs_safe_overwrite` through `chunked_copy_with_metadata`: when true, write to `dest.cmdr-tmp-<uuid>` instead, then rename the original to `dest.cmdr-backup-<uuid>`, rename temp into place, delete backup — the same pattern `safe_overwrite_file` already uses for the `fs::copy` path. Either teach `chunked_copy_with_metadata` to accept a `needs_safe_overwrite` parameter and do the staging itself (so the chunked I/O still happens, just to a temp file the strategy renames on success), or have `safe_overwrite_file` delegate its body to `chunked_copy_with_metadata` instead of `fs::copy` whenever the dispatcher picked the chunked path. The cancellation cleanup path in `copy_data_chunked` (`remove_file_in_background(dest)`) becomes safer too: cleaning up the temp leaves the original untouched.

## Notes
- The transfer/CLAUDE.md "Safe overwrite: temp + backup + rename" gotcha documents the intent. The code's actual coverage is narrower than that documentation implies.
- This is also the most common real-world path on macOS: any copy to a USB drive, external SSD, NAS-mounted share, or non-APFS internal partition hits this branch.
- Related: even when `safe_overwrite_file` does run (APFS clonefile path), a subsequent failure of a later file in the same operation triggers `transaction.rollback()` which just calls `fs::remove_file(dest)` — but the backup is already gone (step 4), so the user's original is unrecoverable. See `medium-A-overwrite-rollback-leaves-no-original.md`.
