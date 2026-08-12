# Media index subsystem — details

Image-ML enrichment: makes a volume's images searchable by their content. Full design and milestone plan:
`docs/specs/later/media-ml-index-plan.md`. This doc covers what's SUBSYSTEM-WIDE (the port rationale, the GC safety
argument, the coverage/scope model) plus the top-level files no area subdir owns; each area documents itself.

Read this before any non-trivial work here: editing, planning, reorganizing, or advising.

**Where the area depth lives:** `scheduler/DETAILS.md` (passes, the pool, importance ordering, live enrichment,
reclaim), `network/DETAILS.md` (byte-fetch, the conservative policy, the network UI), `backend/DETAILS.md` (the Vision
seam and the real macOS impl), `clip/DETAILS.md` (CLIP semantic search), `ann/DETAILS.md` (the HNSW index),
`store/DETAILS.md` (schema), `read/DETAILS.md` (the consumer API), `vector/DETAILS.md` (cosine + the resident caches).

## Why a port of `importance/`, not a re-derivation

`importance/` already solved this plan's hardest plumbing (verified against the shipped code): a per-volume disposable
store carrying the index's cache discipline, a scheduler driven by the neutral lifecycle bus plus a startup registry
sweep plus a coalescing coordinator, and an offline-capable consumer read API. `media_index` copies those patterns
file-for-file in spirit (`store/`, `writer/`, `writer_registry.rs`, `scheduler/`'s `PassCoordinator`, `read/`), so a
maintainer who knows `importance/` already knows this. Read `importance/DETAILS.md` for the shared rationale (one writer
thread per DB, `platform_case` on every connection, delete-and-recreate on a schema bump, path-keyed rows, subscribe →
sweep → wire ordering, edge-triggered bus consumption).

### Two deliberate divergences from `importance/`

- **No per-row scan-generation column.** `importance` stamps every row with its as-of `recompute_generation` (its OWN
  persisted meta counter) because a full pass replaces the whole table. `media_index` doesn't rewrite the table each
  scan; its staleness is `(path, mtime, size)` + the OS/Vision engine stamp, which makes a generation column redundant.
  Crucially, this is NOT the lifecycle-bus `generation` — that one is a transient in-memory wake counter that resets to
  1 every launch and must NEVER be persisted (plan Decision 3). If a durable "as-of" marker is ever needed, mint a
  separate persisted counter à la `importance::next_generation`; never stamp the bus value.
- **A real GC instead of wholesale table replacement.** Media enrichment is expensive and incremental, so a pass
  enriches only stale images and GCs vanished rows, rather than clearing + rewriting the whole table.

## The GC safety argument (data-safety)

GC deletes a stored `media.db` row when its source path no longer appears as a qualifying image in the CURRENT index
walk. The safety comes entirely from **when** a pass (and thus its GC) runs, not from generation arithmetic:

- A pass runs ONLY on a `Completed` bus edge or the Fresh registry sweep. The `Completed` signal fires AFTER the index
  writer flushes the truncate + repopulate (`indexing/lifecycle/scan_completion.rs`), so a triggered sweep always
  observes a COMPLETE tree — never the mid-scan truncate window where every path transiently vanishes.
- The edge is consumed via `borrow_and_update` / `has_changed`, NEVER a `borrow()` poll. The `watch` retains the last
  `Completed` across a new scan's truncate window; a poll could observe that stale `Completed` mid-truncate and GC live
  rows. Edge-triggered consumption fires exactly once per real completion, so a truncate window can't re-trigger a
  sweep.
- The startup sweep is safe for the same reason: `ready_volumes_with_kind()` filters to `Fresh` only (excludes
  `Scanning`/`Stale`), so a mid-scan volume is never swept at launch.
- A cancelled pass (memory watchdog) skips GC entirely, yielding fully; vanished rows are collected on the next
  completed scan.

Three deletion paths bypass this completed-scan edge, each for a reason the edge doesn't cover: the privacy retro-delete
and the reclaim prune (both USER-EXPLICIT, settings-derived — § Per-folder photo-search exclude, `scheduler/DETAILS.md`
§ Reclaim space), and the live-tick scoped GC (INDEX-CONFIRMED, scoped to the touched dirs — `scheduler/DETAILS.md` §
Live enrichment). None may run the whole-store `gc_targets` outside a completed pass.

## The image-qualification predicate (`predicate.rs`)

Pure over a directory's file names (sibling-aware): images enrich (JPEG/PNG/HEIC/…); videos skip (out of scope, also a
Live Photo's motion `.mov`); an image with a same-stem `.mov` is tagged `LivePhotoStill`; `.aae` edit sidecars skip; a
RAW beside a same-stem JPEG defers to the JPEG (cheaper decode), a lone RAW enriches. Classification is typed
(`Qualification`/`MediaKind`/`SkipReason`), never a substring branch. The scheduler groups the index walk by parent
directory and runs `qualify_dir` per group.

## Settings + memory (`gate.rs`, wiring)

The master toggle `mediaIndex.enabled` (off by default, sparse-persisted) seeds `gate` at startup and live-applies via
`set_image_index_enabled`. The scheduler no-ops when off. Cancellation hooks into the EXISTING indexing memory watchdog
through `indexing::register_subsystem_stop_hook` (a new tiny hook registry `stop_all_indexing` runs), so `media_index`
yields to the SAME 16 GB resident-memory ceiling rather than a second independent one that would let two ceilings sum to
~2× over one pool. The gate's emergency-stop atomic is checked between images; enabling the feature clears it.

`gate` also holds the scope (`AtomicU8`, § The indexing scope), the parallelism count, and the semantic-search toggle
(`clip/DETAILS.md` § The semantic-search on/off gate). The importance threshold is an `f64`-bits atomic
(`set_importance_threshold` / `importance_threshold`, clamped `0.0..=1.0`), seeded from `mediaIndex.importanceThreshold`
and live-applied by `media_index_set_importance_threshold`. Default `0.0` (`DEFAULT_IMPORTANCE_THRESHOLD`): enrich every
scored folder, and the slider raises it to defer low-importance folders. Only a DECREASE kicks an immediate pass
(`threshold_decreased`): the newly-covered folders start enriching now, while a raise merely defers future work, so
kicking on a raise would re-walk the index for nothing.

### Disabling stops the running pass (not just future ones)

Every pass's between-images cancel hook is `gate::should_stop`, the ONE predicate all three pass types check (the local
full pass at `run_pass_blocking`, the SMB network pass at `run_network_pass_blocking`, and the live tick at
`run_live_tick_blocking`). It's true on EITHER of two independent reasons: the watchdog's `is_cancelled` emergency stop,
OR the master toggle being OFF (`!is_enabled()`). So turning "Index image contents" off halts an in-flight pass (e.g. a
NAS pass at image 74 of 31,890) within a few images, reusing the SAME safe cancel exit the watchdog uses — the loop
breaks with `cancelled: true`, which SKIPS GC and keeps every already-enriched row. Disabling is "stop processing",
never "erase": no GC, no prune (the privacy retro-delete is the separate, explicit erase path).

**Decision: fold the disable into the cancel predicate, don't overload the stop token.** The two stop reasons stay
SEPARATE — disabling touches the token not at all, it's observed live off `is_enabled()`, so `is_cancelled` /
`request_cancel` keep their exact watchdog-only meaning. This also means re-enable can never leave a stuck signal:
`set_enabled(true)` installs a FRESH `CancellationToken` (a token is one-shot, so it's a swap, not an un-cancel — and
that's also what stops a pass the user told to stop from quietly resuming) and makes `is_enabled()` true, so
`should_stop()` is false again, and `kick_all_ready_passes` starts fresh passes. A distinct third atomic for disable
would add state to reset for no gain. The between-images granularity is right for a NON-destructive stop: unlike the
exclusion veto (privacy, which re-checks before each upsert to close the in-flight-analyze TOCTOU), one more image
finishing after a disable just writes one more KEPT row, so the per-image loop check is enough.

### The indexing scope: chosen folders vs automatic (`gate::IndexScope`)

WHICH folders indexing may cover is an explicit user choice, not something inferred from a number:

- **`ChosenFolders`** (the default): coverage is the "always index" overrides and nothing else. Importance is never
  READ, so the threshold has no effect at any position.
- **`ByImportance`**: the overrides PLUS every folder at or above the threshold — the automatic behavior, and the only
  scope where the slider is shown.

**Decision: an explicit enum, not a sentinel threshold.** A "threshold 1.01 means chosen-only" encoding would put the
model back where this feature came from — inferred, undocumented, and impossible to read off the settings file. The
scope lives in `gate` as an `AtomicU8` beside the threshold, seeded from `mediaIndex.scope` and live-applied by
`media_index_set_scope`.

**Decision: the narrow scope REUSES the override-only path, it doesn't add a second gate.** `local_should_enrich`
already treats `scores: None` as override-only (the unscored-volume fallback, `scheduler/DETAILS.md` §
Defer-until-scored), which is exactly what "only folders I choose" means. `lifecycle::pass_coverage(scope, load_scores)`
is the one place that resolves it: in the narrow scope it returns `scores: None` WITHOUT calling `load_scores`, and —
the part that matters — WITHOUT marking the volume deferred-on-importance. Marking it would have the unscored → scored
bridge re-kick a pass that has nothing new to enrich. `coverage::stored_row_survives` takes the same scope so the
reclaim partition can never propose deleting a row a pass would keep, and `volume_state`'s `waiting_for_importance` is
false in the narrow scope (there's no wait to voice).

**Decision: narrowing the scope deletes nothing.** Switching to `ChosenFolders` re-partitions the stored rows — the
importance-covered ones become "doomed" — but nothing is written. Those rows stay searchable and surface through the
EXISTING kept-rows line and reclaim offer, the same forward-only contract the slider has. There is no new deletion path:
reclaim's user-explicit prune is still the only way rows leave. `stored_coverage*` additionally partitions without
importance in the narrow scope (an empty score map rather than `None`), so the reclaim offer works on a volume
importance never scored — exactly the volume someone narrowing their scope is likely looking at.

**Decision: adding a chosen folder kicks a pass.** `media_index_set_always_index_folder` kicks every ready volume when a
folder is ADDED and the feature is on (the path alone doesn't say which volume it's on; a pass on an unrelated volume is
a fast staleness no-op and the coordinator coalesces). Removing kicks nothing. This mirrors the SMB opt-in and the
threshold-decrease kicks: without it a chosen folder would sit unindexed until the next scan completion, which on a
quiet local drive can be hours, and the feature would look inert at the exact moment the user acts.

**Migration.** `gate::scope_from_settings(scope_token, was_enabled)`: a stated scope wins; with none, an install that
already had image indexing ON resolves to `ByImportance` (someone running it today is running the automatic behavior,
and narrowing their indexed set at launch is a change they never asked for), everyone else to the default. Image
indexing is off by default, so that second group is nearly everyone. The frontend `migrateSettings` (schema 3) writes
the key once with the same rule; the Rust fallback covers the launch before that migration runs, so the two can't
disagree.

### The importance "has scored" detection

`media_index` decides "has importance scored this volume?" via `ImportanceIndex::is_scored` — used by BOTH
`MediaScheduler::folder_scores` and `coverage::importance_scores`. It returns `true` when a full pass stamped a
`recompute_generation` OR any weight row exists (`ImportanceIndex::scored_folder_count() > 0`, a cheap `COUNT(*)`). The
generation-only check that predated this reported "never scored" for two real stores that carry perfectly usable
weights: a store maintained only by INCREMENTAL rescores (the incremental path never bumps the generation), and a
schema-recreated store between its recreate and its first full pass. Both then showed "0 covered" at every threshold.
The matching importance-side fix (a fresh/recreated store actually GETS a full pass at startup) lives in
`importance/DETAILS.md` § The initial full pass; fixing both means media's read-side check is defense in depth, not the
only guard.

## Covered-count preview + honest progress (`coverage/`, `apps/desktop/src-tauri/src/commands/media_index/state.rs`)

`coverage/` is five files: `mod.rs` (the coverage RULE — `covered_in_scope`, `partition_stored`, `stored_row_survives` —
plus `folder_coverage`, the one read that joins both halves), `scores.rs` (the importance score cache, § The importance
score cache), `eligible.rs` (the denominator cache, described here), `accounted.rs` (the numerator cache, § The
per-folder accounted aggregate), and `rollup.rs` (the subtree arithmetic both caches share). Everything a host may name
is re-exported from `mod.rs`; `eligible`, `rollup`, and `scores`' internals are private, and `accounted` is visible only
inside `media_index` because the writer thread mutates it directly.

### The importance score cache

`ImportanceIndex::above_threshold(0.0)` is an ordered read of EVERY scored folder, which SQLite runs as an external
merge sort (a measured 368,043 scored folders on one root). That's fine once per enrichment pass and ruinous per UI
query, and the per-file badge asks per visible range, per pane, on every listing swap and enrichment tick. Uncached,
those queries piled up on the tokio blocking pool until it hit its 512-thread cap, at which point every other
`spawn_blocking` in the app starved: directory listings never completed and the volume list timed out into an empty
picker.

`coverage::importance_scores(data_dir, volume_id, at_least)` therefore serves a per-volume cached `Arc<HashMap>`.
`at_least: None` is every scored folder, so a slider drag gets one read serving every position; `Some(threshold)`
memoizes the projection the enrichment gate checks MEMBERSHIP against (`local_should_enrich`), so the gate stops copying
the whole map per call. ONE function taking the threshold rather than two functions, because the crate's public surface
is capped (`index-crate-isolation`). ❌ Never call `above_threshold` straight from a UI-driven path.

**Freshness rides the recompute subscription, ❌ not the generation stamp.** An INCREMENTAL rescore writes rows at the
CURRENT generation without bumping it (`importance/writer.rs` § `apply_incremental`), so a generation-keyed cache would
serve stale scores until the next full pass. The cache drains its `importance::read::subscribe` receiver at each read: a
`Delta` patches in `O(changed)`, a `ReloadAll` re-reads, and a LAGGED receiver re-reads (a lag is never "nothing
happened" — see `importance/read/DETAILS.md` § The reload contract). Draining is pull-based rather than a background
task because every read already goes through one function, which is the same freshness without spawning a per-volume
task from inside a blocking closure.

`MediaScheduler::folder_scores` deliberately keeps its own uncached read: it runs once per enrichment pass, not per UI
event, and a pass wants the store as of its own start.

`media_index_covered_count(threshold, volume_ids)` powers the slider's live preview: across the ENABLED volumes (master
on AND (local, or SMB opted-in); MTP never), how many folders score `≥ threshold` and how many images they hold —
exactly `(importance ≥ threshold) AND opted-in`, never a non-opted-in SMB/MTP volume. The qualifying-image count per
folder is an O(entries) index walk, so it's cached per volume (`coverage::get_or_build`, a `folder → count` map) and the
threshold is applied cheaply by intersecting the scores with it — a debounced drag only re-runs the CACHED importance
read (§ The importance score cache) + `covered_in_scope` (pure, unit-tested; it dispatches on the scope, so the count
follows the same rule the enrichment gate does — § The indexing scope). `pending` is `true` when any enabled requested
volume isn't ready (still scanning / not yet scored), so the UI voices "naspi still scanning" rather than a confident
wrong number. `media_index_volume_state` carries `qualifying_count: Option<u64>` (the honest denominator for "12,000 of
38,900 images"); ETA math lives UI-side off `(enriched_count, qualifying_count)`.

**Counting is a SINK over the walk, never a materialized list.** `enrich::for_each_qualifying_image` is the one walk
shape (`scheduler/DETAILS.md` § The walk); `coverage::count_qualifying_images` aggregates straight into
`per_folder`/`total`, so a cold build holds `O(folders)`. Deriving counts by collecting first was the launch-time memory
runaway: on an 11.3M-entry NAS index it turned a handful of integers into gigabytes of transient heap (646 MB peak in
dev, 6.7 GB and once 50 GB in prod, against a flat 155 MB with the build suppressed; measured on a fresh launch over the
11.3M-entry NAS index, 2026-07-25 — `docs/notes/memory-runaway-rust-heap-2026-07-25.md`, which isolates it to this exact
call with a single-lever A/B and a `malloc_history` stack).

**Who may pay the cold walk.** `coverage::get_or_build` builds; `coverage::cached` reads the cache or returns `None`,
never building. Polls and startup paths MUST use `cached`: `media_index_volume_state` (which fires at launch, before any
user asks for a number) and `MediaScheduler::stored_coverage_counts` both do, so image indexing being OFF can't trigger
a whole-index walk. The user-initiated settings reads build: `media_index_covered_count` (the slider preview, called on
the image-indexing section's mount) and `stored_coverage` (the reclaim preview). Opening that section is therefore what
warms a cold volume. `qualifying_count: None` consequently means "no honest number yet" (index not registered, OR
nothing has counted); report it as unknown, never as `0`. `MediaIndexImportanceSlider` keys its "the drive scan is still
running" line on `qualifyingCount === null` AND `covered.pending`, so it can't claim a scan that isn't happening; the
plain "counting…" branch covers the gap.

**One walk per volume, concurrent callers deduplicated.** `get_or_build` re-checks the cache under a per-volume build
lock (`BUILD_LOCKS`), so racers queue and the losers find it warm rather than each running their own walk (launch logs
once showed volume `root` walked twice within 70 ms). The build never runs under the `COUNTS` lock, which would stall
every other volume's cheap cached read.

**Keeping the cache warm, not cold.** The cache would go cold on every pass if a pass just invalidated it, so the next
slider preview would pay the full O(entries) walk again (tens of seconds to minutes on a multi-million-entry root index)
— even though the pass had just run that exact walk and thrown it away. So the pass that owns a walk refills from it
instead: a full/network pass calls `coverage::replace_from_entries` with its whole-volume `walk_image_entries` result,
IMMEDIATELY after the walk (not at the pass's end), so readers have the denominator for the pass's whole duration and a
cancelled or paused pass still leaves correct counts behind. A live tick calls `coverage::patch_touched_dirs` to replace
just the counts for the dirs it re-walked (a tick can't rebuild the whole cache — it only walked the touched dirs).
`patch_touched_dirs` runs on the SAME `enriched > 0 || gc_count > 0` condition the live tick's vector-cache invalidate
does: both a GC'd deletion and a new/changed image move a touched dir's qualifying count. `coverage::invalidate`
survives for the rare reclaim / retro-delete prunes: those change no index rows (only stored `media.db` rows), so the
qualifying set is actually unchanged and invalidate is conservative — a cheap cold rebuild on a rare user action rather
than a stale count.

An `ext` column + partial index in the drive index to prune this cold walk was measured on copies of the real dev DBs
and skipped: 6.4x on the root DB but ~nothing on the image-dense NAS, for a schema bump and +58 MB, against a walk that
runs once per session (`docs/notes/m7-ext-index-walk-bench-2026-07-24.md`, incl. why the old 42 s baseline didn't
reproduce). Revisit only off an in-app measurement showing tens of seconds, or a sparse-image 10M+-file corpus.

## The per-folder accounted aggregate + the index-status indicators (`coverage/accounted.rs`,

`apps/desktop/src-tauri/src/commands/media_index/file_status.rs`)

The covered-count cache above is the DENOMINATOR (`eligible`: images the drive index says qualify per folder). The quiet
per-image / per-folder / per-drive index indicators also need the NUMERATOR: how many of those are actually indexed. So
`coverage/accounted.rs` maintains a per-directory `accounted` count = images whose `media_status` row is `done` OR
`failed` (both count — a `failed` image can't progress, so completion is `accounted == eligible`, else one corrupt file
keeps a folder reading incomplete forever).

**Why a SEPARATE cache from `COUNTS`, and a separate FILE.** The two aggregates have different sources and update
models: `eligible` (`COUNTS`) is REBUILT from a whole-volume index walk each pass (`replace_from_entries`), reflecting
the live filesystem; `accounted` (`ACCOUNTED`) is maintained INCREMENTALLY from the stored rows. Folding accounted into
`COUNTS` would let the walk-driven `replace_from_entries` wipe the incrementally-maintained counts every pass. They're
reported together by the folder-coverage command but live apart. ❌ Don't re-merge the two files either: the eligible
half reaches the index walk and the accounted half is written by the writer thread the walk feeds, so one shared file
puts `coverage`, `scheduler::enrich`, and `writer` back in one import cycle. Split, each half depends only on leaves
(`paths`, `rollup`, `store`) and the graph is a DAG:
`coverage::eligible → scheduler::enrich → writer → coverage::accounted`.

**The maintenance invariants (mirroring how `eligible` is seeded and patched):**

- **Seed** once from a `SELECT path, state FROM media_status` scan bucketed by parent dir. This happens on the ONE
  writer thread as its FIRST action (`writer_loop` calls `accounted::seed_from_conn` before processing any message), OR
  lazily via `accounted::ensure_seeded`, which `coverage::folder_coverage` runs itself when the writer hasn't spawned
  this session (feature just enabled / volume never enriched). Both go through `seed_if_absent` (insert-if-absent).
- **Increment** on a genuinely-new completion: `apply_upsert` does a cheap PK existence check (`SELECT EXISTS(…)`)
  inside its transaction and returns whether it INSERTED vs updated; the writer bumps `accounted[parent_dir] += 1` only
  on a new `done`/`failed` row. A `done`↔`failed` transition or a re-enrich of an existing path does NOT move it (the
  path was already counted).
- **Decrement** on deletion: GC / prune / retro-delete return the paths whose `media_status` row actually existed
  (`delete_rows_for_paths` collects the ones `DELETE` reported), and the writer `-1`s each parent dir (saturating, never
  negative). `PurgeVolume` resets the whole aggregate.
- **Subtree rollups**: `folder_coverage` returns each folder's `eligible` and `accounted` summed over the folder AND all
  descendant dirs (`rollup::build_subtree_rollup` adds each dir's count to itself and every ancestor — the one piece
  both halves share, which is why it's its own leaf file). The rollup is cached alongside the per-dir map
  (`VolumeAccounted.subtree`, `ELIGIBLE_ROLLUP`) and invalidated on any change; a query is a cached-map lookup, NEVER a
  `media_status` scan.

**The concurrency line (why insert-if-absent is race-free).** The writer is the ONE mutator of both `media.db` and this
volume's `accounted`, and it seeds BEFORE its first commit. So whenever a committed row could exist, the entry is
already present, and a concurrent command-side seed either wins first (a complete on-disk baseline, since no writer
delta can have landed yet) or finds the entry present and discards its scan. Either way the writer's deltas compose onto
exactly one baseline. A delta on an unseeded volume is a no-op (never inserts a partial entry a later seed would wrongly
trust).

**Staleness caveat (accepted first cut).** A `done` row whose file changed since indexing still counts as `accounted`
until it's re-enriched, so a folder / drive can briefly read "complete" while a changed file awaits re-work. Excluding
stale rows would need a per-row `(mtime, size)` compare against the live index, out of scope here (the per-FILE badge
does surface `stale` via `needs_enrichment`, but the folder/drive rollups don't subtract it).

**The two commands** (both `spawn_blocking`, both speak the volume's INDEX-path space — == the OS path for a local
volume; a network volume's mount-root mapping is a later slice, so the file overlay ships local-first):

- `media_index_file_status(volume_id, paths) -> Vec<FileIndexStatus>`, one per input path IN ORDER.
  `FileIndexStatus { path, state }` with `state` a camelCase enum: `indexed` (a `done` row, current per
  `needs_enrichment`), `stale` (a stored row the live `(mtime, size)` or the analyze engine stamp made stale), `failed`
  (a `failed` row), `pending` (an eligible image the coverage gate would enrich but which has no row yet), `excluded`
  (an indexable image the gate would NOT enrich — out of scope / below threshold / under an excluded folder),
  `notApplicable` (not a qualifying image → no badge). Backend does ALL classification: a bounded, dir-scoped
  `walk_image_entries_in_dirs` supplies each path's live `(mtime, size)` + sibling-aware qualification, `media.db`
  supplies the stored row, and `local_should_enrich` + the live exclusion veto split `pending` from `excluded` (only for
  an un-enriched image). A stored row WINS over the gate: an indexed image reads `indexed`/`stale`/`failed` even if the
  current setting no longer covers it (forward-only, the rows stay searchable). The `pending`/`excluded` scores are
  threshold-filtered exactly as `pass_coverage` sees them (`coverage_scores`). The staleness engine stamp comes from
  `MediaScheduler::current_analysis_stamp`; a missing scheduler falls back to each row's own stamp (only `(mtime, size)`
  staleness).
- `media_index_folder_coverage(volume_id, folder_paths) -> Vec<FolderCoverage>`, one per input folder in order.
  `FolderCoverage { path, eligible, accounted }` (subtree totals). The frontend derives the two-state folder badge
  (`accounted == eligible` vs `<`, no badge when `eligible == 0`) and the `accounted/eligible` tooltip. It is one
  `coverage::folder_coverage(data_dir, volume_id, folders)` call, which seeds the accounted aggregate itself if the
  writer hasn't spawned, then reads the cached rollups.

Both feature-off-short-circuit (`notApplicable` for every file, zeros for every folder), matching the other commands.

## Per-folder photo-search exclude + the privacy retro-delete

`network::config` gained `excluded_folders` (seeded from `mediaIndex.excludedFolders`, live-applied by
`media_index_set_excluded_folder`): an image at or under an excluded folder never enriches, a HARD veto that beats any
"always index" override — the privacy complement to the opt-in (protect a high-importance `~/Documents/IDs` the
threshold alone can't).

Excluding a folder does more than veto the future: it **retro-deletes** the folder's already-indexed rows so extracted
OCR text stops being searchable at once (privacy is a hard requirement, not "eventually on the next GC"). The pieces:

- **Why it's a new deletion path (vs the GC-safety doctrine).** GC's safety comes from _when_ it runs (only a
  `Completed` edge, tree whole — § The GC safety argument). The retro-delete is USER-EXPLICIT and derives ONLY from
  settings state (the exclusion the user just set), never scan/bus/gate state, so it can't wipe live coverage by
  mistiming — it needs no edge. This is the same doctrine the reclaim prune rides. The slider stays forward-only; the
  ONLY row deletions are (a) vanished files via GC on a completed edge, (b) the reclaim prune, (c) this privacy
  retro-delete, and (d) the live-tick scoped GC.
- **Precedence + path mapping.** Exclusion beats coverage everywhere (enrichment gate AND retro-delete), same
  trailing-slash-safe `path_is_within` the veto uses. The exclusion config is OS-path keyed; local rows store index
  paths == OS paths, network rows store mount-stripped index paths — so the retro-delete maps the OS folder into each
  volume's index space via `network::fetch::os_folder_to_index_prefix` (the inverse of `os_join`: passes through on a
  local volume, strips the mount root on a network one, `None` when the folder isn't under that mount).
  `MediaScheduler::retro_delete_excluded_folder(folder, mounts)` iterates the reachable volumes, prunes each via its ONE
  writer, `VACUUM`s (privacy: the text leaves the disk), and drops the vector + coverage caches.
- **Two mid-pass races, both closed** (else the retro-delete is cosmetic). (1) A pass already running holds a
  start-of-pass config snapshot, so its coverage gate is stale — but exclusion is read LIVE
  (`network::config::is_excluded`, the ONLY live part; threshold/override stay snapshot), so the next image it looks at
  is vetoed. (2) The in-flight-analyze TOCTOU: a pass checks the veto, runs a SECONDS-long `analyze`, then upserts; an
  exclusion landing during the analyze would slip a row past the passed check, and a later pass won't collect it (the
  file is still in the GC `current` set). Closed by re-checking the live veto immediately before EACH upsert (both
  cores). Belt-and-suspenders: the command sequences config-set (live veto first) → retro-delete → retro-delete again (a
  double-tap; the blocking prune is its own barrier), so a straggler upsert that squeezed into the enqueue window is
  swept. Order matters — the config write MUST precede the first delete, or in-flight images re-check stale state.
- **Un-excluding** only clears the veto: NO re-delete and NO auto re-enrich — the next natural pass picks the folder up
  again.
- **Offline network volumes** aren't reachable when the exclusion is set (no mount root to map with), so the
  retro-delete skips them and RE-FIRES on reconnect: `wire_volume` (the registration hook) purges any currently-excluded
  folder under a volume as it (re)registers. Cheap when nothing is excluded.
- **The trigger** is a folder context-menu item ("Don't index images in this folder" / "Index images here again", shown
  only while image indexing is on, exactly one keyed on the current state). It's a NATIVE (Rust) menu, so the click
  emits a `MediaIndexFolderExclusion` event to the FE, which persists `mediaIndex.excludedFolders` and calls
  `media_index_set_excluded_folder` (the native menu can't write the FE settings store) — the persist + live-apply +
  rollback pattern from `network-volume-prefs.ts`, in `src/lib/media-index/excluded-folders.ts`, wired in the main
  route's `setupMenuListeners`.

## WAL checkpoint at pass completion (`writer/maintenance.rs`, plan M9)

**Decision:** the writer runs `PRAGMA wal_checkpoint(TRUNCATE)` (`writer::maintenance::run_wal_checkpoint`, driven by
`MediaWriter::checkpoint_wal`) once an enrichment pass completes and actually wrote rows — at both the local
(`run_pass_blocking`) and network (`run_network_pass_blocking`) seams, inside the same `enriched > 0 || gc_count > 0`
guard that drops the vector cache. It runs on the writer thread's own connection (the single-writer invariant), in
autocommit. This mirrors `importance/writer.rs::run_wal_checkpoint` verbatim (this module ports importance's patterns);
the "why", busy tolerance, and the 250 ms bracket are documented there.

**Why here:** without a `wal_autocheckpoint` override, SQLite's default PASSIVE autocheckpoint never shrinks the WAL
file, so a per-image-upsert enrichment pass lets it creep up in place. A pass completion is the natural quiet point to
TRUNCATE it back down (target ≤ ~16 MB at rest). Best-effort: the callers `let _ =` the result, so a reader-blocked
checkpoint never fails a pass. Distinct from `VACUUM` (the reclaim/retro-delete paths), which reclaims free _pages_ in
the main DB after deletes; the checkpoint reclaims the _WAL file_ after writes.

## Progress events + vanished-file skip (`events.rs`, `progress.rs`)

A pass joins the top-right indexing indicator as a second publisher (the FE side is `lib/indexing/DETAILS.md` §
Image-enrichment publisher). `events.rs` defines two typed Tauri events + the emission machinery:

- **`IndexEvent::MediaEnrichProgress`** (`media-enrich-progress` on the wire): throttled progress. `total` /
  `bytes_total` are the ENRICHABLE-subset denominators (`enrichable_totals` / `network_enrichable_totals` = images
  passing `should_enrich` AND not `is_excluded`), NEVER the full walked set — a raw `images.len()` denominator rebuilds
  the never-finishes bug inside the indicator. `done` counts every subset image the pass finishes handling (enriched,
  already-current, or a quiet skip), so it reaches `total` on completion. Bytes ride `ImageEntry.size` (`Option`, `None`
  counts 0 — under-count, never lie). The pure `should_emit_progress` throttle (`progress.rs`) fires at pass start, then
  ≤ every 500 ms or 100 images. Emission is a cheap counter + time check per image; the `EnrichProgressSink` seam keeps
  the registry-free cores testable (a recorder in tests, the throttled `EnrichProgressEmitter` in production).
- **`IndexEvent::MediaEnrichTerminal`** (`media-enrich-terminal` on the wire): exactly one per pass on EVERY exit path.
  The `EnrichTerminalGuard` (RAII) guarantees it: it defaults to `Failed` and emits on `Drop`, so a `?`-error bubble (a
  writer-send failure) still reports a terminal; `run_pass_blocking` / `run_network_pass_blocking` override the reason
  (`Completed { enriched, gc_count }` / `Cancelled` / the two `Paused*`) before a clean exit. Without a terminal on
  every path the FE row sticks at "enriching" (the `index-scan-aborted` stuck-row bug). The local pass distinguishes
  cancel from completion via `PassSummary.cancelled`; the network pass maps its `NetworkPassOutcome`.

The scheduler holds an `Arc<dyn EventSink>` (the real one wired in `start` via `new_with_events`, a `NoopEventSink` in
unit tests via `new`), so a pass reports nothing under test. `pass_emitters` builds the throttled progress sink + the
terminal guard from it. The wire payloads live app-side in `events/index_mapping.rs`; this area produces only the typed
values.

**Vanished / phantom files are DEBUG, never WARN.** A file deleted between the index walk and its analyze, or an
orphaned index row whose reconstructed path can never read, surfaces at analyze as a typed `VisionError::Missing`. The
local core skips it QUIETLY (DEBUG), writes NO row (not `Failed` — the file is gone, so a later completed pass's GC
collects any stale row), and counts it as processed so `done` still reaches `total`. The network core already handles a
vanished source via `FetchError::NotFound` (same quiet skip). The too-small-image skip is a sibling quiet case: it
writes an empty `Done` row instead. Pinned by
`enrich_tests::a_vanished_image_still_completes_the_pass_at_done_equals_total` and
`enrichable_totals_excludes_deferred_and_excluded_images`.

## The IPC surface (`../commands/media_index/`)

One module per command family: `search.rs` (OCR, tag, semantic, find-similar, dedup), `state.rs` (the per-volume state +
the covered-count preview), `reclaim.rs` (the outside-the-setting preview and prune), `file_status.rs` (the per-file
overlay + per-folder badge), `clip_model.rs` (install state, download, delete), `thumbnail.rs` (grid tokens), and
`policy.rs` for the coverage-CHANGING setters, each of which decides whether the change BROADENS coverage and needs an
immediate pass through a pure `*_should_kick` fn tested in `apps/desktop/src-tauri/src/commands/media_index/tests.rs`.
`mod.rs` keeps only what several of them need (the hit-limit clamp, the ONE enabled-volume rule) and glob-re-exports
every module, so each command keeps its `commands::media_index::<name>` path in `ipc.rs` — the glob is deliberate:
`#[tauri::command]` also generates hidden `__cmd__*` / `__tauri_command_name_*` macros that `generate_handler!` resolves
through the same path.

Every command is `async` + `spawn_blocking` (a sync `#[tauri::command]` would block the IPC thread), offline-capable,
and registered in BOTH `ipc.rs` and `ipc_collectors.rs` — regen the typed bindings with `pnpm bindings:regen` after any
command change.

- **`media_index_search_ocr(volume_id, query, limit?)`** — the IPC door onto `MediaIndex::search_ocr` (plan Decision 8):
  it resolves the app data dir, opens `MediaIndex` for the volume, and searches. `limit` defaults to 200, clamped
  to 1000. An empty query, an un-enriched volume, or an offline/purged `media.db` returns an empty list, never an error.
  When the master toggle is off it short-circuits to an empty list before opening `media.db` (defense in depth,
  mirroring `media_index_covered_count`; the frontend also hides the OCR section entirely when off).
- **`media_index_volume_state`** → the honest per-volume coverage signal. `indexing` is a cheap in-memory snapshot off
  the scheduler's `PassCoordinator::is_running` (`MediaScheduler::is_enriching`); `enriched_count` is a `COUNT(*)` over
  `media_status`. It lets the UI tell apart four states rather than ever showing a confident-looking empty result that's
  really "not indexed yet": off (hint to enable), still indexing ("results may be incomplete"), enriched-but-no-match (a
  genuine miss), and not-indexed-yet. Polled per search (no event subscription yet; a reasonable later upgrade).
- **`media_index_thumbnail_token` / `media_index_drop_thumbnail_tokens`** — the grid's thumbnails REUSE the existing
  viewer preview scheme (`cmdr-media://` via the viewer's `file_viewer::media` token registry), never a
  media_index-produced thumbnail file (plan Decision 5). `media_index_thumbnail_token` classifies a path by magic bytes
  and, for an image, mints a `cmdr-media://` token; the frontend builds the URL via the viewer's `mediaUrl`
  (single-source). **Token lifetime is the CALLER's here** — a viewer session drops its token at the window-close choke
  point, but the grid has none, so `ImageSearchResults.svelte` drops every token it minted when the result set changes
  or the component unmounts, or the token map leaks path mappings. The scheme serves the FULL original bytes
  (browser-downscaled for the tile); that's the accepted cost of reusing the preview path rather than producing a
  downscaled thumbnail — a real thumbnail cache would be a media_index-produced file Decision 5 defers.
- **Query commands over the read API**: `media_index_find_similar`, `media_index_dedup_clusters`,
  `media_index_search_tag`, `media_index_search_semantic`, `media_index_covered_count`, `media_index_file_status`,
  `media_index_folder_coverage`, `media_index_reclaim_preview`, `media_index_clip_model_status`. **Setters** (in
  `policy.rs`): `media_index_set_importance_threshold`, `media_index_set_scope`, `media_index_set_excluded_folder`,
  `media_index_set_semantic_search_enabled`, the three network setters (`network/DETAILS.md`), plus the destructive
  `media_index_prune_below_threshold`, `media_index_delete_clip_model`, and `media_index_download_clip_model`.
- **Shapes for the frontend:** `SimilarImage { path, score: f32 }`, `DedupCluster { paths: Vec<String> }`,
  `TagHit { path, score: f32 }`, `CoveredCount { folders: u64, images: u64, pending: bool }`, `Tag { label, score }`,
  `ReclaimPreview { total_stored, covered_stored, doomed_count, estimated_bytes, pending }`,
  `ReclaimResult { deleted_rows, freed_bytes }`, and
  `MediaIndexVolumeState { enabled, indexing, enriched_count, qualifying_count, covered_qualifying_count, kept_count, waiting_for_importance, network_opt_in, always_indexed, paused }`.

### Threshold-aware volume state

`media_index_volume_state`'s `covered_qualifying_count` + `kept_count` come from
`MediaScheduler::stored_coverage_counts` — a counts-only sibling of the reclaim `stored_coverage` that does NOT allocate
the doomed-path `Vec` (the settings poll runs it every few seconds). Both share the ONE canonical survival rule
(`coverage::stored_row_survives`) and the `coverage` cache, so they can never disagree with the reclaim preview.
`covered_qualifying_count` drives the settings progress line "N of M in your covered folders" (N =
`enriched_count − kept_count`, capped); `kept_count` (= the reclaim doomed count) drives the quiet "K more indexed from
broader settings, still searchable" line, gated by the SAME `shouldOfferReclaim` floor so it never duplicates the
reclaim offer. Both `None` when the partition isn't safe (the automatic scope on an unscored volume).

## The frontend surface

The user-facing surface lives in the Svelte frontend, not here; this section is the map so the two stay in sync. The
network-volume UI is in `network/DETAILS.md`, the CLIP UI in `clip/DETAILS.md`.

- **The master toggle** `mediaIndex.enabled` renders in Settings > AI > Image search (a dedicated "Image search" card,
  `ImageSearchSection.svelte`), off by default. It live-applies through `settings-applier.ts` → `setImageIndexEnabled`
  (no restart), the standard backend-affecting-setting pattern.
- **The importance slider** — `src/lib/settings/sections/MediaIndexImportanceSlider.svelte`, rendered in the same card
  when `mediaIndex.enabled` is on. It exposes five NAMED BUCKETS ("Only my most-used folders" → "Everywhere, even
  folders I rarely open") over the typed threshold; each bucket maps to a fixed threshold stop
  `[0.8, 0.6, 0.4, 0.2, 0.0]` (left → right, restrictive → broad). Dragging RIGHT indexes MORE (a LOWER threshold). The
  **default is the rightmost bucket, threshold `0.0`** — deliberately equal to the backend
  `DEFAULT_IMPORTANCE_THRESHOLD`, so the UI and an unpersisted (sparse) store agree without eagerly writing a default,
  and it's non-regressive (junk is floored out at any level regardless). The persisted value is the raw threshold; the
  slider maps it to the nearest bucket on load.
- **Persist + live-apply** follows the `mediaIndex.enabled` precedent, NOT the per-item delta path: the slider calls
  `setSetting('mediaIndex.importanceThreshold', threshold)` and the `settings-applier.ts` passthrough pushes it to
  `media_index_set_importance_threshold`. (Threshold is a scalar, so it fits the applier's key→value table — unlike the
  network/exclude delta setters, which co-locate persist+IPC in a prefs helper.)
- **Live honest preview** — the slider debounces `media_index_covered_count(threshold, enabledVolumeIds)` over the
  enabled volumes, rendering "Indexes about N images across M folders" with thousands separators + ICU plurals.
  `pending` ⇒ a "still scanning" caveat. A drag also shows the incremental delta vs the last settled level ("Adds about
  12,000 images"), which folds into the baseline once the value settles (~900 ms). No ETA on the slider: the
  enriched-rate isn't exposed and a fixed per-image cost would be dishonest across HEIC/RAW/network, so counts stand
  alone.
- **Honest per-volume progress** reads `qualifying_count` from `media_index_volume_state`: the local disk line lives in
  the slider component, the network lines in `MediaIndexNetworkVolumes.svelte`, both showing "N of M images indexed" (or
  "Counting images…" while `qualifying_count` is `null`).
- **The OCR / image grid** is `src/lib/search/ImageSearchResults.svelte`, which QueryDialog renders via its
  `resultsExtra` snippet slot (Search-only; Selection passes none), reusing the SAME live query text as the filename
  results. Its per-tile "Find similar images" action re-queries the grid via `media_index_find_similar` from that tile's
  STORED (index-relative) path (NOT the resolved OS path — the command keys on the stored path), showing a "Similar to
  <name>" header with a back button; a new query exits similar mode. The "why matched" snippet (`[`/`]`-wrapped matched
  terms) is parsed to structured segments by the pure `src/lib/search/ocr-snippet.ts` and rendered with `<mark>`, NEVER
  via `{@html}` — a document whose OCR text contains markup can't inject anything.
- **Tags need no separate UI**: tag labels fold into `media_ocr` (`source='tag'`), so the existing OCR keyword search
  already matches tag words and shows them in the snippet. `OcrHit` carries no `source`, so the grid can't label a hit
  as "matched a tag" without a backend field — deferred, not needed now.
- **Reclaim** is `MediaIndexReclaim.svelte` under the slider (`getEnabledMediaIndexVolumeIds` shared with the slider
  preview). It shows the line + button only once counts settle (parent-passed `blocked` while waiting on importance / a
  scan, plus the backend `pending`) AND the leftover clears the pure `shouldOfferReclaim` floor (> 100 rows AND > 5% of
  stored). The copy frames value first (the extra entries "stay searchable"), then the button offers the
  space-vs-reindex tradeoff — one narrative, composing with the kept-rows line, never two sentences in tension. A
  confirm dialog (recoverable, but re-reading costs time) precedes the prune; an honest toast reports the freed space.
  The arithmetic behind it is `scheduler/DETAILS.md` § Reclaim space.

## Standing cost

`media_index` adds a THIRD long-lived writer thread per volume (index + importance + media) plus a per-volume `watch`
listener. Fine at a few-volumes scale, but it scales per mounted volume — note it before adding more per-volume threads.

## What's left for later

- **Per-folder COUNTS now exist** (`coverage/accounted.rs`'s incremental aggregate + subtree rollups): the honest
  `eligible` / `accounted` per folder feed `media_index_file_status` / `media_index_folder_coverage`, which the file-
  and folder-icon overlays consume (`file-explorer/selection/DETAILS.md` § Image-index overlay) and the drive dot rolls
  up per volume. Accepted staleness caveat: § The per-folder accounted aggregate.
- **CLIP model size:** ~267 MB combined — the image tower is 8-bit palettized (M5b, 2026-07-23; cosine 0.9995, ~83 MB),
  the text tower stays fp (~184 MB; its 8-bit inference NaNs). Down from ~392 MB non-palettized. Numbers:
  `clip/install.rs`.
- **Later:** faces (detect/embed/cluster/name), the durable identity store, and LLM captions.

## Testing

Most tests are FFI-free and registry-free. Per-area inventories live in each area's `DETAILS.md` § Testing; what this
level owns:

- **Pure, top-level:** the qualification predicate (`predicate.rs`), the covered-count arithmetic over a synthetic
  counts+scores map (`coverage/tests.rs`), the two caches (`coverage/eligible/tests.rs`, `coverage/accounted/tests.rs`),
  the command limit clamp and the `*_should_kick` decisions
  (`apps/desktop/src-tauri/src/commands/media_index/tests.rs`), the progress throttle (`progress.rs`).
- **Privacy retro-delete (all real red→green — deletion is data-safety-critical):** the writer prune primitives
  (`writer/tests.rs`) — `prune_under_folder` deletes rows at or under a folder across ALL four tables and only those,
  trailing-slash-safe (`/Photos2` survives pruning `/Photos`); `prune_paths` deletes only the explicit set; prune +
  VACUUM round-trips; `prune_all_clip` drops embeddings, resets stamps, and keeps Vision data. The live veto and the
  mid-`analyze` TOCTOU are pinned on both cores (`scheduler/DETAILS.md`, `network/DETAILS.md`), and the scheduler
  retro-delete is covered in `scheduler/kick_tests.rs`.

## The public surface

11 public modules and 51 public items, down from 14 and 142, each one decided rather than inherited. The item-by-item
audit, the folds, and what narrowing the modules exposed: `../indexing/handle/DETAILS.md` § "The other two subsystems".

Two rules it leaves behind:

- **A new `pub` is a promise.** Take one of the four dispositions first — a facade method named for what the caller
  wants, a fold into a call that already exists, a delete, or a gated door.
- **`#[cfg(test)]` while every consumer is inside the crate; a feature only when one lives outside.** The app turns
  `testing` on for every dev target, so a feature-gated item with only in-crate callers exists in the non-test lib build
  with nothing calling it, and `#[deny(unused)]` makes that an error.
