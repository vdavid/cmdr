# cmdr-search details

The drive search's core. The host side (routing one volume per search, loading and caching arenas, live runs over
unindexed ground, history, AI translation) is `apps/desktop/src-tauri/src/search/DETAILS.md`. Why this is a crate, and
the milestones that built it: `docs/specs/search-crate-plan.md`.

## The seam (`seam.rs`)

Matching never moved into SQL: SQLite's `LIKE '%q%'` measured 1–3 s on 5 M rows, so the scan has always been an
in-memory pass. A source HANDS OVER a built arena and answers the two questions it can't: which entry ids a scope path
names (`SearchRows::resolve_scope`), and how big a folder is (`DirSizeSource::in_range` before the scan,
`DirSizeSource::of` for the returned rows). Four calls per query, nanoseconds against a 100–400 ms scan; a per-row trait
would be millions of virtual calls and, worse, a `dyn` iterator can't `par_chunks`.

- **Two traits** because they read different stores (desktop: `entries` vs `dir_stats`) and, in a browser host, come
  from different producers.
- **The scope path pipeline keeps one order**: `~` (`query::expand_tilde`, against the context's home), then the host's
  `realpath`, then the mount strip (`query::to_index_relative`, in `resolve_scope`), because an index-relative path
  can't be canonicalized. So `search_covered` takes `(original, canonical)` pairs: the original is what `unresolved`
  reports, the canonical is what is looked up.
- **`ResolvedScope`** (`Whole`, `Ids`, `Nothing`) replaced the `[i64::MIN]` sentinel; the engine takes it as a
  parameter. `SearchQuery::include_path_ids` stays on the wire type, read only by the test wrapper `engine::search`.
- **Errors are typed**: `SearchError::Query` (an invalid pattern or exclusion, or too broad) and
  `SearchError::SourceUnreadable` (`SourceRead::Scope`, `SizeRange`, `SizeFill`). The desktop's live run maps the second
  to `IndexUnreadable` with a sentence a person can act on.
- **`NameRules`** (`context.rs`) carries the default case rule, whether a pattern is NFD-normalized, and the `NameFold`
  an exclusion keys on. `NameRules::host()` is the `cfg`-derived desktop answer, pinned to the old answers by
  `the_host_rules_are_what_this_machine_always_answered`; `MACOS` and `LINUX` let any machine run either platform's
  rules (the host's goldens do, both ways).

## The public surface is capped

`index-crate-isolation` pins the surface at extraction (2026-11-04): 32 root promises (the nine modules plus the flat
re-exports), nine public modules, 86 items inside them. An item is `pub` only because the host reaches it:

- **The seam and context**: everything in `seam.rs` and `context.rs` the host composes.
- **The live walk's judge** (host-side, it consumes `cmdr-index`'s walk):
  `matcher::{CompiledQuery, Candidate, Evaluator, CompileError}`,
  `excludes::ExcludeRules::{from_query, excludes_walked}`, `engine::{home_relative_parent, derive_icon_id}`.
- **Routing, MCP, and IPC**: the wire types,
  `query::{parse_scope, parse_size, format_size, format_timestamp, summarize_query, summarize_query_for_diagnostics}`,
  `types::MAX_RESULT_ROWS`.
- **The host's loader and weights**: `arena::*`, `ranking::ImportanceWeights` and its map verbs.
- **The host's tests and benches**: `engine::{search_ranked, Ranked, DirSizes, reconstruct_path_from_index}`; the
  `testing`-gated fixtures and wrappers count apart.
- **A live run's reporting** (raised 2026-11-04 to 33 / 10 / 104, when it joined the crate): `run::*`, the registry
  value, `ResultStream` and its `RunSink` facts, the run-state DTOs, the terminal report's inputs, and `BATCH_ROWS` /
  `BATCH_INTERVAL` (the host's pump and tests). § "A live run's reporting".

Raise a ceiling only when the wider surface is genuinely better, said in the commit and here.

## A live run's reporting (`run.rs`)

A live search answers over time, so its reporting is a seam of its own. The crate owns what a run SAYS; the host owns
what it walks (desktop: `cmdr-index`'s cover walk, pumped by `apps/desktop/src-tauri/src/search/live.rs`).

- **`SearchRuns`** is the registry as a value: register (a `RunOrigin::Dialog { order }` supersedes every dialog run of
  a smaller or equal order, and a late-arriving earlier one starts superseded; an `Agent` run neither supersedes nor is
  superseded), cancel one, cancel all, and `cancel_dialog_runs_except`. The host holds one per process. Superseding is
  not cancelling: a superseded run stops reporting and its walk keeps filling the index (Decision 11).
- **`ResultStream`** batches (at most `BATCH_ROWS` rows per progress fact, at most `BATCH_INTERVAL` between them),
  dedupes the walk against the index half by path hash, caps rows while the count keeps rising, sums both halves'
  exclusion drops, and keeps the run's last phase so the terminal fact repeats what the run WAS doing. Its clock is
  `cmdr_fs::rt`.
- **`RunSink`** hears four facts (`RunProgress`, `RunEnded` for complete or cancelled, `RunFailed`). They carry the
  event payloads field for field; the host's envelopes (desktop's four `tauri_specta::Event` structs) wrap them, so no
  crate type shares an event's name and `bindings.ts` names the same types.
- **`CoverageFacts` → `SearchRunCoverage`** is the one terminal report, built inside `ResultStream::finish` for every
  ending: the abandoned-ground fold (this walk's flag OR the index's durable `abandoned` list), `abandoned_locations`
  (places, grouped by parent), the three unreadable lists kept apart, and the unresolved-scope rule (only a `Completed`
  walk clears a scope it took as a frontier root). A cancelled run is relabeled `Cancelled` after the report is built.
  ❌ No separate "fully covered" report: that run is `CoverageFacts::new(NothingToWalk, Covered, ..)`.

## The arena row (`arena.rs`)

The arena holds one `SearchEntry` per file on the volume — 6,045,549 rows on David's boot disk — and it stays resident
for as long as someone is searching, so **what a row costs IS the app's peak footprint**. That makes the row's size a
design constraint, and `crates/cmdr-search/src/arena/memory_tests.rs` pins it at 40 bytes.

**`size` and `modified_at` are `OptU64`, not `Option<u64>`.** A `u64` uses every one of its bit patterns, so `Option`
has no niche to hide `None` in and Rust adds a whole discriminant word: the two fields were 32 of the struct's 56 bytes
for two values needing 8 each. Sentinel-encoding them took the row to 40 and the loaded root arena from 689.5 MiB to
597.2 MiB, with no measurable change to scan latency (`docs/notes/search-arena-row-2026-08-06.md` has the before/after
and the A/B method).

- **`u64::MAX` is the absent marker and cannot collide.** Both values come out of SQLite `INTEGER` columns, which are
  SIGNED 64-bit, so the index can't store or return anything above `i64::MAX` — the sentinel is outside the
  representable range by construction, not merely an implausible value inside it. (It's unreachable physically too: 16
  EiB is twice APFS's own per-file ceiling.)
- **❌ Never collapse `None` into `0`.** `logical_size` is NULL on every 2nd+ name of a hardlinked inode — the index
  counts an inode's bytes once and stores NULL on the rest, 934,793 of 6.0 M rows here — so the two mean different
  things, and conflating them would change what folder totals and size filters report on a hardlink-heavy tree.
  Symmetrically, a real zero-byte file must not read back as "unknown". `index.rs`'s
  `a_hardlink_deduped_row_loads_back_as_unknown_size` pins both directions.
- **❌ Never compare against the sentinel at a call site.** `OptU64`'s inner `u64` is private and `get()` is the only
  way in, so the encoding is invisible outside the type; its `Debug` prints as the `Option` it stands for, so a log line
  never shows a bare `18446744073709551615`.
- **The rows are private**: `SearchIndex`'s accessors (`len`, `id`, `parent_id`, `name`, `is_directory`, `size`,
  `modified_at`, `index_of_id`, `generation`; the arena-snapshot spec's Contract B) are the whole read surface, and
  `ArenaBuilder` (push in id order, then `finish` or the loader's `merge`) the only way to make one, so a mapped arena
  (GitHub #114) can replace the storage without touching a caller. The readers are `engine.rs` (the `Candidate` build,
  the `sortBy` key, `build_result_entry`) and `ranking.rs` (the recency key). `Candidate`, `CoveredEntry`, and
  `SearchResultEntry` keep plain `Option<u64>`: they're per-result or per-batch, not per-row, so the 16 bytes buy
  readability there instead of costing memory.

**The next lever is the names arena.** `id_to_index` is already gone (the "no `id_to_index` map" decision below); the
measurement that ranked the two is in `docs/notes/size-only-subtrees-rejected-2026-08-06.md` § The search arena.

## Finding a row by id

`entries` is sorted by `id`, ascending, and `SearchIndex::index_of_id` binary-searches it. `id` is the `entries` table's
`INTEGER PRIMARY KEY`, hence its rowid, so a table scan already yields that order; the loader still says `ORDER BY id`
so the guarantee is written down rather than inherited, `ArenaBuilder::push` and `ArenaBuilder::merge` debug-assert it,
and the fixture helper (`index::arena_of`) sorts its rows first.

**Decision: no `id_to_index` map.** A `HashMap<i64, usize>` over the same rows cost ~143 MB on a 5.39 M-row boot index
(5.39 M entries round up to 8,388,608 hashbrown buckets at 17 bytes apiece) and bought a ~20 ns lookup instead of a
~23-probe one. The callers are the four parent-chain walks in `engine.rs`, a few hundred probes per search, which is
microseconds either way against a 100-400 ms scan. ❌ Don't reintroduce it without measuring the ranking phase first.

**Gotcha**: this is why an out-of-order fixture is now a bug rather than a curiosity. `ranking/tests.rs` built rows in
spec order, which the hash map tolerated and a binary search silently answers wrong on.

## Directory size filters and `sortBy`

A directory's size lives in `dir_stats`, not in the search arena, so the parallel scan can't judge it on its own. The
answer is `engine::DirSizes`: `seam::dir_sizes_for` reads the passing set (`DirSizeSource::in_range`; on the desktop
`IndexStore::dir_sizes_in_range`) BEFORE the engine runs and hands it in, so the scan filters directories on size like
everything else.

**Why the order matters.** Applying it afterwards, to the ranked top-k, answers from a recency-ordered sample instead of
from the drive: `sizeMin: 50 GB` over a real machine returned four folders and missed a 1.7 TB `~/Library`, because a
folder nobody touched today loses the importance-boosted recency ranking against hundreds of thousands of
freshly-touched ones long before anything reads its size. Filtering inside the scan also makes `total_count` exact by
construction, which is why count-only no longer needs a correction pass (no over-fetch, no returned directory rows, no
subtraction).

`dir_sizes_for` builds the map only for a query that filters or sorts directories by size, because it's a full scan of
`dir_stats`. That table is deliberately NOT indexed on `recursive_logical_size`: the index would be rewritten by every
rollup update on the indexing hot path, to speed up a query that runs only when someone asks about sizes. The scan is
cheap enough to keep it that way — over a 574,927-row `dir_stats`, a 50 GB floor returns 30 rows in 33 ms and the
unbounded form (what `sortBy: "size"` with no size filter asks for) returns all of them in 40 ms (verified on the
shipped `index-root.db` via read-only `sqlite3`, 2026-08-06).

❌ **A failed read must fail the search.** The engine reads a missing map as "no directory size filter to apply", so
falling back to `None` on a DB error would answer with every matching directory regardless of size — a wrong answer
wearing a right one's clothes.

`sort_by` (`SearchSort`) rides the same map. `Relevance` is the default and what `ranking.rs` owns; `Size` and
`Modified` REPLACE that ranking rather than reordering its top-k, because "the biggest matches" means the biggest that
exist. A directory is compared on its recursive size and a file on its own, so one enormous file can outrank every
folder around it. Unknown keys sort last in both directions (`sort_indices`), ties break on entry id, and the top-k is
taken with `select_nth_unstable_by` so a broad query pays a partition rather than a full sort.

**Over live-walked ground this filter can't apply**: a walked directory has no `dir_stats` row yet, which is Accepted
difference 5 (§ The accepted differences below). The index half is exact; the walked half doesn't match directories on
size.

## The compiled query (`matcher.rs`)

The per-entry predicates (name pattern, type, size, date) are a `CompiledQuery`, not part of the arena scan. The reason
is the second evaluator: a search over ground the index doesn't cover walks it live and matches the entries the walk
emits (`docs/specs/unindexed-search-plan.md` Decision 3). If those entries were judged by a second copy of the rules,
the same query would answer differently depending on whether the drive happened to be indexed, which is the one thing
that plan forbids. So both paths call `CompiledQuery::matches`, and the module owns the rules that break silently when
duplicated: the case-folding resolution (the scope filter and the ranker read it back rather than deriving it again) and
the NFD normalization of the pattern. Both come from the volume's `NameRules` (`context.rs`), a value the host passes
in: `NameRules::host()` is the `cfg`-derived desktop answer, and `MACOS` / `LINUX` let a test run either platform's
rules anywhere (`the_other_platforms_goldens_hold_under_its_rules`).

It isn't in `cmdr-index` because `index-crate-isolation` forbids that crate from depending on the search side. That
constraint is also why `Index::cover` hands back `CoveredEntry` batches instead of taking a match callback, and why the
walk's half of the adapter (`live/walked.rs`: `matches_walked`, `covered_name`) sits beside `WalkJudge`.

**What stays outside, and why:**

- **Directory size filters.** A directory's size isn't in the entries table; it's written over the ranked results from
  `dir_stats` (`seam::dir_sizes_for` before the scan, `fill_dir_sizes` after it). So the matcher's size predicate is
  files-only on both paths, and a directory passes a size filter untouched. Dropping directories in the matcher would
  drop them before the only place that knows their size.
- **The scope filter.** Include roots are arena entry ids and the exclusion check is an ancestor walk through
  `id_to_index`, so neither means anything for an entry that isn't in the arena. The live path applies the same policy
  against a walked entry's own path instead.

**A glob's `.` crosses a newline; a user's regex keeps standard semantics.** Filenames may contain newlines, and a
glob's `*` and `?` mean "any characters" and "one character", so `glob_to_regex` prefixes its output with `(?s)`. It
lives in the translation rather than at each `RegexBuilder` — a property of what a glob MEANS, not of one caller — so a
new call site can't get it wrong; two of them disagreeing is how newline-named files went unfindable in the first place.
It composes with `RegexBuilder::case_insensitive` (an inline `(?s)` sets that one flag and leaves the builder's alone),
pinned by `matcher::tests::a_case_insensitive_accented_glob_still_crosses_a_newline`.

A user-typed regex never goes through `glob_to_regex` and deliberately keeps the standard rule: someone writing one
expects `.` to stop at a newline and `(?s)` to be their own call. ❌ Don't "unify" the two — the asymmetry is the
decision, and the line it's drawn on is **authorship, not notation**. Which is what settles the two neighbours:

- **The AI mappings emit `PatternType::Regex` matched against FILENAMES, and carry `(?is)` inline.** A mapping table
  isn't an author, so the reasoning that protects a person's own regex doesn't reach them; they take the flags a
  filename matcher needs. `keyword_mapping::merge_keyword_and_type` is the single place that prefixes them, so ❌
  `type_mapping`'s `TypeFilter.pattern` entries carry no inline flags of their own.
- **The file viewer takes a third position: it REFUSES.** `file_viewer/search_matcher.rs` rejects a pattern that would
  need to cross a newline with a typed `MultilineNotSupported`, because its streaming model is line-at-a-time. Different
  problem (file CONTENTS, not names), already explicit in its own code.

**The broad-query guard is per evaluator.** An arena's cost is known before the scan, so a query with no narrowing
predicate is refused only above `ARENA_BROAD_QUERY_CEILING` rows; below it, "show me everything, by recency" is a fair
ask. A live walk has no such bound (an unknown filesystem, over a network in the worst case), so it refuses outright. ❌
Don't collapse the two back into one row-count rule: an unindexed volume's arena holds zero rows, so a count-based
ceiling is precisely the guard that can't fire on the path that needs it.

**Two asymmetries survive the shared matcher**, both bounded and both deliberate:

- A `CoveredEntry` carries the entry's OWN size, before hardlink dedup, because that's what a listing shows. The index
  stores the deduplicated size, so a 2nd+ hardlink to one file is sizeless there. A size bound therefore keeps it in a
  live result and drops it from an indexed one. The live answer is the truthful one, so it stays.
- A walked entry's name is derived from its path (`covered_name`), byte-identically to how `insert_visitor` derives the
  row name it writes. For the trait walk (`network_scanner/cover_scan.rs`) the row name is the listing's own `name`
  field while the path is its `path`, so the two agree only as long as a `Volume` backend reports a path whose last
  component IS that name. ⚠️ A backend that broke that would make live and indexed results disagree on the affected
  names with nothing failing loudly.

## Importance ranking (`ranking.rs`)

Search ranks interesting files toward the top by blending a result's match quality with its parent folder's importance
weight (the first consumer of the `crates/cmdr-index/src/importance/DETAILS.md` subsystem). The ranker is a pure module;
`engine.rs` stays pure by receiving importance as DATA (a prebuilt weight map), never querying a DB.

### The blend: quality bands, importance within a band

The load-bearing requirement is that **match quality dominates**: an exact/prefix name match must beat a weaker match no
matter how important the weaker match's folder is. We get this BY CONSTRUCTION with a lexicographic sort:

1. **Match-quality band first** (`MatchQuality`: `Exact` > `Prefix` > `Other`). Importance is applied only WITHIN a
   band, so it can never lift a result across a band boundary — "exact filename in a boring folder beats fuzzy match in
   Documents" holds for any weight. The dominance property is pinned by
   `exact_match_beats_fuzzy_match_regardless_of_importance` (written first against a deliberately-wrong blend that folds
   importance into the band comparison, seen to fail, then fixed).
2. **Importance-boosted recency within a band**: the key is `recency * (1 + IMPORTANCE_BLEND_COEFF * weight)`. A modest
   multiplicative nudge (`IMPORTANCE_BLEND_COEFF = 0.5`, a named future tunable): at max weight `1.0` a result's recency
   key scales by `1.5`, enough to win a same-quality tie against a result up to ~half a recency-order newer, never
   enough to matter across bands. With weight `0.0` the multiplier is exactly `1.0`, so within-band order collapses to
   pure recency.
3. **Id-ascending final tiebreak** for run-to-run determinism.

**A file takes its parent folder's weight; a folder takes its own.** The engine reconstructs the folder's absolute path
and looks it up in the weight map. Absent a weight (unscored, floored, or missing DB), the lookup is `0.0` — neutral,
never a penalty.

**Only a wildcard-free plain query has a quality gradient.** The `stem` fed to the ranker is the raw pattern only when
it's a glob with no `*`/`?` (the auto-wrapped `*stem*` case); a wildcard glob or regex yields an empty stem, so every
result lands in the `Other` band and recency alone orders — unchanged from before this feature (and matching how those
patterns behaved). On macOS the stem is NFD-normalized like the matcher's pattern, so it compares against the arena's
NFD filenames.

### The degradation contract

**Absent importance data, ranking equals today's behavior.** When the weight map is empty (offline volume, fresh
install, disabled indexing, a purged `importance.db`, or a recompute that hasn't run yet), every weight is `0.0`, so the
within-band multiplier is `1.0` and the sort is pure recency within each band — byte-for-byte the pre-importance
ordering. Pinned by `empty_weights_within_band_is_pure_recency` and `empty_weights_and_no_stem_is_pure_recency`. The
engine also takes an empty-map fast path (skipping the per-result parent-path reconstruction entirely).

### Scan cost: allocations per query, never per row

A query costs a fixed few thousand allocations whatever the arena size, and nothing per scanned row. Allocating per row
meant 1–1.3 M allocations for a query matching nothing on a 5.2 M-row arena, 93 M for a one-letter query, and a 1.4×
penalty under the system allocator. Evidence and before/after numbers:
`docs/notes/performance/search-loop-allocations-2026-09-27.md`. Three things hold it:

- **Each scan chunk clones the compiled query and the scope filter** (`SCAN_CHUNK_ROWS`, 32,768 rows). A cloned
  `regex::Regex` gets a cache pool of its own, and the one thread scanning the chunk becomes its owner, so every
  `is_match` takes the pool's lock-free path. ❌ Don't share one `Regex` across the workers: all but one then go through
  the pool's eight mutex-guarded stacks (`regex_automata::util::pool::Pool`, 0.4), and a worker that loses a `try_lock`
  builds and drops a whole cache. That lock traffic, not the allocator, was most of the cost: 64 → 10 ms for a no-match
  query under mimalloc. `ExcludeRules` and `ScopeFilter` share their sets behind `Arc`s so the clone is cheap.
  `collect()` over the chunks still preserves arena order.
- **Each scan chunk memoizes the name-exclusion walk** (`AncestorVerdicts`: folder id → "a name rule excludes this
  folder or one above it"). A match walks up only until a folder the chunk already judged, then records the answer for
  every folder it passed, so each folder is judged once per chunk rather than once per match under it. That halved a
  one-letter query (~300 → ~175 ms on a 5.6 M-row arena). The memo is per chunk, ❌ never shared across workers: a
  shared map is a lock on the hot path.
- **`ExcludeRules::excludes_dir_name` folds an ASCII name on the stack** (`with_ascii_folded`), byte-identical to `fold`
  (pinned by `the_ascii_fold_agrees_with_the_general_one`). A non-ASCII or over-long name still takes `fold`'s `String`s
  (~1% of directory names on a real boot volume), now once per folder per chunk thanks to the memo. ❌ Don't make that
  path allocation-free by folding char by char here: it would re-derive `normalize_for_comparison`'s NFD and case rules
  (final sigma included), the fork `CLAUDE.md` forbids.
- **Ranking hashes a folder's path off a stack buffer**, and its memos are few and warm (§ Ranking cost below).

Pinned per query, not per row, by `engine/tests/allocations.rs` over `test_support::allocations_on_pool`, which counts a
dedicated rayon pool's threads. ⚠️ The regex half isn't reliably red there: a debug build rarely contends the pool's
lock. `bench::bench_query_allocations` measures it on a release build against a real index, split into full search,
scan, and scan without excludes.

### Ranking cost: the top-k pass (`rank_decorated`)

Ranking runs once per MATCHED entry, and a one-letter query matches millions (4.6 M of 6.96 M on a real home dir), so
this pass — not the rayon scan — was the search's dominant cost: 11.9 s for that query, ~75% of it the importance blend.
Four things keep it bounded, all order-preserving:

- **A per-task `folder_id → weight` memo.** Matches cluster hard by folder, and a weight lookup means walking the
  folder's parent chain. `map_init` builds one memo per rayon split, so `RANK_MIN_SPLIT` (8,192 matches) keeps the
  splits few and the memos warm.
- **The folder path is HASHED, never built.** `engine::hash_path_from_index` streams the parent chain's components into
  `cmdr_fs::path_hash::PathHasher` (incremental FNV-1a + the same splitmix finalizer), so the `String` that existed only
  to be hashed and dropped is gone, and the components sit in a 64-deep stack buffer (a deeper chain spills the rest to
  a `Vec`). It's byte-identical to `hash_path(reconstruct_path_from_index(..))`, pinned by
  `streamed_hash_matches_whole_path_hash` (and `…_past_the_inline_depth`) — a drift there would silently read the wrong
  weight, with no symptom beyond subtly worse ranking.
- **`classify_match` allocates nothing** for a case-sensitive compare or an ASCII name+stem (nearly every real
  filename). Non-ASCII case-insensitive names still take the `to_lowercase()` path, so Unicode folding (final sigma and
  friends) is unchanged.
- **Top-k, not a full sort.** `select_nth_unstable_by` partitions to the caller's limit, then only that prefix is
  sorted. Safe because `RankKey::cmp_best_first` is a TOTAL order (the final tiebreak is the unique entry id), so the
  top-k set and its order are unique — an unstable partition returns exactly what the full stable sort did. The
  count-only directory pass passes `usize::MAX`: it needs every matching directory, since the caller subtracts the
  out-of-range ones from the volume total.

Measurements and the before/after table: `docs/notes/search-latency-2026-07-28.md`. The harness is `bench.rs`
(`#[ignore]`d; it can run against a real `index-*.db`). Two of its benches answer the arena-SHAPE questions rather than
the ranking ones: `bench_arena_bytes` (what a loaded arena costs, in heap bytes and in process RSS) and
`bench_arena_scan` (the count-only pass, best-of-N, with and without the size/date filters that read an entry's `OptU64`
fields). A third, `bench_query_allocations`, asks the way the dialog and MCP do (excludes on, optional real weights) and
reports allocations as well as latency. ❌ Don't reach for `bench_real_index` to answer "did the row's shape make
scanning slower": its queries set no size or date bound, so the matcher skips those predicates, and one run per pattern
is swamped by machine noise.
