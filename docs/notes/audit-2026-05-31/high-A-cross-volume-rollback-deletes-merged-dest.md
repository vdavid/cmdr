# Cross-volume copy rollback recursively deletes a merged destination directory, destroying pre-existing dest-only files

**Severity:** high
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_copy.rs:1142-1163` (record path) and `:1420-1432` (rollback delete) and `:1488-1499` (`delete_volume_path_recursive`)

## What
On the cross-volume / non-local-same-volume copy path, a **directory** source records the *top-level destination directory path* (`dest_item_path`) in `copied_paths`, not the individual files it created. When the user clicks **Rollback** (not Cancel), `volume_rollback_with_progress` iterates `copied_paths` and calls `delete_volume_path_recursive` on each entry — which deletes the entire directory tree. If the copy merged into a pre-existing destination directory of the same name (Cmdr's documented "Overwrite means merge for dirs" semantics), rollback recursively deletes the whole merged tree, including files that existed in the destination before the operation and were never created by it.

The local-FS copy path does not have this bug: `CopyTransaction` records individual `created_files`, so its rollback only removes what it actually wrote.

## Why it matters
A user has `~/photos/2024/` on an SMB share containing 500 existing files. They copy a local folder `2024/` (10 new files) into the share, choosing Overwrite/merge at the conflict prompt. Part-way through they click **Rollback**, expecting "undo the 10 files I just added." Instead, `delete_volume_path_recursive` deletes the top-level `~/photos/2024/` tree on the share — all 510 files, including the 500 the user never touched this session. This is silent loss of untouched user data on the one operation explicitly advertised as the safe "undo."

## Evidence
Record site — for a directory source, `landed_path` is the top-level dest directory (`dest_item_path`):
```rust
// volume_copy.rs:1142
let landed_path = match replace_after_write {
    Some(orig) => { /* file→file safe-replace finalize */ orig }
    None => dest_item_path,   // <- top-level dest DIRECTORY for a directory source
};
// Record the landed path (the original after a safe-replace, else the dest)
// for rollback; never the temp.
copied_paths.lock_ignore_poison().push(landed_path);
```
Rollback site — each recorded path is deleted recursively:
```rust
// volume_copy.rs:1420
for path in copied_paths.iter().rev() {
    if load_intent(&state.intent) == OperationIntent::Stopped { return false; }
    // Each copied path may be a file or a directory tree, so delete recursively
    if let Err(e) = delete_volume_path_recursive(volume, path).await { /* warn */ }
```
```rust
// volume_copy.rs:1488 — directory branch lists + deletes the whole subtree
pub(super) async fn delete_volume_path_recursive(volume: &Arc<dyn Volume>, path: &Path) -> Result<(), VolumeError> {
    let is_dir = match volume.is_directory(path).await { Ok(true) => true, ... };
    if !is_dir { return volume.delete(path).await; }
    // ...lists children and deletes children before parents...
```

## Suggested fix
Rollback must remove only what the operation created. For a directory source that merged into an existing destination, either (a) record the per-file destinations actually written (mirror local `CopyTransaction.created_files`) and delete only those, pruning now-empty dirs, or (b) detect at record time whether the destination directory pre-existed (the `create_directory` call in `copy_directory_streaming` returned `AlreadyExists`) and, when it did, never recursively delete that directory on rollback — fall back to per-file deletion of the files this op wrote into it. Add a `volume_copy_tests` case: pre-populate the dest dir with a unique sentinel file, run copy-with-merge then rollback, assert the sentinel survives.

## Notes
- `transfer/CLAUDE.md` documents rollback as "delete all files copied so far in reverse order" — that's the intent; the defect is **granularity** (it records the directory root, not the files). The same doc's "Overwrite means merge for dirs" section establishes that dest-only files legitimately coexist in a merged directory, which is exactly what rollback then destroys.
- Only **Rollback** triggers this. The **Cancel** (keep-partials) path does not delete the directory, so it's unaffected.
- Same code path is shared by Local↔SMB, Local↔MTP, MTP↔MTP, and same-SMB/same-MTP copies. Genuinely cross-device merges are common (importing into an existing share folder), which is what makes this reachable in normal use.
