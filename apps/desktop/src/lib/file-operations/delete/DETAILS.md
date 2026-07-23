# Delete and trash details (frontend)

Depth and rationale. `CLAUDE.md` holds the must-knows; the flow and edge-case catalog live here.

## How delete flows

1. **Shortcut**: F8 (trash) or Shift+F8 (permanent delete).
2. **Command**: `file.delete` or `file.deletePermanently` in `command-registry.ts`, handled in `+page.svelte`.
3. **Selection**: `DualPaneExplorer.openDeleteDialog(permanent)` builds props from selection or cursor item (same
   pattern as copy/move). Looks up `supportsTrash` from the source volume's `VolumeInfo`.
4. **Dialog**: `DeleteDialog` opens with the file list; scan preview starts in the background via `startScanPreview()`.
5. **Confirm**: `DeleteDialog` passes back the active `isPermanent` (from the switch);
   `dialog-state.svelte.ts::handleDeleteConfirm(previewId, isPermanent)` transitions to `TransferProgressDialog` with
   `operationType: 'trash'` or `'delete'`.
6. **Backend**: `trash_files_start()` or `delete_files_start()` in `write_operations/mod.rs` runs the operation.
7. **Progress**: `TransferProgressDialog` shows items/bytes progress with cancel support.
8. **Completion**: toast notification, both panes refreshed, 400 ms minimum display time.

## Scan-preview detail

The confirmation dialog starts a scan preview for deep file/dir/byte counts and shows running tallies, the current
scanning directory, and a throughput readout from `ScanThroughput` (`../scan-throughput.ts`). For trash, the scan is
cancelled on confirm. For permanent delete, the scan must complete first (the progress dialog shows the scanning phase
if needed).

## Edge cases

- **Dangling symlinks**: `symlink_metadata()` instead of `path.exists()`. A dangling symlink (target deleted) is still a
  valid item to trash/delete.
- **Locked files**: `trashItemAtURL` handles locked files on APFS. Permanent delete fails on locked files with a message
  suggesting unlocking via Finder.
- **No-trash volumes**: detected proactively via `supportsTrash`. The dialog forces permanent mode and shows a warning.
  If `trashItemAtURL` unexpectedly fails on a "supports trash" volume, the per-item error suggests Shift+F8.
- **Partial failures**: the operation continues; successful items stay deleted/trashed. Errors are reported via
  `TransferErrorDialog` after completion.
