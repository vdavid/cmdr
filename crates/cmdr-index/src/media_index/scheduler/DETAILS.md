# Media-index scheduler — details

The depth behind `CLAUDE.md`. Read this before any non-trivial work here: editing, planning, reorganizing, or advising.
Subsystem-wide context (the port rationale, the GC safety argument, the scope model, the coverage caches) lives in
`media_index/DETAILS.md`.

## File layout

`scheduler/` splits the coalesced-pass machinery by responsibility: `mod.rs` holds the `MediaScheduler` struct + its
pass bodies (`run_pass_blocking`, `run_network_pass_blocking`, `folder_scores`, `retro_delete_excluded_folder`);
`coordinator.rs` holds the pure, testable `PassCoordinator` (one pass per volume, coalesced re-run — covered by
`coalescing_tests`); `lifecycle.rs` holds the scheduling/wiring layer (`start`, `kick_all_ready_passes_with`,
`kick_network_pass`, `wire_volume`, `spawn_pass`, `local_should_enrich`, `local_dir_may_be_covered`, `pass_coverage`,
`PassKind`); `enrich.rs` holds the walk + the shared enrich/GC core; `pool.rs` the parallel workers; `live.rs` the
live-follow tick (with `live_tests.rs` and `live_bench.rs`, the `#[ignore]`d cost harness behind
`docs/notes/live-tick-cost-2026-08-21.md`); `reclaim.rs` the user-explicit prune. `kick_tests.rs` owns the shared test
fixtures (`build_index`, the importance/media seeders, `reset_gate`) that `live_tests` and `reclaim_tests` reach for.
The starting and kicking entry points are `MediaScheduler::{start, kick_all_ready_passes, kick_network_pass}` in
`mod.rs`, one-liners over `lifecycle.rs`'s private halves, so a host holding the scheduler calls methods on it rather
than passing it back into a module.

## The lifecycle bus

`media_index`'s scheduler subscribes to `indexing/lifecycle/lifecycle_bus.rs` exactly as `importance`'s does — its OWN
`start()` mirrors the ordering (subscribe to registrations → sweep `ready_volumes_with_kind()` → wire per-volume
subscriptions). It can't piggyback `importance`'s subscription; because `app.manage` is keyed by type, an
`Arc<MediaScheduler>` coexists fine alongside `importance`'s scheduler. The bus mechanism (watch vs broadcast,
late-subscriber replay, the registration bus, why the sender outlives the registry) is documented once in
`../../indexing/DETAILS.md` — not re-documented here (single-source).

`wire_volume` routes by typed kind: LOCAL enriches by default (when the master toggle is on); an opted-in SMB volume
runs the conservative network pass (`../network/DETAILS.md`); MTP is NEVER background-swept. Both local and SMB
subscribe to the SAME bus the same way; only which pass method runs differs. The opt-in is checked INSIDE the network
pass, so flipping it on takes effect on the next scan completion (and the opt-in command kicks an immediate pass).

The edge-consumption discipline (`borrow_and_update`, never a poll) and why the startup sweep filters to `Fresh` are the
GC safety argument: `../DETAILS.md` § The GC safety argument.

## Parallel enrichment (plan M2)

**Decision: parallelism is N INDEPENDENT backends, not concurrent calls into one.** A `VisionBackend` is single-threaded
by construction (the CF confinement in `../backend/DETAILS.md`), so N-way parallelism means N whole backends — each its
own thread, stack, autoreleasepool, and request handlers — driven by N worker threads in `pool.rs`. Worker 0 rides the
scheduler's long-lived backend; workers 1..N are built on demand from a `BackendFactory` and dropped when the pool
shrinks, so a steady N=1 pass builds nothing extra and behaves byte-for-byte like the pre-M2 loop (the serial
`enrich_and_gc_scoped` is now a thin wrapper over the pool at width 1, so every enrich test exercises the pool core).

**Why measured, not asserted.** The M2 spike (`../backend/vision/spike.rs`) measured throughput topping out at ~1.25x by
N=2 on an M3 Max (verified 2026-07-23, decode-vs-full-analyze scaling at N ∈ {1,2,4,8} over 200 local images): the ANE
serializes inference (~89% of per-image wall time) so it doesn't parallelize; only decode (~11%, CPU) scales (to 5.4x at
N=8). So the default is 1 (never take more machine unasked — principle 5), the slider is explicit consent, and the
microcopy doesn't over-promise.

**The win is decode↔inference OVERLAP, captured by symmetric workers.** The ~25% at N=2 is not "two inferences at once"
(the ANE won't) — it's that while worker A blocks in ANE inference, worker B decodes the next image on the CPU, so the
decode stage runs AHEAD of and overlaps the serialized inference stage. N independent full workers yield exactly this
pipelining without an explicit decode-stage/inference-stage split: the ANE is the single bottleneck, so a symmetric pool
feeding it keeps it saturated just as a dedicated decode-fan-in would, and is far simpler (no cross-stage `!Send`
handoff of `CGImage`s). Past N=2 the extra workers only pile up behind the ANE, hence the plateau/regression.

**Decision anchor — what would lift the ~1.25x ceiling (and what would NOT).** ❌ Do not "clean up" the slider as
useless because N=8 ≈ N=2 today: the cap is the ANE serializing per-request inference, NOT the thread model. What lifts
it is a BACKEND change — batched Vision requests (one `performRequests` over many images), an explicit
`MLComputeUnits`/compute-unit configuration, or a GPU/CPU inference fallback that adds a second parallel compute unit —
at which point the SAME pool scales further with no rework. More threads never will. The slider's max is the CPU count
by design (David's call), and the backend clamps to it; the honest low-gain-past-2 shape lives in the microcopy, not in
a shrunk range.

**How the pool honors N, live.** `run_enrich_pool` runs in batches: workers pull image indices off ONE shared atomic
cursor (each index taken once ⇒ no double-enrichment is structural, not a lock), re-reading the effective worker count
(`gate::parallelism` capped by `thermal::current_pressure`) between images. A SHRINK retires the excess worker slots
within the running batch; a GROW ends the batch so the outer loop re-spawns wider. The cursor never rewinds, so a
mid-pass slider move or a thermal event applies within ~one image with no pass restart. The single `MediaWriter` thread
is untouched (parallelize compute, never DB writes), and `gate::should_stop` (watchdog OR master toggle off) still stops
the pass promptly and skips GC.

**Thermal backoff** (`../thermal.rs`): `NSProcessInfo.thermalState` read as a TYPED enum (never a string) caps the
EFFECTIVE workers — halved at `serious`, dropped to 1 at `critical` — so N workers pounding the ANE can't cook the
machine into a system-wide throttle that hurts the foreground app more than it helps enrichment. It only ever lowers the
user's chosen count.

The network pass's three-stage pipeline (dispatcher + K fetch workers + N compute workers, byte-bounded prefetch) rides
this same pool: `../network/DETAILS.md` § Network parallelism.

## The walk and its memory floor

`enrich::for_each_qualifying_image` is the one walk shape: it streams file rows ordered by `parent_id`, hands each
COMPLETE per-dir group to `qualify_dir` (the sibling-aware rules need the whole name set), and calls a sink with
`(dir, name, mtime, size, kind)` still split. `enrich::walk_image_entries` is the collecting sink for the passes, which
genuinely need the list, and holds one heap path `String` per image; `coverage::count_qualifying_images` is the
aggregating sink, which holds `O(folders)`. Why aggregation must never collect first: `../DETAILS.md` § Covered-count
preview.

**The walk's floor is its FOLDER side, held compactly.** File rows stream by, but the directories have to stay resident
for the whole walk: rebuilding a folder's absolute path follows parent pointers upward, in any order. So
`indexing/store/dir_tree.rs`'s `DirTree` holds them as one name arena plus a 24-byte `(id, parent_id, name slice)`
record per folder, sorted by id and binary-searched, fed by `IndexStore::for_each_directory` (three columns, streamed,
names borrowed off SQLite's row buffer so the query allocates nothing per row). Reading the same folders as `EntryRow`s
and indexing them by id costs ~3× that and one heap `String` per folder: on the 13.5M-row NAS index, 76.0 MB against
24.6 MB, and 3.40 s against 1.22 s to load (measured 2026-07-25 over 391,563 directories,
`test_support::heap_bytes_held` in a throwaway probe). Binary search costs nothing over the hash map it replaced (0.83 s
→ 0.70 s to reconstruct all 391,563 paths): the walk resolves a path once per folder-with-files, and the sorted array is
far more cache-friendly than a map with hashbrown's power-of-two capacity slack. ❌ Don't reach for
`IndexStore::all_directories` here. `importance/scheduler/walk.rs` shares the same tree (and its own numbers are in
`importance/DETAILS.md` § The walk).

The guards live in `enrich_memory_tests.rs`: one pins the whole walk's allocation count against a folder-heavy corpus (a
per-folder allocation blows straight through it), the other pins the compact tree at several times smaller than the
full-row shape.

## Importance-prioritized scheduling (the headline — plan Cross-cutting)

The local `run_pass_blocking` and the network `should_enrich` read `importance/`'s `ImportanceIndex`
(`MediaScheduler::folder_scores` → `above_threshold(threshold)`), the SAME signal the importance slider sets. The
scheduler:

- **orders** the walk by folder importance descending (`enrich::prioritized`), so high-importance folders enrich first;
- **filters** via a `should_enrich(path)` closure: an EXCLUDED folder never enriches (hard privacy veto, checked first);
  otherwise enrich when an "always index" override covers it OR its folder importance meets the threshold. A deferred
  image stays in the GC `current` set, so a below-threshold folder's rows are never wiped — only vanished files are
  GC'd.
- **`folder_scores` returns `Option`** — `None` when importance genuinely has no data for the volume (fresh, offline,
  importance disabled). "Has data" is `ImportanceIndex::is_scored` (`../DETAILS.md` § The importance "has scored"
  detection). Floored junk (`node_modules`, caches, hidden/system) has no importance row at all, so it's excluded at any
  threshold.

Importance keys on the INDEX identity, so the network gate strips the mount root off the OS path before the lookup. The
threshold and scope atomics live in `gate` (`../DETAILS.md` § The indexing scope).

## Defer-until-scored

When `folder_scores` is `None` (importance unavailable), BOTH the local and network passes DEFER their importance-gated
remainder while still honoring an explicit `config.covers` override — `local_should_enrich` and the network
`should_enrich` share this shape. The local pass does NOT fall back to enrich-all: importance's recompute over a big
volume takes seconds, and a pass that read `None` and enriched everything would over-index the whole volume permanently,
because the slider is forward-only (a below-threshold row is never deleted by moving the slider; only an explicit
reclaim or the privacy veto deletes). A visible, recoverable wait beats permanent over-indexing.

The **unscored → scored bridge** re-kicks the deferred remainder once importance lands:

- `wire_volume` subscribes to `importance::read::subscribe(volume_id)` SYNCHRONOUSLY, before the first pass.
  Watch-channel semantics: a receiver is caught up to the current version at subscribe time, so `changed()` fires only
  on the NEXT bump. A lazy "pass reads `None` → then subscribe" flow has a hole — importance can complete in the gap,
  the receiver comes up already-caught-up, and the volume defers forever. Subscribing up front (mirroring `search`'s
  `start_importance_weight_subscriber`) closes it.
- A pass that deferred sets a per-volume flag (`mark_deferred_for_importance`); the subscriber's
  `take_deferred_for_importance` reads-and-clears it and re-kicks a pass ONLY on that unscored → scored transition. Both
  the lifecycle bus and incremental rescores bump the recompute watch, so scoping the re-kick to the flag keeps a normal
  (already-scored) volume from re-kicking and a later incremental bump from re-walking the index for nothing.

The residual risk is made VISIBLE, never silent: the "has scored" detection guarantees the recompute _trigger_, not its
_success_ (a read-pool or write error leaves generation 0 with no notify). Under defer-until-scored that would mean
image indexing silently never starts. So `media_index_volume_state` exposes `waiting_for_importance` (enabled + index
ready + not scored), and the settings slider voices it ("Working out which folders matter…") REPLACING the generic
covered-count spinner — one honest line for one wait, never two spinners. There is deliberately NO silent fallback to
enrich-all on timeout: a persistently failing recompute is an importance bug to surface, not to paper over.

## Live enrichment: follow the index

Without live enrichment, the only enrichment triggers are scan-completion edges, user kicks, and the importance bridge —
so a NEW or MODIFIED image would wait for the next completed scan, and a DELETED image's rows would linger until a later
pass GC'd them. Live enrichment follows the index live, mirroring importance's incremental rescore rather than inventing
a new mechanism.

`live.rs` subscribes each LOCAL volume to `indexing::lifecycle::lifecycle_bus::subscribe_dirs_changed` (the SAME
per-volume `watch<DirsChanged>` importance's `start_incremental` consumes) from `wire_volume`, AFTER its kind
early-returns — so MTP and `LocalExternal` are auto-skipped, and SMB (which never publishes dir-changed batches; its
live path only enqueues index writes) is left out too. Each batch's touched DIRECTORY paths accumulate into
`pending_touched_dirs` and drive a coalesced, throttled tick (`LIVE_THROTTLE_WINDOW`, leading-edge-immediate then
trailing-edge-spaced — `live_debounce_wait`, copied from importance's `INCREMENTAL_THROTTLE_WINDOW` /
`incremental_debounce_wait`). `DirsChanged.paths` carries every changed file's parent PLUS its ancestor chain up to the
ever-present `/`; ancestor re-checks are harmless (staleness makes them no-ops), and `/` resolves to a cheap
direct-children walk, not a whole-index sweep. `watch` is last-value-wins, so a burst can drop intermediate batches —
the accumulator plus the next full pass heal it.

A tick (`run_live_tick_blocking`) resolves its coverage gates FIRST, filters the touched dirs down to the ones that
could enrich, and only then walks. It walks ONLY those dirs (`walk_image_entries_in_dirs`: per dir, resolve its entry id
via `store::resolve_path` from `ROOT_ID`, fetch the COMPLETE file-child set, run the sibling-aware `qualify_dir` —
fetching only changed files would mis-qualify RAW+JPEG pairs and Live Photos; a dir gone from the index is skipped and
its rows fall to the scoped GC). It then runs the SAME per-image enrich loop as the full pass through the shared
`enrich_and_gc_scoped` core, honoring the coverage gates, the live exclusion veto, and the `(path, mtime, size)` + stamp
staleness key.

**The coverage filter, and why it is one set.** A walk costs ~20 µs per touched dir (a `resolve_path` per path component
plus a `list_children_on`) against 0.03 µs for the filter, and on a machine whose churn is build output nearly every dir
is ineligible; when NOTHING survives the filter the tick returns before opening the index, loading `media_status`, or
spawning a writer (release build, M1 Max, `live_bench.rs`, 2026-08-21 — `docs/notes/live-tick-cost-2026-08-21.md`). ❗
The filtered set then goes to ALL THREE consumers or none: the walk, `GcScope::TouchedDirs`, and
`coverage::patch_touched_dirs`. Filter the walk alone and every stored row under a dropped dir is "in scope, absent from
the walk, therefore deleted"; hand the patch the unfiltered dirs and every dropped dir's cached count is replaced by
zero. Both are pinned by tests that were watched failing under exactly those mutations
(`a_live_tick_keeps_every_row_in_a_dir_its_coverage_filter_dropped`,
`a_live_tick_leaves_the_cached_counts_of_a_dir_it_filtered_out_alone`).

The filter itself is `lifecycle::local_dir_may_be_covered`, and it is a PROVABLE superset of the per-image
`local_should_enrich` rather than a documented promise: the score map is keyed by the parent folder (the dir itself),
and `NetworkEnrichConfig::may_cover_within` additionally keeps any dir an override entry names something at or under —
the only way `covers` can answer differently for a file than for its parent, since an entry could BE that file's path. A
proptest in `kick_tests.rs` holds the implication over overrides, scores, and both scopes.

Two consequences, taken deliberately. **A tick's prompt GC and its counts patch reach only the dirs it walked**: a
vanished file's row in an uncovered dir waits for the next full pass (which still whole-store GCs it), and an uncovered
dir's cached eligible count goes stale until a full pass refills it. Rows staying is what the forward-only "narrowing
deletes nothing" rule already asked for. **The privacy exclusion stays per image**, out of the dir filter: folding it in
would change which rows a tick may GC, and the retro-delete already empties an excluded folder, so a tick's GC has
nothing to find there.

**The GC data-safety line.** `enrich_and_gc`'s GC is a whole-store set-difference against the walked set — correct for a
full pass (whole index walked), CATASTROPHIC for a scoped walk (it would delete every stored row OUTSIDE the touched
dirs). So the GC target set is a parameter: `GcScope::WholeStore` (the full pass / Fresh sweep, via `enrich_and_gc`) vs
`GcScope::TouchedDirs` (the live tick, via `enrich_and_gc_scoped`), which GCs only rows whose parent dir is one the tick
actually WALKED (its filtered set, never its raw touched set) AND absent from the scoped walk. This makes the live tick
one of the four deletion paths that bypasses the completed-scan edge (`../DETAILS.md` § The GC safety argument). Unlike
the user-explicit ones, the live tick's deletion is INDEX-CONFIRMED: a removal from the live index is a fact about the
tree (like importance's subtree clear), not a scan-state inference, so the complete-tree doctrine isn't violated. A
disconnect/unmount still never deletes: no read pool ⇒ the tick no-ops before any GC. The sibling edge is where the
whole-dir fetch earns its keep — deleting `DSC.jpg` promotes the lone `DSC.cr2` to enrich WHILE scoped-GCing the `.jpg`
row, in one tick.

**Guardrails.** The tick coalesces on a DISTINCT `#live` coordinator key (`live_key`), never the full-pass key — else a
`ScanCompleted` full pass coalescing into a tick's slot would silently downgrade to a scoped tick. Before running, it
SKIPS entirely if a full pass is running for the volume (the full pass covers the touched dirs). Progress honesty: a
tick lights the top-right indicator ONLY when its enrichable subset exceeds `LIVE_INDICATOR_THRESHOLD` (25) AND no full
pass runs (`tick_is_loud`); below that BOTH the progress sink and the terminal guard are suppressed together (a lone
row-clearing terminal on a silent tick would clear a visible full-pass row). A tick does NOT
`mark_deferred_for_importance` on an unscored volume — the full-pass bridge covers that, and marking would trigger a
full re-walk on the next importance bump.

**Logging.** One line per tick, from `run_live_tick_blocking` (the caller's own "enriched N images" line was a strictly
less informative duplicate of it and is gone). A tick that enriched or GC'd anything always logs; a tick that did
neither is the normal case on a machine whose churn is builds, so it rolls up through `IDLE_TICKS`
(`cmdr_fs::log_rollup`) to one line a minute per volume carrying how many ticks it stands for. That keeps the "ticks
fire and nothing qualifies" evidence in an error-report bundle — the answer to "why aren't my photos indexed?" — at
~1/20th the volume. Policy: `docs/tooling/logging.md` § "Keeping the file readable".

## Reclaim space (`reclaim.rs`)

Lowering the importance slider is forward-only: it never deletes rows, so a drive indexed at a broad setting keeps that
coverage after the user narrows the setting (the GC `current` set stays the full walked image set). The reclaim UI
surfaces that leftover coverage and offers to delete it. Like the privacy retro-delete, the prune is USER-EXPLICIT and
derives ONLY from settings state, so it needs no `Completed` edge.

- **One arithmetic source, or the numbers don't add up.**
  `MediaScheduler::stored_coverage(volume_id, mount_root, threshold)` computes THREE quantities from ONE pass so the
  reclaim preview, the prune, and the per-volume `keptCount` can never disagree: `surviving_stored` (stored rows inside
  coverage), `doomed_stored` (outside it — the reclaim "delete N" AND the `keptCount`, the SAME set), and
  `covered_qualifying` (drive-index qualifying images in covered folders — the slider preview's number, a DIFFERENT
  thing: it counts what WOULD be indexed, not what IS). It guarantees `total_stored = surviving_stored + doomed_stored`,
  and reuses the `coverage` cache path for `covered_qualifying` (never a second derivation). In the AUTOMATIC scope it
  returns `None` when importance hasn't scored the volume (importance's scoring makes that transient) — the partition
  can't be computed safely, so the command reports `pending` and the UI hides the reclaim line rather than proposing a
  destructive count off a lower bound. In the narrow scope importance isn't an input, so it partitions against an empty
  score map and stays answerable.
- **The partition rule** (`coverage::partition_stored`, pure) reuses the SAME precedence enrichment does: a stored row
  survives when it's NOT under an excluded folder AND (covered by an "always index" override OR — in the automatic scope
  only — its parent folder scores at or above the threshold). Crucially it keys on score-MAP MEMBERSHIP, not a `>= 0.0`
  on a defaulted score: a folder with NO importance row (floored junk, or scored away since enrichment) is treated as
  below any threshold → doomed, even at threshold 0.0. Spell this out — otherwise a floored folder's rows leak into
  neither bucket. `is_override` / `is_excluded` take the stored (index) path; the wrapper wires the OS-mount mapping
  (`os_join`, identity on a local volume) so override/exclude config (OS-path keyed) and importance (index-keyed) both
  resolve.
- **The writer thread IS the race guarantee.** `prune_below_threshold` computes the doomed set up front and hands it to
  the volume's ONE writer thread (`prune_paths`) as a single serialized delete unit, then `VACUUM`s and drops the vector
  - coverage caches. A concurrent enrichment pass can't interleave mid-batch (both flow through the one writer), and it
    enriches only ABOVE-threshold or override-covered rows — a set disjoint from the doomed (below-threshold) set by
    definition — so a pass running NEW rows during the prune is fine. No snapshot-vs-live dance is needed here (unlike
    the exclusion veto): the doomed set is a concrete path list, not a live predicate.
- **Byte estimate.** `store::sum_bytes_for_paths` streams `media_ocr` + `media_tags` + `media_embedding` once each and
  sums the content bytes of the doomed paths (a set membership test, so no giant `IN (…)` for a 200k doomed set). It's a
  content estimate (excludes FTS-index + page overhead), so it's an honest "about" and a `VACUUM` reclaims at least it.
  The preview's "free about X" and the prune's "Freed X" use the SAME method, so the two numbers agree.

The two commands (`media_index_reclaim_preview`, `media_index_prune_below_threshold`) are in `../DETAILS.md` § The IPC
surface, and the FE surface in its § The frontend surface.

## Testing

Pure: the coalescer (`coalescing_tests.rs`), `tick_is_loud` + `live_debounce_wait` + the distinct live key (`live.rs`),
`prioritized` ordering (`enrich_tests.rs`). Over the fake backend + a synthetic index (`enrich_tests.rs`): the walk, the
enrich pass, deletion-driven GC, the throttle/cancel decision, the edge-triggered `Completed` consumption
(`gc_fires_on_a_completed_edge_never_a_retained_poll`), the hazard it defends against
(`gc_over_an_empty_index_would_delete_everything_which_is_why_it_gates_on_completed`), the scoped walk, the scoped GC vs
the whole-store trap, the sibling re-qualify, the DEFER of a below-threshold folder and the ENRICH of an overridden one
(both keeping deferred rows for GC), and the mid-`analyze` exclusion veto. The master-toggle behavior is pinned by
`a_pass_no_ops_while_disabled_and_enriches_once_enabled` (the disable → no-op → re-enable → enrich cycle) and
`disabling_the_master_toggle_stops_a_running_pass_and_keeps_rows` (real red→green: the running pass stops early, rows
preserved). `kick_tests.rs` runs the tick end to end over a registered read pool (re-enrich-on-modify, below-threshold
defer, exclusion veto, index-confirmed GC, unmount deletes nothing) plus the scheduler retro-delete (prunes a local
folder, skips a volume the folder isn't under, maps a network folder into the volume's index space). `reclaim_tests.rs`
covers the partition + prune arithmetic; `pool/tests.rs` the live width changes; `enrich_memory_tests.rs` the walk's
allocation guards.

The async wire-up (`ready_volumes_with_kind` sweep → `wire_volume` → `run_pass_blocking`) is covered indirectly by the
reactive pieces (bus-edge consumption + coalescer + the enrich core); a full end-to-end async test needs the
process-global index registry and is deferred to the E2E slice.
