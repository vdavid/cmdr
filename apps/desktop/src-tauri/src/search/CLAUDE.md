# Search module

In-memory search index and AI-powered query translation for whole-drive file search.

## Module structure

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports from submodules. Flat API: `use crate::search::{SearchQuery, search, ...}` |
| `index.rs` | `SearchIndex` (arena-allocated filename storage), `SearchEntry`, global `SEARCH_INDEX` state with idle/backstop timers and load cancellation |
| `engine.rs` | `search()` pure function (no I/O): compiles glob/regex, parallel-filters entries with rayon, sorts by recency. Scope filtering via `include_path_ids` and `exclude_dir_names` |
| `history.rs` | Persistent recent-searches store (`HistoryStore`, `HistoryEntry`). Atomic JSON read/write, canonical dedupe key, cap eviction, schema-version quarantine. Used by `commands::search::{get,add,remove,clear}_recent_search` and `apply_recent_searches_max_count`. |
| `types.rs` | Pure data definitions: `SearchQuery`, `SearchResult`, `SearchResultEntry`, `ParsedScope`, `PatternType`, `default_limit`. No logic |
| `query.rs` | Operations on the types: `parse_scope()`, `resolve_include_paths()` (DB pre-query), `fill_directory_sizes()` (DB post-query), `format_size()`, `format_timestamp()`, `summarize_query()`, `SYSTEM_DIR_EXCLUDES` |
| `ai/` | Natural-language → `SearchQuery` translation: classification prompt, key-value parser, deterministic enum mappings, and assembler. See [`ai/CLAUDE.md`](ai/CLAUDE.md). |

## Data flow

```
types.rs defines  ->  query.rs prepares (resolve_include_paths)
                        -> engine.rs scans (pure, no I/O)
                        -> query.rs enriches (fill_directory_sizes)
```

## Key decisions

**Decision**: In-memory `Vec` + rayon instead of SQLite queries for search.
**Why**: The index has ~5M entries. SQLite `LIKE '%query%'` takes 1-3s (full table scan). Loading entries into a `Vec` and scanning with rayon gives sub-second results. The index is loaded lazily on dialog open and dropped after idle (5 min timer + 10 min backstop). ~600 MB resident while active.

**Decision**: Structured `SearchQuery` model, not free-text SQL.
**Why**: Safe (no injection), composable (AI mode fills the same struct), and simple to execute (single pass over the in-memory Vec). The frontend owns query building; the backend is a pure filter engine.

**Decision**: Path reconstruction at search time, not stored in index.
**Why**: Storing full paths would double memory usage. Reconstructing by walking the parent chain is O(depth) per result. For 30 results with average depth 8, that's ~240 HashMap lookups -- microseconds.

**Decision**: `engine.rs` is pure -- no I/O, no DB access.
**Why**: It takes `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, and returns results. Trivially testable with synthetic data, no mocks. The hot path is isolated from all side effects.

**Decision**: `types.rs` (data definitions) is separate from `query.rs` (operations).
**Why**: `types.rs` is imported by everything (`engine.rs`, `query.rs`, `ai/`). Keeping it free of logic prevents circular dependencies and makes the data model easy to find.

**Decision**: AI pipeline lives in `search::ai`, not `commands/`.
**Why**: The parser, prompt, and query builder are search domain logic, not IPC concerns. `commands/search.rs` remains a thin IPC wrapper that calls into `search::ai`. AI-pipeline-internal decisions (key-value vs. JSON, classify-vs-compute, label field, single-pass) live in [`ai/CLAUDE.md`](ai/CLAUDE.md).

## Sharing with `selection/`

`crate::selection::history` re-exports `HistoryMode` and `HistoryFilters` from this
module's `history.rs`. The two pure data types are identical in intent across Search
and Selection, so the wire shape stays in sync. The entry struct itself
(`HistoryEntry` here vs `SelectionHistoryEntry`) stays separate because the canonical
dedupe keys differ (Selection has no scope or exclude-system-dirs). If the mode set
ever forks between consumers, drop the re-export and copy the types.

The AI parser helpers in `search/ai/parser.rs` are NOT shared with Selection: the
fields are different (Selection has no `keywords` / `type` / `scope` / `folders`), so
sharing would have meant exporting a few low-level helpers (`is_year`, `is_range`)
that aren't worth the coupling. Selection's parser lives independently in
`crate::selection::ai::parser`.

## Coupling to `indexing/`

`search/` is a read-only consumer of the indexing DB via `ReadPool`, `WRITER_GENERATION`, and `store::resolve_path`. This is intentional -- search reads from the index but doesn't participate in indexing. The dependency is one-way (`search` -> `indexing`, never reverse) and narrow:

- `index.rs` imports `ReadPool` from `indexing::enrichment` and `WRITER_GENERATION` from `indexing::writer`
- `engine.rs` imports `ROOT_ID` and `normalize_for_comparison` from `indexing::store`
- `query.rs` imports `store::resolve_path`, `IndexStore`, and `ROOT_ID` from `indexing::store`

## IPC layer

IPC commands live in `commands/search.rs` -- thin wrappers. `translate_search_query(natural_query, current_type)` orchestrates the AI pipeline: calls LLM with classification prompt (passing the dialog's `Both | Files | Folders` toggle as `current_type` context), parses response via `ai::parse_llm_response`, builds query via `ai::query_builder`. `resolve_ai_backend` stays in `commands/search.rs` since it touches `crate::ai` and `crate::settings` (Tauri-app concerns). The MCP `ai_search` executor calls it with `current_type = None`.

## Recent-searches history (`history.rs`)

Persistent store for the dialog's recent-searches footer and popover.

- **Persistence path**: `{app_data_dir}/search-history.json`. Schema-versioned via the `_schemaVersion` key (currently 1).
- **In-memory cache** + **disk lock**: in-memory `Mutex<HistoryStore>` mirrors `network::known_shares`; a separate
  `OnceLock<Mutex<()>>` (`DISK_LOCK`) serializes the read-modify-write cycle so concurrent IPC commands can't lose
  writes. The cache guard is always dropped before any `fs` call (no `.await` while holding a `MutexGuard`, no fs I/O
  while holding either guard).
- **Canonical dedupe key**: built at runtime from `mode | normalized_query | filters | scope | case_sensitive |
  exclude_system_dirs`. Filters serialize as alphabetically-keyed `k=v,k=v` pairs with undefined fields omitted. The
  key is never persisted; it only exists at compare time. Same canonical key = same search; the most recent copy wins
  (move-to-top), older copies dropped.
- **Recovery**: parse failure or schema-version mismatch → rename file to `.broken`, start fresh. The user keeps using
  the dialog; the corrupted file is preserved for one more rotation in case we want to debug.
- **Add-on-write hook**: history entries are added ONLY from the frontend "Open in pane" action. The Rust side doesn't
  enforce this — the IPC commands accept any entry — but the frontend's only call site for `addRecentSearch` is the
  Open-in-pane handler. The rationale (1000-entry budget stays signal-rich) lives in the Decision/Why block below.
- **Cap**: configurable via `search.recentSearches.maxCount` (default 1000). `apply_max_count` trims the in-memory store
  on live-apply; `0` clears everything and short-circuits future adds.

**Decision**: add only on "Open in pane", not on Enter / auto-apply.
**Why**: David's explicit design call. The 1000-entry budget is signal-rich when it tracks user intent (results worth
acting on) instead of every keystroke-debounced filename search. The Rust side doesn't enforce this gate — it's a
frontend convention. Don't paper over this with a "convenience" add-on-search call site.

**Decision**: `_schemaVersion` mismatch quarantines instead of migrating in-place.
**Why**: There's only schema v1 today; a migrator would be speculative code. When v2 lands, replace the quarantine
branch with a `match` on the version that calls a `migrate_v1_to_v2` helper.

## Gotchas

**Gotcha**: `name_folded` is NOT stored in the search index.
**Why**: The search pattern is NFD-normalized at query time on macOS (APFS filenames are already NFD). Avoids doubling memory for the name arena.

**Gotcha**: Arena-allocated filenames (`SearchIndex.names: String` buffer).
**Why**: Each `SearchEntry` stores `name_offset: u32` + `name_len: u16` instead of an owned `String`. During load, `row.get_ref(col).as_str()` borrows directly from SQLite's internal buffer (zero per-row heap allocations). `SearchIndex::name(&self, entry)` retrieves a `&str` slice from the arena.

**Gotcha**: `expand_tilde` imported from `crate::commands::file_system` in `ai/query_builder.rs`.
**Why**: Business logic reaching into the IPC layer. Architecturally backwards but pragmatic -- moving `expand_tilde` to a shared module would touch 20+ call sites across four files. Worth a separate cleanup PR.
