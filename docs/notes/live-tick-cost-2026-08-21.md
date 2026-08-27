# What a media live tick costs, and which half of it was the surprise

A live tick (`crates/cmdr-index/src/media_index/scheduler/live.rs`) fires at most once a minute per local volume, for as
long as the app is open, and on a machine whose churn is build output almost every one of them finds nothing to do. The
idle-cost effort proposed moving that walk behind the coverage gates, on one condition: measure the gates first, because
they may cost more than the walk being moved behind them.

They did, at David's folder count. The premise held, and the answer it pointed at was not the one the proposal
described.

## Method, and what these numbers are not

`crates/cmdr-index/src/media_index/scheduler/live_bench.rs`, `#[ignore]`d, release build, M1 Max, 2026-08-21:

```sh
cargo test -p cmdr-index --release --lib -- --ignored --nocapture live_tick_cost
```

Synthetic stores in a temp dir: a drive index of ten-component paths under `.claude/worktrees/*/target`, and an
importance store of N scored folders. ⚠️ **Every number here is WALL time against a warm page cache, never CPU.** Both
arms bottom out in SQLite reads, and `idle-cpu-attribution-2026-08-03.md` records what this repo has already paid for
reporting a syscall leaf as CPU. Read them as "what the tick waits for", and treat the split between the two arms as the
useful part rather than any single figure.

Also ⚠️ **a synthetic index is not a real one**: no fragmentation, no competing readers, one connection. The per-dir
walk cost in particular is a floor. The ratios survive that; the absolute microseconds are for this fixture.

## The two arms

**The scoped walk**, `walk_image_entries_in_dirs`, one `resolve_path` (an indexed query per path component) plus one
`list_children_on` per touched dir:

| touched dirs | walk     | µs/dir |
| ------------ | -------- | ------ |
| 100          | 2.47 ms  | 24.7   |
| 500          | 10.03 ms | 20.1   |
| 2,000        | 40.42 ms | 20.2   |
| 10,000       | 216.9 ms | 21.7   |

Flat at roughly **20 µs a directory**, so it's linear in the touched-dir count and depth is the multiplier (ten
components here). Nothing about it depends on whether the directory could ever enrich.

**The coverage gate**, `folder_scores` at N scored folders. Before, it ran `ImportanceIndex::above_threshold` directly,
which SQLite answers with an external merge sort over the whole weights table, then rebuilt an N-entry
`HashMap<String, f64>` and dropped it a moment later:

| scored folders | direct read | cache, cold | cache, warm | warm speed-up |
| -------------- | ----------- | ----------- | ----------- | ------------- |
| 1,000          | 464.5 µs    | 745.7 µs    | 12.5 µs     | 37×           |
| 10,000         | 4.81 ms     | 5.61 ms     | 1.17 µs     | 4,121×        |
| 90,308         | 46.14 ms    | 53.23 ms    | 2.83 µs     | 16,287×       |

**45–46 ms per tick at 90,308 folders**, which is about what walking 2,200 directories costs. So on David's root volume
the gate was not a cheap guard in front of an expensive walk; the two were the same order, and moving the walk behind
the gate without touching the gate would have left the tick's floor at 46 ms.

⚠️ **The gate only runs in the automatic scope.** `pass_coverage` skips the score read entirely under
`IndexScope::ChosenFolders`, which is the default. So the 46 ms is paid by users on `ByImportance` — including everyone
who had image indexing on before the scope setting existed, whom `scope_from_settings` starts there. In the default
scope the gate is free and the walk was the whole cost.

## What shipped

Both halves, because either alone leaves the other as the floor.

**The gate answers from the cache.** `coverage::importance_scores` is a subscription-backed cache of exactly this
question, with a memoized threshold projection, built for the file-status badge and already documented in
`media_index/CLAUDE.md` as the only way a UI path may read scores ("❌ Never `above_threshold` direct: it sorts every
scored folder, and per badge query that froze the app"). The live tick was reading it the forbidden way once a minute
per volume. It now takes the `Arc`: **2.8 µs warm.**

The trade is residency. The cache holds every scored folder's map for the process, plus the threshold projection, so at
90,308 folders that is on the order of 10 MB each where before the tick allocated a map that size every 60 seconds and
freed it. Steady bytes for allocator churn is the right side of that trade in a subsystem whose open questions are
`MALLOC_LARGE` and a mimalloc balloon (`idle-cpu-attribution-2026-08-03.md` § Still open), and the badge path already
pays it whenever a user opens a folder with the feature on.

**The walk runs only over directories that could enrich.** The tick filters its touched dirs first, at **0.03 µs a dir**
(a prefix check against the override list plus one hash lookup):

| touched dirs | filter   | µs/dir |
| ------------ | -------- | ------ |
| 100          | 14.3 µs  | 0.143  |
| 500          | 14.6 µs  | 0.029  |
| 2,000        | 61.0 µs  | 0.030  |
| 10,000       | 312.0 µs | 0.031  |

When nothing survives the filter, the tick returns before opening the index, before loading `media_status`, and before
spawning a writer. **A ~650× drop per ineligible directory**, and on a build-churn machine that is the whole tick.

## The trap, and how it is held shut rather than written down

Filtering the walk alone deletes data. `enrich_and_gc_scoped` GCs stored rows that are in `GcScope::TouchedDirs` and
absent from the walked set, so a directory dropped from the walk but left in the GC scope loses every OCR text, Vision
tag, and CLIP embedding it holds, against `media_index/CLAUDE.md`'s "uncovered rows STAY".
`coverage::patch_touched_dirs` has the same shape one step over: it replaces each dir it is handed with a count taken
from the walk, so a dropped dir would be replaced by zero.

One filtered set now reaches all three, and the walk hands it back as a `WalkedDirs` token only the walk can mint. The
GC scope and the counts patch take nothing else, so "filter the walk, keep the old GC scope" is a **compile error**
rather than a silent deletion.

Two regression tests in `scheduler/live_tests.rs` cover the behavior anyway
(`a_live_tick_keeps_every_row_in_a_dir_its_coverage_filter_dropped`,
`a_live_tick_leaves_the_cached_counts_of_a_dir_it_filtered_out_alone`), and **both were watched failing** under exactly
that mutation while the sets were still two plain `HashSet`s. They stay as the anchor for what the type is protecting: a
type stops today's mistake, and the tests say what the mistake would have cost.

The filter's own correctness is a proof rather than a promise. `NetworkEnrichConfig::may_cover_within` also keeps a
directory that an override entry names something at or under, which is the only way `covers` can answer differently for
a file than for its parent — an entry that IS the file's path. A proptest holds
`local_should_enrich("{dir}/{name}") ⇒ local_dir_may_be_covered(dir)` across overrides, scores, and both scopes; drop
that term and it fails, shrinking to `always = ["/a/b"]`, `dir = "/a"`, `name = "b"`, now a checked-in seed.

## Two consequences, taken deliberately

- **A tick's GC and its counts patch now reach only the directories it walked.** A vanished file's row in an uncovered
  directory waits for the next full pass, which still whole-store GCs it, and an uncovered directory's cached eligible
  count goes stale until a full pass refills it. Rows staying is what "narrowing a setting deletes nothing" already
  asked for; the stale count is a real (small) cost, and it lands on folders whose badge reads "0 of N indexed" anyway.
- **Excluded folders are still walked.** The privacy veto stays per image. Folding it into the dir filter would change
  which rows the tick may GC, and the retro-delete already empties an excluded folder, so there is nothing there for a
  tick's GC to find. Not worth a second dimension in the safety argument.

## What this does NOT settle

- **How many dirs a real tick touches.** The bench gives cost per dir; nobody has counted the distribution on a live
  machine. The 3,704 distinct rescan anchors in eight hours from `idle-cpu-attribution-2026-08-03.md` are the closest
  thing, and they're a different unit.
- **Whether the tick's remaining floor matters.** `load_statuses` still reads every stored `media_status` row into a
  `HashMap` on any tick that survives the filter. Unmeasured, and it scales with the enriched-image count rather than
  with the churn. That's the next thing to measure here if the tick shows up again.
- **Anything about CPU.** See the method warning. Nothing in this note is a CPU claim.
