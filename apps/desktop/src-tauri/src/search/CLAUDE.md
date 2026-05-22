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
| `ai/mod.rs` | Re-exports from AI submodules |
| `ai/prompt.rs` | `CLASSIFICATION_PROMPT` const and `build_classification_prompt()`. Instructs the LLM to extract structured key-value fields (keywords, type, time, size, scope, exclude, folders, note) |
| `ai/parser.rs` | `ParsedLlmResponse` struct and `parse_llm_response()`. Key-value line parser with enum validation. `fallback_keywords()` for when LLM fails |
| `ai/mappings/` | Directory module: pure LLM enum → value conversions, split by domain |
| `ai/mappings/mod.rs` | Shared constants (`KB`, `MB`, `GB`, `KNOWN_EXTENSIONS`) + re-exports |
| `ai/mappings/type_mapping.rs` | Type enum → filename regex pattern (with `include_system_dirs` flag) |
| `ai/mappings/time_mapping.rs` | Time enum → (modified_after, modified_before) timestamps, date range parsing |
| `ai/mappings/size_scope_mapping.rs` | Size enum → byte range, scope enum → search paths |
| `ai/mappings/keyword_mapping.rs` | Keywords → glob/regex pattern, pattern merging with type filter, exclude parsing |
| `ai/query_builder.rs` | Assembles `SearchQuery` from parsed LLM output by calling mapping functions. Also `iso_date_to_timestamp()` (used by MCP executor) and display/caveat generation |

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
**Why**: The parser, prompt, and query builder are search domain logic, not IPC concerns. `commands/search.rs` remains a thin IPC wrapper that calls into `search::ai`.

**Decision**: AI uses key-value line output, not JSON.
**Why**: JSON generation is the #1 failure mode for small LLMs (13% parse failure on 2B models). Key-value lines (`keywords: rymd\ntype: documents\ntime: recent`) are trivial to produce and parse. Missing lines = no filter. Malformed lines are individually skippable without losing the whole response.

**Decision**: LLM classifies intent into enums, Rust computes values deterministically.
**Why**: Even small (2B) LLMs understand natural language across languages and can map "last week" to the token `last_week`. But asking them to generate regex, compute ISO dates, or produce valid JSON fails ~60% of the time on local models. Separating classification (LLM) from computation (Rust) makes the pipeline reliable regardless of model size.

**Decision**: Single LLM pass, no refinement.
**Why**: The previous two-pass system (translate + refine) caused regressions ~15% of the time (over-narrowing, flag dropping). With deterministic structure, there's nothing to refine. Also halves LLM latency.

## Coupling to `indexing/`

`search/` is a read-only consumer of the indexing DB via `ReadPool`, `WRITER_GENERATION`, and `store::resolve_path`. This is intentional -- search reads from the index but doesn't participate in indexing. The dependency is one-way (`search` -> `indexing`, never reverse) and narrow:

- `index.rs` imports `ReadPool` from `indexing::enrichment` and `WRITER_GENERATION` from `indexing::writer`
- `engine.rs` imports `ROOT_ID` and `normalize_for_comparison` from `indexing::store`
- `query.rs` imports `store::resolve_path`, `IndexStore`, and `ROOT_ID` from `indexing::store`

## IPC layer

IPC commands live in `commands/search.rs` -- thin wrappers. `translate_search_query` orchestrates the AI pipeline: calls LLM with classification prompt, parses response via `ai::parse_llm_response`, builds query via `ai::query_builder`. `resolve_ai_backend` stays in `commands/search.rs` since it touches `crate::ai` and `crate::settings` (Tauri-app concerns).

## Recent-searches history (`history.rs`)

Persistent store for the dialog's recent-searches footer and popover (`search-redesign-plan.md` §3.5).

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
- **Add-on-write hook**: history entries are added ONLY from the frontend "Open in pane" action (search-redesign-plan
  §3.5). The Rust side doesn't enforce this — the IPC commands accept any entry — but the frontend's only call site for
  `addRecentSearch` is the Open-in-pane handler (M8 wires it).
- **Cap**: configurable via `search.recentSearches.maxCount` (default 1000). `apply_max_count` trims the in-memory store
  on live-apply; `0` clears everything and short-circuits future adds.

**Decision**: add only on "Open in pane", not on Enter / auto-apply.
**Why**: David's explicit design call. The 1000-entry budget is signal-rich when it tracks user intent (results worth
acting on) instead of every keystroke-debounced filename search. The Rust side doesn't enforce this gate — it's a
frontend convention. Don't paper over this with a "convenience" add-on-search call site.

**Decision**: `_schemaVersion` mismatch quarantines instead of migrating in-place.
**Why**: M5 ships schema v1. There's no v2 yet, so a migrator would be speculative code. When v2 lands, replace the
quarantine branch with a `match` on the version that calls a `migrate_v1_to_v2` helper.

## Gotchas

**Gotcha**: `name_folded` is NOT stored in the search index.
**Why**: The search pattern is NFD-normalized at query time on macOS (APFS filenames are already NFD). Avoids doubling memory for the name arena.

**Gotcha**: Arena-allocated filenames (`SearchIndex.names: String` buffer).
**Why**: Each `SearchEntry` stores `name_offset: u32` + `name_len: u16` instead of an owned `String`. During load, `row.get_ref(col).as_str()` borrows directly from SQLite's internal buffer (zero per-row heap allocations). `SearchIndex::name(&self, entry)` retrieves a `&str` slice from the arena.

**Gotcha**: `expand_tilde` imported from `crate::commands::file_system` in `ai/query_builder.rs`.
**Why**: Business logic reaching into the IPC layer. Architecturally backwards but pragmatic -- moving `expand_tilde` to a shared module would touch 20+ call sites across four files. Worth a separate cleanup PR.
