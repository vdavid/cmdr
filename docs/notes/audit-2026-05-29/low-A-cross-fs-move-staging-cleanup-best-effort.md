# Cross-FS move staging directory cleanup is fire-and-forget on a detached thread; leftover staging may collide on retry

**Severity:** low
**Lens:** A — Data safety
**Confidence:** medium

## Location
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:454-456` (copy-phase failure)
`apps/desktop/src-tauri/src/file_system/write_operations/transfer/move_op.rs:530-532` (rename-phase failure)
`apps/desktop/src-tauri/src/file_system/write_operations/helpers.rs:343-348` (`remove_dir_all_in_background`)

## What
The cross-filesystem move pipeline (`move_with_staging`) creates a staging directory at `destination/.cmdr-staging-<operation_id>`. On any failure (copy phase, rename phase), cleanup of the staging directory is delegated to `remove_dir_all_in_background`, which spawns a detached `std::thread` and returns immediately. If the cleanup thread fails (network mount disconnected, ENOENT race, app exits before it runs), the staging dir is left behind with whatever it contains: every file copied so far is still on disk under `.cmdr-staging-<uuid>/`. Per the doc on `remove_*_in_background`: "If the network mount disconnects or the app exits, partial files or staging directories may remain on disk." That's documented and intentional.

The data-safety angle: the staged data is essentially a duplicate of the user's source files. They still exist on the source side (phase 4 source deletion runs only after the rename phase succeeds). So no data is lost. But the staging dir occupies the destination volume's space, and a future move operation targeting the same destination directory with the SAME `operation_id` (collision is astronomically unlikely with UUIDs, but the doc names this with `operation_id` which is the user-visible operation handle) would either fail to create the staging dir (since it already exists) or worse, write into the leftover one.

## Why it matters
- For data SAFETY proper: nothing is lost.
- For disk-budget safety: a failed multi-GB move leaves multi-GB of orphan files on the destination volume until the user manually discovers them. On a destination that's tight on space (the user's reason for moving in the first place), this can repeat-fill the disk. The next move attempt's pre-flight disk-space check (`validate_disk_space`) reads "available" without knowing about the leftover `.cmdr-staging-*` dirs; it under-reports the actual usable headroom.
- For UX: leftover `.cmdr-staging-<uuid>` directories accumulate silently in the user's destination folders. They're hidden by the `.cmdr-` prefix recognition (helpful for cleanup tooling) but visible in any third-party explorer.

## Evidence
`move_op.rs:454-456`:
```rust
if let Err(e) = copy_result {
    // Cleanup staging directory in background (may block on network mounts)
    remove_dir_all_in_background(staging_dir.clone());
    events.emit_error(WriteErrorEvent::new(
        operation_id.to_string(),
        WriteOperationType::Move,
        e.clone(),
    ));
    return Err(e);
}
```

`helpers.rs:343-348`:
```rust
pub(super) fn remove_dir_all_in_background(path: PathBuf) {
    std::thread::spawn(move || {
        if let Err(e) = fs::remove_dir_all(&path) {
            log::warn!("background cleanup: failed to remove {}: {}", path.display(), e);
        }
    });
}
```

## Suggested fix
On app startup, scan recent destination directories (or implement an opportunistic sweep) for `.cmdr-staging-*` and `.cmdr-tmp-*` directories older than some threshold (24 h?) and delete them. The `.cmdr-` prefix is the recognizability hook the doc names; lean into it for automated reaping.

Alternatively, log a one-line warning to the user on operation failure: "Cleanup of staging dir <path> is happening in the background; if it fails check the warning logs." A more invasive fix would be to make staging cleanup synchronous on failure and force the user to wait (with a spinner) — but that contradicts the "background" pattern the doc deliberately chose.

This is a low-severity finding because:
- The trade-off is explicitly documented in `transfer/CLAUDE.md` § "Background cleanup is best-effort"
- No data is at risk
- The `.cmdr-` prefix already provides a recovery hook

Filing primarily so the cleanup-sweeper idea has a place to live.

## Notes
- The same pattern applies to `.cmdr-tmp-` / `.cmdr-backup-` files left behind by failed `safe_overwrite_file` cleanups.
- For the copy path, similar leftovers happen at `volume_copy.rs:1228-1241` (partial cleanup of in-flight tasks); those use the volume's recursive delete, which respects volume teardown — slightly better than `remove_dir_all_in_background`.
