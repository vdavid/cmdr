# Operation log subsystem

The durable, cross-volume journal of every file mutation: the base for rollback, indexed name search, retention, and a
future undo. **The app's first durable DB** (`operation-log.db` in the app data dir, Time Machine-backed) — every other
on-disk store here is a disposable cache. Full design + rationale: [DETAILS.md](DETAILS.md). Plan:
[`docs/specs/operation-log-plan.md`](../../../../../docs/specs/operation-log-plan.md).

**Shipped end to end: the durable store, capture, the rollback engine, the read/search API and retention, the MCP tools
(in `mcp/executor/operation_log.rs`), the retention settings and Debug panel, and the alpha "Operation log"
dialog.** The UI is frontend-only over the read API: Debug panel in `routes/debug/DebugOperationLogPanel.svelte`,
alpha dialog in `src/lib/operation-log/` (see [DETAILS.md](DETAILS.md) § Alpha UI).

## Module map

- `store/` — the DB: connection factory, migration ladder (`migrations.rs`), schema, `intern_dir`, `fold_name`, and
  low-level reads. `OperationLogStore` owns the schema lifecycle.
- `writer.rs` — the ONE writer thread: `open_operation` / `record_items` / `finalize_operation` / `set_rollback_state` /
  `set_item_outcomes` / `prune`; batched inserts; the retention mechanism (age + size prune, dir GC, vacuum).
- `query.rs` — the read side (index-served name search, paged `recent_operations` / `get_operation`); `retention.rs`
  runs `prune` on a startup + periodic timer; IPC in `commands/operation_log.rs`.
- `rollback.rs` — the rollback engine (inverse-per-item + recheck, `rolling_back` state machine, startup reconcile); spawn glue
  in `write_operations/rollback.rs`. `capture.rs` (the capture layer) feeds the writer. `types.rs` — the typed tokens.
- `mod.rs::start` — opens the DB, reconciles rollback, spawns retention, manages the writer.

## Must-knows

- **DURABLE and MIGRATES; never delete-and-recreates on a version bump.** The ladder (`store/migrations.rs`) is the first
  here and the template future durable DBs follow: append a `Migration`; NEVER edit or renumber a shipped step. A
  downgrade is refused, never wiped; delete-and-recreate is only for a genuinely unparseable file (typed sqlite code, not
  a string).
- **NO `platform_case` collation** (D2): the store precomputes a `name_folded` column (Unicode-lowercase + NFC) and
  queries plain b-tree equality, keeping the file openable in any `sqlite3` browser. Don't add a collation to match the
  other stores.
- **One writer thread, one cross-volume DB, NO per-volume registry** (D1). `record_items` BLOCKS under backpressure
  (lossless), never drops on fullness; a DB *error* on one row logs and drops THAT row without failing the op. So
  `finalize_operation` returns per-`row_role` durable counts, and the capture completeness check degrades a `rollback_unit`
  gap to `not_rollbackable` and a `search_only` gap to `top_level_only` — never a silent under-reverse or false coverage
  claim.
- **Classification is typed end to end** (`no-string-matching`): every `kind`, `initiator`, status, `row_role`,
  `outcome` is a `types.rs` enum with a stable token; the mapping lives ONLY there. Renaming a token is a schema change;
  renaming a variant is free.
- **The writer stores terminal state; it does NOT compute eligibility.** Rollback eligibility (D3) + net-new/subkind
  reasoning are the capture layer's job (`capture.rs`) — keep business logic out of `writer.rs`.
- **Capture is a process-global journal reached by `op_id`, NOT threaded through the pipeline** (recorded deviation from
  D4; its hard rule — never extend `OperationEventSink` — still holds). Install via `set_journal`; the pipeline calls the
  `journal_*` free functions by `op_id`. Rationale + record points: [DETAILS.md](DETAILS.md) § Capture.
- **Rollback FAILS SAFE** (data-safety-critical): recheck each item against its snapshot AND its restore target; drift /
  unverifiable / occupied target ⇒ SKIP (→ `partially_rolled_back`), never operate; a restore-move never overwrites
  (pinned `Skip`, bar a case-only self-collision). `rolling_back` is the double-rollback + retention-race guard. Full
  contract: [DETAILS.md](DETAILS.md) § Rollback.
- **Search spans every `row_role`; retention prunes whole ops only.** Name search matches the indexed
  `source_name_folded` across `rollback_unit` AND `search_only` rows (a leaf hits inside a trashed folder); a
  `top_level_only` op is a queryable known gap, not a false miss. Retention prunes whole ops by age + size, GCs dirs to
  the referenced-plus-ancestors closure, and NEVER prunes an op in `rolling_back` or its target.

Depth (ladder template, schema, query/search, retention, rollback, dev bin): [DETAILS.md](DETAILS.md).
