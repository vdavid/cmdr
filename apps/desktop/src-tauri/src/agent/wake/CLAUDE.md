# Wake pipeline (`agent/wake/`)

How the agent decides it has something worth saying, then says it. Change events become per-folder counters, counters
an interest score, scores deadlines, and a wake turns what waits into one budgeted digest and turn.

## Module map

- The pure core: `coalesce.rs` (counters), `interest.rs` (score, tier, delay), `compact.rs` (the digest), `inbox.rs`
  (what waits and when), `readiness.rs` (the gates), `job.rs` (`prepare_wake` / `run_prepared_wake`). `persist.rs` is
  the one file here taking a `Connection`.
- The driver: `channel.rs` (the tap's lane), `writer.rs` (the thread owning the `Inbox`, its connection, the timer),
  `runner.rs` (the background turn), `schedule.rs` (when it may speak) + `spend.rs` (what it may spend), `snapshot.rs`
  (cached readiness), `indicator.rs` + `staged.rs` (the corner's event, the toast's), `importance.rs` (cached
  weights), `settings.rs` (cadence, `proactive`), `quiet.rs` (a wake with nothing to say), `followup.rs` (the
  rejected-sweep turn).

## Must-knows

- ❌ **Nothing on the live-loop thread may take a lock or touch SQLite.** The tap builds a `FolderActivity`, sends it, returns. A mutex there blocks every live batch for a model call; a per-admit connection runs the
  migration ladder against a 5 s busy timeout.
- **Bundles carry counters, never file names.** Names grow memory with the EVENT count, on a path that must survive
  five million. The digest says WHERE and HOW MUCH; the agent looks up WHAT.
- **Floored never gets in; unscored always does.** `admit_if_permitted` refuses `Floored`: weight 0 earns no deadline,
  so the row is dead tokens in every digest. Cmdr's own data dir floors under `~/Library`, needing no self-naming
  exclusion. ❌ Never refuse `Unknown`: collapsing the two ignores every new folder.
- **Consent outranks disk access outranks the key**, cached in `snapshot.rs`. Without consent the pipeline stores
  nothing and **purges what it stored** (`purge_if_not_permitted`). No key: signal accumulates.
- **A merge only pulls a deadline earlier**, or a trickle postpones a folder forever. ⚠️ **A cold row's deadline is
  `None`, and no-deadline LOSES every merge**: `Option::min` compiles, reads right, does the opposite.
- **The tap is a second observer inside `process_live_batch`**, after rename detection and storm coalescing. ❌ Never
  a parallel FSEvents subscription. Three of its four counters are wired crate-side by hand.
- **A wake reuses `ChatRuntime`** on its own thread (`ConversationOrigin::Notification`), streaming on the rail's
  transport, bracketed by `Started` and, when quiet, `Discarded`. Its first message is the digest as STRUCTURE, outliving
  every locale pass.
- **The corner hears on its OWN event** (`indicator.rs`): ❌ clear it on every exit, or a stale spinner clicks into a
  deleted thread.
- **`askCmdr.proactive` ships TRUE** (`settings.rs`). ⚠️ `settings.json` is sparse: spell defaults out, or
  `unwrap_or_default()` silently ships it off at zero cadence.
- **A cadence change RE-PRICES the inbox, not just the timer** (`Inbox::reprice`): the merge is min-only.
- **A wake with nothing to say deletes its own thread, and only a wake does** (`quiet.rs`). ❌ Never log the reason;
  fold its cost onto the reserved row first, or the agent's spend reads zero.
- **A rejected sweep earns ONE follow-up turn** (`followup.rs`), coalesced per SWEEP. ❌ It never discards its
  thread: that thread is the user's, and a closed gate DROPS the ask.
- **Three seatbelts cap PROACTIVE spend, all backstops rather than calibration** (`schedule.rs` + `spend.rs`): a
  15-minute `MIN_WAKE_SPACING`, a 200,000-token daily ceiling scoped by `ConversationOrigin`, and a six-hour backoff on
  a typed auth or quota refusal. ⚠️ Spacing is NOT the cadence slider (how fast it reacts vs how often it speaks).
  ❌ Nothing the user types is throttled or capped. A force and a follow-up skip spacing; settings and readiness
  changes clear it.

Depth: `DETAILS.md`. What it produces: `../suggested_ops/CLAUDE.md`. The store: `../store/proposals/CLAUDE.md`.
