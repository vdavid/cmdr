# Cross-FS local move deletes source originals before the final destination's rename-into-place is fsynced

**Severity:** low **Lens:** A — Data safety **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:701-733` (`move_with_staging`: Phase 4
source delete → Phase 5 → `flush_created_destinations`)

## What

In `move_with_staging`, Phase 2 copies to a `.cmdr-staging-<uuid>` dir (chunked copy `sync_data`s each file's **data**),
Phase 3 renames staging → final, Phase 4 (`delete_sources_after_move`) deletes the source originals, and only
_afterward_ does `flush_created_destinations` fsync the final files and their **parent directories** (making the
rename-into-place durable). So the source is removed before the directory entry of the final destination is fsynced.

## Why it matters

On power loss in the window between Phase 4 and the parent-directory fsync, the file _data_ is durable (synced in
Phase 2) but the Phase-3 rename-into-place may not yet be on disk. After crash recovery the file could be absent from
its final path while the source is already gone — recoverable only as orphaned data blocks or a leftover
`.cmdr-staging-*` entry, not at either expected name. The "complete means you can eject" promise is upheld at
completion; the issue is purely the _ordering_: deleting the only other copy before the final directory entry is durable
widens the crash window compared with fsyncing the final dests first.

## Evidence

```rust
// move_op.rs (move_with_staging)
// Phase 4: Delete source files (only after successful copy+rename)
delete_sources_after_move(events, operation_id, state, sources, files_done, &skipped_source_paths)?;
// Phase 5: Remove empty staging directory
let _ = fs::remove_dir(&staging_dir);
// Durability: flush the final per-file dests BEFORE reporting complete.
// (runs AFTER the source delete above)
flush_created_destinations(events, operation_id, WriteOperationType::Move, state,
    files_done, scan_result.file_count, bytes_done, scan_result.total_bytes,
    &final_dests, &final_already_synced);
```

## Suggested fix

Move `flush_created_destinations` (or at minimum the parent-directory fsync of the final destinations) to run _before_
Phase 4's source deletion, so the rename-into-place is durable on disk before the only other copy of the data is
removed. This matches the cross-volume move's stated invariant ("a move must never delete the source if the destination
isn't fully in place") and costs nothing in the happy path (the files were already data-synced in Phase 2; this just
reorders the dir-entry fsync ahead of the delete).

## Notes

- Narrow window, low probability: requires power loss in a sub-second gap on a filesystem where the Phase-2 data sync
  didn't already flush the directory entry. Worst case is a recoverable-but-misplaced file, not silent total loss —
  hence low / medium-confidence.
- `transfer/CLAUDE.md` § Durability documents the per-file `sync_data` + end-of-op flush, but doesn't address the
  delete-vs-final-dir-fsync ordering specifically.
