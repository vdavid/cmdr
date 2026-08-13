# Operation sessions: details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

## File map

- `operation-event-fanout.ts`: the demultiplexer. Subscribes the seven broadcast streams once per window, seeds the
  registry snapshot, and routes each event to the session that claimed its `operationId`, buffering for ids nobody has
  claimed yet.
- `operation-session.svelte.ts`: `createOperationSession(id, fanout)` — the derived read state, plus the
  `list_operations` seed.
- `operation-session-commands.svelte.ts`: the five commands, their in-flight guards, and their IPC. Composed into every
  session; ❌ never built on its own.
- `operation-session-registry.ts`: `createOperationSessionRegistry()` — refcounted `acquire` / `release`.
- `bind-operation-session.svelte.ts`: `bindOperationSession(() => id)` — how a view binds, and how it lets go without
  having to remember to. What the queue rows, the corner chip, and the progress dialog use.
- `window-operation-sessions.svelte.ts`: this window's instance, plus `initOperationSessions()` /
  `destroyOperationSessions()`, called by `routes/(main)/+page.svelte` and `routes/queue/+page.svelte`.

## The shape

Two things, where a dialog used to be one.

An **operation session** is keyed by `operationId`. It reads the write-event streams and the `operations-changed`
snapshot, holds phase and metrics, and does not know or care whether anything is rendering it. It lives as long as
something is watching, and its subject lives as long as the backend record.

**Views** bind to a session and render it: the progress dialog is one (`../transfer/DETAILS.md` § "The dialog is a
view"), a queue row is one, the corner chip is a minimal one. Zero views is a legal, ordinary state, and it is precisely
what "backgrounded" means.

## Why a registry, not a session per view

Two views of one operation share one session because **smoothers diverge when they start at different times**. The EMA
in `../progress-readout.ts` is deterministic, so two smoothers fed the same samples from the same starting point agree
exactly; the hazard is a view that attaches twenty minutes into a transfer and builds a smoother whose first sample is
the current rate, while the queue's smoother carries twenty minutes of history. The two then disagree on screen for as
long as the new one takes to converge. `ScanThroughput` has the same property over its rolling window.

Late attachment is the whole point of the design (a queue row that hands its operation back to the progress dialog), so
the shared instance is what keeps "one operation, one truth" structural rather than remembered.

The registry refcounts: `acquire` builds on first ask, `release` disposes on last let-go, and a re-acquired id gets a
FRESH session rather than a revived one (a disposed session has detached from the fan-out and would never update again).
A session's estimators therefore restart if every view lets go and one comes back, which is correct: nothing was on
screen in between, so there is no continuity to preserve.

## How a view binds

`bindOperationSession(() => operationId)` is the only way a view should reach a session. It acquires when the view names
an operation, releases when the view unmounts or names another, and hands back `null` for the first frame and for as
long as the window's registry is still being created (a view renders what it can say without one: a missing ETA for a
tick, never a missing row).

It derives the id as a VALUE before acquiring, and that is load-bearing rather than tidy. A caller reads its id off an
object that `operations-changed` rebuilds on every tick, so a binding that re-ran per object would release and
re-acquire mid-transfer, handing the operation a fresh smoother whenever some unrelated operation started or finished.
The divergence would then be self-inflicted, in the one place built to prevent it. `queue-row-session.svelte.test.ts`
pins it by counting `createEtaSmoother` calls across a snapshot rebuild.

The runes in the binder belong to the VIEW's scope, which is why it is a separate module from the session and not a
method on one: a session may hold no `$derived` at all (see below), and the binder is nothing but view-scoped
reactivity.

## What a session owns, and what the operations store keeps

Both read the same `write-progress` stream in the same window, and the line between them is stateless versus stateful.

`queue/operations-store.svelte.ts` reduces MEMBERSHIP (which operations exist, each one's lifecycle status) and the
latest raw tick. All of it is stateless, so a second copy of one event object can't disagree with the first.

A session owns every ESTIMATE built ON that stream: the ETA smoother and `ScanThroughput`. Those are stateful, they
diverge when one starts later than another, and that is what makes them a single-home concern rather than a preference.
It owns the JUDGEMENT about the raw rates too — whether there is an honest number to print at all — for the same reason:
it is one call, and a view that made it for itself would eventually make it differently. So the speed, the ETA, and the
scan rates a view renders are `session.bytesPerSecondDisplay` / `filesPerSecondDisplay` / `etaSecondsDisplay` /
`session.scan`, from the corner chip and the queue row alike.

## Sessions are per-window, and that is fine

The operation queue is a separate `WebviewWindow`, so a session cannot be shared across windows; each webview builds its
own from the same broadcast events. The backend registry stays the single source of truth, and sessions are a per-window
projection of it. It looks like a violation of "one session per operation" and isn't. Nothing here assumes two windows,
so folding the queue into the main window as a popup would simply make its sessions the same sessions.

## Why sessions read a fan-out, not their own listeners

Seven streams carry everything about one operation. Ten sessions must not mean seventy subscriptions, but listener count
is the least of it: the fan-out is a **correctness boundary**. One place buffers events arriving for an id no session
has claimed yet, and one place defines arrival order.

That buffer is load-bearing rather than defensive. The progress dialog dispatches, learns its `operationId`, and binds a
session on the next effect flush; everything the backend emits in between belongs to an id nobody has claimed. The
buffer is what makes that gap harmless, and it is why the dialog needs no event buffer of its own.

It is a new module rather than an extension of `createOperationsStore()`, which subscribes to two of the seven streams,
is a reducer over all operations at once, and has no per-id attach API to extend. The demultiplexer sits underneath
instead, and both can be its consumers.

### Two fates for one event

The store DROPS a `write-progress` for an id it has no snapshot for; the fan-out BUFFERS the same event. Both are
correct, because the store's authority is snapshot membership and the fan-out's job is to hold what a session has not
claimed yet. Whoever later tries to unify the two should start here.

The fan-out never second-guesses the `operations-changed` snapshot about which ids exist, and it never tells a session
its operation is ABSENT from a snapshot: "removed" is what a completed, a cancelled, and a never-existed operation all
look like, so absence carries no information a session could act on.

## The buffer's bound

`write-progress` fires per interval for every operation in the process, plenty of which never get a session in a given
window: MCP-started operations, ones only ever watched from the queue, ones that settle before anything registers. A
central buffer keyed by unclaimed ids is therefore unbounded unless it is given a rule. Four of them:

- Keep only the **newest** event of each kind per unclaimed id, which bounds the buffer by the number of unclaimed ids
  rather than the number of events.
- Keep at most one of each terminal event, because a session registering after its operation ended must resolve rather
  than hang.
- Drop an id's whole buffer the moment it is claimed.
- Age the rest out at `UNCLAIMED_BUFFER_TTL_MS` (300 s), swept on the next insert. Both the number and the trigger come
  from the backend's own precedent for scan results (`SCAN_RESULT_TTL` in `write_operations/scan_cache.rs`), and the
  insert-triggered sweep is what keeps an idle window at zero cost.

### Where a live operation had got to

The buffer answers "what did I miss while nobody was claiming this id". It cannot answer "where is this operation now",
and a view that attaches twenty minutes in needs exactly that: the buffer is dropped on the first claim, and a PAUSED
operation emits nothing to refill it, so a second session would sit at zero for as long as the pause lasts. That is what
a queue row's Show button produces, and it read as a 21%-written copy shown as a scan that wasn't happening.

So the fan-out also retains the newest `write-progress` per LIVE operation, whether or not anything is watching, and
hands it to a session that attaches with no buffered tick of its own. Three rules keep it honest:

- **The buffered tick wins**, because it is newer; feeding an older sample after a newer one corrupts the session's ETA
  smoother, the one thing this module exists to protect.
- **It is forgotten the moment a terminal event lands** for that id. A session claiming an id after the end must
  resolve, ❌ never paint bars over an ending.
- **Same TTL, same insert-triggered sweep** as the buffer, so it stays one tick per live operation and an idle window
  still costs nothing.

What it does NOT survive is a window reload: the retention is per-window state, so a reloaded window knows a paused
operation's status (from `list_operations()`) and not its progress. Nothing in the frontend does — the registry snapshot
is deliberately thin — and the queue row is equally bar-less there. A view must therefore be able to say "I don't know
yet" (`../transfer/DETAILS.md` § "The dialog is a view").

**Latest-only is safe because of ORDERING, not idempotence.** The tempting justification is that progress is idempotent,
which is true of the store's latest-value map and false of a session: a session owns the stateful ETA smoother, so
dropping intermediate samples is fine, while feeding an older sample after a newer one corrupts the very "one operation,
one truth" property the registry exists for. That is why the flush iterates each kind's newest event in ARRIVAL order
(the buffer re-inserts on overwrite so the map's iteration order stays honest), and why rule (c) below is not optional.

## The three rules

Tauri event callbacks run synchronously on the webview's single JS thread, so an event's delivery is atomic and
"unclaimed at that instant" is well defined. That only holds if registration is one synchronous block, and two things
here are async.

- **(a) The fan-out subscribes at window init, before any session can exist.** `listen()` is async, so subscribing
  lazily on the first session would leave events arriving before that promise resolves unbuffered and unheard. That is
  exactly the dispatch → dialog → session sequence on a cold main window. The window's one `list_operations()` seed is
  taken there too, immediately after those subscriptions resolve (§ Seeding).
- **(b) Claim, flush, and go live are one synchronous block, with no `await` between them.** Seeding is async, so a
  session that claims its id and then awaits `list_operations()` would overwrite live events with an older seed. The
  seed applies only if nothing has been delivered since the claim. `createOperationsStore.init` guards this exact shape
  twice (subscribe before seeding, apply the seed only if nothing fresher arrived), and that is the precedent this
  copies.
- **(c) The flush precedes any live delivery for that id.** See the ordering argument above.

## Seeding, and its miss case

Two seeds at two levels, and the window's is the one that usually answers.

**The fan-out takes the window's one `list_operations()` at `init()`**, and holds the result as its snapshot. That is
what a COLD window opens on: no `operations-changed` has been broadcast yet, so the fan-out would otherwise hold nothing
and each row's session would spend a round trip on the answer the next broadcast carries anyway. With one session per
queue row that is N calls for one fact.

Three things keep that seed honest:

- **It is taken AFTER the subscriptions, never beside them.** A seed issued before the listeners are live could be older
  than a broadcast nobody was there to hear, and nothing would correct it. Taken after, the seed can only be newer than
  the moment the window started listening, so anything it misses is something the window is guaranteed to be told.
- **A broadcast that lands while it is in flight wins**, on the same guard `createOperationsStore.init` uses. The guard
  is a FLAG set by the broadcast, not a "is the snapshot still empty" test: an `operations-changed` carrying zero rows
  is a real event (the last operation finishing broadcasts one), and a seed landing after it would repopulate a snapshot
  the backend has already emptied.
- **`initOperationSessions()` awaits it**, and the queue window renders no rows until that resolves, which is what makes
  one call per cold open structural rather than merely likely. The cost is one round trip added to a window open, in
  exchange for one per row removed.

**A session seeds itself only when the attach told it nothing**, which is what recovers an operation that outlived the
window watching it: operations survive a reload, and a reloaded main window that couldn't recover them would show
nothing for a transfer that is very much still running. The test for "heard nothing" is whether the attach delivered
anything, and it gates the CALL, not just the result. It stays the fallback for the three things the window's snapshot
can't cover: a view that acquired while `init()` was still in flight (the registry is published before it resolves, so
the main window's dispatch → dialog → session sequence can beat it), a fan-out seed that failed, and the miss case.

The **miss case** is a real path, not a defensive branch: a terminal operation leaves the snapshot entirely (retained
failures are the one exception), so an operation that finished between the click and the mount seeds nothing. That
resolves to `outcome: { kind: 'gone' }` rather than hanging empty. Settle detection otherwise comes from the terminal
events only, for the same reason absence carries no information.

A seed that throws leaves the session live and event-driven, which is the safe direction: a running operation still
reports itself.

**A seed will land on a scanning operation.** `list_operations()` can hand back a `Running` record whose progress is
null and whose first tick says `phase: 'scanning'` with both totals at 0, because the operation holds its record from
the moment the user confirmed and its task is waiting on the scan preview. A session seeded that way presents as
live-and-counting: `scan` carries the counts, and no ETA is invented from totals that do not exist yet.

## Read surface

Non-redundant on purpose: `progress` is the latest raw tick (phase, counts, `currentFile`, `activity`,
`filesPerSecond`), `readout` is the same tick's numbers branded through `transferReadout`, and the rest is what the tick
alone cannot tell you.

- `snapshot` / `status`: the registry row. The lifecycle status is the bar-is-moving truth, never `is_running`.
- `phase`: `null` before the first tick, then `scanning` and the write phases. Views gate on it (a scanning operation
  has written nothing, so Rollback makes no sense for it).
- `bytesPerSecondDisplay` / `filesPerSecondDisplay` / `etaSecondsDisplay`: the numbers a view is allowed to print. The
  backend's rates, and its ETA through this session's smoother — except while the operation is PAUSED, where all three
  answer `null`. A parked operation emits no further ticks, so its last measured rate and countdown sit there describing
  a transfer that has stopped, and "1905 files/s, 58s left" over a frozen copy is a number nobody can stand behind
  (`AGENTS.md` principle 2: honest progress and ETA). Deciding it here rather than per-surface is what keeps the dialog,
  the queue row, and the corner chip saying the same thing: they showed different answers to the same paused copy once.
  Every view renders these three, never `progress.bytesPerSecond` / `progress.filesPerSecond` / `progress.etaSeconds`.
- `scan`: the counting readout, including the frontend-computed rates the backend does not emit during a scan.
- `conflict`: the conflict the operation is parked on, set from the event and cleared once the backend has ruled on the
  clash that was ANSWERED, whichever surface asked. A newer clash that arrived mid-answer stays (see below).
- `outcome` / `settled` / `settleEventReceived`: how it ended, whether it ended, and whether the backend task has torn
  down. The last is separate because `write-settled` says the task is gone, not how it finished. `outcome` is
  write-once, so a cancel racing a completion cannot flip an answer a view already rendered.

## The command surface

Pause, resume, cancel, rollback, and answering a clash live on the session, beside the state they act on. The property
that buys is not tidiness: **the guards are shared because the session is.** A Cancel pressed on a queue row is visible
to the corner chip watching the same operation, and a command arriving later over MCP will land in the same place. Two
views of one transfer therefore cannot each send a cancel, and a second press mid-round-trip sends nothing.

Every command reports whether the request reached the backend and logs its own failure, so a view issues one with a bare
`void` and no try/catch. `false` (or a `null` verdict) always means the same thing: nothing was sent, so leave what is
on screen exactly as it is. A guard returns it for the same reason a refused IPC does.

**Which IPC each one uses is a decision, not an accident.**

- `cancel()` calls `cancel_operation`, the MANAGER-level cancel, which drops an operation still queued behind a busy
  lane before it ever spawns and otherwise routes into the same keep-partials path. `cancelWriteOperation(id, false)`
  alone cannot do the first half, and a session can perfectly well be watching a `queued` operation.
- `rollback()` keeps `cancelWriteOperation(id, true)`, the write-op intent switch, because it is the only path that can
  ask a running operation to delete its partial destination.

**The guards, and the one asymmetry.** Pause, resume, and `togglePause` share a guard because they are one button.
Cancel and rollback hold theirs until the operation is gone rather than until the IPC returns, so a second click sends
nothing; a refused request lets go, because the operation is still running and the user must be able to ask again.
Rollback is refused once a cancel is on its way (there is nothing left to put back), but cancel is deliberately NOT
refused during a rollback: "stop undoing and keep what's left" is a real thing to want, and this is the only way to ask.

**`togglePause` reads the registry snapshot's lifecycle status.** A paused operation stays in the write-op state map and
answers `is_running: true`, so a toggle keyed on that would try to pause what is already parked. The commands module
takes the status as a predicate for exactly this reason, and `operation-session-commands.svelte.test.ts` pins it by
asserting the status query is never made at all.

### Answering a clash is a delegation

`write-conflict` reaches every webview, so more than one surface can be showing the same prompt. The backend arbitrates:
`resolve_write_conflict(operationId, conflictId, resolution, applyToAll)` returns a typed outcome (resolved / already
resolved / stale answer / no pending conflict / unknown operation) from the slot in `write_operations/conflict_slot.rs`,
which is where that contract is written down. The session hands that verdict back untouched and lets go of the clash it
answered on any of them, because that question is over either way. Only a call that never landed keeps the prompt up.

**The answer names its clash, and only that clash is let go of.** `resolveConflict` takes the `conflictId` off the event
the surface is showing, and clears `conflict` only while the slot still holds that same id. The operation raises its
next clash the instant it takes an answer, so a newer event routinely lands while the IPC promise is still in the air;
clearing whatever sits in the slot on return threw that one away, and the transfer parked forever with no prompt on
screen and no way out but Cancel. The main window's prompt host scopes its queue entry the same way (`dropPrompt`), and
the two together are the frontend half of the backend's identity contract.

Three rules follow, and all three are guardrails rather than observations:

- ❌ **Never rebuild a frontend rule that makes correctness depend on one surface being allowed to answer.** That rule
  existed because the backend parked on a single sender and a second answer vanished silently; the slot removed the
  hazard. Which surface SHOWS a clash is still a UX preference, and it still lives in `../operation-conflict-rules.ts`.
- ❌ **Never refuse to answer a clash this session has not seen.** The fan-out drops an id's buffer the moment it is
  claimed, so a view that adopts an operation whose `write-conflict` went to a session since let go legitimately has no
  `conflict` field. Refusing would leave the user clicking a button that does nothing, and the backend is the authority
  on whether there is anything parked. This is why the `conflictId` is a PARAMETER rather than something the session
  reads off its own state: the surface holding the event always has it, and a session that never saw the event still
  can't be locked out of answering.
- ❌ **Never clear the clash on "an answer came back".** Clear the clash that was answered, by id. The difference is one
  `===`, and it is the difference between a transfer that carries on and one that parks with an empty screen.

### Where an archive password belongs, and why it is NOT a session concern

An encrypted-archive source raises `archive_needs_password`, which `../../file-explorer/pane/dialog-state.svelte.ts`
intercepts upstream of the error dialog. It looks like a second "operation parked waiting for the user", the same shape
as a conflict. It is not, and the distinction decides where the work goes.

**It is a BIRTH concern with a view-scoped prompt.** The password error arrives as `write-error`: the backend has
already settled that operation, so its session has said everything it will ever say about it and there is no command
that could unpark it. What an unlock does is store the password and **dispatch a NEW operation** from the same birth
context, with `previewId` cleared (the previous preview was consumed, and the backend refuses a second claim on one). So
the unlock path is a second entry into dispatch, not a resumption; the only genuinely view-scoped part is the prompt
itself, which is why `showTransferProgressDialog` goes false while `transferProgressProps` stays alive.

Two consequences worth carrying:

- A session must not grow an "unlock" command, and a view must not treat the password prompt as a parked operation. The
  operation the prompt names is over.
- The re-dispatch reads birth context captured **before** the prompt went up, and re-runs
  `snapshotSourcePaneSelection()` against wherever the pane is now, which may have navigated while the user was typing.
  So the axis for a view's completion work is not "adopted versus started" but **fresh context versus stale context**:
  an operation can be started-by-this-view and still carry stale context. That axis is answered in
  `../../file-explorer/pane/DETAILS.md` § "Birth context": the settled-transfer tail asks whether the source pane still
  shows the folder the operation was born in before it touches a selection, which covers this path and the plain
  navigated-away-mid-copy one with one rule.

## What a view that ADOPTED its operation may not do

A view can bind a session for an operation it never started (the queue's Show button). The session is indifferent — that
is the whole point of it — but the VIEW's parent is not, and the line is worth stating here because it is where a reader
of the session will look for it.

The session says what the operation did. It cannot say what a pane should do about it: which pane started it, which
paths it was aimed at, and how many files and folders the user picked are all captured at birth, in the pane, and none
of them are on the wire. An adopted view therefore degrades honestly rather than guessing — no pane refresh, no
selection change, no snapshot purge, and a completion toast that says only what the operation reported. The full
argument, the two-slot arrangement that makes the wrong version unreachable, and the one known gap it leaves live in
`../../file-explorer/pane/DETAILS.md` § "Birth context".

## Testing

`_testEmit(streamEvent)` on the fan-out (and on the registry, which forwards) drives the same demultiplexing path a live
event takes, following `operations-store.svelte.ts`'s `_testApplySnapshot` / `_testApplyProgress`. The two ordering
rules are driven with explicit interleavings (a `deferred<T>()` for the seed, an attach-then-emit pair in one
synchronous block for the flush), never incidental microtask order.

`bind-operation-session.svelte.test.ts` stands in for a view with an `$effect.root`, and asks the REGISTRY whether a
release happened rather than asking the view: a binding that never releases leaves a session listening for an operation
that ended, which nothing on screen would show, so the test acquires again and checks it got a fresh session.
`operation-conflict.svelte.test.ts` asks the same question the same way, because the conflict prompt holds a session by
hand rather than through the binder.

The commands are covered twice on purpose. `operation-session-commands.svelte.test.ts` drives them directly, with no
fan-out at all, which is what makes each guard a one-line test; the session and registry suites then prove the two
things composition adds, namely that the clash is let go of on every verdict and that one view sees what another view
sent.
