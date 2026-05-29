# Cross-volume Overwrite deletes destination file before the streaming copy starts

**Severity:** high
**Lens:** A — Data safety
**Confidence:** high

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_conflict.rs:261-316` (`apply_volume_conflict_resolution`)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/volume_copy.rs:1064-1067` (the write site that lands the new copy)

## What
On any cross-volume Overwrite (Local↔SMB, Local↔MTP, MTP↔MTP, etc.), `apply_volume_conflict_resolution` calls `dest_volume.delete(dest_path)` to remove the existing destination file BEFORE the streaming writer starts. If `delete` fails it is logged at `warn` and execution continues anyway. The actual `copy_single_path` runs after this, opening a fresh writer at `dest_path`. There is no temp+rename safety net here — no `dest.cmdr-tmp-<uuid>`, no backup. If the source read or the destination write fails mid-stream (network drop, USB disconnect, source-side I/O error, user cancel), the user has lost the original AND has no new file (only a partial `.cmdr-tmp` left by the backend's own writer abort, which is then explicitly cleaned up by the post-loop partial-cleanup branch in `volume_copy.rs:1228-1241`).

## Why it matters
A user picks "Overwrite" to update a 3 GB video on their NAS or MTP-mounted phone. Halfway through the copy the network blips or the USB cable is jostled. Pre-fix on the local-FS APFS path the user would still have the original (safe_overwrite_file's backup); on the cross-volume path the original was already deleted at step one and the partial is cleaned up by `delete_volume_path_recursive(&dest_volume, partial_path)` in the post-loop branch. Net result: the user has lost data they explicitly chose to overwrite, but only because the copy didn't actually complete. The Overwrite semantic should be "replace on success," not "delete now, hope the copy succeeds."

This is amplified by the comment at line 300-301 of `volume_conflict.rs` ("Continue: the streaming writer might still succeed if the failure was transient"): if `delete` reports failure, the code keeps going and tries to write into a file that may or may not exist anymore. If it does still exist (transient `delete` failure), the streaming write either fails with EEXIST-style errors depending on the backend or silently appends/overwrites in ways the user didn't sign up for.

## Evidence
`volume_conflict.rs:274-304`:
```rust
ConflictResolution::Overwrite => {
    // Cmdr's UX promise is "Overwrite means merge for dirs, replace for files":
    //
    // - For files: delete the dest first so the streaming writer lands a fresh copy.
    //   ...
    let is_dir = dest_volume.is_directory(dest_path).await.unwrap_or(false);
    if !is_dir && let Err(e) = dest_volume.delete(dest_path).await {
        log::warn!(
            "apply_volume_conflict_resolution(Overwrite): delete of file {} failed: {}",
            dest_path.display(),
            e
        );
        // Continue: the streaming writer might still succeed if the failure
        // was transient.
    }
    Ok(Some(dest_path.to_path_buf()))
}
```

`volume_copy.rs:1051-1067` (the write site):
```rust
match copy_single_path(
    &source_volume, &source_path, ...,
    &dest_volume, &dest_item_path,
    &state, &on_file_progress, &on_file_complete,
).await {
    Ok(bytes_copied) => {
        copied_paths.lock_ignore_poison().push(dest_item_path);
        ...
```

By the time `copy_single_path` runs against `dest_item_path`, the original file is already gone, regardless of whether the stream succeeds.

## Suggested fix
Have `apply_volume_conflict_resolution(Overwrite)` return a target path of `dest_path.with_extension(...".cmdr-tmp-<uuid>")` and remember the original. After `copy_single_path` returns Ok, atomically rename the original to `.cmdr-backup-<uuid>`, rename the temp into place, delete the backup. On stream failure: delete the temp and leave the original untouched. This mirrors the local-FS `safe_overwrite_file` flow but routed through the `Volume::rename` trait method (all current backends — LocalPosix, SMB, MTP, InMemory — implement `rename`). For backends where atomic rename isn't possible (cross-share SMB?), fall back to the current shape with a logged warning so the trade-off is observable.

A cheaper interim mitigation: when `dest_volume.delete(...)` fails, abort the operation with `WriteOperationError::DestinationExists` rather than continuing — the comment "the streaming writer might still succeed" is wishful thinking that papers over the data-loss path.

## Notes
- `transfer/CLAUDE.md` § "Overwrite means merge for dirs, replace for files" documents the user-facing semantic but doesn't mention the safety gap on cross-volume.
- The post-loop partial-cleanup branch (`volume_copy.rs:1219-1241`) deletes any in-flight `.cmdr-tmp`-style partial via `delete_volume_path_recursive` on Stop/error; that doesn't help here because the partial was the actual `dest_path`, not a temp.
- The dir-merge path is correctly safe (skips the delete entirely; pinned by `dir_overwrite_must_merge_not_replace_even_with_recursive_delete`). The file-replace path is the gap.
