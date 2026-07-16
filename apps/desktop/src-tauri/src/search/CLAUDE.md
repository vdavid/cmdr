# Search module

Multi-volume in-memory filename search + AI query translation. A scope routes to its owning volume(s); an unscoped
query fans out across every volume with a persisted `index-{volumeId}.db` and merges. Flat API:
`use crate::search::{SearchQuery, SearchResult, ...}`.

## Module map

- **`mod.rs`**: re-exports from submodules.
- **`index.rs`**: `SearchIndex` (arena-allocated filename storage), `SearchEntry`, and `load_search_index` (the arena
  loader). No lifecycle state — that's `volumes.rs`.
- **`volumes.rs`**: per-volume registry + dialog/idle/backstop timers (drop ALL arenas at once). `ensure_volume(id)`
  lazily loads + caches a volume's arena, mount root (DB `volume_path` meta), and weights; a non-root volume opens
  read-only from `index-{id}.db` on disk, NOT via `INDEX_REGISTRY`. `VolumeLoad::{Loaded,NotIndexed,Failed}`.
- **`execute.rs`**: `run_blocking(query)`, the multi-volume orchestrator (route → load → per-volume engine → merge).
- **`engine.rs`**: `search_ranked()` pure (no I/O): compiles glob/regex, rayon-filters, ranks, reconstructs paths
  (mount-root-prefixed). Scope via `include_path_ids` / `exclude_dir_names`. `search()` is a `#[cfg(test)]` wrapper.
- **`history.rs`**: persistent recent-searches store. Atomic JSON, canonical dedupe key, cap eviction, schema-version
  quarantine. See must-knows below.
- **`types.rs`**: pure data (`SearchQuery`, `SearchResult`, `SearchResultEntry`, `ParsedScope`, `PatternType`,
  `default_limit`). No logic.
- **`query.rs`**: operations on the types: `parse_scope()`, `resolve_include_paths()` (DB pre-query),
  `fill_directory_sizes()` (DB post-query), formatters, `summarize_query()`, `SYSTEM_DIR_EXCLUDES`.
- **`ai/`**: NL → `SearchQuery` translation. See [`ai/CLAUDE.md`](ai/CLAUDE.md).

Flow: `execute.rs` routes a query to its volume(s) → per volume: `query.rs` resolves scope IDs → `engine.rs` scans
(pure, ranked) → `execute.rs` fills dir sizes + merges.

## Must-knows

- **`engine.rs` is pure: no I/O, no DB access.** Keep it that way; it's the hot path isolated from side effects and
  trivially testable with synthetic data.
- **`types.rs` stays free of logic.** It's imported by everything (`engine.rs`, `query.rs`, `ai/`); adding logic risks
  circular dependencies.
- **`search/` is a read-only, one-way consumer of `indexing/`** (`search → indexing`, never reverse). It imports
  `ReadPool` (`indexing::enrichment`), `WRITER_GENERATION` (`indexing::writer`), `ROOT_ID` /
  `normalize_for_comparison` / `resolve_path` / `IndexStore` (`indexing::store`), and `volume_id_for_local_path`
  (`indexing::routing`, for scope→volume routing). Search reads the index but doesn't participate in indexing.

- **Multi-volume: `execute.rs` routes + merges, the engine stays per-index/pure.** Non-root indices are mount-relative,
  so PREFIX the mount root onto read paths and STRIP it from scope include paths. A scope on an unindexed volume →
  `SearchResult::uncovered_scopes` (typed; branch on emptiness, never string-match), not empty success. Only the root
  writer bumps `WRITER_GENERATION`. Full model + gotchas: [DETAILS.md](DETAILS.md) § Multi-volume search.
- **`name_folded` is NOT stored in the index**: the pattern is NFD-normalized at query time on macOS (APFS filenames are
  already NFD). Avoids doubling the name arena's memory.
- **Filenames are arena-allocated**: each `SearchEntry` holds `name_offset: u32` + `name_len: u16`, not an owned
  `String`. During load, `row.get_ref(col).as_str()` borrows from SQLite's buffer (zero per-row heap alloc).
  `SearchIndex::name(&self, entry)` returns a `&str` slice. Don't switch to owned `String`s; it roughly doubles
  resident memory.
- **`expand_tilde` is imported from `crate::commands::file_system` in `ai/query_builder.rs`**: business logic reaching
  into the IPC layer, kept because moving it touches 20+ call sites across four files. Architecturally backwards but
  intentional; worth a separate cleanup, not a silent "fix" here.

## History store (`history.rs`)

- **Concurrency**: `Mutex<HistoryStore>` cache + a separate `DISK_LOCK` serializing the read-modify-write. Drop the
  cache guard before any `fs` call; no `.await` while holding a guard.
- **Add only on "Open in pane"**, never on Enter / auto-apply (David's call; a signal-rich 1000-entry budget). Not
  Rust-enforced — the FE's ONLY `addRecentSearch` call site is the Open-in-pane handler. Don't add a "convenience" one.
- Persistence + dedupe-key + cap details: [DETAILS.md](DETAILS.md) § History store.

## Sharing with `selection/`

`crate::selection::history` re-exports `HistoryMode` and `HistoryFilters` from this module's `history.rs` (one-way; the
wire shape stays in sync). The entry structs stay separate (`HistoryEntry` vs `SelectionHistoryEntry`) because the
canonical keys differ (Selection has no scope or exclude-system-dirs). The `search/ai/parser.rs` helpers are NOT shared
with Selection (different fields). If the mode set forks, drop the re-export and copy the types.

## IPC

`commands/search.rs` holds thin wrappers. `translate_search_query(natural_query, current_type)` orchestrates the AI
pipeline (classification prompt with the `Both | Files | Folders` toggle as `current_type`, `ai::parse_llm_response`,
`ai::query_builder`). `resolve_ai_backend` stays in `commands/search.rs` (touches `crate::ai` + `crate::settings`). The
MCP `ai_search` executor calls it with `current_type = None`.

Full details (the in-memory-Vec vs SQLite, structured-query, and path-reconstruction rationale; schema-migration
policy): [DETAILS.md](DETAILS.md).
