# Search module

Multi-volume in-memory filename search + AI query translation. A scope routes to its owning volume(s); an unscoped query
fans out across every volume with a persisted `index-{volumeId}.db` and merges. Flat API:
`use crate::search::{SearchQuery, SearchResult, ...}`.

## Module map

- `index.rs`: `SearchIndex` (arena-allocated filename storage), `SearchEntry`, `load_search_index` (the arena loader).
- `volumes.rs`: per-volume registry + dialog/idle/backstop timers (drop ALL arenas at once). `ensure_volume(id)` lazily
  loads a volume's arena, mount root (`volume_path` meta), and weights; a non-root volume opens read-only from
  `index-{id}.db` on disk, NOT via `INDEX_REGISTRY`.
- `execute.rs`: `run_blocking(query)`, the multi-volume orchestrator (route → load → per-volume engine → merge).
- `engine.rs`: `search_ranked()` PURE (no I/O): compiles glob/regex, rayon-filters, ranks, reconstructs
  mount-root-prefixed paths. Scope via `include_path_ids` / `exclude_dir_names`.
- `types.rs`: pure data, no logic. `query.rs`: operations on the types (`parse_scope`, `resolve_include_scope`,
  formatters, `SYSTEM_DIR_EXCLUDES`).
- `history.rs`: persistent recent-searches store (see below). `ai/`: NL → `SearchQuery` translation
  (`ai/CLAUDE.md`).

## Must-knows

- **`engine.rs` is pure: no I/O, no DB.** The hot path, isolated from side effects and trivially testable. Keep it so.
- **`types.rs` stays free of logic** (imported by everything; logic risks circular deps).
- **`search/` is a read-only, one-way consumer of `indexing/`** (`search → indexing`, never reverse): imports
  `ReadPool`, `WRITER_GENERATION`, store helpers, `volume_id_for_local_path`. It reads the index, doesn't participate in
  indexing.
- **Multi-volume: `execute.rs` routes + merges; the engine stays per-index/pure** (DETAILS § Multi-volume search).
  Non-root indices are mount-relative: PREFIX the mount root onto read paths, STRIP it from scope paths (a mount-root
  scope = the WHOLE volume). Mount root = the `volume_path` meta OR the live registry (SMB DBs historically lacked the
  meta; don't assume it's set). Two typed honesty fields (branch on emptiness, never string-match): `uncovered_scopes`
  (volume unindexed), `unresolved_scopes` (path not found). Only the root writer bumps `WRITER_GENERATION`.
- **Count-only (`count_only`)**: `search_ranked` returns exact per-volume totals with no rows; with a dir-size filter it
  returns that volume's matching dirs, so `run_blocking` MUST `fill_ranked_dir_sizes` then `count_only_volume_total`
  (else over-count).
- **Filenames are arena-allocated**: `SearchEntry` holds `name_offset: u32` + `name_len: u16`, borrowing from SQLite's
  buffer during load (zero per-row heap alloc). Don't switch to owned `String`s (roughly doubles resident memory).
  `name_folded` is NOT stored: the pattern is NFD-normalized at query time (APFS filenames are already NFD).
- **`expand_tilde` is imported from `crate::commands::file_system` in `ai/query_builder.rs`**: business logic reaching
  into the IPC layer, kept because moving it touches 20+ call sites. Backwards but intentional; a separate cleanup, not
  a silent "fix" here.

## History store (`history.rs`)

- **Concurrency**: `Mutex<HistoryStore>` cache + a separate `DISK_LOCK` serializing the read-modify-write. Drop the cache
  guard before any `fs` call; no `.await` while holding a guard.
- **Add only on "Open in pane"**, never on Enter / auto-apply (David's call). Not Rust-enforced; the FE's ONLY
  `addRecentSearch` call site is the Open-in-pane handler. Don't add a "convenience" one.
- Persistence, dedupe-key, and cap: `DETAILS.md` § History store.

## Sharing + IPC

- **`selection/` re-exports `HistoryMode` / `HistoryFilters` from `history.rs`** (one-way; the entry structs stay
  separate, canonical keys differ). If the mode set forks, drop the re-export and copy the types.
- **`commands/search.rs`** holds thin wrappers; `translate_search_query` orchestrates the AI pipeline; `resolve_ai_backend`
  stays there (touches `crate::ai` + `crate::settings`). The MCP `ai_search` executor calls it with `current_type = None`.

Full rationale (in-memory-Vec vs SQLite, path reconstruction, schema-migration policy): `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
