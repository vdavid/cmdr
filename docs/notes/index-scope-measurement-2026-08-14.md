# What indexing outside `$HOME` actually costs

**The question this settled:** should phased indexing also narrow the DEFAULT scope to `$HOME`, leaving the rest of the
drive behind a setting? Answer: **no.** Phasing changes the walk ORDER; the extent stays the whole drive. The numbers
below are why, and they are the evidence `docs/specs/phased-indexing-plan.md` decision 3 rests on.

## Method

Read directly out of the live boot-volume index (`index-root.db`, schema v16) on David's M3 MacBook Pro, 2026-08-14,
macOS 26.5.2 build 25F84. Subtree counts are recursive CTEs over `entries.parent_id` (authoritative row counts), ❌ not
`dir_stats.recursive_*`, whose rollups disagreed by ~6% (hardlink dedup and self-counting). Timings and totals are the
index's own calibration meta, written by the last real walk.

Reproduce with a read-only copy of the DB and:

```sql
WITH RECURSIVE sub(id) AS (
  SELECT :root_id UNION ALL SELECT e.id FROM entries e JOIN sub s ON e.parent_id = s.id
) SELECT COUNT(*) FROM sub;
```

⚠️ The DB declares a custom `platform_case` collation, so `sqlite3` can't evaluate predicates on `name`. Query
`name_folded` instead.

## The state of that index

- **Rows**: 5,191,189 (529,261 directories)
- **Index file**: 768 MB (~148 bytes/entry)
- **Full walk**: 193 s (`scan_duration_ms_full_walk`)
- **Reconcile-in-place rescan**: 622 s (`scan_duration_ms`), 3.2× the walk
- **Whole data dir**: 4.1 GB (index 768 MB + importance 70 MB + media 31 MB + the rest)

## Outside `$HOME` — what a home-only default would have skipped

**800,441 entries, 15.4% of the index.** At the measured rate that is roughly **30 seconds of a 193-second walk** and
**~115 MB of 768 MB**. CPU scales with entries walked, so ~15% there too.

| path                     | entries | share |
| ------------------------ | ------: | ----: |
| `/Applications`          | 300,688 |  5.8% |
| `/Library`               | 218,920 |  4.2% |
| `/opt`                   | 212,490 |  4.1% |
| `/private`, `/usr`, rest | ~68,300 |  1.3% |

## Inside `$HOME` — 4,390,748 entries, 84.6%

| path                                                        |    entries | share of index |
| ----------------------------------------------------------- | ---------: | -------------: |
| `~/projects-git`                                            |  1,580,702 |          30.5% |
| `~/Library`                                                 |  1,437,538 |          27.7% |
| Desktop + Documents + Downloads + Pictures + Movies + Music |  **4,735** |      **0.09%** |
| rest of home                                                | ~1,367,700 |          26.4% |

`~/Library` breaks down as Caches 423k, Mail 395k, Application Support 210k, CloudStorage 162k, pnpm 94k.

## The three conclusions

1. **A home-only default buys ~30 s and ~115 MB here.** Small enough that it can't justify a permanently partial index,
   which every completion, freshness, rescan, sweep, watch, and upgrade path in `cmdr-index` would have had to learn
   about.
2. **It skips the small pile and keeps the big one.** `~/Library` alone is **1.8× everything outside `$HOME` combined**,
   and it is inside home, so the "we don't spend your machine on folders you don't care about" story doesn't survive
   contact with the disk.
3. **The product win is nearly free and independent of this decision.** The folders the wow moment is about total 4,735
   entries — under a second — so ordering them first works identically whatever the extent is.

## Caveat on representativeness, stated because it cuts the other way

This machine is atypical: home is 84.6% because of a dev toolchain (`projects-git` 1.58M, `~/Library` 1.44M). A user
without one might hold 200–400k in home against ~520k outside (`/Applications` + `/Library`, no `/opt`), so outside-home
could be half or more of _their_ index, and a home-only default would roughly halve their work. But their whole scan is
30–60 s to begin with, so it saves 15–30 s of background work that phasing has already made interruptible and
deprioritized. The conclusion holds for a different reason on that machine: the absolute saving is small either way.

**What would change the answer:** a machine where the full walk is minutes and most of it is outside home (a small home
on a disk with a huge `/opt` or `/Library`), or a resource-constrained target where 115 MB of index matters. Re-measure
before assuming; the query above is cheap.

## Unrelated finding, fixed

`onboarding.stepOptional.indexing.descCost` used to tell the user the cost was "a 300 MB index on your drive". It now
says around 1 GB for a few million files. Re-measured on David's machine (`ls -l` over the prod data dir,
`~/Library/Application Support/com.veszelovszki.cmdr/`, database plus its `-wal` and `-shm` sidecars, 2026-08-15): the
boot file index is **947.6 MB** over **6,154,077 entries** (623,939 of them folders), importance adds **71.2 MB**, and
the image index another **31.5 MB**, for **1.05 GB** all told. That's ~154 MB of file index per million entries, so the
number tracks how many files someone has and not how big their drive is, which is what the copy now says.
