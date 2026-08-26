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

**Why confirm awaits `startScanPreview`.** Three reasons, all the same shape as `TransferDialog`'s own `scanStarted`
await. A null `previewId` gives the operation nothing to claim, so it re-walks the tree concurrently with the walk the
preview's `startScan` already began; that orphan has no owner and nothing to cancel it, because teardown's cleanup is
gated on `!confirmed`. The IPC itself only mints an id and spawns the walk, so it answers promptly even on a wedged
share, which is what makes awaiting it safe.

**Who consumes the walk.** A permanent delete waits for it in the BACKEND (`scan_bridge::await_claimed_preview`) and
consumes the cached result rather than re-walking. Trash consumes nothing: `trashItemAtURL` is atomic per top-level
item, so `trash_files_start` frees the preview outright rather than leaving an ownerless walk running. Scan events still
carry index-derived `expectedFilesTotal` / `expectedBytesTotal`, but the frontend renders no progress bar from them: it
read as "already deleting" while the scan was still counting.

## Edge cases

- **Dangling symlinks**: `symlink_metadata()` instead of `path.exists()`. A dangling symlink (target deleted) is still a
  valid item to trash/delete.
- **Locked files**: `trashItemAtURL` handles locked files on APFS. Permanent delete fails on locked files with a message
  suggesting unlocking via Finder.
- **No-trash volumes**: detected proactively via `supportsTrash`. The dialog forces permanent mode and shows a warning.
  If `trashItemAtURL` unexpectedly fails on a "supports trash" volume, the per-item error suggests Shift+F8.
- **Partial failures**: the operation continues; successful items stay deleted/trashed. Errors are reported via
  `TransferErrorDialog` after completion.

## Undo and go-to-trash (the trash completion toast)

`TrashCompleteToastContent.svelte` replaces the plain string toast when a TRASH completes and a journaled operation id
is available. It carries two actions and, deliberately, no third.

**Why Undo lives here.** The rollback engine could always reverse a trash: a trash row records the OS's own in-trash
location (`resultingItemURL`), and its inverse is a pinned restore-move back to the source
(`src-tauri/src/operation_log/rollback.rs`). What was missing was a surface. The operation log is read-only and the
queue's Rollback button is a different thing entirely (cancel-an-in-flight-op-and-undo-its-partials, gated on
running/paused), so a completed trash had no reachable undo at the one moment it matters.

**Decision: no "delete permanently" button.** The toast renders after every trash, including the ones the user is glad
they can take back, and a one-click irreversible action on a transient surface that appears that often is a misclick
away from the one operation the journal marks never-rollbackable. Permanent stays a choice made in the delete dialog,
where Shift flips it in place. `TrashCompleteToastContent.svelte.test.ts` pins the button set so the third button can't
arrive by accident.

**Undo mechanics** (`trash-undo.ts`). `undoOperations([operationId])` resolves only once every inverse has run, with the
full tally, so there's no polling. Each inverse is a queued managed operation, so it waits out anything already working
the volume and can take a while: hence a PERSISTENT progress toast rather than a transient one, replaced by a fresh
transient toast at the end (`addToast` replaces content and level in place, never dismissal or timeout).

The honesty rule in `trashUndoOutcome` mirrors Ask Cmdr's rename undo: anything left behind outranks what came back. A
refusal is counted apart from `skipped` because it carries no per-item numbers at all (`RollbackRefusal` is a typed
union: unknown op, already rolling back, already rolled back, not rollbackable, volume unavailable). Per-item skips are
the likelier outcome and are not refusals: drift, an occupied restore target, or an unverifiable precondition all leave
an item in the trash on purpose, because a rollback never overwrites.

**Go-to-trash mechanics** (`go-to-trash.ts`). Two entries, differing in what they know:

- `goToTrash(explorer)` (the `file.goToTrash` palette command) resolves the trash of the FOCUSED PANE's volume, so
  standing on an external drive opens that drive's trash.
- `goToTrashedItems(explorer, operationId, fromPath)` (the toast button) reads the recorded in-trash path out of the
  journal and lands the cursor on the item, falling back to the volume trash when no location was recorded.

Reading the journal requires waiting out `write-settled` first: item rows are buffered in memory and flushed in the
finalize barrier, so a read at completion time comes back empty (`../settled-operations.ts`). Rows come back `seq ASC`
across all row roles, so a trashed folder interleaves `searchOnly` leaves among the `rollbackUnit` rows; only the latter
are the user's own top-level items.

**Gotcha: the resolver answers for a VOLUME, not an item.** `get_trash_dir` asks Cocoa
(`URLForDirectory:inDomain:appropriateForURL:create:`), which resolves the URL's volume and therefore refuses a path
that doesn't exist — and the paths asked about are routinely gone (the item was just trashed away). The Rust side walks
up to the nearest live ancestor for exactly that reason; see
`src-tauri/src/file_system/write_operations/delete/trash.rs`.

**Gotcha: revealing a trashed dotfile throws.** `explorer.moveCursor(pane, name)` throws when the name isn't in the
visible listing, and a dotfile isn't with "show hidden files" off. The navigation has already happened by then, so the
throw is caught and logged: the user is in the right trash, only the cursor didn't land.

**The adopted path keeps the plain toast.** A window that ADOPTED an operation it never started
(`../../file-explorer/pane/adopted-operation.svelte.ts`) has no birth context, so it has no source folder to fall back
to and no business acting on panes it didn't aim. It reports the trash and stops there; the undo stays reachable from
the operation that started it.

**Platform reality.** Linux's trash backend surfaces no in-trash location, so neither action has anything to work with
there; both degrade to their fallbacks rather than being gated on the platform.
