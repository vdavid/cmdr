# The importance rescore treadmill: what it actually was

**Measured 2026-08-04** against prod v0.37.0's log (`~/Library/Logs/com.veszelovszki.cmdr/cmdr.log`, 13:13–23:43 on
2026-08-03) and a read-only copy of the real `index-root.db` (7,086,485 rows, 694,963 directories) and
`importance-root.db` (160,719 weight rows). `docs/notes/idle-memory-profile-2026-07-28.md` § "Cause 2" had reported this
fixed; it was not, and this note is what makes the remaining half decidable.

Keep this one until the batch-width question below is settled. Everything else here has landed.

## What the log actually showed (three claims that were wrong)

The investigation was opened on three premises, and measurement refuted all three.

- **"The counts are pinned at 52,071 folders and 160,718 weights."** They are not. Across 681 rescores the folder count
  takes hundreds of values (`51920` ×28, `51916` ×23, `52071` ×12, plus `17`, `4`, `0`); the weight count drifts too
  (`160719` ×129, `160610` ×75). The set is _nearly_ stable, which is exactly why a naive diff fails (below).
- **"Every 60 seconds, forever."** An incremental pass runs every 60 s (681 passes / 10.5 h), but only **330** took the
  full walk, in bursts, and none after 22:42. The other 351 were scoped passes writing a median of **5** folders.
- **"The delta weight reload is bypassed."** v0.37.0 predates it. The tag is `a0c168182` (07:37, 2026-08-03); the delta
  landed in `8d8118132` (11:25) plus `494236dcc`. `git merge-base --is-ancestor 8d8118132 v0.37.0` says NO, which is why
  the log holds 616 `importance weights loaded` lines and **zero** `importance weights patched`. Nothing to fix, just
  unreleased.

## The real cause: `$HOME` becomes an origin

`origin_dir` (`crates/cmdr-index/src/indexing/reconcile/reconciler.rs`) is the **parent of the changed file**, so any
file written directly in `~` makes `/Users/<user>` an origin. `$HOME` does not floor (`classify::floors_by_path` clears
it: the name isn't denylisted, no dot prefix, `path_class` is Neutral), so it passes `sanitize_incremental_batch`.

`$HOME` covers **574,007 of the volume's 694,963 directories (83%)**. Its non-floored count is **51,081**, which is what
the pass rewrites.

Evidence, four independent agreements:

- Non-floored dirs under `$HOME`: 51,081. Log rescore counts: 51,085 / 51,087 / 51,098 / 51,916 / 52,071.
- Non-floored dirs under `/`: 160,818. Log weight loads: 160,719 (drift over half a day).
- The last full-walk fallback in the log is **22:42:27**. The last modification to `~/.claude.json` is **22:42:19**.
  Eight seconds apart, and the treadmill never fires again.
- Written by Claude Code (`~/.claude.json`, constant), plus `~/.zsh_history`, `~/cmdr-check-log.csv`, `~/.zcompdump`.

**The machine is never idle**: the log's live-event counter moves 5,220,000 → 5,350,000 between 23:01 and 23:40, about
**55 FSEvents/sec** with nobody touching the app. So `sanitize_incremental_batch` always finds something; the question
was only ever how _wide_ the surviving batch is.

Only **19** non-floored dirs on this volume have subtrees over `SCOPED_WALK_MAX_DIRS`, and all but `/`, `/Users`, and
`$HOME` are static (Xcode, homebrew, CommandLineTools, `go/pkg/mod`). This is one path, not a broad cliff.

## Raising `SCOPED_WALK_MAX_DIRS` would make it worse (refuted by measurement)

Run with the cap temporarily raised to 2,000,000, `importance-diff` against the real index, zero walk disagreements
throughout:

| origin                       | dirs descended | scoped walk | rows it would write |
| ---------------------------- | -------------- | ----------- | ------------------- |
| `~/projects-git/vdavid/cmdr` | 127,427        | 1.98 s      | 2,044               |
| `~/projects-git/vdavid`      | 235,193        | 2.93 s      | 7,826               |
| `~/projects-git`             | 245,977        | 3.00 s      | 9,969               |
| **`$HOME`**                  | **574,007**    | **6.02 s**  | **51,081**          |
| the full walk (the fallback) | 694,963        | ~4.9 s      | —                   |

The scoped walk costs ~11 µs/dir, so it beats the full walk up to roughly **440,000 dirs (63% of the volume)** and loses
above that. `$HOME` is past the crossover: **6.02 s scoped against 4.9 s full**. The cap's own rationale
(`scoped_walk.rs`) holds for the origin that actually fires. The abandoned probe costs **31 ms**, which is noise.

❌ Don't raise the cap. The general reasoning ("a 20 k subtree is small next to a 600 k volume") is sound but doesn't
apply here, because `$HOME` _is_ most of the volume.

## Why an unchanged pass rewrote 51,081 rows, and the only equality key that works

There was no diff at all: `rescore_rows` scored every folder in the subset and `apply_incremental` cleared each subtree
and re-inserted every row, never reading the stored value.

Comparing the app's live `importance-root.db` (last written 22:42) against a fresh full recompute over the same index
snapshot:

```
rows in the $HOME subtree:  51,081   (matches the log's 51,087)
  IDENTICAL signals blob:   51,021   (99.88%)
  IDENTICAL score:              17   (0.03%)
```

**This is the load-bearing result.** A diff on `score` finds 17 unchanged rows in 51,081, because `now_secs` advances 60
s per pass and `scorer::recency` moves every score by ~2e-6. A diff on `signals_json` finds **99.88%**, because
`FolderSignals` carries no clock (raw `mtime_secs`, counts, flags). The 60 genuinely-changed rows accrued over a
**1.5-hour** gap, so per-minute drift is far smaller.

Landed as the skip in `importance/writer.rs`'s `fate_of_stored_row`; the guardrail lives with it. Measured on the real
store, the same 51,081-row subtree: a range **read** costs **10 ms** where the old DELETE-plus-reinsert cost **550–620
ms**.

## What it cost, and what is still open

From the log's own timestamps (fallback line to completion line, 330 full-walk passes): **median 15.3 s, mean 20.1 s,
max 363 s, total 6,639 s** — **17.6% of the 10.5-hour log spent inside an importance pass**. Uncontended measurements
put the walk at 4.9 s and a 160,840-row write at 558 ms (`importance-measure`), so the extra ~10 s per live pass is
contention (six concurrent cargo builds on the same SQLite WAL). **That gap is not decomposed**; decomposing it needs
instrumentation in a running app under load.

Not cost drivers, checked and cleared: the importance `wal_checkpoint TRUNCATE` logs `(0 of 0 frames)` on all 612
occurrences, and Spotlight sampling is capped at 500 paths (`last_used.rs`) with no measurable difference between
`local` and `listing-only` full passes.

**Still open: the batch's WIDTH.** The writes are gone, but a `$HOME`-origin pass still walks 574,007 dirs and rescores
51,081 folders to discover that nothing moved. The cheap fix is available and unbuilt: `dir_stats.recursive_dir_count`
is already populated and exact (it reads **574,006** for `$HOME` against the 574,007 counted here) and is a single
indexed PK lookup, so a pass could ask "how much of the volume does this origin cover?" **before** choosing a walk,
replacing the 31 ms probe entirely.

What to do with an over-budget origin is David's call, and it is a semantic change that must be run against
`importance/evals/`:

- **Drop it** — a staleness the next full pass heals, the same trade `sanitize_incremental_batch` already makes.
- **Demote it** to "rescore the origin and its ancestors, not its subtree" — more honest, since a dotfile write in `~`
  genuinely cannot change any descendant's signals. This is the recommendation.

❌ Neither is a denylist: both key on measured cardinality, never on a path shape.

Until it lands, the `incremental rescore of '<volume>' updated N (of M rescored)` log line is the thing to watch — `M`
is the width, and it is why both numbers are logged.
