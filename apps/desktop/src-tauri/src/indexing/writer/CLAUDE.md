# Writer (single writer thread per DB)

Every write to a volume's index DB goes through one dedicated `std::thread`. This area is the CANONICAL owner of
honest sizes (coverage + freshness), the `dir_stats` ledger, coverage epochs, in-memory accumulation, delta
propagation, and the search-feeding generation bump. Other areas point here.

## Module map

- **mod.rs**: the `WriteMessage` protocol, the `IndexWriter` handle, `writer_loop` + `process_message` dispatch,
  `AccumulatorMaps`, `WRITER_GENERATION` + `MutationTracker`, the SQLite busy handler, `ProbeStats` heartbeat.
- **entries.rs**: entry handlers (insert/upsert/move/delete/truncate). **delta.rs**: `propagate_delta_by_id` +
  `propagate_min_subtree_epoch` + `propagate_recursive_has_symlinks`. **aggregation.rs**: `Compute*`/`Backfill`
  delegation to `../aggregator/` + `SkipSeverity`. **repair.rs**: `repair_dir_stats_upward`. **deferred_repair.rs**:
  the `DeferredRepairs` queue. **maintenance.rs**: incremental vacuum + WAL checkpoint. **wait_probe.rs**: writer-queue
  wait accounting (read by reconcile).

## Must-knows (all hold PER volume id)

- **Bounded `sync_channel` (20K); a full channel blocks the sender.** The writer owns the WRITE connection; reads go
  through `ReadPool`, never here. Priority: `UpdateDirStats` before `InsertEntries`.
- **The writer owns the shared `Arc<AtomicI64>` ID counter; never allocate from `MAX(id)`** (uncommitted inserts sit in
  the channel, so a read sees a stale max and double-assigns). `TruncateData` resets it to 2. A drifted counter
  SELF-HEALS on a PK conflict: extended `1555` → `fetch_max` from the table + one retry (`entries.rs`). ❌ Never extend
  the heal to UNIQUE `2067`: a retried `(parent_id, name_folded)` conflict IS the duplicate row the constraint blocks.
- **A fatal storage error STOPS + FAILS the index, never retries** (one incident logged 12,700 warnings in 8 min). The
  writer is the detector (`../metadata.rs`-independent typed classification via `IndexFailureSignal`, never a message
  substring); `BUSY`/`LOCKED` stay retried. The Failed lifecycle representation lives in `../lifecycle/DETAILS.md`.
- **`dir_stats` ledger, four hard rules:** (1) never clamp a negative delta (it's drift — escalate to
  `repair_dir_stats_upward`, never `.max(0)`; floored 1.21 GB to "0 bytes" once); (2) a failed `dir_stats` read OR write
  is drift, not a no-op — queue the id to `deferred_repair.rs`, never warn-and-continue, never read `Err` as "no row";
  (3) structural rewrites repair ancestors ON the writer, never off-writer read-then-credit; (4) suppress propagation
  ONLY inside `BulkReconcileGuard` (it durably `MarkLedgerUnpaid` / `PayLedgerIfUnpaid`) — bare
  `SetDeltaPropagation(false)` left 249 dirs claiming exact sizes.
- **Coverage epochs:** `propagate_delta_by_id` carries `min_subtree_epoch` through UNCHANGED on a pure size/count delta
  (resetting it flips exact→"≥" on every file write); `propagate_min_subtree_epoch` fires on TREE-SHAPE changes only.
  Marks (`MarkDirsListed`) land BEFORE the aggregate. ❌ Never write `listed_epoch = 0` for a dir we listed but skipped.
- **Full-aggregate source is sender-declared (`source: Maps|Sql`), never sniffed.** `Maps` (the in-memory accumulator)
  comes ONLY from a fresh full scan; every other flow sends `Sql`. The subtree handler must NOT clear the accumulator.
- **Partial aggregation borrows the maps READ-ONLY**, no-ops on empty maps with NO SQL fallback (load-bearing: a late
  partial pass must see empty maps and no-op), never bumps the generation, writes depth ≤ 3 + hot dirs.
- **`WRITER_GENERATION` bumps only for the search-feeding (root) writer** (`MutationTracker`), so an SMB/MTP write never
  thrashes the root search reload. Meta-only messages (`MarkDirsListed`, `UpdateMeta`, `BumpCurrentEpoch`) never bump it.
- **Tests must never assert on process-global state (`WRITER_GENERATION`, `PENDING_SIZES`) across a before/after
  window**: every `IndexWriter::spawn()` is a ROOT writer that bumps the generation and clears the root tracker, so
  under `cargo test` a global read flakes and can poison a shared test mutex. Use a per-writer probe
  (`global_generation_bumps`) or a per-volume `IndexInstance` (`TestInstanceGuard`). DETAILS § "Test isolation".
- **`flush_blocking` ≠ settled**: it replies from inside the handler, before the end-of-iteration hourglass clear and
  repair drain. Wait on `idle_epoch()`; ❌ never move the reply.

Everything above in depth, plus the caught-up point, partial aggregation, the heal, and maintenance: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
