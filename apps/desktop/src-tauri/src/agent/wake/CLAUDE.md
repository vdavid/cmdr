# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it. Change events become per-folder counters, counters
an interest score, scores deadlines; a wake turns what waits into one budgeted digest and turn.

## Module map

- The pure core: `coalesce.rs` (counters), `interest.rs` (score, tier, delay), `compact.rs` (the digest), `inbox.rs`
  (what waits and when), `readiness.rs` (the gates), `job.rs` (`prepare_wake` / `run_prepared_wake`). `persist.rs` is
  the one file here taking a `Connection`.
- The driver: `channel.rs` (the tap's lane), `writer.rs` (the thread owning the `Inbox`, its connection, the timer),
  `runner.rs` (the background-turn thread), `snapshot.rs` (cached readiness), `indicator.rs` + `staged.rs` (the
  corner's event, the toast's), `importance.rs` (TTL-cached weights), `settings.rs` (cadence, `proactive`), `quiet.rs`
  (a wake with nothing to say), `followup.rs` (the turn a rejected sweep earns).

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, calls
  `send_rollup`, returns; the writer thread looks up, admits, writes. A mutex there blocks every live batch for a model
  call; a per-admit connection runs the migration ladder against a 5 s busy timeout.
- **Bundles carry counters, never file names.** Names grow memory with the EVENT count, on a path that must survive
  five million. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Unscored importance is not zero.** Mirror `WeightLookup`'s three-way answer and refuse its `score()` collapse, or a
  folder the scorer hasn't reached ranks like `node_modules`.
- **Consent outranks disk access outranks the key**, cached in `snapshot.rs`. Without consent the pipeline stores
  nothing and **purges what it stored** (`purge_if_not_permitted`). No key: signal accumulates. The corner renders the
  last two gaps, silent on `NeedsConsent` and on `proactive` off.
- **A merge can only pull a deadline earlier**, or a trickle postpones a folder forever. ⚠️ **A cold row's deadline is
  `None`, and no-deadline LOSES every merge**: `Option::min` compiles, reads right, does the opposite.
- **The tap is a second observer inside `process_live_batch`, AFTER rename detection and storm coalescing.** ❌ Never a
  parallel FSEvents subscription, never per-file messages. Three of its four counters are unreachable there and are
  wired in crate-side by hand; break one and renames or bulk deletes go unnoticed.
- **A wake reuses `ChatRuntime` and opens a real thread** (`ConversationOrigin::Notification`), drains nothing until
  the turn is certain, and runs on its own thread. It streams on the rail's transport, bracketed by `Started` and, when
  it stays quiet, `Discarded`. Its first message is the digest as STRUCTURE: that row outlives every locale pass.
- **The corner hears on its OWN event** (`indicator.rs`): phase plus readiness, cleared on every exit so no stale
  spinner offers a click into a deleted thread. `staged.rs` is a third, for a turn that PROPOSED something.
- **`askCmdr.proactive` ships TRUE** (`settings.rs`); the readiness gates, not this row, keep a non-AI user untouched.
  ⚠️ `settings.json` is sparse: spell defaults out, or `unwrap_or_default()` silently ships it off, at zero cadence.
- **A cadence change RE-PRICES the inbox, not just the timer** (`Inbox::reprice`), since the merge is min-only.
- **A wake with nothing to say deletes its own thread, and only a wake does** (`quiet.rs`). ❌ Never log the reason, and
  never plain-delete: fold its cost onto the reserved row first, or the agent's spend reads zero.
- **A rejected sweep earns ONE follow-up turn** (`followup.rs`), the second kind of background turn `runner.rs` drives,
  coalesced per SWEEP behind a trailing window. ❌ It never discards its thread: that thread is the user's. A closed
  gate DROPS the ask rather than parking it.

Depth: `DETAILS.md`. What it produces: `../suggested_ops/CLAUDE.md`. The store: `../store/proposals/CLAUDE.md`.
