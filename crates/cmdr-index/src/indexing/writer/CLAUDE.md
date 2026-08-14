# Writer (single writer thread per DB)

Every write to a volume's index DB goes through one dedicated `std::thread`. This area CANONICALLY owns honest sizes
(coverage + freshness), the `dir_stats` ledger, coverage epochs, in-memory accumulation, delta propagation, and the
search-generation bump. Other areas point here.

## Module map

- **mod.rs**: `WriteMessage` protocol, `IndexWriter` handle, `writer_loop` + `process_message` dispatch,
  `AccumulatorMaps`, `WRITER_GENERATION` + `MutationTracker`, SQLite busy handler.
- **batch.rs**: the implicit transaction around queued mutations; each message's `BatchRole`.
- **abandoned_retry.rs**: when to reopen ground a walk gave up on (`ClearAbandonedIfDue`).
- **entries.rs** (insert/upsert/move/delete/truncate), **delta.rs** (`propagate_delta_by_id` /
  `propagate_min_subtree_epoch` / `propagate_recursive_has_symlinks`), **aggregation.rs** (`Compute*`/`Backfill` →
  `../aggregator/`), **repair.rs** (`repair_dir_stats_upward`), **deferred_repair.rs**, **maintenance.rs** (vacuum, WAL
  checkpoint), **probe_stats.rs** (the stall heartbeat, scraped by `scripts/churn-baseline`), **wait_probe.rs**
  (queue-wait accounting).

## Must-knows (all hold PER volume id)

- **Bounded `sync_channel` (20K); a full channel blocks the sender.** The writer owns the WRITE connection; reads go
  through `ReadPool`, never here. Priority: `UpdateDirStats` before `InsertEntries`.
- **The writer owns the shared `Arc<AtomicI64>` ID counter; ❌ never allocate from `MAX(id)`** (uncommitted inserts sit
  in the channel, so a read double-assigns). `TruncateData` resets it to 2. A drifted counter SELF-HEALS on a PK
  conflict (extended `1555` → `fetch_max` + retry, `entries.rs`). ❌ Never extend the heal to UNIQUE `2067`: that retry
  IS the duplicate row the constraint blocks.
- **A fatal storage error STOPS + FAILS the index, ❌ never retries** (one incident: 12,700 warnings in 8 min). Typed
  via `IndexFailureSignal`, ❌ never a message substring; `BUSY`/`LOCKED` stay retried. Failed lifecycle:
  `../lifecycle/DETAILS.md`.
- **`dir_stats` ledger, four hard rules:** (1) ❌ never clamp a negative delta — it's drift, escalate to
  `repair_dir_stats_upward` (`.max(0)` floored 1.21 GB to "0 bytes" once); (2) a failed `dir_stats` read OR write is
  drift too, so queue the id to `deferred_repair.rs`, ❌ never warn-and-continue, ❌ never read `Err` as "no row"; (3)
  structural rewrites repair ancestors ON the writer; (4) suppress propagation ONLY inside `BulkReconcileGuard` (durable
  `MarkLedgerUnpaid`/`PayLedgerIfUnpaid`) — bare `SetDeltaPropagation(false)` left 249 dirs claiming exact sizes.
- **Coverage epochs:** `propagate_delta_by_id` carries `min_subtree_epoch` through UNCHANGED on a pure size/count delta
  (resetting it flips exact→"≥" on every file write); `propagate_min_subtree_epoch` fires on TREE-SHAPE changes only.
  Marks (`MarkDirsListed`) land BEFORE the aggregate. ❌ Never write `listed_epoch = 0` for a dir we listed but skipped.
- **`Abandoned` is the one `unreadable_cause` Cmdr RETRIES** (`abandoned_retry.rs`: a persisted per-volume
  5 min → 1 h → 4 h → 24 h window, armed by the mark). ❌ Never a flat retry, ❌ never clear `Denied`/`Declined`.
- **Full-aggregate source is sender-declared (`source: Maps|Sql`), ❌ never sniffed.** `Maps` comes ONLY from a fresh
  full scan; every other flow sends `Sql`. ❌ The subtree handler must not clear the accumulator.
- **Partial aggregation borrows the maps READ-ONLY**, no-ops on empty maps with NO SQL fallback (load-bearing: a late
  pass must no-op), ❌ never bumps the generation, writes depth ≤ 3 + hot dirs.
- **`WRITER_GENERATION` bumps only for the search-feeding (root) writer** (`MutationTracker`), so an SMB/MTP write never
  thrashes the root search reload. Meta-only messages never bump it.
- **❌ No test may assert on process-global state (`WRITER_GENERATION`, the root tracker) across a before/after
  window**: every `IndexWriter::spawn()` is a ROOT writer that bumps and clears it, so a global read flakes under
  `cargo test`. Use `global_generation_bumps` or a `TestInstanceGuard`. DETAILS § "Test isolation".
- **Live mutations batch, they don't autocommit** (`batch.rs`): queued work coalesces into one `BEGIN IMMEDIATE`, closed
  on an empty queue. Every new `WriteMessage` needs a `BatchRole`; a reply, emit, or pragma must be `Barrier`.
- **`flush_blocking` ≠ settled**: it replies from inside the handler, before the end-of-iteration hourglass clear and
  repair drain. Wait on `idle_epoch()`; ❌ never move the reply.

Everything above in depth, plus the caught-up point, the heal, and maintenance: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
