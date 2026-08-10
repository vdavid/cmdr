# Operation sessions: the dialogs become looking glasses

Status: proposed. Spec only; nothing implemented. Line numbers are as of `101580aa8`.

## The intent

David's mental model, in his words:

> Once the TransferDialog appears, the operation starts preparing. In a sense, the operation itself starts, in the "scan
> first" state. Then, of course, the operation itself is gated on the user pressing the Copy/Move/Compress/Delete/Trash
> button. And conflict resolution is a part of the op, it's a part where the op gets paused, and user input is needed,
> yeah, but still part of the op. And the op is live regardless of whether the TransferProgress dialog is visible, or
> the queue, or neither, just the little bar at the top-right of the main window, or even if the UI bounces between
> these. These dialogs should be looking glasses into the process, but not the process itself. They also hold
> pause/resume/cancel/rollback controls, sure, but these are commands and the op shouldn't care where they come from.
> Later, they might come through MCP, too. It doesn't matter. So coupling between the op and the dialogs is not nice.

The code contradicts that model in two places. One is the module this spec is about. The other is a single line in the
main window's layout, and it contradicts the model harder.

`createTransferProgressState` (`apps/desktop/src/lib/file-operations/transfer/transfer-progress-state.svelte.ts`, 1,294
lines) does not observe an operation. It **owns** one. It scans, it dispatches, and only then learns its `operationId`.
The dialog is not a looking glass; it is the thing that starts the process and happens to render it.

So there is no way to say "attach to operation X", only "start one and watch it". That is why the Foreground button (the
motivating feature: click a running row in the operation queue, get the rich progress dialog back) cannot be built
today.

## What the module actually conflates

Reading it end to end, there are three concerns in one factory:

1. **Session**: per-operation, lives exactly as long as the operation. `operationId`, `phase`, `currentFile`,
   `filesDone` / `filesTotal` / `bytesDone` / `bytesTotal`, `bytesPerSecond` / `filesPerSecond`, the smoothed
   `etaSecondsDisplay`, `activity`, `opStatus` from `operations-changed`, `conflictEvent` and `isResolvingConflict`
   (`:305`), `isCancelling` / `isRollingBack` / `operationSettled`, the terminal-event bookkeeping
   (`cancelEventReceived` `:216`, `settleEventReceived` `:219`, and the single-shot `cancelEventPayload` `:222`, which
   together decide when a cancel is actually over), and the pause/resume/cancel/rollback commands.
2. **Birth**: one-time, ends the moment the backend returns an id. `config.scanInProgress` and the scan-wait path,
   `dispatchOperation` (`:631-693`) and its two helpers, the `pendingEvents` buffer plus `replayBufferedEvents` that
   exist only because events can arrive before the dispatch response does, the `mcp-response` reply on success (`:797`)
   and on failure (`:856`), and the destroyed-during-dispatch cancel (`:805-813`).
3. **View**: belongs to a particular piece of UI, not to the operation. `MIN_DISPLAY_MS` (`:169`), `dismissed`,
   `settleSlow` and `SLOW_SETTLE_LABEL_MS`, `CANCEL_SETTLE_FALLBACK_MS` and its timer, `maybeFinishCancelClose`, the
   `onComplete` / `onCancelled` / `onError` / `onQueue` callbacks, `backgrounded`, and the safety-net cancel in
   `destroy()`.

Concern 1 is the operation. Concerns 2 and 3 are the dialog. They are currently one object with one lifetime, and that
lifetime is the modal's.

Two of those entries change meaning under sessions, so name them now rather than discovering them mid-refactor.
`dispatchOperation` looks like six routes with six helpers and is not: it is six branches with two helpers,
`dispatchCopy` (`:697`) and `dispatchCompress` (`:726`), while trash, delete, volume move, and local move are inline
calls to `trashFiles` / `deleteFiles` / `moveBetweenVolumes` / `moveFiles`. And the destroyed-during-dispatch cancel
exists because "no view attached" is currently impossible, so a dialog that vanished mid-dispatch must mean the user
gave up. Once detaching is legal that inference is wrong, and the branch needs a new rule (probably: only cancel if the
caller explicitly cancelled, which the `!operationId` path in `handleCancel` (`:892-896`) already records by setting
`destroyed = true`).

### Auto-queue is the concerns tangled in miniature

`handleOperationsChanged` (`:1030-1043`) is pure session: find this operation in the snapshot, store its status. Then,
when the status is `queued`, it calls `handleAutoQueued` (`:1011-1026`), which is pure view: set `backgrounded`, release
the foreground slot, open the queue window, raise a toast, and unmount. One of only two places `backgrounded` is ever
set (the other is the manual Queue button), and it is reached from a snapshot reducer.

Under sessions the split is clean and worth stating as the design's smallest working example: the **session** observes
that the operation is `queued` and exposes that. The **view** decides that being queued behind a busy lane means this
particular dialog should detach. Another view, a queue row for instance, sees the same status and does nothing.

## The shape

Two things where there is one today.

**An operation session**, keyed by `operationId`. Reads the write-event streams and the `operations-changed` snapshot,
holds phase and metrics, exposes pause / resume / cancel / rollback / resolve-conflict as plain commands, and does not
know or care whether anything is rendering it. Lives as long as the backend record plus the settle tail.

**Views** that bind to a session and render it. The progress dialog is one. A queue row is one. The corner chip is a
minimal one. Zero views is a legal, ordinary state, and it is precisely what "backgrounded" means.

### Why a registry, not a session per view

One session per `operationId` per window, held in a registry. Two views of the same operation MUST share one session.

The reason is a real shipped bug, though not quite the one it looks like. `progress-readout.ts`'s own header records it:
the copy dialog put `etaSeconds` through `createEtaSmoother()` while the queue window rendered `progress.etaSeconds`
raw, so one operation read "8m 12s remaining" in one window and "5m 46s" in the other. That was smoothed versus raw, and
it is already fixed by both surfaces rendering the same smoothed value.

So "two smoothers give two answers" is not the argument, because the EMA is deterministic: two smoothers fed the same
samples from the same starting point agree exactly. The argument is the one a Foreground button creates. **Smoothers
diverge when they start at different times.** A view that attaches twenty minutes in builds a smoother whose first
sample is the current rate, while the queue's smoother carries twenty minutes of history, and the two disagree on screen
for as long as the new one takes to converge. Late attachment is the whole point of this spec, so the registry is what
keeps "one operation, one truth" structural rather than remembered.

### Why sessions read a fan-out, not their own listeners

Today the dialog subscribes to seven event streams. Ten sessions must not mean seventy subscriptions, but listener count
is the least of it. The fan-out is a correctness boundary: one place that buffers events arriving for an id nobody has
claimed yet, one arrival order, and one authority on which ids are live.

**It is a new module, not an extension of the operations store.** `createOperationsStore()` subscribes to two of the
seven streams (`operations-changed` and `write-progress`), is a reducer over all operations at once, and has no per-id
attach API to extend. Making it the fan-out would mean rewriting it into something else while three surfaces render from
it. Instead the demultiplexer sits underneath, and both `createOperationsStore()` and the session registry become its
consumers.

The behavior that earns it its place in M1: **it buffers events for unknown ids and flushes them on registration.** That
is what makes `pendingEvents` and `replayBufferedEvents` deletable in M4 rather than portable to the session, and it is
why the fan-out lands before anything reads it.

### Sessions are per-window, and that is fine

The operation queue is a separate `WebviewWindow`, so a session cannot be shared across windows; each webview builds its
own from the same broadcast events. The backend registry stays the single source of truth, and sessions are a per-window
projection of it. Worth stating plainly because it looks like a violation of "one session per operation" and isn't.

Note David is considering moving the queue out of a separate window into a soft popup inside the main window. That
change would make the queue's sessions the _same_ sessions as the main window's, which this design absorbs without
alteration. Don't build anything that assumes two windows.

## M0's premise: an operation does not currently survive a reload

Every "the operation outlives the view" sentence above is false today, and not because of the dialog.

`apps/desktop/src/routes/(main)/+layout.svelte:282-284` registers a `beforeunload` handler that calls
`cancelAllWriteOperations()`. That command walks the GLOBAL registry and stops every operation
(`src-tauri/src/file_system/write_operations/state.rs`, pinned by a test literally named
`cancel_all_write_operations_walks_the_global_registry`). Backgrounded operations included. Operations the main window
has no view on at all, included.

The reproduction is one line long: start a copy, press Queue, hot-reload the main window, and the transfer dies while
the queue window sits there rendering a row for it.

This is a defect independent of the refactor, and it contradicts David's model more directly than the dialog coupling
does: the dialog at least only stops the operation it owns. Fix it first, because M1 through M6 all reason from a
premise it currently invalidates.

The fix turned out to be larger than a frontend edit (the backend has to own operation lifetime and the quit decision,
and local copies have to stage before a worker can be safely abandoned), so it has its own spec:
`docs/specs/quit-and-operation-lifetime.md`. That document IS M0.

## The in-dialog guards, and why they retire

There are **two**, not one, and they are separate decisions.

1. `destroy()` (`:1183-1202`, doc comment from `:1179`) fires `cancelWriteOperation(operationId, false)` when the dialog
   unmounts with the operation unsettled and not backgrounded.
2. `handleCancel` (`:872`) opens with `if (backgrounded) return` (`:880`), because the modal's `onclose` (the × button,
   Escape, or focus-trap teardown) routes into cancel during the backgrounding handoff. Without it, sending an operation
   to the queue would cancel it and open an empty queue window.

Under sessions, a view detaching is normal, so neither guard survives in its current form. Guard 2 is the sharper
statement of what has to change: **"the modal closed" must stop mapping to `handleCancel` at all.** A close is a detach.
Cancel is a command, and only the Cancel button issues it.

Guard 1 retires because nothing is left for it to catch. Path by path, with the dialog gone and the operation unsettled:

- **Error.** `handleTransferError` runs from `config.onError` (`:444`), which runs immediately after
  `operationSettled = true` (`:442`). The guard's `!operationSettled` is already false.
- **Archive password.** `handleArchivePasswordCancel` sits downstream of the same `onError`, so likewise settled, and an
  unlock re-dispatches a NEW operation with a new id rather than resuming the old one.
- **Dispatch failure.** The catch at `:851-869` fires before any `operationId` exists, and the guard requires one.
- **Cancel before the id arrives.** `handleCancel`'s `if (!operationId) { destroyed = true; return }` (`:892-896`) is
  covered by the destroyed-during-dispatch branch at `:805-813`, which cancels as soon as the id lands. That is birth
  logic, and it stays (with the corrected rule noted above).
- **Page unload.** M0's territory, and today over-covered rather than under-covered.

That leaves no surviving path where retiring guard 1 loses an operation, which is the evidence the retirement rests on.
Keeping it would also invert the model: "the view went away, so stop the operation" is the coupling this whole spec
exists to remove.

One claim the earlier framing overstated and this spec should not repeat: silent background work is not impossible
today. `pickChipOperation` (`status-corner/operation-chip.ts:127-137`) shows the first `running` row, else the first
`paused` one, so an operation sitting in `queued` shows nothing at all (its own test pins this), and `CHIP_SETTLE_MS`
(`:21`) holds the chip back 500 ms before it first appears. Both are small, both predate this work, and neither blocks
anything here. Decide them separately.

### The `$state`-during-disposal trap must be carried forward

The module doc explains at length why `backgrounded` and `destroyed` are plain `let`s and not `$state`: a rune read
during synchronous reactive-scope disposal returns a stale value, so the guard passed wrongly and cancelled a
just-backgrounded operation (the transfer died and the queue window opened empty). Any flag a teardown path reads
synchronously has the same hazard. Keep that reasoning attached to whatever teardown logic survives, and don't
"modernize" those `let`s.

## Milestones

Sequential. None of these are safe to parallelize: each one moves state out of a module the next one edits.

### M0: an operation survives a reload

Specced separately: `docs/specs/quit-and-operation-lifetime.md`, whose Q1-Q3 all land before M1 starts.

- **Why first:** every later milestone's "the operation outlives the view" reasoning is false until this lands, so
  shipping M1 first would mean proving session lifetimes against a runtime that kills them.
- **What it settles for the milestones below:** the backend owns operation lifetime and the quit decision, local writes
  are staged, and a window closing is just a viewer detaching.

### M1: the fan-out and the session module, read-only

Introduce the per-window event demultiplexer and `operation-session` alongside the existing code, changing no behavior.
A session binds to an `operationId`, attaches to the fan-out, and exposes the derived read state (phase, metrics, rates,
smoothed ETA, status, settled). No commands yet, no dispatch, no view concerns. Registry with refcounted create/release.

- **Why here:** it can be proven correct against the live streams before anything depends on it, and the fan-out's
  unknown-id buffering has to exist before M4 can delete the dialog's private buffer.
- **Deliverable, not just a check: seed a new session from `list_operations`.** With M0 landed, operations genuinely
  survive a reload, so a reloaded main window must recover them or the chip shows nothing for a transfer that is very
  much still running. Seeding needs a defined **miss case**: terminal operations leave the snapshot entirely
  (`write_operations/DETAILS.md:242`; retained failures are the one exception), so an operation that finished between
  the click and the mount seeds nothing, and the session must resolve to "already gone" rather than hang empty.
- **Settle detection comes from the terminal events, not the snapshot.** Same reason: disappearing from
  `operations-changed` means "removed", which a completed, cancelled, and never-existed operation all look like.
- **Test seam:** `_testEmit(event)` on the demultiplexer, following `operations-store.svelte.ts`'s `_testApplySnapshot`
  / `_testApplyProgress` (`:213-214`).
- **Tests, TDD (red first):** registry identity (same id gives the same instance, release drops it), the fan-out routing
  an event to the right session, buffering an event for an unregistered id and flushing it on registration, seeding from
  `list_operations` including the miss case, and one smoother per operation. Bug-fix-shaped invariants, so they earn
  real red to green.
- **Docs:** colocated `CLAUDE.md` + `DETAILS.md` for the new module, a module-map entry in `file-operations/CLAUDE.md`,
  and a line in `docs/architecture.md`. The `DETAILS.md` carries the registry rationale and the divergent-smoother
  argument.
- **Checks:** `pnpm check svelte -q`.

### M2: the corner chip and the queue rows read sessions

Move the two existing ambient surfaces onto sessions. They are the low-risk consumers: read-only, no dispatch, no
teardown semantics.

- **Why here:** the session's read surface has to be proven sufficient for real UI before the hard part, and this is the
  cheapest proof available. It also collapses the chip's and the rows' separate progress derivation into one place,
  which is the "one operation, one truth" property the registry exists for, made visible.
- **Regression risk to handle head-on:** `operations-store.svelte.ts` already keeps a per-`operationId` smoother map
  (`:92-96`) in every window. M2 must **delete** the store's smoothing and read the session's, not stack a second layer.
  Two smoothers over one stream is exactly the divergence the registry is meant to prevent.
- **Tests:** written after, because this is a substitution behind an unchanged surface and the existing chip and row
  suites are the contract. They must pass unchanged, which is the point. Add one new test: a chip and a row bound to the
  same operation report the identical ETA.
- **Docs:** `status-corner/DETAILS.md` and `queue/DETAILS.md` updated to point at the session as the source.
- **Checks:** `pnpm check svelte -q`, plus the queue E2E.

### M3: commands move to the session

Pause, resume, cancel, rollback, and resolve-conflict become session methods. Views call them; nothing else changes.

- **Why separate from M2:** commands have failure modes (in-flight guards, the paused-operation-reports-`is_running`
  trap) and deserve their own red.
- **Settle cross-window conflict ownership here.** `onWriteConflict` broadcasts to every webview, and the backend parks
  the operation on a single stored oneshot sender (`src-tauri/src/file_system/write_operations/state.rs:48`, an
  `Option<oneshot::Sender<..>>` behind a mutex). Two windows rendering the same prompt is therefore a genuine lost-take
  race, not untidiness: the second answer finds the sender already taken and vanishes. `operation-conflict.svelte.ts`
  shipped the main window's host and keeps the ownership rule pure in `operation-conflict-rules.ts`, so M3's job is to
  name which window may render and resolve, and to put that rule in the session rather than leaving it implicit in who
  happened to be listening.
- **Also classify `archive_needs_password`**, which has the same shape: intercepted upstream of the error dialog
  (`dialog-state.svelte.ts:553`), and an unlock re-dispatches a new operation rather than resuming the parked one. In
  scope to classify (is it a session concern, a birth concern, or a view concern?), out of scope to solve.
- **Tests, TDD (red first):** each command's in-flight guard, that a paused operation is read from the snapshot status
  and never from `is_running`, and that a command issued from one view is observed by another view of the same session.
  That last one is the "commands don't care where they come from" property, which is also the MCP story.
- **Docs:** this milestone invalidates a `transfer/CLAUDE.md` must-know verbatim (`:52-54`: Queue and F2 are
  frontend-only, set `backgrounded`, and that flag makes `onDestroy` skip its safety-net cancel). Rewrite it rather than
  patch it. `queue/CLAUDE.md`'s command story moves too, since per-row pause/resume/cancel becomes a session call.
- **Checks:** `pnpm check svelte -q`.

### M4: the progress dialog becomes a view

Re-express `createTransferProgressState` as: a **dispatch** path (scan-wait, `dispatchOperation`, the destroyed-during-
dispatch rule, the `mcp-response` replies) that produces a session, plus a **view** (`MIN_DISPLAY_MS`, dismissal, the
settle-slow label, the cancel-settle fallback) bound to it. `pendingEvents` and `replayBufferedEvents` are deleted, not
moved: M1's fan-out buffering replaced them. The guard retirements above land here.

- **Why last among the refactors:** it is the riskiest edit in the frontend's most stateful module, and by the time it
  starts the session has been proven by three consumers and one behavior change. Doing it first would mean debugging the
  session design and the dialog rewrite at the same time, in the one place where a mistake cancels a user's transfer.
- **Decide, don't inherit: `MIN_DISPLAY_MS` becomes view-local.** `startTime` (`:209`) is set at dispatch (`:754`), so
  it is the operation's start, not the view's. An adopted view's elapsed time is enormous and the anti-flicker floor
  never applies, which is probably right (nothing flickered; the dialog was open for twenty minutes) but should be a
  written decision rather than an accident of which variable moved where.
- **`activity` and stall detection need no work**, and this line exists so nobody re-litigates it: `transfer-stall.ts`
  is pure over `TransferActivity`, and `activity` rides every `write-progress` event, so a late-attaching view gets a
  correct stall notice on the next tick with zero state to reconstruct.
- **Tests:** the existing suites in `transfer-progress-state.svelte.test.ts` (887 lines) and the six
  `TransferProgressDialog.*.test.ts` files (a11y, cancel-settle, conflict, flushing, queue, rollback) are the contract.
  They must keep passing, and where one encodes a dialog-owns-the-operation assumption, the assumption changes and the
  test changes with a written reason. Written-after for the restructuring itself, with one exception that is red first:
  `transfer-progress-state.svelte.test.ts:804`, "fires the safety-net cancel for an unexpected teardown", encodes the
  contract this milestone deletes. It must be **replaced** by a named new test asserting the opposite ("an unexpected
  teardown leaves the operation running"), not quietly removed. A deleted test with no replacement is indistinguishable
  from a regression.
- **Docs:** the module doc rewritten around the split; the `backgrounded` explanation survives wherever the flag does.
- **Checks:** full `pnpm check -q`, plus the transfer E2E specs run in isolation.

### M5: Foreground (the payoff)

A "Foreground" button on a running row in the operation queue adopts that operation into the rich progress dialog on the
main window.

- **Mechanics:** the row carries its `operationId`, so "the one clicked" is free with parallel operations. The queue
  window is a separate webview, so the click crosses windows, and it already holds `core:event:default`
  (`src-tauri/capabilities/queue.json`), so a small typed event the main window listens for is the clean path (and it
  collapses to a direct call if the queue becomes a popup). The main window creates or reuses the session for that id
  and mounts the dialog as a view.
- **Falls out for free:** the chip already hides the foreground-owned operation via `setForegroundOperationId`, and
  re-backgrounding is detaching the view again.

**The hard problem is birth context, and it is not solvable by the session.**

`OperationSnapshot` (`lib/ipc/bindings.ts:6753-6767`) carries exactly `operationId`, `operationType`, `status`,
`source`, `destination`, `supportsRollback`, and `error`. It does not carry `sourcePaths[]`, `fileCount` /
`folderCount`, `sourcePaneSide`, or `sourceVolumeId`. Those come from `transferProgressProps`, captured in the pane at
dispatch time.

That matters because completion is not just rendering. `handleTransferComplete` (`dialog-state.svelte.ts:457-504`) uses
`sourcePaths` to purge every stored search-results snapshot, uses `fileCount` / `folderCount` to compose the completion
toast ("Moved 1 file and 3 folders"), then calls `refreshPanesAfterTransfer()`, `clearOperationSnapshot()`, and
`clearSourcePaneSelection()` against a pane chosen by `sourcePaneSide` (`:207`). Foreground an operation started twenty
minutes ago, in a pane that has since navigated somewhere else, and completion mutates the wrong pane's selection while
raising a toast that cannot name what moved.

So the split is not two-way but three-way, and M5 is where that gets settled: **what the operation did** belongs to the
session, and **what this pane should do about it** belongs to the view and is bound to birth. A view that adopted an
operation has no birth context and must degrade honestly rather than guess: no pane refresh, no selection mutation, no
snapshot purge, and a completion toast that says only what the snapshot knows. A view that started the operation keeps
doing exactly what it does today.

Three more things to decide in the milestone:

- **The dialog slot is single-occupancy.** `showTransferProgressDialog` / `transferProgressProps`
  (`dialog-state.svelte.ts:175-176`) hold one dialog, matching `foreground-operation.svelte.ts`'s deliberate single-slot
  invariant. Define what Foreground does when a dialog is already open for a different operation: refuse, swap, or
  queue. Refusing is the honest default and needs a way to say so.
- **Raise the main window.** The queue window holds `core:window:allow-set-focus`, and without an explicit raise the
  adopted dialog opens behind the queue window, which reads as the button doing nothing.
- **The button's label is not free.** A new user-facing string means a catalog key with its `@key` description, nine
  more locales, the i18n parity checks, and a `.a11y.test.ts` for the new control. Budget it.

Open questions to settle in the milestone, not now: which phase an adopted operation enters (straight to active; it
missed scanning, and inventing a scan phase would be a lie), and what Rollback offers (the snapshot's `supportsRollback`
answers it).

- **Tests, TDD (red first):** adopting a running operation yields a dialog showing its live progress; adopting the same
  operation twice does not create a second session; foregrounding then backgrounding leaves the operation running; and
  an adopted view's completion mutates no pane.
- **Docs:** `queue/CLAUDE.md` and `DETAILS.md`, plus the birth-context rule in the session module's `DETAILS.md`.
- **Checks:** full `pnpm check -q`, plus a real-app run: start a copy, background it, foreground it from the queue,
  confirm live progress and that backgrounding again keeps it alive.

### M6: the scan joins the session (optional, decide after M5)

David's model says the operation begins when the TransferDialog opens, in a "scan first" state, and the Copy/Move button
is a transition inside it rather than the operation's birth.

Today the scan lives in `TransferDialog` and is handed across as `scanInProgress` + `previewId`. Moving it into the
session would match the model exactly.

**The obstacle, stated honestly:** a session is keyed by `operationId`, and during the scan there is no `operationId`,
because the backend has not been asked to do anything yet. Two ways out:

- (a) **Leave the boundary where it is.** The session starts at dispatch; the scan stays in the pre-flight dialog. Less
  faithful to the model, zero risk, and everything above still works.
- (b) **Give the session a pre-identity phase.** It is born at dialog-open with a local token, runs the scan, and adopts
  the real `operationId` when dispatch returns. Faithful to the model, and more expensive: the session must survive the
  pre-flight dialog closing, and the registry gains a second key space.

(b) buys model fidelity and nothing else. It does **not** remove the buffer/replay problem, which is the payoff it might
look like it has. The buffer exists because events arrive keyed by an id the frontend does not know yet, and a session
born at dialog-open still cannot claim `write-progress` for `op-1` until dispatch returns the mapping. The race is
dispatch-response versus first-event, and birth time does not touch it. What actually kills the buffer is the fan-out's
central unknown-id buffering, which lands in M1 under option (a).

With the payoff gone, (b) is mostly cost. Decide after M5, leaning decline unless the model fidelity buys something
concrete by then. Do not let this question block M0 through M5.

## What this does not change

- The backend, except possibly M0's quit-versus-reload hook. No new events, no new IPC, no manager changes. This is a
  frontend restructuring of state that already crosses the wire.
- The operation's semantics. Pause still parks between files, mid-large-file pause is still unimplemented, and rollback
  availability still comes from the snapshot.
- Where copy lives. Every user-facing string stays in the catalog.

## Risks

1. **M4 is a large edit to the most stateful frontend module in the app**, and its test suite encodes the current
   ownership model in places. Expect to change tests, and expect each such change to need a written reason: a test
   changed without one is how a real regression gets waved through.
2. **M0 and the guard retirements are behavior changes**, not refactors. They are argued above, and they should be
   argued again at implementation time against whatever the code looks like then.
3. **Refcount leaks.** A session that is never released holds listeners for an operation that ended, so the registry
   needs a settle-driven sweep and not just view-driven release. The sweep must be driven by the terminal events, and it
   must not sweep a retained `failed` row: those persist on the snapshot by design until someone dismisses them, and a
   sweep that treats "settled" as "gone" would delete the session behind a failure the user has not read yet.
4. **Two efforts touched this area while the spec was written** (the queue bar labels plus the Queue/Background button,
   and the main-window conflict prompt). Both have landed. Read what actually shipped rather than this spec's
   description of it.
