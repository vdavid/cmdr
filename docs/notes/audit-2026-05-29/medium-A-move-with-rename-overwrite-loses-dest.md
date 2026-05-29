# Same-FS move with Overwrite clobbers destination atomically with no backup, breaking multi-source rollback

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:163-166` (`move_with_rename`, conflict-resolved rename)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:289-291` (`merge_move_directory`, conflict-resolved rename)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:42-54` (`MoveTransaction::rollback`)

## What
When the user picks Overwrite during a same-FS move and the destination already exists, `move_with_rename` calls `fs::rename(source, &resolved.path)` directly. `fs::rename` over an existing file is atomic and unconditionally clobbers the destination — no `safe_overwrite_file`-style temp+backup+rename pattern, even though the move path's transaction (`MoveTransaction`) records `(original_source, moved_to_dest)` pairs and runs an `fs::rename(dest, original_source)` rollback on cancel. The rollback inverts the move (puts the source back) but the original destination file is gone forever; rollback ends with the source restored and the original destination file destroyed.

## Why it matters
User moves a folder of edited photos over the older versions in their main library, picks "Overwrite all," then sees a later file fail (permission error on an extended attribute, ENOSPC, etc.). The operation rolls back: the photos that were already moved-with-overwrite get renamed back to their source folder. The library's original photos at the destination locations were silently clobbered by the atomic renames; rollback does nothing about that. The user ends with: original library photos gone, current-edit photos back at the source location, destination locations either empty (rollback removed them) or holding the overwriting edits (rollback hasn't reached them yet). No combination of these states is recoverable.

This is also the failure mode for any move that succeeds-then-cancels-then-rollbacks: each Overwrite-resolved rename destroyed a destination file that the rollback cannot recreate.

## Evidence
`move_op.rs:152-176`:
```rust
} else if path_exists_or_is_symlink(&dest_path) {
    // File-to-file (or type mismatch) conflict
    match resolve_conflict(...)? {
        Some(resolved) => {
            fs::rename(source, &resolved.path).with_path(source)?;
            //  ⚠ Atomic rename over existing dest. The pre-existing file at
            //    resolved.path is gone the moment this returns Ok.
            move_tx.record(source.clone(), resolved.path);
        }
        None => {
            files_skipped += 1;
            continue;
        }
    }
}
```

`move_op.rs:42-54` shows the rollback path doesn't even attempt to restore the destination — it just reverses the rename:
```rust
fn rollback(&self) {
    for (original_source, moved_to_dest) in self.renames.iter().rev() {
        if let Err(e) = fs::rename(moved_to_dest, original_source) {
            log::warn!(...);
        }
    }
}
```

Same pattern in `merge_move_directory:289-291` for the recursive merge case.

## Suggested fix
Before calling `fs::rename(source, &resolved.path)` on an Overwrite conflict, rename the existing destination to `resolved.path.cmdr-backup-<uuid>` (same atomic-rename guarantee on same-FS). Record the backup path on the `MoveTransaction` entry. On commit, delete backups in the background. On rollback: rename the source back, then rename the backup back to dest. On stream-style failure during the rename itself (extremely unlikely for same-FS but possible across crypto-region boundaries), restore the backup before propagating.

`move_with_staging` (the cross-FS path) doesn't have this exact hole because the renames from staging to dest are also atomic, BUT it has the same "no backup of the displaced original" property — worth fixing both paths together.

## Notes
- The same-FS move path is the most-used move shape (Cmdr's APFS-dominant target environment); this is not an edge case.
- Related to `medium-A-overwrite-rollback-leaves-no-original.md` (the copy-side version of the same gap).
- `transfer/CLAUDE.md` § "Move rollback (same-FS)" claims "Same-FS rename rollback is instant (just another rename)" — which is true but only describes the source-restore half, not the destination-restore half.
