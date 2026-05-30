# Cross-volume move source cleanup swallows per-child delete errors, surfacing a misleading failure

**Severity:** low
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_copy.rs:1488-1527` (`delete_volume_path_recursive`), invoked from `transfer/volume_move.rs:491-495`.

## What
After a cross-volume directory move's copy phase succeeds, the source tree is deleted via `delete_volume_path_recursive`. Per-child file and subdir delete failures are only `log::warn!`'d and swallowed; only the final `volume.delete(path)` on the top dir propagates, and it will itself fail ENOTEMPTY if any child survived. This is not data loss (the data is durable at the destination), but it leaves orphaned source files and surfaces as a confusing post-success error.

## Why it matters
Moving a folder off an SMB share where one file is briefly locked / permission-denied: every file gets copied to the destination, most source files get deleted, the locked one is logged-and-skipped, then the top-dir delete fails ENOTEMPTY and the whole move reports a generic error to the user — even though the copy fully succeeded. The user can't tell whether their data is safe at the destination or whether the move half-failed.

## Evidence
```rust
// volume_copy.rs:1509-1526
} else if let Err(e) = volume.delete(&child_path).await {
    log::warn!("delete_volume_path_recursive: failed to delete file {}: {:?}",
        child_path.display(), e);          // ← swallowed; child survives
}
// ...
volume.delete(path).await                  // ← then fails ENOTEMPTY, propagates a misleading error
```

## Suggested fix
Aggregate per-child delete failures and surface them as a distinct, non-fatal "moved, but couldn't remove N source items" outcome rather than letting the symptom appear as a generic ENOTEMPTY on the parent. At minimum, when child deletes failed, skip the parent `delete(path)` (guaranteed to fail) and report the partial-cleanup state explicitly so the user knows the destination copy is complete and only source cleanup was incomplete.

## Notes
The recursive source-delete itself is documented (transfer CLAUDE.md § "Cross-volume move source-delete is recursive"); the error-swallowing + misleading-ENOTEMPTY aspect is not.
