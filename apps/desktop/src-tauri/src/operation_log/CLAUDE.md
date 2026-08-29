# Operation log subsystem

The durable, cross-volume journal of every file mutation: the base for rollback/undo, indexed name search, and
retention. **The app's first durable DB** (`operation-log.db` in the app data dir, Time Machine-backed) — every other
on-disk store here is a disposable cache.

MCP tools live in `mcp/executor/operation_log.rs`; UI surfaces are frontend-only over the read API.

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
  refused, never wiped; delete-and-recreate only for an unparseable file (typed sqlite code, not a string).
- **NO `platform_case` collation.** A precomputed `name_folded` column (Unicode-lowercase + NFC) plus plain b-tree
  equality keeps the file `sqlite3`-openable. Don't add a collation like the other stores.
- **One writer thread, one cross-volume DB, NO per-volume registry.** `record_items` BLOCKS under backpressure
  (lossless), never dropping; a DB error on one row drops THAT row without failing the op. So the capture completeness
  check degrades a `rollback_unit` gap to `not_rollbackable` and a `search_only` gap to `top_level_only` — never a
  silent under-reverse.
- **Classification is typed end to end**: every `kind`, `initiator`, status, `row_role`,
  `outcome`, and `rollback_skip_reason` is a `types.rs` enum with a stable token, mapped ONLY there. Renaming a token is
  a schema change; renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** That reasoning lives in `capture.rs` — keep
  business logic out of `writer.rs`. A driver can also NOTE a reason the rows can't express
  (`note_not_rollbackable`, e.g. a directory merge); a note beats the per-kind rule.
- **Capture is a process-global journal reached by `op_id`, NOT threaded through the pipeline.** ❌ Never extend
  `OperationEventSink`. Install via `set_journal` (production `start` only; tests use `TestJournalGuard`).
- **A multi-operation undo reverses NEWEST FIRST, one at a time** (`undo_order`): a later rename batch can take a name
  an earlier one freed, so oldest-first leaves that file unrestored. Callers pass ids in APPLY order.
- **Rollback FAILS SAFE** (data-safety-critical): recheck each item against its snapshot AND its restore target; drift,
  unverifiable, or occupied target ⇒ SKIP (→ `partially_rolled_back`), never operate. A restore-move never overwrites.
  `rolling_back` guards double-rollback and the retention race. A skip stores WHICH reason; NULL = not recorded.
- **The engine PLANS; an injected `RollbackRunner` acts.** ❌ Never import `write_operations` from here to perform an
  act — the reach is the whole reason the runner is injected. Pause parks at the item boundary BEFORE the snapshot is
  verified: a park between "verified unchanged" and "delete" would let a stale verification authorize a destructive act.
  And a reversal's "should I stop?" is NOT `is_cancelled` (`StopMeans`). `DETAILS.md` § "Planner here, executor
  injected".
- **Search spans every `row_role`; retention prunes whole ops only.** Name search matches `source_name_folded` across
  `rollback_unit` AND `search_only` rows, so a leaf hits inside a trashed folder. Retention never prunes a
  `rolling_back` op or its target.

Design, ladder, schema, query/search, retention, rollback contract, alpha UI, and the dev bin: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
