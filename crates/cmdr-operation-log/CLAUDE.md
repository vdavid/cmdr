# Operation log (`cmdr-operation-log`)

The durable, cross-volume journal of every file mutation: the base for rollback/undo, indexed name search, and
retention. **The app's first durable DB** (`operation-log.db`, Time Machine-backed); every other on-disk store is a
disposable cache. Tauri-free; the app hosts it in `apps/desktop/src-tauri/src/operation_log_host/`.

## Module map

`handle.rs` (`OperationLog`: journal slot, writer, the `recording.rs` gate; `OpJournal`), `store/` (the DB, the
migration ladder), `writer.rs` (+ `writer/prune.rs`), `capture.rs`, `types.rs` (typed tokens), `query.rs`,
`retention.rs`, `rollback.rs` (+ `order.rs`, `skips.rs`, `runner.rs`, `self_collision.rs`: the PLANNER; its executor is
`crates/cmdr-operations/src/rollback.rs`).

## Must-knows

The full text of each, with its why: DETAILS § "Must-knows, in full".

- **The log never names its host**: the data dir, settings, runtime, and volume lookup come in as values or traits.
- **DURABLE and MIGRATES; never delete-and-recreate.** Append a `Migration` to `store/migrations.rs`, ❌ NEVER edit or
  renumber a shipped step. A downgrade is refused; only an unparseable file is rebuilt (typed sqlite code).
- **NO `platform_case` collation**: `name_folded` plus plain b-tree equality keeps the file `sqlite3`-openable.
- **One writer thread, one cross-volume DB.** `record_items` BLOCKS under backpressure; a DB error drops ONE row and the
  completeness check degrades the op.
- **Classification is typed end to end**, each token mapped ONLY in `types.rs`. Renaming a token is a schema change.
- **The writer stores terminal state; eligibility is `capture.rs`'s**, and a driver's note beats the per-kind rule.
- **Capture is the op's `OpJournal`, fixed at `journal_open`** and carried in the engine's op scope (❌ never extend
  `OperationEventSink`). Recording is decided ONCE per op, at open (❌ never mid-op).
- **A multi-operation undo reverses NEWEST FIRST** (`undo_order`); callers pass ids in APPLY order.
- **Rollback FAILS SAFE**: recheck each item against its snapshot AND its restore target (a copy: its original; a trash:
  the node it landed as); drift, unverifiable, an occupied target, or a gone original ⇒ SKIP, never operate.
  `verify_snapshot` + `SkipReason` + `SkipTally` are SHARED with the in-flight reversals: ❌ don't fork them.
  `rolling_back` guards double-rollback and the retention race.
- **The planner PLANS; an injected `RollbackRunner` acts.** Pause parks BEFORE the item is verified, and a reversal's
  "should I stop?" is `StopMeans`, ❌ NOT `is_cancelled`.
- **Op ids are UUIDv7 (`new_operation_id`)**, ❌ never v4: only v7 sorts by time.
- **The journal records what SURVIVES**, whatever the terminal state, never the mid-write partial.
- **Search spans every `row_role`; retention prunes whole ops only**, ❌ never a `rolling_back` op or its target.

Everything else: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
