# Operation sessions: the dialogs become looking glasses

Status: proposed. Spec only; nothing implemented. Line numbers are as of `5d75512ab`.

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

The code contradicts that model in two places. One is the module this spec is about. The other is the window between
confirming a transfer and the backend naming it, which is where a user currently loses control of their own operation.

`createTransferProgressState` (`apps/desktop/src/lib/file-operations/transfer/transfer-progress-state.svelte.ts`, 1,299
lines) does not observe an operation. It **owns** one. It scans, it dispatches, and only then learns its `operationId`.
The dialog is not a looking glass; it is the thing that starts the process and happens to render it.

So there is no way to say "attach to operation X", only "start one and watch it". That is why the Foreground button (the
motivating feature: click a running row in the operation queue, get the rich progress dialog back) cannot be built
today.

## What the module actually conflates

Reading it end to end, there are three concerns in one factory:

1. **Session**: per-operation, lives exactly as long as the operation. `operationId` (`:202`), `phase` (`:203`),
   `currentFile`, `filesDone` / `filesTotal` / `bytesDone` / `bytesTotal` (`:205-208`), `bytesPerSecond` /
   `filesPerSecond` (`:322-323`), the smoothed `etaSecondsDisplay` (`:327`), `activity` (`:331`), `opStatus` from
   `operations-changed` (`:246`), `conflictEvent` and `isResolvingConflict` (`:304-305`), `isCancelling` /
   `isRollingBack` / `operationSettled` (`:210-211`, `:240`), the terminal-event bookkeeping (`cancelEventReceived`
   `:216`, `settleEventReceived` `:219`, and the single-shot `cancelEventPayload` `:222`, which together decide when a
   cancel is actually over), and the pause/resume/cancel/rollback commands.
2. **Birth**: one-time, ends the moment the backend returns an id. `config.scanInProgress` (`:120`) and the scan-wait
   path (`waitForScanThenStart`, `:1073-1166`), `dispatchOperation` (`:631-694`) and its two helpers, the
   `pendingEvents` buffer (`:264`) plus `replayBufferedEvents` (`:275-301`) that exist only because events can arrive
   before the dispatch response does, the `mcp-response` reply on success (`:797`) and on failure (`:856`), and the
   destroyed-during-dispatch cancel (`:807-815`).
3. **View**: belongs to a particular piece of UI, not to the operation. `MIN_DISPLAY_MS` (`:169`), `dismissed` (`:234`),
   `settleSlow` (`:225`) and `SLOW_SETTLE_LABEL_MS` (`:174`), `CANCEL_SETTLE_FALLBACK_MS` (`:188`) and its timer
   (`:230`), `maybeFinishCancelClose` (`:565-583`), the `onComplete` / `onCancelled` / `onError` / `onQueue` callbacks
   (`:124-128`), `backgrounded` (`:251`), and the safety-net cancel in `destroy()` (`:1194-1200`).

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

The reason is a real shipped bug, though not quite the one it looks like. `progress-readout.ts`'s own header records it
(`:20-24`): the copy dialog put `etaSeconds` through `createEtaSmoother()` while the queue window rendered
`progress.etaSeconds` raw, so one operation read "8m 12s remaining" in one window and "5m 46s" in the other. That was
smoothed versus raw, and it is already fixed by both surfaces rendering the same smoothed value.

So "two smoothers give two answers" is not the argument, because the EMA is deterministic: two smoothers fed the same
samples from the same starting point agree exactly. The argument is the one a Foreground button creates. **Smoothers
diverge when they start at different times.** A view that attaches twenty minutes in builds a smoother whose first
sample is the current rate, while the queue's smoother carries twenty minutes of history, and the two disagree on screen
for as long as the new one takes to converge. Late attachment is the whole point of this spec, so the registry is what
keeps "one operation, one truth" structural rather than remembered.

### Why sessions read a fan-out, not their own listeners

Today the dialog subscribes to seven event streams (`:761-772`). Ten sessions must not mean seventy subscriptions, but
listener count is the least of it. The fan-out is a correctness boundary: one place that buffers events arriving for an
id nobody has claimed yet, one arrival order, and one authority on which ids are live.

**It is a new module, not an extension of the operations store.** `createOperationsStore()` subscribes to two of the
seven streams (`operations-changed` and `write-progress`, `queue/operations-store.svelte.ts:161-166`), is a reducer over
all operations at once, and has no per-id attach API to extend. Making it the fan-out would mean rewriting it into
something else while three surfaces render from it. Instead the demultiplexer sits underneath, and both
`createOperationsStore()` and the session registry become its consumers.

The behavior that earns it its place in M2: **it buffers events for unknown ids and flushes them on registration.** That
is what makes `pendingEvents` and `replayBufferedEvents` deletable in M5 rather than portable to the session, and it is
why the fan-out lands before anything reads it.

### Sessions are per-window, and that is fine

The operation queue is a separate `WebviewWindow`, so a session cannot be shared across windows; each webview builds its
own from the same broadcast events. The backend registry stays the single source of truth, and sessions are a per-window
projection of it. Worth stating plainly because it looks like a violation of "one session per operation" and isn't.

Note David is considering moving the queue out of a separate window into a soft popup inside the main window. That
change would make the queue's sessions the _same_ sessions as the main window's, which this design absorbs without
alteration. Don't build anything that assumes two windows.

## The scan-wait has no name, and that is a live bug

M1 exists because of a defect David hit on 2026-08-11, and it is the one milestone here that a user would notice on its
own.

The scan preview lives entirely in the backend, keyed by `previewId`
(`src-tauri/src/file_system/write_operations/scan_preview.rs:44-90`, tracked in the `SCAN_PREVIEW_STATE` map at
`scan_cache.rs:286-287`). It emits `scan-preview-progress` / `-complete` / `-error` / `-cancelled` to every webview, and
it has **no operation record**: `scan_preview.rs` never touches `manager.rs`. No `operationId`, no queue row, no lane
reservation, no busy-volume entry, no quit-gate visibility.

`TransferDialog` starts that preview when it opens. If the user confirms before the walk finishes, the progress dialog
takes over the wait in `waitForScanThenStart` (`:1073-1166`), and for however long the scan runs there is still no
`operationId`. Three consequences, all shipped:

- `canPauseOrQueue` (`:310-317`) ends with `operationId !== null`, so **Pause and Queue/Background do not render** during
  the scan.
- `handleQueue` (`:988-1005`) opens with `if (!operationId || backgrounded) return` (`:989`), so even a synthetic click
  would do nothing.
- `destroy()` cancels the preview when the dialog unmounts (`:1189-1192`), and `handleCancel`'s scan branch
  (`:882-890`) does the same on a close. **The scan dies with the dialog.**

Net: you cannot background a transfer while it is still scanning, and the scan cannot outlive its viewer. That is
exactly the coupling this spec exists to remove, arriving before the operation is even born.

Two things make it worse than a corner case:

- **The confirm button now acts immediately.** `TransferDialog`'s confirm no longer awaits the pre-flight conflict check
  (`pre_known_conflicts` is consumed only under a `Skip` resolution, `transfer_driver/mod.rs:256-262`, and the policy
  radios only render after the check completes, so nobody can have chosen `Skip` while it is pending). The MCP
  auto-confirm path with `conflictPolicy === 'skip'` still awaits. So for any large transfer, landing in the progress
  dialog on a scan you cannot background goes from a corner case to the normal opening experience.
- **The quit gate cannot see it.** `blocks_quit` (`src-tauri/src/quit/mod.rs:108-126`) reads
  `list_operations()`, so a scan-waiting transfer holds nothing back: ⌘Q proceeds silently and the scan dies. Confirmed
  work that a user is watching should hold a quit.

The fix is to give the confirmed operation a real backend record from the moment of confirmation, and to move the wait
behind it. That is M1, and it is argued in full in its milestone below.

## The in-dialog guards, and why they retire

There are **two**, not one, and they are separate decisions.

1. `destroy()` (`:1183-1202`, doc comment from `:1179`) fires `cancelWriteOperation(operationId, false)` (`:1199`) when
   the dialog unmounts with the operation unsettled and not backgrounded.
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
  unlock re-dispatches a NEW operation with a new id rather than resuming the old one
  (`file-explorer/pane/dialog-state.svelte.ts:554`).
- **Dispatch failure.** The catch at `:850-869` fires before any `operationId` exists, and the guard requires one.
- **Cancel before the id arrives.** `handleCancel`'s `if (!operationId) { destroyed = true; return }` (`:892-896`) is
  covered by the destroyed-during-dispatch branch at `:807-815`, which cancels as soon as the id lands. That is birth
  logic, and it stays (with the corrected rule noted above). M1 shrinks this window from "the whole scan" to "one IPC
  round trip", which is what makes the corrected rule easy to hold.
- **Page unload.** M0's territory, and today over-covered rather than under-covered.

That leaves no surviving path where retiring guard 1 loses an operation, which is the evidence the retirement rests on.
Keeping it would also invert the model: "the view went away, so stop the operation" is the coupling this whole spec
exists to remove.

One claim the earlier framing overstated and this spec should not repeat: silent background work is not impossible
today. `pickChipOperation` (`status-corner/operation-chip.ts:127-140`) shows the first `running` row, else the first
`paused` one, so an operation sitting in `queued` shows nothing at all (its own test pins this), and `CHIP_SETTLE_MS`
(`:21`) holds the chip back 500 ms before it first appears. Both are small, both predate this work, and neither blocks
anything here. Decide them separately.

### The `$state`-during-disposal trap must be carried forward

The module doc explains at length (`:22-32`) why `backgrounded` and `destroyed` are plain `let`s and not `$state`: a
rune read during synchronous reactive-scope disposal returns a stale value, so the guard passed wrongly and cancelled a
just-backgrounded operation (the transfer died and the queue window opened empty). Any flag a teardown path reads
synchronously has the same hazard. Keep that reasoning attached to whatever teardown logic survives, and don't
"modernize" those `let`s.

## Milestones

Sequential. M2 through M6 each move state out of a module the next one edits, so none of them are safe to parallelize.
M1 is the one milestone that could in principle run beside M2 (it is backend work plus deletions, and the fan-out
neither reads nor writes what it touches), but it lands first anyway, for the reasons argued under it.

### M0: an operation survives a reload

**Done.** Specced separately: `docs/specs/quit-and-operation-lifetime.md`, whose Q1-Q3 have all landed.

Every "the operation outlives the view" sentence above used to be false, and not because of the dialog: a `beforeunload`
handler in `(main)/+layout.svelte` walked the GLOBAL registry and stopped every operation, backgrounded ones included.
Start a copy, press Queue, hot-reload the main window, and the transfer died while the queue window sat there rendering
a row for it.

- **Why first:** every later milestone's "the operation outlives the view" reasoning is false until this lands, so
  shipping the session first would mean proving session lifetimes against a runtime that kills them.
- **What it settles for the milestones below:** the backend owns operation lifetime and the quit decision, local writes
  stage through temp+rename, and a window closing is just a viewer detaching. The quit gate (`src-tauri/src/quit/`) is
  the only thing that stops work.

### M1: the scan-wait gets a name

Give a confirmed-but-still-scanning transfer a real operation record in the backend registry, so it is a queue row from
the first frame and the progress dialog is already only a view of it.

**The change, concretely.** The write commands already take `previewId` and already return `{ operationId }` the moment
`spawn_managed` registers the record (`manager.rs:334-360`). What changes is what the backend does when `previewId`
names a preview that has not finished: instead of the frontend awaiting `scan-preview-complete` and dispatching
afterwards, the frontend dispatches immediately and the **operation's own task** awaits the preview before it starts
writing. `take_cached_scan_result` (`scan_cache.rs:199`) already returns `None` for a preview still in flight, and
preflight already falls back to its own walk, so today an early dispatch would silently duplicate the scan. Waiting
inside the task is what makes the early dispatch correct.

- **No new IPC and no changed signatures.** `copy_between_volumes`, `move_between_volumes`, `move_files`,
  `delete_files`, `trash_files`, and `compress_files` all keep their current shape (`commands/file_system/write_ops.rs`).
  Only the behavior behind `previewId` changes.
- **Don't poll for the preview.** `check_scan_preview_status` (`write_ops.rs:339-345`) is a poll because IPC has no
  other shape; inside the process there is no such excuse, and "subscribe, don't poll" is a house principle. Give
  `ScanPreviewState` (`scan_cache.rs:26-29`) a completion signal the task can await. The two worker exits that remove
  their own `SCAN_PREVIEW_STATE` entry (`scan_preview.rs:247-249` local, `:387-389` volume) are where it fires, and both
  must fire it on the error and cancelled paths too, or a waiting operation hangs forever.

**Why the id is minted at confirm, not at dialog open.** David's model says the operation begins when the TransferDialog
appears. This milestone deliberately starts it one step later, and the reason is what a queue row promises. A row says
"something is happening on your behalf, and here is how to control it". Before confirm there is no destination, so the
row cannot say what it is doing; Pause is meaningless; Cancel means "close the dialog you are looking at"; and
`blocks_quit` would start prompting on ⌘Q because a picker is counting files. Confirm is the exact moment intent becomes
a process, and it is also the exact moment the current code loses the thread. Serving the model's *purpose* (the dialogs
are looking glasses, not the process) does not require minting identity for something the user has not committed to.
The pre-confirm scan stays where it is, in `TransferDialog` and `transfer-scan-state.svelte.ts`. This is a decision, not
a deferral: see "What we decided not to do" below.

**One id, no handoff.** The scan-wait and the write are one record with one `operationId` from registration to settle,
because there is nothing to hand off: the record is created by the write command, and awaiting the preview is simply the
first thing its task does. There is no separate "scan record" to retire and no second key space. The seamless queue row
is not a feature we build; it is what falls out of never having minted a second id. (`previewId` stays a separate
identifier for the preview itself, which is right: the same preview can outlive one dialog and feed a later dispatch,
and the pre-confirm scan has no operation at all.)

**Status: reuse `Running`, and let `phase` carry the distinction.** Do not add a `LifecycleStatus::Scanning`.

- `phase: 'scanning'` already exists on `write-progress`, the backend already emits it during its own re-scan, and
  `handleProgress` already renders it (`:381-393`, with `TransferProgressDialog.svelte:362` mounting `ScanPhaseBody`
  from it). The scan-phase UI is built; it just never fires on the path this milestone fixes.
- A new `LifecycleStatus` variant is a compile error in only two places (`quit/mod.rs:109-114` and
  `mcp/resources/operations.rs:50-62`) and degrades **silently** everywhere else: `queue.row.status`
  (`intl/messages/en/queue.json:35`) falls through to `other {Running}` and would display the wrong word;
  `operations_are_idle` (`mcp/executor/async_tools.rs:272-276`) matches only `Running | Queued`, so `await
  operations_idle` would return immediately while a scan ran; `hasRunning` / `hasPaused`
  (`operations-store.svelte.ts:199`, `:203`), `pickChipOperation`, and `queue-backlog.ts:28-35` would all quietly
  disagree about whether anything is happening. Reusing `Running` makes every one of those correct with no edit.
- The visible consequences follow for free: the corner chip picks the operation up, `hasOtherQueuedWork` counts it, and
  `await operations_idle` waits for it.

**The descriptor.** `OperationDescriptor` (`manager.rs:105-118`) is filled at registration with the values the WRITE
will need, not the scan's:

- `lanes`: the same source and destination lanes the write takes (`transfer/volume/copy.rs:206`, `local_lanes` at
  `write_operations/mod.rs:378-383`). With `LANE_BUDGET = 1` (`manager.rs:254`), a transfer confirmed while another runs
  on the same lane is admitted as `Queued` and the existing auto-queue path backgrounds it, which is correct and needs
  no new code. The preview is already walking independently, so waiting for the lane costs nothing.
- `volume_ids`: as the write needs them. **This is a behavior change worth stating:** Eject becomes disabled from
  confirm rather than from first byte. That is the right answer (the operation is committed), but it is new.
- `supports_rollback`: the value the write will have (`matches!(type, Copy | Move)` and the per-site values at
  `mod.rs:276`, `copy.rs:220`, `move.rs:166`, and the rest). It is a promise about the operation, not a statement about
  the current phase. Rollback must be **disabled during scanning** because there is nothing to undo yet, and that is a
  view decision keyed on `phase`, never on `supportsRollback`. Getting this backwards means a row that offers Rollback
  and then can't, or one that never offers it at all.
- `summary`: source and destination, both known at confirm.

**`OperationSnapshot` needs no new fields.** It carries `operationId`, `operationType`, `status`, `source`,
`destination`, `supportsRollback`, and `error` (`manager.rs:153-166`, `lib/ipc/bindings.ts:6834-6851`), and byte counts
were never on it: they ride `write-progress`. "No bytes yet" is already representable, because `OperationRow.progress`
is `WriteProgressEvent | null` and is null until the first tick (`operations-store.svelte.ts:37-45`). A scanning row is
an ordinary running row whose progress happens to say `phase: 'scanning'`.

**Cancel maps straight through, and the settle contract is untouched.** `cancel_operation(id)` during the scan-wait sets
the operation's cancellation token; the wait aborts; the task's cleanup calls `cancel_scan_preview(previewId)`
(`scan_preview.rs:111-118`) so an abandoned walk stops instead of finishing for nobody, then emits `write-cancelled` and
`write-settled` and calls `on_settled` (`manager.rs:450-454`). The frontend's cancel path needs no change at all: it
already issues `cancelWriteOperation` and waits for both terminal events. The special-case scan branch in `handleCancel`
(`:882-890`) is **deleted**, not adapted, and that deletion is the proof the mapping worked.

**The quit gate needs no edit.** A scanning transfer is `status: Running` with `operation_type: Copy` (or Move / Delete /
Trash / ArchiveEdit), so `blocks_quit` (`quit/mod.rs:108-126`) already returns true. ⌘Q during a scan starts prompting,
which today it does not. A scanning operation has written nothing, so it cancels instantly and the gate's 2-second
budget is never at risk.

**Frontend deletions.** This milestone removes code rather than adding it, which is most of why it is worth doing before
the restructuring:

- `waitForScanThenStart` (`:1073-1166`), `isOurScanEvent` (`:1053-1056`), `cleanupScanListeners` (`:1045-1051`), the four
  `onScanPreview*` subscriptions, and the `checkScanPreviewStatus` race resolution (`:1152-1160`).
- `config.scanInProgress` (`:120`) and the branch in `start()` (`:1171-1177`), which collapses to `void startOperation()`.
- The seven scan `$state` fields and their getters (`:191-199`, `:1212-1232`), and the `ScanThroughput` instance
  (`:197`): scan-phase throughput now arrives through `handleProgress`'s scanning branch (`:381-393`), which already
  computes it.
- The scan branch in `handleCancel` (`:882-890`) and the preview cancel in `destroy()` (`:1189-1192`).
- **One of the two duplicate scan bodies in `TransferProgressDialog.svelte`.** It renders `ScanPhaseBody` twice, once
  under `{#if waitingForScan}` (`:302`) and once under `{#if phase === 'scanning'}` (`:362`), because the same UI had to
  be fed from two different state sources. One source means one body. This duplication is a real defect in its own
  right, and collapsing it is how you know the milestone actually unified the two paths.

**Why it lands first.** Three reasons, in order of weight:

1. **It is a bug fix, not a refactor.** A user cannot background a scanning transfer today, and the scan dies with the
   dialog. Everything else in this spec is invisible until M6. Shipping a fix behind four milestones of restructuring is
   the wrong order.
2. **It shrinks the module M5 has to rewrite,** by roughly 150 lines and by one of the three members of the "Birth"
   concern. M5 is the riskiest edit in the app's most stateful frontend module; every line M1 deletes is a line M5 does
   not have to re-express.
3. **It nearly closes the `!operationId` window,** from "the whole scan" to "one IPC round trip". The
   destroyed-during-dispatch rule and the guard retirements above are much easier to argue against a millisecond gap
   than a multi-minute one.

The cost of going first is one double-touch: the queue row learns to render a scanning operation against today's store,
and M3 then re-points it at a session. That is small (the row reads `progress.phase`, which the store already carries
verbatim) and it is the right trade against shipping the fix late.

- **Tests, TDD (red first).** This is a bug fix in a data-writing path, so it earns real red:
  - Rust unit: a write command dispatched with an in-flight `previewId` registers its operation immediately and reports
    `Running` on `list_operations()` before the preview completes.
  - Rust unit: the same operation, once the preview lands, consumes the cached result rather than re-walking (assert on
    the scan-cache take, not on timing).
  - Rust unit: `cancel_operation` during the scan-wait cancels the underlying preview and emits `write-cancelled` +
    `write-settled`. Pair it with the reverse: a preview that errors settles the operation as a failure rather than
    hanging.
  - Rust unit: `blocks_quit` is true for a scanning operation (`quit/tests.rs` has the pattern at `:154-195`).
  - Vitest: the progress dialog exposes a non-null `operationId` and `canPauseOrQueue` while `phase === 'scanning'`,
    and Queue backgrounds it. This is the user-visible bug; write it first and watch it fail.
  - Vitest, written after: the deletions keep the existing `transfer-progress-state.svelte.test.ts` scan cases passing
    where they describe outcomes, and the ones that describe the scan-wait *mechanism* are replaced by cases against the
    new path, each with a written reason.
  - E2E: confirm a large copy, background it from the queue button while the scan-phase readout is still up, and see the
    row in the queue window. This is the regression that matters and it cannot be proven at the unit level.
- **Docs:** `write_operations/CLAUDE.md` and `DETAILS.md` (the scan preview gains an operation record; state the
  identity rule: one `operationId` from confirm, `previewId` still names the preview), `scan_preview.rs`'s module doc
  (it now has a completion signal an operation awaits), `transfer/CLAUDE.md` (the scan-wait must-know is now wrong),
  `queue/CLAUDE.md` and `DETAILS.md` (a running row may be in `phase: 'scanning'`; Rollback is phase-gated),
  `quit/`'s docs if they enumerate what holds a quit, and a line in `docs/architecture.md`.
- **Checks:** `pnpm check rust -q` and `pnpm check svelte -q` while iterating, full `pnpm check -q` before wrapping,
  plus the transfer E2E specs.

### M2: the fan-out and the session module, read-only

Introduce the per-window event demultiplexer and `operation-session` alongside the existing code, changing no behavior.
A session binds to an `operationId`, attaches to the fan-out, and exposes the derived read state (phase, metrics, rates,
smoothed ETA, status, settled). No commands yet, no dispatch, no view concerns. Registry with refcounted create/release.

- **Why here:** it can be proven correct against the live streams before anything depends on it, and the fan-out's
  unknown-id buffering has to exist before M5 can delete the dialog's private buffer.
- **Deliverable, not just a check: seed a new session from `list_operations`.** With M0 landed, operations genuinely
  survive a reload, so a reloaded main window must recover them or the chip shows nothing for a transfer that is very
  much still running. Seeding needs a defined **miss case**: terminal operations leave the snapshot entirely
  (`write_operations/DETAILS.md:248`; retained failures are the one exception, `manager.rs:619-674`), so an operation
  that finished between the click and the mount seeds nothing, and the session must resolve to "already gone" rather
  than hang empty.
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

### M3: the corner chip and the queue rows read sessions

Move the two existing ambient surfaces onto sessions. They are the low-risk consumers: read-only, no dispatch, no
teardown semantics.

- **Why here:** the session's read surface has to be proven sufficient for real UI before the hard part, and this is the
  cheapest proof available. It also collapses the chip's and the rows' separate progress derivation into one place,
  which is the "one operation, one truth" property the registry exists for, made visible.
- **Regression risk to handle head-on:** `operations-store.svelte.ts` already keeps a per-`operationId` smoother map
  (`:92-96`) in every window. M3 must **delete** the store's smoothing and read the session's, not stack a second layer.
  Two smoothers over one stream is exactly the divergence the registry is meant to prevent.
- **Carries M1's scanning row.** The scan-phase rendering M1 added to the queue row moves onto the session with
  everything else. Keep the phase gating on Rollback; it is a rule, not an accident.
- **Tests:** written after, because this is a substitution behind an unchanged surface and the existing chip and row
  suites are the contract. They must pass unchanged, which is the point. Add one new test: a chip and a row bound to the
  same operation report the identical ETA.
- **Docs:** `status-corner/DETAILS.md` and `queue/DETAILS.md` updated to point at the session as the source.
- **Checks:** `pnpm check svelte -q`, plus the queue E2E.

### M4: commands move to the session

Pause, resume, cancel, rollback, and resolve-conflict become session methods. Views call them; nothing else changes.

- **Why separate from M3:** commands have failure modes (in-flight guards, the paused-operation-reports-`is_running`
  trap) and deserve their own red.
- **Settle cross-window conflict ownership here.** `onWriteConflict` broadcasts to every webview, and the backend parks
  the operation on a single stored oneshot sender (`src-tauri/src/file_system/write_operations/state.rs:45`, an
  `Option<oneshot::Sender<..>>` behind a mutex). Two windows rendering the same prompt is therefore a genuine lost-take
  race, not untidiness: the second answer finds the sender already taken and vanishes. `operation-conflict.svelte.ts`
  shipped the main window's host and keeps the ownership rule pure in `operation-conflict-rules.ts`, so M4's job is to
  name which window may render and resolve, and to put that rule in the session rather than leaving it implicit in who
  happened to be listening.
- **Also classify `archive_needs_password`**, which has the same shape: intercepted upstream of the error dialog
  (`file-explorer/pane/dialog-state.svelte.ts:554`), and an unlock re-dispatches a new operation rather than resuming
  the parked one. In scope to classify (is it a session concern, a birth concern, or a view concern?), out of scope to
  solve.
- **Tests, TDD (red first):** each command's in-flight guard, that a paused operation is read from the snapshot status
  and never from `is_running`, and that a command issued from one view is observed by another view of the same session.
  That last one is the "commands don't care where they come from" property, which is also the MCP story.
- **Docs:** this milestone invalidates a `transfer/CLAUDE.md` must-know verbatim (`:52-54`: Queue and F2 are
  frontend-only, set `backgrounded`, and that flag makes `onDestroy` skip its safety-net cancel). Rewrite it rather than
  patch it. `queue/CLAUDE.md`'s command story moves too, since per-row pause/resume/cancel becomes a session call.
- **Checks:** `pnpm check svelte -q`.

### M5: the progress dialog becomes a view

Re-express `createTransferProgressState` as: a **dispatch** path (`dispatchOperation`, the destroyed-during-dispatch
rule, the `mcp-response` replies) that produces a session, plus a **view** (`MIN_DISPLAY_MS`, dismissal, the settle-slow
label, the cancel-settle fallback) bound to it. `pendingEvents` and `replayBufferedEvents` are deleted, not moved: M2's
fan-out buffering replaced them. The guard retirements above land here. M1 already removed the scan-wait from this
module, so dispatch is a single path rather than two.

- **Why last among the refactors:** it is the riskiest edit in the frontend's most stateful module, and by the time it
  starts the session has been proven by three consumers and one behavior change. Doing it first would mean debugging the
  session design and the dialog rewrite at the same time, in the one place where a mistake cancels a user's transfer.
- **Decide, don't inherit: `MIN_DISPLAY_MS` becomes view-local.** `startTime` (`:209`) is set at dispatch (`:754`), so
  it is the operation's start, not the view's. An adopted view's elapsed time is enormous and the anti-flicker floor
  never applies, which is probably right (nothing flickered; the dialog was open for twenty minutes) but should be a
  written decision rather than an accident of which variable moved where.
- **`activity` and stall detection need no work**, and this line exists so nobody re-litigates it: `transfer-stall.ts`
  is pure over `TransferActivity`, and `activity` rides every `write-progress` event (`:372`), so a late-attaching view
  gets a correct stall notice on the next tick with zero state to reconstruct.
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

### M6: Foreground (the payoff)

A "Foreground" button on a running row in the operation queue adopts that operation into the rich progress dialog on the
main window.

- **Mechanics:** the row carries its `operationId`, so "the one clicked" is free with parallel operations. The queue
  window is a separate webview, so the click crosses windows, and it already holds `core:event:default`
  (`src-tauri/capabilities/queue.json:18`), so a small typed event the main window listens for is the clean path (and it
  collapses to a direct call if the queue becomes a popup). The main window creates or reuses the session for that id
  and mounts the dialog as a view.
- **Falls out for free:** the chip already hides the foreground-owned operation via `setForegroundOperationId`, and
  re-backgrounding is detaching the view again.

**The hard problem is birth context, and it is not solvable by the session.**

`OperationSnapshot` (`lib/ipc/bindings.ts:6834-6851`) carries exactly `operationId`, `operationType`, `status`,
`source`, `destination`, `supportsRollback`, and `error`. It does not carry `sourcePaths[]`, `fileCount` /
`folderCount`, `sourcePaneSide`, or `sourceVolumeId`. Those come from `transferProgressProps`, captured in the pane at
dispatch time.

That matters because completion is not just rendering. `handleTransferComplete`
(`file-explorer/pane/dialog-state.svelte.ts:458-505`) uses `sourcePaths` to purge every stored search-results snapshot
(`:471-475`), uses `fileCount` / `folderCount` to compose the completion toast ("Moved 1 file and 3 folders", `:482-488`),
then calls `refreshPanesAfterTransfer()`, `clearOperationSnapshot()`, and `clearSourcePaneSelection()` (`:498-500`)
against a pane chosen by `sourcePaneSide` (`:207-209`). Foreground an operation started twenty minutes ago, in a pane
that has since navigated somewhere else, and completion mutates the wrong pane's selection while raising a toast that
cannot name what moved.

So the split is not two-way but three-way, and M6 is where that gets settled: **what the operation did** belongs to the
session, and **what this pane should do about it** belongs to the view and is bound to birth. A view that adopted an
operation has no birth context and must degrade honestly rather than guess: no pane refresh, no selection mutation, no
snapshot purge, and a completion toast that says only what the snapshot knows. A view that started the operation keeps
doing exactly what it does today.

Three more things to decide in the milestone:

- **The dialog slot is single-occupancy.** `showTransferProgressDialog` / `transferProgressProps`
  (`file-explorer/pane/dialog-state.svelte.ts:176-177`) hold one dialog, matching `foreground-operation.svelte.ts`'s
  deliberate single-slot invariant. Define what Foreground does when a dialog is already open for a different operation:
  refuse, swap, or queue. Refusing is the honest default and needs a way to say so.
- **Raise the main window.** The queue window holds `core:window:allow-set-focus` (`capabilities/queue.json:10`), and
  without an explicit raise the adopted dialog opens behind the queue window, which reads as the button doing nothing.
- **The button's label is not free.** A new user-facing string means a catalog key with its `@key` description, nine
  more locales, the i18n parity checks, and a `.a11y.test.ts` for the new control. Budget it.

Open questions to settle in the milestone, not now: which phase an adopted operation enters (straight to whatever the
next `write-progress` reports, including `scanning` now that M1 makes that a real state an operation can be adopted in),
and what Rollback offers (the snapshot's `supportsRollback` answers it, phase-gated per M1).

- **Tests, TDD (red first):** adopting a running operation yields a dialog showing its live progress; adopting the same
  operation twice does not create a second session; foregrounding then backgrounding leaves the operation running; and
  an adopted view's completion mutates no pane.
- **Docs:** `queue/CLAUDE.md` and `DETAILS.md`, plus the birth-context rule in the session module's `DETAILS.md`.
- **Checks:** full `pnpm check -q`, plus a real-app run: start a copy, background it, foreground it from the queue,
  confirm live progress and that backgrounding again keeps it alive.

## What we decided not to do: a pre-identity session

An earlier draft carried an optional final milestone: give the frontend session a **pre-identity phase**, born at
dialog-open with a local token, running the scan, and adopting the real `operationId` when dispatch returns. Declined,
and the reasoning belongs here so it does not get re-proposed.

- **It mints identity in the wrong place.** The backend registry is this spec's single source of truth for what an
  operation is. A frontend-only token gives the registry a second key space that the backend, MCP, the quit gate, and
  the operation log all know nothing about.
- **Its practical payoff is M1's, and M1 takes it properly.** "You cannot background a scanning transfer" is the real
  complaint, and a backend record fixes it for every surface at once, including the queue window, the corner chip, ⌘Q,
  and `await operations_idle`. A frontend token would fix it for one dialog in one window.
- **It does not kill the buffer/replay problem,** which is the payoff it might look like it has. The buffer exists
  because events arrive keyed by an id the frontend does not know yet, and a session born at dialog-open still cannot
  claim `write-progress` for `op-1` until dispatch returns the mapping. The race is dispatch-response versus
  first-event, and birth time does not touch it. What kills the buffer is the fan-out's central unknown-id buffering,
  in M2.
- **What is left is model fidelity for the pre-confirm scan alone**, and that is precisely the part M1 argues should
  stay unnamed: no destination, no committed intent, no actionable row, and a quit prompt for a file picker.

If this comes back, it needs a new argument, not this one.

## What this does not change

- The operation's semantics. Pause still parks between files, mid-large-file pause is still unimplemented, and rollback
  availability still comes from the snapshot.
- The IPC surface. M1 changes backend behavior behind an existing `previewId` parameter; no command signature changes,
  no new events, and no new `LifecycleStatus` or `WriteOperationType` variants. M2 through M6 are a frontend
  restructuring of state that already crosses the wire.
- The scan preview's own identity. `previewId` still names a preview, previews still broadcast to every webview, and the
  pre-confirm scan in `TransferDialog` is untouched.
- Where copy lives. Every user-facing string stays in the catalog.

## Risks

1. **M1 touches the data-writing path.** Registering an operation earlier means an operation exists that has written
   nothing, and every exit from the scan-wait (complete, error, cancelled, quit) has to reach `on_settled`
   (`manager.rs:450-454`) or the row leaks and its lane stays reserved. The current scan workers are detached
   `std::thread` / `tokio::spawn` with no `JoinHandle` and no RAII guard (`scan_preview.rs:72-87`), which is the
   structural gap to close: the operation's task, not the scan worker, must own the settle.
2. **M5 is a large edit to the most stateful frontend module in the app**, and its test suite encodes the current
   ownership model in places. Expect to change tests, and expect each such change to need a written reason: a test
   changed without one is how a real regression gets waved through.
3. **The guard retirements are behavior changes**, not refactors. They are argued above, and they should be argued again
   at implementation time against whatever the code looks like then.
4. **Refcount leaks.** A session that is never released holds listeners for an operation that ended, so the registry
   needs a settle-driven sweep and not just view-driven release. The sweep must be driven by the terminal events, and it
   must not sweep a retained `failed` row: those persist on the snapshot by design until someone dismisses them
   (`manager.rs:619-674`), and a sweep that treats "settled" as "gone" would delete the session behind a failure the
   user has not read yet.
5. **Efforts kept landing in this area while the spec was written** (the queue bar labels plus the Queue/Background
   button, the main-window conflict prompt, the whole of `quit-and-operation-lifetime.md`, and the immediate-confirm
   change). All have landed. Read what actually shipped rather than this spec's description of it, and re-verify these
   line numbers before trusting one.
