# Operation sessions: details

## The shape

Two things, where a dialog used to be one.

An **operation session** is keyed by `operationId`. It reads the write-event streams and the `operations-changed`
snapshot, holds phase and metrics, and does not know or care whether anything is rendering it. It lives as long as
something is watching, and its subject lives as long as the backend record.

**Views** bind to a session and render it: the progress dialog is one, a queue row is one, the corner chip is a minimal
one. Zero views is a legal, ordinary state, and it is precisely what "backgrounded" means.

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
object that `operations-changed` rebuilds on every tick, so a binding that re-ran per object would release and re-acquire
mid-transfer, handing the operation a fresh smoother whenever some unrelated operation started or finished. The
divergence would then be self-inflicted, in the one place built to prevent it. `queue-row-session.svelte.test.ts` pins
it by counting `createEtaSmoother` calls across a snapshot rebuild.

The runes in the binder belong to the VIEW's scope, which is why it is a separate module from the session and not a
method on one: a session may hold no `$derived` at all (see below), and the binder is nothing but view-scoped
reactivity.

## What a session owns, and what the operations store keeps

Both read the same `write-progress` stream in the same window, and the line between them is stateless versus stateful.

`queue/operations-store.svelte.ts` reduces MEMBERSHIP (which operations exist, each one's lifecycle status) and the
latest raw tick. All of it is stateless, so a second copy of one event object can't disagree with the first.

A session owns every ESTIMATE built ON that stream: the ETA smoother and `ScanThroughput`. Those are stateful, they
diverge when one starts later than another, and that is what makes them a single-home concern rather than a preference.
So the ETA a view renders is `session.etaSecondsDisplay` and the scan rates are `session.scan`, from the corner chip and
the queue row alike.

## Sessions are per-window, and that is fine

The operation queue is a separate `WebviewWindow`, so a session cannot be shared across windows; each webview builds its
own from the same broadcast events. The backend registry stays the single source of truth, and sessions are a per-window
projection of it. It looks like a violation of "one session per operation" and isn't. Nothing here assumes two windows,
so folding the queue into the main window as a popup would simply make its sessions the same sessions.

## Why sessions read a fan-out, not their own listeners

The progress dialog subscribes to seven streams for one operation. Ten sessions must not mean seventy subscriptions, but
listener count is the least of it: the fan-out is a **correctness boundary**. One place buffers events arriving for an
id no session has claimed yet, and one place defines arrival order.

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
  exactly the dispatch → dialog → session sequence on a cold main window.
- **(b) Claim, flush, and go live are one synchronous block, with no `await` between them.** Seeding is async, so a
  session that claims its id and then awaits `list_operations()` would overwrite live events with an older seed. The
  seed applies only if nothing has been delivered since the claim. `createOperationsStore.init` guards this exact shape
  twice (subscribe before seeding, apply the seed only if nothing fresher arrived), and that is the precedent this
  copies.
- **(c) The flush precedes any live delivery for that id.** See the ordering argument above.

## Seeding, and its miss case

A session created for an operation this window has heard nothing about asks `list_operations()`. With operations
surviving a reload, a reloaded main window has to recover them or the chip shows nothing for a transfer that is very
much still running.

The test for "heard nothing" is whether the attach delivered anything, and it gates the CALL, not just the result. A
live window's fan-out already holds the latest snapshot, so every row that appears while it is up is claimed with its
row in hand: without the gate each view of each row would cost an IPC round trip for an answer already in memory, and
sessions now have one view per queue row.

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
- `etaSecondsDisplay`: the backend ETA through this session's smoother. Every view renders this, never
  `progress.etaSeconds`.
- `scan`: the counting readout, including the frontend-computed rates the backend does not emit during a scan.
- `conflict`: the conflict the operation is parked on, set from the event. Nothing clears it, because resolving one is a
  command and a session issues none.
- `outcome` / `settled` / `settleEventReceived`: how it ended, whether it ended, and whether the backend task has torn
  down. The last is separate because `write-settled` says the task is gone, not how it finished. `outcome` is
  write-once, so a cancel racing a completion cannot flip an answer a view already rendered.

## Testing

`_testEmit(streamEvent)` on the fan-out (and on the registry, which forwards) drives the same demultiplexing path a live
event takes, following `operations-store.svelte.ts`'s `_testApplySnapshot` / `_testApplyProgress`. The two ordering
rules are driven with explicit interleavings (a `deferred<T>()` for the seed, an attach-then-emit pair in one
synchronous block for the flush), never incidental microtask order.

`bind-operation-session.svelte.test.ts` stands in for a view with an `$effect.root`, and asks the REGISTRY whether a
release happened rather than asking the view: a binding that never releases leaves a session listening for an operation
that ended, which nothing on screen would show, so the test acquires again and checks it got a fresh session.
