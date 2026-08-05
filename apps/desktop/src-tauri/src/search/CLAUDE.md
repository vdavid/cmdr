# Search module

In-memory filename search + AI query translation, **one volume per search**. A scope routes to the volume that owns it;
unscoped means the boot volume. Flat API: `use crate::search::{SearchQuery, ...}`.

## Module map

- `index.rs`: the arena-backed `SearchIndex`. `volumes.rs` (+ `volumes/weights.rs`): per-volume registry, drop timers,
  `ensure_volume(id)`, importance weights.
- `execute.rs`: `run_blocking(query)`, the orchestrator (route → load → engine). `engine.rs`: `search_ranked()`.
  `matcher.rs`: the compiled query. `ranking.rs`: the quality/importance blend.
- `types.rs`: pure data. `query.rs`: operations on it. `history.rs`: recent searches. `ai/`: NL → `SearchQuery`
  (`ai/CLAUDE.md`).

## Must-knows

- **Three purity rules**: `engine.rs` is PURE (no I/O, no DB); `types.rs` stays free of logic; `search/` is a one-way
  read-only consumer of `indexing/`, ❌ never the reverse.
- **One matcher, two evaluators**: `matcher.rs`'s `CompiledQuery` owns the name/type/size/date predicates, for the arena
  scan AND a live walk's batches. ❌ Never re-derive case folding or NFD normalization elsewhere: that fork is how an
  unindexed drive starts answering differently. ❌ Not in it: directory sizes (`dir_stats`, after ranking), the scope
  filter. The broad-query guard is per evaluator; a walk refuses outright.
- **One volume is the CEILING, enforced at the API** (`resolve_target` returns one target or `ScopeError`), not just in
  the UI. ❌ Don't reintroduce a fan-out: the only way a search can silently omit a drive
  (`docs/specs/unindexed-search-plan.md` Decision 4).
- **`execute.rs` routes.** Non-root indices are mount-relative: PREFIX the mount
  root onto read paths, STRIP it from scope paths (a mount-root scope means the WHOLE volume). Mount root = the
  `volume_path` meta OR the live registry; ❌ don't assume the meta is set.
- **Honesty is TYPED**: branch on `uncovered_scopes` (unindexed volume) / `unresolved_scopes` (path not in the index),
  ❌ never a string match. `target_volume_id` names the volume routing picked, so callers act on the right drive.
  `unresolved_scopes` can't tell a typo from a not-yet-walked folder: ❌ don't word it "doesn't exist".
- **`prepare_search_index`'s `loading` says whether an event is COMING.** `loading: false, ready: false` is the terminal
  "no index here"; without it a machine that declined indexing waits forever.
- **An SMB id keys on the ADDRESS**, so one NAS reached over both Tailscale and LAN has two index DBs both claiming
  `/Volumes/naspi`. Routing reads the LIVE-registered id only; ❌ don't add a read-time dedupe back.
- **Every search WAITS for its volume's arena** (10.9 s cold for a 13.5 M-entry NAS), accepted under Decision 4.
- **A stale root arena is SERVED, not rebuilt in front of the search**: `get_loaded` kicks a background refresh
  (≤1/30 s). ❌ Don't restore reload-on-mismatch: root's generation ticks several times a second (2.6 s per search).
- **Count-only** returns an exact total and no rows — except under a dir-size filter, where `run_blocking` MUST
  `fill_dir_sizes` then `count_only_volume_total`, else it over-counts.
- **Memory is the design constraint.** Filenames are arena-allocated (`name_offset` + `name_len` into one `String`), so
  ❌ no owned `String`s and ❌ no stored `name_folded`. `ImportanceWeights` keys on `hash_path(path)`, ❌ never the
  path, with no enumeration API (`ranking/memory_tests.rs` guards it); root's map is PATCHED per incremental recompute,
  so a LAGGED notice MUST rebuild it or it drifts.
- **Ranking runs per MATCH** (a one-letter query matches millions), so it stays top-k and allocation-light:
  `hash_path_from_index` never builds a path, `classify_match` is ASCII-fast (`bench.rs` measures it).
- **`history.rs` holds two locks** (a cache `Mutex`, then `DISK_LOCK`): ❌ no `fs` call or `.await` under a guard.
- **`ai/mappings/size_scope_mapping.rs` imports `expand_tilde` from `crate::commands::file_system`**: business logic
  reaching into the IPC layer, intentional (a move touches 20+ call sites), ❌ not a silent fix.

Rationale, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
