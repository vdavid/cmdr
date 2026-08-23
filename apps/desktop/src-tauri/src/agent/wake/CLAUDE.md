# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it. Change events become per-folder counters, counters
an interest score, scores deadlines, and a wake turns whatever waits into one budgeted digest and one turn. Depth:
`DETAILS.md`.

## Module map

- The pure core: `coalesce.rs` (counters), `interest.rs` (score, tier, delay), `compact.rs` (the budgeted digest),
  `inbox.rs` (what waits and when it is due), `readiness.rs` (may it watch, may it think), `job.rs` (a wake in two
  halves: `prepare_wake`, then `run_prepared_wake`). `persist.rs` is the one file here taking a `Connection`.
- The driver: `channel.rs` (the tap's process-global lane), `writer.rs` (the thread owning the `Inbox`, its write
  connection, and the timer), `runner.rs` (the wake thread and its outcome line), `snapshot.rs` (cached readiness),
  `importance.rs` (TTL-cached weights), `settings.rs` (cadence and `proactive`).

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, calls
  `send_rollup`, returns; the writer thread does the lookup, the admit, and the write. A mutex would block every live
  batch for a model call; a per-admit connection runs the migration ladder against a 5 s busy timeout.
- **Bundles carry counters, never file names.** Names grow memory with the EVENT count, on the one path that must
  survive five million. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Unscored importance is not zero.** Mirror `WeightLookup`'s three-way answer and refuse its `score()` collapse, or a
  folder the scorer hasn't reached ranks like `node_modules`.
- **Consent outranks disk access outranks the key.** Without consent the pipeline stores NOTHING; with consent but no
  key, signal accumulates. None of the three is silence, and the answer is CACHED (`snapshot.rs`), never per batch.
- **A merge can only pull a deadline earlier**, or a steady trickle postpones a folder forever. ⚠️ **A cold row's
  deadline is `None`, and no-deadline LOSES every merge**: `Option::min` compiles, reads right, does the opposite
  (`None < Some(_)`).
- **Any wake drains the WHOLE inbox**, so cold bundles ride along free and a MAX-interest policy falls out rather than
  being written.
- **The tap is a second observer inside `process_live_batch`, AFTER rename detection and storm coalescing.** ❌ Never a
  parallel FSEvents subscription, never per-file messages. Three of its four counters are unreachable there and wired
  in by hand crate-side; break one and the agent stops noticing renames or bulk deletes.
- **A wake reuses `ChatRuntime` and opens a real thread** with `ConversationOrigin::Notification`. Nothing drains until
  the turn is certain, and the turn runs on its own thread, so the inbox is never held across it.
- **`askCmdr.proactive` ships FALSE** (`settings.rs`), so nothing wakes until the surfaces that make a wake visible
  exist. ⚠️ `settings.json` is sparse: spell every default out, since `unwrap_or_default()` means a zero-second cadence.
- **A cadence change RE-PRICES the inbox, not just the timer** (`Inbox::reprice`). The merge is min-only, so a
  lengthened delay would otherwise never reach anything already waiting.

Depth for every bullet above: `DETAILS.md`. What a wake produces: `../suggested_ops/CLAUDE.md`. The store:
`../store/proposals/CLAUDE.md`.
