# Aggregator (bottom-up dir-stats computation)

Computes each directory's recursive `dir_stats` (size, counts, `recursive_has_symlinks`, `min_subtree_epoch`) from the
`entries` table. Pure compute + SQL readers; the writer thread drives it. The honest-sizes DATA MODEL (what
`listed_epoch` / `min_subtree_epoch` / `current_epoch` mean, and the live-path discipline) is canonical in
`../writer/DETAILS.md` — this area owns the COMPUTE math only.

## Module map

- **mod.rs**: the `compute_*` entry points, `compute_and_write`, `compute_bottom_up`, `topological_sort_bottom_up`,
  `absorbing_min_epoch`, `depth_of`, `backfill_missing_dir_stats`, `compute_partial_aggregates`(`_sql`), the
  `AggregationPhase` / `AggregationProgress` types.
- **readers.rs**: the `bulk_*` / `scoped_*` / `load_*` / `get_listed_epochs_for_ids` SQL readers. One-directional seam:
  compute → readers, never the reverse.

## Must-knows

- **Full aggregation is a single O(N) bottom-up pass** (`topological_sort_bottom_up` deepest-first, then
  `compute_bottom_up`), NOT per-dir recursive queries. Two entry points: `compute_all_aggregates_with_maps` (writer's
  pre-built `AccumulatorMaps`) and `compute_all_aggregates_reported` (loads maps from SQL). The composite indexes use
  binary collation, so there is NO per-scan index-rebuild phase.
- **Subtree aggregation uses scoped recursive CTEs** (`scoped_get_children_stats_by_id`, `scoped_get_child_dir_ids`),
  keeping it O(subtree_size) regardless of total DB size. Never a full-table scan for a subtree.
- **`compute_bottom_up` rolls up `min_subtree_epoch` as the 0-absorbing `min` (`absorbing_min_epoch`) of a dir's own
  `listed_epoch` and every child dir's already-computed `min_subtree_epoch`.** The per-dir `listed_epoch` map is a
  SEPARATE input (NOT in `AccumulatorMaps`, which are keyed by `parent_id` and never see a dir's own epoch). ALL FOUR
  callers must supply it; a missing one re-breaks coverage (every touched dir → `min_subtree_epoch = 0`).
- **`compute_partial_aggregates` (mid-scan) derives its dir list from the BORROWED maps, never a SQL dir list.** A SQL
  `load_all_directory_ids` here would be the forbidden empty-maps fallback the writer's late-race safety depends on. It
  reads `listed_epoch` for exactly the dirs already in the maps via one batched `WHERE id IN (...)`
  (`get_listed_epochs_for_ids`), no full-table scan, no N+1. It still no-ops on empty maps.
- **`backfill_missing_dir_stats`** finds directories without a `dir_stats` row and computes them bottom-up; triggered
  after reconciler/cold-start replay via the `BackfillMissingDirStats` writer message.

Bottom-up + topological sort, the scoped-CTE readers, the epoch rollup, backfill, and the two partial-aggregate
variants: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
