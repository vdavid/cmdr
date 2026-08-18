# Writer (single writer thread per DB)

Every write to a volume's index DB goes through one dedicated `std::thread`. This area CANONICALLY owns honest sizes
(coverage + freshness), the `dir_stats` ledger, coverage epochs, in-memory accumulation, delta propagation, and the
search-generation bump. Other areas point here.

`mod.rs` the `WriteMessage` protocol + `IndexWriter` handle + `writer_loop`; `batch.rs` the implicit transaction;
`entries.rs`, `delta.rs`, `aggregation.rs`, `repair.rs`, `deferred_repair.rs`, `pending_rollups.rs`,
`abandoned_retry.rs`, `maintenance.rs`, `probe_stats.rs`, `wait_probe.rs`, one job each.

## Must-knows (all hold PER volume id)

- **Bounded `sync_channel`; a full channel blocks the sender.** The writer owns the WRITE connection; reads go through
  `ReadPool`, ❌ never here. Priority: `UpdateDirStats` before `InsertEntries`.
- **The writer owns the shared `Arc<AtomicI64>` ID counter; ❌ never allocate from `MAX(id)`** (uncommitted inserts sit
  in the channel, so a read double-assigns). A drifted counter SELF-HEALS on a PK conflict (extended `1555` →
  `fetch_max` + retry). ❌ Never extend the heal to UNIQUE `2067`: that retry IS the duplicate row the constraint
  blocks.
- **A fatal storage error STOPS + FAILS the index, ❌ never retries** (one incident: 12,700 warnings in 8 min). Typed
  via `IndexFailureSignal`, ❌ never a message substring; `BUSY`/`LOCKED` stay retried.
- **`dir_stats` ledger, four hard rules:** (1) ❌ never clamp a negative delta — it's drift, escalate to
  `repair_dir_stats_upward`; (2) a failed `dir_stats` read OR write is drift too, so queue the id to
  `deferred_repair.rs`, ❌ never warn-and-continue, ❌ never read `Err` as "no row"; (3) structural rewrites repair
  ancestors ON the writer; (4) suppress propagation ONLY inside `BulkReconcileGuard` (durable
  `MarkLedgerUnpaid`/`PayLedgerIfUnpaid`), ❌ never a bare `SetDeltaPropagation(false)`.
- **Coverage epochs:** `propagate_delta_by_id` carries `min_subtree_epoch` through UNCHANGED on a pure size/count delta
  (resetting it flips exact→"≥" on every file write); `propagate_min_subtree_epoch` fires on TREE-SHAPE changes only.
  Marks (`MarkDirsListed`) land BEFORE the aggregate. ❌ Never write `listed_epoch = 0` for a dir we listed but skipped.
- **`Abandoned` is the one `unreadable_cause` Cmdr RETRIES** (`abandoned_retry.rs`, a persisted per-volume backoff armed
  by the mark). ❌ Never a flat retry, ❌ never clear `Denied`/`Declined`.
- **Full-aggregate source is sender-declared (`source: Maps|Sql`), ❌ never sniffed.** `Maps` comes ONLY from a fresh
  full scan; every other flow sends `Sql`. ❌ The subtree handler must not clear the accumulator.
- **Partial aggregation borrows the maps READ-ONLY** and no-ops on empty maps with NO SQL fallback (load-bearing: a late
  pass must no-op); ❌ it never bumps the generation.
- **`WRITER_GENERATION` bumps only for the search-feeding (root) writer** (`MutationTracker`), so an SMB/MTP write never
  thrashes the root search reload. Meta-only messages never bump it.
- **❌ No test may assert on process-global state (`WRITER_GENERATION`, the root tracker) across a before/after
  window**: every `IndexWriter::spawn()` is a ROOT writer that bumps and clears it, so a global read flakes under
  `cargo test`. Use `global_generation_bumps` or a `TestInstanceGuard`.
- **Live mutations batch, they don't autocommit** (`batch.rs`): queued work coalesces into one `BEGIN IMMEDIATE`, closed
  on an empty queue. Every new `WriteMessage` needs a `BatchRole`; a reply, emit, or pragma must be `Barrier`.
- **`flush_blocking` ≠ settled**: it replies from inside the handler, before the end-of-iteration hourglass clear and
  `settle_the_ledger`. Wait on `idle_epoch()`; ❌ never move the reply.
- **The subtree handler QUEUES its ancestor roll-up** (`pending_rollups.rs`), drained at the caught-up point, because
  repairing per message made a wide directory `O(width²)`. Safe because a repair recomputes from committed children, so
  it can't double-count and can't be reordered wrong. A quit drains it; a crash mid-burst is a rescan we accept.

Everything above in depth, plus the caught-up point, the heal, test isolation, and maintenance: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
