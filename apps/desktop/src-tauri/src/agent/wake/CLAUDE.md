# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it. Change events become per-folder counters, counters an
interest score, scores deadlines, and a wake turns whatever waits into one budgeted digest and one turn.

## Module map

- The pure core: `coalesce.rs` (counters), `interest.rs` (score, tier, delay), `compact.rs` (the budgeted digest),
  `inbox.rs` (what waits and when it's due), `readiness.rs` (may it watch, may it think), `job.rs` (a wake in two
  halves: `prepare_wake`, then `run_prepared_wake`). `persist.rs` is the one file here taking a `Connection`.
- The driver: `channel.rs` (the tap's process-global lane), `writer.rs` (the thread owning the `Inbox`, its write
  connection, and the timer), `runner.rs` (the wake thread and its outcome line), `snapshot.rs` (cached readiness),
  `indicator.rs` (the status corner's one event), `importance.rs` (TTL-cached weights), `settings.rs` (cadence,
  `proactive`), `quiet.rs` (a wake with nothing to say).

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, calls
  `send_rollup`, returns; the writer thread does the lookup, the admit, the write. A mutex there blocks every live batch
  for a model call; a per-admit connection runs the migration ladder against a 5 s busy timeout.
- **Bundles carry counters, never file names.** Names grow memory with the EVENT count, on the one path that must
  survive five million. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Unscored importance is not zero.** Mirror `WeightLookup`'s three-way answer and refuse its `score()` collapse, or a
  folder the scorer hasn't reached ranks like `node_modules`.
- **Consent outranks disk access outranks the key**, and the answer is CACHED (`snapshot.rs`). Without consent the
  pipeline stores NOTHING; with consent but no key, signal accumulates. The corner renders the last two gaps and stays
  silent on `NeedsConsent` (and whenever `proactive` is off).
- **A merge can only pull a deadline earlier**, or a steady trickle postpones a folder forever. ⚠️ **A cold row's
  deadline is `None`, and no-deadline LOSES every merge**: `Option::min` compiles, reads right, does the opposite
  (`None < Some(_)`).
- **Any wake drains the WHOLE inbox**, so cold bundles ride along free and a MAX-interest policy falls out unwritten.
- **The tap is a second observer inside `process_live_batch`, AFTER rename detection and storm coalescing.** ❌ Never a
  parallel FSEvents subscription, never per-file messages. Three of its four counters are unreachable there and are
  wired in by hand crate-side; break one and the agent stops noticing renames or bulk deletes.
- **A wake reuses `ChatRuntime` and opens a real thread** (`ConversationOrigin::Notification`), drains nothing until the
  turn is certain, and runs on its own thread. Its turn streams on the transport a rail send uses
  (`agent/chat/stream.rs`), bracketed by `Started` and, when it stays quiet, `Discarded`.
- **The corner hears about it on a SEPARATE event** (`indicator.rs`, `agent-wake-status`): phase plus readiness, cleared
  on every exit so no stale spinner offers a click into a deleted thread. Its stop button is `ask_cmdr_cancel` on the
  shared `agent::chat::cancel` registry rather than a second mechanism.
- **`askCmdr.proactive` ships FALSE** (`settings.rs`). ⚠️ `settings.json` is sparse: spell every default out, or
  `unwrap_or_default()` means a zero-second cadence.
- **A cadence change RE-PRICES the inbox, not just the timer** (`Inbox::reprice`), since the merge is min-only.
- **A wake with nothing to say deletes its own thread, and only a wake does** (`quiet.rs`). ❌ Never log the reason, and
  never plain-delete the thread: fold its cost onto the reserved row first, or the agent's spend reads zero.

Depth: `DETAILS.md`. What a wake produces: `../suggested_ops/CLAUDE.md`. The store: `../store/proposals/CLAUDE.md`.
