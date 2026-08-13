# Operation sessions

One session per operation per window: bound to an `operationId`, it reads the window's event fan-out and exposes what
the operation is right now (status, phase, counts, rates, smoothed ETA, conflict, outcome) plus what you can do to it.
Views bind, render, and command through it; zero views is an ordinary state.

## Module map

- `operation-event-fanout.ts` demultiplexes seven broadcast streams per window; `operation-session.svelte.ts` +
  `operation-session-commands.svelte.ts` are the read state and the five commands; `operation-session-registry.ts`
  refcounts them; `bind-operation-session.svelte.ts` is how a view attaches; `window-operation-sessions.svelte.ts` holds
  this window's instance. Per-file detail: DETAILS § File map.

## Must-knows

- **Two views of one operation share ONE session** (stateful smoothers diverge when started apart). Bind with
  `bindOperationSession`, ❌ never `createOperationSession`.
- **Bind on the id VALUE, ❌ never the row holding it**: rows are rebuilt on each `operations-changed` tick, and
  re-acquiring restarts the smoother mid-transfer.
- **Claim → flush → go live is ONE sync block**; ❌ no `await` between `fanout.attach()` and the return, or an older
  sample lands after a newer one and corrupts the smoother.
- **`list_operations()` runs ONCE, awaited, in `fanout.init()`**; a broadcast landing mid-seed wins over it, a session's
  own seed is the fallback. DETAILS § Seeding.
- **Settle comes from the terminal EVENTS, ❌ never from leaving the snapshot** (completed, cancelled, and never-existed
  all look "removed"). The seeding miss is the one exception: `outcome: 'gone'`.
- **The fan-out routes and holds, ❌ it isn't a gate**: `queue/operations-store.svelte.ts` DROPS a `write-progress` for
  an id it has no snapshot for, the fan-out BUFFERS it. Both are right. DETAILS § "Two fates".
- **The buffer is bounded, and the bound is the point**: newest event of each kind per unclaimed id, dropped on claim,
  aged out at `UNCLAIMED_BUFFER_TTL_MS`. ❌ Not an append-only log. Beside it, the newest tick of each LIVE operation is
  kept claimed or not (a paused one emits nothing to refill), and dropped on every terminal event, so a session claiming
  an id after the end resolves instead of painting bars over an ending.
- **An operation waiting on a PERSON has no speed, and still has a time left**: `bytesPerSecondDisplay` /
  `filesPerSecondDisplay` answer `null` while it's paused or parked on an unanswered clash (the status, plus the
  backend's `activity.waitingOn === 'you'`); `etaSecondsDisplay` survives both, because the backend leaves human-wait
  time out of its rate window so the number stays true. Render those three, ❌ never a rate or ETA off the raw tick.
- **❌ No `$derived` in a session** (it outlives the view that created it). Compose in the getter.
- **A scanning operation reads as live and counting, never 0%**: totals stay 0 through a scan, so views branch on
  `phase === 'scanning'`.
- **No command throws, and each says whether it landed**: `false` or a `null` verdict means nothing was sent, so leave
  the screen alone.
- **`togglePause` steers by the snapshot's lifecycle status, ❌ never `is_running`** (a parked operation still says
  `true`).
- **Cancel goes through the MANAGER, rollback through the write op** (`cancelWriteOperation(id, true)`; `cancel()` also
  drops a lane-waiting operation). Rollback is refused once a cancel is on its way; cancel is never refused during a
  rollback.
- **Answering a clash is a DELEGATION, and the answer NAMES its clash**: `resolveConflict(conflictId, …)` releases THAT
  clash only. ❌ Never clear on "an answer came back" (the next clash arrives mid-flight), never make correctness depend
  on one privileged surface, and never refuse a clash this session hasn't seen — an adopted operation has none.

Architecture, the registry rationale, the buffer's bound, the ordering rules, and where an archive-password prompt
belongs: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
