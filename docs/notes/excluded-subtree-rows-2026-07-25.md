# Rows under recursion-excluded dirs, and why we rebuild instead of pruning (2026-07-25)

Measurements behind the one-time exclusion-list rebuild. Mechanism and decisions:
`apps/desktop/src-tauri/src/indexing/network_scanner/DETAILS.md` § "Rebuilding an index that predates the current list".

All figures come from a **copy** of the author's live production index,
`~/Library/Application Support/com.veszelovszki.cmdr/index-smb-192-168-1-111-445-naspi.db` (QNAP TS-464 over SMB, schema
v14, 1.88 GB), queried offline. The drive index registers a custom `platform_case` collation that isn't persisted in the
file, so a stock `sqlite3` CLI fails on any query comparing `name`; these ran through Python with

```python
con.create_collation('platform_case', lambda a, b: (a.lower() > b.lower()) - (a.lower() < b.lower()))
```

Never query the live DB in place.

## What's in there

- `entries`: **13 541 603** rows. `dir_stats`: 391 563 rows (one per dir).
- `meta.total_entries`, written by the last completed scan (`scan_completed_at` = 2026-07-24): **2 642 902**.
- Rows beneath a recursion-excluded dir, deduplicated for nesting: **10 898 710** — 80% of the table.
  `13 541 603 − 10 898 710 = 2 642 893`, the last scan's own count to within nine rows.
- 52 directories matched the exclusion list: `@Recently-Snapshot` ×24, `@Recycle` ×24, `System Volume Information` ×4.
  Only **three** are outermost; the other 49 sit inside `/@Recently-Snapshot`, which holds a full copy of the share (its
  own `@Recycle`, and a `@Recently-Snapshot` of its own, per snapshot). **Naive per-root descendant counts
  double-count**: summing all 52 gives 10 974 484, not 10 898 710.
- Consequences: `dir_stats` put `/@Recently-Snapshot` at **83.5 TB** and the index root at **89.1 TB**, on a 10 TB NAS.
  Every O(entries) walk paid 5×, which is what multiplied the `media_index::coverage` memory runaway.

## Where the rows came from

Every one of the 391 563 dirs is at `listed_epoch` 0 or 20, and the split is exact: the 71 199 in-scope dirs sit at
epoch 20 (the last completed reconcile), and all 320 076 dirs under `/@Recently-Snapshot` sit at 0 — never listed by any
epoch-aware pass, yet fully populated with rows. The excluded dirs also carry the lowest ids in the table (3, 9, then
693–1100), i.e. they were walked early in a BFS.

That fits one story: a FRESH scan (before the exclusion existed, or before it covered these names) inserted the whole
tree and was then cancelled, so it wrote no marks — but its rows stayed, because a discarded scan removes the registry
instance, not the DB file. Every later pass took the reconcile path (`entry_count > 1`), which only diffs the dirs it
lists, and it never lists these. Nothing is actively re-populating them; they're inert legacy rows no reconcile can
reach.

## Why the fix is a rebuild, not a prune

A prune was built first, and it worked: post-order deletion of all 10 898 710 rows in 54 s (23 s for a tighter
prototype), an in-progress marker so a mid-run quit stayed resumable, an orphan sweep for indexes an earlier top-down
delete had already severed, a fingerprint gate, `listed_epoch` and `dir_stats` resets, and a data-safety argument that
it could only ever delete LESS than the scanner's own rule allows.

It was still the wrong trade. The drive index is a disposable cache, no index holds anything valuable (the app is
pre-launch, and a NAS rescan is ~10 minutes), and preserving one didn't justify a permanent piece of migration machinery
that fixes only the one thing it targets. So the prune is gone: a network index now stamps the exclusion list it was
BUILT against, and a mismatch makes the next load truncate and rescan. Standing rule:
`apps/desktop/src-tauri/src/indexing/CLAUDE.md` § "Rebuild, don't migrate".

Verified against a copy of this DB (2026-07-25): it carries no exclusion stamp, so the rebuild arms rather than silently
no-opping; the truncate the rebuild sends first takes it from 13 541 603 rows and 52 matched excluded dirs to the 1-row
root sentinel with none left; and the stamp written straight after makes the next load leave it alone.

## Two findings the code still depends on

**Subtree deletes must be post-order.** `IndexStore::delete_descendants_by_id` still serves the pre-subtree-rescan
delete, and the ordering is why an interruption can't strand rows. The evidence: an interrupted TOP-DOWN run over this
index left 12 442 990 rows of which **9 793 362 were unreachable** from the root (910 316 of them directly parentless),
and re-running found nothing to do — severed rows are invisible to any descent. Post-order deletes files on the way down
and directories deepest-level-first, so every stop point leaves a walkable tree. Zero orphans after interrupting a run
1.1M deletes in, and after every prefix of the deletion order in
`store/tests/subtree_deletes.rs::interrupting_a_subtree_delete_never_strands_a_row`. The price is 324 128 retained dir
ids (2.6 MB) across seven levels, versus a 5 951-id peak for a single frontier, both far under the ~87 MB a
recursive-CTE `DELETE` materializes.

**Freed pages go to the freelist, and only a stepped vacuum drains them.** Removing the 10.9M rows left 330 305 free
pages × 4 KiB = **1 353 MB**; draining took 45 s and brought the file from **1.88 GB to 529 MB**. In the app this drains
on the 30 s `IncrementalVacuum` tick. `PRAGMA incremental_vacuum(N)` frees one page per step and yields a row each time,
so a driver that doesn't fetch the rows reclaims a single page and looks like a no-op; the app handles that in
`sqlite_util::run_incremental_vacuum`, and the first prototype run did not, which made the freelist look unreclaimable.
