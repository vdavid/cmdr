# Operation sessions

One session per operation per window: it binds to an `operationId`, reads the window's event fan-out, and exposes what
that operation is right now (status, phase, counts, rates, smoothed ETA, conflict, outcome). Read-only; commands stay
with their callers. Views bind to a session and render it, and zero views is an ordinary state.

## Module map

- `operation-event-fanout.ts`: the demultiplexer. Subscribes the seven broadcast streams once per window and routes each
  one to the session that claimed its `operationId`, buffering for ids nobody has claimed yet.
- `operation-session.svelte.ts`: `createOperationSession(id, fanout)` — the derived read state, plus the
  `list_operations` seed.
- `operation-session-registry.ts`: `createOperationSessionRegistry()` — refcounted `acquire` / `release`.
- `bind-operation-session.svelte.ts`: `bindOperationSession(() => id)` — how a view binds, and how it lets go without
  having to remember to. What the queue rows and the corner chip use.
- `window-operation-sessions.svelte.ts`: this window's instance, plus `initOperationSessions()` /
  `destroyOperationSessions()`, called by `routes/(main)/+page.svelte` and `routes/queue/+page.svelte`.

## Must-knows

- **Two views of one operation MUST share one session.** The ETA smoother and the scan-rate estimator are stateful, so a
  second one started later disagrees with the first until it converges. Views bind with `bindOperationSession`, which
  `acquire`s and releases for them; ❌ never `createOperationSession` directly.
- **Bind on the id as a VALUE, never on the object holding it.** The operations store rebuilds every row on each
  `operations-changed` tick, so a binding keyed on the row would re-acquire mid-transfer and restart the smoother
  whenever an unrelated operation started or finished. `bindOperationSession` derives the string for that reason.
- **Claim, flush, and go live are ONE synchronous block.** ❌ Never introduce an `await` between `fanout.attach()` and
  the return: the flush must land before any live event for that id, because feeding the smoother an older sample after
  a newer one corrupts it. The `list_operations` seed is async on purpose and guarded on `receivedDelivery` for the same
  reason.
- **Settle comes from the terminal EVENTS, never from leaving the snapshot.** "Removed" is what a completed, a
  cancelled, and a never-existed operation all look like. The one exception is the seeding miss case, which resolves to
  `outcome: 'gone'` rather than hanging empty.
- **The fan-out is a router with a holding area, ❌ not a gate.** `queue/operations-store.svelte.ts` keeps receiving
  everything unbuffered, so the same `write-progress` has two fates in one window: the store DROPS it for an id it has
  no snapshot for, the fan-out BUFFERS it. Both are right. DETAILS § "Two fates".
- **The buffer is bounded, and the bound is the point**: the newest event of each kind per unclaimed id, dropped on
  claim, aged out at `UNCLAIMED_BUFFER_TTL_MS` on the next insert. ❌ Don't turn it into an append-only log.
- **❌ No `$derived` in a session.** It outlives the view that created it, and a `$derived` built during a component's
  init belongs to that component's scope. Compose in the getter instead.
- **A session presents a scanning operation as live and counting**, never as 0%: `filesTotal` / `bytesTotal` stay 0
  through a scan, so `scan` carries the counting readout and views branch on `phase === 'scanning'`.

Architecture, the registry rationale, the buffer's bound, and the ordering rules: `DETAILS.md`.
