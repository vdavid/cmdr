# Rollback after a multi-file copy with Overwrite resolutions deletes the new copies but doesn't restore the originals

**Severity:** medium
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:623-674` (`safe_overwrite_file`, step 4)
`apps/desktop/src-tauri/src/file_system/write_operations/state.rs:646-656` (`CopyTransaction::rollback`)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy.rs:865-877` (record into transaction)

## What
`safe_overwrite_file` does temp+backup+rename and then deletes the backup as step 4, so the only durable evidence of the user's pre-overwrite file is gone once a single overwrite completes. The freshly-written file path is recorded in `CopyTransaction::created_files`. If a LATER file in the same operation fails (or the user requests Rollback after multiple Overwrites have already succeeded), `CopyTransaction::rollback` just calls `fs::remove_file(file)` on every recorded path — removing the new copies — with no provision to put the originals back. Net effect: a "Rollback" UX promise that's a partial truth. New copies disappear (correct), and the originals the user agreed to overwrite are also gone (wrong; the user expected "undo").

## Why it matters
User selects 50 files to copy to a folder containing 50 same-named files, picks "Overwrite all," then 30 files in clicks Rollback because they noticed a mistake. The first 30 destination files were already overwritten — the original 30 are gone. Rollback then deletes those 30 new copies, leaving the destination with neither the user's pre-existing files NOR the copies the user wanted to roll back. The remaining 20 destination files stay intact (because no overwrite happened yet). The user's mental model is "Rollback should make it like nothing happened"; the actual behavior is "Rollback only undoes additions, not overwrites." This is the rollback-on-failure path too: a failed copy of file 31 triggers `transaction.rollback()` automatically, with the same outcome.

## Evidence
`helpers.rs:665-672`:
```rust
// Step 4: Delete backup (non-critical, ignore errors)
if backup_path.is_dir() {
    let _ = fs::remove_dir_all(&backup_path);
} else {
    let _ = fs::remove_file(&backup_path);
}

Ok(bytes)
```

`state.rs:646-656`:
```rust
pub fn rollback(&self) {
    // Delete files first (in reverse order)
    for file in self.created_files.iter().rev() {
        let _ = std::fs::remove_file(file);
    }
    // Then directories (deepest first, already in reverse due to creation order)
    for dir in self.created_dirs.iter().rev() {
        let _ = std::fs::remove_dir(dir);
    }
}
```

`copy.rs:876-878` (record site, called for both fresh-create and Overwrite branches):
```rust
transaction.record_file(actual_dest.clone());
record_file_done(&progress_ctx, source, progress_weight, files_done, bytes_done);
```
No tracking of `backup_path` is kept past `safe_overwrite_file`'s return, so rollback has nothing to restore from.

## Suggested fix
Defer the step-4 backup deletion until commit time. Track per-file `(dest, Option<backup_path>)` pairs inside `CopyTransaction` so that:
- `commit()` deletes all backups in the background (current step 4).
- `rollback()` restores each backup via `fs::rename(&backup, &dest)` before falling back to `remove_file(dest)` for files that had no pre-existing copy.

This costs disk space for the lifetime of the operation (one backup per Overwrite, deleted on commit) but gives the rollback UX promise teeth. The same shape would extend to `rollback_with_progress` for user-initiated rollback (currently runs the same `remove_file` loop).

A weaker mitigation: surface the limitation in the rollback UI ("Rollback removes newly-copied files; files that were overwritten can't be restored") so users don't expect more than the implementation delivers.

## Notes
- The transfer/CLAUDE.md `Safe overwrite: temp + backup + rename` gotcha frames the guarantee as "The original is intact until step 3 completes" — which is only true for THAT file's success path. Rollback of a successful overwrite was outside scope of the safety pattern as documented.
- This sits adjacent to two other findings: `high-A-chunked-copy-truncates-existing-dest-on-overwrite.md` (no safe-overwrite at all on most copy paths) and `high-A-volume-overwrite-deletes-dest-before-stream-success.md` (cross-volume Overwrite has the same hole pre-stream). All three are facets of the same systemic "Overwrite isn't actually reversible" gap.
