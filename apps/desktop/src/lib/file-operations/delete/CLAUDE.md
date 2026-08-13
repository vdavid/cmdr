# Delete and trash (frontend)

Delete files permanently or move them to macOS Trash, with a confirmation dialog, scan preview, and progress tracking.
Triggered by F8 (trash) or Shift+F8 (permanent). Always shows a confirmation dialog; reuses `TransferProgressDialog` for
progress. Backend counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md`.

## Files

- **DeleteDialog.svelte**: confirmation dialog with file list (max 10 + overflow), live scan stats, symlink notice,
  no-trash warning, and a "Move to trash" switch in the footer row (`ModalDialog`'s `footerLeading`) that flips the
  operation in-dialog, hidden on no-trash volumes where permanent is forced. Uses `ModalDialog` with `role="dialog"` for
  trash, `role="alertdialog"` for permanent; the role flips reactively with the switch.
- **delete-dialog-utils.ts** (+ test): pure utilities `generateDeleteTitle()`, `abbreviatePath()`, `getSymlinkNotice()`,
  `countSymlinks()`.

## Must-knows

- **F8/Shift+F8 just set the initial mode; the user can flip it in-dialog.** F8 preselects trash, Shift+F8 preselects
  permanent. `file.delete` / `file.deletePermanently` commands; `DualPaneExplorer.openDeleteDialog(permanent)` builds
  props from selection or cursor and looks up `supportsTrash` from the source `VolumeInfo`.
- **`data-scan-state` on `.scan-stats`** (`counting` | `done`) is the only "counting done" signal; there's no completion
  checkmark. Mirrors `TransferDialog`'s marker, which E2E polls.
- **`DeleteDialog` must forward `sourceVolumeId` into `startScanPreview`.** Without it, an MTP delete runs
  `walk_dir_recursive` on `/DCIM/Camera`, hits path-not-found, and silently leaves the dialog stuck at "0 files".
  Non-local volumes (MTP, SMB) must route through `run_volume_scan_preview`, not the local-FS walker.
- **`supportsTrash` drives the mode.** Each volume exposes it from `fsType` (statfs): APFS/HFS+ yes; FAT32, exFAT,
  smbfs, nfs, afpfs, webdav no. When false, the dialog forces permanent mode with a warning banner.
- **Confirm AWAITS the `startScanPreview` IPC**, so `onConfirm` never dispatches a null `previewId`. A null id gives the
  operation nothing to claim, so it re-walks the tree concurrently with the preview `startScan` already began, and that
  orphan has no owner and nothing to cancel it (teardown's cleanup is gated on `!confirmed`). The IPC only mints an id
  and spawns the walk, so it answers promptly even on a wedged share. `TransferDialog` awaits its own `scanStarted` for
  the same three reasons.
- **A permanent delete then waits for the WALK, in the backend, not here** (`scan_bridge::await_claimed_preview`), and
  consumes the cached result rather than re-walking. **Trash is the one operation that doesn't wait**: `trashItemAtURL`
  is atomic per top-level item, so it consumes nothing and `trash_files_start` frees the preview outright rather than
  leaving an ownerless walk running. Scan events still carry index-derived `expectedFilesTotal`/`expectedBytesTotal`,
  but the FE no longer renders a progress bar from them (it read as "already deleting" during scan).
- **No undo, no `confirmBeforeDelete` setting.** Delete is destructive so the dialog always shows; both delete settings
  were removed from the registry. Items trashed via `NSFileManager.trashItemAtURL` support Finder's "Put back".
- **`TransferProgressDialog` is shared** (`operationType: 'delete' | 'trash'`); transfer-only props (`destinationPath`,
  `direction`, `conflictResolution`) are optional and hidden. Progress dialog stays visible ≥400 ms to avoid flashes.
- **After delete, cursor stays at the same position index** (not the same file): `apply-diff.ts` returns
  `Math.min(originalCursorIndex, files.length - 1)`. Selection is cleared (items gone); both panes refresh.
- **Existence checks use `symlink_metadata()`, not `path.exists()`** so a dangling symlink is still a valid item to
  trash/delete.

## Backend touchpoints

`write_operations/delete/trash.rs` (`move_to_trash_sync`, `trash_files_with_progress`) and
`write_operations/delete/walker.rs` (`delete_files_with_progress`). `WriteOperationType::Trash` is a distinct variant in
event payloads. MCP `delete` tool opens this confirmation dialog (`delete-confirmation` dialog type).

Full details (the full F8→completion flow, partial-failure and locked-file handling): `DETAILS.md`.
