# cmdr-search details

The drive search's core. The host side (routing one volume per search, loading and caching arenas, live runs over
unindexed ground, history, AI translation) is `apps/desktop/src-tauri/src/search/DETAILS.md`. Extracted under
[#302](https://github.com/getcmdr/cmdr/issues/302) (`docs/specs/search-crate-plan.md`, now retired).

## Why a crate

The search core lived in the app crate, braided with the index handle, SQL, Tauri events, app statics, the home folder,
and `cfg!(target_os)`. As a crate: (1) desktop tests the core over an in-memory source with no SQLite, the purity rules
are a compiler-enforced boundary, and the platform name rules are a value (macOS rules testable on Linux CI); (2) the
browser demo runs the real search over its in-memory tree (`crates/cmdr-listing-wasm/DETAILS.md` § Search).

- **The name**: it's the drive search (file names over a volume). Media and OCR search stay in `cmdr-index`'s
  `media_index` read API, which this crate doesn't touch.
- **Decision: the index-backed source stays in the app** (`apps/desktop/src-tauri/src/search/index_source.rs`, the
  `listing_host/` pattern): it's SQL, threads, and the app's arena lifecycle, and the demo's rows come from its own
  tree, held identical by `source_equivalence.rs`.
- **Decision: the recent-list rules are their own crate**, never in here: `crates/cmdr-recents/DETAILS.md`.
- **Moved types keep their `bindings.ts` names**: specta names a type by its struct, not its module, and `specta` is
  pinned to the app's exact pre-release (two in one graph and the `Type` impls stop satisfying `tauri-specta`).

## The invariants

Each is what a review of this crate checks first.

1. Desktop answers, order, counts, honesty fields, and events match the host's goldens
   (`apps/desktop/src-tauri/src/search/execute/tests/goldens.rs`); a moved golden is a bug, except a change registered
   below.
2. No `tauri`, `tauri-specta`, `cmdr`, or `cmdr-index` in the tree (`index-crate-isolation`); the crate builds for
   `wasm32-unknown-unknown` (`desktop-rust-listing-wasm` links and smoke-runs it).
3. The scan runs over a concrete arena inside the crate; every source call is per query, never per row.
4. One matcher, one exclusion set, one `NameRules` per run: no case folding or normalization derived outside it.
5. No statics, and no ambient clock, zone, home, platform, or runtime (`clippy.toml` bans the reads; `NameRules::host()`
   is the one `cfg`, and it's the host's to call).
6. `SearchEntry` is 40 bytes, the arena is id-sorted with no id map, and `OptU64` is the only read of size and date.
7. Every source read error fails the search as `SourceUnreadable` (a live run's `IndexUnreadable`); a scope is a typed
   `ResolvedScope`, never a sentinel id.
8. The browser demo answers every search command through the crate; its TS answers none.
9. One folder-size truth per host (desktop's `dir_stats`, the demo's fold); search never rolls up sizes of its own.
10. One recent-list rule and one history key for every host and every history feature.

## Deliberate behavior changes

Desktop behavior moved on purpose in exactly these places, each a red-first commit; the goldens pin the result.

1. **`home_relative_parent` matches home on a `/` boundary**: a bare prefix strip turned `/Users/david/x` into `~id/x`
   for a home of `/Users/dav`.
2. **Days are local**: "today", every AI time range, the MCP `modifiedAfter` / `modifiedBefore` dates, the AI's date
   chips, and the dialog's own date filter start at the user's local midnight, never UTC's. Every date the app shows a
   person or the model (`summarize_query`, `LocalDays::format_date` and `format_datetime`) reads on that same calendar
   (`local_days.rs`).
3. **A bad exclusion glob is a `Query` error naming it**, never silently dropped.
4. **A failed folder-size fill, size-range read, or scope resolution fails the search**, never a sizeless row, "no
   filter", or an "unresolved" scope; a live run reports it as `IndexUnreadable` with conversational copy.
5. **On a Mac a pattern matches a name stored in either Unicode normal form** (#955). APFS keeps whichever form the
   writer used, so an NFC `café-terrace.jpg` was invisible to every typed pattern once the pattern was forced to NFD;
   the macOS goldens moved: `accented_folder` from zero rows to the folder and its file, and `accented_nfc_pattern` /
   `accented_nfd_pattern` gained the NFC `Photos/résumé.txt`.

## Performance across the crate edge

- **Nothing hot crosses it**: the scan's chunk closure, `CompiledQuery::matches`, and the arena accessors all sit inside
  this crate, and release runs thin LTO. The accessors keep `#[inline]` anyway, for the mapped-arena facade (#114).
- **The load is the one per-row crossing**: the host's SQL loader pushes about six million rows into `ArenaBuilder`, so
  `push` is `#[inline]` (the dev and `e2e` profiles run without LTO). The extraction measured the scan, allocations, and
  load within noise of the baseline: `docs/notes/search-crate-baseline-2026-11-04.md`.
- **In the browser, rayon runs inline**: rayon-core's `default_global_registry` falls back to a one-thread pool on the
  calling thread when threads are unsupported (rayon-core 1.13.0 `registry.rs`, read 2026-11-04), so `par_chunks` runs
  on the page's thread; `smoke.mjs` proves it doesn't trap. Its size cost: `crates/cmdr-listing-wasm/DETAILS.md` § The
  build.

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
- **Decision: the seam sits ABOVE the engine, never beside it.** `seam.rs` resolves and orchestrates, `engine.rs` scans,
  so the scan's own input and output types (`ResolvedScope`, `DirSizes`, `Ranked`) live in `engine.rs` and the seam
  imports them. Likewise `context.rs` aggregates `NameRules` (a leaf) and ranking's weights, and both parent-chain path
  walks live in `src/arena/paths.rs` under ranking and the scan. Why: a type the lower layer needs but the upper one
  defines welds the two into a circle, which `module-cycles` fails on (`scripts/check/checks/DETAILS.md` § "Rust module
  cycles").
- **Errors are typed**: `SearchError::Query` (an invalid pattern or exclusion, or too broad) and
  `SearchError::SourceUnreadable` (`SourceRead::Scope`, `SizeRange`, `SizeFill`). The desktop's live run maps the second
  to `IndexUnreadable` with a sentence a person can act on.
- **`NameRules`** (`name_rules.rs`, a private leaf promised at the crate root) carries the default case rule, whether a
  pattern meets names in either Unicode normal form, and the `NameFold` an exclusion keys on. `NameRules::host()` is the
  `cfg`-derived desktop answer, pinned to the old answers by `the_host_rules_are_what_this_machine_always_answered`;
  `MACOS` and `LINUX` let any machine run either platform's rules (the host's goldens do, both ways).

## Rows held in memory (`memory.rs`)

`MemorySource` is the seam's second answer, with no database: rows a caller hands in (path below the volume root, kind,
size, date), any order, every folder listed. It refuses rows that can't form a tree (`MemorySourceError`: malformed,
duplicate, an unlisted parent, a file as a parent) rather than guessing.

- **Ids follow one defined order**: `ARENA_ROOT_ID` for the root, then depth-first (a folder's whole subtree before its
  next sibling), children by the byte order of their names. Every sort breaks ties on the id and each result row carries
  it, so equal answers need equal ids. The desktop's goldens insert their corpus in this order, and
  `apps/desktop/src-tauri/src/search/execute/tests/source_equivalence.rs` holds the two sources to identical arenas and
  identical answers on every golden query, under both platforms' rules.
- **A scope lookup folds through the volume's `NameRules`** (a `(parent id, folded name) → id` map built once), which
  under `NameRules::host()` is the fold the drive index keys `name_folded` on. ❌ Never `normalize_for_comparison`
  directly: its browser branch is the identity, which would disagree with a host that presents as a Mac. Two names one
  fold makes equal: the first in byte order answers. Under the OTHER platform's rules a respelled scope differs from the
  index on purpose (the index folds with the rules it was written under), so the equivalence test checks respellings
  under the host's rules only.
- **Folder sizes aren't its job**: a host pairs it with its own `DirSizeSource`, keyed by `MemorySource::id_of`. Tests
  use `testing::FolderSizes`.
- **Its arena never catches up** (generation 0): a host rebuilds it when its tree changed.

## Recent searches (`history.rs`)

`HistoryEntry` is what the dialog records on "Open in pane", and its `RecentItem` impl is the key `cmdr-recents`' list
rules dedupe on, so every host's list (the desktop's file, the browser demo's session list) dedupes alike.

- **The canonical key** (compare-time only, never persisted):
  `mode | normalized_query | filters | scope | case_sensitive | exclude_system_dirs`. Same key = same search; the newest
  copy wins. Six segments against the desktop Selection dialog's four, pinned by
  `key_carries_scope_and_exclude_system_dirs`.
- **Shared with Selection**: `HistoryMode`, `HistoryFilters`, and the key helpers (`normalize_query`,
  `filters_fingerprint`, `flag`); the entry structs stay separate. Sharing the helpers is what keeps a newly-added
  filter field from reaching one key and not the other. If the two mode sets ever fork, copy the types rather than
  widening the share.
- **Ids are the host's**: `cmdr_recents::add_to` takes a `fresh_id` minter, because minting reads an RNG or a clock.

## The public surface is capped

`index-crate-isolation` pins the surface at extraction (2026-11-04): 32 root promises (the nine modules plus the flat
re-exports), nine public modules, 86 items inside them. An item is `pub` only because the host reaches it:

- **The seam and context**: everything in `seam.rs` and `context.rs` the host composes, plus `NameRules` / `NameFold` at
  the root and `engine::ResolvedScope`.
- **The live walk's judge** (host-side, it consumes `cmdr-index`'s walk):
  `matcher::{CompiledQuery, Candidate, Evaluator, CompileError}`,
  `excludes::ExcludeRules::{from_query, excludes_walked}`, `engine::{home_relative_parent, derive_icon_id}`.
- **Routing, MCP, and IPC**: the wire types,
  `query::{parse_scope, parse_size, format_size, summarize_query, summarize_query_for_diagnostics}`,
  `translate::LocalDays` (with its `format_date` / `format_datetime`), `types::MAX_RESULT_ROWS`.
- **The host's loader and weights**: `arena::*`, `ranking::ImportanceWeights` and its map verbs.
- **The host's tests and benches**: `engine::{search_ranked, Ranked, DirSizes}`, `arena::reconstruct_path_from_index`;
  the `testing`-gated fixtures and wrappers count apart.
- **A live run's reporting** (raised 2026-11-04 to 33 / 10 / 104, when it joined the crate): `run::*`, the registry
  value, `ResultStream` and its `RunSink` facts, the run-state DTOs, the terminal report's inputs, and `BATCH_ROWS` /
  `BATCH_INTERVAL` (the host's pump and tests). § "A live run's reporting".
- **Natural language into a query** (raised 2026-11-04 to 38 / 12 / 125): `translate::*`, flat re-exports over private
  modules (`translate/CLAUDE.md`).
- **A recent search** (raised 2026-11-04 to 37 / 11 / 112): `history::*`, the entry, its cap, and the key helpers the
  desktop's Selection history shares. § "Recent searches".
- **A source over rows in memory** (raised 2026-11-04 to 36 root promises): `MemorySource`, `MemoryRow`, and
  `MemorySourceError`, from a private module. The browser demo's host builds one from its tree. § "Rows held in memory".

Raise a ceiling only when the wider surface is genuinely better, said in the commit and here.

## A live run's reporting (`run.rs`)

A live search answers over time, so its reporting is a seam of its own. The crate owns what a run SAYS; the host owns
what it walks (desktop: `cmdr-index`'s cover walk, pumped by `apps/desktop/src-tauri/src/search/live.rs`).

- **`SearchRuns`** is the registry as a value: register (a `RunOrigin::Dialog { order }` supersedes every dialog run of
  a smaller or equal order, and a late-arriving earlier one starts superseded; an `Agent` run neither supersedes nor is
  superseded), cancel one, cancel all, `cancel_dialog_runs_except`, and `is_live` (registered, not superseded, and its
  terminal fact not yet sent). The host holds one per process. Superseding is not cancelling: a superseded run stops
  reporting and its walk keeps filling the index (Decision 11).
- **`LiveRun::is_live` flips BEFORE the terminal fact goes out** (`ResultStream::finish` / `fail` call `mark_ended`
  first), ❌ never after, and ❌ never read off registration alone: the run deregisters only after it emitted. A caller
  that installs its listeners and then reads `true` is guaranteed the ending; "Open in pane" relies on it
  (`apps/desktop/src/lib/search/walk-handoff.svelte.ts`).
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
~23-probe one. The callers are the four parent-chain walks in `engine.rs` and `src/arena/paths.rs`, a few hundred probes
per search, which is microseconds either way against a 100-400 ms scan. ❌ Don't reintroduce it without measuring the
ranking phase first.

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
the Unicode forms the pattern compiles in. Both come from the volume's `NameRules` (`name_rules.rs`), a value the host
passes in: `NameRules::host()` is the `cfg`-derived desktop answer, and `MACOS` / `LINUX` let a test run either
platform's rules anywhere (`the_other_platforms_goldens_hold_under_its_rules`).

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

**A Mac's pattern compiles in both normal forms.** APFS doesn't normalize: Finder writes NFD, while downloads, most
non-Finder apps, and files from Linux or Windows arrive NFC, so one drive holds both. `NameRules::pattern_forms` gives
the pattern NFD first, then NFC when that spells it differently (only ever for a non-ASCII letter), and `CompiledQuery`
keeps a second regex, `other_form`, tried only when the first misses. The candidate name is matched raw, so the scan
stays allocation-free per row and the live walk's judge answers exactly as the arena does (both call `matches`). Two
regexes, ❌ never one alternation: an alternation repeats a user regex's named groups, which the regex crate refuses. A
name mixing the two forms within itself still matches neither; no writer we know produces one.

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
- **The file viewer takes a third position: it REFUSES.** `crates/cmdr-viewer/src/search_matcher.rs` rejects a pattern
  that would need to cross a newline with a typed `MultilineNotSupported`, because its streaming model is
  line-at-a-time. Different problem (file CONTENTS, not names), already explicit in its own code.

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
patterns behaved). The stem takes the matcher's pattern forms (`ranking::Stem`), and a name is banded by the best form
it meets, so on a Mac the same name ranks alike in either normal form.

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
- **The folder path is HASHED, never built.** `arena::hash_path_from_index` streams the parent chain's components into
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
