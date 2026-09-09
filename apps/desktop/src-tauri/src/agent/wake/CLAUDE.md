# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it: change events become per-folder counters, counters
an interest score, scores deadlines, and a wake turns what waits into one budgeted digest and turn.

## Module map

- **Pure core**: `coalesce.rs` (counters) → `interest.rs` (score, tier, delay) → `inbox.rs` (what waits, when) →
  `compact.rs` (the digest); `job.rs` runs one, `persist.rs` alone holds a `Connection`.
- **Driver**: `writer.rs` owns the `Inbox` and the timer, fed by `channel.rs`, gated by `readiness.rs` +
  `snapshot.rs`, paced by `schedule.rs` + `spend.rs`, weighted by `importance.rs`, run by `runner.rs`, announced by
  `indicator.rs` + `staged.rs`.

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, sends it,
  returns; a mutex there stalls every live batch on a model call.
- **Bundles carry counters, never file names**: this path must survive five million events. The digest says WHERE and
  HOW MUCH; the agent looks up WHAT.
- **Floored never gets in; unscored always does.** `admit_if_permitted` refuses `Floored`. ❌ Never refuse `Unknown`:
  that ignores every new folder.
- **Consent > AI-off > disk access > key** (`snapshot.rs` caches it). ❌ Never fold `Off` into `NeedsApiKey`, or the
  corner nags for a provider the user switched off. ❌ Only lost consent purges stored rows.
- **A merge only pulls a deadline earlier**, or a trickle postpones a folder forever; a cadence change re-prices the
  whole inbox (`Inbox::reprice`), not just the timer. ⚠️ A cold row's deadline is `None`, which `Option::min` ranks
  FIRST: it compiles, reads right, does the opposite.
- **The tap is a second observer inside `process_live_batch`**, after rename detection and storm coalescing. ❌ Never
  a parallel FSEvents subscription.
- **A wake reuses `ChatRuntime`** on its own thread (`ConversationOrigin::Notification`). Its first message is the
  digest as STRUCTURE, outliving every locale pass.
- **The corner hears on its OWN event** (`indicator.rs`): ❌ clear it on every exit, or a stale spinner clicks into a
  deleted thread.
- **`askCmdr.proactive` ships TRUE** (`settings.rs`). ⚠️ `settings.json` is sparse: spell defaults out, or
  `unwrap_or_default()` silently ships it off at zero cadence.
- **A wake with nothing to say deletes its own thread, and only a wake does** (`quiet.rs`). ❌ Never log its reason;
  fold its cost onto the reserved row first, or the agent's spend reads zero.
- **A rejected sweep earns ONE follow-up turn** (`followup.rs`), coalesced per SWEEP. ❌ It never discards its
  thread: that thread is the user's.
- **Three seatbelts cap PROACTIVE spend** (`schedule.rs` + `spend.rs`): minimum spacing between wakes, a daily token
  ceiling, a backoff on a typed auth or quota refusal. ⚠️ Spacing is NOT the cadence slider. ❌ Nothing the user types
  is capped.

Depth: `DETAILS.md`. What it produces: `../suggested_ops/CLAUDE.md`. The store: `../store/proposals/CLAUDE.md`.
