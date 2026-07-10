# Operation log subsystem

The durable, cross-volume journal of every file mutation: the foundation for rollback, indexed name search, retention,
and a future undo. **The app's first durable DB** (`operation-log.db` in the app data dir, Time Machine-backed) — every
other on-disk store here is a disposable cache. Full design + rationale: [DETAILS.md](DETAILS.md). Plan:
[`docs/specs/operation-log-plan.md`](../../../../../docs/specs/operation-log-plan.md).

**Shipped: the durable store (M1), capture at the chokepoint (M2), and the rollback engine (M3).** Search/retention
enforcement (M4), MCP (M5), and the UI (M6/M7) build on this.

## Module map

- `store/` — `operation-log.db`: connection factory (`connection.rs`), the migration ladder (`migrations.rs`), schema,
  `intern_dir`, `fold_name`, and read functions. `OperationLogStore` owns the schema lifecycle.
- `writer.rs` — the ONE writer thread: `open_operation` / `record_items` / `finalize_operation` / `set_rollback_state`
  / `set_item_outcomes` / `prune`, batched inserts, retention mechanism.
- `rollback.rs` — the M3 engine (inverse-per-item + recheck, the `rolling_back` state machine, startup reconcile);
  managed-op spawn glue is `write_operations/rollback.rs::dispatch_rollback`. `capture.rs` (M2) feeds the writer.
- `types.rs` — the typed vocabulary (`OpKind`, `RollbackState`, …) and their stable DB tokens.
- `mod.rs::start` — opens the DB, runs the rollback reconcile, and manages the `OperationLogWriter` at app setup.

## Must-knows

- **This DB is DURABLE and MIGRATES; it never delete-and-recreates on a version bump.** The migration ladder
  (`store/migrations.rs`) is the first in the codebase and the template future durable DBs follow: append a `Migration`
  with the next version; NEVER edit or renumber a shipped step (users' DBs already ran it). A downgrade is refused, never
  wiped. Delete-and-recreate is reserved for a genuinely unparseable file (typed sqlite error code, not a string match).
- **NO `platform_case` collation** (unlike the index/importance stores) — deliberate (D2): the store precomputes a
  `name_folded` column via `fold_name` (Unicode-lowercase + NFC) and queries plain b-tree equality, so the file stays
  openable in any `sqlite3` browser. Don't add a collation to "match" the other stores.
- **One writer thread, one cross-volume DB, NO per-volume registry** (divergence from importance, D1). One
  `OperationLogWriter` in managed state. All writes cross the bounded channel: `record_items` BLOCKS under backpressure
  (lossless), never drops on fullness; a DB *error* on one row logs and drops THAT row without failing the op. That's
  why `finalize_operation` returns per-`row_role` durable counts — the M2 completeness check compares them to items
  issued and degrades a `rollback_unit` gap to `not_rollbackable`, a `search_only` gap to `top_level_only` (never a
  silent under-reverse or false coverage claim).
- **Classification is typed end to end, never a string/substring branch** (`no-string-matching`): every `kind`,
  `initiator`, status, `row_role`, `outcome`, etc. is a `types.rs` enum with a stable token; the token↔enum mapping lives
  ONLY there. Renaming a token is a schema change (needs a migration); renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** Rollback eligibility (D3) and the net-new/
  subkind reasoning are the M2 capture layer's job (`capture.rs` — `compute_eligibility` + `apply_completeness`),
  upstream of the writer — keep business logic out of `writer.rs`.
- **Capture is a process-global journal reached by `op_id`, NOT threaded through the pipeline** (a recorded deviation
  from D4's `OperationObservers`; D4's hard rule — never extend `OperationEventSink` — still holds). Install via
  `set_journal`; the write pipeline calls the `journal_open` / `journal_record_items` / `journal_note_coverage` /
  `journal_finalize` free functions by `op_id`, mirroring `update_operation_status`. Full rationale + per-kind record
  points: [DETAILS.md](DETAILS.md) § Capture.
- **Rollback FAILS SAFE** (`rollback.rs`, data-safety-critical): recheck each item against its snapshot AND its restore
  target; drift / unverifiable (absent MTP-SMB mtime) / occupied target ⇒ SKIP (→ `partially_rolled_back`), never
  operate; a restore-move never overwrites (pinned `Skip`, bar a case-only self-collision). `rolling_back` is the
  double-rollback + retention-race guard (set at spawn, reset on sync spawn failure, reconciled on crash). Full contract:
  [DETAILS.md](DETAILS.md) § Rollback.
- **Interned dirs never grow unbounded**: retention GCs dirs down to the referenced-plus-ancestors closure (a referenced
  dir's whole parent chain survives). Prune whole operations only; null dangling `rolls_back_op_id`; skip `rolling_back`
  ops. Vacuum runs in bounded slices between batches so it never starves capture.

Depth (D1/D2 rationale, the ladder template, schema, retention, the dev bin): [DETAILS.md](DETAILS.md).
