# Delete and trash (frontend)

Permanent delete or macOS Trash, with a confirmation dialog, scan preview, and progress (`TransferProgressDialog`).
Backend counterpart: `apps/desktop/src-tauri/src/file_system/write_operations/delete/CLAUDE.md`.

## Files

- **DeleteDialog.svelte**: the confirmation, with a file list (max 10 + overflow), live scan stats, symlink notice, a
  no-trash warning, and a `footerLeading` "Move to trash" switch that flips the operation in-dialog, hidden wherever
  permanent is forced. Role follows the mode: `dialog` for trash, `alertdialog` for permanent.
- **delete-dialog-utils.ts** (+ test): pure title, path-abbreviation, and symlink-notice helpers.
- **TrashCompleteToastContent.svelte** + **trash-undo.ts** (journal rollback, worded) + **go-to-trash.ts** (toast
  button + `file.goToTrash`).

## Must-knows

- **F8/Shift+F8 only set the INITIAL mode; the user flips it in-dialog.**
  `DualPaneExplorer.openDeleteDialog({ permanent })` reads `supportsTrash` off the source `VolumeInfo`.
- **Holding Shift over an F8 dialog upgrades it to permanent until release; Shift NEVER demotes**, and a Shift+F8 dialog
  ignores the hold. Keep `blur` clearing the hold, or a window switch strands the dialog on "Delete permanently", and ❌
  keep the `keydown`/`keyup` listeners in the CAPTURE phase: `ModalDialog`'s overlay stops keydown, so a bubble-phase
  listener never sees the hold. DETAILS § Shift-hold upgrade.
- **`data-scan-state` on `.scan-stats`** (`counting` | `done`) is the only "counting done" signal.
- **`DeleteDialog` must forward `sourceVolumeId` into `startScanPreview`**, or a non-local volume (MTP, SMB) runs the
  local-FS walker, hits path-not-found, and leaves the dialog stuck at "0 files".
- **`supportsTrash` drives the mode.** Each volume exposes it from `fsType` (statfs): APFS/HFS+ yes; FAT32, exFAT,
  smbfs, nfs, afpfs, webdav no. When false, the dialog forces permanent with a banner.
- **Online-only cloud content opens the PERMANENT delete, with its own banner** (`cloudOnlineOnly`: `'all'` or
  `'mixed'`): trashing an evicted file downloads it first, so `openDeleteDialog` asks `trashRoutingForPaths` and drops
  `supportsTrash`. ❌ That rule is Rust's (`delete/cloud_trash.rs`); an answer that never lands keeps the trash. A
  FOLDER's verdict arrives MID-SCAN (`cloudFolderMayHoldOnlineOnly` → `onlineOnlyFound`), always as `'mixed'`: the
  dialog flips live, and a confirm pressed first WAITS, then hands the dialog back, saying so. DETAILS § Cloud storage.
- **Confirm AWAITS the `startScanPreview` IPC**, so `onConfirm` never dispatches a null `previewId`: that leaves an
  ownerless concurrent walk nothing can cancel.
- **A permanent delete waits for the WALK in the BACKEND** (`scan_bridge::await_claimed_preview`), consuming the cached
  result. **Trash is the one operation that doesn't wait**: `trashItemAtURL` is atomic per top-level item, so
  `trash_files_start` frees the preview outright, and the FE renders no bar from the scan's expected totals. DETAILS §
  Scan-preview detail.
- **A trash is undoable, a delete never is.** Its toast carries Undo and "Go to trash", both needing the journaled op id
  (no id → plain sentence). ❌ Never add a permanent delete there: it shows after EVERY trash, one misclick from the one
  op no rollback reverses. "Skip confirmation" skips a TRASH dialog only
  (`../../file-explorer/pane/confirmation-skip.ts`).
- **A partly refused trash must never read as a clean success**: `refused` on the completion drops the toast to `warn`
  and raises a second naming what stayed (`composeTrashRefusedToast`). ❌ Nor a failure: what went IS trashed, and Undo
  must stay reachable.
- **The trash is PER VOLUME** (`get_trash_dir`), and revealing a trashed dotfile with hidden files off THROWS in
  `moveCursor`; keep that guarded. DETAILS § Undo and go-to-trash.
- **`TransferProgressDialog` is shared** (`operationType: 'delete' | 'trash'`); transfer-only props are optional and
  hidden, and it stays visible ≥400 ms.
- **After delete, the cursor keeps its row**, or the same position index (clamped) when that row went away
  (`pane/listing-diff-sync.svelte.ts`). Selection is cleared; both panes refresh.

## Backend touchpoints

`delete/trash.rs`, `delete/walker.rs`, `delete/cloud_trash.rs` (the routing). `WriteOperationType::Trash` is its own
event-payload variant; the MCP `delete` tool opens this dialog (`delete-confirmation`).

Depth: `DETAILS.md`.
