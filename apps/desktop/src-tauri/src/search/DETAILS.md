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
- **The translation DTOs live with the code that fills them**: `TranslatedQuery` and `TranslateDisplay` are defined in
  `ai/types.rs` and `pub use`d from `commands/search.rs`, so the IPC path callers import is unchanged while
  `search::ai` no longer depends on `commands`. specta names a type by its struct identity, not its module, so
  `bindings.ts` doesn't move either. Define a new translation DTO in `ai/types.rs` and re-export it, not the reverse:
  the reverse is what made `commands::search ↔ search::ai ↔ query_builder` a cycle. The one remaining edge from
  `search/` up into `commands/` is `expand_tilde` in `ai/mappings/size_scope_mapping.rs` (see `CLAUDE.md`).
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
- **Count-only mode (`SearchQuery.count_only`) trades rows for an exact total, cheaply**: when set,
  `engine::search_ranked` computes `total_count` from the filtered matches but skips ranking, truncation, and per-row
  path materialization (the expensive parts) and returns no rows. The one wrinkle is directory size filters: directory
  sizes live in `dir_stats` (the volume's DB), not the in-memory index, so the pure engine can't size-filter
  directories. So when a size filter is set AND directories aren't excluded (`is_directory != Some(false)`),
  `search_ranked` hands the matching directories back in its ranked slice (with the total still counting every match,
  files already size-filtered), and `run_blocking` fills their sizes via `fill_dir_sizes` then calls
  `count_only_volume_total`, which subtracts the directories outside the filter. Net: an exact count in every case,
  materializing only the matching directories (never the -- usually far larger -- file set), and never building rows.
  The MCP `search` tool answers a count-only run with an empty `entries`, the number in `matchCount` and
  `matchCountHuman`, and the same typed coverage block every other run carries (`mcp/executor/search/result.rs`); the
  dialog shows it as a prominent count instead of the list (`QueryResults` count-only branch).
- **`_schemaVersion` mismatch quarantines instead of migrating in place**: there's only schema v1, so a migrator would
  be speculative. When v2 lands, replace the quarantine branch with a `match` on the version calling a
  `migrate_v1_to_v2` helper.

## The arena row (`index.rs`)

The arena holds one `SearchEntry` per file on the volume — 6,045,549 rows on David's boot disk — and it stays resident
for as long as someone is searching, so **what a row costs IS the app's peak footprint**. That makes the row's size a
design constraint, and `search/index/memory_tests.rs` pins it at 40 bytes.

**`size` and `modified_at` are `OptU64`, not `Option<u64>`.** A `u64` uses every one of its bit patterns, so `Option`
has no niche to hide `None` in and Rust adds a whole discriminant word: the two fields were 32 of the struct's 56 bytes
for two values needing 8 each. Sentinel-encoding them took the row to 40 and the loaded root arena from 689.5 MiB to
597.2 MiB, with no measurable change to scan latency (`docs/notes/search-arena-row-2026-08-06.md` has the before/after
and the A/B method).

- **`u64::MAX` is the absent marker and cannot collide.** Both values come out of SQLite `INTEGER` columns, which are
  SIGNED 64-bit, so the index can't store or return anything above `i64::MAX` — the sentinel is outside the
  representable range by construction, not merely an implausible value inside it. (It's unreachable physically too:
  16 EiB is twice APFS's own per-file ceiling.)
- **❌ Never collapse `None` into `0`.** `logical_size` is NULL on every 2nd+ name of a hardlinked inode — the index
  counts an inode's bytes once and stores NULL on the rest, 934,793 of 6.0 M rows here — so the two mean different
  things, and conflating them would change what folder totals and size filters report on a hardlink-heavy tree.
  Symmetrically, a real zero-byte file must not read back as "unknown". `index.rs`'s
  `a_hardlink_deduped_row_loads_back_as_unknown_size` pins both directions.
- **❌ Never compare against the sentinel at a call site.** `OptU64`'s inner `u64` is private and `get()` is the only
  way in, so the encoding is invisible outside the type; its `Debug` prints as the `Option` it stands for, so a log
  line never shows a bare `18446744073709551615`.
- **The arena readers are `engine.rs`** (the `Candidate` build, the `sortBy` key, `build_result_entry`) **and
  `ranking.rs`** (the recency key). `Candidate`, `CoveredEntry`, and `SearchResultEntry` keep plain `Option<u64>`:
  they're per-result or per-batch, not per-row, so the 16 bytes buy readability there instead of costing memory.

**The next lever is the names arena plus `id_to_index`**, now 366.6 MiB of the 597.2 MiB an arena costs. ⚠️ Removing
`id_to_index` is NOT a free win — it's hit once per ancestor per candidate inside an interactive loop, so it has to be
measured on the latency axis first (`docs/notes/size-only-subtrees-rejected-2026-08-06.md` § The search arena).

## Single-volume search

A search covers at most ONE volume. `execute.rs::run_blocking` owns the orchestration; `engine.rs` stays per-index and
pure.

### Routing, and the ceiling

`execute.rs::resolve_target` returns exactly one `Target` or a typed `ScopeError`:

- **Scoped** (`include_paths` non-empty): each path routes to its owning volume via
  [`volume_id_for_local_path`](crates/cmdr-index/src/indexing/paths/routing.rs) (SMB mount → `smb_volume_id`, `mtp://` →
  `{device}:{storage}`, registered external mount → its id, everything else → `root`). Every path must agree on the
  volume; two or more yields `ScopeError::SpansMultipleVolumes`, which `run_blocking` turns into the message the dialog
  toasts and MCP returns. The target is `from_scope`.
- **Unscoped**: the boot volume, whole-volume, not `from_scope`. It's the MCP default (the dialog always sends a scope);
  an agent that wants a different volume names it.

**Why one volume** (`docs/specs/unindexed-search-plan.md` Decision 4): a fan-out is the only way a search can quietly
omit a drive, or report a 2%-walked drive as covered. The ceiling has to hold at the API rather than in the dialog,
because MCP and the AI translator both build queries the UI never sees. What it costs: searching the boot disk and a NAS
in one action is no longer possible, and a search of a cold volume waits for that volume's arena instead of deferring
it.

Deleted with the fan-out (❌ don't reintroduce): the k-way merge, `ColdVolumePolicy` / `RunOutcome::deferred_volumes` /
`volumes::warm_in_background` (the "answer now, fold the NAS in on `search-index-ready`" path), and
`volumes::all_indexed_volume_ids` with its `distinct_mount_roots_in` dedupe.

### Two SMB indexes for one NAS: routing picks, nothing dedupes

An SMB volume id is `smb_volume_id(server, port, share)`, keyed on the ADDRESS the share was mounted from, so one NAS
reached over Tailscale and over the LAN gets two ids and two full index DBs (David's box: 2.6 M entries and ~525 MB
EACH, both stamping `volume_path = /Volumes/naspi`). Under the fan-out both were scanned, hits were merged twice, and
two arenas stayed resident, which is why a read-time dedupe existed.

With one volume per search there's nothing to dedupe: a `/Volumes/naspi` scope routes through the live `VolumeManager`
to the id that IS mounted there, and the other DB is never opened. The stale DB still occupies disk (the `resources/`
retention cap's problem, not search's) and wins straight back the moment it's the one mounted. Fixing the ROOT cause
(keying the index on a stable server identity instead of `host:port`) was deliberately NOT attempted: see "Why the index
isn't keyed on a stable server identity" below.

### Waiting for a cold arena

Loading a volume's arena is a multi-second, multi-hundred-MB read (2.4 s warm page cache for a 2.6 M-entry NAS index,
10.9 s observed cold in prod), and every caller now pays it: there's one target, and answering "nothing found" for the
one place someone asked about would be a lie, not a fast path. The dialog's phase states voice that wait honestly
instead of hiding it.

Every arena load is SINGLE-FLIGHTED per volume through `LOAD_GATES`: a second caller waits for the first's arena instead
of reading the same DB again, which used to cost both a duplicate multi-second load and a transient second copy of the
arena whenever a search arrived while the dialog's root pre-load was still running. `cancel_active_loads` bumps
`CANCEL_EPOCH` alongside the per-load cancel flags, so cancelling doesn't just hand the load to the next thread queued
on the gate.

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
  mismatch deletes and rebuilds — see `crates/cmdr-index/src/indexing/CLAUDE.md` § Rebuild, don't migrate). On a real NAS that's a multi-
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

### The result slice

`search_ranked` returns entries already ranked best-first (band + importance-boosted recency + id, see
[`RankKey`](ranking.rs)), so `run_blocking` never sorts: it fills directory sizes from the volume's `dir_stats`, applies
the size post-filter, and truncates the engine's dir over-fetch back to the caller's limit. `total_count` is the
engine's own match total, adjusted by that post-filter.

### Honesty: `uncovered_scopes` and `unresolved_scopes`

Two TYPED sibling fields on `SearchResult` (callers branch on emptiness, never string-match), for the two ways a scoped
search returns nothing for a STRUCTURAL reason rather than a genuine "no matches". Both the dialog
(`apps/desktop/src/lib/search/CoverageNote.svelte`) and MCP (`mcp/executor/search/result.rs`) render them, with distinct copy per
field:

- **`uncovered_scopes`** — a `from_scope` target whose volume has no persisted index (`VolumeLoad::NotIndexed`). An
  unscoped search never fills this: nobody named the boot volume, so there's no user intent to report against. The
  dialog also offers to index that drive, which is why the result names the volume it routed to.
- **`unresolved_scopes`** — the volume IS indexed but the specific path isn't in it. **The two causes are
  indistinguishable here**: a typo or deleted folder, and a real folder the user is standing in on a partially indexed
  volume, both land in this bucket. So the copy says what the index knows ("Cmdr's index doesn't cover this folder
  yet") and never that the folder doesn't exist. A LIVE search resolves the difference by acting rather than probing: a
  path the index has never seen is a frontier root, so the walk goes and reads it, and a path that genuinely isn't
  there is a root the walk declines. No filesystem probe inside routing, which would be a network-hang hazard on a
  scope pointing at a dead mount.

### Directory size filters and `sortBy`

A directory's size lives in `dir_stats`, not in the search arena, so the parallel scan can't judge it on its own. The
answer is `engine::DirSizes`: `execute.rs::dir_sizes_for` reads the passing set (`IndexStore::dir_sizes_in_range`) BEFORE
the engine runs and hands it in, so the scan filters directories on size like everything else.

**Why the order matters.** Applying it afterwards, to the ranked top-k, answers from a recency-ordered sample instead of
from the drive: `sizeMin: 50 GB` over a real machine returned four folders and missed a 1.7 TB `~/Library`, because a
folder nobody touched today loses the importance-boosted recency ranking against hundreds of thousands of freshly-touched
ones long before anything reads its size. Filtering inside the scan also makes `total_count` exact by construction, which
is why count-only no longer needs a correction pass (no over-fetch, no returned directory rows, no subtraction).

`dir_sizes_for` builds the map only for a query that filters or sorts directories by size, because it's a full scan of
`dir_stats`. That table is deliberately NOT indexed on `recursive_logical_size`: the index would be rewritten by every
rollup update on the indexing hot path, to speed up a query that runs only when someone asks about sizes. The scan is
cheap enough to keep it that way — over a 574,927-row `dir_stats`, a 50 GB floor returns 30 rows in 33 ms and the
unbounded form (what `sortBy: "size"` with no size filter asks for) returns all of them in 40 ms (verified on the
shipped `index-root.db` via read-only `sqlite3`, 2026-08-06).

❌ **A failed read must fail the search.** The engine reads a missing map as "no directory size filter to apply", so
falling back to `None` on a DB error would answer with every matching directory regardless of size — a wrong answer
wearing a right one's clothes.

`sort_by` (`SearchSort`) rides the same map. `Relevance` is the default and what `ranking.rs` owns; `Size` and `Modified`
REPLACE that ranking rather than reordering its top-k, because "the biggest matches" means the biggest that exist. A
directory is compared on its recursive size and a file on its own, so one enormous file can outrank every folder around
it. Unknown keys sort last in both directions (`sort_indices`), ties break on entry id, and the top-k is taken with
`select_nth_unstable_by` so a broad query pays a partition rather than a full sort.

**Over live-walked ground this filter can't apply**: a walked directory has no `dir_stats` row yet, which is Accepted
difference 5 (§ The accepted differences below). The index half is exact; the walked half doesn't match directories on
size.

### Honesty: `hidden_by_excludes`

The third typed honesty field, and the only one that fires on a search that worked perfectly. It counts matches an
exclusion rule kept out of `total_count`: the system/build/cache tier (`SYSTEM_DIR_EXCLUDES`, on unless the query sets
`exclude_system_dirs: Some(false)`) plus any `!` excludes in the scope.

**Why a filtered count needs to say so.** The defaults are right for "find my invoice" and exactly wrong for "where is
my disk space going", where `node_modules`, `Caches`, and `.git` ARE the answer. A caller that can't see the number
reads "27 files match" as the whole truth and states a wrong conclusion confidently; with it, MCP reports the number in
`coverage.hiddenByExcludes` and names the flag that reveals them in a note beside it
(`mcp/executor/search/result.rs`).

Counted across BOTH halves of a live run, or it would under-report the very case the walk exists for: the arena scan
counts in `engine::search_ranked` (via `ScopeVerdict::Excluded`) and rides back on `engine::Ranked`; the walk counts in
`WalkJudge::consume` via `ResultStream::note_excluded`. `ResultStream::finish` stamps the merged total onto
`SearchRunCoverage`, the one place that has seen both.

**A match outside the include roots is NOT counted.** Scope is the question, not a filter over the answer — the user
asked about somewhere else, so those aren't results they could reveal by flipping a flag. That's why
`ScopeFilter::verdict` is three-way (`Inside` / `Excluded` / `OutsideRoots`) rather than a bool.

The counter is a relaxed `AtomicU32` rather than a rayon fold, because `filter().collect()` on an indexed parallel
iterator preserves arena order and the ranking's tie-break rides on it; a fold/reduce would make equal-ranked results
non-deterministic to save an increment that only fires on an excluded match.

`target_volume_id` rides alongside: the ONE volume routing picked. The dialog acts on it rather than re-deriving a
volume from the scope path, which would fork routing (an SMB id keys on the ADDRESS; cloud drives route to `root`) and
could offer to index the wrong drive when the user typed a scope on another one.

A `VolumeLoad::Failed` volume (a DB that won't open, or a load cancelled by the dialog closing) also returns
`uncovered_result` on the index-only path, so it reads as "not indexed" rather than getting its own signal. That's
deliberate: for the user, search has no usable index for that drive either way, and re-indexing is the same fix. **The
live path splits them**, because there the difference decides what happens next — `NotIndexed` is walkable (that's the
whole point of the milestone) while `Failed` is a `SearchRunError::IndexUnreadable`, which no walk can fix.

A scope that spans two volumes is NOT one of these: it's a hard `Err` from `resolve_target`, because there's no volume
to return partial results from.

## A live search (`execute/live_run.rs::run_live_blocking`, `live.rs`)

The milestone the whole coverage concept was built for: on a folder the index doesn't cover, a search **walks it**
rather than reporting a gap. `docs/specs/unindexed-search-plan.md` is the plan; this is what landed.

Three files, split along what a run decides before it emits anything: `execute.rs` routes (`resolve_target`) and owns
the covered half both paths share; `execute/coverage.rs` holds the coverage model (`CoverageQuestion`,
`UnreadableGround`, `coverage_of`, `coverage_scopes`, `coverage_kind`, `arena_for_coverage`), which is decided against
the index alone and says nothing about reporting; `execute/live_run.rs` is the run itself (`start_live`,
`run_live_collected`, `run_live_blocking`, `groundwork`, `wait_for_the_other_walk`, `StallWatch`). `start_live`,
`run_live_collected`, and the two `AGENT_WAIT_*` budgets re-export through `execute` so callers keep one path.

### The shape

1. **Ask what's uncovered** — `Index::coverage(volume, scope, Listing)` per scope path, merged. Frontier roots plus the
   directories nothing will walk, plus a `CoverageToken` naming the state of the index the answer describes, plus which
   of those roots another walk is covering right now (`being_walked`), and how far those walks have got
   (`walk_pulse`).

   `UnreadableGround` keeps the three "nothing will walk this" lists apart exactly as the index does
   (`crates/cmdr-index`'s `UnreadableCause`, canonically `indexing/store/DETAILS.md` § "What coverage needs"):
   `permission_denied`, `declined`, and `abandoned`. ⚠️ **`abandoned` is the one that doesn't reach the wire as a
   list.** `SearchRunCoverage` carries the other two as paths and folds this one into the `abandoned_ground` boolean,
   OR-ed with what this run's own walk gave up on. That fold is what keeps a search over a wedged mount honest: the
   index remembers that ground, so the frontier never offers it, so nothing else in the answer would hint that it was
   skipped.

   What DOES cross is a COUNT, `abandoned_locations`: those paths grouped by their parent
   (`cmdr_fs::path_locations::location_count`, shared with the drive badge, which reports the same rule about a completed
   index). ❌ Never a folder count — a wedged mount marked 1,497 directories on one
   real machine, which `coverage_for_scope` already cuts to 76 shallowest ancestors, and grouping those lands on the one
   place the user would recognize. `0` alongside `abandoned_ground` is a real state and the note handles it in words:
   this run's own walk gave up on ground it recorded no path for. ❌ Still not a fourth LIST on screen — the copy that
   ground needs ("nothing for you to do, Cmdr comes back to it") is a footnote under the two lists, not a third one
   beside them.
2. **Load the arena** — after step 1, deliberately (below).
3. **The covered half** — `search_covered_half`, the identical engine pass `run_blocking` runs. The frontier is exactly
   the ground the arena has nothing to say about, so an unfiltered pass over the scope IS the covered half; nothing
   enumerates covered subtrees.
4. **Walk the rest** — `Index::cover(volume, frontier, Listing, token)`, batches judged by the same `CompiledQuery` an
   arena row gets plus the same `ExcludeRules` (`excludes.rs`), streamed out through `ResultStream`.
5. **A terminal event**, with what the run could not answer for.

Steps 1–3 are one repeatable unit (`groundwork`), because nothing has been emitted by the end of them. That is what
lets a run which would answer with NOTHING AT ALL wait instead: no rows and no count from the index, and every frontier
root already claimed by a walk in flight (`another_walk_owns_the_whole_answer`). Only one walk may have a patch of
ground (`cover/live/mod.rs`), so such a run has nothing to show and nothing it may walk; it used to finish on the spot
reading as "no files found", under a note promising the files would turn up in a moment. It now waits for that walk
(`wait_for_the_other_walk`: the COVERAGE question only, 200 ms apart — reloading the arena per poll would rebuild a
multi-second snapshot for nothing), then redoes the groundwork once and answers from what the walk wrote. That redo
reloads the arena on a token mismatch without consulting the walk mark (`AfterAnotherWalk::Yes`): a run that watched a
walk end knows rows landed, and the mark is a global one-shot somebody else may have taken.

**The wait ends early on a walk that has STOPPED, never on a clock.** A walk is bounded, but the bound scales badly: a
share that stopped answering fails one listing per 120 s `LIST_TIMEOUT`, and a share the user is browsing drops the walk
to one listing in flight (`network_scanner/scan_pace.rs`), so 32 consecutive failures serialize into roughly an hour,
times the number of frontier roots. So each poll reads `CoverageQuestion::walk_pulse` — the directory reads the walks
holding this ground have STARTED (`lifecycle/cover/live/DETAILS.md` § "The pulse of a walk") — and `StallWatch` gives up
after `OTHER_WALK_STALL` (30 s) without a single one.

Why 30 s is defensible: one read may legitimately take the full 120 s timeout, but a cover walk keeps up to 64 listings
in flight, so a healthy walk keeps STARTING reads while a slow one is outstanding. A walk that starts none for 30 s is
one whose concurrency has collapsed onto a mount that isn't answering. ⚠️ The give-up is deliberately not a new state:
the run `break`s out with no walk, which sets the same `unwalkable` flag a volume nobody can walk sets, so it answers
with what it has as `WalkEnding::Interrupted` — a lower bound, in the words the UI already has. It does NOT stop the
other walk, whose rows still land in the index for the next search. `execute/tests/stalled_walk.rs` pins all of it;
`StallWatch` takes its clock as an argument, so the 30 s is tested at full size.

❌ Not when the index answered with something: those rows are worth showing now, and holding them back for somebody
else's frontier would break Decision 11's promise that a refined query keeps what its predecessor covered. That run
reports `still_covering`, which is true for it.

The partition covers the frontier ROOT itself, not only what's inside it. A walk reports a directory's contents, so a
frontier root the index had no row for would be the one entry neither half emits — and a scope root matches its own
query as readily as anything under it (the arena's include filter passes an entry that IS an include root). So
`lifecycle/cover/` emits a root it had to materialize, once, before its listing. A root the index already held belongs to the
covered half instead, which is why it isn't emitted twice.

### Decision 12: the arena and the coverage answer have to be in step

A coverage answer that calls a subtree covered is a **promise the arena holds its rows**. A walk that wrote rows behind
the arena breaks that promise, and the break is silent: the same query, run again, prunes the ground it just walked and
returns FEWER results than the first time. `execute/tests/live_e2e.rs` pins it (and fails with an empty list if
`arena_for_coverage` is reduced to a plain `ensure_volume`).

Two mechanisms, both load-bearing:

- **The order.** Coverage is asked BEFORE the arena is loaded, so an arena whose load STARTED after the answer was taken
  holds every row it calls covered, whatever else landed meanwhile. That is the actual invariant, and it is causal: the
  answer read rows committed before it returned, and the load reads rows after that.
- **The walk mark plus the freshness test.** `volumes::mark_walked_behind` is set when a walk starts and again on every
  batch (`live::drive_walk`), so a walk still running re-marks whatever a query consumed. The rebuild runs only when the
  mark is set AND the arena can't honor the answer. Without the freshness test, every query after any walk pays a full
  rebuild; without the mark, a boot disk — whose background indexer moves the token several times a second — would
  rebuild in front of nearly every search, the regression `volumes::get_loaded` documents removing once already. What's
  left uncovered is ordinary index lag, which search has always had.

**Two ways an arena honors an answer** (`LoadedVolume::honors`), and the second exists because the first can't see a
cold load:

- Its **token** is the answer's, so the two describe the same rows outright. This is the only thing that can be said for
  a WARM arena, which was built before the question was asked.
- Its **load started after the answer was taken**, so it holds a superset of what the answer calls covered. `Instant`,
  not the token: `CoverageToken` is a watermark, comparable for equality only (`cmdr-index`'s `read/DETAILS.md`
  § "The freshness token"), so it can say "something changed" but never "this one is newer". Both stamps are read
  BEFORE the rows they describe, so each can only under-claim, and under-claiming costs a rebuild rather than serving an
  answer the arena can't back.

Why the second one matters: a token moves on any write, including the ones that land during the seconds an arena takes
to build. On a drive being indexed for the first time that is constant, so a COLD load — one already reading the
database after the answer, and honorable on arrival — read as "out of step" and was thrown away for a second, identical
build. Every first search of a session paid for two arenas (measured 2.0 s + 2.1 s over a 6 M-entry root index,
2026-08-15). `live_e2e.rs::a_cold_arena_is_built_once_even_though_the_index_moved_while_it_loaded` pins the count;
`a_warm_arena_a_walk_wrote_behind_is_rebuilt_before_it_answers` pins that the protection survived, and step 4 of
`a_drive_with_no_index_is_walked_live_then_read_back_from_what_the_walk_wrote` still fails with an empty list if the
rebuild goes.

**Known narrow hole, pre-existing:** `ensure_volume` single-flights per volume, so a caller can be handed an arena
another thread was ALREADY building when the answer was taken. The freshness test catches that and rebuilds — but the
rebuild itself can be donated the same way, and its result is served unchecked. It takes a search landing inside the
dialog's own pre-load window on a volume a walk is writing to. Closing it needs a load primitive that can promise "built
by this call", which is a bigger change than it's worth so far.

### Decision 11: superseding is not cancelling

Refining a query registers a new run and marks the old one superseded. The old run stops emitting; its walk keeps
running, and its driver keeps DRAINING — the walk's channel is bounded, so a run that stopped reading would park the
walk it isn't allowed to stop, and the arena mark has to keep pace with rows it's still writing. The ground it already
covered comes back to the next query from the index, not from a replay buffer.

Ground a live walk already holds is reported as `still_covering` rather than walked twice (one walk per patch of
ground, `lifecycle/cover/live/mod.rs`); those rows reach the same index, so it means "these arrive a bit later" — for a run
that has other ground to show. A run whose WHOLE scope is somebody else's waits for it instead of saying that (step 1
above): with nothing to show and nothing to walk, "a bit later" would have meant "not in this run".

Superseding is scoped by `RunOrigin`, because it only makes sense for an asker that RETYPES. The dialog is one such
asker; an MCP call is not. So a `Dialog` run supersedes the dialog's previous run and nothing else, and closing the
dialog (`cancel_dialog_runs_except`) stops only the dialog's. Without the split, an agent's search would have emptied a
person's mid-type, and a person closing the dialog would have cancelled an agent's walk out from under its caller.
Only `cancel_all_live_runs` (app quit) reaches every origin.

The one-shot fold, the terminal events, and what a live row can't carry: `live/DETAILS.md`.

### Why the walk handle never leaves its thread

`CoverWalk` owns a `Receiver`, so it's `!Sync`. `drive_walk` gives it to a thread whose only job is blocking on it and
forwarding, while the run's own loop waits on a channel with a deadline — which is what lets it flush on the interval
and notice a cancel without polling. Cancelling goes through the `CancellationToken` the run handed `Index::cover`.
### Which ground the answer came from (`CoverageKind`)

Beside the ending, the terminal answer says whether the run needed a walk at all: `Covered` (empty frontier), `Live`
(every scope root was ITSELF a frontier root, so nothing was covered) or `Mixed`. Derived by the pure `coverage_kind`
from the coverage question alone, so it describes the QUESTION rather than how far the run got — a cancelled run over
half-covered ground is still `Mixed`, and `WalkEnding` says the rest.

It exists to be counted: "how often does a search still have to walk" is the measure of this whole effort, and the
frontend ships it as the `coverage` prop on `search_used` (`analytics/DETAILS.md` § "The search events, in detail").
Nothing branches on it, which is why it's a field on the coverage report rather than a second signal.
### There is no way to turn live walking off

Search is a deliberate action and a walk is what it means, so ❌ don't add a setting, a per-drive opt-out, or a "search
index only" mode. A half-answer behind a preference is the confident-looking wrong answer this whole path exists to
remove. Neither indexing switch is that setting either: both govern BACKGROUND work only, so a search walks a drive
whose indexing the user turned off (`lifecycle/cover/DETAILS.md`, `src/lib/settings/sections/DETAILS.md`).

### The accepted differences: where indexed and live still diverge

The governing principle: **a drive being indexed or not must not produce a behavioral difference, only a speed one.**
This register is where the code does not reach that, kept complete so the gaps stay visible rather than buried, and
numbered stably because tests, module docs, and code comments across both crates cite an item by its number. Anything
found later belongs here. Each item names its canonical doc rather than restating the mechanism.

1. **An interrupted walk is narrower.** Cancel, drive disconnect, and app quit each end a walk early. The one people
   meet most, so the result list says it is a lower bound: `live/DETAILS.md` § Terminal states.
2. **Unreadable subtrees are narrower.** A refusal, a standing policy over a snapshot tree, or ground the walker gave
   up on: `crates/cmdr-index/src/indexing/read/DETAILS.md` § The descent rule.
3. **Auto-apply works on indexed drives and not on uncovered ground.** Crossing into a frontier needs Enter, because
   six keystrokes would otherwise start and abandon five multi-minute walks: `src/lib/search/DETAILS.md` § The live
   search.
4. **Ranking is not preserved.** Importance weights come from the index, so live rows rank by match quality and recency
   alone; results are capped, so at the boundary a different order is a different visible set, and the completion
   re-rank reorders what survived without recovering what the cap dropped: `src/lib/search/DETAILS.md`
   (`rankLiveResults`).
5. **Directory size filters behave differently.** A walked directory has no `dir_stats` row yet, so a "folders over
   100 MB" filter returns a different set than the indexed run: § Directory size filters and `sortBy` above.
6. **A covered-but-stale subtree is trusted, not re-walked.** A volume disconnected while its watcher was down can
   return a deleted file until `reconcile/` catches up. It applies equally to indexed and walk-covered volumes, since
   both are watched, so it is a property of the index rather than a gap between the two:
   `crates/cmdr-index/src/indexing/read/DETAILS.md` § The descent rule, and
   `crates/cmdr-index/src/indexing/watch/DETAILS.md` for why walk-written coverage carries no expiry.
7. **The walk indexes what the user will never see in results.** `excludeSystemDirs` is a MATCH-time filter, so a live
   search of `~/projects` walks and writes every `node_modules` and `.git` under it. That is the multiplier on "a
   search of an unindexed drive can take minutes", and it is deliberate:
   `crates/cmdr-index/src/indexing/scanner/DETAILS.md`.
8. **Media, OCR, and semantic search stay empty.** The walk writes the drive index only, never `media_index`, so photo
   and OCR search over walked-but-unindexed ground returns nothing. The existing `search.imageResults.notIndexed` copy
   is what signals it.
9. **A walk that ran to completion can still be short.** Ground the walker abandoned rides alongside the ending rather
   than inside it: `live/DETAILS.md` § Terminal states.
10. **A size filter treats hardlinks differently live than indexed.** A walk emits each entry's OWN size, before
    hardlink dedup, because that is what a listing shows; the index stores the deduplicated size, `NULL` for the 2nd+
    link. So "files over 1 MB" keeps a hardlinked duplicate in a live result and drops it from an indexed one. Bounded
    (multiply-linked files under a size bound only) and the live answer is the truthful one, so ❌ don't "fix" it by
    teaching the walk to dedupe.
11. **The master-switch settings note is deliberately inaccurate** once a search has written coverage:
    `src/lib/settings/sections/DETAILS.md`.
12. **A live count-only search can count a file twice.** The row path dedupes a walked entry against the rows already
    emitted, bounded by the result cap; a count has no such bound, so a file that is BOTH in the arena and inside a
    frontier subtree is counted by each half. It takes rows under an unlisted directory to happen at all (a
    verification pass, or an interrupted walk), and the row path is unaffected.
13. **A non-virgin frontier root's newly found rows arrive one search late.** The local repair path
    (`lifecycle/cover/mod.rs::repair_non_virgin`) writes through the serial reconcile, which takes no live consumer, so
    rows it ADDS appear on the next query rather than this one. Rare (it takes an FSEvents verification pass writing
    children under a directory nothing listed), and the arena mark is what makes "the next query" true.
14. **Ground another walk holds answers narrower, and says so.** One walk per patch of ground, so a run with index rows
    of its own answers with the covered half and reports `still_covering`; those rows reach the index and the next
    search picks them up: § Decision 11 above.
15. **A volume mid-full-scan is not walked at all.** The scan owns the writer and is covering that ground anyway, so
    the search answers from what the index already holds and reports that it is waiting on another walk:
    `lifecycle/cover/DETAILS.md`.
16. **A broad query answers on a fully indexed scope and fails the whole RUN on one with any frontier.** The arena
    evaluator allows a query that narrows nothing below 100k rows; the live evaluator refuses outright, and refusing
    takes the run with it, deliberately (answering from the index alone over uncovered ground is the
    confident-looking half-answer this path exists to remove). The starkest item in this register, and the one a user
    is most likely to read as a bug: § The compiled query below.

## The compiled query (`matcher.rs`)

The per-entry predicates (name pattern, type, size, date) are a `CompiledQuery`, not part of the arena scan. The reason
is the second evaluator: a search over ground the index doesn't cover walks it live and matches the entries the walk
emits (`docs/specs/unindexed-search-plan.md` Decision 3). If those entries were judged by a second copy of the
rules, the same query would answer differently depending on whether the drive happened to be indexed, which is the one
thing that plan forbids. So both paths call `CompiledQuery::matches`, and the module owns the rules that break
silently when duplicated: the case-folding resolution (the scope filter and the ranker read it back rather than
deriving it again) and the NFD normalization of the pattern.

It sits app-side rather than in `cmdr-index` because `index-crate-isolation` forbids the crate from depending on the
app. That constraint is also why `Index::cover` hands back `CoveredEntry` batches instead of taking a match callback.

**What stays outside, and why:**

- **Directory size filters.** A directory's size isn't in the entries table; it's written over the ranked results from
  `dir_stats` afterwards (`execute.rs::fill_dir_sizes`, then `filter_dirs_by_size`). So the matcher's size predicate is
  files-only on both paths, and a directory passes a size filter untouched. Dropping directories in the matcher would
  drop them before the only place that knows their size.
- **The scope filter.** Include roots are arena entry ids and the exclusion check is an ancestor walk through
  `id_to_index`, so neither means anything for an entry that isn't in the arena. The live path applies the same policy
  against a walked entry's own path instead.

**A glob's `.` crosses a newline; a user's regex keeps standard semantics.** Filenames may contain newlines, and a
glob's `*` and `?` mean "any characters" and "one character", so `glob_to_regex` prefixes its output with `(?s)`. It
lives in the translation rather than at each `RegexBuilder` — a property of what a glob MEANS, not of one caller — so a
new call site can't get it wrong; two of them disagreeing is how newline-named files went unfindable in the first place.
It composes with `RegexBuilder::case_insensitive` (an inline `(?s)` sets that one flag and leaves the builder's alone),
pinned by `matcher::tests::a_case_insensitive_accented_glob_still_crosses_a_newline`.

A user-typed regex never goes through `glob_to_regex` and deliberately keeps the standard rule: someone writing one
expects `.` to stop at a newline and `(?s)` to be their own call. ❌ Don't "unify" the two — the asymmetry is the
decision, and the line it's drawn on is **authorship, not notation**. Which is what settles the two neighbours:

- **The AI mappings emit `PatternType::Regex` matched against FILENAMES, and carry `(?is)` inline.** A mapping table
  isn't an author, so the reasoning that protects a person's own regex doesn't reach them; they take the flags a
  filename matcher needs. `keyword_mapping::merge_keyword_and_type` is the single place that prefixes them, so
  ❌ `type_mapping`'s `TypeFilter.pattern` entries carry no inline flags of their own.
- **The file viewer takes a third position: it REFUSES.** `file_viewer/search_matcher.rs` rejects a pattern that would
  need to cross a newline with a typed `MultilineNotSupported`, because its streaming model is line-at-a-time. Different
  problem (file CONTENTS, not names), already explicit in its own code.

**The broad-query guard is per evaluator.** An arena's cost is known before the scan, so a query with no narrowing
predicate is refused only above `ARENA_BROAD_QUERY_CEILING` rows; below it, "show me everything, by recency" is a fair
ask. A live walk has no such bound (an unknown filesystem, over a network in the worst case), so it refuses outright.
❌ Don't collapse the two back into one row-count rule: an unindexed volume's arena holds zero rows, so a count-based
ceiling is precisely the guard that can't fire on the path that needs it.

**Two asymmetries survive the shared matcher**, both bounded and both deliberate:

- A `CoveredEntry` carries the entry's OWN size, before hardlink dedup, because that's what a listing shows. The index
  stores the deduplicated size, so a 2nd+ hardlink to one file is sizeless there. A size bound therefore keeps it in a
  live result and drops it from an indexed one. The live answer is the truthful one, so it stays.
- A walked entry's name is derived from its path (`covered_name`), byte-identically to how `insert_visitor` derives the
  row name it writes. For the trait walk (`network_scanner/cover_scan.rs`) the row name is the listing's own `name`
  field while the path is its `path`, so the two agree only as long as a `Volume` backend reports a path whose last
  component IS that name. ⚠️ A backend that broke that would make live and indexed results disagree on the affected
  names with nothing failing loudly.

## History store (`history.rs`)

The list itself (persistence to `{app_data_dir}/search-history.json`, dedupe, cap, quarantine, locking) is
`crate::recents`; see `apps/desktop/src-tauri/src/recents/DETAILS.md`. What lives here:

- **`RECENT_SEARCHES`**, the `RecentsFile<HistoryEntry>` static, plus the entry shape and its `RecentEntry` impl.
- **Canonical dedupe key** (compare-time only, never persisted): `mode | normalized_query | filters | scope |
  case_sensitive | exclude_system_dirs`. Same key = same search; the most recent copy wins (move-to-top). Six segments
  against Selection's four, pinned by `key_carries_scope_and_exclude_system_dirs` in `history.rs`.
- **Cap**: `search.recentSearches.maxCount` (`DEFAULT_MAX_COUNT` = 1000), resolved per call in `commands/search.rs`.
  `apply_max_count` trims in-memory on live-apply; `0` clears and short-circuits future adds.
- **Shared types**: `HistoryMode`, `HistoryFilters`, and the key-building helpers (`normalize_query`,
  `filters_fingerprint`, `flag`) live here and `selection/` uses them one-way; the entry structs stay separate. Sharing
  the helpers is what keeps a newly-added filter field from reaching one key and not the other. If the two mode sets ever
  fork, copy the types rather than widening the re-export.

## AI backend resolution

`ai/` builds the prompt and parses the reply, but it never picks a provider. The `translate_search_query` command
(`commands/search.rs`) resolves one through `crate::ai::manager::resolve_translate_backend`, then runs
`crate::ai::translate::translate_once`. Backend choice, keys, and settings therefore stay in `crate::ai`, and `search/`
takes no dependency on them.

## Image-OCR search boundary (`media_index`)

"Text in images" search is a SEPARATE query path from filename search, and it reaches a volume's `media.db` ONLY through
the [`MediaIndex`](crates/cmdr-index/src/media_index/read/mod.rs) read API — never a raw `rusqlite` dep on `media.db` (plan Decision 8), so
that store's `platform_case`/one-writer invariants don't leak into a second subsystem. The door is the
`media_index_search_ocr` command (`commands/media_index/search.rs`), which returns `OcrHit { path, snippet }` (the
snippet is the highlighted "why matched" reason). The frontend query-ui that blends OCR hits into the results surface is a later
slice; `search/` itself takes no dependency on `media_index` today.

## Importance ranking (`ranking.rs`)

Search ranks interesting files toward the top by blending a result's match quality with its parent folder's importance
weight (the first consumer of the `crates/cmdr-index/src/importance/DETAILS.md` subsystem). The ranker is a pure module; `engine.rs` stays
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
`bench.rs` (`#[ignore]`d; it can run against a real `index-*.db`). Two of its benches answer the arena-SHAPE
questions rather than the ranking ones: `bench_arena_bytes` (what a loaded arena costs, in heap bytes and in process
RSS) and `bench_arena_scan` (the count-only pass, best-of-N, with and without the size/date filters that read an
entry's `OptU64` fields). ❌ Don't reach for `bench_real_index` to answer "did the row's shape make scanning slower":
its queries set no size or date bound, so the matcher skips those predicates, and one run per pattern is swamped by
machine noise.

### The weight-map lifecycle (`volumes/weights.rs`)

Per-volume weight maps live in the `WEIGHTS` map (`volume_id → Arc<ImportanceWeights>`), built ONCE by
streaming [`ImportanceIndex::for_each_nonzero_weight`](crates/cmdr-index/src/importance/read/mod.rs) and never queried per result (a search
ranks tens of thousands of candidates):

- **Loaded with the arena, cloned per search.** `ensure_volume` loads a volume's weights alongside its arena;
  `run_blocking` clones the cheap `Arc` per target and ranks against a stable snapshot even if a reload swaps the map
  mid-search. Kept SEPARATE from `LoadedVolume` so the root recompute subscriber can swap root's map without rebuilding
  the arena.
- **Subscribe (root), snapshot (non-root).** `start_importance_weight_subscriber` (wired from `lib.rs` setup, which
  also records the app data dir) subscribes to root's
  [`read::subscribe`](crates/cmdr-index/src/importance/read/mod.rs) recompute channel and keeps root's map current on
  each pass, plus one full load up front. A non-root volume takes a load-time snapshot instead: it drops on idle and
  reloads next session, and its importance rarely recomputes mid-session. A volume with no `importance-{id}.db`
  degrades to match-quality + recency (empty map).
- **A full pass reloads; an incremental PATCHES.** `weight_refresh_for` maps each notice to one of three actions, and
  the rule that matters is that a lagged receiver reloads — see the contract in
  `crates/cmdr-index/src/importance/read/DETAILS.md` § The reload contract, which is canonical for what the notices
  mean. `apply_weight_delta` applies removals then upserts through `Arc::make_mut`, so the map mutates IN PLACE when
  nobody is searching and clones only when a search holds the `Arc` — the lock-free reader below is untouched either
  way, and no reader can see a half-applied delta. Holding the `WEIGHTS` mutex across the whole patch is what makes
  that sound (`weights_for` takes the same mutex to hand the `Arc` out).
  Measured 2026-08-03 over the real 160,302-folder `importance-root.db` (`bench::bench_weight_reload`, release, warm
  cache): a full reload is 72–74 ms; patching a typical 8-upsert / 1-removal delta is 333 ns unshared, 72 µs while a
  search holds the map. That every-60s reload was what pinned the incremental throttle window
  (`crates/cmdr-index/src/importance/scheduler/DETAILS.md` § Throttle).
- **Only non-zero weights enter the map.** Floored folders have NO row in `importance.db` (the store's compaction — see
  `crates/cmdr-index/src/importance/DETAILS.md` storage model), and `for_each_nonzero_weight` also filters `score > 0`, so the ~312k
  folders under `node_modules` on a 646k-folder home never enter the map (their lookup defaults to `0.0` anyway).
- **The map stores a hash of the path, not the path.** Nothing enumerates it and `weight_for` only does exact lookups,
  so `ImportanceWeights` keys on `hash_path(folder_path)` and each folder costs one 17-byte table slot: measured
  8.9 MB for 368,043 scored folders on the NAS and 4.5 MB for 158,457 on the home, down from 58 MB and 27 MB with
  path keys (2026-07-27, `heap_bytes_held` over the real `importance-*.db` files). Rationale, the collision argument,
  and why an `f32` weight would buy nothing: the `ImportanceWeights` doc comment in `ranking.rs`. `memory_tests.rs`
  guards the per-folder cost.
- **Rows stream straight into the compact map**, so the wide `path → weight` form never exists. That matters for the
  full reload, which builds the new map while the old one is still live: its transient is a second copy of ~4.5 MB
  rather than a ~27 MB intermediate on top of it.
- **A missing DB is empty, not an error.** `for_each_nonzero_weight` short-circuits to visiting nothing when the file is
  absent (a read-only open would fail `CannotOpen`), so an unscored volume degrades cleanly.

The blend coefficient is an unvalidated starting point (the importance weights themselves are too — see
`crates/cmdr-index/src/importance/scorer/weights.rs`).
