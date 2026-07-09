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
- **`engine.rs` is pure (no I/O, no DB)**: it takes `&SearchIndex` + `&SearchQuery`, scans in-memory with rayon, returns
  results. Trivially testable without mocks; the hot path is isolated from side effects.
- **`types.rs` (data) separate from `query.rs` (operations)**: `types.rs` is imported by everything, so keeping it
  logic-free prevents circular dependencies and makes the data model easy to find.
- **AI pipeline lives in `search::ai`, not `commands/`**: the parser, prompt, and query builder are search domain logic,
  not IPC concerns; `commands/search.rs` stays a thin wrapper. AI-internal decisions live in [`ai/CLAUDE.md`](ai/CLAUDE.md).
- **Add history only on "Open in pane"**: David's explicit call. The 1000-entry budget stays signal-rich when it tracks
  results worth acting on, not every keystroke-debounced filename search. The gate is a frontend convention, not
  Rust-enforced.
- **`_schemaVersion` mismatch quarantines instead of migrating in place**: there's only schema v1, so a migrator would
  be speculative. When v2 lands, replace the quarantine branch with a `match` on the version calling a
  `migrate_v1_to_v2` helper.

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

### The weight-map lifecycle (`index.rs`)

The per-volume weight map is built ONCE from [`ImportanceIndex::all_nonzero_weights`](../importance/read/mod.rs) and
reloaded on each recompute — never queried per result (a search ranks tens of thousands of candidates):

- **Owned beside the warm arena.** `IMPORTANCE_WEIGHTS` (an `Arc<ImportanceWeights>` snapshot) lives next to
  `SEARCH_INDEX` in `index.rs`. `search_files` (and the MCP `search`/`ai_search` path, one shared integration point)
  clone the cheap `Arc` and rank against a stable snapshot even if a reload swaps the map mid-search.
- **Subscribe, don't poll.** `start_importance_weight_subscriber` (wired from `lib.rs` setup) subscribes to the root
  volume's [`read::subscribe`](../importance/read/mod.rs) recompute-completed `watch` and reloads the map on each pass,
  plus once up front. Search is root-only (it loads `get_read_pool()`, the root drive index), so the map mirrors
  `importance-root.db`.
- **Only non-zero weights enter the map.** Floored folders have NO row in `importance.db` (the store's compaction — see
  [`importance/DETAILS.md`](../importance/DETAILS.md) storage model), and `all_nonzero_weights` also filters `score > 0`,
  so the ~312k folders under `node_modules` on a 646k-folder home never enter the map (their lookup defaults to `0.0`
  anyway). Footprint is a `HashMap<String, f64>` over the non-floored folders: absolute-path keys plus an `f64`, order
  tens of MB on a large home. If it ever grows heavy, switch to folder-id or hashed-path keys.
- **A missing DB is empty, not an error.** `all_nonzero_weights` short-circuits to an empty map when the file is absent
  (a read-only open would fail `CannotOpen`), so an unscored volume degrades cleanly.

The blend coefficient and weight-map footprint are unvalidated starting points (the importance weights themselves are
too — see [`importance/scorer/weights.rs`](../importance/scorer/weights.rs)).
