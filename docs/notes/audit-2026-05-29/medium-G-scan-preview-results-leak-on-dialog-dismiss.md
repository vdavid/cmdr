# SCAN_PREVIEW_RESULTS leaks when a confirmation dialog completes its scan and is dismissed

**Severity:** medium
**Lens:** G — Resource hygiene
**Confidence:** high

## Location

- `apps/desktop/src-tauri/src/file_system/write_operations/state.rs:492-497` (`SCAN_PREVIEW_STATE`, `SCAN_PREVIEW_RESULTS` statics)
- `apps/desktop/src-tauri/src/file_system/write_operations/scan_preview.rs:182-200` (local insert on complete)
- `apps/desktop/src-tauri/src/file_system/write_operations/scan_preview.rs:325-335` (volume insert on complete)
- `apps/desktop/src-tauri/src/file_system/write_operations/scan.rs:454` (`take_cached_scan_result` is the only consumer / remover for completed entries during op start)
- `apps/desktop/src/lib/file-operations/delete/DeleteDialog.svelte:168-189` (cleanup gate: `isScanning` only)

## What

`SCAN_PREVIEW_RESULTS` is a process-global `RwLock<HashMap<String, CachedScanResult>>` that the scan-preview pipeline populates when a scan finishes (`scan-preview-complete`). It is freed only by `take_cached_scan_result(preview_id)` which runs when the matching `copy_files_start` / `move_files_start` / `delete_files_start` consumes the cache. The Tauri command `cancel_scan_preview` only sets the cancellation flag and removes the entry from `SCAN_PREVIEW_STATE`; it never touches `SCAN_PREVIEW_RESULTS`.

The FE (`DeleteDialog.svelte` cleanup path, `TransferDialog` follows the same pattern) only calls `cancelScanPreview` while `isScanning === true`. After `scan-preview-complete` arrives, `isScanning` flips to `false`. If the user closes the dialog at that point (Escape, OS title bar, click outside, app close), neither `cancelScanPreview` nor the operation-start consumer runs, so the `CachedScanResult` stays in the HashMap until process exit.

## Why it matters

`CachedScanResult` carries `Vec<FileInfo>` (one entry per scanned file with full path + size + metadata) plus `Vec<PathBuf>` for directories plus `per_path: Vec<(PathBuf, CopyScanResult)>` for volume scans. For a deep tree scan (think `Pictures/` on a home folder), that's tens-of-thousands of allocations and many MB of strings kept alive for the rest of the app session.

Repro a user can hit normally:
1. Select a 10k-file directory.
2. Press F8 (trash) — scan starts and completes.
3. Read the dialog summary, decide "not now", press Escape.
4. `cancelScanPreview` is skipped (`isScanning === false`), `CachedScanResult` stays.
5. Repeat ~10 times through the session — bytes accumulate linearly.

There is no upper bound, no TTL, and no eviction policy.

## Evidence

`scan_preview.rs::run_scan_preview` (local path):
```rust
if let Ok(mut cache) = SCAN_PREVIEW_RESULTS.write() {
    cache.insert(
        preview_id.clone(),
        CachedScanResult { files, dirs, file_count, total_bytes, per_path: Vec::new() },
    );
}
```
The volume path at line 328 inserts the same way. Neither path schedules an eviction.

`scan_preview.rs::cancel_scan_preview`:
```rust
pub fn cancel_scan_preview(preview_id: &str) {
    if let Ok(cache) = SCAN_PREVIEW_STATE.read()
        && let Some(state) = cache.get(preview_id)
    {
        state.cancelled.store(true, Ordering::Relaxed);
    }
}
```
Does not touch `SCAN_PREVIEW_RESULTS`. (`cancel` only matters mid-scan; entries published at complete-time are never removed by cancel.)

`DeleteDialog.svelte`:
```svelte
onDestroy(() => {
    if (previewId && isScanning) {
        void cancelScanPreview(previewId)
    }
    cleanup()
})
```
Gated by `isScanning`. After `scan-preview-complete` fires (`isScanning` set to `false` in the complete handler), this branch is dead.

## Suggested fix

Decouple the eviction from the cancel path. Two simple options that compose:

1. **FE-side: always release the cache slot on dialog dismiss.** Track `previewId` independently of `isScanning` and call a new `release_scan_preview(preview_id)` IPC (or extend `cancel_scan_preview` to always evict from `SCAN_PREVIEW_RESULTS` too) on every dialog teardown. Keep the existing "only set the cancel flag mid-scan" semantics for `SCAN_PREVIEW_STATE`.
2. **BE-side safety net: TTL or cap.** On insert, stash the timestamp; on next insert, evict entries older than ~5 min (or cap the map at, say, 8 entries with LRU). Even if the FE always releases, this guards against a future caller that forgets.

The minimal fix is option 1 (semantic: "dialog gone => preview cache gone"). Option 2 is the belt-and-suspenders backstop that the rest of the write-ops state machine already favors elsewhere (timeouts, settle guards).

## Notes

- The same pattern exists in `TransferDialog`'s scan flow — same cleanup gate, same leak shape.
- Each leaked entry's size scales with the scanned subtree; not constant. A user who opens a delete dialog over `~/Library` and dismisses it lands ~hundreds of MB in `SCAN_PREVIEW_RESULTS` until process exit.
- Not a security issue; just RSS bloat over a long session.
