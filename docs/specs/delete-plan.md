# Delete feature plan

## Intention

Delete is the last must-have file operation for MVP. The goal: let users delete files/folders with **full clarity** about
what they're about to destroy, while making the operation **blazing fast** to invoke. The UX should feel safer than
Finder (you see exactly what you're deleting and how much space it frees) but faster to trigger (one keypress +
Enter to confirm).

**Implementation note**: This feature will be developed by agents in one fell swoop (~30 min). The plan is designed so
that an implementing agent can pick it up and execute end-to-end without ambiguity. Every design decision includes
the "why"9 so the agent can adapt if it hits something unexpected.

## Design decisions

### Trash vs permanent delete

- **Default: move to trash.** macOS users expect `⌘⌫` to trash, not destroy. Recoverable by default.
- **No `deletePermanently` setting.** Both `fileOperations.deletePermanently` and `fileOperations.confirmBeforeDelete`
  should be **removed** from the settings registry entirely. The default is always trash. Permanent delete is available
  via Shift+F8 / Shift+`⌘⌫`. This keeps the feature simple and safe — no hidden setting that turns a common key into
  a permanent-destroy shortcut.
- **Shift+F8 / Shift+`⌘⌫`**: Force permanent delete. Visual cue changes to red/destructive.
- **Trash backend**: Extract the core `move_to_trash_sync()` logic (the `NSFileManager.trashItemAtURL` call via
  `objc2-foundation`) from `commands/rename.rs` into the new `write_operations/trash.rs` module as a reusable
  function. Then call it from both the existing `move_to_trash` command and the new `trash_files_with_progress()`.
  Why: the existing function is a `#[tauri::command]` wrapper — coupling the new batch operation to a Tauri command
  signature would be wrong. The core ObjC logic should live in the write_operations layer.
- **Symlink-safe existence check**: The existing `move_to_trash_sync()` uses `path.exists()`, which follows symlinks.
  A dangling symlink (target deleted) returns `false`, causing a spurious "doesn't exist" error even though the
  symlink file is right there. The new `trash_files_with_progress()` **must use `symlink_metadata()`** (or
  `fs::symlink_metadata(&path).is_ok()`) to check whether the path itself exists, regardless of symlink target state.
  Fix the existing `move_to_trash_sync()` too while extracting it.
- **Operation type**: Add a new `WriteOperationType::Trash` variant to distinguish trash from permanent delete in
  progress events and UI. The frontend uses `operationType` in event handlers and for display strings ("Moving to
  trash..." vs "Deleting..."), so a distinct variant avoids ambiguity.
- **No-trash volumes** (network mounts, FAT32, exFAT): Detected proactively via the `supportsTrash` field on
  `VolumeInfo` (see "Filesystem type on volumes" section). Always permanent delete. The dialog shows a prominent
  warning (see "Remote volume warning" section below).
- **Trash failure fallback**: The `supportsTrash` field on `VolumeInfo` proactively detects most no-trash volumes
  (FAT32, exFAT, network mounts) so the dialog forces permanent delete mode before the user even tries. But if
  `trashItemAtURL` unexpectedly fails on a volume where `supportsTrash` was `true`, treat it as a per-item error
  with a clear message: "Couldn't move to trash on this volume. Use Shift+F8 to delete permanently." This avoids
  silently falling back to permanent delete (which would violate the user's intent).

### No undo, but Finder "Put back" works

Cmdr does not implement its own undo for delete. However, macOS `NSFileManager.trashItemAtURL` automatically stores
the original path metadata that Finder uses for "Put back". So items trashed via Cmdr can be restored from Finder's
Trash using "Put back" as usual. This is handled automatically by the OS — no extra work needed.

### Confirmation dialog: always shown

**The delete confirmation dialog is always shown.** There is no setting to skip it. Delete is destructive and
irreversible (for permanent) or disruptive (for trash), so the user always sees exactly what they're about to delete
before confirming. This eliminates an entire class of "oops I didn't mean to delete that" accidents.

Both delete-related settings are removed from the registry (see "Trash vs permanent delete" above). One less
setting to maintain, one less footgun.

### DeleteDialog layout

A new, purpose-built dialog — **not** reusing TransferDialog. Reason: delete has no destination, no conflict
resolution, no volume picker. Sharing TransferDialog would mean hiding 60% of the UI and still having awkward props.

```
┌──────────────────────────────────────────────────────┐
│  Delete 3 selected files and 1 folder                │  ← Title: "N selected" or "1 file under cursor"
│                                                      │
│  From: ~/projects/myapp/src                          │  ← Source path (abbreviated)
│                                                      │
│  ┌──────────────────────────────────────────────┐    │
│  │ ▸ components/          324 MB   1,204 files  │    │  ← Scrollable list, max 10 rows
│  │   utils.ts               2.4 KB              │    │    Folders: recursive size + file count
│  │   index.ts               1.1 KB              │    │    Files: size only
│  │   README.md                800 B             │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  ⚠ Your selection includes 2 symlinks. Only the     │  ← Symlink notice (when applicable)
│    links themselves will be deleted, not their        │    Not shown when no symlinks
│    targets.                                          │
│                                                      │
│  327 MB / 1,207 files / 42 dirs                 ✓   │  ← Live scan stats (deep count)
│                                                      │
│  ┌──────────┐  ┌──────────────────────────────┐     │
│  │  Cancel   │  │  Move to trash               │     │  ← Primary action = trash (blue)
│  └──────────┘  └──────────────────────────────┘     │    or "Delete permanently" (red)
└──────────────────────────────────────────────────────┘
```

**Key UX details:**

- **Selection source in title**: The title makes it clear WHERE the items come from. If items are explicitly selected:
  "Delete 3 selected files and 1 folder". If acting on the cursor item (nothing selected): "Delete 1 file under
  cursor" or "Delete 1 folder under cursor". This prevents the "wait, what am I deleting?" confusion, especially
  when the cursor-as-implicit-selection kicks in. Why: the existing copy/move flow uses
  `buildTransferPropsFromCursor()` when `selectedIndices.length === 0`, and the delete flow must use the same
  pattern — but the user should always know whether they're acting on a selection or just the cursor item.
- **File list with sizes**: For each item, show name + size. For folders, show recursive size and file count. This
  answers "what exactly am I deleting?" at a glance.
- **Max 10 items visible**: The scrollable list shows up to 10 items. If the selection has more than 10 items, show
  the first 10 with a final row: "… and 14 more items". The total stats line below always shows the full picture.
  Why 10: enough to verify you selected the right things, not so many that the dialog becomes a wall of text.
- **Deep count in stats line**: The stats line at the bottom always shows the deep recursive count (total files,
  dirs, bytes) — this communicates the real impact of the operation. On indexed volumes, the drive index makes this
  instant (`recursiveSize`/`recursiveFileCount` from `FileEntry`). On non-indexed volumes, the scan preview fills
  stats in progressively (same live-counting UX as TransferDialog).
- **Primary button always enabled**: The confirm button ("Move to trash" or "Delete permanently") is always clickable,
  even while the scan is still running. What happens on confirm depends on the mode — see the "Progress" section
  below for the full flow. In short: permanent delete waits for scanning to finish (in the progress dialog), while
  trash cancels the scan and starts immediately. No separate "Skip scan" button — the primary button just works.
- **Symlink notice**: If the selection includes any symlinks (directly selected, not nested inside folders), show a
  notice: "Your selection includes N symlinks. Only the links themselves will be deleted, not their targets." For a
  single symlink: "This item is a symlink. Only the link will be deleted, not its target." Uses `color-warning` but
  is informational, not blocking.
- **Keyboard flow**: F8 → dialog opens with "Move to trash" focused → Enter confirms. Two keypresses total.

### Filesystem type on volumes (`fsType`)

The volumes module (`volumes/mod.rs`) currently has no filesystem type info. SMB-mounted shares under `/Volumes/`
appear as `AttachedVolume`, not `Network` — so checking `category === 'network'` misses real network mounts.

**Fix: expose `fsType` on every volume via `statfs`.** Use `libc::statfs` on the volume's path, read `f_fstypename`
(a C string like `"apfs"`, `"hfs"`, `"smbfs"`, `"nfs"`, `"afpfs"`, `"msdos"`, `"exfat"`, `"webdav"`). Add an
`fs_type: Option<String>` field to `LocationInfo` (Rust) and `fsType?: string` to `VolumeInfo` (TypeScript). Populate
it for every volume during `list_locations()`. This is a single `libc::statfs` call per volume — negligible cost.

Why this matters beyond delete: knowing the filesystem type is useful everywhere (copy strategy selection, volume
badges in the sidebar, explaining why operations fail on FAT32, showing "APFS" vs "HFS+" in volume info tooltips).
A reusable building block.

**Network detection** then becomes: check `fsType` against a known set of network filesystem types
(`smbfs`, `nfs`, `afpfs`, `webdav`). This is reliable — it detects the actual mount, not the UI category.

### Trash support on volumes (`supportTrash`)

Similarly, **expose `supportTrash: boolean` on every volume**. Detection: attempt `statfs` and check the filesystem
type. APFS and HFS+ always support trash. FAT32 (`msdos`), exFAT (`exfat`), and network filesystems (`smbfs`, `nfs`,
`afpfs`, `webdav`) do not reliably support trash. Add `supports_trash: bool` to `LocationInfo` and
`supportsTrash?: boolean` to `VolumeInfo`.

This allows the DeleteDialog to **proactively** show the "no trash" warning before the user even tries — instead of
waiting for `trashItemAtURL` to fail. The dialog checks `supportsTrash` on the source volume. If `false`, it forces
permanent delete mode and shows the warning banner, same as the network case. If `trashItemAtURL` unexpectedly fails
on a volume where `supportsTrash` was `true`, the per-item error handling still catches it.

### Remote volume warning

When the source path is on a volume where `supportsTrash === false` (network filesystem, FAT32, exFAT, or other
volumes that don't support trash — detected proactively via `fsType`), the dialog changes:

```
┌──────────────────────────────────────────────────────┐
│  Delete 3 files and 1 folder                         │
│                                                      │
│  From: /Volumes/NAS/projects                         │
│                                                      │
│  ┌──────────────────────────────────────────────┐    │
│  │  ⚠ This volume doesn't support trash.        │    │  ← Warning banner
│  │    Files will be permanently deleted.         │    │    color-warning background
│  └──────────────────────────────────────────────┘    │    Icon + bold first sentence
│                                                      │
│  ┌──────────────────────────────────────────────┐    │
│  │ report-draft.docx        2.4 MB              │    │
│  │ notes.txt                  800 B             │    │
│  └──────────────────────────────────────────────┘    │
│                                                      │
│  3.2 MB / 3 files                               ✓   │
│                                                      │
│  ┌──────────┐  ┌──────────────────────────────┐     │
│  │  Cancel   │  │  Delete permanently          │     │  ← Red/destructive button
│  └──────────┘  └──────────────────────────────┘     │    (forced, no trash option)
└──────────────────────────────────────────────────────┘
```

The warning banner sits above the file list, uses `color-warning` for the background/border, and includes a warning
icon. The primary button is always "Delete permanently" (red) on network volumes — no trash option. Uses
`role="alertdialog"` since this is always a permanent (irreversible) operation.

### Shortcuts

| Action | Shortcuts | Notes |
|---|---|---|
| Delete (move to trash) | `F8`, `⌘⌫` | `⌘⌫` is the macOS standard for "Move to trash" |
| Delete permanently | `⇧F8`, `⇧⌘⌫` | Always permanent — no setting, explicit user choice |

Note: plain `Backspace` is already used for `nav.parent` (go to parent folder). `⌘⌫` (Cmd+Backspace) does not
conflict — it's a different key combo.

### Progress: reuse TransferProgressDialog

**Always show a progress dialog**, even if the operation completes near-instantly. This keeps the UX consistent and
predictable. **Minimum display time: 400ms.** If the operation completes faster (for example, trashing a single small
file), the dialog stays visible for 400ms with a completed state before closing. This prevents a jarring one-frame
flash while still confirming the action happened. Implementation: track the dialog open timestamp, and on completion,
delay closing by `Math.max(0, 400 - elapsed)` ms.

**Reuse TransferProgressDialog** for delete/trash progress. Speed, ETA, and progress bars are all useful for delete
too, and a unified progress component keeps the UX consistent across all file operations. The main changes needed:

1. **Widen `TransferOperationType`** (in `types.ts`): add `'delete' | 'trash'` to the union. Consider renaming to
   `WriteOperationType` while touching it, since it now covers all write operations — but this rename is optional
   and can be deferred. Update `operationVerbMap` in `transfer-error-messages.ts` and `operationLabelMap` in
   `transfer-dialog-utils.ts` with delete/trash verbs.
2. **Make transfer-specific props optional**: `destinationPath`, `direction`, `destVolumeId`, and
   `conflictResolution` don't apply to delete/trash. Make them optional (`?`) in the Props interface. The
   component already needs to handle conditional display of destination-related UI.
3. **Hide inapplicable UI for delete/trash**: rollback button (delete can't undo — though may add later for trash),
   per-file conflict resolution (no conflicts in delete). Guard these with
   `if (operationType === 'copy' || operationType === 'move')`.
4. **Phase labels**: Add `'deleting'` and `'trashing'` to `WriteOperationPhase` (or map `Trash` to "Moving to
   trash..." and `Delete` to "Deleting..." in the display logic).

**Scanning always starts in the confirmation dialog** (via `startScanPreview()`). What happens next depends on
whether the user confirms before the scan completes, and whether it's trash or permanent delete:

**Permanent delete — scanning must finish before deleting:**

1. User presses Shift+F8. Confirmation dialog opens, scan preview starts in the background.
2. User sees stats filling in live. Primary button is enabled immediately.
3. User clicks "Delete permanently" (before or after scan completes).
4. Progress dialog opens. If the scan preview hadn't finished yet, the progress dialog shows the scanning phase
   first (same as copy/move) — deletion only begins after scanning completes. If the scan preview already finished,
   the progress dialog reuses those results and jumps straight to the deleting phase.
5. During the deleting phase, `delete_files_with_progress()` recurses and deletes individual files. Items bar:
   "537 of 1,207 files". Bytes bar: "128 of 512 MB". Speed and ETA work normally.

**Trash — scanning is cancelled on confirm, trash starts immediately:**

1. User presses F8. Confirmation dialog opens, scan preview starts in the background.
2. User sees stats filling in live. Primary button is enabled immediately.
3. User clicks "Move to trash" (before or after scan completes).
4. **Scan preview is cancelled** (if still running). Trash starts immediately — no waiting for the scan to finish.
   Why: `trashItemAtURL` is atomic per top-level item (it moves the whole folder in one OS call), so there's no
   need to know the deep file tree. Waiting would just add latency for no benefit.
5. Progress dialog opens. Items bar counts **top-level items** using the selection size as the total: "2 of 5".
   If per-item sizes are available (scan completed before confirm, or from drive index), the bytes bar is shown
   too — it jumps in chunks as each top-level item completes. If sizes aren't available (scan was cancelled
   mid-flight), the bytes bar is hidden. Speed/ETA work when the bytes bar is visible.

**TransferProgressDialog layout for trash:**

```
┌──────────────────────────────────────────────┐
│  Moving 5 items to trash…                    │  ← Title verb: "Moving to trash"
│                                              │
│  ████████████░░░░░░░░░░░░░░░░  2 of 5       │  ← Top-level items progress
│  ████████░░░░░░░░░░░░░░░░░░░░  128 of 512 MB│  ← Bytes bar (when sizes available)
│  components/                                 │  ← Current item name
│                                              │     Speed/ETA shown as usual
│              ┌──────────┐                    │     No rollback button
│              │  Cancel   │                    │     No conflict resolution
│              └──────────┘                    │
└──────────────────────────────────────────────┘
```

**TransferProgressDialog layout for permanent delete:**

```
┌──────────────────────────────────────────────┐
│  Deleting 1,207 files…                       │  ← Title verb: "Deleting"
│                                              │
│  ████████████░░░░░░░░░░░░░░░░  537 of 1,207 │  ← Individual files progress (from scan)
│  ████████░░░░░░░░░░░░░░░░░░░░  128 of 512 MB│  ← Bytes progress
│  components/Header.svelte                    │  ← Current file name
│                                              │     Speed/ETA shown as usual
│              ┌──────────┐                    │     No rollback button
│              │  Cancel   │                    │     No conflict resolution
│              └──────────┘                    │
└──────────────────────────────────────────────┘
```

- **Confirm → progress transition**: Same pattern as TransferDialog → TransferProgressDialog. The DeleteDialog's
  `onConfirm` callback calls the backend (`trash_files_start()` or `delete_files`), closes the confirmation dialog,
  and opens TransferProgressDialog with the appropriate `operationType`. Follow the same wiring in
  `dialog-state.svelte.ts`.
- **dialog-state.svelte.ts**: The existing `showTransferProgress()` method and state should work for delete too —
  just pass `operationType: 'delete'` or `'trash'`. May need to make some fields in `TransferProgressPropsData`
  optional (destination, conflicts). Check whether new state variables are needed or the existing ones suffice.

### Cancellation

Both trash and permanent delete must be **reliably cancellable** at any point during execution:

- **Trash**: The `trash_files_with_progress()` loop checks an `AtomicBool` (same pattern as
  `delete_files_with_progress`) between each item. Cancellation stops after the current item completes — items
  already trashed stay trashed (no rollback, since trash is recoverable anyway). Emits `write-cancelled`.
- **Permanent delete**: Already cancellable via existing `AtomicBool` in `delete_files_with_progress`. Same behavior:
  stops between items, already-deleted items are gone.
- **UI**: The progress dialog shows a "Cancel" button. Pressing Escape also cancels.

### Post-delete behavior

- **Cursor stays at the same position index** (not the same file). The intent is: the cursor lands on whatever file
  now occupies the position the user was looking at, keeping the visual context stable.
  - **Cursor file survived**: If the file under the cursor was not deleted, the cursor stays on it. Its index may
    shift down if items above it were deleted.
    Example: 25 files, cursor on #10 ("config.json"). Items #3, #5, #7 are deleted. "config.json" is now at index
    #7. Cursor stays on "config.json" at its new position #7.
  - **Cursor file was deleted**: The cursor lands on whatever file now occupies the same position (or the last file
    if the list shrank past the cursor position). Effectively, it moves to the next surviving file.
    Example: 25 files, cursor on #10. Items #9, #10, #11 are deleted. The file that was at #12 is now at #9.
    Cursor lands on position #9 (the next surviving file).
  - **All files deleted**: Cursor goes to position 0 (which will be ".." if the directory has a parent).
  - Implementation: **requires a small change to `apply-diff.ts`**. Currently, when the cursor file is removed,
    `applyDiff()` returns `0` (resets to the first entry). The desired behavior is to stay at the same position
    index, landing on the next surviving file. Fix: when `pathUnderCursor` is not found after applying changes,
    return `Math.min(originalCursorIndex, files.length - 1)` instead of `0`. This is a one-line change that
    improves cursor behavior for all removals (not just delete). The file watcher emits `directory-diff` events →
    `apply-diff.ts` removes entries in-place → the clamped cursor index lands on the next file naturally.
- **Pane refresh**: File watcher emits `directory-diff` events → `apply-diff.ts` removes entries in-place. Both panes
  refresh if watching the same directory.
- **Selection**: Cleared after delete (items no longer exist).

### Error handling

- **Partial failures**: If some items fail (permission denied, file in use), the operation continues with remaining
  items. After completion, show an error dialog listing the failed items and reasons. Use the `TransferErrorDialog`
  pattern.
- **Full failure**: If the operation can't start at all (for example, no permission on the parent directory), show
  the error dialog immediately.
- **Error messages**: Positive and actionable per the style guide. For example: "Couldn't delete 'config.json' — the
  file is in use by another app. Close it and try again."
- **Locked files (`chflags uchg`)**: `trashItemAtURL` handles locked files fine (moves them to trash on APFS). But
  `fs::remove_file` in permanent delete will fail on locked files. The error message should be specific: "Couldn't
  delete 'filename' — the file is locked. Unlock it in Finder (Get Info → uncheck Locked) and try again." Detect
  via `io::ErrorKind::PermissionDenied` + checking `metadata.permissions().readonly()` or `st_flags & UF_IMMUTABLE`.
- **TOCTOU (time-of-check-to-time-of-use)**: Between the user confirming the dialog and the backend starting the
  operation, files may have been moved, renamed, or deleted by another process. If a source path no longer exists
  when the operation starts, treat it as a per-item error (same as permission failures). No special handling needed
  — the partial failure path already covers this.

## Implementation plan

### Milestone 1: Volume infrastructure — `fsType` + `supportsTrash`

Expose filesystem type and trash support on every volume. This is a reusable building block used by delete
(for proactive "no trash" warnings) and useful elsewhere (copy strategy hints, volume info display, diagnostics).

1. **Rust: add `fs_type` and `supports_trash` to `LocationInfo`** (`volumes/mod.rs`). New fields:
   `fs_type: Option<String>` and `supports_trash: bool`. Use `libc::statfs` on each volume's path during
   `list_locations()`, read `f_fstypename` (cast the `[i8; 16]` C string to a Rust `&str`). Map to
   `supports_trash`: `true` for `apfs`, `hfs` → `true`; `smbfs`, `nfs`, `afpfs`, `webdav`, `msdos`, `exfat` →
   `false`; unknown → `true` (optimistic default, `trashItemAtURL` failure is caught anyway). If `statfs` fails
   (for example, volume ejected mid-listing), set `fs_type: None`, `supports_trash: true`.
2. **TypeScript: add `fsType` and `supportsTrash` to `VolumeInfo`** (`types.ts`). New optional fields:
   `fsType?: string` and `supportsTrash?: boolean`. These are populated from the Rust side; `undefined` means
   unknown (treat as `true` for trash support).
3. **Tests**: Unit test the `statfs` → `fs_type` mapping. Test that favorites (which aren't mount points themselves)
   gracefully handle `statfs` (they inherit from the root volume or report `None`).

### Milestone 2: Backend — trash support

Extend the existing `move_to_trash()` pattern into a batch operation with progress and cancellation.

1. Add `WriteOperationType::Trash` variant to the `WriteOperationType` enum in `types.rs`. This distinguishes
   trash from permanent delete in all event payloads and frontend display logic.
2. **Extract** `move_to_trash_sync()` from `commands/rename.rs` into `write_operations/trash.rs` as a reusable
   function. Fix the symlink bug: replace `path.exists()` (follows symlinks — fails on dangling symlinks) with
   `fs::symlink_metadata(&path).is_ok()` (checks the path itself). Update `commands/rename.rs` to call the
   extracted function. Why: the core ObjC `trashItemAtURL` logic should live in the write_operations layer, not
   coupled to a Tauri command signature.
3. Create `trash_files_with_progress()` in `trash.rs`:
   - Takes `sources: &[PathBuf]`, app handle (for events), state (for cancellation via `AtomicBool`), plus
     optional per-item sizes (from scan stats or drive index, for the bytes progress bar)
   - Iterates top-level items, calling the extracted `move_to_trash_sync()` for each
   - Uses `symlink_metadata()` for existence checks (same fix as above — dangling symlinks must work)
   - Checks cancellation flag between each item
   - Emits `write-progress` events with `operationType: Trash`. Items count = top-level items (matches the
     actual backend granularity — `trashItemAtURL` is atomic per item). Bytes = cumulative size of items
     trashed so far (from per-item sizes, if available)
   - Emits `write-complete`, `write-cancelled`, or `write-error`
   - Collects per-item errors and reports them in the completion event (partial success is possible)
   - On `trashItemAtURL` failure for a non-network volume (for example, FAT32 USB drive), report a clear per-item
     error: "This volume doesn't support trash. Use Shift+F8 to delete permanently."
4. Add `trash_files_start()` public entry point in `mod.rs` (mirrors `delete_files_start()`).
5. Add Tauri command wrapper in `commands/file_system.rs` (mirrors `delete_files` command).
6. Add TypeScript bindings in `write-operations.ts`: both `trashFiles()` (new) and `deleteFiles()` (currently
   missing — the Rust `delete_files` command exists but has no frontend wrapper). `TransferProgressDialog` will need
   to call these in its `startOperation()` function alongside the existing `copyFiles()`/`moveFiles()` branches.

### Milestone 3: Frontend — DeleteDialog + extend TransferProgressDialog

Build the delete confirmation dialog and extend TransferProgressDialog for delete/trash.

1. Create `DeleteDialog.svelte` in `src/lib/file-operations/delete/`.
2. Props: `sourceItems` (array of `{ name, size, isDirectory, isSymlink, recursiveSize?, recursiveFileCount? }`),
   `sourceFolderPath`, `isPermanent` (true when user pressed Shift+F8/Shift+⌘⌫; the dialog also forces this to
   true when `supportsTrash` is false), `supportsTrash` (boolean — from the source volume's `supportsTrash` field;
   when false, forces permanent delete mode and shows the warning banner), `isFromCursor` (boolean — true when
   acting on cursor item with no explicit selection, false when acting on selected items).
3. **Title with selection source**: Use `isFromCursor` to generate the title. Selected items:
   "Delete 3 selected files and 1 folder". Cursor item: "Delete 1 file under cursor" or "Delete 1 folder under
   cursor". Why: the user must always know whether they're acting on their selection or just the cursor item.
   The cursor-as-implicit-selection pattern (from `buildTransferPropsFromCursor()`) is convenient but can surprise
   users if they don't realize nothing was selected.
4. File list: scrollable list showing up to 10 items with size info. "… and N more items" overflow row when > 10.
   Use `recursiveSize` from FileEntry when available (indexed volumes), fall back to scan preview for others.
5. Symlink notice: if any `sourceItems` have `isSymlink: true`, show the warning text.
6. No-trash warning: if `supportsTrash === false`, show the warning banner above the file list.
7. Scan preview: reuse `startScanPreview()` for total deep stats. Same live-counting pattern as TransferDialog.
   On indexed volumes, stats are instant from drive index (no scan needed). On non-indexed volumes, the scan runs
   progressively in the background.
8. **Primary button always enabled**: The confirm button works immediately, even while the scan is still running.
   No separate "Skip scan" button — the user just clicks "Move to trash" whenever they're ready. If they confirm
   before the scan completes, the progress dialog adapts: for trash, it always works (top-level count is known
   from the selection); for permanent delete, it shows the scanning phase first (same as copy/move). If they
   wait, the stats fill in live. Why: "Skip scan" is jargon-y, adds a conditional button, and doesn't improve
   safety. The primary action should always be one click away.
9. Buttons: "Cancel" (secondary) + "Move to trash" (blue) or "Delete permanently" (red when permanent or network).
10. Keyboard: Enter confirms (focus on primary button by default), Escape cancels.
11. Accessibility: use `role="dialog"` for trash (recoverable, low urgency) and `role="alertdialog"` for permanent
    delete (irreversible, high urgency). `ariaDescribedby` points to the warning/description text.
12. **Extend TransferProgressDialog** for delete/trash support (see "Progress" section above for full details):
    widen `TransferOperationType` to include `'delete' | 'trash'`, make transfer-specific props optional, hide
    rollback/conflict UI for delete/trash, add phase labels.
13. **Wire dialog state**: The existing `showTransferProgress()` in `dialog-state.svelte.ts` should work for delete
    too — pass `operationType: 'delete'` or `'trash'`. Make transfer-specific fields optional in the props data
    type. Add state for the DeleteDialog (confirmation) itself: `showDeleteDialog`, `deleteDialogProps`, etc.

### Milestone 4: Wiring — commands, shortcuts, function key bar

Connect delete to the rest of the app.

1. **Command registry** (`command-registry.ts`):
   - Add `file.delete`: name "Delete", scope "Main window/File list", shortcuts `['F8', '⌘⌫']`,
     `showInPalette: true`
   - Add `file.deletePermanently`: name "Delete permanently", scope "Main window/File list",
     shortcuts `['⇧F8', '⇧⌘⌫']`, `showInPalette: true`
2. **Command execution** (`+page.svelte`): Add cases for `file.delete` and `file.deletePermanently`.
3. **DualPaneExplorer**: Add `openDeleteDialog(permanent: boolean)` method. Use the same selection-or-cursor
   pattern as `openUnifiedTransferDialog`: if `selectedIndices.length > 0`, use selection; otherwise use
   `buildTransferPropsFromCursor()` for the cursor item. Pass `isFromCursor` boolean to the dialog so the title
   can distinguish "3 selected files" from "1 file under cursor". Look up `supportsTrash` from the source volume's
   `VolumeInfo`, open dialog.
4. **FunctionKeyBar**: Enable F8 button, add `onDelete` prop. Shift state: show "⇧F8 Permanently" when Shift held.
5. **DialogManager**: Render `DeleteDialog` when active. TransferProgressDialog is already rendered — it handles
   delete/trash via the widened operation type.

### Milestone 5: MCP + context menu

Expose delete to agents and right-click menu.

1. **MCP tools** (`tools.rs`): Add `"delete"` to `get_file_op_tools()`. No params (uses selection, like copy).
2. **MCP executor** (`executor.rs`): Route `"delete"` tool call → emit event → frontend opens delete dialog.
3. **MCP dialog types** (`tools.rs`): Add `"delete-confirmation"` to dialog type enum.
4. **Context menu**: When the file-list context menu is added (separate feature), include "Move to trash" / "Delete"
   items. Not blocking for MVP.
5. Update MCP tool count tests.

### Milestone 6: Polish + edge cases

1. **Remove both delete settings**: Delete both `fileOperations.confirmBeforeDelete` and
   `fileOperations.deletePermanently` from `settings-registry.ts` entirely. Neither has ever been enabled. The
   dialog is always shown, and trash is always the default. Permanent delete is available via Shift+F8.
2. **No-trash volume detection**: Match source path prefix against mounted volumes. If the matching volume has
   `supportsTrash === false` (based on `fsType`: network filesystems, FAT32, exFAT), force permanent delete mode
   and show the warning banner. This uses the new `fsType`/`supportsTrash` fields from Milestone 1.
3. **Empty selection guard**: If nothing is selected and cursor is on "..", do nothing (same as copy). This is
   already handled by `buildTransferPropsFromCursor()` returning `null` for ".." entries.
4. **Post-delete cursor positioning**: Fix `apply-diff.ts` — when the cursor file is removed, return
   `Math.min(originalCursorIndex, files.length - 1)` instead of `0`. This one-line change gives the "stay at same
   position" behavior described in the "Post-delete behavior" section. Improves cursor behavior for all removals.
5. **Error handling**: Reuse the existing `TransferErrorDialog` (already wired in `DialogManager.svelte` and
   `dialog-state.svelte.ts`) for per-item errors. It accepts `operationType` and `error` props — pass the
   appropriate values for trash/delete operations.

### Milestone 7: Testing + docs

1. **Rust tests**: Unit tests for `trash_files_with_progress` (mock fs), test cancellation, test progress events,
   test partial failure (some items fail, others succeed).
2. **Vitest**: Test `DeleteDialog` rendering: normal mode, permanent mode, network volume warning, symlink notice,
   overflow (> 10 items), scan stats. Test TransferProgressDialog with delete/trash operation types.
3. **Manual testing**: Test with indexed volumes (instant sizes), non-indexed (scan preview), network volumes
   (permanent only), large selections (1000+ files), permissions errors, cancellation mid-operation.
4. **Coverage allowlist**: Add new files to `coverage-allowlist.json` if they depend on Tauri/DOM.
5. **CLAUDE.md files**: Add `delete/CLAUDE.md` documenting the delete subsystem.
6. Run `./scripts/check.sh` for all checks.

## Task list

### Milestone 1: Volume infrastructure — `fsType` + `supportsTrash`
- [x] Add `fs_type: Option<String>` and `supports_trash: bool` to `LocationInfo` in `volumes/mod.rs`
- [x] Implement `statfs`-based `f_fstypename` reading for each volume during `list_locations()`
- [x] Map `fs_type` → `supports_trash` (apfs/hfs → true; smbfs/nfs/afpfs/webdav/msdos/exfat → false; unknown → true)
- [x] Add `fsType?: string` and `supportsTrash?: boolean` to `VolumeInfo` in TypeScript `types.ts`
- [x] Write Rust unit tests for `fs_type` mapping

### Milestone 2: Backend — trash support
- [x] Add `WriteOperationType::Trash` variant to `types.rs`
- [x] Extract `move_to_trash_sync()` from `commands/rename.rs` into `write_operations/trash.rs`
- [x] Fix symlink bug: replace `path.exists()` with `symlink_metadata()` in extracted function
- [x] Update `commands/rename.rs` to call the extracted function
- [x] Create `trash_files_with_progress()` with cancellation via `AtomicBool`, top-level item + optional bytes progress
- [x] Handle `trashItemAtURL` failure on non-trash volumes with clear error message
- [x] Add `trash_files_start()` in `mod.rs`
- [x] Add Tauri command wrapper
- [x] Add TypeScript bindings: both `trashFiles()` (new) and `deleteFiles()` (missing — Rust command exists, no frontend wrapper)
- [x] Add `startOperation()` branches for delete/trash in TransferProgressDialog
- [x] Write Rust unit tests (including cancellation, partial failure, dangling symlinks)

### Milestone 3: Frontend — DeleteDialog + extend TransferProgressDialog
- [x] Create `DeleteDialog.svelte` with file list (max 10 items + overflow), scan stats, confirm/cancel
- [x] Add `isFromCursor` prop; title shows "N selected files" vs "1 file under cursor"
- [x] Handle `isPermanent` prop for visual differences (red button, warning text)
- [x] Handle `supportsTrash === false` for proactive warning banner (replaces old `isNetworkVolume` check)
- [x] Add symlink notice when selection includes symlinks
- [x] Integrate scan preview for live stats; primary button always enabled (no "skip scan" button)
- [x] Use `recursiveSize` from FileEntry when available (indexed volumes)
- [x] Extend TransferProgressDialog: widen operation type, optional props, hide rollback/conflicts for delete/trash
- [x] Add 400ms minimum display time for progress dialog (delay close if operation completes faster)
- [x] Wire dialog state: reuse transfer progress state for delete, add delete confirmation dialog state
- [x] Write Vitest tests for both dialogs

### Milestone 4: Wiring
- [x] Add `file.delete` and `file.deletePermanently` to command registry with F8/`⌘⌫` shortcuts
- [x] Add command execution cases in `+page.svelte`
- [x] Add `openDeleteDialog()` in DualPaneExplorer with selection-or-cursor pattern and `isFromCursor` flag
- [x] Enable F8 in FunctionKeyBar with Shift variant
- [x] Render DeleteDialog in DialogManager (TransferProgressDialog already handles delete/trash)

### Milestone 5: MCP + context menu
- [x] Add `delete` MCP tool
- [x] Route in MCP executor
- [x] Add `delete-confirmation` dialog type
- [x] Update MCP tool count tests

### Milestone 6: Polish + edge cases
- [x] Remove both `confirmBeforeDelete` and `deletePermanently` settings from registry
- [x] Handle no-trash volumes via `supportsTrash` field (force permanent, show warning)
- [x] Empty selection guard (cursor on ".." with nothing selected → no-op)
- [x] Fix `apply-diff.ts`: return `Math.min(originalCursorIndex, files.length - 1)` instead of `0` on cursor file removal
- [x] Error handling via existing TransferErrorDialog (including locked file detection with specific message)

### Milestone 7: Testing + docs
- [x] Manual testing: indexed volumes, non-indexed, no-trash volumes (FAT32, network), large selections, symlinks, dangling symlinks, locked files, errors, cancellation
- [x] Add new files to `coverage-allowlist.json`
- [x] Create `delete/CLAUDE.md`
- [x] Run `./scripts/check.sh`
