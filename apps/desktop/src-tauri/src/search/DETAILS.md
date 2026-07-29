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
  not IPC concerns; `commands/search.rs` stays a thin wrapper. AI-internal decisions live in `ai/CLAUDE.md`.
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
  [`volume_id_for_local_path`](../indexing/paths/routing.rs) (SMB mount → `smb_volume_id`, `mtp://` → `{device}:{storage}`,
  registered external mount → its id, everything else → `root`). Paths group by volume; each target is `from_scope`.
- **Unscoped**: `volumes::all_indexed_volume_ids` enumerates every `index-*.db` in the data dir (root first), collapsed
  to one index per mounted location (below). Whole-volume each.

### One index per mounted location

`volumes::distinct_mount_roots_in` drops any index whose mount root another index already covers. It has to, because an
SMB volume id is `smb_volume_id(server, port, share)` — keyed on the ADDRESS the share happened to be mounted from. One
NAS reached over Tailscale and over the LAN therefore gets two ids, two full index DBs (David's box: 2.6 M entries and
~525 MB EACH, both stamping `volume_path = /Volumes/naspi`), and an unscoped search that scanned both, merged duplicate
hits, doubled the reported match count, and held two arenas resident.

The rule is about PLACES, not boxes: a mount root is a unique path, only one volume can be mounted there at a time, and
search reports ABSOLUTE paths, so two indexes claiming the same root necessarily describe the same paths. At most one of
them is what's actually there, so reporting both is never right. The keeper is the volume currently registered in
`VolumeManager` (that IS what's mounted there); failing that, the highest `scan_completed_at`, the more recent picture.
An index with NO known mount root (root itself, or an offline DB predating the `volume_path` meta) never enters a group,
so it's always kept — an unknown location can't be shown to collide.

Mount roots are read via `peek_index_identity`: a warm arena's cached value, else a read-only open of `index-{id}.db`
plus two `meta` lookups. That's a file open and two b-tree hits (sub-millisecond), NOT an arena load.

Nothing on disk is deleted or rewritten — this is a read-time choice, so the skipped DB wins straight back the moment
it's the one mounted, and no user index is ever lost. Fixing the ROOT cause (keying the index on a stable server
identity instead of `host:port`) was deliberately NOT attempted: see "Why the index isn't keyed on a stable server
identity" below.

### Who waits for a cold arena

Loading a volume's arena is a multi-second, multi-hundred-MB read (2.4 s warm page cache for a 2.6 M-entry NAS index,
10.9 s observed cold in prod). `ColdVolumePolicy` decides who pays:

- **`DeferColdVolumes`** (the search dialog): a cold, unscoped, non-root target is dropped from this run and returned in
  `RunOutcome::deferred_volumes`. `commands::search::search_files` warms each one via `volumes::warm_in_background` and
  emits `SearchIndexReadyEvent` as it lands; the dialog's existing ready listener re-runs the query, so the volume's
  matches fold into results that already came back. Converges: once warm, nothing defers and nothing re-emits. A warm-up
  that hasn't started when the dialog closes declines (nobody's waiting, and the idle timer would drop the arena
  straight away); one already in flight is stopped by `cancel_active_loads`.
- **`Wait`** (MCP `search` / `ai_search`): a tool call gets ONE shot at an answer with no dialog to re-run it, so it
  waits for everything and stays complete.

Root and any `from_scope` target are never deferred under either policy: answering "nothing found" for the one place the
user asked about would be a lie, not a fast path, and the dialog's readiness and entry count are root's.

Whatever a run does wait for loads in PARALLEL (`into_par_iter` over the targets), because each target is a separate DB
file and they don't contend. And every arena load is SINGLE-FLIGHTED per volume through `LOAD_GATES`: a second caller
waits for the first's arena instead of reading the same DB again, which used to cost both a duplicate multi-second load
and a transient second copy of the arena whenever a search arrived while the dialog's root pre-load was still running.
`cancel_active_loads` bumps `CANCEL_EPOCH` alongside the per-load cancel flags, so cancelling doesn't just hand the load
to the next thread queued on the gate.

Measurements: `docs/notes/search-latency-2026-07-28.md`.

### Why the index isn't keyed on a stable server identity

Keying an SMB index on something stable (so one NAS is one index regardless of the address it's reached at) was
investigated and rejected, not overlooked:

- **The id must exist before any SMB session does.** `volume_id_for_mount` derives it from `statfs`'s `f_mntfromname` at
  mount-detection time, for an OS mount with no smb2 client attached. The SMB2 server GUID Cmdr already parses
  (`smb2::NegotiatedParams::server_guid`, surfaced in the debug SMB diagnostics) exists only for a DIRECT session, so it
  can't be the primary key — at best a later reconciliation step.
- **No share-level serial is available.** The smb2 crate implements `FILE_FS_FULL_SIZE_INFORMATION` only; there's no
  `FileFsVolumeInformation` (volume serial) or object-id query, and `statfs`'s `f_fsid` / DiskArbitration's volume UUID
  aren't populated for smbfs mounts.
- **A server GUID identifies the BOX, not the share**, so it would still need pairing with the share name.
- **Re-keying orphans every existing SMB index**, and there is no rename/merge machinery for index DBs (a schema
  mismatch deletes and rebuilds — see `../indexing/CLAUDE.md` § Rebuild, don't migrate). On a real NAS that's a multi-
  hour rescan for a problem the read-time dedupe already removes the symptoms of.

If it's ever worth doing: reconcile AFTER the fact (on a direct SMB session, record the server GUID in the index's
`meta` and alias the address-keyed ids to it), rather than changing the primary derivation.

### Per-volume load (`volumes.rs`)

`ensure_volume(id)` is cache-aware. Root's pool is the live `get_read_pool()`; a NON-root volume opens a read-only
`ReadPool` straight from `index-{id}.db` on disk — deliberately NOT via `INDEX_REGISTRY`, because the DB file is the
source of truth and an ejected/unmounted drive's index is still searchable. The mount root comes from the DB's
`volume_path` meta, falling back to the LIVE volume registry when that meta is absent — historically SMB index DBs never
wrote `volume_path` (only the local scan-completion path did), so a real NAS index has none; the fallback recovers the
mount root while the volume is mounted (the only time a `/Volumes/…` scope even routes to it), and both the SMB
scan-completion path and `start_indexing_for_smb` now persist it (the latter heals an existing DB on the next
registration — no rescan). Lifecycle is dialog-scoped, not per-volume: opening the dialog pre-loads
root and arms the timers; a search lazily loads its scope's volumes; idle/backstop drops ALL arenas at once (RAM
reclaim). A long root pre-load is cancelable (`cancel_active_loads` on dialog close).

**Staleness: serve warm, refresh behind.** Only the root writer bumps the global `WRITER_GENERATION`, so only root can
fall behind its stamp; a non-root volume stamps `0` and simply reloads next dialog session (a NAS/MTP index is far less
volatile, and every arena drops on idle regardless).

A behind-the-writer root arena still ANSWERS the search, and `get_loaded` kicks a background rebuild that swaps in when
it lands (skipping the swap if the arena was dropped meanwhile — re-inserting would resurrect hundreds of MB nobody
asked for). `claim_load_slot` paces those rebuilds to one per `REFRESH_MIN_INTERVAL` (30 s) and declines while one is in
flight. Why: the arena is a SNAPSHOT by construction, and root's DB moves under it several times a second on a
live-watched boot disk (measured ~5.7 `WRITER_GENERATION` bumps/s idle, 2026-07-28 prod logs). Rebuilding on any
mismatch therefore put a full 2.6 s / 6.3 M-row pass (warm page cache; 6–18 s cold) in front of nearly every dialog open
and every auto-applied keystroke search — paying seconds to shrink staleness from seconds to milliseconds, on data the
indexer itself lags by seconds. Evidence: `docs/notes/search-latency-2026-07-28.md`.

### Mount-relative path spaces (the load-bearing gotcha)

A non-root volume's index `ROOT_ID` is its MOUNT ROOT, so it stores mount-relative paths (`/sub/file`, not
`/Volumes/naspi/sub/file`). Two mirror transforms bridge the spaces:

- **Read side** — `engine::search_ranked` takes a `path_prefix` (the mount root, empty for root) and PREPENDS it to
  every reconstructed path, so a NAS result reports `/Volumes/naspi/sub/file` and opens in a pane.
- **Scope side** — `query::resolve_include_scope` STRIPS the mount root from each include path before
  `store::resolve_path` (which walks from `ROOT_ID`), with two special cases:
  - **The mount root itself** (`/Volumes/naspi` → stripped to `/`) means the WHOLE VOLUME — routing already scoped to
    this volume, so there's no sub-restriction. `run_blocking` then leaves `include_path_ids` `None` (search everything
    in that volume). Without this, the empty strip resolved to nothing and every volume-root scope returned 0.
  - **A path not found in the volume's index** (a typo, a since-deleted folder, or one outside the mount root) is
    collected into `unresolved` and surfaces as `SearchResult::unresolved_scopes` (see Honesty below), instead of the
    engine silently matching nothing.

Without the strip, an indexed NAS folder would show bare paths that don't open, or a scope would match zero entries.

### Merge

Each volume's `search_ranked` returns entries already ranked best-first WITH their [`RankKey`](ranking.rs) (band +
importance-boosted recency + id). The keys are volume-independent scalars, so `run_blocking` concatenates the
per-volume slices and does ONE global `sort_by(RankKey::cmp_best_first)`, then truncates to the limit — a correct top-k
merge because each slice is already its volume's top-k. `total_count` sums the per-volume match totals. Directory sizes
are filled per volume (each from its own pool) BEFORE the merge, so the size post-filter runs against the right
`dir_stats`.

### Honesty: `uncovered_scopes` and `unresolved_scopes`

Two TYPED sibling fields on `SearchResult` (callers branch on emptiness, never string-match), for the two ways a scoped
search returns nothing for a STRUCTURAL reason rather than a genuine "no matches":

- **`uncovered_scopes`** — a `from_scope` target whose volume has no persisted index (`VolumeLoad::NotIndexed`). The
  dialog and MCP render "Cmdr hasn't indexed X yet". An unscoped unindexed volume is skipped silently (no user intent).
- **`unresolved_scopes`** — the volume IS indexed but the specific path isn't in it (a typo, a deleted folder, or a
  path outside the mount root). Rendered as "couldn't find that path". Distinct copy, distinct field.

Partial coverage works: covered volumes still return results alongside the note(s).

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
`media_index_search_ocr` command (`media_index/commands/search.rs`), which returns `OcrHit { path, snippet }` (the
snippet is the highlighted "why matched" reason). The frontend query-ui that blends OCR hits into the results surface is a later
slice; `search/` itself takes no dependency on `media_index` today.

## Importance ranking (`ranking.rs`)

Search ranks interesting files toward the top by blending a result's match quality with its parent folder's importance
weight (the first consumer of the `../importance/DETAILS.md` subsystem). The ranker is a pure module; `engine.rs` stays
pure by receiving importance as DATA (a prebuilt weight map), never querying a DB.

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

### Ranking cost: the top-k pass (`rank_decorated`)

Ranking runs once per MATCHED entry, and a one-letter query matches millions (4.6 M of 6.96 M on a real
home dir), so this pass — not the rayon scan — was the search's dominant cost: 11.9 s for that query,
~75% of it the importance blend. Four things keep it bounded, all order-preserving:

- **A per-thread `folder_id → weight` memo.** Matches cluster hard by folder, and a weight lookup means
  walking the folder's parent chain.
- **The folder path is HASHED, never built.** `engine::hash_path_from_index` streams the parent chain's
  components into `ranking::PathHasher` (incremental FNV-1a + the same splitmix finalizer), so the
  `String` that existed only to be hashed and dropped is gone. It's byte-identical to
  `hash_path(reconstruct_path_from_index(..))`, pinned by `streamed_hash_matches_whole_path_hash` — a
  drift there would silently read the wrong weight, with no symptom beyond subtly worse ranking.
- **`classify_match` allocates nothing** for a case-sensitive compare or an ASCII name+stem (nearly
  every real filename). Non-ASCII case-insensitive names still take the `to_lowercase()` path, so
  Unicode folding (final sigma and friends) is unchanged.
- **Top-k, not a full sort.** `select_nth_unstable_by` partitions to the caller's limit, then only that
  prefix is sorted. Safe because `RankKey::cmp_best_first` is a TOTAL order (the final tiebreak is the
  unique entry id), so the top-k set and its order are unique — an unstable partition returns exactly
  what the full stable sort did. The count-only directory pass passes `usize::MAX`: it needs every
  matching directory, since the caller subtracts the out-of-range ones from the volume total.

Measurements and the before/after table: `docs/notes/search-latency-2026-07-28.md`. The harness is
`bench.rs` (`#[ignore]`d; it can run against a real `index-*.db`).

### The weight-map lifecycle (`volumes.rs`)

Per-volume weight maps live in the `WEIGHTS` map (`volume_id → Arc<ImportanceWeights>`) in `volumes.rs`, built ONCE by
streaming [`ImportanceIndex::for_each_nonzero_weight`](../importance/read/mod.rs) and never queried per result (a search
ranks tens of thousands of candidates):

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
  `../importance/DETAILS.md` storage model), and `for_each_nonzero_weight` also filters `score > 0`, so the ~312k
  folders under `node_modules` on a 646k-folder home never enter the map (their lookup defaults to `0.0` anyway).
- **The map stores a hash of the path, not the path.** Nothing enumerates it and `weight_for` only does exact lookups,
  so `ImportanceWeights` keys on `hash_path(folder_path)` and each folder costs one 17-byte table slot: measured
  8.9 MB for 368,043 scored folders on the NAS and 4.5 MB for 158,457 on the home, down from 58 MB and 27 MB with
  path keys (2026-07-27, `heap_bytes_held` over the real `importance-*.db` files). Rationale, the collision argument,
  and why an `f32` weight would buy nothing: the `ImportanceWeights` doc comment in `ranking.rs`. `memory_tests.rs`
  guards the per-folder cost.
- **Rows stream straight into the compact map**, so the wide `path → weight` form never exists. That matters most for
  root, which reloads on EVERY recompute while the old map is still live: the reload's transient is now a second copy
  of ~4.5 MB rather than a ~27 MB intermediate on top of it.
- **A missing DB is empty, not an error.** `for_each_nonzero_weight` short-circuits to visiting nothing when the file is
  absent (a read-only open would fail `CannotOpen`), so an unscored volume degrades cleanly.

The blend coefficient is an unvalidated starting point (the importance weights themselves are too — see
`../importance/scorer/weights.rs`).
