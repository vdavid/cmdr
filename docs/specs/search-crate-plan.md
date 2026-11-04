# The drive search in its own crate

Issue: [#302](https://github.com/vdavid/cmdr/issues/302) (the browser demo is the second host). Status: PLANNED, not
started (2026-11-04). Process: `docs/guides/multi-agent-refactors.md` (characterize, then convert; fresh-eyes review
rounds before executing).

**Problem**: the ⌘F search engine (query model, matcher, excludes, ranking, the arena and its scan) lives in the app
crate at `apps/desktop/src-tauri/src/search/`, braided with the index handle, `ReadPool` SQL, Tauri events, app statics,
`dirs::home_dir()`, and `cfg!(target_os = "macos")`. So the search core can't be tested without the app and a real index
database, and the browser demo can't run it at all (it answers `prepareSearchIndex` with a stopgap refusal).

**Goal**: a Tauri-free `crates/cmdr-search` that owns the search core and its data types, fed through a narrow source
seam, so (1) desktop gets a core testable over an in-memory source with no SQLite, the purity rules in
`search/CLAUDE.md` become a compiler-enforced crate boundary, and the platform name rules become a value (macOS rules
testable on Linux CI); (2) as a consequence, the demo runs the real search over its in-memory tree.

**Name**: `cmdr-search`. Nothing better fell out: it's the drive search (filename search over a volume), and the
media/OCR search stays in `cmdr-index`'s `media_index` read API, which this crate doesn't touch.

## Loud rules

- ❌ **Desktop behavior stays byte-identical.** Same results, same order, same counts, same honesty fields, same events,
  same log lines (shape). M0's goldens prove it; a golden change in M1–M6 is a bug, never a re-record. The one
  deliberate exception is M1's `fill_dir_sizes` error path.
- ❌ **`bindings.ts` doesn't change.** specta names a type by its struct identity, not its module, so moved types keep
  their names. Regenerate and `git diff --exit-code apps/desktop/src/lib/ipc/bindings.ts` on every milestone. Specta
  stays pinned `=2.0.0-rc.24` (one specta in the graph, or the `Type` impls stop satisfying `tauri-specta`).
- ❌ **The seam is per query, never per row.** The scan runs over a concrete `SearchIndex` inside the crate. No
  `dyn Iterator` of rows, no trait call in the scan loop: that loses `par_chunks` and the per-chunk regex clones (§
  Perf).
- ❌ **No `cmdr-index` dependency in the core.** `cmdr-index` doesn't build for `wasm32`, and the core must. What the
  core borrows from it today (`ROOT_ID`, `SYSTEM_DIR_EXCLUDES`, `normalize_for_comparison`, `CoveredEntry`) moves or
  gets an adapter (M1).
- ❌ **No `tauri`, `tauri-specta`, or `cmdr`** in the crate's tree: add it to `index-crate-isolation`'s
  `guardedIndexCrates` and give it a capped public surface in `index-crate-isolation-ceilings.go`. Ceilings only
  tighten.
- ❌ **No ambient platform, clock, home, or runtime in the core.** Name folding comes from a `NameRules` value, the home
  dir and mount prefix are parameters, a timing log reads `cmdr_fs::rt::Instant`, and the core spawns nothing (rayon's
  `par_chunks` is the one parallelism, and it falls back to the calling thread in the browser, § Perf). The crate's
  `clippy.toml` bans `std::time::Instant::now`, `SystemTime::now`, `dirs::home_dir`, and `std::thread::spawn`.
- ❌ **No app statics in the crate.** State the core needs is a value the host owns (the listing and viewer rule: "the
  host is a value"). Arena caching, idle drops, load gates, staleness, and the weights subscriber stay host policy.
- ❌ **No search logic in TS, no `if (demo)` in app code.** The demo's TS search answers are deleted as the engine takes
  each command over (`apps/desktop/src/lib/demo/CLAUDE.md`: the backend tries TS tables first, so a leftover answer
  silently shadows the engine).
- ❌ **Don't touch `cmdr-index`'s behavior.** Only its surface shrinks (`SYSTEM_DIR_EXCLUDES` moves to `cmdr-fs`).
- ❌ **Strip milestone tags** from code, comments, test names, and docs before landing (the guide's `rg`).
- Every milestone ends with the `AGENTS.md` § "Checker script" cadence
  (`pnpm check --fast && pnpm check svelte-tests-related`, `pnpm check clippy`, the touched crates'
  `cargo nextest run`), the `bindings.ts` diff, and from M4 on `pnpm build:listing-wasm -smoke`.

## Survey (2026-11-04, `main` at `7a199a9705`, non-test sources)

### Where it lives

`apps/desktop/src-tauri/src/search/` is about 17,800 lines with tests and docs. Non-test line counts by file:

- **Pure core, moves**: `engine.rs` 664, `matcher.rs` 322, `excludes.rs` 217, `ranking.rs` 412, `types.rs` 333 (about
  150 of it tests), `query.rs` 472 (pure except two functions), `index.rs` 404 (arena types; about 250 are tests), the
  arena-building half of `index/load.rs` (`merge_segments`, `spare_rows`, `copy_with_room`, about 150 of 648).
- **Reporting, moves in M3**: `live.rs` 719 (`ResultStream`, the run registry, `WalkJudge`, `drive_walk`),
  `live/events.rs` 346, `live/collect.rs` 149.
- **Host, stays**: `execute.rs` 332 (routing), `execute/coverage.rs` 206, `execute/live_run.rs` 626, `volumes.rs` 767,
  `volumes/weights.rs` 211, `history.rs` 373, the SQL half of `index/load.rs`, `bench.rs` 725, `commands/search.rs` 618.
- **AI translation** (`ai/`, about 2,100): pure domain logic (prompt, parser, query builder, mappings) except
  `size_scope_mapping.rs`'s `expand_tilde` and `ai/types.rs`'s specta DTOs. Backend resolution is already in `crate::ai`
  (`search/DETAILS.md` § AI backend resolution).

### What the core touches today

- **SQLite / the index**: none in the scan. `engine.rs` imports only `cmdr_index::store::ROOT_ID` (the constant `1`).
  `excludes.rs` reaches `store::normalize_for_comparison`, which is really `cmdr_fs::name_fold`. `matcher.rs` imports
  `cmdr_index::CoveredEntry` for one adapter (`matches_covered`). `query.rs` re-exports
  `cmdr_index::SYSTEM_DIR_EXCLUDES` (defined in `cmdr-index`'s `scanner/exclusions.rs` and also read by
  `importance/classify.rs`), and its `resolve_include_scope` walks `store::resolve_path` over a `ReadPool`.
- **SQL that feeds the core** (all in the host): the arena load
  (`SELECT id, parent_id, name, is_directory, logical_size, modified_at FROM entries` over up to eight rowid ranges on
  threads), the catch-up (rows past the arena's highest id), scope resolution (`resolve_path`, a component walk),
  `dir_sizes_in_range` (a `dir_stats` scan, 33–40 ms over 574,927 rows), and `get_dir_stats_batch_by_ids` for the
  returned directory rows.
- **The index handle**: `index().volume_id_for_path` in routing, `Index::coverage` / `Index::cover` in the live run,
  `ReadPool` and `SearchDirs` in `volumes.rs`. All host.
- **Tauri**: `live/events.rs` (`TauriSearchEventSink`, the four `tauri_specta::Event` structs), `live_run.rs`
  (`AppHandle`), `volumes.rs` (`tauri::async_runtime`), `commands/search.rs`. None in the core files.
- **Settings**: none in the core. `search.recentSearches.maxCount` is resolved in `commands/search.rs`.
- **AI**: `ai/` builds the prompt and parses replies; the provider comes from `crate::ai::manager`. No network here.
- **Clocks**: `Instant::now` for the engine's timing log (`engine.rs:288`), `SystemTime` in `index.rs::now_secs` (used
  only by `volumes.rs`'s activity timer, host), `Instant` across `live.rs` (flush intervals), `chrono` in routing and
  the live run (host). The `time` crate's `format_timestamp` formats a given value and reads no clock.
- **Platform**: `cfg!(target_os = "macos")` decides the default case rule (`matcher.rs::case_insensitive_for`), NFD
  pattern normalization (`compile_pattern`, `ranking::stem_for`), and, through `cmdr_fs::name_fold`'s `cfg`, the
  exclusion fold. In the browser all three silently take the Linux answer.
- **Home**: `dirs::home_dir()` in `engine.rs:416` (the `~` parent path) and `query.rs::parse_scope`; `expand_tilde` in
  `canonicalize_scope_path` and the AI size mapping.
- **Threads and cancellation**: the core uses rayon only. `canonicalize_scope_path` spawns a thread under a 2 s
  `realpath` deadline (host: it guards dead mounts). The loader takes an `&AtomicBool` cancel; live runs take a
  `CancellationToken` (host). The registry and `ResultStream` use `Mutex` statics and `Instant`.
- **IPC types in `bindings.ts`**: `SearchQuery`, `SearchResult`, `SearchResultEntry`, `PatternType`, `SearchSort`,
  `ParsedScope` (`types.rs`); `SearchRunCoverage`, `SearchPhase`, `WalkEnding`, `CoverageKind`, `SearchRunError` plus
  the four events (`live/events.rs`); `PrepareResult`, `SearchSortRow`, `SearchSortAnswer`, `TranslateResult`
  (`commands/search.rs`); `TranslatedQuery`, `TranslateDisplay` (`ai/types.rs`); `HistoryEntry`, `HistoryMode`,
  `HistoryFilters` (`history.rs`).
- **Tests the move carries**: `engine/tests/` (7 files), `matcher/tests.rs`, `query/tests.rs`, `ranking/tests.rs` +
  `memory_tests.rs`, `excludes/tests.rs`, `index/memory_tests.rs`. They build arenas by hand (`build_synthetic_index`)
  and lean on the app's `test_support` (`allocations_on_pool` is a counting global allocator in the app's test binary).

### What's already in the right shape

- `engine.rs` is pure by rule and takes importance as data (`ImportanceWeights`).
- `SearchEventSink` already keeps the live run off Tauri; `CollectingSink` is the MCP fold.
- `docs/specs/search-arena-snapshot.md` Contract B (#114, deferred) already designed `SearchIndex` as an accessor
  facade. This plan adopts that facade, so a mapped arena later swaps storage inside the crate without touching callers.

## The seam

### The headline

**The matcher never moved into SQL, so nothing has to leave it.** Matching has been an in-memory scan since SQLite's
`LIKE '%q%'` measured 1–3 s on 5 M rows (`search/DETAILS.md` § Decisions). What SQL does is FILL the arena and answer
the two questions the arena can't: which ids a scope path names, and how big a directory is. So the seam is "give me a
built arena, plus those two answers", asked a few times per query. The desktop source answers from the index database;
an in-memory source answers from a tree.

### Shapes (intent; the executing agent may rename)

- **`SearchIndex`**: the arena, crate-owned, fields private behind Contract B's accessors (`len`, `id`, `parent_id`,
  `name`, `is_directory`, `size`, `modified_at`, `index_of_id`, `generation`). `SearchEntry` stays 40 bytes and `OptU64`
  stays the only way to read size and date. Built only through:
- **`ArenaBuilder`**: push rows in ascending id order (`debug_assert` on order, ❌ never a sort in release), names into
  the one `String`, then `finish()`. Plus `merge_segments` (the parallel load's ordered concat, with `spare_rows` room)
  and `SearchIndex::append_created` (the catch-up). `ROOT_ID` becomes the crate's own `ARENA_ROOT_ID = 1`; the desktop
  loader carries `const _: () = assert!(cmdr_search::ARENA_ROOT_ID == cmdr_index::store::ROOT_ID);`.
- **`SearchRows`** (trait): `fn arena(&self) -> &SearchIndex`, `fn path_prefix(&self) -> &str` (mount root, empty for
  root), `fn resolve_scope(&self, include_paths: &[String]) -> ScopeResolution` (ids plus a typed `unresolved` list;
  empty ids means whole volume, today's rule).
- **`DirSizeSource`** (trait): `fn in_range(&self, min, max) -> Result<HashMap<i64, u64>, SourceError>` and
  `fn of(&self, ids: &[i64]) -> Result<Vec<Option<u64>>, SourceError>`. Both fail loudly: a read error fails the search
  (❌ never "no map", which the engine reads as "no filter"). There is no "sizes not tracked" answer: every host has
  real folder sizes (desktop's `dir_stats`, the demo's in-memory fold), so ❌ don't add one.
- **Why two traits**: they read different tables (`entries` vs `dir_stats`) and, in the demo, come from different
  producers (rows from the tree, sizes from the folder-size fold the panes already show). A host composes them.
- **`SearchContext`**: `NameRules`, `home_dir: Option<String>`, and `&ImportanceWeights`. `NameRules` carries the
  default case rule, whether a pattern is NFD-normalized, and the exclusion/scope fold. `NameRules::host()` is the
  `cfg`-derived desktop answer, so desktop is byte-identical; tests can name `NameRules::MACOS` or `LINUX` on any CI.
- **`search_covered(rows, sizes, query, ctx) -> Result<CoveredHalf, SearchError>`**: today's `search_covered_half`
  (scope resolution, the dir-size map, `search_ranked`, the dir-size fill) as one crate function. `run_blocking` and the
  live run's covered half both call it; the host only routes and loads.
- **`MemorySource`** (crate, M4): `SearchRows` over rows a caller hands in (`path`, kind, size, mtime), ids assigned in
  path order from `ARENA_ROOT_ID`, scope resolution through a `(parent_id, folded name) → id` map built at load with the
  SAME fold the index's `resolve_component` uses. Desktop tests use it to test the core with no database; the demo uses
  it over `InMemoryVolume`.
- **The index-backed source stays in the app** (`search/` becomes the host, the `listing_host/` pattern): `IndexSource`
  wraps `LoadedVolume` (pool, mount root, arena) and answers through the existing SQL (David, 2026-11-04: it's SQL,
  threads, and the app's arena lifecycle, and the demo's rows come from its tree, proven identical by M4).

### Perf (reasoned with numbers; M0 records the baseline, M2 re-measures)

- **Dyn dispatch**: four trait calls per query (arena, scope, range, fill), nanoseconds against a 100–400 ms scan on 5–6
  M rows. A per-row trait would be 6 M virtual calls (about 12–30 ms at 2–5 ns), and worse, a `dyn` iterator can't
  `par_chunks`: 16 cores collapse to one, roughly a 10× regression on a one-letter query. The rule above forbids it.
- **Inlining across the crate boundary**: the hot loop (`search_ranked`'s chunk closure, `CompiledQuery::matches`,
  `SearchIndex::name`, `index_of_id`, `OptU64::get`) ends up wholly inside `cmdr-search`, so nothing hot crosses a crate
  edge; release also runs thin LTO (`Cargo.toml` `[profile.release]`). Keep `#[inline]` on the accessors anyway, for the
  Contract B facade.
- **SQL that could have stayed "smart"**: nothing. The dir-size range scan (33–40 ms) and scope resolution (a handful of
  indexed lookups) were already source questions, and they stay SQL in `IndexSource`. The arena is query-independent and
  cached across queries, so no query predicate can be pushed into the load.
- **Memory**: unchanged. Same 40-byte row, same id-sorted arena with no id map, same 1/128 spare room, same merge peak
  (the loader's threads stay in the host).
- **Allocations per query**: unchanged by construction (per-chunk clones of the compiled query and scope filter move
  verbatim). `engine/tests/allocations.rs` needs a counting allocator in the crate's own test binary.
- **The browser**: rayon-core 1.13.0's `default_global_registry` falls back to a one-thread pool on the calling thread
  when threads are unsupported (`registry.rs` lines 209–228, verified 2026-11-04 by reading the locked source), so
  `par_chunks` runs inline. The demo tree is a few thousand rows: well under a millisecond even at a 2–3× wasm penalty.
  Only a Node/browser smoke proves the fallback doesn't trap, so M4 adds one.

## Milestones

### M0: characterize (S–M)

- **Scope**: a golden suite pinning TODAY's answers end to end, before any code moves. A fixed corpus written into a
  temp-dir index through `IndexStore` (deep trees, accented names in both forms, a newline in a name, hardlink-deduped
  NULL sizes, an unknown mtime, `node_modules` / `.git` / `Caches`, a user `!name` exclude, a mount-rooted non-root
  volume with a prefix, `dir_stats` rows for size filters, an importance map). About 40 queries through
  `search_covered_half` (glob, wildcard glob, regex, case override both ways, accented, newline, size and date bounds,
  directory size filter and `sortBy: size` / `modified`, include scopes, unresolved scope, whole-volume mount-root
  scope, system dirs on and off, count-only, the limit ceiling, too broad under both evaluators). Plus the event trace
  of one live run over a fully covered scope (progress batches, the complete payload). Committed as JSON goldens. Record
  `bench_arena_scan` and `bench_query_allocations` (release, a real `index-root.db`) in `docs/notes/`.
- **Intentions**: the proof every later milestone must match. Platform-dependent answers get one golden per platform
  today; M1 turns them into explicit `NameRules` runs.
- **Landmines**: pin reality, not expectation. Make each pin bite: break the engine on purpose (drop the NFD step, flip
  the `Excluded` count, reverse a tie-break) and see the golden go red, then revert. Fixtures in id order (the arena's
  binary search answers wrong otherwise). The event trace has timing-driven batch splits: drive it with the flush
  interval out of the picture, or pin only the rows and the terminal payload.
- **Test plan**: the goldens themselves, green on macOS and Linux CI.
- **DONE**: goldens committed and green on both; each family seen red once; baseline numbers in `docs/notes/`.

### M1: cut the seams in place (M)

- **Scope**: still in the app crate, so every change is a reviewable diff instead of a diff inside a `git mv`. Introduce
  `NameRules` and `SearchContext` and route all three `cfg!(target_os)` decisions and the exclusion fold through them;
  pass `home_dir` in (engine, `parse_scope`); read the timing log's clock through `cmdr_fs::rt`. Make `SearchIndex`
  fields private behind the Contract B accessors and add `ArenaBuilder`; port every hand-built fixture to it. Define
  `SearchRows`, `DirSizeSource`, `DirSizeAnswer`, `ScopeResolution`, and `IndexSource`; turn `search_covered_half` into
  `search_covered` over them. Remove the core's `cmdr_index` imports: `ARENA_ROOT_ID` plus the static assert;
  `SYSTEM_DIR_EXCLUDES` moves to `cmdr-fs` (`cmdr-index`'s `importance/classify.rs` and the app read it there, and
  `cmdr-index` drops the root re-export, which tightens its ceiling); `normalize_for_comparison` from
  `cmdr_fs::name_fold`; `matches_covered` and `covered_name` move to the live-walk code (`live.rs`) as an adapter
  building a `Candidate`. Fix `execute.rs::fill_dir_sizes`, which swallows a `dir_stats` read error today (`let _ =`
  plus `if let Ok`) and ships directory rows with no size and no log line: through `DirSizeSource::of`, a read error
  fails the search, exactly as `dir_sizes_for` already does (invariant 7).
- **Intentions**: after M1, the future crate's files compile with only `cmdr-fs`, `regex`, `rayon`,
  `unicode-normalization`, `serde`, `specta`, `time`, and `log` in their imports. M2 is then pure relocation.
- **Landmines**: `NameRules::host()` must reproduce today's `cfg` answer exactly, including the fold being a no-op off
  macOS (the exclusion fold explicitly lowercases on top: `excludes.rs::fold`'s comment). `normalize_for_comparison` is
  `cfg(target_os = "macos")` in `cmdr-fs`, so the browser would get the identity fold: that's what `NameRules` replaces
  in the core. Contract B touches `ranking/tests.rs`'s literal arenas and `bench.rs`. Moving `SYSTEM_DIR_EXCLUDES`
  changes `cmdr-index`'s public surface: run `index-crate-isolation` and commit its rewrite. `include_path_ids` stays a
  `SearchQuery` field (it's on the wire; changing it is a `bindings.ts` diff). The `fill_dir_sizes` fix is the plan's
  ONE deliberate behavior change, and only on an error path no golden covers; keep it its own commit.
- **Test plan**: M0's goldens unchanged; existing suites green; one new test runs the macOS-only goldens under
  `NameRules::MACOS` on Linux; a failing fill (a `DirSizeSource` fake that errors) fails the search, seen red first.
- **DONE**: goldens byte-identical; `bindings.ts` unchanged; the core files' imports are the list above; no swallowed
  size read left.

### M2: extract `crates/cmdr-search` (M–L, mostly moves)

- **Scope**: `git mv` the core (engine, matcher, excludes, ranking, types, the pure half of query, the arena and
  builder, the seam traits, `search_covered`) with their tests into the new crate. `apps/desktop/src-tauri/src/search/`
  keeps routing (`resolve_target`), `IndexSource` and the SQL loader, `volumes.rs`, the live run, `history.rs`, `ai/`
  (until M5), and re-exports what callers name (`search::SearchQuery` keeps resolving). Wire the crate into the
  workspace, `index-crate-isolation` (guarded list plus a capped ceiling), `AGENTS.md`'s crate list,
  `docs/architecture.md`, and a `C.md` / `D.md` pair: the crate's `C.md` takes the purity, memory, arena, matcher, and
  ranking must-knows; the app host's `C.md` keeps routing, staleness, mount-relative paths, coverage, and runs. Rehome
  `search/DETAILS.md` sections to whichever side owns the code (single-source: each claim lives once).
- **Intentions**: the crate boundary becomes the purity rule. A future agent physically can't reach the DB from the
  engine.
- **Landmines**: `pub(crate)` everywhere becomes `pub` only where the host needs it; keep the surface minimal and let
  the ceiling pin it. The counting allocator behind `allocations.rs` lives in the app's test binary: the crate needs its
  own (one `#[global_allocator]` per test binary). `types.rs`'s `the_ceiling_matches_the_frontend_snapshot_cap` names a
  frontend file in prose; keep it findable. Cross-links in moved docs (`docs-dead-links`, `docs-reachable`). The
  `#[cfg(test)] engine::search` wrapper builds a `SearchResult`; keep it test-only. `cmdr-search` needs the `testing`
  feature gate pattern (`any(test, feature = "testing")`) for fixtures the app's tests use.
- **Test plan**: goldens unchanged; `cargo nextest run -p cmdr-search` plus the app's search tests; desktop Playwright
  search specs (`pnpm check desktop-e2e-playwright`); re-run M0's benches and record them beside the baseline (within
  noise, or stop and investigate).
- **DONE**: app search unchanged, the crate green alone, `index-crate-isolation` green with the new entry, benches
  within noise, `bindings.ts` unchanged.

### M3: the run's reporting moves too (M)

- **Scope**: move `ResultStream` (batching, dedupe, cap, flush cadence, the terminal fold), the run registry as a value
  (`SearchRuns`: register with `RunOrigin::Dialog { order }`, supersede, cancel, `cancel_dialog_runs_except`), the
  run-state enums that are data (`SearchPhase`, `WalkEnding`, `CoverageKind`, `SearchRunError`, `SearchRunCoverage`),
  and a crate `RunSink` trait that hears facts. The app keeps the four `tauri_specta::Event` structs and maps facts onto
  them in `TauriSearchEventSink` and `CollectingSink`; the app holds its one `SearchRuns` where the statics were.
  `WalkJudge` and `drive_walk` stay in the app (they consume `cmdr-index`'s walk), judging through the crate's
  `CompiledQuery` and `ExcludeRules`. Add `search_fully_covered(...)`: the run a scope with an empty frontier makes
  (announce, the covered half, finish as `Covered` / complete), which `live_run.rs` calls for that case.
- **Intentions**: the demo's run must say exactly what a desktop run over a fully indexed drive says, without
  re-implementing batching or the terminal rules. Events as facts, wire types in the app: the listing engine's
  `ListingEvents` shape.
- **Landmines**: `ResultStream`'s flush interval reads `Instant`: route it through `cmdr_fs::rt`. Decision 11 (supersede
  by `order`, never arrival) and "superseding is not cancelling" move verbatim, tests with them. The registry lock used
  by tests (`test_registry_lock`) becomes per-value. No crate type may share a name with an app event (specta would emit
  both): name facts `RunProgress`, `RunComplete`, and so on. The phase rule (`live/CLAUDE.md`: what the run IS doing,
  never inferred from the ending) lives in the crate now.
- **Test plan**: M0's event-trace golden unchanged; `live/tests.rs` moves; the MCP fold tests stay green.
- **DONE**: desktop event traces byte-identical; no `static` in the moved code; `bindings.ts` unchanged.

### M4: `MemorySource` and the wasm32 build (M)

- **Scope**: `MemorySource` in the crate. A differential test feeds M0's corpus to both `IndexSource` and `MemorySource`
  (with a test `DirSizeSource` over the corpus's own `dir_stats` rows) and requires identical answers on every query,
  directory size filters and `sortBy: size` included. Build the crate for `wasm32-unknown-unknown` in the build script's
  wasm lane (add it to `browserBound` once the write-engine branch, which introduces that list, has landed). Link it
  into `cmdr-listing-wasm` behind a minimal call so `smoke.mjs` runs one query in Node.
- **Intentions**: the equivalence test is what lets the demo say "the real search": same core, rows from another source,
  proven identical.
- **Landmines**: scope resolution in `MemorySource` must fold names the way `IndexStore::resolve_component` does
  (`name_folded`, the platform fold), or a scope that resolves on desktop is "unresolved" in memory. Paths: the
  browser's `Path::is_absolute` is always `false` (`crates/cmdr-listing-wasm/CLAUDE.md`), so the core uses `&str` path
  work only. The rayon fallback is runtime behavior: only the smoke proves it. `std::time::Instant::now` compiles and
  then panics in the browser: the smoke has to execute the timing log path (`log` level at debug in the smoke).
- **Test plan**: the differential test natively; the smoke runs a glob and a scoped query against the real `.wasm`.
- **DONE**: differential test green; the wasm lane builds `cmdr-search`; the smoke answers a query.

### M5: the AI translation pipeline joins the crate (M, can run beside M3/M4 after M2)

- **Gate**: do it only if the boundary is clean (David, 2026-11-04). First step: list every import `ai/` makes outside
  itself. If anything beyond `expand_tilde`, the specta DTOs, and a clock read turns up (settings, a provider, app
  state), stop and report instead of threading it through; review rounds weigh in before execution.
- **Scope**: `ai/`'s prompt, parser, query builder, mappings, and `TranslatedQuery` / `TranslateDisplay` move into the
  crate as a `translate` module. `expand_tilde` becomes the context's `home_dir`; any date math takes "now" as a
  parameter. Backend resolution, keys, and the `translate_search_query` command stay in the app.
- **Intentions**: it's search domain logic with no I/O, and today the one upward edge from `search/` into `commands/`
  (`search/DETAILS.md`). The demo doesn't use it (no model in a browser; the demo's AI answers already say unavailable),
  so this is a desktop-only win.
- **Landmines**: the `(?is)` prefix rule lives in `keyword_mapping::merge_keyword_and_type` alone (`search/DETAILS.md` §
  The compiled query); keep it single-sourced. The property tests named in `apps/desktop/src-tauri/Cargo.toml` (parser,
  glob→regex) move with it.
- **Test plan**: `ai/` tests move unchanged; one translation golden over a fixed "now".
- **DONE**: no `crate::commands` edge left under `search/`; `bindings.ts` unchanged.

### M6: recent searches go Tauri-free (S–M, after M2, beside M3–M5)

- **Scope**: split `crate::recents` (used by search, selection, Go to path, and multi-rename) into its pure list (the
  `RecentEntry` trait and the add/move-to-top, cap, remove, clear, and max-count ops: `add_to`, `trim_to`,
  `remove_from`) and its persistence (`RecentsFile`'s JSON file, quarantine, locking, `AppHandle` paths). The pure list
  goes to a Tauri-free home; `HistoryEntry`, `HistoryMode`, `HistoryFilters`, the key helpers (`normalize_query`,
  `filters_fingerprint`, `flag`), and their `RecentEntry` impl move into `cmdr-search`'s `history` module.
  `selection/history.rs` keeps importing the helpers one way, now from the crate. The app keeps `RECENT_SEARCHES` and
  file persistence; the demo keeps an in-memory list for the session (David, 2026-11-04).
- **Intentions**: one dedupe key and one list rule for both hosts, so a recent search dedupes in the demo exactly as on
  a Mac. A desktop win too: four features' list rules become testable without a file or an app.
- **Landmines**: pick the pure list's home by its users: four app features, so a tiny Tauri-free crate (or a module in
  an existing shared crate if a reviewer finds a natural one), ❌ never inside `cmdr-search` where Go to path and
  multi-rename would depend on search. Entry ids: if minting one reads a clock or an RNG, it's a host parameter (the
  browser panics on `SystemTime::now`). The six-segment key is pinned by `key_carries_scope_and_exclude_system_dirs`; it
  moves with the helpers. The on-disk format of all four files is unchanged: their existing load tests stay green.
- **Test plan**: `recents` and the four features' history tests unchanged; a new crate test drives the pure list.
- **DONE**: no Tauri in the list logic; all four histories load the same files; `bindings.ts` unchanged.

### M7: ⌘F in the demo (M–L, after M3, M4, M6, the write-engine + viewer branches, and the folder-size fold land)

- **Scope**: a `search.rs` host in `crates/cmdr-listing-wasm` (with `search_test.rs`), the demo's third engine beside
  listing and viewer. It builds a `MemorySource` from the demo's `InMemoryVolume` and rebuilds it when the volume's
  write generation moved (a few thousand rows: cheaper than any staleness policy). It answers `prepare_search_index`
  (`ready: true` on first ask), `search_files`, `search_files_streaming` (through `search_fully_covered`, events by
  their Tauri names), `cancel_search`, `release_search_index`, `parse_search_scope`, `get_system_dir_excludes`,
  `sort_search_results` (on the demo's own listing collator), and the recent-search commands (M6's list, in memory for
  the session). Its `DirSizeSource` is the demo's in-memory folder-size fold (`worktree-agent-a03dc8ea1594f1b12`, in
  progress: it already feeds `cmdr_listing::IndexSizes` and implements `cmdr_operations`' `OperationIndex`), so a size
  filter reads the same numbers the panes show. `translate_search_query` keeps the demo's typed "AI unavailable" answer.
  Delete every TS search answer in `apps/desktop/src/lib/demo/`. `NameRules` for the demo: case-insensitive by default
  (it presents as a Mac), patterns left in the form typed, because the tree's names are the browser's NFC strings and an
  NFD pattern would miss every accented name. Home is the demo tree's home, so result rows read `~/…`.
- **Intentions**: the dialog behaves as on a fully indexed Mac, directory size filters included. No temporary
  difference.
- **Landmines (sizes)**: the fold must be ONE fold for panes, operations, and search: ❌ never a second size rollup in
  the search host. Its answer must keep `dir_stats`' meaning (`recursive_logical_size`: a hardlinked inode's bytes
  counted once, symlinks not followed); if the fold's rule differs anywhere, the fold is what gets fixed, in its own
  crate, with a test. A write moves the sizes: read them per query (the fold is current), never cached in the arena.
- **Landmines**: events must arrive after the command resolves (`apps/desktop/src/lib/demo/DETAILS.md` § The listing
  seam's one-task delay), or the dialog drops a run it hasn't registered. Event payload keys must match `bindings.ts`:
  pin them the way `events_test.rs` pins listing events. A scope outside the demo tree must come back in
  `unresolved_scopes`, never panic or route elsewhere (there is one volume). The idle-drop timer is host policy: the
  demo keeps its arena (it's tiny) and `release_search_index` only cancels dialog runs. A symlinked folder in the tree
  isn't canonicalized (no `realpath` in a page), so a scope through it is unresolved: record it as a demo difference.
- **Test plan**: native host tests replaying a slice of M0's goldens through the demo host; `smoke.mjs` drives a
  streaming search; `apps/website/e2e/demo.spec.ts`: ⌘F, type a name, Enter lands the cursor on the file; a headless
  screenshot for David.
- **DONE**: ⌘F finds files by name, size, and date in `pnpm build:demo`; no TS search answer left; the demo's
  `unanswered` list stays empty.

### M8: close out (S)

- **Scope**: the three demo-image input lists (below) name `crates/cmdr-search` (and the pure recents crate, if M6 made
  one); `crates/cmdr-listing-wasm/DETAILS.md` § "Where the demo differs from the app" gains the search differences
  (recent searches last one session, no symlink canonicalization, no AI); wasm size before and after in that doc; wipe
  this spec per `docs/specs/DETAILS.md` § "Wiping a shipped spec".
- **DONE**: CI publishes a demo image with ⌘F; the adversarial conformance review (against the invariants below) and the
  docs-audit agent pass.

Total: L–XL, eight milestone agents (M5 gated on a clean boundary). M0 can start now. M1–M2 touch only the app's
`search/`, `cmdr-fs` (one constant), and `cmdr-index` (one re-export), so they run in parallel with the demo branches.
M3–M6 can run side by side after M2 (separate worktrees; M6 touches `recents/` and three other features' histories). M7
waits for the demo branches and the fold.

## Collisions with in-flight work (branches as of 2026-11-04)

- **The demo's folder-size fold** (`worktree-agent-a03dc8ea1594f1b12`, in progress): M7's `DirSizeSource`. M7 builds on
  it after it lands; if its type or crate moves before then, follow it rather than wrapping a copy.
- **The ⌘F stopgap** (`answers.ts` refusing `prepareSearchIndex` so the dialog opens in its "index not available" state,
  on `worktree-agent-a28ab52ef2f8b33b1`): M7 deletes it.
- **The write-engine and viewer branches** (`worktree-agent-a683f589da8b16ef6` and descendants, including
  `worktree-agent-aff43f5d181de5ed9`): own `cmdr-listing-wasm`'s `commands.rs` / `lib.rs` / `Cargo.toml` routing,
  `browserBound` in `scripts/build-listing-wasm/main.go`, `lib/demo/answers.ts`, and the in-memory volume's mutation
  hook (M7's rebuild trigger). M4's wasm-lane edit and all of M7 start after they land; ❌ never in parallel against the
  same files. `cmdr-viewer` brings `regex` into the wasm graph, so the search core adds little beyond its own code.
- **`cmdr-listing-wasm` itself**: M7 adds a module and commands; its `COMMANDS` routing and `smoke.mjs` are shared with
  every demo branch. Batch the landing.
- **The three demo-image input lists** must stay equal: `scripts/website-demo-key.sh`'s `paths`, `websiteDemoInputs` in
  `scripts/check/checks/inputs.go`, and `.github/workflows/ci.yml`'s `website` filter. M4 adds `crates/cmdr-search` to
  all three, batched with the other demo crates (`docs/guides/multi-agent-refactors.md` § batch landing).
- **#114, the mapped arena** (`docs/specs/search-arena-snapshot.md`, deferred): this plan lands its Contract B facade,
  so that spec's scope list (`search/index.rs`, `engine.rs`, ...) moves to `crates/cmdr-search/src/`. Update its paths
  in M2.

## Decisions (David, 2026-11-04)

David's framing: every step is a genuine architectural win, never a demo-only shim.

1. **The index-backed source stays in the app** (`search/` becomes the host, like `listing_host/`).
2. **Recent searches in the demo work for the session**: the list rules and history keys go Tauri-free (M6).
3. **No temporary difference for directory size filters**: the demo's `DirSizeSource` is the same folder-size fold its
   panes and operations read (M7).
4. **M5 runs if the AI pipeline's boundary is clean** (its gate); review rounds weigh in.
5. **`fill_dir_sizes` stops swallowing its read error** in M1: the search fails, matching invariant 7.

## Invariants

1. Desktop search answers, order, counts, honesty fields, events, and `bindings.ts` are byte-identical to M0's goldens.
2. `cmdr-search` depends on no `tauri`, `tauri-specta`, `cmdr`, or `cmdr-index`; it builds for `wasm32-unknown-unknown`.
3. The scan runs over a concrete arena inside the crate; every source call is per query, never per row.
4. One matcher, one exclusion set, one `NameRules` per run: no case-folding or normalization rule derived outside it.
5. The core holds no statics and reads no ambient clock, home, platform, or runtime.
6. `SearchEntry` is 40 bytes, the arena is id-sorted with no id map, and `OptU64` is the only read of size and date.
7. A directory size read error fails the search, in the range read and the row fill alike; nothing swallows one.
8. The browser demo answers every search command through the crate; TS answers none.
9. One folder-size truth per host: desktop's `dir_stats`, the demo's fold. Search never rolls up sizes of its own.
10. One recent-list rule and one history key for every host and every history feature.
