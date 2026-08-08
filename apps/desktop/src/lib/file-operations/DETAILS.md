# File operations details

The must-knows are in `CLAUDE.md`; per-dialog depth lives in each subdir's docs. This file holds only the umbrella-level
detail.

## Archive edits (`archive_edit`)

Copy/move/delete/mkdir/mkfile targeting a path INSIDE a zip run the backend's managed archive-edit op (an O(archive)
temp+rename rewrite), surfaced through the same transfer/queue UI as any write:

- **Routing.** Copy always goes through `copyBetweenVolumes` (backend resolves the archive dest), so it needs no
  special-casing. Move has a local same-FS fast-path (`moveFiles`) that would reject an archive-inner path, so
  `transfer-progress-state`'s `isVolumeMove` OR-s in `pathInsideArchive(destinationPath)` and `sourcePaths.some(...)` to
  force the cross-volume route for a move INTO or OUT of a zip — source and dest can share the parent drive's
  `volumeId`, so the id comparison alone misses it.
- **Op handle, not a path.** `create_directory`/`create_file` on an archive target return an operation id, and an in-zip
  rename starts an async op — the FE never treats these as a landed cursor target. The cursor lands via the durable
  `pendingCursorName` channel when the backing `.zip`'s live-watch refresh diff arrives (see the pane DETAILS). The
  create dialogs' return value is discarded either way (they forward the typed name), so no signature change was needed.
- **Permanent delete.** There's no Trash inside a zip (the backend rejects trashing an archive-inner path), so
  `openDeleteDialog` forces `isPermanent` + drops `supportsTrash` and passes `isArchive` for a source inside a zip;
  `DeleteDialog` then shows the archive warning banner and hides the "Move to trash" switch.
- **Presentation.** `archive_edit` is a `WriteOperationType` with the `file-archive` queue glyph (`operation-icon.ts`)
  and the "Editing archive" `queue.row.label` arm. It has no scan phase, so `TransferProgressDialog`'s `scanTitleMap`
  excludes it (the `scanTitle` derivation short-circuits for `archive_edit`).

## `TransferProgressReadout.svelte`

The dual-bar readout — a bytes bar and a count bar, each with its amount, percent, and rate, over one shared time-left
line — is ONE component rendered by both surfaces that show a running write op: `TransferProgressDialog`
(copy/move/delete/trash) and the operation queue's `QueueRow`. It owns layout only; the numbers are single-sourced by
`progress-readout.ts` (speed, ETA) and `$lib/units` (text), and the strings by the `fileOperations.transferProgress.*`
catalog keys.

- **Two densities, one layout.** Both label each bar "Bytes" / "Files" (or "Items" for trash); `compact` (a list row)
  differs only in taking the 4 px bar, a 64 px bar floor, and tighter gaps. Two unlabelled bars stacked in a queue row
  read as a puzzle, and the units in the amounts don't answer it fast enough, so the row pays the label column's width
  (`queue-window.ts`'s `MIN_WIDTH` carries it) rather than making the reader work it out. Nothing else differs, so the
  two surfaces can't drift apart in what they show.
- **Every readout cell is a fixed-width column**, sized in `ch` for the widest string it can hold ("999 GB / 999 GB",
  "(100%)", "999 MB/s"). This is the point of the component, not a detail: the bars' width then depends only on the
  window, and nothing shifts as digits come and go. The columns are `minmax(…, auto)` so a rarer outlier (a byte-scale
  pair) grows instead of clipping.
- **Live sizes are ROUNDED** (`<Size rounded>`, "7 GB" not "7.09 GB"), amounts and speed alike. A number that changes
  several times a second doesn't earn decimals nobody can read at that rate; a size column, where people compare and
  copy values, still gets them. Rounding is half-up, so a nearly-finished transfer can read "23 GB / 23 GB" — the
  percent beside it is what stays exact.
- **The time left has its own row**, spanning the grid and right-aligned. It keeps the bars wide, and lets an estimate
  firm up from "1h 8m left" to "56m 24s left" without moving anything above it. The row renders even while empty (a
  `:empty::before` no-break space), so the estimator warming up doesn't shove the rest of the dialog down.
- **The bar column has a min width** (80 px, 64 px compact), below which a bar reads as a smudge rather than progress.
  Whatever hosts the readout owes it that width, plus the `auto` label column in front of it: it's why the operation
  queue window's `MIN_WIDTH` is what it is (`queue/queue-window.ts`, which shows the per-locale label measurements) and
  why the progress dialog is 580 px wide.
- **The dialog has no "Copying" chip.** A phase banner renders for SCANNING only, where nothing else on screen says what
  the wait is for. During the copy the title says "Copying...", the bars are labelled, and a third copy of the word
  earned nothing but height.
- **A stalled transfer's notice displaces the time left**, in both surfaces, from the same `transfer/transfer-stall.ts`
  verdict. The dialog additionally shows the explanatory stall block underneath.
- **Speed and ETA describe a transfer that's moving**, so a caller passes `null` for all three while paused, and the
  cells go empty rather than freezing a stale number.

## Foreground-operation slot

`foreground-operation.svelte.ts` holds the id of the operation the foreground `TransferProgressDialog` is showing, so
main-window surfaces that report operations ambiently (the status corner's chip, the backgrounded-failure notice) can
skip the one the user is already looking at in full.

- `setForegroundOperationId(id: string | null)`, `getForegroundOperationId(): string | null` (reactive: reading it in a
  `$derived` / `$effect` re-runs on ownership change), `clearForegroundOperation(id: string)`.
- `transfer-progress-state.svelte.ts` claims the slot right after the start command's response lands (after the
  `destroyed` bail-out — a dialog that's already gone owns nothing) and releases it in `destroy()`, `handleQueue()`, and
  `handleAutoQueued()`. `destroy()` is the catch-all for completion, cancel, error, and any other unmount; the two queue
  paths release EARLY because `onQueue` is optional, so the modal may stay mounted after the handoff.
- The delete/trash path comes free: `DeleteDialog` drives the same state machine.
- A SECOND slot, `setForegroundFailureId` / `getForegroundFailureId`, names the failure the foreground
  `TransferErrorDialog` is showing. It exists because of ordering, not taste: the progress dialog releases the first
  slot as it unmounts, and the backend's retained failure row only reaches the snapshot after that, so a single slot is
  already empty when the ambient surfaces get their chance to double-report.
  `pane/dialog-state.svelte.ts::handleTransferError` reads the first slot while the dialog still holds it and claims the
  second; `handleTransferErrorClose` releases it and calls `dismissFailedOperation(id)`, so a failure the user has read
  and closed leaves nothing behind in the queue. Full reasoning: `apps/desktop/src/lib/status-corner/DETAILS.md` § "Why
  the foreground handover needs two slots". The slots themselves are pinned by `foreground-operation.svelte.test.ts`,
  the handover across them by `../file-explorer/pane/dialog-state.failure-handover.svelte.test.ts` (which simulates the
  progress dialog's unmount, so a claim made too late fails there).

Decisions:

- **A module-scoped signal, not a prop.** Prop-drilling would run `transfer-progress-state` → `TransferProgressDialog` →
  `DialogManager` → `DualPaneExplorer` → `routes/(main)/+page.svelte`: four hops of a value nobody in between cares
  about.
- **Main-window-only by construction.** Module scope is per-webview, so the queue window can't see (or accidentally
  write) this slot.
- **One slot, not a set.** Exactly one foreground progress dialog exists at a time; a second operation either replaces
  the dialog or auto-queues behind a busy lane. If that invariant ever breaks, reconsider the invariant rather than
  widening the slot.
- **Ownership-checked release.** `clearForegroundOperation(id)` no-ops unless `id` still owns the slot, so a dialog
  tearing down after the next one claimed it can't silence the new operation.
- **The unclaimed window is announced, not hidden.** The operation registers (and reaches the main window's store, and
  can already emit) before the start command's response returns, so there is a real span where a live operation has no
  name here. `beginForegroundClaim()` / `endForegroundClaim()` / `isForegroundClaimPending()` mark it:
  `startOperation()` brackets its dispatch, settling in a `finally` so the id landing, the abandoned-dialog bail-out,
  and a thrown command all release it. Consumers that only DISPLAY can ignore it and ride out a frame's flash (the
  corner chip does, on its settle delay); a consumer that DECIDES must defer while it's pending, because both wrong
  answers are expensive — see "Conflict prompts" below.
- **A counter, not a flag.** A dispatch can still be in flight when the next dialog begins its own claim (Escape during
  dispatch abandons the first without ending it any sooner), and a flag would let the first one's teardown clear the
  second one's claim.

## Conflict prompts for operations with no dialog

`operation-conflict.svelte.ts` (+ `operation-conflict-rules.ts`, `OperationConflictDialog.svelte`) is the main window's
answer to a `write-conflict` that no progress dialog is listening for.

The bug it exists for: `TransferDialog`'s upfront check is one destination listing, top level only, and a destination
folder that already exists MERGES — so a clash on a file inside it can't be known before the operation starts. Under
"Ask for each" the backend emits `write-conflict` and parks the operation on a oneshot. Press Queue and the progress
dialog (the app's only listener) unmounts, and the operation waits for an answer nobody can give.

The host is started and stopped by `routes/(main)/+page.svelte` next to the failure watch, listens for the life of the
window, and on a conflict nobody owns: pauses, raises the main window, and prompts. The dialog is chrome around the same
`TransferConflictDialog` the progress dialog embeds, resolving through the same
`resolveWriteConflict(operationId, resolution, applyToAll)` and cancelling through the same
`cancelWriteOperation(operationId, rollback)`.

- **Ownership is `conflictOwner(operationId, foreground)`**, returning `here` / `foreground` / `unknown`. `unknown`
  means a dialog is mid-dispatch: the empty slot proves nothing, so the event is HELD and re-decided when the claim
  settles (an `$effect` reading `isForegroundClaimPending()` unconditionally). Guessing costs a double prompt or a
  wedge; deferring costs milliseconds. ❌ Not a settle delay — that's the chip's tool, and the chip can afford to be
  late.
- **`operationsToPauseFor(conflictOperationId, rows)`** is how wide the pause is: today every `running` id, the asking
  one included. `queued` and `paused` rows stay out, and the ids are remembered so the answer resumes exactly them. ❌
  Never `pauseAll()` / `resumeAll()`: resuming everything restarts an operation the USER paused by hand.
- **The asking operation is paused too**, though the backend already has it parked on the oneshot. The pause gate is a
  flag read at the next between-files boundary and the operation isn't at one, so it costs nothing and buys honesty: the
  queue window and the corner chip both read `paused`, instead of one row claiming to run with a frozen bar.
- **Resolve lands before resume.** A resolve that doesn't land leaves the prompt up and everything paused, which is the
  honest state for an unanswered question. The resolved operation may park for the moment before its resume arrives;
  parking between files is what pause is for, and cancel still wins over both.
- **One prompt at a time, in arrival order**, resuming only after the last. The backend serializes prompts within one
  operation (`conflict_dispatch_lock` plus a single sender slot), so the queue holds at most one entry per operation; a
  second event for one that's already queued replaces it, because `resolve_write_conflict` is keyed by operation id and
  an answer lands on whatever that operation is parked on now.
- **An operation dying mid-prompt** is covered three ways: the prompt's Cancel, `reconcileConflictPrompts(rows)`
  dropping an entry whose operation left the snapshot (a queue-window cancel, a failure), and either of those releasing
  the hold. An entry is only droppable once its operation has been SEEN live: the rows arrive on their own stream, and
  "not there yet" must not read as "gone".
- **The main window closing isn't a case.** It quits the app (`lib.rs`'s `CloseRequested` handler), so there's no state
  where operations run with no window to host the prompt, and no need for a fallback host in the queue window.
- **It raises the main window** (`getCurrentWindow().setFocus()`, self-focus — cross-window `setFocus()` doesn't
  reliably raise on macOS), once per run of prompts, skipped under `isE2eRun()`. The path into this bug ends with the
  queue window in front; a prompt nobody can see is the same wedge with more code. That's the opposite of the failure
  toast's reasoning, and deliberately: a settled failure asks for no decision, a conflict holds a transfer still.
- **No `onclose`, so no × and no Escape.** Every exit is a decision about the user's files, and the conflict body's own
  Cancel / Rollback row is the way out. ⚠️ It follows that E2E's `ensureAppReady` Escape can't clear this dialog; only a
  real background conflict raises it, so no automated run should meet one.
- **It can stack over an open progress dialog**, when a backgrounded operation clashes while a new one is running in the
  foreground. That's supported rather than avoided: `trapFocus` hands enforcement to the most recently mounted trap and
  back on close, and this dialog is mounted after `DualPaneExplorer` so it paints on top. Deferring the prompt until the
  foreground dialog closed would be a softer version of the wedge this fixes.
- **`rollbackUnavailable` comes from the snapshot's `supportsRollback`**, which is more than the progress dialog knows:
  a CROSS-volume move can't roll back either, and that dialog still derives the same-volume case itself.

## `scan-throughput.ts`

`ScanThroughput` turns scan-event tally deltas into a calm `filesPerSecond` / `bytesPerSecond` readout over a rolling
window (default 2 s, constructor-overridable). It exists because the backend `EtaEstimator` only covers write phases,
not the scan-preview pipeline, so the frontend computes its own scan-phase rate. The algorithm is deliberately tiny: a
number for the user to read, not a forecast. It returns nulls until two samples have arrived, drops samples older than
the window (always keeping the most recent so a long pause still has a baseline), clamps negative rates to zero, and
resets cleanly between scans.
