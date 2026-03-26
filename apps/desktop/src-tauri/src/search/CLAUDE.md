# Search module

In-memory search index and AI-powered query translation for whole-drive file search.

## Module structure

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports from submodules. Flat API: `use crate::search::{SearchQuery, search, ...}` |
| `index.rs` | `SearchIndex` (arena-allocated filename storage), `SearchEntry`, global `SEARCH_INDEX` state with idle/backstop timers and load cancellation |
| `engine.rs` | `search()` pure function (no I/O): compiles glob/regex, parallel-filters entries with rayon, sorts by recency. Scope filtering via `include_path_ids` and `exclude_dir_names` |
| `types.rs` | Pure data definitions: `SearchQuery`, `SearchResult`, `SearchResultEntry`, `ParsedScope`, `PatternType`, `default_limit`. No logic |
| `query.rs` | Operations on the types: `parse_scope()`, `resolve_include_paths()` (DB pre-query), `fill_directory_sizes()` (DB post-query), `format_size()`, `format_timestamp()`, `summarize_query()`, `SYSTEM_DIR_EXCLUDES` |
| `ai/mod.rs` | Re-exports from AI submodules |
| `ai/prompt.rs` | `CLASSIFICATION_PROMPT` const and `build_classification_prompt()`. Instructs the LLM to extract structured key-value fields (keywords, type, time, size, scope, exclude, folders, note) |
| `ai/parser.rs` | `ParsedLlmResponse` struct and `parse_llm_response()`. Key-value line parser with enum validation. `fallback_keywords()` for when LLM fails |
| `ai/mappings.rs` | Pure LLM enum → value conversions: type-to-regex, time-to-timestamp, size-to-bytes, scope-to-paths, keyword-to-pattern, pattern merging, exclude parsing |
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

## Gotchas

**Gotcha**: `name_folded` is NOT stored in the search index.
**Why**: The search pattern is NFD-normalized at query time on macOS (APFS filenames are already NFD). Avoids doubling memory for the name arena.

**Gotcha**: Arena-allocated filenames (`SearchIndex.names: String` buffer).
**Why**: Each `SearchEntry` stores `name_offset: u32` + `name_len: u16` instead of an owned `String`. During load, `row.get_ref(col).as_str()` borrows directly from SQLite's internal buffer (zero per-row heap allocations). `SearchIndex::name(&self, entry)` retrieves a `&str` slice from the arena.

**Gotcha**: `expand_tilde` imported from `crate::commands::file_system` in `ai/query_builder.rs`.
**Why**: Business logic reaching into the IPC layer. Architecturally backwards but pragmatic -- moving `expand_tilde` to a shared module would touch 20+ call sites across four files. Worth a separate cleanup PR.
