# The coverage frontier query, measured on a real root index

The frontier query is the performance hinge of search over partly-indexed ground: every search asks it before it can
serve anything, so if it's slow, every search is. This is its measured exit criterion — **under 50 ms warm** — taken
before anything was built on top of it, so a later regression has a number to compare against.

**Answer: 5.4 ms warm in release, with no new database index needed.** Roughly 9× headroom.

## What was measured

`read::coverage::walk_coverage` over the whole boot volume (`/`), the widest scope a search can have.

- **Corpus**: a copy of the dev root index at
  `~/Library/Application Support/com.veszelovszki.cmdr-dev-unindexsearch/index-root.db`, captured 2026-08-04 from a full
  boot-disk walk. 6 587 612 entries, 658 188 directories, 375 of them unlisted (the honest-stale gaps a completed walk
  leaves behind: directories the walker abandoned at `LOCAL_LIST_TIMEOUT` or give-up-pruned). `current_epoch` 2,
  `min_subtree_epoch("/")` 0.
- **Machine**: David's laptop (macOS 25.5.0, Apple silicon), 2026-08-05, otherwise idle.
- **Method**: `coverage::tests::measure_frontier_query_on_a_real_index`, an `#[ignore]`d test in the crate. One warm-up
  run, then five timed runs, median reported. Rerun it with
  `CMDR_COVERAGE_BENCH_DB=<copy> cargo test -p cmdr-index --lib coverage::tests::measure -- --ignored --nocapture`. ⚠️
  It writes to the database it's given (it adds the v15 column and stamps the current exclusion policy so an index
  captured under an older build is measurable without a rescan), so always point it at a copy.

## The numbers

| Build   | First run (cache cold-ish) | Warm median (5 runs) | Spread           |
| ------- | -------------------------- | -------------------- | ---------------- |
| release | 19.1 ms                    | **5.43 ms**          | 5.24 – 5.56 ms   |
| debug   | (not separately taken)     | 16.78 ms             | 16.59 – 17.06 ms |

Frontier: **373 directories**. Unreadable: 0 (at the time of this measurement nothing stamped the marker yet; the
cover walk does now).

## Why it's fast, and what it actually scales with

**7 762 of 658 188 directories were considered** — 1.2%. That is the descent rule doing its job: a subtree with
`min_subtree_epoch > 0` is one row lookup and the descent stops, so the query's cost tracks the number of directories on
the paths to the gaps, never the size of the index. About 0.7 µs per directory considered, so even a pathological index
with 100 000 directories on gap paths would land near 70 ms rather than anywhere catastrophic, and a fully covered scope
answers in a single row lookup.

The concern the plan raised was that `idx_parent_name_folded (parent_id, name_folded)` gives the child scan a
leading-column seek but isn't covering (`listed_epoch`, `known_unreadable`, and `is_directory` aren't in it), so each
child costs a main-table fetch plus a `dir_stats` primary-key lookup. That's real and it's what the 0.7 µs is made of;
it just isn't enough to matter at this fan-out. **No index was added**, and adding a covering one would cost disk on
every index on the machine to buy headroom nothing needs yet. If a future measurement finds the descent considering tens
of thousands of directories on a real machine, that's the moment to revisit it — the shape of the fix is a covering
index on `entries (parent_id, is_directory, listed_epoch, known_unreadable, id)`.

## What this doesn't cover

- **Cold-start**, meaning the first search after launch with nothing of the database in the OS page cache. The 19.1 ms
  first run is only cache-cold-ish (the file had just been copied). It's well inside the budget either way, and a cold
  search is dominated by loading the volume's search arena (about 10.9 s for a 13.5 million-entry NAS index), not by
  this query.
- **SMB and MTP indexes.** The query is transport-agnostic (it reads SQLite, not the volume), so the only difference is
  the size and shape of the tree. A NAS index is larger but flatter, which the descent likes.
- **A partly-walked index**, which is what search-written coverage produces. That tree has far more gaps than 375, so
  it's the shape worth re-measuring against a real live-search-built index.
