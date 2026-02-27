# Delete and trash

Delete files permanently or move them to macOS Trash, with a confirmation dialog, scan preview, and progress tracking.

## Purpose

Provides the delete/trash workflow triggered by F8 (trash) or Shift+F8 (permanent delete). Always shows a confirmation
dialog before acting. Reuses `TransferProgressDialog` for progress display.

## Files

- **DeleteDialog.svelte**: Confirmation dialog with file list (max 10 items + overflow), live scan stats, symlink
  notice, and no-trash volume warning. Uses `ModalDialog` with `role="dialog"` for trash and `role="alertdialog"` for
  permanent delete.
- **delete-dialog-utils.ts**: Pure utility functions: `generateDeleteTitle()` (handles "N selected files" vs "1 file
  under cursor"), `abbreviatePath()`, `getSymlinkNotice()`, `countSymlinks()`.
- **delete-dialog-utils.test.ts**: Vitest tests for the pure utilities.

## How delete flows

1. **Shortcut**: F8 / Cmd+Backspace (trash) or Shift+F8 / Shift+Cmd+Backspace (permanent delete)
2. **Command**: `file.delete` or `file.deletePermanently` in `command-registry.ts`, handled in `+page.svelte`
3. **Selection**: `DualPaneExplorer.openDeleteDialog(permanent)` builds props from selection or cursor item (same
   pattern as copy/move). Looks up `supportsTrash` from the source volume's `VolumeInfo`.
4. **Dialog**: `DeleteDialog` opens with file list, scan preview starts in background via `startScanPreview()`
5. **Confirm**: `dialog-state.svelte.ts::handleDeleteConfirm()` transitions to `TransferProgressDialog` with
   `operationType: 'trash'` or `'delete'`
6. **Backend**: `trash_files_start()` or `delete_files_start()` in `write_operations/mod.rs` runs the operation
7. **Progress**: `TransferProgressDialog` shows items/bytes progress with cancel support
8. **Completion**: Toast notification, both panes refreshed, 400ms minimum display time

## Key design decisions

- **Trash by default**: F8 moves to trash. Permanent delete requires explicit Shift+F8. No setting to change this.
- **Always show dialog**: No `confirmBeforeDelete` setting. Delete is destructive, so the user always sees what they're
  about to delete. Both delete settings were removed from the settings registry.
- **No undo**: Cmdr doesn't implement undo, but items trashed via `NSFileManager.trashItemAtURL` support Finder's "Put
  back" automatically.
- **`supportsTrash` detection**: Each volume exposes `supportsTrash` based on `fsType` from `statfs`. APFS/HFS+ support
  trash; FAT32, exFAT, and network filesystems (smbfs, nfs, afpfs, webdav) do not. When `supportsTrash` is false, the
  dialog forces permanent delete mode with a warning banner.
- **Scan preview integration**: The confirmation dialog starts a scan preview for deep file/dir/byte counts. For trash,
  the scan is cancelled on confirm (trashItemAtURL is atomic per top-level item, no need to wait). For permanent delete,
  the scan must complete first (the progress dialog shows scanning phase if needed).
- **400ms minimum display**: Progress dialog stays visible for at least 400ms to prevent jarring flashes on fast
  operations.
- **Cursor positioning**: After delete, cursor stays at the same position index (not same file). Fixed in
  `apply-diff.ts`: returns `Math.min(originalCursorIndex, files.length - 1)` instead of 0 when cursor file is removed.

## Backend (Rust)

- **`write_operations/trash.rs`**: `move_to_trash_sync()` (ObjC `trashItemAtURL` wrapper, reused by
  `commands/rename.rs`) and `trash_files_with_progress()` (batch trash with per-item progress and cancellation)
- **`write_operations/delete.rs`**: `delete_files_with_progress()` for permanent delete (scan, then delete files first,
  dirs deepest-first)
- **`WriteOperationType::Trash`**: Distinct variant for trash operations in all event payloads

## Edge cases

- **Dangling symlinks**: Uses `symlink_metadata()` instead of `path.exists()` for existence checks. A dangling symlink
  (target deleted) is still a valid item to trash/delete.
- **Locked files**: `trashItemAtURL` handles locked files on APFS. Permanent delete fails on locked files with a
  specific error message suggesting unlocking via Finder.
- **No-trash volumes**: Detected proactively via `supportsTrash` on the volume. Dialog forces permanent delete and shows
  warning. If `trashItemAtURL` unexpectedly fails on a "supports trash" volume, the per-item error message suggests
  using Shift+F8.
- **Partial failures**: If some items fail, the operation continues. Successful items stay deleted/trashed. Errors are
  reported via `TransferErrorDialog` after completion.
- **MCP**: `delete` tool in `tools.rs` opens the delete confirmation dialog. `delete-confirmation` dialog type added.

## Gotchas

- **TransferProgressDialog reuse**: Delete/trash use `TransferProgressDialog` with `operationType: 'delete' | 'trash'`.
  Transfer-specific props (`destinationPath`, `direction`, `conflictResolution`) are optional and hidden for delete.
- **Trash has no scan phase**: `trashItemAtURL` is atomic per top-level item, so progress tracks top-level items (not
  individual files). Byte-level progress only shows when per-item sizes are available from the scan or drive index.
- **Selection cleared after delete**: Items no longer exist, so selection is cleared. Both panes refresh (they might
  show the same directory).
