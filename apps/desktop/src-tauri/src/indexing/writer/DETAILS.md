# Writer details

Read this before any non-trivial work in `writer/`: editing, planning, reorganizing, or advising. Must-know guardrails
are in [CLAUDE.md](CLAUDE.md). This area is the canonical home for the mechanisms below; other indexing areas link here
rather than restating them.

Points outward: the registry / phase machine / freshness / the Failed representation live in
[`../lifecycle/DETAILS.md`](../lifecycle/DETAILS.md); the SQLite schema + `name_folded` + `platform_case` collation in
[`../store/DETAILS.md`](../store/DETAILS.md); the bottom-up compute math in
[`../aggregator/DETAILS.md`](../aggregator/DETAILS.md); who sends which aggregate (the reconcile finish, the bulk guard)
in [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md); the scanner's mark-accumulate in
[`../scanner/DETAILS.md`](../scanner/DETAILS.md); path→volume routing in [`../paths/DETAILS.md`](../paths/DETAILS.md).

## Single-writer architecture

All writes go through a dedicated `std::thread` via a bounded `sync_channel` (20K capacity). When the channel is full,
senders block (backpressure). The writer thread owns the write connection and processes messages in order, prioritizing
`UpdateDirStats` over `InsertEntries`. Reads happen on separate WAL connections (any thread) via a `ReadPool`, so
enrichment/verification never contend on the write connection or the lifecycle mutex.

`WriteMessage` has integer-keyed variants (`InsertEntriesV2`, `UpsertEntryV2`, `MoveEntryV2`, `DeleteEntryById`,
`DeleteSubtreeById`, `PropagateDeltaById`, `ComputeAllAggregates`, `ComputeSubtreeAggregates`,
`ComputePartialAggregates`, `BackfillMissingDirStats`, `TruncateData`, `MarkDirsListed`, `PropagateMinSubtreeEpoch`,
`BumpCurrentEpoch`, `SetDeltaPropagation`, `MarkLedgerUnpaid`/`PayLedgerIfUnpaid`, `ArmLedgerHealLatch`,
`IncrementalVacuum`/`WalCheckpoint`, `EmitDirUpdated`, `Flush`) plus path-keyed backward-compat variants. `Flush` +
the async `flush()` let callers wait for all prior writes to commit.

**Rationale — single writer, not connection pooling.** SQLite's write concurrency is limited by its single-writer
design. Rather than fight it with `BUSY_TIMEOUT` + retries, one thread owns the write connection and eliminates
contention entirely.

## The shared ID counter and its self-heal

All ID allocation goes through an `Arc<AtomicI64>` owned by `IndexWriter`, seeded from `IndexStore::get_next_id` at
spawn. The local walker's `InsertVisitor` increments it via `fetch_add`, the network `ScanContext` via `alloc_id()`,
and the writer's `UpsertEntryV2` insert path does the same (passing the id to `insert_entry_v2_with_id`). `TruncateData`
resets it to 2 (id 1 is the ROOT sentinel).

**Never fall back to `MAX(id)` from a read connection.** The writer can hold uncommitted inserts in its channel, so a
read sees a stale value and the scanner double-assigns IDs. `IndexWriter` exposes `db_path()`; the local scanner opens a
temporary connection only to seed/read the epoch and resolve the scan root, never for ID allocation.

**`INSERT OR IGNORE`, not `OR REPLACE`.** `insert_entries_v2_batch` uses `INSERT OR IGNORE` and returns a `Vec<bool>`
parallel to the input. `OR REPLACE` would delete the old row and insert a new id, orphaning all children; plain
`INSERT` would roll back the whole ~2000-entry batch via the wrapping savepoint on any conflict (catastrophic if one
filesystem oddity — a case-folding twin, an NFC/NFD cross-OS-sync duplicate — takes out 1999 unrelated rows). `OR
IGNORE` skips just the conflicting row. `handle_insert_entries_v2` filters `entries` by the returned flags BEFORE
calling `accumulator.accumulate`, so the in-memory aggregation state never claims bytes that lost the OR-IGNORE — the
contract "in-memory state never claims more than the DB has". Conflicts log at WARN with sample names. The table has two
unique constraints: PK on `id`, and `UNIQUE (parent_id, name_folded)` (v12); the savepoint still wraps the batch so
non-constraint errors (disk full) roll back cleanly.

**Decision: a PRIMARY KEY conflict on an upsert insert resyncs the counter and retries once.** The counter can fall
behind the table's real `MAX(id)` (a foreign writer on the same DB — what the lock-first `start_indexing` now prevents
— or a crash between allocation and commit). Before healing, `upsert_insert_new` just `signal.note`d the
`SQLITE_CONSTRAINT_PRIMARYKEY` and moved on, so the file was dropped from the index forever (until a verifier scan
noticed) and, because the counter stayed behind, every following live insert collided identically: one incident
produced ~9,600 warnings in seconds. `insert_with_allocated_id` (`entries.rs`) now `fetch_max`es the counter from
`IndexStore::get_next_id(conn)`, logs ONE warn naming the old and new counter values, allocates a fresh id, and retries;
a second failure falls through to `signal.note`. The resync puts the counter past `MAX(id)`, so the "once per resync"
log cadence is structural, not rate-limited.

**Only 1555 heals, never 2067.** `IndexStoreError::is_primary_key_conflict()` (`../store/`) matches the EXTENDED code
`SQLITE_CONSTRAINT_PRIMARYKEY`; both it and the `(parent_id, name_folded)` conflict `SQLITE_CONSTRAINT_UNIQUE` (2067)
share the primary `ErrorCode::ConstraintViolation`, so the extended code is the only discriminator (never the message
string — `no-string-matching`). A UNIQUE conflict means the NAME is already in the table (a real duplicate, a
case-folding twin, a writer that raced between `resolve_component` and the insert); retrying under a fresh id would
insert exactly the duplicate row the constraint exists to block (the 1.83 TB ghost size on `..` of a 994 GB volume).

## Fatal storage failure — the writer is the detector

A real incident: the local index DB began returning `SQLITE_IOERR` on every read and write mid-scan. The writer thread
and the reconciler each `log::warn!`-and-continued and retried FOREVER: 12,700+ identical warnings over 8 min, ~190%
CPU, a frozen webview, "Find files" stuck at 0%. The fix makes a dead DB fail loudly, stop cleanly, and show an honest
state.

**Classification (typed, never the message string — `no-string-matching`).** `IndexStoreError::sqlite_code()` extracts
`(rusqlite::ErrorCode, extended_code)`; `is_fatal_storage_error()` is `true` for the storage-death classes
(`SQLITE_IOERR*`, `SQLITE_CORRUPT`, `SQLITE_CANTOPEN`, `SQLITE_FULL`, `SQLITE_READONLY`, `SQLITE_NOTADB`) where every
later op fails identically. Transient contention (`SQLITE_BUSY` / `SQLITE_LOCKED`) is deliberately NOT fatal (the busy
handler backs those off). `as_index_failure()` builds the typed `IndexFailure { code, extended_code }`.

**Detection lives here (`../failure.rs::IndexFailureSignal`).** A one-shot per-volume `Arc<IndexFailureSignal>` created
in `IndexWriter::spawn_for`, cloned into the writer thread, exposed via `IndexWriter::failure_signal()`. Every
write-handler DB-error site calls `signal.note(&err, ctx)` instead of a bare `log::warn!`: a non-fatal error logs at
warn and returns `false`; the FIRST fatal error CAS-trips the signal, records the reason, logs ONCE at error level, and
wakes the supervisor (later fatal errors are suppressed — that's what stops the flood). `writer_loop` checks
`signal.is_tripped()` after each message and returns, so the dead-DB writer thread exits instead of spinning. Pinned by
`tests::a_fatal_storage_error_stops_the_writer_and_trips_the_signal` (a `query_only` connection makes writes fail
`SQLITE_READONLY`; the loop must terminate on its own with the sender still alive). The supervisor, `IndexPhase::Failed`,
`Freshness::Failed`, and recovery-by-rebuild live in [`../lifecycle/DETAILS.md`](../lifecycle/DETAILS.md).

## Honest sizes (coverage + freshness)

The full design (the four write paths, the read-side derivation, the decisions) is captured here; the FE display table
lives in `src/lib/indexing/DETAILS.md` § "Honest size rendering". The data model splits the overloaded "0 bytes" into
two orthogonal facts, one stored integer each (columns defined in [`../store/DETAILS.md`](../store/DETAILS.md) §
schema):

- **`entries.listed_epoch`** (per dir): the epoch at which this dir's direct contents were last successfully listed.
  `0` = never listed. This distinguishes a genuinely empty `0 bytes` folder (`listed_epoch > 0`, no children) from an
  unknown `—` folder (`listed_epoch == 0`).
- **`dir_stats.min_subtree_epoch`** (rolled up by the aggregator): `min` over the dir's own `listed_epoch` and every
  child dir's `min_subtree_epoch`, with `0` absorbing. `> 0` ⇒ subtree fully covered (size exact); `0` ⇒ some
  descendant unlisted (size a lower bound).
- **`meta.current_epoch`** (per volume): bumped only on a continuity break; a scan/reconcile STAMPS listed dirs with it
  but never bumps it. Read at scan start (seeded to `"1"` if absent), so a first scan stamps epoch 1.

Helpers in `../store/`: `read_current_epoch` (absent/unparseable ⇒ 1), `seed_current_epoch`, `bump_current_epoch`,
`mark_dirs_listed(ids, epoch)`.

**The complete `current_epoch` bump-site list (every continuity break, nowhere else).** A bump means "I can no longer
vouch that the stored sizes are current". All bumps route through the writer (`WriteMessage::BumpCurrentEpoch`, which
does NOT bump the writer generation — meta-only) EXCEPT the launch one (no writer spawned yet):

- **The two scan-start funnels** — `start_scan` (local) and `start_volume_scan` (SMB/MTP). Every full (re)scan funnels
  through one regardless of trigger, so bumping HERE covers them all without enumerating `RescanReason`. The bump rides
  the same flush as the existing `DeleteMeta`/`TruncateData`, so it's COMMITTED before the scan thread reads
  `current_epoch` on its own connection (else the walk stamps the stale epoch). The first-ever scan bumps 1→2 (benign).
- **The non-rescanning continuity breaks** — `on_smb_watcher_died`, `on_smb_overflow`, `on_mtp_device_disconnected`
  each call `state::bump_current_epoch_for(vid)` alongside the freshness event. The disconnect completion branch in
  `start_volume_scan` bumps via its CAPTURED `writer` handle directly (a registry lookup would no-op while the volume is
  still `Initializing`).
- **Launch-loading-Stale** — `start_indexing_for`. `initial_freshness_on_launch` is pure (no DB handle) and cannot
  bump, so the call site bumps when it loads Stale (a non-journaled SMB/MTP index with a completed prior scan) on a
  short-lived write connection. A journaled local index loads Fresh and does NOT bump (continuity self-heals via
  FSEvents replay).

A failed bump is non-fatal: `read_current_epoch` degrades a missing/unparseable epoch to "all current" (1), so the
worst case is reading Fresh-looking until the next break — never a crash or a falsely-stale lie.

**The two freshness layers stay consistent.** The per-volume `Freshness` enum (badge summary) and the epoch model
(per-dir truth) are kept consistent: `root.min_subtree_epoch == current_epoch ⇒ Fresh` (modulo `Scanning`), `<` ⇒
Stale. This holds BY CONSTRUCTION because the same events drive both — a clean `ScanCompleted` leaves the root stamped
at `current_epoch` AND sets `Freshness::Fresh`; every continuity break bumps `current_epoch` AND fires
`WatcherDied`/`OverflowUnrecoverable` ⇒ `Freshness::Stale`. Pinned by
`tests::root_coverage_epoch_tracks_current_epoch_across_a_continuity_break` (data layer) and
`state::tests::disconnect_keeps_instance_stale_user_cancel_resets_to_gray` (enum layer).

**Scanner stamp mechanism (the mark-before-aggregate ordering invariant).** A dir's `entries` row is inserted as part
of its PARENT's `InsertEntriesV2` batch, so a per-dir `MarkDirsListed` could run its `UPDATE` before that row flushed
and match nothing. So both scanners ACCUMULATE the ids of every SUCCESSFULLY-listed dir and emit `MarkDirsListed`
(chunked) ONCE after the final batch flush and BEFORE the final aggregate. On the single in-order writer this guarantees
the marks land before aggregation reads `listed_epoch` (a mark queued behind the aggregate would leave a dir at epoch 0
→ the whole subtree rolls to `min_subtree_epoch = 0` → a cleanly-scanned volume renders incomplete forever).
`MarkDirsListed` does NOT bump the writer generation. The two scanners' accumulate rules are in
[`../scanner/DETAILS.md`](../scanner/DETAILS.md) (local walker) and the network scanner docs; the aggregator's rollup
math is in [`../aggregator/DETAILS.md`](../aggregator/DETAILS.md).

**Live-path discipline (the path local lives on).** After a scan the local index spends ~all its life in live mode, so
coverage must stay honest under every live mutation. Three rules, all in `writer/` + `../reconcile/`:

- **Preserve `min_subtree_epoch` on a pure size/count delta.** A file write changes size, not coverage, so
  `propagate_delta_by_id` (`delta.rs`) read-modify-writes the row and carries `min_subtree_epoch` through unchanged
  (alongside `recursive_has_symlinks`). Resetting it here would flip an exact dir to "≥" on every file write — the
  headline lie this milestone prevents (pinned by `delta::tests::propagate_delta_by_id_preserves_min_subtree_epoch`).
  The ONE deliberate `0` is the new-dir zero-init `dir_stats` literal in `handle_upsert_entry_v2` (a live-created dir is
  genuinely unlisted). Any `None`-branch fresh-row construction in `delta.rs` also gets `0`, never a stale default.
- **Propagate coverage on a TREE-SHAPE change, never a size change.** `propagate_min_subtree_epoch(conn, start_id)`
  (`delta.rs`) mirrors `propagate_recursive_has_symlinks` (walk the parent chain, recompute per dir, short-circuit when
  the stored value stabilizes — valid for `min` on both loss AND gain), but the per-dir recompute
  (`store::recompute_min_subtree_epoch`) is self-and-children: the 0-absorbing `min` of the dir's own `listed_epoch`
  AND every child dir's stored `min_subtree_epoch` (the OR-precedent is children-only — the load-bearing difference). It
  fires from: a new live dir (from its `parent_id`, so ancestors drop to `0`); delete / delete-subtree (from the old
  `parent_id` — a removed incomplete child may RAISE coverage); and a cross-parent `MoveEntryV2` (BOTH old and new
  parent chains; the moved subtree's own `min` is unchanged — it moved intact). Off-writer callers fire it via the
  `PropagateMinSubtreeEpoch(start_id)` message.
- **The live FILL path (`reconcile_subtree`) MARKS what it lists.** It accumulates the id of every dir it successfully
  lists, then sends `MarkDirsListed` at the current epoch and one `PropagateMinSubtreeEpoch` per listed dir,
  deepest-first. The verifier's `scan_subtree` path stamps via the scanner's accumulate-and-mark, and its
  `ComputeSubtreeAggregates` handler repairs the ancestor chain (`repair_dir_stats_upward`), whose per-level recompute
  lifts ancestor coverage — so neither the verifier nor `run_background_verification` sends `PropagateMinSubtreeEpoch`
  for scanned subtrees (the old off-writer send also double-counted sizes — Leak A). Full detail in
  [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md).

**Read side — derive booleans, never ship raw epochs.** The frontend renders from `{recursive_size, complete, stale}`
only. The backend derives two booleans from `min_subtree_epoch` vs the volume's `current_epoch` (read ONCE per read
pass via `read_current_epoch`, absent ⇒ 1) on both read surfaces — the `FileEntry` enrichment path and the path-keyed
`DirStats` IPC struct. These live in [`../read/DETAILS.md`](../read/DETAILS.md); a write-op denominator rejects lower
bounds (`expected_totals::per_source_contribution` returns `None` for `min_subtree_epoch == 0`).

## The dir_stats ledger

`dir_stats` is an incrementally delta-adjusted ledger: a live mutation writes its own row and walks a signed delta up
the `parent_id` chain, so ancestor sizes stay current without a full recompute. Debits are exact (a subtree delete
computes its totals from `entries` via a recursive CTE), but credits used to leak or race on the rare paths, and the
mismatch was then silently clamped to zero — the incident's confident "0 bytes" for a folder holding 1.21 GB.

**The three principles.**

1. **Deltas stay the fast path** for single-entry live mutations (upsert/delete/move), computed on the writer thread
   against the rows it holds.
2. **Structural rewrites propagate their net effect ON THE WRITER.** Any operation that replaces a subtree wholesale
   (subtree scan, backfill) finishes by repairing the ancestor chain inside the writer's message handling — never via
   off-writer read-then-credit, which raced before (a live upsert inside the just-scanned subtree could be counted
   twice, or a dropped read could skip the credit silently).
3. **Detected drift triggers repair, never a clamp.** A subtraction that would go negative is arithmetic PROOF the
   stored balance is wrong. Repair there, log it, continue with corrected values.

The rule of thumb: **delta when you know exactly what changed; repair when you know something changed but not exactly
what.**

**The repair primitive (`repair.rs::repair_dir_stats_upward`).** The universal escalation: from a start id, recompute
each level from its committed children (one indexed SUM over direct file children + a SUM over direct child dirs' stored
`dir_stats` — O(children) per level, never a recursive CTE), write it, walk to the parent, and STOP as soon as a
level's recompute already equals its stored row (the short-circuit compares ALL fields, so an epoch-only or
symlink-only difference keeps walking — coverage restoration depends on it). Idempotent and order-independent, so it's
safe to fire from every escalation site with no coordination. **Missing-child-row semantics** (they diverge from
`ComputeAllAggregates`, which computes the child first): a child dir with NO `dir_stats` row contributes 0 to
sizes/counts (LEFT JOIN + COALESCE 0), absorbs `min_subtree_epoch` to 0, reads false for symlinks — an honest
under-count the backfill pass then heals and repairs upward (monotone convergence toward truth). A parentless /
`ROOT_ID`-boundary start no-ops gracefully.

**The escalation sites** (where repair replaces a drop or a clamp):

- **`propagate_delta_by_id` (the un-clamped sink).** All eight `.max(0)` clamps are gone. In the `Some` branch, when
  any field would go negative, the walk switches to `repair_dir_stats_upward(current_id)` and logs ONE `warn!`. In the
  `None` branch, a missing row with any negative delta component escalates to repair (a zeroed row would be a fresh
  lie); a pure-positive delta to a missing row still creates it (load-bearing for live-created dirs, epoch 0).
- **`handle_compute_subtree_aggregates`.** After the scoped recompute writes the subtree's rows, it calls
  `repair_dir_stats_upward(parent_of_root)` — one level UP, since the root already agrees with its children. This one
  walk rolls up sizes, counts, `recursive_has_symlinks`, AND `min_subtree_epoch` at once, subsuming the former
  symlink-only ancestor walk and both deleted off-writer `PropagateDeltaById` compensation blocks (leaving those in
  place would double-credit every verified new dir).
- **`backfill_missing_dir_stats`.** After writing the missing rows, it repairs each "missing root" 's parent upward (a
  missing root = a missing dir whose parent is NOT missing), crediting ancestors a delta never walked through.

**Deferred repair: a failed DB operation is drift, not a no-op (`deferred_repair.rs`).** Every ancestor walk
(`propagate_delta_by_id`, `propagate_recursive_has_symlinks`, `propagate_min_subtree_epoch`, `repair_dir_stats_upward`)
can fail mid-chain on a transient SQLite error — the busy handler gives up at attempt 51, and the wild fingerprint is
`propagate_delta_by_id: upsert failed for id=946: database is locked`. Warning and walking away makes that ancestor, and
everything above it, permanently and silently short by the delta. So each failing step hands the id to the writer's
`DeferredRepairs` queue and STOPS; the writer drains it later with `repair_dir_stats_upward`. Two shared rules: a failed
READ is never "no row" (below), and a failed `get_parent_id` queues the current id rather than silently ending the walk.

**A failed READ must never write.** `let existing = get_dir_stats_by_id(..).ok().flatten()` collapsed `Err` into
`None`, and the `None` branch with a positive delta `INSERT OR REPLACE`s a fresh row holding ONLY the delta — a
transient busy read turned into a permanently wrong aggregate. `Err` now writes nothing and queues the chain; `Ok(None)`
keeps its meaning. Same fix in `repair_dir_stats_upward`'s stored-row read, and `recompute_recursive_has_symlinks`
returns `Result<bool>` instead of `unwrap_or(false)`.

**Decision: the drain point is the writer loop's caught-up tick, outside any explicit transaction.** `writer_loop`
drains at the end of an iteration when `queue_depth == 0` and `conn.is_autocommit()` — the same "fully caught up" point
that clears the pending-size hourglass. Why there: with nothing queued, every committed row is final, so a
recompute-from-children sees the whole truth, and whatever contention failed the original write has had its chance to
clear. Not mid-transaction (a bulk sender's batch is only half applied); not inline at the failure (the DB is locked
right now). `TruncateData` clears the queue (its ids name rows that no longer exist).

**Decision: bounded at 1,024 ids, keep the oldest, count the drops.** Ancestor chains overlap heavily, so a real
episode queues a handful; the cap is a memory ceiling for a pathological run. When full, the queue keeps what it has
(each entry is proof of drift we still owe a repair) and counts the newcomer. A drain that fails again re-queues the id
and gives up after 5 passes rather than re-walking a doomed chain; dropped and given-up ids ride one `warn` line. The
backstop for anything given up is the next full aggregate or backfill. Empty→non-empty logs at `warn` (normally
silent), each drain at `debug`.

**The negative-delta warn is drift telemetry.** After the fixes it should be rare; a steadily-firing warn means a NEW
leak to find, not a repair to tune.

**The full-aggregate source contract (`source: Maps|Sql`).** `ComputeAllAggregates` and `ComputePartialAggregates` each
carry an `AggSource`, declared by the SENDER — never sniffed from map-emptiness or `propagate_deltas`. `Maps` (the
writer's in-memory accumulator, populated only by `InsertEntriesV2`) comes ONLY from fresh full-scan completions; an
empty-`Maps` full aggregate falls back to SQL (a consumed-maps sender must not treat "empty" as "all zero"). `Sql`
(recompute from committed rows, ignores the accumulator) comes from the reconcile finish and the one-shot heal. This
closes Leak D: a verification subtree scan's `InsertEntriesV2` batches can leave the maps holding SUBTREE-ONLY data,
and a full aggregate that trusted the maps would roll every out-of-subtree dir up from zero. **The subtree handler must
NOT clear the accumulator** — a clear there opens its own window: a `force_scan` over a never-completed partial takes
the truncate + `Maps` path, and an uncancelled in-flight verification's `ComputeSubtreeAggregates` landing mid-scan
would wipe maps that then partially repopulate. `TruncateData` already clears the maps at the start of every legitimate
`Maps` flow. The interleaved-aggregate test pins this.

**The hourglass for coalesced rescans (the held-roots tier).** A detached `reconcile_subtree` runs for seconds while
the writer queue oscillates empty, so the wholesale queue-drain clear of `PendingSizes` would wipe the "size updating"
mark long before the reconcile finishes. `PendingSizes` (owned by `../read/`) gains a HELD-roots tier: `queue_must_scan_sub_dirs`
holds the root, `is_pending(path)` is true for any transient mark OR any path related to a held root in EITHER
direction, and the writer-drain `clear()` wipes only the TRANSIENT set. On completion: `release(root)` FIRST, then emit
`index-dir-updated` via `WriteMessage::EmitDirUpdated` (release before emit, else the triggered refetch re-reads
`pending = true`).

**The one-shot heal for existing installs (the writer-side latch).** The fixes prevent drift going forward but don't
retro-correct rows nothing touches, so every existing DB heals once, keyed on the meta key
`aggregates_rebuilt_for_ledger`. The key is written from INSIDE `ComputeAllAggregates` on `Ok` ONLY
(`set_heal_key_on_success`), via a writer-side latch armed once at launch when the key is absent
(`WriteMessage::ArmLedgerHealLatch`) and consumed by the first SUCCESSFUL full aggregate. A quit or a FAILED aggregate
leaves the key unset (the latch stays armed, re-heals next launch), and `force_rescan` is covered for free. The DECISION
(arm + maybe enqueue) lives in `resume_or_scan` / `resume_or_scan_network`: `start_scan`/`start_volume_scan` branches
arm ONLY (their own final aggregate consumes the latch), while the journal-REPLAY branch and the SMB/MTP completed-index
branch arm AND enqueue the heal's own `ComputeAllAggregates { source: Sql }`. A DB rebuild is the wrong tool: `entries`
is fine, only `dir_stats` drifted.

**Accepted drift window (not fixed here).** A CANCELLED full local reconcile exits with no marks and no final aggregate
while its walk ran under `SetDeltaPropagation(false)`, so entries it already diffed have no ancestor propagation until
the next COMPLETED rescan. The heal is the existing next-rescan flow. The `BulkReconcileGuard` mechanism (the
`MarkLedgerUnpaid`/`PayLedgerIfUnpaid` debt recording) lives in [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md).

**The whole-effort invariant.** After the writer drains and rescans quiesce, `dir_stats` ≡ recompute-from-`entries`
(carve-out: the cancelled-full-reconcile window). `stress_tests_concurrency.rs`'s mixed-storm oracle pins it using a
fixed-point quiescence loop; `stress_test_helpers::check_db_consistency` is the recompute oracle every ledger test
shares.

## In-memory accumulation

During a full scan the writer accumulates two HashMaps in `AccumulatorMaps` as `InsertEntriesV2` batches arrive:
`direct_stats` (parent_id → file size/count/dir count) and `child_dirs` (parent_id → child dir IDs), plus
`entries_inserted`/`entries_skipped` counters. When `ComputeAllAggregates { source: Maps }` fires, these are passed to
`aggregator::compute_all_aggregates_with_maps()`, skipping the two expensive full-table-scan SQL queries
(`bulk_get_children_stats_by_id`, `bulk_get_child_dir_ids`) that otherwise dominate aggregation (~70%). Maps are cleared
on `TruncateData` and after aggregation. Only a fresh full-scan completion sends `Maps` (its maps are complete); a
`Maps` sender whose maps got consumed falls back to SQL. A per-batch `INSERT OR IGNORE` UNIQUE-conflict skip logs at
DEBUG (3-row sample) and tallies into `entries_skipped`; `handle_compute_all_aggregates` summarizes the scan-wide tally
once via `classify_skip_severity` (none → silent, sparse dedup → DEBUG, racing-writer ratio ≥50 skips and >1% of rows →
WARN), so normal scans log nothing and only the actionable double-write case warns.

## Partial aggregation (writer side)

During a full scan folder sizes otherwise don't exist until the single end-of-scan aggregate, so every listing shows
placeholders for the whole scan (~2.5 min on a 5M-entry volume) and all sizes pop in at once — exactly when a new user
judges the headline feature. Instead, on `ComputePartialAggregates { hot_paths, source }` (sent mid-scan by the
progress reporter), `handle_compute_partial_aggregates` **borrows** the accumulator maps READ-ONLY (no clear, no
mutation, no generation bump), no-ops on empty maps with NO SQL fallback, delegates the math to
`aggregator::compute_partial_aggregates` (full bottom-up over every scanned dir — cheap, pure in-memory), writes a
depth-capped (`PARTIAL_AGG_MAX_DEPTH = 3`) subset plus each resolvable hot-path dir + its direct children, and emits
`index-dir-updated { paths: ["/"] }` when an `AppHandle` is present. Real-volume cost (release, 5.94M entries / 558K
dirs): p95 377 ms/pass, 151–716 rows/pass, indistinguishable from the feature-off baseline.

The don'ts (all load-bearing):

- **Don't consume or mutate the maps** — the final pass needs them for exact totals (`stress_tests_partial_aggregation.rs`
  pins byte-identical final state with and without partial passes).
- **Don't add a SQL fallback to the empty-maps no-op** — the scanner sends `ComputeAllAggregates` before `scan_done` is
  set, so one last partial message can land AFTER the final aggregation; the only thing making that safe is that the
  final pass cleared the maps so the late partial sees empty maps and no-ops (a SQL fallback would overwrite the final
  `dir_stats` with a depth-capped subset).

The `source: Sql` variant (`compute_partial_aggregates_sql`, for reconcile/network paths whose maps stay empty), its
`PARTIAL_AGG_SQL_MAX_SUBTREE = 100_000` stability cap, and the timer that drives sending live in `../events/` (the
progress reporter) and `../aggregator/DETAILS.md` (the scoped-CTE math). The reporter chooses the source at spawn and
maps hot paths to index-relative form before sending; the writer stays a pure message-processor.

## Delta propagation and the search-generation coupling

`UpsertEntryV2`, `DeleteEntryById`, and `DeleteSubtreeById` auto-propagate `dir_stats` deltas on the writer thread.
`propagate_delta_by_id` (`delta.rs`) walks the parent chain via `get_parent_id` lookups. `UpsertEntryV2` auto-propagates
on both insert (full size, +file_count/+dir_count) and update (reads the old entry, propagates only the difference), so
callers never need a separate `PropagateDeltaById` for upserted entries; for new directories it also initializes a
zero-valued `dir_stats` row so enrichment always has a row. `MoveEntryV2 { entry_id, new_parent_id, new_name }` updates
`(parent_id, name, name_folded)` in place, preserving `id` and (for directories) `dir_stats`; a destination
`(parent_id, name_folded)` collision is deleted first (subtree-aware, with delta propagation) so the move never fails
the UNIQUE constraint. Same-parent renames don't change ancestor totals; cross-parent moves subtract from the old
ancestor chain and add to the new one (and recompute the OR-aggregated `recursive_has_symlinks` on both chains).

**`recursive_has_symlinks`** is OR-aggregated bottom-up; adding a symlink flips the flag to `true`, removing the last
one triggers a recompute (`propagate_recursive_has_symlinks`) that walks up until the recomputed value matches the
existing one (monotone, so the chain stabilizes early). Cmdr's recursive size deliberately omits symlinked bytes
(matching `du`/Finder); the flag drives the FE `(i)` explaining the omission.

**`WRITER_GENERATION` and the single-volume-search coupling.** `WRITER_GENERATION: AtomicU64` (init 1) is bumped on
every mutation of the SEARCH-FEEDING (root) writer only, for search-index staleness detection. The full-text/AI search
box is LOCAL-DISK-ONLY in v1: `search/index.rs::load_search_index` loads one in-memory `SearchIndex` off `root`'s DB via
the root global pool, so SMB/MTP DBs are structurally unreachable from search. The one real coupling is the shared
global generation: search captures it at load and reloads the whole index on a mismatch. If EVERY writer bumped it, a
NAS/phone change-notify event would thrash a full root-search reload of an index it doesn't feed. So the bump is gated
in `MutationTracker { counter, feeds_search }` (the single point of policy): it always ticks the per-writer `counter`
(test observable) but bumps `WRITER_GENERATION` only when `feeds_search`. `IndexWriter::spawn_for(.., feeds_search)`
sets it from `kind.feeds_search()` (`true` only for `IndexVolumeKind::Local`); `spawn` defaults to `true`. Meta-only
messages (`MarkDirsListed`, `UpdateMeta`/`DeleteMeta`, `BumpCurrentEpoch`) never bump the generation. Pinned by
`tests::{search_feeding_tracker_bumps_global_generation, non_search_feeding_tracker_does_not_bump_global_generation,
spawned_non_feeding_writer_does_not_bump_global_generation}`.

## Maintenance: vacuum and WAL checkpoint (`maintenance.rs`)

Free pages are reclaimed both inline after `TruncateData` and on a 30 s background timer that sends `IncrementalVacuum`
+ `WalCheckpoint`. The vacuum handler uses a tiered cap (`pick_vacuum_cap`): skip when freelist < 1,000, a 2,000-page
cap up to 20,000, a 20,000-page cap above — tiny steady-state lock holds while draining real backlog in tens of minutes.
The WAL checkpoint handler runs `PRAGMA wal_checkpoint(TRUNCATE)`; the scanner fires an explicit `WalCheckpoint` after
`ComputeAllAggregates` so the GB-scale post-scan WAL spike trims immediately. The schema/pragma side (WAL mode, page
cache, `wal_autocheckpoint`, `journal_size_limit`) lives in [`../store/DETAILS.md`](../store/DETAILS.md).

**Gotcha — row-yielding pragmas need per-row stepping, not `execute_batch`.** `PRAGMA incremental_vacuum(N)` compiles to
a loop that frees ONE page per `sqlite3_step()`, yielding a row after each; `execute_batch` steps a statement exactly
once, so it frees a single page regardless of `N`. Vacuum call sites route through
`crate::sqlite_util::run_incremental_vacuum(conn, cap)` (prepares the pragma, steps to exhaustion);
`wal_checkpoint(TRUNCATE)` also returns a row, so it goes through `query_row`. Never send either through
`execute_batch` — the freelist then drains one page per 30 s tick and the file never shrinks.

**Gotcha — a checkpoint can't run inside a transaction, so the tick defers it.** `PRAGMA wal_checkpoint(TRUNCATE)` fails
with `SQLITE_LOCKED` whenever a transaction is open. A journal replay wraps its entire run in one `BeginTransaction`, so
the 30 s tick would fail on every tick for the whole replay (a warn per tick naming a non-error, and no checkpoint when
write volume was highest). `request_wal_checkpoint` now checks `conn.is_autocommit()` and parks the tick
(`deferred_checkpoint`, debug-logged); `run_deferred_wal_checkpoint` runs it right after the `CommitTransaction`
handler's COMMIT and re-checks `is_autocommit` first. A real SQLite error from a checkpoint that DID run still routes to
`signal.note`.

**The busy handler logs per episode.** The writer's SQLite busy handler (`mod.rs::spawn`) emits ONE
`stall_probe::sqlite_busy` line per contention episode, not per retry: "writer waited 340 ms over 27 attempts…", or
"writer gave up… after 260 ms over 52 attempts" once it passes `BUSY_GIVE_UP_ATTEMPT` (50, ~255 ms at 5 ms a retry).
Brief contention is routine (WAL checkpoints, long-lived readers), so a short episode stays at DEBUG; a sustained one
(peak attempt ≥ 20, >100 ms of lock wait) goes to WARN via the pure `busy_handler_escalates(attempt, in_checkpoint)`
policy. A per-attempt ladder is a log flood by construction (a measured run: 107 lines in three bursts, up to 52 in
0.9 s). During `handle_wal_checkpoint`'s TRUNCATE the handler stays quiet: the TRUNCATE deliberately waits readers out to
~attempt 51 before degrading to PASSIVE, so a `WalCheckpointGuard` (writer-thread-local flag) is stamped onto the
episode and `busy_handler_escalates` keeps that expected wait at debug ("(WAL checkpoint reader wait)"). Every other
contention still warns.

## Writer-wait attribution (`wait_probe.rs`)

The writer channel is a bounded `sync_channel(20_000)`, so a producer parks once it's full — the backpressure that keeps
it from outrunning the single writer. The wait lands inside whatever the producer is timing with nothing to attribute
it to, so `reconcile_subtree`'s own duration silently included it ("reconcile slow for … (21s)" meant "the writer was
saturated for 19 of those seconds"). `IndexWriter::send` (via `send_blocking_with_depth`) and `flush_blocking` add every
wait to a thread-local probe: `send` tries a non-blocking enqueue FIRST (only a genuinely parked send costs anything to
measure) and the message comes back on `Full` so nothing is lost. The reconcile side arms the probe and reports the span
(see [`../reconcile/DETAILS.md`](../reconcile/DETAILS.md)). Thread-local because each producer walks on its own thread.
