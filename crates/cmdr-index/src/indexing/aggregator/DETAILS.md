# Aggregator details

Read this before any non-trivial work in `aggregator/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

This area owns the COMPUTE math that turns the `entries` table into `dir_stats` rows. It does not own the honest-sizes
data model: what `listed_epoch`, `min_subtree_epoch`, and `current_epoch` MEAN, when marks are stamped, and the
live-path discipline that keeps coverage honest between scans are canonical in
`../writer/DETAILS.md` § "Honest sizes". The schema columns are defined in
`../store/DETAILS.md`. The writer's ledger repair primitive (`repair_dir_stats_upward`) is the
incremental counterpart to full aggregation and lives in `../writer/DETAILS.md` § "The dir_stats
ledger".

## The three modes

- **Full aggregation**: after a full scan, compute `dir_stats` for every directory (deepest first).
- **Subtree aggregation**: after a subtree scan, compute `dir_stats` only under a given root.
- **Delta propagation**: after a watcher event, walk up the ancestor chain updating counts. This one lives on the
  writer (`../writer/delta.rs`), not here — it's the incremental complement to a full recompute.

## Bottom-up compute (`compute_bottom_up`, `compute_and_write`)

Full aggregation is a single O(N) pass. `topological_sort_bottom_up` orders directories deepest-first (a child is
always computed before its parent), then `compute_bottom_up` sums each dir's direct file children plus each child dir's
already-computed `dir_stats` into the dir's own recursive totals. Two entry points share the topological sort +
bottom-up + batch write via `compute_and_write`:

- **`compute_all_aggregates_with_maps`**: accepts the writer's pre-built `AccumulatorMaps` (populated during a fresh
  scan by `InsertEntriesV2`), so it skips the two expensive full-table-scan SQL queries (`bulk_get_children_stats_by_id`,
  `bulk_get_child_dir_ids`) that would otherwise dominate aggregation time (~70%).
- **`compute_all_aggregates_reported`**: loads the maps from SQL. Used when there are no accumulator maps (the reconcile
  finish and the one-shot ledger heal both send `source: Sql`).

Both accept an `on_progress: &mut dyn FnMut(AggregationProgress)` callback and report at phase transitions and every
~1% during the compute/write loops. `AggregationPhase`: `SavingEntries` (flushing the writer channel),
`LoadingDirectories`, `Sorting`, `Computing`, `Writing`. Because the composite indexes use binary collation there is no
per-scan index-rebuild phase.

**`recursive_has_symlinks`** is OR-aggregated bottom-up alongside sizes: a dir's flag is `true` if any direct child is
a symlink OR any child dir's `recursive_has_symlinks` is set. The `ChildrenStatsMap` alias carries the direct
`has_symlinks_direct` bit per parent.

## The `min_subtree_epoch` rollup

`compute_bottom_up` computes each dir's `min_subtree_epoch` as the 0-absorbing `min` (`absorbing_min_epoch`) of the
dir's own `listed_epoch` and every child dir's already-computed `min_subtree_epoch`. So a listed-empty dir keeps its own
epoch (`> 0`, renders as a genuine `0 bytes`), a fully-listed subtree rolls up to its epoch (exact), and a single
unlisted descendant anywhere drags the whole subtree to `0` (incomplete → lower bound / unknown).

**The per-dir `listed_epoch` map is a NEW, SEPARATE input to `compute_bottom_up`** — it is NOT carried in
`AccumulatorMaps` (those are keyed by `parent_id` and never see a dir's OWN epoch; the mark arrives via the writer's
separate `MarkDirsListed` message). ALL FOUR callers supply it, and a missing one would re-break coverage (every
touched dir → `min_subtree_epoch = 0`):

- `compute_all_aggregates_with_maps` and `compute_all_aggregates_reported`: full-table read (`bulk_get_listed_epochs`)
  in the same pass that loads the dir list.
- `compute_subtree_aggregates`: a scoped read (`scoped_get_listed_epochs`, mirroring its scoped CTE child queries).
- `backfill_missing_dir_stats`: full-table read.
- `compute_partial_aggregates` (mid-scan): reads `listed_epoch` for exactly the dirs already in the borrowed maps via
  a SINGLE batched `WHERE id IN (...)` (`get_listed_epochs_for_ids`) — never a full-table scan, never per-dir N+1, and
  crucially never a SQL dir list. Mid-scan the marks haven't landed yet, so these read `0` and the partial sizes are
  honest lower bounds; the final aggregate stamps them exact.

Why marks must land before aggregation, and the four live-path propagation rules, are in the writer doc. The
partial-aggregation differential invariant holds: the FINAL `min_subtree_epoch` is byte-identical with and without
partial passes (the recompute-from-`entries` oracle in `stress_test_helpers::check_db_consistency` covers the column).

## Subtree aggregation uses scoped queries

`scoped_get_children_stats_by_id` and `scoped_get_child_dir_ids` (`readers.rs`) use recursive CTEs scoped to the target
subtree, not full-table scans, so subtree aggregation stays O(subtree_size) regardless of total DB size.
`compute_subtree_aggregates` writes the subtree's rows (including the subtree root's fresh totals); the writer's
`handle_compute_subtree_aggregates` then calls `repair_dir_stats_upward(parent_of_root)` to roll the net effect up the
ancestor chain (sizes, counts, symlinks, and `min_subtree_epoch` in one walk) — see the writer doc.

**The compute → readers seam is one-directional.** `mod.rs` (compute) calls into `readers.rs` (SQL); `readers.rs` never
calls back. Keep it that way: a reader that reached into compute would tangle the pure algorithm with its data source.

## `backfill_missing_dir_stats`

A catch-up pass that finds directories WITHOUT a `dir_stats` row (`load_dirs_missing_stats`) and computes their stats
bottom-up. Triggered after reconciler replay and cold-start replay via the `BackfillMissingDirStats` writer message.
After writing the missing rows, the writer repairs each "missing root" 's parent upward (see the ledger doc), crediting
ancestors that a delta never walked through because the row never existed — monotone convergence toward truth, never
preserved corruption.

## Mid-scan partial aggregation math

`compute_partial_aggregates` is the mid-scan variant the writer calls on `ComputePartialAggregates { source: Maps }`
(the writer borrows its maps read-only; the writer-side discipline and the load-bearing empty-maps no-op are in the
writer doc). It:

- derives the dir list and parent relations from the BORROWED accumulator maps — NOT a SQL `load_all_directory_ids`
  scan (that would be the forbidden empty-maps fallback);
- computes each dir's depth from the scan root via a memoized walk (`depth(ROOT_ID) = 0` is the explicit base case;
  unreachable dirs get `usize::MAX` so the depth cap never writes them);
- reuses the SAME `topological_sort_bottom_up` + `compute_bottom_up` over ALL dirs (cheap, pure in-memory);
- writes only dirs at `depth ≤ max_depth` (`PARTIAL_AGG_MAX_DEPTH = 3`) plus each resolvable hot-path dir and its
  direct children — bounding the expensive WRITE, not the compute.

**`compute_partial_aggregates_sql(conn, hot_paths, cap)`** is the SQL-sourced sibling for the paths whose accumulator
maps stay empty — a LOCAL rescan-in-place (`UpsertEntryV2`) and the SMB/MTP scan — so those also get growing sizes
mid-scan. It resolves each hot path to a dir id via `store::resolve_path_under(conn, ROOT_ID, ..)`, then runs a SCOPED
bottom-up aggregate over that dir's subtree (reusing the scoped CTE readers + `compute_subtree_map`) and writes ONLY the
hot dir + its DIRECT CHILDREN (the rows on screen). It does NOT honor the depth-≤3 shallow write set the maps path does
— that would need a whole-tree aggregate (no maps to lean on), the writer stall the cap guards against.
`min_subtree_epoch` falls out of `compute_bottom_up` + `absorbing_min_epoch` over the scoped `listed_epochs`, not
special-cased.

The stability cap (`PARTIAL_AGG_SQL_MAX_SUBTREE = 100_000`) is load-bearing: unlike the maps path (pure in-memory), the
`Sql` path runs real scoped recursive CTEs per hot dir, O(subtree_size). A hot path near the volume root (a pane on `/`
or a share root) would otherwise trigger a near-whole-tree CTE on the single writer thread and stall every queued insert.
So before scoping a hot dir, a cheap check reads that dir's CURRENT `dir_stats` recursive counts
(`recursive_file_count + recursive_dir_count`, O(1)); above `cap` the dir is SKIPPED and the final aggregate (at most
seconds away) fills it. A dir with no `dir_stats` row yet is freshly created and tiny, so it proceeds. Raise the cap
only against real network-volume timings, never on a hunch. When a pane's parent AND child are both hot, only the
DEEPEST is scoped (the ancestor is dropped via a component-aware string prefix check). The reporter that drives sending,
and `resolve_path_under`'s role in resolving mount-relative hot paths on network volumes, live in `../events/` and
`../paths/DETAILS.md`.
