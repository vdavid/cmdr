# Delete and trash details (frontend)

Depth and rationale. `CLAUDE.md` holds the must-knows; the flow and edge-case catalog live here.

## How delete flows

1. **Shortcut**: F8, Delete, or ⌘⌫ (trash); Shift+F8, Shift+Delete, or ⌥⌘⌫ (permanent delete, Finder's "Delete
   Immediately"). Total Commander users add Shift to the key they trash with.
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

**A dismiss can land before the scan is under way.** The start is async (four listener registrations, then the IPC), so
`startScan` reads its props before the first await, and a dialog that closed keeps no listener and no preview it
started. Why a prop read after unmount throws, and the rule both scanning dialogs follow: `../DETAILS.md` § "A dialog's
async start can outlive it".

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
- **Partial failures**: the operation continues; successful items stay deleted/trashed, and a batch trash whose items
  were ALL refused ends as a `write-error` the `TransferErrorDialog` renders. A batch that took some and was refused the
  rest ends as a completion carrying `refused` (`{ itemCount, reason }`, the backend's `strongest_refusal`), and the
  frontend says both halves: the completion toast drops from success to `warn` and keeps its Undo, and a second `warn`
  toast beside it names how many items stayed and why. That sentence is `errors.write.trashRefused.message.<reason>`,
  the error dialog's own wording (`composeTrashRefusedToast` in `transfer-complete-toast.ts`), so the partial ending
  can't drift from the total one. It rides a toast of its own because the completion sentences carry no terminal
  punctuation to append to, and no locale would agree on which mark to add.

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
  journal and lands the cursor on the item, falling back to the volume trash when no location was recorded or the
  journal read throws.

Neither entry throws. The toast calls `goToTrashedItems` with `void`, so a rejection would escape as an unhandled one
and auto-send an error report for what is usually a slow drive: `get_trash_dir` answers `MutationError::TimedOut` after
2 s. A failed trash lookup shows the shared `locationUnreachableToast` and logs at info.

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

## Cloud storage: when a trash opens the delete dialog

`~/Library/CloudStorage/<provider>/` is Apple's location for third-party File Provider drives, and a provider there
evicts files it has uploaded, leaving a placeholder macOS marks `SF_DATALESS` (Finder: "online-only"). The Trash is a
folder on the boot volume, so trashing one of those DOWNLOADS it first. That download is the whole reason this routing
exists, ❌ not the refusals macOS sometimes gives instead: a 111 kB file cost half a second, a folder holding 50 GB of
evicted content would cost the lot, to fill a folder the person is about to empty. Rationale and evidence:
`write_operations/delete/cloud_trash.rs`.

So `openDeleteDialog` (and its search-results twin) asks `trashRoutingForPaths(sourcePaths)` before it opens anything.
The answer has two fields:

- Either delete variant of `routing` → `cloudOnlineOnly: 'all' | 'mixed'`, `isPermanent` forced, `supportsTrash` dropped
  (which is what hides the in-dialog switch). `DeleteDialog` renders the online-only banner in place of the generic
  no-trash one, so a person who pressed Trash reads why they're being asked about a delete, and confirm dispatches the
  same permanent delete Shift+F8 would have. The extent picks between two whole messages
  (`fileOperations.delete.cloudOnlineOnly{Mixed,All}Warning`): they differ in their opening sentence and in the ways out
  they can name, since "deselect all online-only files" would leave nothing selected once everything is evicted.
  `ONLINE_ONLY_EXTENT_BY_ROUTING` in `file-operation-commands.ts` is the one mapping, keyed by `TrashRouting` so a new
  variant is a compile error rather than a banner that quietly stops appearing.
- `folderMayHoldOnlineOnly` → the answer isn't final. `SF_DATALESS` lives on files, so a selected FOLDER can only be
  judged by walking it, and the dialog's scan preview is that walk (`scan_walker.rs`'s `OnlineOnlyWatch`). Its
  `scan-preview-progress` / `-complete` events carry `onlineOnlyFound`, and the dialog flips itself the moment one
  reports a hit: banner in, switch out, confirm button becomes the delete. ❌ Never a second walk for this. ❗ A walk
  hit always renders the MIXED banner: the event is one boolean for the whole subtree, so it says nothing about the
  files beside the evicted one, and a folder with one evicted file among ordinary ones is the ordinary case.

**Confirm waits, but only here.** With `cloudFolderMayHoldOnlineOnly`, `handleConfirm` holds on `onlineOnlyAnswer` (a
spinner rides inside the confirm button meanwhile) so a press landing mid-walk can't settle the question by luck. One
hit is the whole answer, so a progress tick releases it and a 50 GB folder needn't finish counting. If the answer says
online-only, the confirm is HANDED BACK rather than run: the button said "Move to trash" and now means a permanent
delete, and nothing undoes that one. An MCP `autoConfirm` goes through, having asked for the delete outright. Everywhere
else confirm stays exactly as instant as before.

**A handed-back press says so** (`handedBackForOnlineOnly` → `fileOperations.delete.cloudOnlineOnlyHandedBack`). The
dialog stays open and changes shape under the person's finger, so without a line saying what happened the press reads as
a dead button. It renders last in the body, directly above the button whose meaning changed, as a `role="status"` region
that announces without stealing focus, and the next press clears it. ❌ Not the `blockedReason` shape onboarding step 3
uses: that's a tooltip on a control the person isn't hovering, and it explains a press BEFORE it happens, while this one
explains a press that already landed.

❗ **An answer that never arrives means the TRASH.** A thrown IPC, a backend timeout, a scan that errored or was
cancelled, and `ONLINE_ONLY_ANSWER_TIMEOUT_MS` all leave the dialog on today's behavior, because attempting a trash and
getting the typed refusal is recoverable while a permanent delete isn't. The timeout is generous on purpose: a real walk
of a big tree legitimately takes a while, and giving up early runs the download this avoids.

Three things belong to the backend and must not be re-derived here (`write_operations/delete/cloud_trash.rs`, DETAILS §
"A trash of online-only cloud content becomes a delete"): which locations count, the all-or-nothing rule across a mixed
selection, and symlink resolution (`~/Dropbox` is a link into the same drive).

The question is asked for Shift+F8 too, not only F8: the dialog's own switch could otherwise flip a permanent delete
back to a trash and walk straight into that download.
