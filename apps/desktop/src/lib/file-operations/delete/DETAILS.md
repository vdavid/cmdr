# Delete and trash details (frontend)

Depth and rationale. `CLAUDE.md` holds the must-knows; the flow and edge-case catalog live here.

## How delete flows

1. **Shortcut**: F8 (trash) or Shift+F8 (permanent delete).
2. **Command**: `file.delete` or `file.deletePermanently` in `command-registry.ts`, handled in `+page.svelte`.
3. **Selection**: `DualPaneExplorer.openDeleteDialog({ permanent })` builds props from selection or cursor item (same
   pattern as copy/move). Looks up `supportsTrash` from the source volume's `VolumeInfo`.
4. **Dialog**: `DeleteDialog` opens with the file list; scan preview starts in the background via `startScanPreview()`.
5. **Confirm**: `DeleteDialog` passes back the active `isPermanent` (from the switch);
   `dialog-state.svelte.ts::handleDeleteConfirm(previewId, isPermanent)` transitions to `TransferProgressDialog` with
   `operationType: 'trash'` or `'delete'`.
6. **Backend**: `trash_files_start()` or `delete_files_start()` in `write_operations/mod.rs` runs the operation.
7. **Progress**: `TransferProgressDialog` shows items/bytes progress with cancel support.
8. **Completion**: toast notification, both panes refreshed, 400 ms minimum display time.

## Shift-hold upgrade

**Decision**: on a dialog opened with F8, holding Shift reads as "permanent for as long as I hold it": the switch, the
confirm button, and the `alertdialog` role all follow the key, and releasing it returns to trash. It makes the
escalation one gesture instead of cancel-and-retry, and it matches what Shift already means on F8 itself.

**Why it's gated to F8 dialogs** (`shiftUpgradesToPermanent = !initialIsPermanent && supportsTrash`, snapshotted at
open): on a Shift+F8 dialog the user is still holding the key that opened it, so acting on that release would demote a
permanent delete they deliberately asked for. Shift therefore only ever upgrades; it never demotes. The switch position
(`switchIsPermanent`) stays separate from the effective `isPermanent`, so flipping the switch by hand outlives a Shift
tap.

**Why the window, not the dialog**: `keydown`/`keyup` are on `window`, and every event re-reads `event.shiftKey` rather
than matching the key name, so a keyup we never saw (a window switch, a native menu eating it) self-heals on the next
keystroke. `blur` clears the hold outright, since a Shift released outside the window never comes back to us.

**Why the CAPTURE phase** (`SHIFT_LISTENER_PHASE`): `ModalDialog`'s overlay opens `handleOverlayKeydown` with an
unconditional `event.stopPropagation()`, which is how every dialog shields the file explorer from its own typing. Focus
sits on that overlay (it takes focus on mount), so a keydown starts inside the dialog and dies at the overlay: a
bubble-phase `window` listener is downstream and never runs. `keyup` isn't stopped, so a bubble-phase listener would see
only releases: the hold could never turn on, the feature would be dead in the app, and the unit tests would still pass.
Capture on `window` runs before anything in the tree can stop the event.

**Test the real path.** `DeleteDialog.shift-hold.svelte.test.ts` dispatches from `document.activeElement` inside the
dialog and lets the event bubble, exactly as a browser does. Dispatching straight on `window` skips the overlay and
turns the suite into a false-positive net; one test asserts the overlay really does eat the keydown, so a future
"simplification" back to `window.dispatchEvent` fails loudly.

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
