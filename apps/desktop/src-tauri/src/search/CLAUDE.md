# Search module

Multi-volume in-memory filename search + AI query translation. A scope routes to its owning volume(s); an unscoped
query fans out across every volume with a persisted `index-{volumeId}.db`. Flat API:
`use crate::search::{SearchQuery, ...}`.

## Module map

- `index.rs`: `SearchIndex` (arena-allocated filename storage), `SearchEntry`, `load_search_index` (the arena loader).
- `volumes.rs`: per-volume registry + dialog/idle/backstop timers (drop ALL arenas at once). `ensure_volume(id)` lazily
  loads a volume's arena, mount root, and weights; a non-root volume opens read-only from `index-{id}.db` on disk, NOT
  via `INDEX_REGISTRY`.
- `execute.rs`: `run_blocking(query)`, the multi-volume orchestrator (route → load → per-volume engine → merge).
- `engine.rs`: `search_ranked()` PURE (no I/O): compiles glob/regex, rayon-filters, ranks, reconstructs
  mount-root-prefixed paths. Scope via `include_path_ids` / `exclude_dir_names`.
- `types.rs`: pure data, no logic. `query.rs`: operations on the types (`parse_scope`, `resolve_include_scope`,
  formatters, `SYSTEM_DIR_EXCLUDES`).
- `history.rs`: recent-searches store (below). `ai/`: NL → `SearchQuery` translation (`ai/CLAUDE.md`).

## Must-knows

- **`engine.rs` is pure: no I/O, no DB.** The hot path, trivially testable. Keep it so.
- **`types.rs` stays free of logic** (imported by everything; logic risks circular deps).
- **`search/` is a read-only, one-way consumer of `indexing/`** (never the reverse): it imports `ReadPool`,
  `WRITER_GENERATION`, and store helpers. It reads the index, doesn't participate in indexing.
- **Multi-volume: `execute.rs` routes + merges; the engine stays per-index/pure** (DETAILS § Multi-volume search).
  Non-root indices are mount-relative: PREFIX the mount root onto read paths, STRIP it from scope paths (mount-root
  scope = the WHOLE volume). Mount root = the `volume_path` meta OR the live registry (don't assume the meta is set).
  Two typed honesty fields (branch on emptiness, never string-match): `uncovered_scopes` (volume unindexed),
  `unresolved_scopes` (path not found).
- **Count-only (`count_only`)**: `search_ranked` returns exact per-volume totals, no rows; with a dir-size filter it
  returns that volume's matching dirs, so `run_blocking` MUST `fill_ranked_dir_sizes` then `count_only_volume_total`
  (else over-count).
- **Filenames are arena-allocated**: `SearchEntry` holds `name_offset: u32` + `name_len: u16` into one `String` (zero
  per-row heap alloc). Don't switch to owned `String`s (roughly doubles resident memory). `name_folded` is NOT stored:
  the pattern is NFD-normalized at query time (APFS filenames are already NFD).
- **`ImportanceWeights` keys on `hash_path(path)`, not the path** (17 B a folder; root's map is resident). ❌ No path
  keys, no enumeration API; `ranking/memory_tests.rs` guards it.
- **Ranking runs per MATCH** (a 1-letter query matches millions), so it's top-k + allocation-light: folder→weight memo,
  `hash_path_from_index` (never builds the path), ASCII-fast `classify_match`, `select_nth_unstable_by`. Keep it so;
  `bench.rs` measures it.
- **`ai/query_builder.rs` imports `expand_tilde` from `crate::commands::file_system`**: business logic reaching into
  the IPC layer, kept because moving it touches 20+ call sites. Intentional, not a silent fix.

## History store (`history.rs`)

- **Concurrency**: `Mutex<HistoryStore>` cache + a separate `DISK_LOCK` serializing the read-modify-write. Drop the
  cache guard before any `fs` call; no `.await` while holding a guard.
- **Add only on "Open in pane"**, never on Enter / auto-apply (David's call). Not Rust-enforced: the FE's ONLY
  `addRecentSearch` call site is the Open-in-pane handler.
- Persistence, dedupe-key, and cap: `DETAILS.md` § History store.

## Sharing + IPC

- **`selection/` re-exports `HistoryMode` / `HistoryFilters` from `history.rs`** (one-way; the entry structs stay
  separate). If the mode set forks, drop the re-export and copy the types.
- **`commands/search.rs`** holds thin wrappers; `translate_search_query` orchestrates the AI pipeline; `resolve_ai_backend`
  stays there (it touches `crate::ai` + `crate::settings`).

Full rationale (in-memory-Vec vs SQLite, path reconstruction, schema migration): `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
