# Auto-rename placeholder leaks as a stray 0-byte file when a later validation step fails

**Severity:** low
**Lens:** A — Data safety
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:749-764` (`find_unique_name` reserves the placeholder) → `apps/desktop/src-tauri/src/file_system/write_operations/transfer/copy.rs:884-906` (`copy_single_item`).

## What
When a conflict resolves to Rename, `find_unique_name` creates a 0-byte `name (N)` placeholder on disk (the TOCTOU reservation) and returns it as `actual_dest`. In `copy_single_item`, `validate_path_length(&actual_dest)` then runs. If it returns `Err` (name/path too long — plausible because the `(N)` suffix grows the name toward the 255-byte / 1024-byte limits), the function returns before recording or consuming the placeholder, leaking a real-looking empty `name (N)` file at the destination. The transaction never recorded it, so rollback won't remove it.

## Why it matters
Not data loss, but a stray 0-byte file the user didn't ask for, surviving even a rolled-back or failed copy. It doesn't carry the recognizable `.cmdr-` prefix (it's a plausible `name (1)`), so it's mildly confusing rather than obviously-Cmdr-debris. Edge-triggered by long names.

## Evidence
```rust
// resolution Rename arm: find_unique_name creates a 0-byte placeholder on disk
ConflictResolution::Rename => {
    let unique_path = find_unique_name(dest_path);
    Ok(Some(ResolvedDestination { path: unique_path, needs_safe_overwrite: true }))
}
// copy.rs, regular-file branch:
let (actual_dest, needs_safe_overwrite) = ...;  // actual_dest == reserved placeholder
validate_path_length(&actual_dest)?;            // Err here ⇒ placeholder left on disk, unrecorded
```

## Suggested fix
Move `validate_path_length` ahead of the conflict-resolution / reservation step (validate the intended `dest_path` first), or have the Rename arm clean up the placeholder if any post-reservation step in `copy_single_item` returns `Err` before the write lands. Simplest: validate the candidate name length inside `find_unique_name`'s loop and skip over-long candidates, so the reserved name is always valid.

## Notes
Medium confidence: depends on `validate_path_length` running after reservation in the current control flow; confirm the exact ordering in `copy_single_item` before fixing.
