# Operation log subsystem

The durable, cross-volume journal of every file mutation: the base for rollback/undo, indexed name search, and
retention. **The app's first durable DB** (`operation-log.db`, Time Machine-backed) — every other on-disk store here is
a disposable cache. MCP tools: `mcp/executor/operation_log.rs`; UI surfaces are frontend-only over the read API.

## Module map

- `store/` — the DB (connection factory, migration ladder, schema, `intern_dir`, `fold_name`, low-level reads).
- `writer.rs` — the ONE writer thread (+ `writer/prune.rs`, the bounded retention sweep it runs); `capture.rs` feeds
  it; `types.rs` holds the typed tokens.
- `query.rs` — reads; `retention.rs` — startup + periodic prune; IPC in `commands/operation_log.rs`.
- `rollback.rs` (+ `order.rs`, `skips.rs`, `runner.rs`) — the rollback PLANNER; the executor it's handed, the spawn
  glue, and the multi-op driver live in `write_operations/rollback.rs`. `mod.rs::start` opens the DB, reconciles,
  spawns retention.

## Must-knows

- **DURABLE and MIGRATES; never delete-and-recreate on a version bump.** The ladder (`store/migrations.rs`) is the
  template future durable DBs follow: append a `Migration`, NEVER edit or renumber a shipped step. A downgrade is
  refused; delete-and-recreate only for an unparseable file (typed sqlite code, ❌ not a string).
- **NO `platform_case` collation.** A precomputed `name_folded` column (Unicode-lowercase + NFC) plus plain b-tree
  equality keeps the file `sqlite3`-openable. Don't add a collation like the other stores.
- **One writer thread, one cross-volume DB, NO per-volume registry.** `record_items` BLOCKS under backpressure
  (lossless); a DB error drops that ONE row, and the completeness check degrades the op rather than under-reversing
  it.
- **Classification is typed end to end**: every `kind`, `initiator`, status, `row_role`, `outcome`, and
  `rollback_skip_reason` is a `types.rs` enum with a stable token, mapped ONLY there. Renaming a token is a schema
  change; renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** That reasoning lives in `capture.rs`. A driver
  may also NOTE a reason the rows can't express (`note_not_rollbackable`, say a directory merge), and a note beats the
  per-kind rule.
- **Capture is a process-global journal reached by `op_id`, NOT threaded through the pipeline.** ❌ Never extend
  `OperationEventSink`. Install via `set_journal` (production `start` only; tests use `TestJournalGuard`).
- **A multi-operation undo reverses NEWEST FIRST** (`undo_order`): a later rename batch can take a name an earlier one
  freed. Callers pass ids in APPLY order.
- **Rollback FAILS SAFE** (data-safety-critical): recheck each item against its snapshot AND its restore target; drift,
  unverifiable, or occupied target ⇒ SKIP (→ `partially_rolled_back`), never operate, and a restore-move never
  overwrites. `verify_snapshot` + `SkipReason` + `SkipTally` are SHARED with the in-flight reversals
  (`write_operations/reversal.rs`): ❌ don't fork them. `rolling_back` guards double-rollback and the retention race.
- **The engine PLANS; an injected `RollbackRunner` acts.** ❌ Never import `write_operations` from here to perform an
  act. Pause parks BEFORE the item is verified (a stale verification must never authorize a destructive act), and a
  reversal's "should I stop?" is NOT `is_cancelled` (`StopMeans`). `DETAILS.md` § "Planner here, executor injected".
- **Op ids are UUIDv7 (`new_operation_id`), never v4.** The clock is whole SECONDS, so same-second ops tie and every
  ordered read falls back to `op_id`; v7 sorts chronologically, v4 is a coin flip.
- **The journal records what SURVIVES an operation, whatever its terminal state**: a canceled copy's files AND the dirs
  it created, never the mid-write partial (both terminal paths remove it).
- **Search spans every `row_role`; retention prunes whole ops only.** Name search matches `source_name_folded` across
  `rollback_unit` AND `search_only` rows, so a leaf hits inside a trashed folder. Retention never prunes a
  `rolling_back` op or its target.

Design, ladder, schema, query/search, retention, rollback contract, alpha UI, and the dev bin: `DETAILS.md`. Read it
before any non-trivial work here.
