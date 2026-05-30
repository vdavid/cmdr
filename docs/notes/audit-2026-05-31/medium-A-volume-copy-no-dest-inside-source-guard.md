# No "destination inside source" guard on the volume copy path — copying a folder into its own descendant can recurse unboundedly

**Severity:** medium **Lens:** A — Data safety **Confidence:** medium

## Location

Missing guard at the volume entry points: `apps/desktop/src-tauri/src/commands/file_system/volume_copy.rs:16-48`
(`copy_between_volumes`) and the ops path `file_system/write_operations/transfer/volume_copy.rs` →
`copy_directory_streaming` in `transfer/volume_strategy.rs:129-193`. Compare the protected local path:
`file_system/write_operations/mod.rs:207` and `:241` call `validate_destination_not_inside_source`.

## What

The local `copy_files_start` / `move_files_start` handlers validate that the destination is not inside a source
directory (`validate_destination_not_inside_source`, which canonicalizes and fails closed — `helpers.rs:138-165`). The
cross-volume / non-local-same-volume path (`copy_between_volumes` → `copy_volumes_with_progress` → `copy_single_path` →
`copy_directory_streaming`) has no equivalent check. `copy_directory_streaming` re-lists each subdirectory live on every
recursion frame, so when the destination is inside the source subtree on the **same** volume (e.g. same-SMB or same-MTP
copy of folder `A` into `A/sub`), the files it writes become new entries that get re-listed and re-copied — unbounded
recursion that grows the tree until the share/device fills or the operation is killed.

## Why it matters

On a same-share SMB or same-device MTP copy, a user drags folder `A` into `A/sub/` (easy to do by accident in a
dual-pane manager pointed at two locations on the same share). The local fast path rejects this outright; the volume
path instead recurses, writing an ever-deeper `A/sub/A/sub/A/...` until the share fills, leaving a deeply nested partial
mess the user then has to clean up. Same-volume _move_ is incidentally protected because the OS `rename` rejects moving
a directory into its own child, but _copy_ has no such backstop.

## Evidence

Local path is guarded:

```rust
// mod.rs:202 (copy_files_start handler closure)
validate_sources(&sources)?;
validate_destination(&destination)?;
validate_destination_writable(&destination)?;
validate_not_same_location(&sources, &destination)?;
validate_destination_not_inside_source(&sources, &destination)?;  // <- closed
```

Volume command entry has no such validation before dispatching:

```rust
// commands/file_system/volume_copy.rs:16
pub async fn copy_between_volumes(
    app: tauri::AppHandle, source_volume: String, source_paths: Vec<String>,
    dest_volume: String, dest_path: String, config: WriteOperationConfig,
) -> Result<...> {
    // ...resolves volumes... then:
    ops_copy_between_volumes(app, source_volume, source_paths, dest_volume, dest_path, config).await
}
```

The recursion re-lists live each frame:

```rust
// transfer/volume_strategy.rs (copy_directory_streaming)
let entries = source_volume.list_directory(source_path, None).await?;
for entry in &entries {
    let child_dest = dest_path.join(&entry.name);
    if entry.is_directory {
        total_bytes += Box::pin(copy_directory_streaming(
            source_volume, &child_source, dest_volume, &child_dest, /* ... */)).await?;
```

## Suggested fix

Add a dest-inside-source check at the `copy_between_volumes` / `move_between_volumes` ops entry (or in
`copy_volumes_with_progress` before the per-source loop): when `source_volume == dest_volume`, reject when the resolved
`dest_path` starts with any source path that is a directory, returning `WriteOperationError::DestinationInsideSource`
(the variant already exists, `types.rs:465`). For genuinely cross-device copies the case can't arise (different path
spaces), so the guard only needs to fire on the same-volume branch. Cheap, and brings the volume path to parity with the
local guard's contract.

## Notes

- The runaway behavior depends on backend `list_directory` semantics while writes are in flight (the exact growth
  pattern differs between SMB and MTP), which is why confidence is medium rather than high — but the _missing guard_ is
  certain, and "copy a folder into its own child" is a classic data-integrity footgun that the local path already treats
  as must-reject.
- Not applicable to the both-local path (already guarded via the `copy_files_start` delegation).
