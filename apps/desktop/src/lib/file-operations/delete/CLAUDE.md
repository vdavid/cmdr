# Delete and trash (frontend)

Delete files permanently or move them to macOS Trash, with a confirmation dialog, scan preview, and progress tracking
(via `TransferProgressDialog`). Backend counterpart:
`apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md`.

## Files

- **DeleteDialog.svelte**: confirmation dialog with file list (max 10 + overflow), live scan stats, symlink notice,
  no-trash warning, and a "Move to trash" switch in the footer row (`ModalDialog`'s `footerLeading`) that flips the
  operation in-dialog, hidden on no-trash volumes where permanent is forced. The `ModalDialog` role flips with the mode:
  `dialog` for trash, `alertdialog` for permanent.
- **delete-dialog-utils.ts** (+ test): pure utilities `generateDeleteTitle()`, `abbreviatePath()`, `getSymlinkNotice()`,
  `countSymlinks()`.
- **TrashCompleteToastContent.svelte** + **trash-undo.ts** (journal rollback, worded) + **go-to-trash.ts** (the toast
  button and the `file.goToTrash` command).

## Must-knows

- **F8/Shift+F8 only set the INITIAL mode; the user flips it in-dialog.**
  `DualPaneExplorer.openDeleteDialog({ permanent })` builds props from selection or cursor and reads `supportsTrash` off
  the source `VolumeInfo`.
- **Holding Shift over an F8 dialog upgrades it to permanent until release; Shift NEVER demotes**, and a Shift+F8 dialog
  ignores the hold. Keep `blur` clearing the hold, or a window switch strands the dialog on "Delete permanently". ❌
  Keep the `keydown`/`keyup` listeners in the CAPTURE phase: `ModalDialog`'s overlay stops keydown, so a bubble-phase
  `window` listener never sees the hold. DETAILS § Shift-hold upgrade.
- **`data-scan-state` on `.scan-stats`** (`counting` | `done`) is the only "counting done" signal; there's no completion
  checkmark. Mirrors `TransferDialog`'s marker, which E2E polls.
- **`DeleteDialog` must forward `sourceVolumeId` into `startScanPreview`**, or a non-local volume (MTP, SMB) runs the
  local-FS walker, hits path-not-found, and leaves the dialog stuck at "0 files".
- **`supportsTrash` drives the mode.** Each volume exposes it from `fsType` (statfs): APFS/HFS+ yes; FAT32, exFAT,
  smbfs, nfs, afpfs, webdav no. When false, the dialog forces permanent mode with a warning banner.
- **Confirm AWAITS the `startScanPreview` IPC**, so `onConfirm` never dispatches a null `previewId`: a null id leaves an
  ownerless concurrent walk nothing can cancel. `TransferDialog` awaits its own `scanStarted` for the same reasons.
  DETAILS § Scan-preview detail.
- **A permanent delete waits for the WALK in the BACKEND** (`scan_bridge::await_claimed_preview`), consuming the cached
  result. **Trash is the one operation that doesn't wait**: `trashItemAtURL` is atomic per top-level item, so
  `trash_files_start` frees the preview outright. The FE renders no bar from the scan's expected totals (it read as
  "already deleting"). DETAILS § Scan-preview detail.
- **A trash is undoable, a delete never is.** Its toast carries Undo and "Go to trash", both needing the journaled op id
  (no id → plain sentence). ❌ Never add a permanent delete there: it shows after EVERY trash, one misclick from the one
  op no rollback reverses. No `confirmBeforeDelete` setting; the dialog always shows.
- **The trash is PER VOLUME** (`get_trash_dir`), and revealing a trashed dotfile with hidden files off THROWS in
  `moveCursor`. Keep that guarded. DETAILS § Undo and go-to-trash.
- **`TransferProgressDialog` is shared** (`operationType: 'delete' | 'trash'`); transfer-only props (`destinationPath`,
  `direction`, `conflictResolution`) are optional and hidden. Progress dialog stays visible ≥400 ms to avoid flashes.
- **After delete, the cursor keeps its row**, falling back to the same position index (clamped) when that row is the one
  that went away (`pane/listing-diff-sync.svelte.ts`). Selection is cleared; both panes refresh.
- **Existence checks use `symlink_metadata()`, not `path.exists()`** so a dangling symlink is still a valid item to
  trash/delete.

## Backend touchpoints

`write_operations/delete/trash.rs` (`move_to_trash_sync`, `trash_files_with_progress`) and
`write_operations/delete/walker.rs` (`delete_files_with_progress`). `WriteOperationType::Trash` is a distinct variant in
event payloads. MCP `delete` tool opens this confirmation dialog (`delete-confirmation` dialog type).

Full details (the full F8→completion flow, partial-failure and locked-file handling): `DETAILS.md`.
