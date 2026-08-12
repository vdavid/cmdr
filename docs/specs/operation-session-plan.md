# Operation sessions: the dialogs become looking glasses

Status: proposed. Spec only; nothing implemented. Line numbers are as of `cce94565d`.

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
(`:19-22`): the copy dialog put `etaSeconds` through `createEtaSmoother()` while the queue window rendered
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
id no session has claimed yet, and one arrival order. It is a router with a session-side holding area, not a gate:
`createOperationsStore()` is a reducer over ALL operations and must keep receiving everything unbuffered, so the fan-out
never second-guesses the `operations-changed` snapshot about which ids exist.

**It is a new module, not an extension of the operations store.** `createOperationsStore()` subscribes to two of the
seven streams (`operations-changed` at `queue/operations-store.svelte.ts:159` and `write-progress` at `:162`), is a
reducer over all operations at once, and has no per-id attach API to extend. Making it the fan-out would mean rewriting
it into something else while three surfaces render from it. Instead the demultiplexer sits underneath, and both
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

- `canPauseOrQueue` (`:310-317`) ends with `operationId !== null`, so **Pause and Queue/Background do not render**
  during the scan.
- `handleQueue` (`:988-1005`) opens with `if (!operationId || backgrounded) return` (`:989`), so even a synthetic click
  would do nothing.
- `destroy()` cancels the preview when the dialog unmounts (`:1189-1192`), and `handleCancel`'s scan branch (`:882-890`)
  does the same on a close. **The scan dies with the dialog.**

Net: you cannot background a transfer while it is still scanning, and the scan cannot outlive its viewer. That is
exactly the coupling this spec exists to remove, arriving before the operation is even born.

Two things make it worse than a corner case:

- **The confirm button acts immediately as of `cce94565d`.** `TransferDialog`'s confirm awaits the pre-flight conflict
  check only under a `needsConflictNames()` gate, because `pre_known_conflicts` is consumed only under a `Skip`
  resolution, at both independent gates (`transfer_driver/mod.rs:256-262` and `transfer/copy/mod.rs:244-245`), and the
  policy radios only render after the check completes, so nobody can have chosen `Skip` while it is pending. The MCP
  auto-confirm path with `conflictPolicy === 'skip'` still awaits. So for any large transfer, landing in the progress
  dialog on a scan you cannot background goes from a corner case to the normal opening experience. It is on `main`, so
  its line numbers are as trustworthy as the rest of this document's.
- **`await scan.scanStarted` outlives the reason it was written for, and the new reason is stronger.** That commit keeps
  it on every confirm path to guarantee the non-null `previewId` the progress dialog's scan-wait needs. M1 deletes that
  scan-wait and the await must stay anyway, because dropping it is a three-part failure, not a missing id. A fast
  confirm would fire `onConfirm` with `previewId = null` while the preview `TransferDialog` already started keeps
  walking. The operation then falls into M1's own miss case and re-walks, so the two walks run **concurrently**, which
  is the exact regression `preview_id` gets threaded through the archive routes to prevent. And the orphaned preview has
  no owner and nothing cancels it, because `cce94565d`'s `confirmed` guard means `handleCancel` never reaches
  `freeAndCleanup()`, so its result sits until a TTL sweep. M1 owns rewriting that comment; a rationale that no longer
  matches the code is how the next person deletes the line.
- **The delete path has the same race and no guard, so M1 fixes it here.** `DeleteDialog.handleConfirm` (`:199`) is
  fully synchronous and passes whatever `previewId` it holds (`:205`), but that field (`:99`) is only assigned at
  `:168`, after the `await startScanPreview(...)` at `:167`. Confirm before that IPC returns and the operation
  dispatches with `previewId = null`. The transfer side guards this with `await scan.scanStarted`; the delete side has
  no equivalent.

  Today the progress dialog's scan-wait absorbs it, and **M1 deletes that scan-wait**, so afterwards a null `previewId`
  on the delete path lands in M1's own miss case: the operation re-walks, concurrently with the preview that
  `startScanPreview` already started, and the orphan is never cancelled, because `onDestroy`'s cleanup is gated on
  `previewId && !confirmed` (`:193-195`) and confirming sets `confirmed` before the id ever arrives. That is finding 7's
  failure, verbatim, on a second path.

  It folds into M1 rather than shipping as a standalone guard for a plain reason: a guard written against today's code
  would be written against the scan-wait, and M1 would then delete it. Give the delete path the same treatment the
  transfer path gets, and write the comment to match.

- **The quit gate cannot see it.** `blocks_quit` (`src-tauri/src/quit/mod.rs:108-126`) reads `list_operations()`, so a
  scan-waiting transfer holds nothing back: ⌘Q proceeds silently and the scan dies. Confirmed work that a user is
  watching should hold a quit.

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
today. `pickChipOperation` (`status-corner/operation-chip.ts:127-137`) shows the first `running` row, else the first
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

The plan now spans two layers, which the old all-frontend framing did not, and that adds one cross-layer constraint
worth stating up front: **M1 is backend-plus-deletions and must land before M5.** M5 restructures
`transfer-progress-state.svelte.ts`, M1 deletes a chunk of it, and doing them in the other order means carefully
re-expressing code that is about to disappear. M1 could in principle run beside M2 (the fan-out neither reads nor writes
what M1 touches), but it lands first anyway, for the reasons argued under it.

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

There are three backend pieces. The second is easy to miss and would ship a worse scan UI than today if it were, and the
third cannot be built the obvious way at all.

**1. The IPC surface is unchanged; two internal signatures are not.** The six write commands keep their Tauri signatures
and `VolumeCopyConfig` already carries `preview_id`, so nothing regenerates in `bindings.ts`. But `preview_id` stops at
the command boundary on two routes. An archive destination goes to `ops_route_archive_copy_into`
(`commands/file_system/volume_copy.rs:96` for copy, `:181` for move) and `compress_files` (`:217`) goes to
`ops_compress_start`, and neither call passes it: `compress_start` has no such parameter
(`archive_edit/compress.rs:151-161`). Meanwhile `dispatchCompress` fills `previewId` in (`:740`) and ⌥F5 runs a real
sampling preview. So M1 threads `preview_id` through both routes, and the archive-edit `spawn_managed` sites are **in**
scope.

Thread it so the operation **awaits** the preview, not so it reuses the result. These paths cannot reuse it:
`copy_into.rs` plans its changeset with its own `WalkDir` walk (`:3`, `:16`) and never calls `take_cached_scan_result`.
So a Compress already walks the tree twice today, serialized by the frontend's scan-wait. Awaiting preserves that
serialization. Without it, M1's deletion would make the two walks concurrent, which is the part that actually costs on
MTP and SMB. The duplicate walk itself is pre-existing and stays: see "Adjacent bugs this does not fix".

Two lists, and they are different sets. The **preview-reachable `spawn_managed` sites** are what the settle audit under
Risks walks (eight of the ten non-test sites; `rollback.rs:189` and `rename/bulk.rs:197` never see a `previewId` and
stay out of scope): `transfer/volume/copy.rs:306`, `transfer/volume/move.rs:232`, `transfer/volume/move_same.rs:171`,
`write_operations/mod.rs:363` (local copy/move/trash and local delete), `write_operations/mod.rs:650` (volume-aware
delete), plus `archive_edit/copy_into.rs:734`, `move_out.rs:297`, and `driver.rs:223` once threading lands. The **six
preview-consumption sites** are where the wait has to be inserted so nothing walks early: `delete/walker.rs:35` and
`:594`, `transfer/move_op.rs:495`, `transfer/copy/mod.rs:125`, and `transfer/volume/preflight.rs:145` and `:327`.
Auditing one list and calling it the other is how a path gets missed.

**2. The waiting task MUST forward preview progress as its own `write-progress`.** Awaiting a signal emits nothing, and
nothing else will emit for this operation: `scan-preview-progress` is keyed by `previewId` and carries no `operationId`,
and `operations-store.svelte.ts` subscribes to `operations-changed` (`:159`) and `write-progress` (`:162`) only. Skip
this and every scan-phase surface goes blank rather than live:

- `TransferProgressDialog`'s `{#if phase === 'scanning'}` body (`:362`) would render `ScanPhaseBody` with every count
  frozen at its initial zero for the whole scan, where today it shows live counts.
- `QueueRow.svelte:76` gates the readout on `progress !== null && (progress.bytesTotal > 0 || progress.filesTotal > 0)`,
  so the row would draw nothing at all.
- `barFraction` returns 0 for a null progress (`status-corner/operation-chip.ts:47`), so the chip would sit at 0% with
  an empty tooltip.

So M1 owns a `previewId → operationId` bridge, claimed when the operation registers and released when the wait ends,
consulted at the preview's progress emit sites (`scan_preview.rs:208-218` local, `:341-351` volume) so a claimed preview
emits `write-progress { operationId, phase: 'scanning', … }` alongside its existing `scan-preview-progress`. Both events
keep firing: a pre-confirm `TransferDialog` may still be watching the same preview by `previewId`. **Forward the
preview's `expected_files_total` / `expected_bytes_total`** (`scan_preview.rs:215-216`) into the event's
`expectedFilesTotal` / `expectedBytesTotal` (`bindings.ts:9418-9420`), the field pair the index expectation already
rides on.

**A claimed preview has exactly one owner.** A second operation naming the same `previewId` is refused the claim and
falls through to its own walk, never shares the bridge. Not hypothetical: the archive-password path re-dispatches a new
operation for the same sources, and `take_cached_scan_result` REMOVES the entry it reads (`scan_cache.rs:203`), so two
claimants would race for one consumable result and the loser would silently get nothing. Single-owner, stated once,
kills the class.

**Emit one synthetic tick when the row appears, and mind the ordering.** `row.progress` must stop being `null`
immediately rather than at the first `progress_interval_ms` boundary, which on a preview near its end may never arrive.
But the tick cannot simply be the task's first action: `spawn_managed` inserts the record, runs `run_admission_pass()`
(which spawns the deferred task at `manager.rs:431`), and only then calls `emit_changed()` (`:358-359`), while
`applyProgress` early-returns for an id with no snapshot yet (`operations-store.svelte.ts:140-144`). A tick that beats
its own `operations-changed` is discarded and the row stays blank until the next real preview event, which is exactly
the case the tick exists for. **The tick must land after the `operations-changed` that first carries the row**, and for
a `Queued` operation after admission, since its task does not exist until then. Assert the ordering in the test, not
merely the tick's existence.

`filesTotal` and `bytesTotal` stay 0 during a scan (finding the totals is what the scan is for), which is why the
dual-bar readout must NOT be what a scanning row shows. Render the same scan-phase line the dialog does. **This costs no
new strings:** `ScanPhaseBody.svelte` already resolves `fileOperations.scanPhase.*` and
`fileOperations.shared.scanningTooltip` from the catalog, and the queue row reuses them. Nor does the rate: the backend
emits none during scanning (`progress-readout.ts:12-15`), so `ScanThroughput` stays the frontend's job, computed from
the forwarded event exactly as it is computed from a preview event today.

**Do not let `expected_*` populate `filesTotal` / `bytesTotal`.** It is the tempting shortcut, because it turns the bars
on. It is wrong twice: `showReadout` (`QueueRow.svelte:75-77`) would flip on and draw a bar measured against a guess,
and the number would jump when the real totals land. `filesTotal` means "what the scan concluded", and during the scan
there is no such thing. The expectation is a hint, and it renders as one.

**3. Publish a terminal OUTCOME, not a completion pulse, and classify it from the flag.** A signal hung on
`ScanPreviewState` (`scan_cache.rs:26-29`) cannot work, because both workers remove their own `SCAN_PREVIEW_STATE` entry
(`scan_preview.rs:246-249` local, `:387-389` volume) **before** `insert_scan_result` (`:266-277`) and before the
terminal event. With `LANE_BUDGET = 1` a queued operation's task may not spawn for minutes, so "look up the preview and
find nothing" is the common case, not the rare one, and "nothing" is ambiguous four ways: complete-and-consumed,
errored, cancelled, and never-existed. This is exactly what `checkScanPreviewStatus`'s race resolution (`:1152-1160`)
handles today by reading the results cache rather than the state map, and M1 deletes that frontend path, so the backend
has to carry the property.

Spec it as a terminal outcome (complete with its `CachedScanResult`, error with its message, or cancelled), published
atomically with the in-flight state's removal and readable after the fact, with the same TTL eviction
`SCAN_PREVIEW_RESULTS` already applies (`scan_cache.rs:138`). Collapsing `SCAN_PREVIEW_STATE` and `SCAN_PREVIEW_RESULTS`
into one map whose value is either in-flight or settled is the shape that makes the atomicity free; take it unless
something argues otherwise.

**The `cancelled` variant comes from `ScanPreviewState::cancelled` at the worker's exit, never from which event fired.**
The two `Cancelled` event arms (`scan_preview.rs:255`, `:394`) are near-unreachable: they need the walk to have finished
normally with the flag set afterwards, whereas a genuinely cancelled walk returns `Err((ctx.on_cancelled)())`
(`scan.rs:164-165`, `:290-291`) and lands in the **error** arm (`:291-293`), and the volume path stringifies
`VolumeError::Cancelled` into `"Scan failed: {e}"` (`:371`, emitted at `:419-421`). Read the event instead of the flag
and a user's cancel reaches the operation as `write-error` "Scan failed: Cancelled" rather than `write-cancelled`, and
recovering it from the message would break `no-string-matching` on top. **Reconciling the two workers' error and cancel
arms is part of M1.** The frontend never depended on the cancelled event either: `handleCancel`'s scan branch tears the
listeners down before it could arrive, which is why nobody has seen this.

**Define the miss case:** a `previewId` naming nothing at all (evicted, or a stale id from a reloaded window) falls back
to the operation's own walk, which is today's foolproof re-scan, never a hang. Same discipline as M2's `list_operations`
miss case, and for the same reason.

**A claimed preview is exempt from TTL eviction.** `SCAN_RESULT_TTL` is 300 s and eviction runs on the next
`insert_scan_result` (`scan_cache.rs:138`, `:179-186`). With `LANE_BUDGET = 1` a queued operation can sit well past five
minutes, so the ordinary busy-lane case would evict the very result its owner is waiting for and silently downgrade to a
re-walk. Exempt entries with a live claim, and let the miss case cover only genuinely unowned ids.

**Why the id is minted at confirm, not at dialog open.** David's model says the operation begins when the TransferDialog
appears. This milestone deliberately starts it one step later, and the reason is what a queue row promises. A row says
"something is happening on your behalf, and here is how to control it". Before confirm there is no destination, so the
row cannot say what it is doing; Pause is meaningless; Cancel means "close the dialog you are looking at"; and
`blocks_quit` would start prompting on ⌘Q because a picker is counting files. Confirm is the exact moment intent becomes
a process, and it is also the exact moment the current code loses the thread. Serving the model's _purpose_ (the dialogs
are looking glasses, not the process) does not require minting identity for something the user has not committed to. The
pre-confirm scan stays where it is, in `TransferDialog` and `transfer-scan-state.svelte.ts`. This is a decision, not a
deferral: see "What we decided not to do" below.

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
  `operations_are_idle` (`mcp/executor/async_tools.rs:272-276`) matches only `Running | Queued`, so
  `await operations_idle` would return immediately while a scan ran; `hasRunning` / `hasPaused`
  (`operations-store.svelte.ts:199`, `:203`), `pickChipOperation`, and `queue-backlog.ts:28-35` would all quietly
  disagree about whether anything is happening. Reusing `Running` makes every one of those correct with no edit.
- The visible consequences follow for free: the corner chip picks the operation up, `hasOtherQueuedWork` counts it, and
  `await operations_idle` waits for it.

**The descriptor.** `OperationDescriptor` (`manager.rs:105-118`) is filled at registration with the values the WRITE
will need, not the scan's:

- `lanes`: the same source and destination lanes the write takes (`transfer/volume/copy.rs:206`, `local_lanes` at
  `write_operations/mod.rs:378-383`), reserved from confirm. This is the milestone's one genuinely contested decision,
  so both directions are on the record. **For:** with `LANE_BUDGET = 1` (`manager.rs:254`), a transfer confirmed while
  another runs on the same lane is admitted as `Queued` and the existing auto-queue path backgrounds it, needing no new
  code; and admission is oldest-first (`manager.rs:379`), so operations run in the order the user confirmed them. Today
  they do not: A dispatches only after its scan, so B, confirmed second with a finished scan, takes the lane first.
  **Against:** an operation scanning for three minutes holds a lane it is not writing to, so a destination device can
  sit idle where today it would be receiving B's bytes. That is a real utilization cost under "respect the user's
  resources", and it is bounded by the scan's duration rather than the transfer's. **Decision: reserve from confirm.**
  Matching confirm order is a correctness property; better lane utilization is an optimization. The alternative is
  two-phase reservation (scan without lanes, request them at write start), which costs the invariant that `Running`
  means "admitted and holding its lanes" (`manager.rs:90`) and adds a second admission point that can leak a lane. Reach
  for it only if the idle-device cost shows up in practice, and pay that invariant knowingly.
- `volume_ids`: as the write needs them. **This is a behavior change worth stating:** Eject becomes disabled from
  confirm rather than from first byte. That is the right answer (the operation is committed), but it is new.
- `supports_rollback`: the value the write will have (`matches!(type, Copy | Move)` and the per-site values at
  `mod.rs:276`, `copy.rs:220`, `move.rs:166`, and the rest). It is a promise about the operation, not a statement about
  the current phase. Rollback must be **disabled during scanning** because there is nothing to undo yet, and that is a
  view decision keyed on `phase`, never on `supportsRollback`. Getting this backwards means a row that offers Rollback
  and then can't, or one that never offers it at all.
- `summary`: source and destination, both known at confirm.

**Pause is refused during the scan-wait, in the backend, and gated in three places on the frontend.** `set_paused`
(`manager.rs:585-604`) flips any `Running` record to `Paused`, and a scan-waiting operation is `Running`, so without a
rule the snapshot would say `Paused`, the dialog title would say "Paused", and the scan would walk on at full speed.
That is the same class of lie as "a paused op reports `is_running: true`", which this codebase already documents and
works around. The amplifier that makes it more than cosmetic: `set_paused` deliberately keeps the lane slots
(`manager.rs:576-578`), so a "paused" scan would hold its lane indefinitely while doing nothing.

The refusal is cheap, and deliberately does not grow the IPC surface. `set_paused` declines the flip for a record in its
scan-wait (the flag from the previous section is what lets it tell), and that refusal is **already observable everywhere
it matters**: no surface flips optimistically, the dialog's own comment says so (`:961-964`), and every consumer reads
the lifecycle status. A refused pause therefore shows as "the status stayed `running`, the button still says Pause". No
return-type change, no `bindings.ts` regeneration, no new agent-facing string.

Three frontend gates go with it, and all three are work rather than documentation:

- The dialog's Pause control, on `phase === 'scanning'`.
- `QueueRow`'s Pause, which today renders for any `running` row (`QueueRow.svelte:151`).
- `QueueRow`'s Rollback, which today shows whenever `supportsRollback && (isRunning || isPaused)` (`QueueRow.svelte:63`)
  and would otherwise offer to reverse a copy that has written nothing.

**A refused pause MUST be latched, or "Pause all" silently loses it.** This is the part that would ship as a real
defect, so it gets stated as a requirement rather than an aside. `pause_all` (`manager.rs:937-941`) walks
`running_ids()` calling `pause_operation`, and `pause_operation` (`manager.rs:914-921`) sets the driver's park gate
**only if `set_paused` returned true**. A bare refusal therefore drops the request on the floor with nothing holding it:
minutes later the scan-wait ends, the flag clears, and that one operation starts writing at full speed while every other
operation is paused and the user believes the device is free. That is precisely the scenario pause exists for, which
makes losing it worse than never offering it.

So the branch that refuses the flip **records the request on the record**, and the point that clears the in-scan-wait
flag applies it. The wiring is already there: `set_paused` returns `bool` (`manager.rs:585`) and `pause_operation`
already gates on it, so the write-side park gate follows for free once the deferred flip happens. The refusal is one
match arm plus one field plus one apply point.

M1 does **not** surface the pending pause, and that is a deliberate limit rather than an oversight. The row keeps saying
"Running" while the operation scans, then flips to "Paused" on its own when the write would have begun. Showing "pause
pending" would mean a new snapshot field and a new string, and the harm being fixed here is the silent full-speed write,
not the surprise. Revisit if the delayed flip confuses anyone.

MCP is the one caller that will misreport the refusal, and that is a pre-existing bug this milestone does not take on:
see "Adjacent bugs this does not fix".

The alternative, real parking semantics, would mean a `paused` flag on the preview alongside its `cancelled` one.
Rejected, and the volume path is what settles it rather than taste: a local walk could poll a pause flag per entry the
way it polls `cancelled`, but a volume scan sits inside `scan_for_copy_batch_with_progress` (`scan_preview.rs:627`) for
a whole batch, so there is no park point, and on MTP the batch can be the entire scan. Pausing would therefore work on
the volume kind that needs it least. Add the ordinary reasons (pause exists so a user can free a busy device or CPU
mid-write, a read-only walk is short next to the write it precedes, the useful controls during a scan are Background and
Cancel, and parking would still hold the lane) and the deferred pause above is both cheaper and more honest.

**`OperationSnapshot` needs no new fields, but `OpRecord` needs one.** The snapshot carries `operationId`,
`operationType`, `status`, `source`, `destination`, `supportsRollback`, and `error` (`manager.rs:153-166`,
`lib/ipc/bindings.ts:6834-6851`), and byte counts were never on it: they ride `write-progress`. "No bytes yet" is
already representable, because `OperationRow.progress` is `WriteProgressEvent | null` and is null until the first tick
(`operations-store.svelte.ts:37-45`). A scanning row is an ordinary running row whose progress happens to say
`phase: 'scanning'`.

The **record** is a different question, and the wire-level answer reads as if it settles both. `OpRecord`
(`manager.rs:136-146`) holds `descriptor`, `status`, `deferred`, and `reserved_lanes` and has no notion of phase, so
`set_paused` (`manager.rs:588-598`) cannot tell a scanning operation from a writing one. M1 adds a manager-visible
in-scan-wait flag on the record, set at registration and cleared when the wait ends. It stays off the snapshot: the
frontend already learns the same fact from `write-progress`'s `phase`, and putting it on the snapshot would give two
sources for one truth.

**Cancel maps straight through, and the settle contract is untouched.** `cancel_operation(id)` during the scan-wait sets
the operation's cancellation token; the wait aborts; the task's cleanup calls `cancel_scan_preview(previewId)`
(`scan_preview.rs:111-118`) so an abandoned walk stops instead of finishing for nobody, then emits `write-cancelled` and
`write-settled` and calls `on_settled` (`manager.rs:450-454`). The frontend's cancel path needs no change at all: it
already issues `cancelWriteOperation` and waits for both terminal events. The special-case scan branch in `handleCancel`
(`:882-890`) is **deleted**, not adapted, and that deletion is the proof the mapping worked.

**The quit gate needs no edit.** A scanning transfer is `status: Running` with `operation_type: Copy` (or Move / Delete
/ Trash / ArchiveEdit), so `blocks_quit` (`quit/mod.rs:108-126`) already returns true. ⌘Q during a scan starts
prompting, which today it does not.

Don't lean on "and it cancels instantly" as the reassurance, because it is only measured for local walks. A local scan
polls its cancellation flag per entry and stops promptly. A volume scan drives `Volume::scan_for_copy`
(`scan_preview.rs:511`) and `scan_for_copy_batch_with_progress` (`:627`) over MTP or SMB, and can sit inside one remote
call for as long as that call takes, exactly like the chunk awaits M0's hard-abort tier was built for. **Verify at
implementation time whether the volume scan path is covered by that tier**, and if it is not, decide whether M1 extends
it or whether the quit gate's cooperative-then-abort budget (`DRAIN` 1.5 s of a documented 2 s whole
decision-to-process-gone budget, `quit/mod.rs:38-39`) absorbs it. Measure before claiming.

**Two more behavior changes worth naming before someone hits them.**

- **Auto-queue fires at confirm, so the dialog can flash.** An operation confirmed onto a busy lane is `Queued` from
  registration, so the dialog's one-shot `listOperations()` seed (`:837`) sees `queued` immediately and
  `handleAutoQueued` runs, mounting and unmounting the modal within a frame or two, with a toast and a queue window.
  `MIN_DISPLAY_MS` does not cover the queue route (`maybeFinishCancelClose` and `handleComplete` apply it;
  `handleAutoQueued` does not). **Accept it in M1 and note it.** The tempting fix, not mounting at all when the dispatch
  response already says `queued`, is not available: the dispatch IS the dialog (`startOperation` calls
  `dispatchOperation` from inside `createTransferProgressState`, driven by the component's `onMount`, `:747-846` and
  `:1171-1177`), so there is no response to consult before mounting. Getting one means moving dispatch, the
  destroyed-during-dispatch rule, and the foreground claim out of the dialog, which is M5's birth/view split pulled
  forward into M1, and it would also drop the MCP round-trip: `emit('mcp-response', …)` fires only at `:797` inside
  `startOperation`, with the failure reply at `:856`, and `TransferDialog`'s own emit carries no `operationId`, so it is
  no substitute. The behavior is not new either, only more frequent: `handleAutoQueued` (`:1011-1026`) already does
  exactly this when the manager admits an operation as queued. M5 is where it gets fixed properly.
- **A scanning operation counts as backlog.** `hasOtherQueuedWork` (`queue-backlog.ts:28-35`) excludes only terminal and
  instant operations, so a second transfer's button reads "Queue" instead of "Background" while the first is merely
  scanning. That is arguably correct (there is other work) and needs no code change, but it is a visible wording flip
  with no obvious cause, so it belongs in the milestone rather than in a bug report.

**The queue row keeps saying "Running" during a scan, and that is deliberate.** `queue.row.status`
(`intl/messages/en/queue.json:35`) is a `select` over the lifecycle status, and the lifecycle status genuinely is
`running`. The status column names the lifecycle (Waiting / Running / Paused / Couldn't finish); the readout names the
activity, and the scan-phase line says "Counting…" in the user's language already. Adding a "Scanning" arm would mix two
axes into one column and would need a `phase` input the row does not take. Zero new strings in M1, which is also what
keeps this milestone off the nine-locale critical path. Revisit if the row reads as stuck to real users.

**The chip gets an explicit indeterminate scan state, and this is M1's problem to solve.** The reasoning that keeps the
dual bar off a scanning row applies to the corner chip too, and it is easy to miss because the bridge looks like it
covers the chip. It does not: `barFraction` is `bytesTotal > 0 ? … : filesTotal > 0 ? … : 0`
(`status-corner/operation-chip.ts:46-57`), and both totals stay 0 through the scan, so the bridge changes the tooltip
and leaves the bar at zero. The chip appears at all only because M1 created the record, so M1 would be introducing a new
ambient surface reading "Copying · 0%" for minutes where today nothing appears. A percentage that cannot move is not
honest progress. Give the chip a scan-phase state (indeterminate, with the counting tooltip), keyed on the same `phase`
the row and the dialog read.

**A `Queued` row renders the scan-phase line too.** `showReadout` requires `isRunning || isPaused`
(`QueueRow.svelte:75-77`), so without this an operation admitted behind another on the same lane would render "Waiting"
with nothing underneath for its whole scan, which on a busy lane is the common case rather than the edge. Clear that
gate for the scanning case: "Waiting" plus a moving file count is exactly what is happening, the counts are real, and a
bare row reads as a hung queue. Decided, and it is why the tally carries the row's scan-phase branch as work.

**Frontend deletions.** The scan-wait machinery goes, and what replaces it lives in the backend:

- `waitForScanThenStart` (`:1073-1166`) whole, including `isOurScanEvent` (`:1053-1056`), `cleanupScanListeners`
  (`:1045-1051`), the four `onScanPreview*` subscriptions, and the `checkScanPreviewStatus` race resolution
  (`:1152-1160`).
- `config.scanInProgress` (`:120`) and the branch in `start()` (`:1171-1177`), which collapses to
  `void startOperation()`.
- `waitingForScan` (`:191`), its getter (`:1212-1214`), and its two reads in `canPauseOrQueue` (`:311`) and `destroy()`
  (`:1190`).
- The scan branch in `handleCancel` (`:882-890`) and the preview cancel in `destroy()` (`:1189-1192`).
- **One of the two duplicate scan bodies in `TransferProgressDialog.svelte`.** It renders `ScanPhaseBody` twice, once
  under `{#if waitingForScan}` (`:302`) and once under `{#if phase === 'scanning'}` (`:362`), because the same UI had to
  be fed from two different state sources. One source means one body. This duplication is a real defect in its own
  right, and collapsing it is how you know the milestone actually unified the two paths.

**What must NOT be deleted, and why the distinction is easy to get wrong.** The six scan count and rate fields
(`scanFilesFound` / `scanDirsFound` / `scanBytesFound` / `scanCurrentDir` `:192-195`, `scanFilesPerSec` /
`scanBytesPerSec` `:198-199`), their getters (`:1215-1232`), and the `ScanThroughput` instance (`:197`) all **stay**.
They are not the scan-wait path's state; they are the surviving `handleProgress` scanning branch's state (`:381-393`),
which is what writes them and what calls `scanThroughput.push`. Deleting them would break the very body this milestone
keeps. `scanUnlisteners` (`:196`) goes with `waitForScanThenStart`.

That is roughly 120 lines out of `transfer-progress-state.svelte.ts` and one duplicated block in the component. It is
worth counting because it bears on the ordering, not because M1 is a net deletion. **It is not.** Tallied honestly, M1
adds: the scan-wait inside the operation task, the progress bridge with its single-owner and ordering rules, the
terminal-outcome contract plus reconciling the workers' cancel arms, a record-level in-scan-wait flag with the
`set_paused` refusal and its latched deferred pause, `preview_id` threaded through two archive entry points, a cleanup
hook on `cancel_if_queued`, a TTL exemption, the chip's indeterminate state, phase gates on three controls, the queue
row's scan-phase rendering branch (including rendering it for `Queued` rows, against `showReadout`'s
`isRunning || isPaused` gate at `QueueRow.svelte:75-77`), the E2E preview-delay affordance in `src-tauri/src/lib.rs`
without which the regression test does not get written, and the delete path's missing `previewId` guard. An earlier
draft sold M1 as "removes code rather than adding it"; that stopped being true as the milestone was pinned down, and the
claim is retired rather than defended. The ordering rests on the other two reasons, which never depended on it.

**Why it lands first.** Three reasons, in order of weight:

1. **It is a bug fix, not a refactor.** A user cannot background a scanning transfer today, and the scan dies with the
   dialog. Everything else in this spec is invisible until M6. Shipping a fix behind four milestones of restructuring is
   the wrong order.
2. **It is a hard ordering constraint on M5, not merely a saving.** M5 restructures `transfer-progress-state.svelte.ts`
   around the birth/view split, and M1 deletes a chunk of that module along with one of the three members of the "Birth"
   concern. Run them the other way and M5 carefully re-expresses a scan-wait path that is about to disappear, then
   deletes its own work. The ~120 lines are the visible part; the constraint is the point.
3. **It nearly closes the `!operationId` window,** from "the whole scan" to "one IPC round trip". The
   destroyed-during-dispatch rule and the guard retirements above are much easier to argue against a millisecond gap
   than a multi-minute one.

The cost of going first is one double-touch: the queue row learns to render a scanning operation against today's store,
and M3 then re-points it at a session. It is a genuine double-touch (the row gains a phase-aware branch reading
`progress.phase`, which only becomes non-null once M1's bridge exists), and it is still the right trade against shipping
a user-visible fix behind four milestones of restructuring.

**Land the two halves in one commit.** The frontend deletions alone make the named red Vitest go green, at which point
the app dispatches early and the backend silently re-walks the whole tree, which is a performance regression that no
test would catch. Write the Rust cache-consumption test first, and do not split M1 across commits by layer.

- **Tests, TDD (red first).** This is a bug fix in a data-writing path, so it earns real red. Rust first, per the
  landing note above:
  - Rust unit: a write command dispatched with an in-flight `previewId` registers its operation immediately and reports
    `Running` on `list_operations()` before the preview completes.
  - Rust unit: the same operation, once the preview lands, consumes the cached result rather than re-walking (assert on
    the scan-cache take, not on timing). **Write this one first**; it is the only guard against the frontend half
    shipping alone.
  - Rust unit, the outcome contract, one case per terminal shape: complete, error, cancelled, and a `previewId` naming
    nothing (which must fall back to its own walk, not hang). The error case is unimplementable against a bare
    completion pulse, which is how you know piece 3 landed.
  - Rust unit: the progress bridge emits `write-progress { phase: 'scanning' }` under the operation's id while the
    preview runs, and stops emitting once the wait ends.
  - Rust unit: a Compress (and a copy into a zip) confirmed mid-scan awaits its preview rather than walking concurrently
    with it. The copy-path tests pass without the archive threading, so this is the one that catches that regression,
    and it is worth writing before the threading rather than after.
  - Rust unit, the ORDERING of the synthetic tick: it lands after the `operations-changed` that first carries the row,
    and for a `Queued` operation after admission. Asserting the tick exists is not enough; a tick the store discards is
    indistinguishable from no tick.
  - Rust unit: a cancelled preview reaches its operation as `write-cancelled`, not as a `write-error` reading "Scan
    failed: Cancelled". This fails against any implementation that reads the event instead of the flag.
  - Rust unit: a second operation naming a claimed `previewId` is refused the claim and falls back to its own walk.
  - Rust unit: a claimed preview's result survives a TTL sweep triggered by a later `insert_scan_result`.
  - Rust unit: `set_paused` declines a record in its scan-wait and the record stays `Running`.
  - Rust unit, and this is the one that matters: **`pause_all` during a scan-wait latches, and the operation is paused
    before it writes a byte.** Pause all with one scanning and one writing operation, end the scan, and assert the
    scanning one lands `Paused` with its driver gate set rather than running. Without this test the refusal looks
    correct and silently loses the pause.
  - Vitest: the dialog's Pause, `QueueRow`'s Pause, and `QueueRow`'s Rollback are all absent for a scanning operation.
    Three gates, three assertions; one of them silently missing is the shape this milestone invites.
  - Vitest: the corner chip renders its indeterminate scan state rather than 0% for a scanning operation.
  - Rust unit: `blocks_quit` is true for a scanning operation (`quit/tests.rs` has the pattern at `:154-195`).
  - Vitest: the progress dialog exposes a non-null `operationId` and `canPauseOrQueue` while `phase === 'scanning'`, and
    Queue backgrounds it. This is the user-visible bug; write it first and watch it fail.
  - Vitest: a queue row bound to a scanning operation renders live counts, not a blank row, and no dual bar, for a
    `queued` row as well as a `running` one. Worth a test precisely because the naive implementation passes every other
    test.
  - Vitest, red first: confirming `DeleteDialog` before `startScanPreview` resolves dispatches with a non-null
    `previewId`. Drive the IPC with an explicit deferred rather than incidental microtask order, the way
    `TransferDialog.test.ts` does, and watch it fail first: today it dispatches `null` and the failure is real.
  - Vitest, written after: the deletions keep the existing `transfer-progress-state.svelte.test.ts` scan cases passing
    where they describe outcomes, and the ones that describe the scan-wait _mechanism_ are replaced by cases against the
    new path, each with a written reason.
  - E2E: confirm a large copy, background it from the queue button while the scan-phase readout is still up, and see the
    row in the queue window. This is the regression that matters and it cannot be proven at the unit level. **It needs a
    harness affordance and will not otherwise get written:** E2E fixture trees are deliberately small and
    `data-scan-state` signals "counting done", which is the opposite of what this test has to hold. Add an
    E2E-mode-gated preview delay (the `CMDR_E2E_MODE`-gated env-var pattern in `src-tauri/src/lib.rs`), so the scan
    window is deterministic rather than a race against a 40-file fixture.
- **Docs:** `write_operations/CLAUDE.md` and `DETAILS.md` (the scan preview gains an operation record; state the
  identity rule: one `operationId` from confirm, `previewId` still names the preview; and the terminal-outcome
  contract), `scan_preview.rs`'s module doc (the outcome publication and the progress bridge), `transfer/CLAUDE.md` (the
  scan-wait must-know is now wrong), **`transfer/DETAILS.md`** (two `waitForScanThenStart` references at `:310` and
  `:452` name a function M1 deletes, and `:345` justifies `await scan.scanStarted` because "the progress dialog's
  scan-wait path depends on it being non-null", which is the sentence M1 invalidates and replaces with the reasoning
  above), `queue/CLAUDE.md` and `DETAILS.md` (a running row may be in `phase: 'scanning'`; Pause and Rollback are
  phase-gated), `quit/`'s docs if they enumerate what holds a quit, and a line in `docs/architecture.md`.
- **Checks:** `pnpm check rust` and `pnpm check svelte` while iterating, full `pnpm check` before wrapping, plus the
  transfer E2E specs.

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
- **Seeding will immediately hit a scanning operation.** After M1, `list_operations()` can hand back a `Running` record
  whose progress is `null` and whose first tick says `phase: 'scanning'` with zero totals. A session seeded that way
  must present as live-and-scanning, never as stuck at 0%. Cover it in the seeding tests rather than discovering it when
  a reload lands mid-scan.
- **The buffer needs a stated bound.** `pendingEvents` is bounded today by one IPC round trip. A central buffer keyed by
  unclaimed ids is not, and `write-progress` fires per interval for every operation in the process, plenty of which
  never get a session in a given window: MCP-started operations, ones only ever watched from the queue, ones that settle
  before anything registers. So: keep only the **latest** `write-progress` per unclaimed id, which bounds the buffer to
  the number of unclaimed ids rather than the number of events; keep at most one of each terminal event per id, because
  a session registering after its operation ended must resolve rather than hang; drop an id's buffer once it has settled
  and been claimed, and age the rest out on the backend's own precedent (`SCAN_RESULT_TTL` is 300 s, evicted on the next
  insert, `scan_cache.rs:138` and `:181`).

  **Latest-only is safe because of ORDERING, not idempotence.** The tempting justification is that progress is
  idempotent, which is true of the store's latest-value map and false of a session: from M3 onward a session owns the
  stateful EMA smoother, so dropping intermediate samples is fine but feeding an older sample after a newer one corrupts
  the very "one operation, one truth" property the registry exists for. That is what makes rule (c) below non-optional
  rather than tidy.

- **Three rules make the buffer implementable, and each one closes a race the codebase has already hit.** Tauri event
  callbacks run synchronously on the webview's single JS thread, so an event's delivery is atomic and "unclaimed at that
  instant" is well defined, but only if registration is one synchronous block. Two things here are async and would break
  that.

  - **(a) The fan-out subscribes at window init, before any session can exist.** Its own `listen()` is async, so
    subscribing lazily on the first session means events arriving before that promise resolves are not buffered at all.
    That is exactly M1's dispatch → dialog → session sequence on a cold main window.
  - **(b) Claim, flush, and go live are one synchronous block, with no `await` between them.** M2's seeding deliverable
    is async, so a session that claims its id and then awaits `list_operations()` will overwrite live events with an
    older seed. `createOperationsStore.init` already guards this exact shape twice, and both guards are the precedent to
    copy: subscribe before seeding (`operations-store.svelte.ts:157-158`), and apply the seed only if nothing fresher
    arrived (`:174-176`). The miss case above covers "the seed found nothing"; this covers "the seed found something
    stale".
  - **(c) The flush precedes any live delivery for that id.** See the ordering argument above.

  Worth one line for whoever later tries to unify the two: **the same event has two fates in one window.** The store
  drops `write-progress` for an id it has no snapshot for (`operations-store.svelte.ts:140-144`), while the fan-out
  buffers it. Both are correct, because the store's authority is snapshot membership and the fan-out's job is to hold
  what a session has not yet claimed.

- **Test seam:** `_testEmit(event)` on the demultiplexer, following `operations-store.svelte.ts`'s `_testApplySnapshot`
  / `_testApplyProgress` (`:213-214`).
- **Tests, TDD (red first):** registry identity (same id gives the same instance, release drops it), the fan-out routing
  an event to the right session, buffering an event for an unregistered id and flushing it on registration, seeding from
  `list_operations` including the miss case, and one smoother per operation. Bug-fix-shaped invariants, so they earn
  real red to green. Then pin the bound and the ordering rules, because "an event was buffered and flushed" proves none
  of them:
  - N `write-progress` events for one unclaimed id collapse to one, and it is the newest.
  - At most one of each terminal event survives per id.
  - A settled-and-claimed id's buffer is dropped, and an unclaimed one ages out on the TTL.
  - An event delivered between a session's claim and its seed resolving wins over the seed (rule (b)), and the flush
    lands before any live event for that id (rule (c)). Both are ordering assertions, so drive them with explicit
    interleavings rather than incidental microtask order, the way `TransferDialog.test.ts`'s `deferred<T>()` helper
    does.
- **Docs:** colocated `CLAUDE.md` + `DETAILS.md` for the new module, a module-map entry in `file-operations/CLAUDE.md`,
  and a line in `docs/architecture.md`. The `DETAILS.md` carries the registry rationale and the divergent-smoother
  argument.
- **Checks:** `pnpm check svelte`.

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
  suites are the contract. They must pass unchanged, which is the point. Add one new test, and make it the right one: "a
  chip and a row report the identical ETA" cannot fail, because both already render `row.etaSecondsDisplay` from the one
  store smoother. The property this milestone actually risks is stacking a second smoothing layer, so pin the count
  where M3 can satisfy it: in the **queue window**, exactly one `createEtaSmoother` is constructed per operation (spy on
  the factory), and it lives on the session rather than the store. Scoped deliberately: the progress dialog still builds
  its own smoother (`:328`) until M5 re-expresses the module, so a main-window assertion would be red at M3 and stay red
  through M4. M5 inherits the main-window half, and its milestone says so.
- **Docs:** `src/lib/status-corner/DETAILS.md` and `src/lib/file-operations/queue/DETAILS.md` updated to point at the
  session as the source. (They are not siblings; the chip lives outside `file-operations/`.)
- **Checks:** `pnpm check svelte`, plus the queue E2E.

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
  That last one is the "commands don't care where they come from" property, which is also the MCP story. **The
  two-window race does not exist yet, and saying so changes what M4 is doing.** `onWriteConflict` has exactly two
  subscribers, both in the main window (`operation-conflict.svelte.ts:349` and `transfer-progress-state.svelte.ts:766`);
  the queue page subscribes nowhere. So today's hazard is two hosts in ONE window, already arbitrated by
  `operation-conflict-rules.ts`, and a genuine cross-window race is something **M6's adoption would introduce**. M4 is
  therefore settling the rule ahead of the hazard, which is the right order (the alternative is discovering it inside
  M6's hardest milestone), but this spec should not claim it fixes something live. Run the conflict E2E specs
  (`conflict-copy.spec.ts`, `conflict-move.spec.ts`, `conflict-dialog-matrix.spec.ts`, `conflict-edge-cases.spec.ts`,
  `conflict-overwrite-conditional.spec.ts`, and `mtp-conflicts.spec.ts` where MTP is available) as regression cover for
  the single-window arbitration M4 moves, not as evidence about two windows, which they cannot give.
- **Docs:** this milestone invalidates a `transfer/CLAUDE.md` must-know verbatim (`:56-59`: Queue and F2 are
  frontend-only, set `backgrounded`, and that flag makes `onDestroy` skip its safety-net cancel). Rewrite it rather than
  patch it, and find it by its opening words rather than by line: it has already moved once. `queue/CLAUDE.md`'s command
  story moves too, since per-row pause/resume/cancel becomes a session call.
- **Checks:** `pnpm check svelte`, plus the conflict E2E specs above.

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
  from a regression. Also inherit M3's smoother-count assertion for the MAIN window: once the dialog is a view, exactly
  one `createEtaSmoother` exists per operation per window there too. M3 could only prove it for the queue window,
  because the dialog still built its own until this milestone.
- **Docs:** the module doc rewritten around the split; the `backgrounded` explanation survives wherever the flag does.
- **Checks:** full `pnpm check`, plus the transfer E2E specs run in isolation.

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
(`:471-475`), uses `fileCount` / `folderCount` to compose the completion toast ("Moved 1 file and 3 folders",
`:482-488`), then calls `refreshPanesAfterTransfer()`, `clearOperationSnapshot()`, and `clearSourcePaneSelection()`
(`:498-500`) against a pane chosen by `sourcePaneSide` (`:207-209`). Foreground an operation started twenty minutes ago,
in a pane that has since navigated somewhere else, and completion mutates the wrong pane's selection while raising a
toast that cannot name what moved.

So the split is not two-way but three-way, and M6 is where that gets settled: **what the operation did** belongs to the
session, and **what this pane should do about it** belongs to the view and is bound to birth. A view that adopted an
operation has no birth context and must degrade honestly rather than guess: no pane refresh, no selection mutation, no
snapshot purge, and a completion toast that says only what the snapshot knows. A view that started the operation keeps
doing exactly what it does today.

**The pane work really is view-scoped; the purge is not, and it is already broken.** An audit of
`dialog-state.svelte.ts` says the split holds for everything except one member. `handleTransferCancelled` (`:525-536`)
does `refreshPanesAfterTransfer()` plus `adjustSelectionAfterCancel(op)` (`:220-232`), both genuinely pane-scoped, both
degrading exactly the way completion's pane work does. No new confusion there.

The search-snapshot purge (`:471-475`) is the exception, twice over.

- **It is operation-scoped truth wearing view-scoped clothing.** It walks `sourcePaths` and removes each from every
  stored search-results snapshot, so skipping it for an adopted view does not merely leave a pane unrefreshed; it leaves
  **rows for files that no longer exist**, in snapshots the user can still open, in any window, long after the operation
  ended. What was deleted or moved is a fact about the operation, not about which pane started it.
- **It already misses two paths and over-fires on a third**, none of which M6 introduces. `removeEntryFromAllSnapshots`
  has exactly one caller, `handleTransferComplete`, so a cancelled or errored move or delete purges nothing and leaves
  those same phantom rows today. And `sourcePaths` is _intent_, not outcome: completion purges every source regardless
  of `filesSkipped`, so a `skip`-resolved move purges rows for files that are still on disk.

So re-homing the read is necessary and not sufficient. M6 either says the input has to change too (the purge keys on
what the operation actually moved, which means the outcome has to carry it), or it scopes explicitly to "make the purge
survive the view, and accept the intent-versus-outcome imprecision that already ships". Either is defensible; leaving it
unsaid means inheriting a shipped bug under the impression it was designed.

**The archive-password re-dispatch is a fourth category the split does not name.** "A view that started the operation
keeps doing exactly what it does today" reads as if starting it implies fresh context, and here it does not:
`handleArchivePasswordSubmit` (`:618-639`) starts a NEW operation from birth context captured before the prompt went up,
and re-runs `snapshotSourcePaneSelection()` (`:638`) against wherever the pane is _now_, which may have navigated while
the user was typing. So the axis is not "adopted versus started" but "fresh context versus stale context", and an
operation can be started-by-this-view and still stale. M4 already schedules `archive_needs_password` for classification;
this is where that classification lands.

Three more things to decide in the milestone:

- **The dialog slot is single-occupancy, and the occupancy test is `transferProgressProps !== null`, never
  `showTransferProgressDialog`.** The two flags (`file-explorer/pane/dialog-state.svelte.ts:176-177`) look
  interchangeable and one path splits them on purpose. When a copy or move out of an encrypted archive needs a password,
  `handleTransferError`'s archive branch sets `showTransferProgressDialog = false` while **keeping
  `transferProgressProps` alive** (`:554-570`, reasoning at `:195-199` and `:549-553`), and the unmounting progress
  dialog releases the foreground slot on its way out. So while the password prompt is up there is no dialog shown, no
  foreground claim, and a live props object that `handleArchivePasswordSubmit` reads at submit time (`:622`) to
  re-dispatch the operation (`:637-639`).

  Test occupancy on the shown flag and Foreground passes, adopts, and overwrites those props. The user then types the
  right password and re-dispatches a copy or move of the **adopted** operation's sources to the **adopted** operation's
  destination. That is a wrong-write against a user's files, produced by a correct-looking guard, and it is the single
  most important line in this milestone. Define what Foreground does when the slot is occupied (refuse, swap, or queue);
  refusing is the honest default and needs a way to say so. And note that "occupied" includes the invisible case, which
  is exactly the one a reviewer will not think of.

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
- **Checks:** full `pnpm check`, and specifically the i18n parity checks plus the screenshot capture run for the new
  string this milestone budgets (a new key is not done until both are green). Then a real-app run: start a copy,
  background it, foreground it from the queue, confirm live progress and that backgrounding again keeps it alive.

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
  first-event, and birth time does not touch it. What kills the buffer is the fan-out's central unknown-id buffering, in
  M2.
- **What is left is model fidelity for the pre-confirm scan alone**, and that is precisely the part M1 argues should
  stay unnamed: no destination, no committed intent, no actionable row, and a quit prompt for a file picker.

If this comes back, it needs a new argument, not this one.

## Adjacent bugs this does not fix

Two pre-existing defects sit directly under M1's feet. Both are named here so nobody rediscovers them from scratch and
neither gets quietly absorbed into a milestone with no business growing.

- **Compress and archive-destination transfers walk the tree twice.** `copy_into.rs` plans its changeset with its own
  `WalkDir` walk (`:3`, `:16`) and never calls `take_cached_scan_result`, so the deep scan preview's result is discarded
  and the walk repeats. M1 threads `preview_id` through these routes only so the operation can **await** the preview,
  restoring the serialization the frontend's scan-wait provides today; it does not make the result consumable. Doing
  that means teaching the archive changeset planner to seed from a `CachedScanResult`, which is real work with its own
  correctness questions and no bearing on operation identity.
- **MCP reports a pause that did not happen.** `pause_operation` returns `()` (`manager.rs:914`,
  `commands/file_system/write_ops.rs:400`), and MCP's `queue` tool answers `OK: Paused operation {id}.` unconditionally
  (`mcp/executor/queue.rs:41-43`), while `is_controllable` (`:132-135`) admits any non-`Failed` status. So a `Queued`
  operation, documented as a pause no-op (`manager.rs:911-913`), already gets a false confirmation today. M1 widens the
  surface by adding a second refusable case, and deliberately does not fix it: an honest answer means `pause_operation`
  returning `bool` or `Result`, the command's return type changing, `bindings.ts` regenerating, and a new agent-facing
  refusal string. That is a self-contained fix for a bug predating this spec, and it should land as its own change
  rather than inside a milestone about operation identity. Cheaper than it sounds, though: the manager-side `set_paused`
  already returns `bool` and `pause_operation` already gates on it (`manager.rs:919`), so the truth is available at the
  IPC boundary and only needs forwarding.

## What this does not change

- The operation's semantics. Pause still parks between files, mid-large-file pause is still unimplemented, and rollback
  availability still comes from the snapshot.
- The IPC surface. M1 changes backend behavior behind an existing `previewId` parameter: no Tauri command signature
  changes, no `bindings.ts` regeneration, no new events, and no new `LifecycleStatus` or `WriteOperationType` variants.
  It does change two internal Rust signatures (`route_archive_copy_into`, `compress_start`) so `preview_id` reaches the
  archive routes. M2 through M6 are a frontend restructuring of state that already crosses the wire.
- The scan preview's own identity. `previewId` still names a preview, previews still broadcast to every webview, and the
  pre-confirm scan in `TransferDialog` is untouched.
- Where copy lives. Every user-facing string stays in the catalog.

## Risks

1. **M1 touches the data-writing path.** Registering an operation earlier means an operation exists that has written
   nothing, and every exit from the scan-wait (complete, error, cancelled, quit) has to reach `on_settled`
   (`manager.rs:450-454`) or the row leaks and its lane stays reserved. The current scan workers are detached
   `std::thread` / `tokio::spawn` with no `JoinHandle` and no RAII guard (`scan_preview.rs:72-87`), which is the
   structural gap to close: the operation's task, not the scan worker, must own the settle. Audit every `spawn_managed`
   site listed in M1, including the three archive-edit ones the threading brings into scope, and note that the six
   preview-consumption sites are a different, larger list.
2. **A cancelled-while-queued operation leaks its cached scan result.** `cancel_if_queued` (`manager.rs:552-572`)
   removes the record without ever running its `DeferredStart`, and that `FnOnce` is where M1's cleanup would live, so
   nothing calls `cancel_scan_preview` / `release_scan_result`. The orphaned `CachedScanResult` holds tens of thousands
   of `FileInfo` entries and only goes away when a later `insert_scan_result` TTL-evicts it (`scan_cache.rs:155-181`) or
   the process exits. A `Queued` op cancelled before admission is the ordinary case on a busy lane, not an exotic one.
   M1 needs an explicit cleanup hook on that path; this is a separate leak from risk 1's settle-and-lane leak, and it
   will not be caught by the same test.
3. **M5 is a large edit to the most stateful frontend module in the app**, and its test suite encodes the current
   ownership model in places. Expect to change tests, and expect each such change to need a written reason: a test
   changed without one is how a real regression gets waved through.
4. **The guard retirements are behavior changes**, not refactors. They are argued above, and they should be argued again
   at implementation time against whatever the code looks like then.
5. **Refcount leaks.** A session that is never released holds listeners for an operation that ended, so the registry
   needs a settle-driven sweep and not just view-driven release. The sweep must be driven by the terminal events, and it
   must not sweep a retained `failed` row: those persist on the snapshot by design until someone dismisses them
   (`manager.rs:619-674`), and a sweep that treats "settled" as "gone" would delete the session behind a failure the
   user has not read yet.
6. **Efforts kept landing in this area while the spec was written** (the queue bar labels plus the Queue/Background
   button, the main-window conflict prompt, the whole of `quit-and-operation-lifetime.md`, and the immediate-confirm
   change). All have landed. Read what actually shipped rather than this spec's description of it, and re-verify these
   line numbers before trusting one.
