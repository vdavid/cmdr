# Search module

In-memory search index and AI query translation for whole-drive file search. Flat API:
`use crate::search::{SearchQuery, search, ...}`.

## Module map

- **`mod.rs`**: re-exports from submodules.
- **`index.rs`**: `SearchIndex` (arena-allocated filename storage), `SearchEntry`, global `SEARCH_INDEX` with idle/
  backstop timers and load cancellation.
- **`engine.rs`**: `search()` pure function (no I/O): compiles glob/regex, parallel-filters with rayon, sorts by
  recency. Scope filtering via `include_path_ids` and `exclude_dir_names`.
- **`history.rs`**: persistent recent-searches store. Atomic JSON, canonical dedupe key, cap eviction, schema-version
  quarantine. See must-knows below.
- **`types.rs`**: pure data (`SearchQuery`, `SearchResult`, `SearchResultEntry`, `ParsedScope`, `PatternType`,
  `default_limit`). No logic.
- **`query.rs`**: operations on the types: `parse_scope()`, `resolve_include_paths()` (DB pre-query),
  `fill_directory_sizes()` (DB post-query), formatters, `summarize_query()`, `SYSTEM_DIR_EXCLUDES`.
- **`ai/`**: NL → `SearchQuery` translation. See [`ai/CLAUDE.md`](ai/CLAUDE.md).

Flow: `types.rs` defines → `query.rs` prepares (`resolve_include_paths`) → `engine.rs` scans (pure) → `query.rs`
enriches (`fill_directory_sizes`).

## Must-knows

- **`engine.rs` is pure: no I/O, no DB access.** Keep it that way; it's the hot path isolated from side effects and
  trivially testable with synthetic data.
- **`types.rs` stays free of logic.** It's imported by everything (`engine.rs`, `query.rs`, `ai/`); adding logic risks
  circular dependencies.
- **`search/` is a read-only, one-way consumer of `indexing/`** (`search → indexing`, never reverse). It imports
  `ReadPool` (`indexing::enrichment`), `WRITER_GENERATION` (`indexing::writer`), and `ROOT_ID` /
  `normalize_for_comparison` / `resolve_path` / `IndexStore` (`indexing::store`). Search reads the index but doesn't
  participate in indexing.
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

- **Persistence path**: `{app_data_dir}/search-history.json`, schema-versioned via `_schemaVersion` (currently 1). On
  parse failure or version mismatch, rename to `.broken` and start fresh (corrupt file kept one rotation for debugging).
- **Concurrency**: in-memory `Mutex<HistoryStore>` cache plus a separate `OnceLock<Mutex<()>>` (`DISK_LOCK`) serializing
  the read-modify-write cycle. Always drop the cache guard before any `fs` call; no `.await` while holding a guard.
- **Canonical dedupe key** (compare-time only, never persisted): `mode | normalized_query | filters | scope |
  case_sensitive | exclude_system_dirs`. Same key = same search; the most recent copy wins (move-to-top).
- **Add only on "Open in pane"**, never on Enter / auto-apply. This is David's explicit design call (a signal-rich
  1000-entry budget). The Rust side doesn't enforce it; the frontend's only `addRecentSearch` call site is the
  Open-in-pane handler. Don't add a "convenience" add-on-search call site.
- **Cap**: `search.recentSearches.maxCount` (default 1000). `apply_max_count` trims in-memory on live-apply; `0` clears
  and short-circuits future adds.

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
