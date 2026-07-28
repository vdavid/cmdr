# Search module

Multi-volume in-memory filename search + AI query translation. A scope routes to its owning volume(s); unscoped fans
out across every persisted `index-{volumeId}.db`. Flat API: `use crate::search::{SearchQuery, ...}`.

## Module map

- `index.rs`: `SearchIndex` (arena filename storage), `SearchEntry`, `load_search_index`.
- `volumes.rs`: per-volume registry + dialog/idle/backstop timers (all arenas drop together). `ensure_volume(id)` loads
  a volume's arena, mount root, and weights; non-root reads `index-{id}.db` from disk, NOT `INDEX_REGISTRY`.
- `execute.rs`: `run_blocking(query, policy)`, the multi-volume orchestrator (route → load → engine → merge), plus
  `ColdVolumePolicy` (who waits for a cold arena) and `RunOutcome` (result + `deferred_volumes`).
- `engine.rs`: `search_ranked()` PURE (no I/O): compiles glob/regex, rayon-filters, ranks, reconstructs
  mount-root-prefixed paths. Scope via `include_path_ids` / `exclude_dir_names`.
- `types.rs`: pure data, no logic. `query.rs`: operations on them (`parse_scope`, `resolve_include_scope`,
  formatters). `history.rs`: recent searches. `ai/`: NL → `SearchQuery` (`ai/CLAUDE.md`).

## Must-knows

- **`engine.rs` is pure: no I/O, no DB.** It's the hot path; keep it so.
- **`types.rs` stays free of logic** (imported by everything; risks circular deps).
- **`search/` is a one-way, read-only consumer of `indexing/`** — never the reverse.
- **Multi-volume: `execute.rs` routes + merges; the engine stays per-index/pure.**
  Non-root indices are mount-relative: PREFIX the mount root onto read paths, STRIP it from scope paths (mount-root
  scope = the WHOLE volume). Mount root = the `volume_path` meta OR the live registry (don't assume the meta is set).
  Honesty is TYPED: branch on `uncovered_scopes` (volume unindexed) / `unresolved_scopes` (path not found) emptiness,
  never a string match.
- **Count-only (`count_only`)**: exact per-volume totals, no rows — except under a dir-size filter, where the engine
  returns that volume's matching dirs and `run_blocking` MUST `fill_ranked_dir_sizes` then `count_only_volume_total`
  (else over-count).
- **Filenames are arena-allocated** (`name_offset` + `name_len` into one `String`): owned `String`s roughly double
  resident memory. `name_folded` is NOT stored; the pattern is NFD-normalized at query time instead.
- **`ImportanceWeights` keys on `hash_path(path)`, not the path** (17 B a folder; root's map is resident). ❌ No path
  keys, no enumeration API; `ranking/memory_tests.rs` guards it.
- **A stale root arena is SERVED, not rebuilt in front of the search**: `get_loaded` kicks a background refresh
  (≤1/30 s). Root's generation ticks several times a second, so reload-on-mismatch cost 2.6 s per search. Don't restore
  it.
- **A cold volume never blocks the DIALOG's search** (`ColdVolumePolicy::DeferColdVolumes`): it warms behind the
  reply; `search-index-ready` makes the dialog re-run so its matches fold in. Root and scoped volumes still wait; MCP
  passes `Wait`. Waited-for loads run in PARALLEL, single-flighted per volume.
- **One index per mounted LOCATION** (`distinct_mount_roots_in`): an SMB id keys on the ADDRESS, so one NAS over
  Tailscale and LAN has two 525 MB indexes both claiming `/Volumes/naspi`. Keep the live-registered one (else newest
  scan); touch no DB.
- **Ranking runs per MATCH** (a 1-letter query matches millions), so it's top-k + allocation-light: folder→weight memo,
  `hash_path_from_index` (never builds the path), ASCII-fast `classify_match`, `select_nth_unstable_by`. `bench.rs`
  measures it.
- **`ai/query_builder.rs` imports `expand_tilde` from `crate::commands::file_system`**: business logic reaching into
  the IPC layer, intentional (moving it touches 20+ call sites), not a silent fix.

## History store (`history.rs`)

- **Concurrency**: `Mutex<HistoryStore>` cache + a separate `DISK_LOCK` for the read-modify-write. Drop the cache
  guard before any `fs` call; no `.await` while holding a guard.
- **Add only on "Open in pane"**, never on Enter / auto-apply (David's call). Not Rust-enforced: the FE's only
  `addRecentSearch` call site is that handler.
- **`selection/` re-exports `HistoryMode` / `HistoryFilters`** (one-way; the entry structs stay separate). If the mode
  set forks, copy the types instead.
- **`resolve_ai_backend` stays in `commands/search.rs`** (it touches `crate::ai` + `crate::settings`).

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
