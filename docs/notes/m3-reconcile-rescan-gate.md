# M3.0 gate — non-destructive reconcile rescan (perf + correctness)

Status: gate complete. Recommendation: **GO**, full (reconcile for all volume kinds), with one mandatory design
constraint (single bottom-up coverage recompute, not per-dir propagation). Measured 2026-06-25 on the
`index-reconcile-rescan` worktree.

Throwaway gate artifacts (review, then delete or promote): `apps/desktop/src-tauri/src/indexing/reconcile_bench.rs`
(perf, `#[ignore]`d), `apps/desktop/src-tauri/src/indexing/reconcile_correctness.rs` (correctness, runs in CI). Wired in
`indexing/mod.rs`.

## TL;DR

- **Correctness is achievable cleanly.** The existing `reconcile_subtree` already does the exhaustive per-dir diff
  (add/remove/modify/dir↔file type-change) and already stamps `listed_epoch` + propagates `min_subtree_epoch` (M1/M2
  landed it). A complete reconcile deletes vanished children itself, so an interrupted→complete cycle self-heals: no
  orphans, no ghost sizes, across repeated cycles. The reconciled index matches a fresh-from-scratch index byte-for-byte
  on sizes/counts/membership. **No orphan-sweep mechanism is required for correctness** when a complete reconcile runs —
  and none is wired: an epoch sweep was prototyped and rejected (see the residual note below).
- **Perf is acceptable — IF the coverage recompute is done right.** Firing `PropagateMinSubtreeEpoch` once per dir (37k
  ancestor-walks) is the dominant cost and makes a no-op reconcile ~2× SLOWER than today's truncate baseline. Replacing
  it with ONE bottom-up aggregate after the walk makes a no-op reconcile CHEAPER than the truncate baseline.

## A. Performance

Bench measures the DB write-path delta only (the FS/network walk is unchanged from today's scan and is the same across
all strategies, so it's excluded — we drive the real writer with each strategy's exact message stream). Synthetic tree:
**486,836 entries / 37,449 dirs** (≈ local `root` entry scale; ~37k dirs extrapolates ×~14 to the doc's ~538k-dir figure
— costs below are O(dirs)/O(entries) linear, so scale them by ~14 for full `root`).

Release build, representative run (run-to-run varies with machine load; the _ranking_ is stable):

| Strategy                                                        | Time          | vs baseline            |
| --------------------------------------------------------------- | ------------- | ---------------------- |
| Baseline: truncate + bulk reinsert + `ComputeAllAggregates`     | ~910–1900 ms  | 1.0×                   |
| Reconcile no-op, **per-dir `PropagateMinSubtreeEpoch`** (naive) | ~1950–2600 ms | ~2× SLOWER             |
| Reconcile no-op, **single bottom-up aggregate** (recommended)   | ~720–1070 ms  | **~0.6–0.8× (faster)** |
| Reconcile 1%-changed (375 dirs)                                 | ~1200–2600 ms | ~1.3×                  |

No-op phase breakdown (release): per-dir DB read+name-diff ≈ 350 ms, `MarkDirsListed` ≈ 40–55 ms (cheap, PK-keyed UPDATE
as designed), **per-dir propagate ≈ 1550–2200 ms** (the killer), single aggregate ≈ 370–550 ms.

Why per-dir propagate is so costly: `propagate_min_subtree_epoch` walks the ancestor chain doing `recompute` +
`get_parent_id` queries per hop, with a short-circuit only once a value stabilizes. After a _full_ reconcile that
re-stamps every dir to the same new epoch, nothing stabilizes early on the first deep walks, so it degenerates toward
O(dirs × depth) round trips. A single bottom-up pass (`compute_all_aggregates_reported`, the SQL fallback
`ComputeAllAggregates` already takes when accumulator maps are empty) recomputes the whole coverage rollup in O(dirs)
with two bulk loads — and is verified to re-stamp coverage to the new epoch correctly (assertion in the bench).

Key gate property holds: **a no-op reconcile writes ZERO entry rows** (asserted), so it never touches the catastrophic
`INSERT OR REPLACE` / `platform_case` path. The 1%-changed arm writes only the changed rows + their delta propagation.
Neither arm re-UPSERTs unchanged rows (the failure mode the gate worried about). `UpsertEntryV2` does always issue an
`UPDATE` for a matched row, but `reconcile_subtree` only sends `UpsertEntryV2` when `changed` is true, so unchanged rows
are never written.

Extrapolation to full `root` (~538k dirs, ~5.5M entries): multiply by ~11–14. The recommended no-op reconcile stays at
or below the truncate baseline's order of magnitude; the absolute win is that it never blanks the index and never
regresses prior-complete data. (Debug-build numbers are ~3× higher across the board; use release figures.)

## B. Correctness

All in `reconcile_correctness.rs`, real `reconcile_subtree` against real on-disk temp trees + real store/writer:

1. `reconcile_handles_add_remove_modify_typechange` — add, remove, size+mtime modify, and dir→file type change all land;
   counts and `min_subtree_epoch` update; no orphans.
2. `partial_reconcile_keeps_a_fresh_b_stale` — after an epoch bump, reconciling only `/a` re-stamps `/a` to the current
   epoch while `/b` stays at the old epoch (stale). Proves partial-rescan granularity.
3. `deleted_dir_unreached_by_interrupted_reconcile_heals_on_next_complete` — the exact gate probe. A dir deleted on disk
   while a reconcile is interrupted before reaching it DOES linger after the interrupted pass (asserted hazard) and DOES
   drag the parent's coverage below the current epoch — then a later complete reconcile sweeps it and restores coverage.
4. `repeated_interrupted_then_complete_reconcile_leaves_no_orphans_or_ghosts` — 3 cycles of mutate (incl. deleting a
   whole subtree) → bump → interrupted reconcile → complete reconcile. After all cycles: no orphans, and the index
   matches a fresh-from-scratch build of the final disk state (no ghost sizes). This is the 1.83 TB-ghost-class guard as
   a standing test.

Why orphan-freedom holds without a mandatory sweep: a _complete_ reconcile re-lists every dir and its delete branch
(`for row in db_children { if !matched { Delete } }`) removes any child absent from the live listing — including the
whole subtree of a dir deleted while a prior pass was interrupted, because that dir is a direct child of _some_ dir the
complete pass re-lists. The interrupted state is transiently dirty (lingering rows, coverage shows incomplete = honest
"≥"/"—"), and the next complete pass is exhaustive.

The one residual: if a user is interrupted on EVERY rescan and never completes one, lingering rows persist (but coverage
honestly reads incomplete the whole time — no false "complete"). An epoch sweep was prototyped for this case and
rejected: it can't help. Its predicate (`listed_epoch < rescan_epoch` AND parent re-listed this epoch) only matches a
child under a parent the pass DID re-list — and a re-listed parent already ran the delete branch, so its genuinely-gone
children are pruned already. The only rows the sweep would then match are dirs still present on disk whose own listing
failed transiently (kept at their old epoch by design as "honest unknown") — deleting those is a regression, not
insurance. Nothing short of completing a reconcile of the parent fixes the never-completes case.

## Recommendation: full GO

Route the full rescan through reconcile for ALL volume kinds (local jwalk + SMB/MTP `volume_scanner`). The diff
machinery, epoch stamping, and coverage rollup already exist and are proven correct here. No local-vs-network split is
warranted — the perf concern was the coverage recompute strategy, not the volume kind.

### M3.1 design shape (blueprint)

- **Reuse `reconcile_subtree`'s per-dir diff** as the rescan body. For local, generalize it to drive from the jwalk walk
  (or keep BFS read_dir — the walk cost is I/O-bound and unchanged); for SMB/MTP, drive it from `volume_scanner`'s
  `Volume::list_directory` BFS instead of `std::fs::read_dir`. The diff/upsert/delete logic is identical.
- **Drop the up-front `TruncateData` for rescans.** Production truncate sites to change: `manager.rs:764` (local
  `start_scan`) and `manager.rs:380` (network `start_volume_scan`). Keep `TruncateData` ONLY for a true first scan (no
  existing index) and `clear_index` rebuild.
- **MANDATORY: do NOT propagate coverage per-dir on a full rescan.** After the walk, stamp all listed dirs
  (`MarkDirsListed`, already cheap) and run ONE `ComputeAllAggregates` (which with empty accumulator maps takes the
  O(dirs) bulk-SQL `compute_all_aggregates_reported` path and recomputes sizes + `min_subtree_epoch` correctly). This is
  the difference between ~2× slower and faster-than-baseline. Per-dir `PropagateMinSubtreeEpoch` stays only for the
  small-scope live verifier reconciles (where the chain is short), not the full rescan.
- **Epoch bump at rescan start** (continuity break) → whole tree reads stale-but-visible; each reconciled dir flips
  fresh as re-listed. The bump funnels already exist (`start_scan` / `start_volume_scan` per M2).
- **Keep `next_id` from the shared `Arc<AtomicI64>`** (reads `MAX(id)+1` on writer spawn; never reset for a rescan). IDs
  growing across rescans is harmless (i64).
- **Preserve the pre-arm-before-snapshot live-change buffering**, adapted: there's no truncate to race now, so a
  mid-rescan live change to an already-reconciled dir can be applied directly (or kept buffered + replayed; either is
  safe since the prior tree is intact). Simpler than today.
- **No orphan sweep.** An epoch sweep was prototyped and dropped: after a complete reconcile its true-positive set is
  empty (the per-dir delete branch already pruned every gone child under a re-listed parent), and the only rows it would
  match are present-on-disk dirs with a transient listing error — false deletes. See the residual note above.

### What this buys

Mid-rescan disconnect leaves the prior complete data intact (no truncate to blank it) — the proper fix for M2's
session-scoped-partial limitation. A persisted index shows stale-but-whole across relaunch.
