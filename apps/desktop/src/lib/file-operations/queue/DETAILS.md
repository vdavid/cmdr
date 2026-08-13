# Operation queue window — details

Depth for `CLAUDE.md`. The window is the frontend of the operation-queue + pause feature; the backend lives in
`src-tauri/src/file_system/write_operations/` (DETAILS § "Operation manager").

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
exact Settings-window pattern (singleton, vibrancy, overlay title bar, position via `$lib/window-positioning`) so it
reads as a first-class macOS utility window, consistent with Settings and Keyboard shortcuts.

## The two-stream model

The window renders from `createOperationsStore()`, which merges:

1. **`operations-changed`** (`onOperationsChanged`): the thin registry snapshot the backend emits whenever an operation
   is registered, admitted, paused, resumed, or settles. Payload is `{ operations: OperationSnapshot[] }`, where each
   `OperationSnapshot` is `{ operationId, operationType, status, source, destination, supportsRollback, error }` —
   membership, lifecycle status, whether the row may offer Rollback, and (on a retained failure only) the typed
   `WriteOperationError` that stopped it. No 200 ms progress. This decides which rows exist and each row's status.
2. **`write-progress`** (`onWriteProgress`, the existing per-file stream): drives the live per-row progress bars. The
   store keys the latest `WriteProgressEvent` by `operationId`.

On every snapshot tick the store prunes the progress map to the new membership, so a finished op's bar can't outlive its
row, and the map can't grow unbounded. Progress for an op not (yet) in the snapshot is ignored — the snapshot is the
membership source of truth.

This split keeps `operations-changed` cheap (no progress fattening it every 200 ms) while still giving each row a live
bar — the design the plan mandates under "subscribe, don't poll" and "thin snapshot".

### What the store may hold, and what belongs to the session

Everything in the store is STATELESS: which operations exist, each one's lifecycle status, and the latest tick each one
emitted. A second reducer over the same stream is harmless, because two copies of one event object can't disagree.

Every ESTIMATE is stateful and lives on the operation's session instead (`../operation-session/CLAUDE.md`), which each
row binds to with `bindOperationSession(() => snapshot.operationId)`:

- `session.etaSecondsDisplay`, the backend's ETA through the one smoother this operation has in this window.
- `session.scan`, the walk's files/s and bytes/s, which the backend doesn't emit while it counts.

Two estimators fed identical samples from identical starting points would agree; the divergence comes from one starting
later, which is what any view attaching to an operation already in flight does. So the rule is positional rather than
stylistic: ❌ a smoother in the store is a second layer by construction, and `queue-row-session.svelte.test.ts` counts
constructions to keep it that way.

⚠️ Bind on the id as a VALUE, never on the row object. `operations-changed` rebuilds every row whenever anything in the
registry moves, so a binding that re-ran per row object would hand the operation a fresh smoother mid-transfer, every
time some unrelated operation started or finished. `bindOperationSession` derives the id string for exactly that reason.

### Why snapshot status, not `is_running`

A paused Running op stays in `WRITE_OPERATION_STATE`, so `get_operation_status().is_running` reports `true` while paused
("running but not progressing"). The bar-is-moving truth is therefore the snapshot `status`: only `'running'` shows the
spinner and an animated bar; `'paused'` shows a static bar and the Paused label. Rows never read `is_running`.

## Store public API (the progress-dialog reuse contract)

`createOperationsStore()` returns:

- `operations: OperationRow[]` — reactive; each `OperationRow` is
  `{ snapshot: OperationSnapshot, progress: WriteProgressEvent | null }`. Ordered as the backend emits them.
- `supportsRollback` on each snapshot — whether cancelling can also undo what the op wrote, decided per spawn path in
  the backend (`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Rollback availability"). The row
  shows Rollback on exactly that set, so this window and the progress dialog can't disagree about which operations are
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
either way; only the word and its `aria-label` change.

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

- **Only the id crosses.** The row emits `foreground-operation` (`$lib/tauri-commands/dialog-events.ts`), and the main
  window resolves that id against ITS OWN `operations-changed` snapshot
  (`../foreground-request.ts::adoptedOperationFor`). The registry row is the single source of truth about an operation,
  and both webviews already receive it, so nothing about the operation travels on the wire. Fold this window into the
  main window as a popup one day and the emit collapses to a direct call with the same argument.
- **The queue window holds `core:event:default`**, so no capability change was needed. It does NOT raise the main
  window: the main window focuses ITSELF in its listener, whatever the verdict, because a refusal the user cannot see
  reads as the button doing nothing.
- **Offered only where a dialog would have something to show.** Running or paused, and not an instant op. ❌ Never on a
  `queued` row: the dialog auto-backgrounds a queued operation, so it would open and hand it straight back. ❌ Never on
  a failed row: there is nothing left to watch, and its reason is already on the row in full.
- **The main window can refuse.** Its dialog slot is single-occupancy; a refusal comes back as a toast there, next to
  the dialog that refused. Reasoning and the invisible-occupancy hazard:
  `../../file-explorer/pane/DETAILS.md` § "Birth context".
- Pinned by `QueueRow.svelte.test.ts` (which statuses offer it, and that the click asks for that row's own operation)
  and `../foreground-request.test.ts` (the lookup, including the ordinary miss).

## A scanning row

An operation is registered the moment the user confirms, which is before its `TransferDialog` scan preview has finished
walking (`apps/desktop/src-tauri/src/file_system/write_operations/scan_bridge.rs`). So a row can be `running`, or
`queued` behind a busy lane, while its `progress.phase` is still `scanning`.

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
- **No Pause, no Rollback.** The backend declines a pause in a scan-wait (there is nothing to park, and a "paused" scan
  would hold its lane doing nothing), and a scanning operation has written nothing to reverse. `supportsRollback` stays
  true throughout: it is a promise about the OPERATION, so the phase is what decides which controls make sense now.
- **The status column still says "Running"**, and that is deliberate. `queue.row.status` is a `select` over the
  LIFECYCLE status, which genuinely is `running`; the readout names the activity, and the scan-phase line already says
  "Counting…" in the user's language. A "Scanning" arm would mix two axes into one column and would need a `phase` input
  the row's message doesn't take.

## Retained failures

The backend keeps a bounded list of failed operations and carries them on the same `operations-changed` snapshot, each
with its typed `error` (`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Retained failures" owns
the mechanism). This window is the durable surface for them: it survives a dismissed toast, a closed window, and a
reopen.

- **The row keeps its place, so the SELECTION has to let go of it.** `routes/queue/+page.svelte` prunes `selectedIds`
  against the rows that are still `!isTerminalStatus(...)`, not against mere membership: a failure stays in the list and
  drops its checkbox, so a tick the user made while it was running would be unclearable by hand, leaving "1 selected" on
  screen and "Cancel selected" enabled over an operation the backend no-ops. Pinned by
  `apps/desktop/src/routes/queue/queue-selection.svelte.test.ts`.
- **The row.** No pause, cancel, rollback, or select checkbox — there's nothing live left to act on. A `triangle-alert`
  glyph and "Couldn't finish" in `--color-error-text`, one Dismiss button, and the reason on line 2 where the readout
  would be.
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
- **Dismissal is explicit, always.** Per-row Dismiss → `dismiss_failed_operation`; the toolbar's "Dismiss all" (shown
  only when `failureCount > 1`) → `dismiss_all_failed_operations`; and closing `TransferErrorDialog` for a foreground
  failure, which the user has by definition just read (`apps/desktop/src/lib/status-corner/DETAILS.md` § "Why the
  foreground handover needs two slots"). Nothing else drops a failed row — the whole feature exists for the user who was
  away from the keyboard.
- **The main window's half of this** (the persistent failure toast and the corner chip's failure state) lives in
  `apps/desktop/src/lib/status-corner/CLAUDE.md`. Both render `failure-reason.ts`, so the three surfaces can't describe
  one failure three ways.

The progress-dialog Queue button and the auto-queue surfacing open the window via `openQueueWindow()` and read this same
store. Don't fork a second opener or store.

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
click deletes the same files. Rollback hides again once the op IS rolling back, which the row reads off the live
`write-progress` phase: rollback is an `OperationIntent`, so the lifecycle status stays `running` throughout, and the
status cell shows "Rolling back..." instead.

A row is a five-column grid whose chrome (select, type icon, source→dest summary, status, actions) sits on line 1, with
the shared `../TransferProgressReadout.svelte` spanning line 2 from the summary column to the end. The readout gets the
full row width rather than a slot beside the buttons because its columns are fixed-width: sharing a line with the status
and two buttons would have pushed the window's minimum width past 700 px for the bars to survive. A row with no
`write-progress` to show — a queued op, or an instant `rename` / `create_folder` / `create_file` — renders line 1 only.

## Vibrancy + reduce-transparency

`queue-window.ts` opens transparent and applies `Effect.UnderWindowBackground` via `setEffects` after creation (the
`windowEffects` creation option drops silently in this Tauri version; `setEffects` is the reliable IPC path, gated by
`core:window:allow-set-effects`). UnderWindowBackground reads as a clean utility/HUD-style panel — the macOS convention
for a transfer/activity manager — and follows the window's active state.

Under macOS "Reduce transparency" the window opens opaque (no material, `backgroundColor` mirroring the theme) and the
page surface uses the shared `--color-bg-glass` / `--color-border-glass` tokens, which flip to opaque under
`html.reduce-transparency` (toggled from the backend `NSWorkspace` value via `$lib/reduce-transparency`, since WKWebView
doesn't reflect `prefers-reduced-transparency`). `prefers-color-scheme` IS reflected, so dark detection stays a media
query. Reduced motion is honored by the shared `ProgressBar` and `Spinner`: the bar's shimmer lives inside
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
  running/paused presence) and `isTerminalStatus`.
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
- `QueueRow.a11y.test.ts`: axe over the row in running / paused / queued / selected states.
- `queue-row-session.svelte.test.ts`: what a row takes from its session and what it asks of it, through real rows on a
  real registry — one `createEtaSmoother` per operation however many ticks or snapshot rebuilds arrive, none in the
  store, a scanning row's files/s, and each control's command (including the toggle following the snapshot status, and
  one cancel however many presses).
- `apps/desktop/src/routes/queue/queue-selection.svelte.test.ts`: the window's selection bookkeeping, driven through the
  real page and the real store — the count and "Cancel selected" following a checked row, and letting go of one that
  fails, leaves, or was never theirs.
- E2E: `test/e2e-playwright/operation-queue.spec.ts` — two same-lane ops → one Running + one Queued, cancel the queued,
  pause + resume the running; plus the retention contract: a copy fails with NO queue window open, the window opens on
  the failed row and its reason, closes on Escape, reopens still showing it, and only Dismiss clears it. That spec's
  `afterEach` dismisses retained failures explicitly — its drain loop waits for an empty `list_operations`, and a
  failure is designed never to leave on its own.
