# Orphaned rows under recursion-excluded dirs (2026-07-25)

Measurements behind the excluded-subtree prune. Mechanism and decisions:
`apps/desktop/src-tauri/src/indexing/writer/DETAILS.md` § "Pruning recursion-excluded subtrees" and
`apps/desktop/src-tauri/src/indexing/network_scanner/DETAILS.md` § "NAS snapshot/system dirs aren't recursed".

All figures come from a **copy** of the author's live production index,
`~/Library/Application Support/com.veszelovszki.cmdr/index-smb-192-168-1-111-445-naspi.db` (QNAP TS-464 over SMB, schema
v14, 1.88 GB), queried offline. The drive index registers a custom `platform_case` collation that isn't persisted in the
file, so a stock `sqlite3` CLI fails on any query comparing `name`; these ran through Python with

```python
con.create_collation('platform_case', lambda a, b: (a.lower() > b.lower()) - (a.lower() < b.lower()))
```

Never query the live DB in place.

## What was in there

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
lists, and it never lists these. Nothing was actively re-populating them, so a one-time prune plus the per-reconcile
prune is sufficient.

## Prototype prune, run against a copy

The exact algorithm now in `IndexStore::delete_descendants_by_id`: descend from each excluded root with a frontier of
dir ids, 256 parents per child lookup, deletes chunked at 512 ids, committing per level.

- Candidate discovery (`SELECT id, name FROM entries WHERE is_directory = 1 AND lower(name) IN (…)`, a full table scan,
  no index covers `name`): **0.72 s warm, 4.42 s cold**.
- Prune: **23.1 s** for 10 898 710 rows. Peak frontier **5 951 dir ids** (48 KB) — the reason to prefer this over a
  single recursive-CTE `DELETE`, which would materialize 10.9M ids into one ephemeral table twice and one transaction.
- Result: 2 642 893 entries, 71 199 dirs, 71 199 `dir_stats`, three excluded roots surviving (the 49 nested ones
  correctly deleted as descendants), and **zero** rows whose parent no longer exists.
- Freelist afterwards: 330 305 pages × 4 KiB = **1 353 MB**. Draining it fully took 45 s and took the file from **1.88
  GB to 529 MB**. In the app this drains on the 30 s `IncrementalVacuum` tick rather than in one hold.

**Gotcha while measuring:** `PRAGMA incremental_vacuum(N)` frees one page per step and yields a row each time, so a
driver that doesn't fetch the rows reclaims a single page and looks like a no-op. The app already handles this in
`sqlite_util::run_incremental_vacuum`; the first prototype run did not, which made the freelist look unreclaimable.

## The interrupted prune stranded 9.8M rows permanently

The prototype above measured an uninterrupted run. Quitting the app mid-prune is not an edge case: the prune runs at
startup and takes tens of seconds on this index. Doing that once, against the real app, produced a state no later launch
could repair.

- Quit part-way: **12 442 990 rows**, only ~1.1M deleted.
- Relaunch and let the prune run again: **12 442 990 rows. Unchanged.** It found nothing to do.
- Why: rows reachable from the root were **2 649 628 of 12 442 990**, so **9 793 362 were stranded**. 910 317 of them
  had a directly-missing parent (the severed tops, counting the root sentinel itself, whose `parent_id = 0` has no row
  by design); the rest hung below those.

The deletion descended top-down, so interrupting it severed the tree: a recursive descent from `@Recently-Snapshot` then
reached only its own row. The rows stayed invisible to every future prune, still bloating the DB and every O(entries)
walk. On any machine whose first post-upgrade launch got interrupted, the fix silently never completed.

That state is preserved as `live-check.db` for regression work.

## Measurements after the fix

Post-order deletion (a directory row goes only once its subtree has), a durable in-progress mark written before the
first delete, and a closing orphan sweep. Mechanism and rationale:
`apps/desktop/src-tauri/src/indexing/store/DETAILS.md` § "Decision: a subtree delete is post-order…" and
`apps/desktop/src-tauri/src/indexing/writer/DETAILS.md` § "Pruning recursion-excluded subtrees".

All three runs below drove the REAL `IndexStore` code (not a prototype) over copies of the two production DBs, in a
debug build on an M-series laptop. Candidate discovery was ~8 s cold in each.

- **Pristine 13 541 603-row DB, uninterrupted**: 10 898 710 rows deleted in 54 s, sweep 0.4 s / 0 rows. Result **2 642
  893 rows, 0 orphans, all 2 642 893 reachable** — exactly the last completed scan's own count.
- **Pristine DB, interrupted after 1 100 000 deletes**: 12 441 603 rows and **0 orphans** at the stop point (the old
  ordering left 9 793 362 stranded at the same depth). The relaunch deleted the remaining 9 798 710 and landed on the
  same **2 642 893 rows, 0 orphans**.
- **`live-check.db`, the already-damaged half-pruned DB**: re-descending the six surviving excluded roots reached only
  **6 735** rows, confirming the prune alone can never recover. The sweep removed the other **9 793 362** in 54 s.
  Result **2 642 893 rows, 0 orphans, all reachable**, i.e. byte-for-byte the state an uninterrupted run produces.

Sweep cost on an index with nothing to collect is **0.36 s** (one table scan with a PK probe), and it only runs inside a
prune run, which the exclusion-list fingerprint gates to once per DB per list version.

Post-order's price is holding directory ids for all levels rather than one frontier: **324 128 ids (2.6 MB)** across
seven levels here, versus a 5 951-id peak for a single frontier. Files are deleted during the descent and never
accumulate. Both remain far under the ~87 MB a recursive-CTE `DELETE` materializes.
