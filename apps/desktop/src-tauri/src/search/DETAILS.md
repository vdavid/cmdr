# Search details

Depth for the search backend. `CLAUDE.md` holds the must-knows; this file holds the design rationale.

## Decisions

- **In-memory `Vec` + rayon instead of SQLite queries**: the index has ~5M entries. SQLite `LIKE '%query%'` takes 1–3s
  (full table scan). Loading entries into a `Vec` and scanning with rayon gives sub-second results. The index loads
  lazily on dialog open and drops after idle (5 min timer + 10 min backstop), ~600 MB resident while active.
- **Structured `SearchQuery` model, not free-text SQL**: safe (no injection), composable (the AI mode fills the same
  struct), and simple to execute (single pass over the in-memory `Vec`). The frontend owns query building; the backend
  is a pure filter engine.
- **Path reconstruction at search time, not stored**: storing full paths would double memory. Reconstructing by walking
  the parent chain is O(depth) per result (for 30 results at average depth 8, ~240 HashMap lookups, microseconds).
- **The load arena is right-sized from the row count, not a fixed worst case**: `load_search_index` runs one
  `SELECT COUNT(*)` and reserves `Vec::with_capacity(count)` + `String::with_capacity(count * ~20 bytes)`, the arena
  estimate clamped to a 512 MiB ceiling so a bogus count can't request gigabytes (both still grow if the estimate runs
  low, so correctness is unchanged). A small index no longer pays the old fixed ~100 MB / 5M-slot allocation on every
  load.
- **`engine.rs` is pure (no I/O, no DB)**: it takes `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, returns
  results. Trivially testable without mocks; the hot path is isolated from side effects.
- **`types.rs` (data) separate from `query.rs` (operations)**: `types.rs` is imported by everything, so keeping it
  logic-free prevents circular dependencies and makes the data model easy to find.
- **AI pipeline lives in `search::ai`, not `commands/`**: the parser, prompt, and query builder are search domain logic,
  not IPC concerns; `commands/search.rs` stays a thin wrapper. AI-internal decisions live in [`ai/CLAUDE.md`](ai/CLAUDE.md).
- **Add history only on "Open in pane"**: David's explicit call. The 1000-entry budget stays signal-rich when it tracks
  results worth acting on, not every keystroke-debounced filename search. The gate is a frontend convention, not
  Rust-enforced.
- **Scope include-paths are canonicalized before the DB walk** (`resolve_include_paths` → `canonicalize_scope_path`):
  the scanner walks the real filesystem, so the index stores canonical paths (`/private/tmp/…`), while panes and agents
  report the symlinked form (`scope:/tmp/…`). Without resolving symlinks first, `store::resolve_path`'s literal
  component walk finds nothing → silent empty results. Canonicalization happens ONCE per include path (a handful),
  outside the hot per-entry scan, on a detached thread under a 2 s deadline (`realpath` blocks on a dead mount, and
  `resolve_include_paths` is sync — the sync analog of `blocking_with_timeout`); a non-existent / timed-out path keeps
  its literal so an offline-index scope still gets a best-effort match. Applies to `search`, `ai_search`, and the FE
  search dialog — all route through the one `resolve_include_paths`.
- **Count-only mode (`SearchQuery.count_only`) trades rows for an exact total, cheaply, across volumes**: when set,
  `engine::search_ranked` computes each volume's `total_count` from the filtered matches but skips ranking, truncation,
  and per-row path materialization (the expensive parts) and returns no rows; `execute.rs::run_blocking` sums the
  per-volume totals and skips the k-way merge, returning an empty `entries`. The one wrinkle is directory size filters:
  directory sizes live in `dir_stats` (the per-volume DB), not the in-memory index, so the pure engine can't size-filter
  directories. So when a size filter is set AND directories aren't excluded (`is_directory != Some(false)`),
  `search_ranked` hands that volume's matching directories back in its ranked slice (with the volume total still counting
  every match, files already size-filtered), and `run_blocking` fills their sizes via `fill_ranked_dir_sizes` then calls
  `count_only_volume_total`, which subtracts the directories outside the filter from that volume's total. Net: an exact
  count in every case, materializing only the matching directories per volume (never the -- usually far larger -- file
  set), and never merging or building rows. The MCP `search` tool formats the result as a bare line (`format_match_count`,
  e.g. `1,234 files match`) with any `uncovered_scopes` coverage note appended; the dialog shows it as a prominent count
  instead of the list (`QueryResults` count-only branch).
- **`_schemaVersion` mismatch quarantines instead of migrating in place**: there's only schema v1, so a migrator would
  be speculative. When v2 lands, replace the quarantine branch with a `match` on the version calling a
  `migrate_v1_to_v2` helper.

## Multi-volume search

Search spans every volume with a persisted `index-{volumeId}.db`, not just root. `execute.rs::run_blocking` owns the
orchestration; `engine.rs` stays per-index and pure.

### Routing

- **Scoped** (`include_paths` non-empty): each path routes to its owning volume via
  [`volume_id_for_local_path`](../indexing/routing.rs) (SMB mount → `smb_volume_id`, `mtp://` → `{device}:{storage}`,
  registered external mount → its id, everything else → `root`). Paths group by volume; each target is `from_scope`.
- **Unscoped**: `volumes::all_indexed_volume_ids` enumerates every `index-*.db` in the data dir (root first). Whole-volume each.

### Per-volume load (`volumes.rs`)

`ensure_volume(id)` is cache-aware. Root's pool is the live `get_read_pool()`; a NON-root volume opens a read-only
`ReadPool` straight from `index-{id}.db` on disk — deliberately NOT via `INDEX_REGISTRY`, because the DB file is the
source of truth and an ejected/unmounted drive's index is still searchable. The mount root comes from the DB's
`volume_path` meta (so it's known offline). Lifecycle is dialog-scoped, not per-volume: opening the dialog pre-loads
root and arms the timers; a search lazily loads its scope's volumes; idle/backstop drops ALL arenas at once (RAM
reclaim). A long root pre-load is cancelable (`cancel_active_loads` on dialog close).

**Staleness.** Only the root writer bumps the global `WRITER_GENERATION`, so `get_loaded` reloads root when it moved
past the stamp; a non-root volume stamps `0` and simply reloads next dialog session. That's acceptable: a NAS/MTP index
is far less volatile than the boot disk, and every arena drops on idle regardless.

### Mount-relative path spaces (the load-bearing gotcha)

A non-root volume's index `ROOT_ID` is its MOUNT ROOT, so it stores mount-relative paths (`/sub/file`, not
`/Volumes/naspi/sub/file`). Two mirror transforms bridge the spaces:

- **Read side** — `engine::search_ranked` takes a `path_prefix` (the mount root, empty for root) and PREPENDS it to
  every reconstructed path, so a NAS result reports `/Volumes/naspi/sub/file` and opens in a pane.
- **Scope side** — `query::resolve_include_path_ids` STRIPS the mount root from each include path before
  `store::resolve_path` (which walks from `ROOT_ID`). A path outside the mount root resolves to nothing.

Without either, an indexed NAS folder would show bare paths that don't open, or a scope would match zero entries.

### Merge

Each volume's `search_ranked` returns entries already ranked best-first WITH their [`RankKey`](ranking.rs) (band +
importance-boosted recency + id). The keys are volume-independent scalars, so `run_blocking` concatenates the
per-volume slices and does ONE global `sort_by(RankKey::cmp_best_first)`, then truncates to the limit — a correct top-k
merge because each slice is already its volume's top-k. `total_count` sums the per-volume match totals. Directory sizes
are filled per volume (each from its own pool) BEFORE the merge, so the size post-filter runs against the right
`dir_stats`.

### Honesty: `uncovered_scopes`

A `from_scope` target whose volume has no persisted index (`VolumeLoad::NotIndexed`) is NOT silently empty — its scope
paths ride back in `SearchResult::uncovered_scopes`, a TYPED field callers branch on by emptiness (never string-match).
The dialog and MCP render "Cmdr hasn't indexed X yet" instead of "no files found", so an unindexed NAS scope reads
honestly. Partial coverage works too: covered volumes still return results alongside the note. An unscoped unindexed
volume is skipped silently (no user intent to honor).

## History store (`history.rs`)

- **Persistence**: `{app_data_dir}/search-history.json`, schema-versioned via `_schemaVersion` (currently 1). On parse
  failure or version mismatch, rename to `.broken` and start fresh (corrupt file kept one rotation for debugging). A
  `_schemaVersion` mismatch quarantines rather than migrating in place — there's only v1, so a migrator would be
  speculative; when v2 lands, replace the quarantine branch with a `match` on the version.
- **Canonical dedupe key** (compare-time only, never persisted): `mode | normalized_query | filters | scope |
  case_sensitive | exclude_system_dirs`. Same key = same search; the most recent copy wins (move-to-top).
- **Cap**: `search.recentSearches.maxCount` (default 1000). `apply_max_count` trims in-memory on live-apply; `0` clears
  and short-circuits future adds.

## Image-OCR search boundary (`media_index`)

"Text in images" search is a SEPARATE query path from filename search, and it reaches a volume's `media.db` ONLY through
the [`MediaIndex`](../media_index/read/mod.rs) read API — never a raw `rusqlite` dep on `media.db` (plan Decision 8), so
that store's `platform_case`/one-writer invariants don't leak into a second subsystem. The door is the
`media_index_search_ocr` command (`media_index/commands.rs`), which returns `OcrHit { path, snippet }` (the snippet is
the highlighted "why matched" reason). The frontend query-ui that blends OCR hits into the results surface is a later
slice; `search/` itself takes no dependency on `media_index` today.

## Importance ranking (`ranking.rs`)

Search ranks interesting files toward the top by blending a result's match quality with its parent folder's importance
weight (the first consumer of the [`importance/`](../importance/DETAILS.md) subsystem). The ranker is a pure module;
[`engine.rs`](engine.rs) stays pure by receiving importance as DATA (a prebuilt weight map), never querying a DB.

### The blend: quality bands, importance within a band

The load-bearing requirement is that **match quality dominates**: an exact/prefix name match must beat a weaker match no
matter how important the weaker match's folder is. We get this BY CONSTRUCTION with a lexicographic sort:

1. **Match-quality band first** (`MatchQuality`: `Exact` > `Prefix` > `Other`). Importance is applied only WITHIN a band,
   so it can never lift a result across a band boundary — "exact filename in a boring folder beats fuzzy match in
   Documents" holds for any weight. The dominance property is pinned by
   `exact_match_beats_fuzzy_match_regardless_of_importance` (written first against a deliberately-wrong blend that folds
   importance into the band comparison, seen to fail, then fixed).
2. **Importance-boosted recency within a band**: the key is `recency * (1 + IMPORTANCE_BLEND_COEFF * weight)`. A modest
   multiplicative nudge (`IMPORTANCE_BLEND_COEFF = 0.5`, a named future tunable): at max weight `1.0` a result's recency
   key scales by `1.5`, enough to win a same-quality tie against a result up to ~half a recency-order newer, never enough
   to matter across bands. With weight `0.0` the multiplier is exactly `1.0`, so within-band order collapses to pure
   recency.
3. **Id-ascending final tiebreak** for run-to-run determinism.

**A file takes its parent folder's weight; a folder takes its own.** The engine reconstructs the folder's absolute path
and looks it up in the weight map. Absent a weight (unscored, floored, or missing DB), the lookup is `0.0` — neutral,
never a penalty.

**Only a wildcard-free plain query has a quality gradient.** The `stem` fed to the ranker is the raw pattern only when
it's a glob with no `*`/`?` (the auto-wrapped `*stem*` case); a wildcard glob or regex yields an empty stem, so every
result lands in the `Other` band and recency alone orders — unchanged from before this feature (and matching how those
patterns behaved). On macOS the stem is NFD-normalized like the matcher's pattern, so it compares against the arena's NFD
filenames.

### The degradation contract

**Absent importance data, ranking equals today's behavior.** When the weight map is empty (offline volume, fresh
install, disabled indexing, a purged `importance.db`, or a recompute that hasn't run yet), every weight is `0.0`, so the
within-band multiplier is `1.0` and the sort is pure recency within each band — byte-for-byte the pre-importance
ordering. Pinned by `empty_weights_within_band_is_pure_recency` and `empty_weights_and_no_stem_is_pure_recency`. The
engine also takes an empty-map fast path (skipping the per-result parent-path reconstruction entirely).

### The weight-map lifecycle (`volumes.rs`)

Per-volume weight maps live in the `WEIGHTS` map (`volume_id → Arc<ImportanceWeights>`) in `volumes.rs`, built ONCE
from [`ImportanceIndex::all_nonzero_weights`](../importance/read/mod.rs) and never queried per result (a search ranks
tens of thousands of candidates):

- **Loaded with the arena, cloned per search.** `ensure_volume` loads a volume's weights alongside its arena;
  `run_blocking` clones the cheap `Arc` per target and ranks against a stable snapshot even if a reload swaps the map
  mid-search. Kept SEPARATE from `LoadedVolume` so the root recompute subscriber can swap root's map without rebuilding
  the arena.
- **Subscribe (root), snapshot (non-root).** `start_importance_weight_subscriber` (wired from `lib.rs` setup, which
  also records the app data dir) subscribes to root's [`read::subscribe`](../importance/read/mod.rs) recompute `watch`
  and reloads root's weights on each pass, plus once up front. A non-root volume takes a load-time snapshot instead: it
  drops on idle and reloads next session, and its importance rarely recomputes mid-session. A volume with no
  `importance-{id}.db` degrades to match-quality + recency (empty map).
- **Only non-zero weights enter the map.** Floored folders have NO row in `importance.db` (the store's compaction — see
  [`importance/DETAILS.md`](../importance/DETAILS.md) storage model), and `all_nonzero_weights` also filters `score > 0`,
  so the ~312k folders under `node_modules` on a 646k-folder home never enter the map (their lookup defaults to `0.0`
  anyway). Footprint is a `HashMap<String, f64>` over the non-floored folders: absolute-path keys plus an `f64`, order
  tens of MB on a large home. If it ever grows heavy, switch to folder-id or hashed-path keys.
- **A missing DB is empty, not an error.** `all_nonzero_weights` short-circuits to an empty map when the file is absent
  (a read-only open would fail `CannotOpen`), so an unscored volume degrades cleanly.

The blend coefficient and weight-map footprint are unvalidated starting points (the importance weights themselves are
too — see [`importance/scorer/weights.rs`](../importance/scorer/weights.rs)).
