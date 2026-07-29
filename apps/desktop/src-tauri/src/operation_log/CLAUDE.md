# Operation log subsystem

The durable, cross-volume journal of every file mutation: the base for rollback/undo, indexed name search, and
retention. **The app's first durable DB** (`operation-log.db` in the app data dir, Time Machine-backed) — every other
on-disk store here is a disposable cache. Full design + rationale: `DETAILS.md`.

MCP tools live in `mcp/executor/operation_log.rs`; UI surfaces are frontend-only over the read API (Debug panel
`routes/debug/DebugOperationLogPanel.svelte`, alpha dialog `src/lib/operation-log/`, Ask Cmdr's rename undo;
`DETAILS.md` § Alpha UI).

## Module map

- `store/` — the DB: connection factory, migration ladder (`migrations.rs`), schema, `intern_dir`, `fold_name`,
  low-level reads. `OperationLogStore` owns the schema lifecycle.
- `writer.rs` — the ONE writer thread (`open_operation` / `record_items` / `finalize_operation` / `set_rollback_state`
  / `set_item_outcomes` / `prune`); batched inserts; the retention mechanism (age + size prune, dir GC, vacuum).
- `query.rs` — reads (index-served name search, paged `recent_operations` / `get_operation`); `retention.rs` runs
  `prune` on a startup + periodic timer; IPC in `commands/operation_log.rs`.
- `rollback.rs` — the rollback engine (inverse-per-item + recheck, `rolling_back` state machine, startup reconcile);
  `rollback/order.rs` — `undo_order`; `rollback/skips.rs` — the skip breakdown. Spawn glue + the multi-operation driver
  (`undo_operations`): `write_operations/rollback.rs`. `capture.rs` feeds the writer. `types.rs` — the typed tokens.
- `mod.rs::start` — opens the DB, reconciles rollback, spawns retention, manages the writer.

## Must-knows

- **DURABLE and MIGRATES; never delete-and-recreates on a version bump.** The ladder (`store/migrations.rs`) is the
  template future durable DBs follow: append a `Migration`, NEVER edit or renumber a shipped step. A downgrade is
  refused, never wiped; delete-and-recreate only for an unparseable file (typed sqlite code, not a string).
- **NO `platform_case` collation** (D2): a precomputed `name_folded` column (Unicode-lowercase + NFC) + plain b-tree
  equality keeps the file `sqlite3`-openable. Don't add a collation like the other stores.
- **One writer thread, one cross-volume DB, NO per-volume registry** (D1). `record_items` BLOCKS under backpressure
  (lossless), never dropping; a DB error on one row drops THAT row without failing the op. So
  `finalize_operation` returns per-`row_role` durable counts, and the capture completeness check degrades a
  `rollback_unit` gap to `not_rollbackable`, a `search_only` gap to `top_level_only` — never a silent under-reverse.
- **Classification is typed end to end** (`no-string-matching`): every `kind`, `initiator`, status, `row_role`,
  `outcome`, `rollback_skip_reason` is a `types.rs` enum with a stable token; the mapping lives ONLY there. Renaming a token is a schema
  change; renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** Eligibility (D3) + net-new/subkind reasoning
  live in `capture.rs` — keep business logic out of `writer.rs`.
- **Capture is a process-global journal reached by `op_id`, NOT threaded through the pipeline** (recorded deviation
  from D4; its hard rule — never extend `OperationEventSink` — holds). Install via `set_journal` (production
  `start` only; tests use `TestJournalGuard`, which serializes the slot under plain `cargo test`); the pipeline calls
  the `journal_*` free functions by `op_id`. Rationale + record points: `DETAILS.md` § Capture.
- **A multi-operation undo reverses NEWEST FIRST, one at a time** (`undo_order`, driven by `undo_operations`): a later
  rename batch can take a name an earlier one freed, so oldest-first leaves that file unrestored. Callers pass ids in
  APPLY order (it breaks a same-second tie). Full contract: `DETAILS.md` § Undoing a job.
- **Rollback FAILS SAFE** (data-safety-critical): recheck each item against its snapshot AND its restore target; drift
  / unverifiable / occupied target ⇒ SKIP (→ `partially_rolled_back`), never operate; a restore-move never overwrites
  (pinned `Skip`, bar a case-only self-collision). `rolling_back` guards double-rollback + the retention race. A skip
  stores WHICH reason; NULL = not recorded, never a default. Contract: `DETAILS.md` § Rollback.
- **Search spans every `row_role`; retention prunes whole ops only.** Name search matches `source_name_folded` across
  `rollback_unit` AND `search_only` rows, so a leaf hits inside a trashed folder; a `top_level_only` op is a queryable known gap,
  not a false miss. Retention never prunes a `rolling_back` op or its target.

Depth (ladder, schema, query/search, retention, rollback, dev bin): `DETAILS.md`.
