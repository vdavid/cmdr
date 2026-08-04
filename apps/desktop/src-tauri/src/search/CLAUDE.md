# Search module

In-memory filename search + AI query translation, **one volume per search**. A scope routes to the single volume that
owns it; unscoped means the boot volume. Flat API: `use crate::search::{SearchQuery, ...}`.

## Module map

- `index.rs`: the arena-backed `SearchIndex`. `volumes.rs` (+ `volumes/weights.rs`): per-volume registry, drop timers,
  `ensure_volume(id)`, importance weights.
- `execute.rs`: `run_blocking(query)`, the orchestrator (route → load → engine). `engine.rs`: `search_ranked()`.
  `ranking.rs`: the quality/importance blend.
- `types.rs`: pure data. `query.rs`: operations on it. `history.rs`: recent searches. `ai/`: NL → `SearchQuery`
  (`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB) and is the hot path; `types.rs` stays free of logic (it's
  imported by everything, so logic risks circular deps); `search/` is a one-way read-only consumer of `indexing/`, ❌
  never the reverse.
- **One volume is the CEILING, enforced at the API** (`resolve_target` returns one target or `ScopeError`), not just in
  the UI. ❌ Don't reintroduce a fan-out: it's the only way a search can silently omit a drive
  (`docs/specs/unindexed-search-plan.md` Decision 4).
- **`execute.rs` routes; the engine stays per-index and pure.** Non-root indices are mount-relative: PREFIX the mount
  root onto read paths, STRIP it from scope paths (a mount-root scope means the WHOLE volume). Mount root = the
  `volume_path` meta OR the live registry, so ❌ don't assume the meta is set.
- **Honesty is TYPED**: branch on `uncovered_scopes` (unindexed volume) / `unresolved_scopes` (path not found), ❌ never
  a string match.
- **An SMB id keys on the ADDRESS**, so one NAS reached over both Tailscale and LAN has two index DBs both claiming
  `/Volumes/naspi`. Routing resolves that scope to the LIVE-registered id, so only one is ever read; ❌ don't add a
  read-time dedupe back.
- **Every search WAITS for its volume's arena** (10.9 s cold for a 13.5 M-entry NAS). Accepted under Decision 4; M6
  voices the wait.
- **A stale root arena is SERVED, not rebuilt in front of the search**: `get_loaded` kicks a background refresh
  (≤1/30 s). ❌ Don't restore reload-on-mismatch; root's generation ticks several times a second, so it cost 2.6 s per
  search.
- **Count-only** returns an exact total and no rows — except under a dir-size filter, where `run_blocking` MUST
  `fill_dir_sizes` then `count_only_volume_total`, else it over-counts.
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
