# Operation sessions

One session per operation per window, bound to an `operationId`: it reads the window's fan-out and says what the
operation is now plus what you can do to it. Views bind and command through it; zero views is ordinary.

## Module map

- `operation-event-fanout.ts` demultiplexes eight broadcast streams; `operation-session.svelte.ts` +
  `operation-session-commands.svelte.ts` hold the read state and the five commands; `operation-session-registry.ts`
  refcounts them, `bind-operation-session.svelte.ts` attaches a view, `window-operation-sessions.svelte.ts` is this
  window's instance. DETAILS § File map.

## Must-knows

- **Two views of one operation share ONE session** (stateful smoothers diverge when started apart). Bind with
  `bindOperationSession`, ❌ never `createOperationSession`, and on the id VALUE, ❌ never the row holding it: rows are
  rebuilt each `operations-changed` tick, and re-acquiring restarts the smoother mid-transfer.
- **Claim → flush → go live is ONE sync block**; ❌ no `await` in it, or an older sample lands after a newer one and
  corrupts the smoother. DETAILS § "The three rules".
- **Settle comes from the terminal EVENTS, ❌ never from leaving the snapshot** (completed, cancelled, and never-existed
  all look "removed"). The seeding miss is the one exception: `outcome: 'gone'`.
- **The fan-out BUFFERS what `queue/operations-store.svelte.ts` DROPS**, bounded on purpose: newest event of each kind
  per unclaimed id, plus the newest tick of each live one. ❌ Not a gate, ❌ not a log. DETAILS § "The buffer's bound".
- **Render `bytesPerSecondDisplay` / `filesPerSecondDisplay` / `etaSecondsDisplay`, ❌ never a rate or ETA off the raw
  tick**: the rates go `null` while a person decides, the ETA survives that wait. DETAILS § "Read surface".
- **❌ No `$derived` in a session** (it outlives the view that created it). Compose in the getter.
- **A scanning operation reads as live and counting, never 0%**: totals stay 0, so branch on `phase === 'scanning'`.
- **No command throws, and each says whether it landed**: `false` or a `null` verdict means nothing was sent, so leave
  the screen alone. `togglePause` steers by the snapshot's `LifecycleStatus` it already holds, never a round trip;
  cancel goes through the MANAGER, rollback through the write op.
- **An answer NAMES its clash**: `resolveConflict(conflictId, …)` releases THAT clash only, and so does the
  `conflictResolved` delivery, which is how a clash answered by ANOTHER surface (or by an agent over MCP) stops being
  shown here. ❌ Never clear on "an answer came back", never privilege one surface, never refuse a clash this session
  hasn't seen. DETAILS § "Answering a clash is a delegation".

Everything else: `DETAILS.md`. Read it before any non-trivial work here.
