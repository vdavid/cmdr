# MTP file operations plan

## Intention

MTP browsing and copy work today. Delete, rename, and move do not — the Rust backend (`MtpVolume` trait impl +
`MtpConnectionManager` mutation ops) and TypeScript IPC wrappers are complete, but the UI operation flows bypass
the Volume abstraction and call raw `std::fs` functions, which fail on MTP virtual paths.

The goal is to wire the existing backend through the existing UI flows with minimal new code. We're NOT building new
IPC commands — we're routing existing operations through the Volume trait where they currently go straight to `std::fs`.

## Current state

| Operation        | Works on MTP? | Why not                                                                                |
|------------------|---------------|----------------------------------------------------------------------------------------|
| Browse           | Yes           | Routes through `MtpVolume::list_directory`                                             |
| Copy (F5)        | Yes           | Routes through `copyBetweenVolumes` → Volume trait                                     |
| New folder (F7)  | Yes           | `create_directory` command uses `VolumeManager` → `MtpVolume::create_directory`        |
| Delete (F8)      | No            | `delete_files` calls `std::fs::remove_file` — bypasses Volume trait                    |
| Rename (F2)      | No            | `rename_file` calls `std::fs::rename` — bypasses Volume trait                          |
| Move (F6)        | No            | Explicit UI guard blocks it; `move_files` also uses `std::fs`                          |
| Clipboard Cmd+CV | No            | Fundamentally incompatible — system clipboard stores local file paths via NSPasteboard  |

## Key decisions

### Delete must use full progress UI

The current delete uses `TransferProgressDialog` with operation IDs, `write-progress` events, cancellation, and
item-by-item reporting. MTP delete must integrate with this same infrastructure — no simplified modal.

The challenge: the existing `delete_files_with_progress` scans via `walkdir` (can't walk MTP) and deletes via
`fs::remove_file` (can't touch MTP files). We need a **volume-aware delete variant** that:
1. Scans recursively via `volume.list_directory()` instead of `walkdir`
2. Deletes leaf-first via `volume.delete()` per individual file/dir instead of `fs::remove_file`/`fs::remove_dir`
3. Emits the same `write-progress`, `write-complete`, `write-cancelled` events with the same `operationId` contract

This reuses the `WriteOperationState` and event infrastructure from `write_operations/` — it's a new function
alongside `delete_files_with_progress`, not a replacement.

Note: `MtpVolume::delete()` currently does recursive deletion internally (children-first in `mutation_ops.rs`). For
progress integration, we need per-file granularity, so we should NOT call `volume.delete()` on a directory and let
it recurse internally. Instead: list the tree, build a flat file list (deepest-first), delete individually, and
report progress per item. The Volume trait's `delete()` on a single file is non-recursive and fine for this.

### Move uses copy-delete-copy-delete interleaving

Cross-volume move = copy + delete per file, interleaved. After each file's copy succeeds, immediately delete the
source. This is safer than copy-all-then-delete-all: partial failure leaves fewer duplicates and the user can see
what completed. Slower on MTP (each operation is a separate USB transaction), but correctness matters more than speed
for destructive operations.

### Clipboard stays out of scope

`copyFilesToClipboard` writes local filesystem paths to macOS `NSPasteboard`. MTP files have no local paths — they're
virtual. Implementing an internal (in-app) clipboard would lose inter-app interop, which is the whole point. F5/F6
already provide the same functionality. Keep the guards, just improve the toast messages to suggest F5/F6.

## Approach

### 1. Delete (F8 / Shift+F8)

**Problem:** `openDeleteDialog` opens the dialog for any volume. The confirm path goes through
`TransferProgressDialog.dispatchOperation()` → `deleteFiles()` → Rust `delete_files` → `scan_sources` (walkdir) →
`fs::remove_file`. Neither walkdir nor `fs::remove_file` work on MTP.

**Fix — new Rust function `delete_volume_files_with_progress`:**
- Lives in `write_operations/delete.rs` alongside the existing function.
- Accepts `Arc<dyn Volume>` + source paths + the usual `WriteOperationState` + `AppHandle`.
- **Scan phase:** Recursively enumerate the MTP tree via `volume.list_directory()`. Build a flat list of
  `(path, size, is_dir)` tuples, directories-last (deepest-first for deletion order). Emit `scan-progress` events
  matching the existing contract.
- **Delete phase:** Iterate the flat list. For files, call `volume.delete(path)` per item. For directories (in
  reverse/deepest-first order), call `volume.delete(path)`. Emit `write-progress` events with the same
  `WriteProgressEvent` shape. Check `state.cancelled` between items for cancellation support.
- **Completion:** Emit `write-complete` or `write-cancelled` as appropriate.
- The existing `delete_files_with_progress` stays unchanged for local files.

**Fix — routing in `delete_files_start` (mod.rs):**
- `delete_files` command already has `sources` — add `volume_id: Option<String>`.
- In `delete_files_start`, if `volume_id` is `Some` and non-default, resolve the volume via `VolumeManager` and call
  `delete_volume_files_with_progress`. Otherwise, call the existing `delete_files_with_progress`.

**Fix — frontend threading:**
- `write-operations.ts::deleteFiles`: add `volumeId` to signature, forward to `invoke`.
- `TransferProgressDialog.svelte::dispatchOperation`: pass `sourceVolumeId` to `deleteFiles()`.
- `dialog-state.svelte.ts`: already receives `sourceVolumeId` — thread it through to `TransferProgressDialog` props.
- No changes to `TransferProgressDialog`'s progress polling — the events are identical.

**Trash:** MTP devices have no trash. `supportsTrash` is already `false` for MTP volumes. The dialog already shows
"Delete permanently." Verify this end-to-end.

### 2. Rename (F2)

**Problem:** `startRename` opens the inline editor. On confirm, `rename-operations.ts::performRename` calls
`renameFile(from, to, force)` → Rust `rename_file` → `std::fs::rename`. The permission check
(`checkRenamePermission` uses `std::fs::symlink_metadata`) also fails on MTP paths.

**Fix — backend:**
- In `commands/rename.rs::rename_file`, accept an optional `volume_id: Option<String>`.
- When set and non-default, resolve the volume and call `volume.rename(from, to, force)`.
  `MtpVolume::rename()` already handles force (checks existence, calls `rename_object`).
- Skip `checkRenamePermission` for non-default volumes (MTP has its own permission model).

**Fix — frontend:**
- `rename-operations.ts::performRename`: accept `volumeId`, pass to `renameFile`.
- `tauri-commands/rename.ts::renameFile`: add `volumeId` param, forward to `invoke`.
- `FilePane.svelte` (or wherever `performRename` is called): pass the current pane's volume ID.
- Skip `checkRenamePermission` and `checkRenameValidity` calls for MTP volumes on the frontend side too (they use
  `symlink_metadata` which fails on MTP paths).

**Edge case — rename across directories:** MTP `rename_object` only changes the name within the same parent. The
inline rename UI only changes the filename (same parent), so this can't happen — but add a backend guard just in case.

### 3. Move (F6)

**Problem:** Explicit guard in `DualPaneExplorer.svelte` (~line 1486) blocks move when source or dest is MTP. Even
without the guard, `move_files` uses `std::fs::rename` or copy+delete via `std::fs`.

**Fix — three cases:**

1. **MTP-to-same-MTP (same device + storage):** Use `moveMtpObject` (the TS wrapper for `move_mtp_object`). This
   calls `storage.move_object()` which is a single MTP operation. Note: not all devices support `MoveObject` — the
   backend returns an error if unsupported. Surface it clearly: "This device doesn't support moving files. You can
   use copy instead."
2. **MTP ↔ local or MTP ↔ different MTP:** Interleaved copy-delete per file. For each source file: copy via
   `copyBetweenVolumes` (already works), then delete the source using volume-aware delete from milestone 1. This
   ensures partial failure leaves minimal duplicates.
3. **Local-to-local:** Keep existing behavior unchanged.

**Fix — frontend:**
- Remove the MTP guard in `openTransferDialog`.
- In `TransferProgressDialog.dispatchOperation()`, for `operationType === 'move'`:
  - If same MTP volume: call `moveMtpObject` directly (with simple progress per item).
  - If cross-volume (either side is MTP): interleaved copy+delete per file.
  - Otherwise: existing `moveFiles` path.
- Show copy progress, then per-file "Removing source..." updates.

### 4. Polish

**Clipboard toast improvements:**
- Copy: "Use F5 to copy files from MTP devices"
- Cut: "Use F6 to move files from MTP devices"
- Paste: "Use F5 to copy files to MTP devices"

**Fix `sourceVolumeId` bug in paste:**
`pasteFromClipboard` (line ~1604) sets `sourceVolumeId: destVolId` — the source volume is set to the *destination*
pane's volume. This is wrong for any cross-volume paste. Fix: store the source volume ID alongside paths in the
clipboard/paste flow. (The system clipboard itself doesn't carry volume IDs, but we can infer it from the paths, or
store it in an in-memory side channel alongside the `isCut` flag.)

**Update stale CLAUDE.md:**
`apps/desktop/src/lib/mtp/CLAUDE.md` says "No Volume trait integration (yet)" — this is outdated. Volume trait
integration exists for browsing, `create_directory`, and (after this work) delete/rename. Update it.

## Implementation order

### Milestone 1: Delete (F8)
- Write `delete_volume_files_with_progress` in `write_operations/delete.rs`
  - Recursive scan via `volume.list_directory()`
  - Per-file delete with progress events, cancellation
- Add `volume_id` param to Rust `delete_files` command, route to new function for non-default volumes
- Thread `sourceVolumeId` through the TS delete flow (`dialog-state` → `TransferProgressDialog` → `deleteFiles`)
- Verify `supportsTrash: false` works correctly for MTP
- Test on a real MTP device
- Run clippy, rustfmt, svelte checks

### Milestone 2: Rename (F2)
- Add `volume_id` param to Rust `rename_file` command
- Route through `volume.rename()` for non-default volumes, skip permission check
- Skip `checkRenamePermission`/`checkRenameValidity` for MTP on frontend
- Thread `volumeId` through the TS rename flow
- Test on a real MTP device
- Run checks

### Milestone 3: Move (F6)
- Remove the MTP move guard
- Same-volume MTP move via `moveMtpObject`
- Cross-volume move as interleaved copy+delete per file
- Handle `MoveObject` unsupported error gracefully
- Test on a real MTP device
- Run checks

### Milestone 4: Polish
- Update clipboard toast messages to suggest F5/F6
- Fix `sourceVolumeId` bug in `pasteFromClipboard`
- Update `apps/desktop/src/lib/mtp/CLAUDE.md`
- Run full check suite

## Size estimate

- ~120–150 lines Rust (`delete_volume_files_with_progress` + routing in `delete_files`/`rename_file`)
- ~100–150 lines TypeScript (threading `volumeId`, move dispatch logic with interleaved copy+delete)
- ~30 lines toast/clipboard/CLAUDE.md fixes
- Total: ~300–350 lines changed across ~12 files

## Risks

- **MTP delete is slow per item.** Each `volume.delete()` is a USB round-trip. Deleting 500 photos will be noticeably
  slower than local delete. The progress UI helps — the user sees it's working. But we should test with a realistic
  workload to see if the UX is acceptable.
- **MTP move not universally supported.** Some devices don't support `MoveObject`. Backend already errors. Need clear
  user-facing messaging.
- **Device disconnection mid-operation.** The MTP connection manager handles disconnections with errors. Ensure these
  bubble up as user-friendly messages per our style guide.
- **Testing.** MTP operations need a real device. Can't be tested in CI. Manual testing required per milestone.

## Non-goals

- Granular byte-level progress for MTP delete (we report per-file, not per-byte — MTP delete is instant per file)
- Clipboard support for MTP (fundamentally requires local paths for OS interop)
- Drag-and-drop for MTP (separate feature)
- MTP device write-protection detection (the device itself rejects writes)
