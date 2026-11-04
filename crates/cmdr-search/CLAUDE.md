# cmdr-search

The drive search's core (⌘F over one volume's file names): query model, the one matcher and exclusion set, ranking, and
the in-memory arena it scans. Tauri-free and `cmdr-index`-free; Cmdr hosts it from `apps/desktop/src-tauri/src/search/`.

## Module map

- **seam.rs** (the host's door: `SearchRows`, `DirSizeSource`, `search_covered`, typed errors, `ResolvedScope`),
  **context.rs** (`NameRules`, `SearchContext`), **arena.rs** (`SearchIndex`, `ArenaBuilder`, `OptU64`), **memory.rs**
  (`MemorySource`, the seam over rows in memory, ids in one defined order).
- **engine.rs** (the scan, scope filter, sort, path reconstruction), **matcher.rs**, **excludes.rs**, **ranking.rs**,
  **query.rs** (scope parsing, scope paths, summaries), **types.rs** (wire types), **history.rs** (a recent search and
  its dedupe key), **translate/** (natural language into a query, `translate/CLAUDE.md`), **run.rs** (a live run's
  registry, `ResultStream`, and the one terminal report), **testing.rs** (fixtures).

## Must-knows

- **No I/O, no statics, nothing ambient.** No database, runtime, `cfg!` platform, home folder, or clock read: the host
  passes `NameRules` and `home_dir` in a `SearchContext`, and timing goes through `cmdr_fs::rt` (`clippy.toml` bans the
  rest). ❌ No `cmdr-index`: it doesn't build for `wasm32`.
- **The seam is per query, ❌ never per row.** A per-row trait call loses `par_chunks` and the per-chunk regex clones.
- **A source read that fails fails the search** (`SearchError::SourceUnreadable`), ❌ never an empty map (read as "no
  filter"), an "unresolved" scope (read as a typo), or a sizeless folder.
- **One matcher and one exclusion set, two evaluators** (arena scan = ancestor ids; the host's live walk = the entry's
  own path). ❌ Never re-derive case folding or NFD normalization: both are `NameRules`.
- **The broad-query guard is per evaluator** (`Evaluator`): ❌ never one row-count rule.
- **A folder's size filter applies BEFORE ranking** (`dir_sizes_for`), ❌ never after.
- **The arena is sorted by id; `index_of_id` binary-searches it.** ❌ No id map (~143 MB), ❌ no out-of-order rows
  (`ArenaBuilder::push` debug-asserts; fixtures go through `testing::arena_of`, which sorts).
- **A `SearchEntry` is 40 bytes and stays 40.** `size` / `modified_at` are `OptU64`: ❌ never `Option<u64>`, ❌ never
  compare the sentinel, ❌ never collapse `None` into `0` (a NULL size is a hardlink-deduped row).
- **Every live run ends through `ResultStream::finish(CoverageFacts)`**, ❌ never a hand-built `SearchRunCoverage`: the
  abandoned-ground fold and the unresolved-scope rule live in that one report. Superseding a run ≠ cancelling it.
- **Gate test scaffolding on `any(test, feature = "testing")`**: the host's tests use it.

The seam, the arena, the compiled query, ranking and its costs, and the capped surface: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
