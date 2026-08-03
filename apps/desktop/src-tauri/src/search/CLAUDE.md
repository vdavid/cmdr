# Search module

Multi-volume in-memory filename search + AI query translation. A scope routes to its owning volume(s); unscoped fans
out across every persisted `index-{volumeId}.db`. Flat API: `use crate::search::{SearchQuery, ...}`.

## Module map

- `index.rs`: the arena-backed `SearchIndex`. `volumes.rs` (+ `volumes/weights.rs`): per-volume registry, drop timers,
  `ensure_volume(id)`, importance weights.
- `execute.rs`: `run_blocking(query, policy)`, the orchestrator (route → load → engine → merge). `engine.rs`:
  `search_ranked()`. `ranking.rs`: the quality/importance blend.
- `types.rs`: pure data. `query.rs`: operations on it. `history.rs`: recent searches. `ai/`: NL → `SearchQuery`
  (`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB) and is the hot path; `types.rs` stays free of logic (it's
  imported by everything, so logic risks circular deps); `search/` is a one-way read-only consumer of `indexing/`, ❌
  never the reverse.
- **`execute.rs` routes and merges; the engine stays per-index and pure.** Non-root indices are mount-relative: PREFIX
  the mount root onto read paths, STRIP it from scope paths (a mount-root scope means the WHOLE volume). Mount root =
  the `volume_path` meta OR the live registry, so ❌ don't assume the meta is set.
- **Honesty is TYPED**: branch on `uncovered_scopes` (unindexed volume) / `unresolved_scopes` (path not found), ❌ never
  a string match.
- **One index per mounted LOCATION** (`distinct_mount_roots_in`): an SMB id keys on the ADDRESS, so one NAS reached over
  both Tailscale and LAN yields two indexes claiming `/Volumes/naspi`. Keep the live-registered one, and touch no DB.
- **A cold volume never blocks the DIALOG's search** (`ColdVolumePolicy::DeferColdVolumes`): it warms behind the reply,
  and `search-index-ready` re-runs the dialog. Root and scoped volumes still wait (MCP passes `Wait`).
- **A stale root arena is SERVED, not rebuilt in front of the search**: `get_loaded` kicks a background refresh
  (≤1/30 s). ❌ Don't restore reload-on-mismatch; root's generation ticks several times a second, so it cost 2.6 s per
  search.
- **Count-only** returns exact per-volume totals and no rows — except under a dir-size filter, where `run_blocking` MUST
  `fill_ranked_dir_sizes` then `count_only_volume_total`, else it over-counts.
- **Memory is the design constraint.** Filenames are arena-allocated (`name_offset` + `name_len` into one `String`), so
  ❌ no owned `String`s and ❌ no stored `name_folded` (the pattern is NFD-normalized at query time instead).
  `ImportanceWeights` keys on `hash_path(path)`, ❌ never the path, with no enumeration API
  (`ranking/memory_tests.rs` guards it); root's map is PATCHED per incremental recompute, so a LAGGED notice MUST
  rebuild it or it silently drifts.
- **Ranking runs per MATCH** (a one-letter query matches millions), so it stays top-k and allocation-light:
  `hash_path_from_index` never builds a path, `classify_match` is ASCII-fast. `bench.rs` measures it.
- **`history.rs` holds two locks** (a cache `Mutex`, then `DISK_LOCK`): ❌ no `fs` call or `.await` while holding a
  guard.
- **`ai/mappings/size_scope_mapping.rs` imports `expand_tilde` from `crate::commands::file_system`**: business logic
  reaching into the IPC layer, intentional (a move touches 20+ call sites), ❌ not a silent fix.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
