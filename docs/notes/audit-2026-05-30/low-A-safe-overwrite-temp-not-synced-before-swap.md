# `safe_overwrite_file` (Linux/std path) deletes the original aside before the new content is proven durable

**Severity:** low
**Lens:** A — Data safety
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:845-902`; Linux/std copy strategy via `transfer/copy_strategy.rs:185-223`.

## What
`safe_overwrite_file` copies source → temp (step 1), renames the original aside (step 2), renames temp → dest (step 3), deletes the aside (step 4). The original is recoverable from the `.cmdr-temp-` aside until step 3 — good. But on Linux/non-macOS the step-1 copy is `std::fs::copy`, whose bytes are not fsynced before step 3 renames the temp into place, and step 4 deletes the aside immediately after. The end-of-op `flush_created_destinations` pass does eventually fdatasync the final dest, so for the local-FS copy engine the window is between step-3 rename and that end-of-op pass.

## Why it matters
Overwriting a file on a power-loss-prone external drive on Linux: the original is gone (aside deleted at step 4), the replacement's directory entry exists, but its data may not be on platters yet if a crash hits before the end-of-op flush. Narrow window, local-FS-only, and partially mitigated by the end-of-op pass — hence low. macOS clonefile/chunked paths are unaffected (chunked syncs per file; clonefile is CoW).

## Evidence
```rust
// step 1 (non-macOS): no sync
let bytes = fs::copy(source, &temp_path)...?;
// step 2: original renamed aside
fs::rename(dest, &aside_path) ...
// step 3: temp -> dest
fs::rename(&temp_path, dest) ...
// step 4: aside deleted immediately (original now unrecoverable); temp data not yet fsynced
```

## Suggested fix
`sync_data` the temp file handle before the step-3 rename on the std / `copy_file_range` paths, so the rename-into-place commits durable data before the original aside is deleted. Cheapest correct change; macOS paths need no change.

## Notes
Cmdr's primary copy/move durability story is sound and well-documented; this is a narrow ordering edge on the Linux overwrite path specifically. Related to the high-severity volume-path durability gap (a different code path).
