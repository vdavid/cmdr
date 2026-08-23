# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it. Change events become per-folder counters, counters
an interest score, scores deadlines, and a wake turns whatever waits into one budgeted digest and one turn. Depth:
`DETAILS.md`.

## Module map

- `coalesce.rs`: events or rollups into per-folder counters, through one `Merger` fold.
- `interest.rs`: how much a bundle is worth waking for, and how soon (`wake_delay` takes the user's hot delay as a
  value; warm derives, cold gets none). Also `tier_of`, what the outcome line counts.
- `compact.rs`: the digest, fitted to a hard token budget; what missed a line is rolled up and counted.
- `inbox.rs`: what is waiting, when it comes due, and what a restart does to it.
- `readiness.rs`: whether the agent may watch, and whether it may think.
- `job.rs`: one wake in two halves — `prepare_wake` (gates, digest, thread, drain) and `run_prepared_wake` (the turn).
- `persist.rs`: the one pure-core file taking a `Connection`; the rest is values in, values out.
- The driver: `channel.rs` (the tap's process-global inbox), `writer.rs` (the thread owning the `Inbox`, its write
  connection, and the timer), `runner.rs` (the wake thread and its outcome line), `snapshot.rs` (cached readiness),
  `importance.rs` (TTL-cached weights), `settings.rs` (cadence and `proactive`).

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, calls
  `send_rollup`, returns; the writer thread does the lookup, the admit, and the write. A mutex would block every live
  batch for a model call; a per-admit connection runs the migration ladder against a 5 s busy timeout.
- **Bundles carry counters, never file names.** Names grow memory with the EVENT count, on the one path that must
  survive five million. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Unscored importance is not zero.** `interest` and `importance.rs` mirror `WeightLookup`s three-way answer and
  refuse its `score()` collapse: a folder the scorer hasn't reached must stay distinguishable from one scored as junk,
  or a fresh clone ranks like `node_modules`.
- **Consent outranks disk access outranks the key**, because each asks the user for something. Without consent the
  pipeline stores NOTHING; with consent but no key, signal accumulates. None of the three is silence, and the answer is
  CACHED (`snapshot.rs`), never queried per batch.
- **A merge can only pull a deadline earlier.** Otherwise a steady trickle postpones a folder forever, which reads as
  an agent asleep rather than patient.
- **Gotcha: a cold row's deadline is `None`, and no-deadline LOSES every merge.** `Option::min` compiles, reads right,
  does the opposite (`None < Some(_)`). `soonest`, `next_deadline`, and `reconcile` each say so, each tested.
- **Any wake drains the WHOLE inbox.** The expensive part is the turn, not the row, so cold bundles ride along free and
  a MAX-interest policy falls out rather than being written.
- **The tap is a second observer inside `process_live_batch`, AFTER rename detection and storm coalescing**, handing
  over per-batch rollups. ❌ Never a parallel FSEvents subscription, never per-file messages. Three of its four counters
  are unreachable there and wired in by hand crate-side; break one and the agent stops noticing renames or bulk
  deletes. `DETAILS.md`.
- **A wake reuses `ChatRuntime` and opens a real thread** with `ConversationOrigin::Notification`, so the sweep points
  back at reasoning the user can read. Nothing drains until the turn is certain, and the turn runs on its own thread,
  so the inbox is never held across it.
- **`askCmdr.proactive` ships FALSE** (`settings.rs`), so nothing wakes until M2 makes wakes visible.

What a wake produces: `../suggested_ops/CLAUDE.md`. The store: `../store/proposals/CLAUDE.md`.
