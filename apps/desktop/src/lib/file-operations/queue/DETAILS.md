# Operation queue window — details

Depth for `CLAUDE.md`. The window is the frontend of the operation-queue + pause feature; the backend lives in
`crates/cmdr-operations/src/` (DETAILS § "Operation manager").

## Why it's the "operation queue", not the "transfer queue"

The window lists deletes, trashes, renames, folder and file creates, and archive edits, not only transfers, so the old
name was wrong on the facts. "Transfer" also already means copy-or-move in the code (`transfer/`, the transfer driver,
`TransferProgressReadout`), and the old name made one word mean two different things at two altitudes: a reader could
reasonably wonder whether a delete belonged in a "transfer" queue. The new name pairs with "Operation log" as present
tense versus past tense, and the two sit next to each other in the same View block.

Scope of the rename: user-facing copy ONLY. The code identifiers stay as they are (`operations-store`,
`OperationSnapshot`, `operations-changed`, `openQueueWindow`, `queue.show`, `QUEUE_SHOW_ID`, the `/queue` route, the
`queue.*` message namespace, the `transfer-queue` toast group). The label uses the category noun; body copy stays
concrete, which is why `queue.empty.body` still reads "Copies, moves, and deletes show up here while they run" rather
than abstracting into "operations".

## Why a hard window

A copy/move/delete today shows only a modal progress dialog: it blocks the main window and there's no single place to
manage several operations at once. The queue is the "stop blocking me, let me keep working" surface, so it must be a
real OS window you can leave open in the background — not a sheet, not a panel inside the main window. It's built on the
exact Settings-window pattern (singleton, vibrancy, overlay title bar, position via `#lib/window-positioning`) so it
reads as a first-class macOS utility window, consistent with Settings and Keyboard shortcuts.

## The two-stream model

The window renders from `createOperationsStore()`, which merges:

1. **`operations-changed`** (`onOperationsChanged`): the thin registry snapshot the backend emits whenever an operation
   is registered, admitted, paused, resumed, or settles. Payload is `{ operations: OperationSnapshot[] }`, where each
   `OperationSnapshot` is `{ operationId, operationType, status, source, destination, rollbackSupport, error, items }` —
   membership, lifecycle status, whether the row may offer Rollback, (on a retained failure only) the typed
   `WriteOperationError` that stopped it, and what the row says the operation acts on (§ "Row layout"). No 200 ms
   progress. This decides which rows exist and each row's status.
2. **`write-progress`** (`onWriteProgress`, the existing per-file stream): drives the live per-row progress bars. The
   store keys the latest `WriteProgressEvent` by `operationId`.

On every snapshot tick the store prunes the progress map to the new membership, so a finished op's bar can't outlive its
row, and the map can't grow unbounded. Progress for an op not (yet) in the snapshot is ignored — the snapshot is the
membership source of truth.

This split keeps `operations-changed` cheap (no progress fattening it every 200 ms) while still giving each row a live
bar — the design the plan mandates under "subscribe, don't poll" and "thin snapshot".

**A paused operation opens expanded.** A parked operation emits no `write-progress`, so a window opened during the pause
would hold no tick for it: no readout, no tree, one folded line until somebody pressed Resume. `init()` therefore reads
`latest_operation_progress` once, after the snapshot seed, and fills in each row that no live tick reached meanwhile (a
live one is newer). It's a one-time read at mount, not a poll; the stream takes over from there. The session gets no
copy, so a freshly opened paused row shows no ETA until it moves again.

### What the store may hold, and what belongs to the session

Everything in the store is STATELESS: which operations exist, each one's lifecycle status, and the latest tick each one
emitted. A second reducer over the same stream is harmless, because two copies of one event object can't disagree.

Every ESTIMATE is stateful and lives on the operation's session instead (`../operation-session/CLAUDE.md`), which each
row binds to with `bindOperationSession(() => snapshot.operationId)`:

- `session.etaSecondsDisplay`, the backend's ETA through the one smoother this operation has in this window. It stays on
  a parked row: how much longer this has to run doesn't stop being true because somebody is deciding something.
- `session.bytesPerSecondDisplay` / `filesPerSecondDisplay`, the backend's rates, dropped while a person is deciding (a
  row that isn't moving has no speed to report).
- `session.scan`, the walk's files/s and bytes/s, which the backend doesn't emit while it counts.

Two estimators fed identical samples from identical starting points would agree; the divergence comes from one starting
later, which is what any view attaching to an operation already in flight does. So the rule is positional rather than
stylistic: ❌ a smoother in the store is a second layer by construction, and `queue-row-session.svelte.test.ts` counts
constructions to keep it that way.

⚠️ Bind on the id as a VALUE, never on the row object. `operations-changed` rebuilds every row whenever anything in the
registry moves, so a binding that re-ran per row object would hand the operation a fresh smoother mid-transfer, every
time some unrelated operation started or finished. `bindOperationSession` derives the id string for exactly that reason.

### Why rows read the snapshot status

The bar-is-moving truth is the snapshot `status`, the backend's `LifecycleStatus`: only `'running'` shows the spinner
and an animated bar; `'paused'` shows a static bar and the Paused label. Every surface reads that one enum, so the
corner chip, the queue row, and the progress dialog cannot disagree about the same operation.

## Store public API (the progress-dialog reuse contract)

`createOperationsStore()` returns:

- `operations: OperationRow[]` — reactive; each `OperationRow` is
  `{ snapshot: OperationSnapshot, progress: WriteProgressEvent | null }`. Ordered as the backend emits them.
- `rollbackSupport` on each snapshot — whether cancelling can also undo what the op wrote (and if not, why), decided per
  spawn path in the backend (`crates/cmdr-operations/DETAILS.md` § "Rollback availability"). The row shows Rollback on
  exactly that set (`'supported'`), so this window and the progress dialog can't disagree about which operations are
  reversible.
- `hasRunning: boolean` — any op with `status === 'running'` (gates "Pause all").
- `hasPaused: boolean` — any op with `status === 'paused'` (gates "Resume all").
- `init(): Promise<void>` — subscribes to both streams, then seeds from `list_operations`. Subscribe-before-seed so a
  tick during the await isn't missed; the seed only applies if no snapshot tick beat it. Failures `log.warn` (perms /
  IPC), never throw.
- `dispose(): void` — drops both listeners. Call on window teardown.
- `_testApplySnapshot` / `_testApplyProgress` — test seams that drive the reducers without a live backend.

- `failureCount: number` — how many rows are retained failures. Gates the toolbar's "Dismiss all" (offered only past
  one) and feeds the corner chip's failure state.

Three typed set tests sit beside the factory, all module exports, all sets rather than substring tests:

- `isTerminalStatus(status)` — `done` / `cancelled` / `failed`: the op has stopped, whatever the outcome.
- `isHiddenSettledStatus(status)` — `done` / `cancelled`, the settled statuses the window drops. Separate from
  `isTerminalStatus` because the two stopped meaning the same thing when failures became retainable: a `done` op has
  nothing left to say, a failed one has the only thing the user still wants.
- `isInstantOperation(type)` — `rename` / `create_folder` / `create_file`, the metadata ops that emit no
  `write-progress` at all. This window still lists them (it promises completeness); ambient surfaces like the corner
  chip skip them, since there's never a bar to draw and they're gone before the eye lands on them.

## `queue-backlog.ts` — the word on the progress dialog's button

`hasOtherQueuedWork(rows, selfOperationId)` answers one question for `TransferProgressDialog`: is there anything in the
queue besides the operation the dialog is showing? No → the button reads "Background" (you're not queueing behind
anything, you're sending this out of sight); yes → "Queue". The click, the tooltip, and the F2 binding are identical
either way; only the word changes, and the accessible name with it (the button has no `aria-label`).

- **It reads the MAIN window's store** (`main-window-operations.svelte.ts`), the same live rows the corner chip reads,
  through a `$derived` in the dialog. No new store, no new event, no polling, and the word follows the queue while the
  dialog is open. The label flipping under the cursor when an unrelated operation finishes is accepted, deliberately:
  the alternative is a word that lies until you close the dialog.
- **Three gates, each one a wrong word if it's missing.** The dialog's OWN operation is excluded (it's in the queue for
  as long as the dialog is up, so counting it pins the label to "Queue" forever); instant ops are excluded via
  `isInstantOperation`, since a rename is over before the word could settle; and only live work counts, via
  `!isTerminalStatus`.
- **Decision: a retained failure does NOT count.** It's a notice, not work you'd wait behind — nothing about it delays
  the operation in front of you, and "Queue" would promise a queue that isn't there. `!isTerminalStatus` covers it along
  with `done` and `cancelled`, so the rule is one positive definition (`queued` / `running` / `paused`) rather than a
  list of exclusions that a new status could slip past.
- **The self-exclusion needs the dialog's id**, which `transfer-progress-state.svelte.ts` exposes as `operationId` (null
  until the start command answers). The button doesn't render before then (`canPauseOrQueue` requires the id), so the
  null case can't be seen; the function still handles it by excluding nothing.
- Pinned by `queue-backlog.test.ts` (every gate) and `../transfer/TransferProgressDialog.queue.test.ts` (the live flip
  through a real store instance fed by the same `operations-changed` stream).

## Show — handing a row back to the main window

A running or paused row offers **Show**, which puts that operation in the main window's progress dialog: full bars, the
smoothed ETA, and the same Pause / Cancel / Rollback the row has. Closing the dialog hands the operation straight back
here, still running, exactly as Background does. It is not a command on the operation: nothing about it changes, only
where it is shown.

- **Only the id crosses.** The row emits `foreground-operation` (`#lib/tauri-commands/dialog-events.ts`), and the main
  window resolves that id against ITS OWN `operations-changed` snapshot
  (`../foreground-request.ts::adoptedOperationFor`). The registry row is the single source of truth about an operation,
  and both webviews already receive it, so nothing about the operation travels on the wire. Fold this window into the
  main window as a popup one day and the emit collapses to a direct call with the same argument.
- **The queue window holds `core:event:default`**, so no capability change was needed. It does NOT raise the main
  window: the main window focuses ITSELF in its listener, whatever the verdict, because a refusal the user cannot see
  reads as the button doing nothing.
- **Offered only where a dialog would have something to show.** Running or paused, and not an instant op. ❌ Never on a
  `queued` row: an operation waiting for a lane has no progress to put in the dialog's bars, so Show would open an empty
  one. ❌ Never on a failed row: there is nothing left to watch, and its reason is already on the row in full.
  - ❌ Don't write that the dialog would bounce a queued operation back: auto-queue is a dispatching view's decision
    only, and an adopted one stays put (`../transfer/DETAILS.md` § "Auto-queue surfacing", pinned by
    `../transfer/transfer-progress-state.ownership.svelte.test.ts`).
- **Every way Show can open nothing leaves a warn line** in the release log: the row logs a rejected emit, and the main
  window logs a miss in its snapshot (with its row count) or a missing explorer. A refusal already logs and toasts. A
  CI-only Linux failure once left no trace at all, because the miss logged at info, which release builds drop.
- **The E2E names the hop a failed Show died on.** That Linux failure (CI runs 37450234515 and 37763174127, both
  attempts each) came back with NONE of those warns, and never reproduced locally (40 repeats of the spec, three full
  Linux suites, two of them under CPU contention, 2026-10-08). So `operation-queue.spec.ts` puts a throwaway listener in
  each window and reads the main window's app listener out of Tauri's listener table; a failure now says whether the
  request left the queue window, reached the main webview, and found a live handler there.
- **The main window can refuse.** Its dialog slot is single-occupancy; a refusal comes back as a toast there, next to
  the dialog that refused. Reasoning and the invisible-occupancy hazard: `../../file-explorer/pane/DETAILS.md` § "Birth
  context".
- Pinned by `QueueRow.svelte.test.ts` (which statuses offer it, and that the click asks for that row's own operation)
  and `../foreground-request.test.ts` (the lookup, including the ordinary miss).

## A scanning row

An operation is registered the moment the user confirms, which is before its `TransferDialog` scan preview has finished
walking (`crates/cmdr-operations/src/scan_bridge.rs`). So a row can be `running`, or `queued` behind a busy lane, while
its `progress.phase` is still `scanning`.

- **No dual bar.** `filesTotal` and `bytesTotal` mean "what the scan concluded", and during the scan there is no such
  thing: both stay 0, and the index-derived expectation rides `expectedFilesTotal` / `expectedBytesTotal` as the hint it
  is. Letting the expectation populate the totals is the tempting shortcut — it turns the bars on — and it is wrong
  twice: the bar would be measured against a guess, and the number would jump when the real totals landed.
- **The compact `ScanPhaseBody` instead**, the same component and the same catalog keys the progress dialog uses at
  comfortable density, so the two surfaces can't drift on what a scanning operation looks like. It drops the "From:"
  line and the current dir/file boxes, which the row already says or has no height for.
- **With a real rate.** The backend emits none during a scan, so the files/s and bytes/s come from the session's
  `ScanThroughput` over the ticks the row is already rendering. It needs two samples, so a scan opens without one.
- **`queued` rows render it too.** `showReadout` requires `isRunning || isPaused`, so without this an operation admitted
  behind another on the same lane shows "Waiting" over an empty row for its whole scan — on a busy lane the common case,
  and it reads as a hung queue. "Waiting" over a moving file count is exactly what is happening.
- **Pause yes, Rollback no.** Pause parks the WALK (`crates/cmdr-operations/src/scan_bridge.rs` § `ScanPause`), so the
  row offers it during the scan exactly as it does during the write, and offers Resume on a scan paused from "Pause all"
  — without that, such a row's only way back was Cancel. A scanning operation has written nothing to reverse, so
  Rollback stays away; `rollbackSupport` stays `'supported'` throughout, since it is a promise about the OPERATION and
  the phase is what decides which controls make sense now. A PAUSED scanning row drops `ScanPhaseBody`'s spinner and its
  rates: the spinner is the claim that the walk is moving.
- **The status column still says "Running"**, and that is deliberate. `queue.row.status` is a `select` over the
  LIFECYCLE status, which genuinely is `running`; the readout names the activity, and the scan-phase line already says
  "Counting…" in the user's language. A "Scanning" arm would mix two axes into one column and would need a `phase` input
  the row's message doesn't take. The clash wait below is the case that does earn the column, and § "A row parked on a
  clash" is where the line between them is drawn.

## Compression phases

A compression row uses the same operation session and shared readout rules as its foreground dialog. `Compressing` shows
uncompressed source-byte and source-entry progress. A remote fallback then starts a separate `Transferring` bar whose
total is the completed ZIP size; the phase change resets the session's ETA smoother. `FinishingCompression` and
`FinishingTransfer` have no measurable denominator, so the row replaces the dual readout with a spinner and the
phase-specific label. It never falls back from zero bytes to stale file totals, and it never leaves a 100% bar visible
while close or publication continues. `progress-readout.ts` owns both the label map and indeterminate classifier used by
this row, the dialog, and the corner chip.

## A row parked on a clash

An operation that hits a name clash stops and waits for an answer, and its lifecycle status stays `running` throughout
(a clash pauses nothing). So the row read "Running" over a bar at 0%, which is precisely the state somebody opens this
window to explain. The row says `queue.row.statusAwaitingAnswer` ("Needs your answer") in the STATUS column instead,
with `circle-alert` in place of the spinner (nothing is turning) and `--color-warning-text` (warmer than a running row,
and not the error red a failure owns, because nothing has gone wrong).

**The column names the lifecycle, EXCEPT where the operation has stopped or reversed and nothing else on the row says
so.** That is the whole rule, and it admits exactly two exceptions today: a rollback, whose only other signal is a bar
running backwards, and this. A scan is neither — it is moving, and its own line says "Counting…" — which is why the
column stays "Running" there. ❌ Don't grow a third exception for something the readout already names.

**Not the readout, for the same reason.** The readout answers "how fast, how much left", and a stall notice annotates a
running transfer that has gone quiet. This operation is not slow; it is finished waiting for the machine and waiting for
a person, and `transfer-stall.ts` deliberately stays silent for a DELIBERATE wait (calling one a stall trains people to
ignore the warning). The status column is also what a person reads DOWN when several operations are listed and one of
them isn't moving.

**Where it comes from.** `session.awaitingAnswer` — the backend's own classification
(`activity.waitingOn === 'conflict'`), reaching every operation including a local copy, which keeps no in-flight table
(`crates/cmdr-operations/DETAILS.md` § "Parking on a person"). ❌ Never the row's own `progress.activity` test and ❌
never `session.conflict`: the first would let two views word the same wait differently, and the second is this window's
own copy of a prompt it may never have been sent.

**The tooltip says where to answer** (`queue.row.awaitingAnswerTooltip`), because the prompt is in the MAIN window and
this row is in another one, quite possibly on top of it. A pause needs none of this: it changes the lifecycle status, so
the column already says "Paused" on its own.

## Retained failures

The backend keeps a bounded list of failed operations and carries them on the same `operations-changed` snapshot, each
with its typed `error` (`crates/cmdr-operations/DETAILS.md` § "Retained failures" owns the mechanism). This window is
the durable surface for them: it survives a dismissed toast, a closed window, and a reopen.

- **The row keeps its place, so the SELECTION has to let go of it.** `routes/queue/+page.svelte` prunes `selectedIds`
  against the rows that are still `!isTerminalStatus(...)`, not against mere membership: a failure stays in the list and
  drops its checkbox, so a tick the user made while it was running would be unclearable by hand, leaving "1 selected" on
  screen and "Cancel selected" enabled over an operation the backend no-ops. Pinned by
  `apps/desktop/src/routes/queue/queue-selection.svelte.test.ts`.
- **The row.** No pause, cancel, rollback, or select checkbox — there's nothing live left to act on. A `triangle-alert`
  glyph and "Couldn't finish" in `--color-error-text`, Retry (below), "Send error report…", and Dismiss, and the reason
  on line 2 where the readout would be. The report (`failureIncidentFor`) opens in the MAIN window, which hosts the
  dialog (`#lib/error-reporter/DETAILS.md` § Report offers); a stalled row's readout line gets the slow-report link the
  same way.
- **Retry starts it again, from this window.** Shown when the snapshot says the backend kept a way to start the
  operation again (`canRetry`: every cross-volume copy and move) AND the failure is one the error dialog would offer
  Retry for too (`offersRetry` in `../transfer/transfer-error-messages.ts`, the one rule both share). The click calls
  `retry_failed_operation(id)`: the backend starts a NEW operation through the routing the original took, resetting what
  the foreground's `retryPropsFrom` resets, and dismisses this row, so the new one takes its place (in the background:
  this window is no birth context). It sits in the row's ← / → walk like every row button (`tabindex={-1}`, § Keyboard).
  A refused start (the volume is still gone) leaves the row and logs. Why the recipe lives in the backend:
  `crates/cmdr-operations/DETAILS.md` § "Retained failures".
- **Why red here and amber in the corner chip.** Severity follows the THING, not the surface. This row names the
  operation and prints its reason, so it carries the full weight, same as the failure toast. The chip names nothing (a
  count and a glyph), so it stays `--color-warning-text`: a pointer, not a verdict.
- **The reason.** `failure-reason.ts` maps the snapshot onto `../transfer/transfer-error-messages.ts`. It owns exactly
  one decision the pipeline can't make: a snapshot carries the WIRE operation type (`archive_edit`, `create_folder`, …)
  while the `errors.write.*` catalog only phrases `copy` / `move` / `delete` / `trash`, so a `Record` keyed by every
  wire type maps the rest onto the copy arms. A cast would resolve a missing catalog key at runtime instead.
- **Untruncated, on purpose.** The queue promises completeness, so the explanation AND the suggestion both render in
  full, wrapping mid-token (an interpolated path can be arbitrarily long). The main window's toast is the surface that
  abbreviates; it points here for the rest.
- **Dismissal is explicit, always.** Per-row Dismiss → `dismiss_failed_operation`; a Retry that started; the toolbar's
  "Dismiss all" (shown only when `failureCount > 1`) → `dismiss_all_failed_operations`; and closing
  `TransferErrorDialog` for a foreground failure, which the user has by definition just read
  (`apps/desktop/src/lib/status-corner/DETAILS.md` § "Why the foreground handover needs two slots"). Nothing else drops
  a failed row — the whole feature exists for the user who was away from the keyboard.
- **The main window's half of this** (the persistent failure toast and the corner chip's failure state) lives in
  `apps/desktop/src/lib/status-corner/CLAUDE.md`. Both render `failure-reason.ts`, so the three surfaces can't describe
  one failure three ways.

Every opener goes through `openQueueWindow()` and reads this same store. Don't fork a second opener or store.

## Backgrounding never opens the window

The progress dialog's Background / Queue (and its F2), the auto-queue, and a setup dialog's F2 all leave the window
shut. They show `queue-handoff-toast.ts`'s toast instead: the sentence saying where the job went plus "Show in operation
queue" (`QueueHandoffToastContent.svelte`), and the status corner's chip carries the job from there. Every opener is a
person asking to look (the menu item, the corner chip, either toast's button), so the window always takes focus.
**Decision/Why:** an unfocused window still lands over the panes (macOS raises a new window even when it isn't key),
hiding the files the person went back to, and "next to the main window" has no room when it's maximized or in full
screen. Backgrounding means "don't make me watch"; a window appearing says the opposite (2026-10-11, David's call; it
also made PR #390's "F2 closes the queue window" moot).

**One keypress, one toast.** The handoff toast's id is keyed to its operation (`queueHandoffToastId`), and whatever
later speaks for the job's ending takes its place: `raiseCompletionToasts` in `dialog-state.svelte.ts` (a trash's Undo
toast) and the failure toast (`operation-failure-watch.svelte.ts`) both call `dismissQueueHandoffToast` first, and
`showQueueHandoffToast` skips a job whose main-window row already reads `failed`. A job that ends quietly (copy, move,
compress: no completion toast in the background) keeps its handoff toast, its only word. **Why:** a plain local trash
finishes about as fast as the handoff toast appears, so F2 left "Sent to the background" stacked beside the Undo toast,
stale the moment it showed (2026-10-11).

## The main window's instance

`main-window-operations.svelte.ts` holds a second instance of the same factory, owned by `routes/(main)/+page.svelte`
(`initMainWindowOperations()` beside `initIndexState()`, `destroyMainWindowOperations()` in `onDestroy`). It exists so
main-window surfaces can read live operation state; the first consumer is the status corner
(`apps/desktop/src/lib/status-corner/CLAUDE.md`).

- `initMainWindowOperations(): Promise<void>` — idempotent, never throws. `destroyMainWindowOperations(): void` — safe
  without an init, twice, or mid-init.
- `getMainWindowOperations(): OperationsStore | null` and `getMainWindowOperationRows(): OperationRow[]` (empty before
  init) are the read seams.

Decisions:

- **Two instances, not shared state.** Each window is its own webview, so they can't share a store even in principle.
  Both subscribe to the same app-wide `payload.emit(app)` streams and seed from the same `list_operations`, so the
  backend stays the single source of truth. No new event, IPC command, or polling was added for the main window.
- **A fresh instance per init, never a revived one.** `dispose()` latches the store's `disposed` flag: re-initing the
  same object would unsubscribe itself inside `init()` and silently render nothing after a remount or HMR pass.
- **The instance holder is `$state.raw`.** The store is a getter-bearing object that must not be deeply proxied; only
  the null → instance swap needs reactivity, so a consumer that renders before `init()` resolves re-renders when the
  instance lands.
- **Cost.** Two idle listeners on an empty queue; one small object per 200 ms progress event during a transfer, which is
  what the queue window already carries. No memoisation until something measures.

## Row layout

A row's actions are Pause/Resume, Cancel, and — on a reversible op only — Rollback, all issued on the row's own session
rather than handed up to the page: which way Pause goes is decided from the lifecycle status the session already holds,
and the guards it carries are shared with every other view of that operation, so a Cancel pressed here is visible to
whatever else is watching. The page keeps the fleet actions (Pause all, Resume all, Cancel selected) and Dismiss, none
of which is a command on one operation. Rollback is styled `danger` exactly like the progress dialog's, since the same
click stops the same operation, and its tooltip splits by what the reversal DOES the same way the dialog's does
(`inFlightRollbackTooltipKey`): a move's says the files travel back, never that they're deleted. Rollback hides again
once the op IS rolling back, which the row reads off the live `write-progress` phase: rollback is an `OperationIntent`,
so the lifecycle status stays `running` throughout, and the status cell shows "Rolling back..." instead.

It also hides once a local cross-FS move reaches its `deleting` phase, where every file has landed and the engine has
committed, so a reversal can no longer carry anything home (`reversalWindowClosed` in `../reversal-wording.ts`, shared
with the progress dialog). The row HIDES rather than blocking with an explanation, as it already does during a scan: a
compact list of live operations is not where a reason gets read, and the dialog one Show away is the surface that
explains itself. Same verdict either way, so the two can't contradict each other.

The summary (`QueueRowSummary.svelte`, wording in `row-items.ts`) says what the operation acts on and where it goes:
"Copying 2 folders and 1 file Dropbox → cmdr-s3-test/target/folder", from the snapshot's `items` (backend
`crates/cmdr-operations/DETAILS.md` § "What a row says it acts on").

- **What it acts on is always on display** (`flex-shrink: 0`), because it's what tells two copies to different places
  apart. "2 folders and 1 file" when the starter said which is which, "3 items" when it didn't (MCP, compress). Its
  tooltip lists the first 20 names, then "… and 1,980 more": the names are known at registration, so it costs nothing.
- **The source folder is its last segment**, the full path on hover, and gives way first (capped at 40% of the line).
- **The destination is elided in the MIDDLE** (`useShortenMiddle`, breaking at `/`), so both the drive and the last
  folder stay readable; the full path is on hover and in an `sr-only` twin, since the elided span is `aria-hidden`.
- **A delete or trash has no arrow**, so its folder takes `queue.row.itemsInFolder` ("Deleting 3 items in Downloads").

A row without `items` (a rename, a create, a reversal, an edit inside a zip) keeps the older shape: `basename(source)`,
then an arrow and `basename(destination)` when there is one, with the full paths in tooltips. A REMOVAL reversal
(undoing a copy, a new file or folder, or a compress) has no destination, so no arrow renders and its single folder name
lands right against "Deleting what it created", reading as though that folder is the thing about to go. So that one
shape wraps the name in `queue.row.reversalInFolder` ("in {folder}"), which puts it back where it belongs: the reversal
is clearing files INSIDE it. Only that shape needs it; every other row already has the arrow to say which way things
move, and "Putting files back in report.pdf" would be a fresh nonsense. The Rust side of the summary is `InverseSummary`
/ `summarize_inverse` (`crates/cmdr-operation-log/src/rollback.rs`), which fills `from` for a removal and both `from`
and `to` for a restore.

A row is a five-column grid whose chrome (select, type icon, source→dest summary, status, actions) sits on line 1, with
the shared `../TransferProgressReadout.svelte` spanning line 2 from the summary column to the end. The readout gets the
full row width rather than a slot beside the buttons because its columns are fixed-width: sharing a line with the status
and two buttons would have pushed the window's minimum width past 700 px for the bars to survive. A row with no
`write-progress` to show — a queued op, or an instant `rename` / `create_folder` / `create_file` — renders line 1 only.
A running transfer adds a third line, its live tree (below).

## The inline tree

A running or paused copy, move, or compress past its scan shows `../transfer-tree/TransferTree.svelte` on a third line,
in the readout's columns (`3 / -1`), six rows tall at most and as few as it has (a single file is one row). Which rows
get one is `row-tree.ts` (`rowTreeOperation`), over the progress dialog's own rule (`treeOperationFor`), so the two
surfaces agree; a queued row has written nothing and a failed one has nothing to watch, so neither gets one. The tree
binds nothing of its own: it asks `get_transfer_tree_view` for this row's operation and pulls on the session's tick
(`session.progress`, through the row's `bindOperationSession`).

- **It reads as this row's, and the next row as the next item, by indent alone.** The tree starts under the label, so
  its gutter (the checkbox and type icon columns) stays empty; a waiting row below brings its checkbox and icon back to
  the left edge. No card or divider was needed.
- **Always open with one tree in the list.** Once two rows show one (`treesCollapsible`: the queue runs one operation
  per lane, so copies on disjoint drives run side by side), each gets a chevron `Button` at the end of its actions
  (`aria-expanded`, `aria-controls` the tree's cell, one constant name, "Files in this operation"), open by default.
  Shut is the row's own `$state`; with a lone tree left, it's open again whatever that says. The toggle is last in the
  row's ← / → walk, with no key work.
- **The window's size** (`queue-window.ts`) leaves room for one running row with its tree plus a few waiting rows; more
  scroll. Its width leaves the summary line room to name the items and an elided destination.

**Decision: focus reaches the tree by Tab from its row, and Escape hands it back.** **Why**: the tree is a widget of its
own (arrows, Home / End, Left / Right, F), so it can't share keys with the grid: ⌫ inside it must not cancel the whole
copy, and ↓ must walk the files, not the rows. So:

- Tab from the grid's cursor row lands in its tree (the tree's own tab-stop row; Follow takes no Tab, F reaches it).
  Only the cursor row's tree is a Tab stop (`tabbable`), so the grid stays one stop plus that tree; Shift+Tab goes back.
- Inside, the tree owns every key, and the page's grid hears none (`gridHears` skips `[data-queue-tree]`).
- Escape (the `queue.close` command, read by the row's `leaveTree`) returns focus to the row instead of closing the
  window; from the row, Escape closes it as before.
- A click in a row's tree focuses its treeitem, and the focus bubbling up moves the cursor to that row.
- F anywhere in a row (the row, its buttons, its tree) resumes that row's paused tree: the row forwards its keydowns to
  `handleFollowKey` (`../transfer-tree/DETAILS.md` § "The F key"). The grid has no F of its own.

## Keyboard

The list is one Tab stop with a row cursor, so the window works without the mouse (#397). The page owns the cursor and
the keys (`routes/queue/+page.svelte`); `queue-cursor.svelte.ts` is the bookkeeping, `queue-keys.ts` reads a keypress,
and each `QueueRow` exports what a key does to ITS operation (`QueueRowApi`: `show`, `pauseOrResume`, `cancel`,
`rollback`, `cancelOrDismiss`, `focusControl`, `focusRow`), next to the buttons that do the same. Each returns `false`
where the row doesn't offer that button, so its key does nothing there.

- **The keys** are fixed-key registry commands in the `Operation queue window` scope
  (`../../commands/sources/queue-window.ts`), matched whole through `comboMatchesCommand`, so Settings and the Help
  window list them. Fixed because this window holds no `store:` capability, so it can't read a rebinding: a rebindable
  key here would be a setting that silently does nothing.
  - ↑ / ↓ step (no wrap; with no cursor yet, ↓ starts at the top and ↑ at the bottom), Home / End jump. They work from
    the bare page too, so ↓ answers the moment the window opens.
  - Space checks the cursor row for "Cancel selected"; ⇧↑ / ⇧↓ extend from an anchor, macOS-table style (turning back
    toward the anchor takes rows out again, and what was checked before the range began stays); ⌘A checks every row that
    can still be canceled. A settled failure is never selectable.
  - Enter is the row's Show; ⌫ (or ⌦) is the row's own Cancel, or Dismiss on a failure. ← / → walk the row's buttons and
    back out. ⌥↑ / ⌥↓ reorder (below). Escape closes the window.
  - **A letter per row button**, shown as its `keyHint` chip and acting on the cursor row from anywhere in it (the row,
    or a button → reached): S Show, P Pause / Resume, C Cancel, B Rollback (`QUEUE_ROW_KEYS` in `../decision-keys.ts`).
    B is the clash prompt's own; C stays here although the prompt cancels with Escape, since Escape closes this window.
    The letters read the prompts' way (`decisionKeyOf`): bare, never a held key's repeat (a held P would pause and
    resume in turn), and the physical key on a non-Latin layout; `queue-keys.ts::queueCombo` drops a bare letter that
    reading refuses. They stay clear of the window's other keys, of Retry and Dismiss (no letter; ⌫ dismisses), and of
    F, which the row forwards to its live tree (§ "The inline tree"); inside the tree the grid hears no letter at all.
    Untranslated, like every decision letter.
  - Tab from the cursor row enters its live tree, where the tree owns the keys and Escape comes back (§ "The inline
    tree").
- **⌘A arrives as a menu event, not a keydown.** The Edit > Select all accelerator fires before the webview sees the
  key, and every other secondary window gets the native `selectAll:` (text selection).
  `apps/desktop/src-tauri/src/menu/menu_handlers.rs` routes it to this window as `queue-select-all` instead
  (`onQueueSelectAll`); the keydown road stays for a menu that doesn't claim the key (the dev browser).
- **Decision: ⌫ and C cancel with no confirmation, and B asks first; neither takes a modifier.** Each does exactly what
  its button does. Cancel keeps every file already written (`CLAUDE.md` § "Cancel keeps partials"), so a confirm on the
  key would guard nothing the button doesn't. Rollback, the destructive one, raises the row's `RollbackConfirmDialog`
  like its button, and nothing reverses until that's answered: the confirmation is the guard, so a modifier would only
  make B harder to find. The letters' own guards (bare, no repeat, never inside the tree or a dialog) keep a stray or
  held key from acting twice.
- **The cursor is an operation id, ❌ never an index.** `operations-changed` rebuilds the rows whenever anything moves;
  an index would slide a different operation under the cursor on every reorder or finish. When the cursor's operation
  leaves, the row that slid into its place takes the cursor (`reconcile`).
- **Focus follows the cursor row while the grid owns focus.** A finished row, or a keyed `{#each}` moving the focused
  element during a reorder, drops focus on `<body>`; the page's effect puts it back on the cursor row. `gridOwnsFocus`
  clears only when focus moves to a REAL element outside the grid (a `null` related target is a removed element or the
  window blurring).
- **A click anywhere on a row moves the cursor there**, except a click that ends a text selection in the failure reason
  (focusing would collapse it).
- **The cursor looks like the file list's** (`apps/desktop/src/app-file-list.css`): `--color-cursor-inactive` with the
  accent hairline, `--color-cursor-active` while the grid has focus, winning over the selection fill. It IS the row's
  focus indicator, so the row drops the browser ring; a focused button keeps the global one. While focus is inside the
  row's live tree, the row's cursor dims to inactive (`:has([data-queue-tree]:focus-within)`): the tree's own cursor,
  drawn the same way, is the active one, so exactly one accent cursor says where the keys go.

### Decision: an ARIA grid, not a listbox

The issue left it open: a listbox whose buttons only have keys, or a grid whose ←/→ reach the buttons.

- **A listbox `option` can't contain interactive content.** VoiceOver reads an option as one flat string, so Show,
  Pause, Cancel, Dismiss, and Rollback would be audible only as words, reachable only through shortcuts nothing on
  screen announces. Every new row action (#398's Retry, #399's "Send error report…") would then need a key of its own,
  or be mouse-only.
- **A grid keeps every button a button.** → lands VoiceOver on a real, named `button` ("Pause"), and a new button in
  `.actions-cell` joins the walk with no key work at all. The row is the focus target (`role="row"`, roving `tabindex`),
  named by its summary and status cells (`aria-labelledby`), described by its failure reason, and carries
  `aria-selected` for "Cancel selected" (`aria-multiselectable` on the grid). Each button's letter (plus Enter on Show
  and ⌫ on Cancel) rides `aria-keyshortcuts`, since the chip itself is `aria-hidden`.
- **A keyed button is named by its visible word alone** (Show, Pause, Resume, Cancel, Rollback), ❌ no `aria-label`.
  WCAG 2.5.3 (label in name) wants the visible text inside the name, and axe counts the `aria-hidden` key chip as
  visible text, so "Pause this operation" fails against "Pause P" while "Pause" is never checked. The row already says
  which operation, and Show's tooltip says where it goes. Retry and Dismiss have no chip, so their "… this operation"
  labels pass.
- **The checkbox stays for the pointer** but leaves the Tab order and the → walk: Space does its job from the row, and
  the row's `aria-selected` already says it.

### Decision: `ColumnList`'s cursor PATTERN, not the component

`ColumnList` (`../../ui/DETAILS.md` § ColumnList) is the house list, but it's built for single-line records in measured
columns: a fixed row height for virtual scrolling, `option` rows (or `table` rows with no focus model), and no cursor
inside a row. A queue row is a two-line grid (the readout spans the second line, see § Row layout), varies in height (a
failure's reason wraps), holds buttons, and is never more than a few dozen rows. So the queue takes the pattern instead:
the parent owns the cursor and the keys, the row only paints `is-under-cursor`, and the cursor reads the same as the
file list's.

### Reordering (⌥↑ / ⌥↓)

The house reorder key (`../../ui/menu-navigation.ts`) and the APG rearrangeable-listbox key; ❌ not ⇧-arrows, which
extend. The page sends `moveQueuedOperation(id, direction)` and announces the backend's answer in a polite live region
(`queue.reorder.*`: the new place among WAITING operations, "already first/last", or "only waiting operations can
move"). Only a waiting row moves; running and paused ones keep their place. The row itself moves when the next
`operations-changed` lands, and the cursor (an id) moves with it. Backend: `crates/cmdr-operations/DETAILS.md` §
"Reordering the line". The mouse does the same with the grip (below).

### Dragging to reorder

A waiting row's grip (`grip-vertical`, left of its checkbox) drags it among the WAITING rows; the drop sends one
`placeQueuedOperation(id, position)` and announces the answer in the same live region as ⌥↑ / ⌥↓.

- **Who gets a grip**: a waiting row, and only while two or more are waiting (a lone one has nobody to trade with). The
  grip COLUMN is all-or-nothing across the list (`gripColumn`), so the checkboxes stay in one line. The grip is
  `aria-hidden`: keyboard and screen-reader users have ⌥↑ / ⌥↓ and the row menu.
- **Pointer events, ❌ never HTML5 drag-and-drop**: Tauri's `dragDropEnabled` takes the gesture before WKWebView fires
  `dragstart`. `queue-drag.svelte.ts` owns the gesture: a 4 px threshold (a shorter press is a click), pointer capture,
  the house reorder math (`#lib/ui/menu-reorder.ts`) over the waiting rows' live midpoints, and a wheel mid-drag keeps
  the row under the pointer. Escape mid-drag (the page's `queue.close`) puts the row back instead of closing the window.
- **The look**: the held row lifts into an opaque card (shadow, 1% larger) that follows the pointer; an accent drop line
  with a ring at its start (like Finder's) marks the gap it would land in, and no line shows over its own two gaps (a
  no-op). While held, `.is-reordering` on the window makes every cursor `grabbing` and stops text selection.
- **The drop slides, it doesn't jump**: the dropped row HOLDS its offset until the reordered `operations-changed` lands
  (or one second passes, or the answer isn't `moved`). The page measures every row in `$effect.pre` when the order
  changes, before the DOM moves, then FLIPs them into place after `tick()` (`queue-flip.ts`); the dropped row is
  measured where it was dropped. ❌ Not `animate:flip`: it measures the END position before the row's reactive offset
  clears, so the dropped row would slide from the wrong place and jump. The same FLIP slides rows on ⌥↑ / ⌥↓ and when a
  row finishes. `prefers-reduced-motion` skips the slide.

## The row's right-click menu

A native muda menu (`apps/desktop/src-tauri/src/menu/queue_row_menu.rs`), per `docs/guides/building-ui.md` § Building a
menu: plain rows, opened by a right-click on a backend object. Right-clicking puts the cursor on the row first, as in
any list.

- **Every row**: Move up (⌥↑), Move down (⌥↓), divider, Select (Space, ticked while the row is selected), divider,
  Cancel (C), or Dismiss (⌫) on a failure. **A running or paused row** leads with Show (↩) and Pause or Resume (P), then
  a divider, and puts Rollback… (B) beside Cancel: the two ways to stop it, at the bottom. Items that don't apply are
  greyed (Move up on the first WAITING row, all but Dismiss on a failure), never dropped, so the menu's shape stays put.
- **The frontend lists the entries** (`queue-row-menu.ts`, pure, unit-tested); Rust only labels them and pops them up,
  because every fact behind them lives in this window: the row's own (`QueueRow`'s `menuFacts()`, read off the same
  derived state its buttons and key guards use), plus the selection and the line's order (`queuedNeighbors`).
- **A pick runs the KEY's action** (`queueRowMenuKeyAction` → the page's `runGridAction`), on the row the menu opened
  over (`menuRowId`), so the menu and the keys can't drift. It arrives as `queue-row-context-action` after the menu
  closed, so the page re-reads the row and drops a pick it no longer offers live (`queueRowMenuOffers`): a Pause picked
  off a row the toolbar paused meanwhile would otherwise toggle it back to running.
- **The bare `C` / `Space` / `P` labels are safe.** A popup's key equivalents are never registered with the app (muda
  builds the `NSMenu` per popup and never attaches it to the menu bar), so they can't eat the key while the window is
  focused; while the menu is open, pressing one picks its item, which is AppKit's own behavior. Labels come from the
  pushed `boundShortcuts()` (`menu/CLAUDE.md`), so they're the registry's keys, never literals. (Verified on muda
  0.20.0's macOS backend by reading its source, 2026-10-14.)

## Title bar

The heading ("Operations") sits on the traffic lights' own line, centered like a native window title, the way the main
window's title bar does it: the lights at the main window's `trafficLightPosition` (9, 17), the strip
`--titlebar-height` tall and a drag region, its text too (Tauri reads the attribute off the element under the cursor).
The toolbar starts right below it, so no row of the window is spent on a title alone. The `h1` stays `sr-only` and
carries the window title for a screen reader.

## Vibrancy + reduce-transparency

`new WebviewWindow(...)` doesn't throw when creation is refused (a missing grant, a label clash): the refusal arrives
afterwards as `tauri://error`, which `queue-window.ts` listens for and logs at warn, so it can't fail without a trace.

`queue-window.ts` opens transparent and applies `Effect.UnderWindowBackground` via `setEffects` after creation (the
`windowEffects` creation option drops silently in this Tauri version; `setEffects` is the reliable IPC path, gated by
`core:window:allow-set-effects`). UnderWindowBackground reads as a clean utility/HUD-style panel — the macOS convention
for a transfer/activity manager — and follows the window's active state.

Under macOS "Reduce transparency" the window opens opaque (no material, `backgroundColor` mirroring the theme) and the
page surface uses the shared `--color-bg-glass` / `--color-border-glass` tokens, which flip to opaque under
`html.reduce-transparency` (toggled from the backend `NSWorkspace` value via `#lib/display-accessibility`, since
WKWebView doesn't reflect `prefers-reduced-transparency`). `prefers-color-scheme` IS reflected, so dark detection stays
a media query. Reduced motion is honored by the shared `ProgressBar` and `Spinner`: the bar's shimmer lives inside
`@media (prefers-reduced-motion: no-preference)` and the spinner's spin freezes through `app-utilities.css`.

## Capabilities

`src-tauri/capabilities/queue.json` mirrors `settings.json`'s window perms (close, set-focus, set-min/max-size,
set-effects, start-dragging, outer-position/size, scale-factor, `core:event:default`, `core:app:allow-set-app-theme`,
`core:webview:allow-internal-toggle-devtools`) but DROPS `store:default` (no persistence in v1) and `dialog:allow-ask`
(keep-partials cancel needs no confirm). The pause/resume/cancel app commands go through the `tauri_specta` invoke
handler, not the capability ACL, so they need no per-command grant. The opener's `getByLabel` + `readMonitors()` run on
the MAIN window, which already holds those perms — nothing to add there (see `docs/guides/adding-a-window.md`).

## Opening the window

- Command palette + View menu, default ⌥⌘Q: `queue.show` ("Operation queue"), handled in
  `routes/(main)/command-handlers/app-dialog-handlers.ts` → `openQueueWindow()`. Wired through the full command path
  (id, registry with the default shortcut, handler, Rust menu mappings, both platform menu builders, `menuCommands` so a
  rebind syncs the accelerator). It sits immediately after "Command palette…" and before "Operation log", pairing the
  present-tense and past-tense views of the same work.
- The progress dialog also opens/raises it automatically when an op lands on a busy lane (auto-queue surfacing).

## Tests

- `operations-store.svelte.test.ts`: the reducers (snapshot → rows, progress merge + unknown-op drop, prune on leave,
  running/paused presence), `isTerminalStatus`, and a paused operation's row seeded from `latest_operation_progress`.
- `row-items.test.ts`: the summary's wording ("2 folders and 1 file", "2,000 items", the tooltip's "… and 1,998 more", a
  display path's last folder). `QueueRow.svelte.test.ts` pins the rendered line, a copy's and a delete's.
- `main-window-operations.svelte.test.ts`: the main window's lifecycle (subscribe once, idempotent init, both listeners
  dropped on destroy, a re-init after destroy yielding a LIVE instance, and a destroy mid-init leaving nothing
  subscribed).
- `failure-reason.test.ts`: the wire-type → catalog-arm mapping (per-operation wording, the copy fallback for the types
  the catalog has no arm for) and the null-for-a-live-row contract.
- `QueueRow.svelte.test.ts`: which control a given status offers (Pause vs Resume vs queued vs failed), the select
  checkbox, the live bar from a progress event, the failed row's reason across two error variants and two operation
  types, and the `data-status` / `data-operation-id` E2E hooks. What a click SENDS isn't here: that needs a window
  registry, so it lives in `queue-row-session.svelte.test.ts`. The readout's own behavior (both bars, percents, rates,
  time left, stall) is covered once, in `../TransferProgressReadout.svelte.test.ts`.
- `QueueRow.a11y.test.ts`: axe over the row (a grid `row` mounted in a `grid`) in running / paused / queued / selected /
  cursor states, and with its live tree and the tree toggle, open and shut.
- `QueueRow.tree.test.ts` and `row-tree.test.ts`: which rows show a tree, the toggle (only with two trees, open by
  default, `aria-expanded` / `aria-controls`), Tab reaching the tree only from the cursor row, and Escape back to the
  row. `queue-row-session.svelte.test.ts` pins the pull on each tick, and the page test the grid keys staying out of a
  tree.
- `queue-row-menu.test.ts`: the right-click menu's entries per row state (order, dividers, greying, Select's tick,
  Resume), each pick's keyboard action, a stale pick refused, and which waiting rows can move. `QueueRow.svelte.test.ts`
  pins the row's `menuFacts()` against its buttons; the page test the right-click → menu → pick road end to end.
- `queue-drag.svelte.test.ts`: the drag gesture (a press under the threshold is a click, drops to the first and last
  place, the drop line's gap and its no-op gaps, a drop where it started sends nothing, the dropped row holding until
  the list changes, Escape, a non-waiting row). Placement itself is pinned in Rust (`manager::tests::reorder`).
- `queue-keys.test.ts`: which keypress means which action, whole-combo (⌥⇧↑ is neither a reorder nor an extend).
- `queue-cursor.svelte.test.ts`: the cursor's steps and edges, following its operation through a reorder or a finish,
  Space / ⌘A, and the ⇧-arrow range (shrinking back toward the anchor, keeping what was checked before it, skipping a
  failure).
- `queue-row-session.svelte.test.ts`: what a row takes from its session and what it asks of it, through real rows on a
  real registry — one `createEtaSmoother` per operation however many ticks or snapshot rebuilds arrive, none in the
  store, a scanning row's files/s, and each control's command (including the toggle following the snapshot status, and
  one cancel however many presses).
- `apps/desktop/src/routes/queue/queue-selection.svelte.test.ts`: the window's selection bookkeeping, driven through the
  real page and the real store — the count and "Cancel selected" following a checked row, and letting go of one that
  fails, leaves, or was never theirs. Plus the keyboard, through the real page: one Tab stop, ↓ from the bare page, the
  cursor keys, click-to-cursor, Space / ⇧↓ / the ⌘A menu road, ⌫ cancelling or dismissing, Enter, ⌥↑ and its
  announcement, focus surviving a reorder and a finish, and the → / ← walk.
- E2E: `test/e2e-playwright/operation-queue.spec.ts` — two same-lane ops → one Running + one Queued, cancel the queued,
  pause + resume the running; plus the retention contract: a copy fails with NO queue window open, the window opens on
  the failed row and its reason, closes on Escape, reopens still showing it, and only Dismiss clears it. That spec's
  `afterEach` dismisses retained failures explicitly — its drain loop waits for an empty `list_operations`, and a
  failure is designed never to leave on its own.
