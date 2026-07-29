# Importance scheduler — details

What recomputes a volume's folder weights, when, and how cheaply. Read this before any non-trivial work here: editing,
planning, reorganizing, or advising.

## What drives a recompute (plan Decision 4 / 5)

Two triggers, unified through one coalescing core:

- **The lifecycle bus** (`indexing/lifecycle/lifecycle_bus.rs`, mechanism documented in `../../indexing/DETAILS.md`,
  single-source): the scheduler subscribes per volume; a `ScanCompleted` publish ⇒ recompute.
- **The startup registry sweep** (`indexing::ready_volumes_with_kind`): a volume already Fresh at launch never re-fires
  `ScanCompleted` (its retained bus value stays `Pending`), so wiring its subscription alone never recomputes it — the
  common restart case. The sweep wires each ready volume WITH its typed kind (so MTP is excluded and SMB degrades
  correctly), then runs `enqueue_initial_full_pass_if_unscored` per volume to actually score a fresh or recreated store.
- **The registration bus** (`lifecycle_bus::subscribe_registrations`): a volume that registers AFTER the sweep (a share
  mounted mid-session) is wired then. The scheduler subscribes to it BEFORE the sweep, so no volume registering in the
  gap is lost. The registration event carries the typed kind, so the same score/degrade/exclude policy applies.

`PassCoordinator` is the pure, unit-tested coalescing core: it guarantees ONE pass per `volume_id` at a time — a request
arriving mid-pass sets a single re-run flag rather than starting a second pass (so the sweep plus a concurrent
`ScanCompleted` collapse to one pass, then at most one re-run). The recompute itself is full-volume: walk the index tree
through the read pool (`get_read_pool_for`), assemble a `FolderSignals` per folder (`signals::signals_for_dir`), run the
pure scorer, and write every row at a freshly-bumped generation. It runs on a blocking background task (SQLite plus
scoring), never on the IPC thread; a `None` read pool (index not registered) is a no-op. Wiring a volume twice is
harmless: the coordinator collapses the duplicate pass, and a volume is wired from at most two places.

## Generation semantics

`recompute_generation` (a `meta` counter) is stamped ONLY by a full pass (`write_weights` → `apply_full_pass`, in the
same transaction as the table replace). The incremental path (`write_weights_incremental`) deliberately never bumps or
stamps it. So "generation 0" does NOT mean "no weights": a store maintained only by incremental rescores holds hundreds
of thousands of usable weight rows at generation 0, and a schema-recreated store sits at generation 0 until its first
full pass. Consumers that must tell "genuinely unscored" from "scored but no generation" key on the weight-row count,
not the generation (media's `coverage::importance_scored`).

## The initial full pass (the fresh/recreated-store trigger)

**A fresh or recreated store must get a full pass — the invariant.** Because a Fresh-at-launch volume never fires
`ScanCompleted`, the bus subscription alone never scores it. The sweep therefore runs
`enqueue_initial_full_pass_if_unscored` per ready volume: it enqueues a full recompute IFF the store carries no
generation. Gating on "no generation" (not an unconditional kick) is deliberate — importance is expensive, so an
unconditional kick would rescore every volume on every launch; media's kick is unconditional because a redundant
enrichment pass is a cheap staleness no-op. The policies differ on purpose.

**The recreate-ordering trap, and why the decision binds to the write-path open.** The schema delete-and-recreate
happens LAZILY, only inside `ImportanceStore::open` on a WRITE-path open (`open_write_connection`); the read path never
recreates. So on a schema-upgrade launch, the DB is still on the OLD schema at sweep time WITH its old stamped
generation. A naive sweep-time generation READ would read that non-zero generation, decide "already scored", skip the
full pass — and THEN the recreate fires on the first incremental write, wiping the generation and leaving the volume
stuck at "never scored" forever. `store::needs_initial_full_pass` avoids this by opening the store on the WRITE path
FIRST (forcing the recreate), then reading the generation, so the decision reflects the current schema. ❌ Never probe
the generation via the read path before the write-path open.

`should_enqueue_initial_full_pass` is the combined kind + store-state decision, extracted so it's testable without
spawning a recompute (which needs a read pool). The probe itself is a DB open, so it runs on a blocking task, and when
the volume is unscored it hands off to the normal coordinated `spawn_recompute`, so a concurrent `ScanCompleted`
coalesces correctly.

## The walk is O(dirs) in a small constant (`walk.rs`)

The full-recompute walk holds **directories only**, and holds them compactly. Two structures, both in `WalkedFolders`:

- the shared `indexing::store::DirTree` (one name arena plus a 24-byte `(id, parent_id, name slice)` record per
  directory, id-ordered and binary-searched), which every path is reconstructed FROM on demand;
- one `IndexFolder` per folder: a `Copy` record of the folder's tree-row index, its mtime, its `ChildAggregate`, and the
  two subtree flags. Nothing owned, no path.

File rows STREAM (`IndexStore::for_each_file_child_by_parent`), grouped by parent, so one reusable accumulator folds a
directory's distinct extensions, file count, and marker flag and closes at the group boundary. Directory children still
come from the directory set (a `.git`/`.hg`/`.svn` marker is a directory), so `has_direct_marker` folds both the
streamed file children and the sibling directory children. `signals_for_dir` takes the `ChildAggregate` and the mtime,
not child rows and not an `EntryRow`. `has_marker_below` is one upward propagation after the walk (a `.git` deep in a
tree raises its ancestors, plan Decision 3); `under_floored_ancestor` is its downward twin, a second pass over the same
tree that floors every folder below a self-flooring one (`../DETAILS.md` § The floor propagates to descendants). The
floor seed set is a flag per TREE row, so an ancestor check is a binary search plus a byte, never a hash lookup.

**Every part of that shape is load-bearing, and each was measured.** Against the shape that materialized a full
`EntryRow` plus a reconstructed path per folder plus a per-folder `HashSet<String>` of extensions, a full pass over a
real 391,563-folder NAS index costs 84.2 MB instead of 256.4 MB, and over a 611,699-folder root index 105.3 MB instead
of 424.8 MB. **It buys that with ~20% more walk time** (NAS 6.4 s against 5.4 s, root 5.5 s against 4.6 s, warm cache,
five alternating runs): the grouped file query and the extra path pass the floor seed makes both cost real time, and on
a background pass measured in seconds that's the right side of the trade. Both shapes produced identical output over
both volumes — same row set, byte-identical `path` and `signals` for every row (scores drift only with the wall clock,
which moves the recency signal; the same binary run twice 65 s apart differs in as many scores). Measured 2026-07-27
with `cargo run -p index-query --bin importance-measure`, which reports the walk's `phys_footprint` growth. Concretely:

- ❌ Don't reach for `IndexStore::all_directories` here (~112 B plus a heap `String` per row), and don't put an
  `EntryRow` back in `IndexFolder` — the mtime is the only column scoring reads.
- ❌ Don't store the reconstructed path per folder. It is the single biggest per-folder cost, and every consumer
  (`score_folders`, `incremental_rescore`, the eval corpus dump) sees it through `WalkedFolders::for_each`, which
  reconstructs into ONE reused buffer. The incremental path materializes paths only for the touched subset.
- ❌ Don't drop `for_each_file_child_by_parent`'s `ORDER BY parent_id` (the store doc explains what it buys and costs).
  Without the grouping, a distinct-extension set has to stay open per directory for the whole scan, which was ~280 B a
  folder — 70 MB more on the NAS index.
- The `classify` predicates run twice per folder here (once in the walk, once in scoring), which is only affordable
  because they don't allocate on the ASCII path — `../DETAILS.md` § The shared classifiers.

`walk_memory_tests.rs` guards the shape: one test pins the bytes held per folder, the other pins the whole walk's
allocation count (both blow through if anything goes back to being per-folder or per-file).

**Signal assembly agrees with the fixtures by construction.** The categorical signals (denylist, path class, project
marker, hidden) come from the shared `classify.rs` module that BOTH `signals::signals_for_dir` (production) and
`fixtures::signals_for` (tests) call, so the formula's test stand-in and the real assembler can't drift on what a signal
means.

## The measurement entry point

`recompute::recompute_index_to_db` walks a real index read-only, scores, and writes an `importance.db` through a fresh
writer — the full-pass core without the registry, read-pool registry, or async driver. The `importance-measure` dev bin
(`crates/index-query`) wraps it and reports the row count, store size, the phase wall-clock split (walk+score vs
write+flush), and the pass's memory growth. The live full pass logs that same split at info (`run_pass_blocking`,
`target: "importance"`), so a regression in a real recompute's cost shows up in the logs.

## Incremental recompute (plan Decision 5)

A full-volume recompute on `ScanCompleted` stays the default. On top of it, live listing changes drive an **incremental
rescore** of only the touched folders, so a single file edit doesn't re-walk-and-rescore the whole volume.

### The event source (documented choice)

There is no clean in-process per-directory hook in `indexing/`: the reconciler
reports directory changes only via `IndexDirUpdatedEvent` to the frontend, and the writer/aggregation `emit_dir_updated`
sites aren't uniformly volume-aware. So, exactly as the full pass uses `publish_scan_completed` alongside the frontend
`.emit`, there is a per-volume `dir-changed` channel on `indexing/lifecycle/lifecycle_bus.rs` (`publish_dirs_changed`),
published from the **live-change sites where `volume_id` is in scope**: the live event loop (FSEvents batches, under
`indexing/watch/event_loop/`) and the per-navigation verifier. The scan-completion `/`-refresh emits stay on the
full-recompute path (already covered by `ScanCompleted`), so incremental captures exactly the "listing changed while
running" signal. The scheduler subscribes via `subscribe_dirs_changed` and coalesces bursts per volume (accumulating
paths into a pending set, one pass plus at most one re-run — a distinct coordinator key from the full pass so the two
don't block each other).

### Rescoping and the ancestor cap

For each changed path the touched set is the folder itself plus its ancestor chain
(`touched_folder_set`, because a project marker or size/mtime change can raise parents) UNION each changed path's whole
descendant subtree (`is_in_changed_subtree`, because a floor transition flips the whole subtree). The ancestor walk is
capped at `ANCESTOR_WALK_CAP` (32) levels per changed path: a project marker appearing deep in a tree could otherwise
raise every ancestor to the root and rescope half the volume (plan open-question); the downward side is bounded by the
subtree that actually changed. The pass walks the index once, filters to the touched subset, clears each changed
subtree, and re-inserts only its non-floored folders. Spotlight is sampled only for the touched subset (bounded work).

**The downward expansion is only as bounded as the batch is.** The batch arrives on `dir-changed`, which carries the
ORIGIN dirs — those whose OWN listings changed — and NOT their ancestor closure; that contract is canonical in
`indexing/lifecycle/DETAILS.md` § The lifecycle bus, and it is what keeps `is_in_changed_subtree` proportional. Feed the
same code an ancestor and it rescores that ancestor's entire subtree: `incremental_scope_follows_the_changed_dir_not_its_ancestors`
pins both sides on one synthetic volume (5 rows from the origin, all 423 from the closure).

**Clear and insert must agree, and they do because both read the SAME `changed_paths` slice.**
`write_weights_incremental` clears each entry's subtree; the row set is `touched ∪ in_changed_subtree` over the same
entries, so the cleared region is always a SUBSET of what gets re-inserted (the touched ancestors add rows outside the
cleared region, which is harmless — the insert is an upsert on the folded PK). ❌ Never narrow `is_in_changed_subtree`
(or widen the clear list) independently: a clear wider than the insert deletes rows nothing re-adds, and the weights
vanish silently until the next full pass.

**A floor transition reaches the renamed folder through its PARENT.** The live pipeline reports the parent as the
origin, never the renamed directory itself (its new name is just an entry in the parent's listing), so the parent's
downward expansion is the ONLY thing that revisits a folder that just became — or stopped being — a `node_modules`.
`a_floor_transition_propagates_from_the_parent_origin_without_widening` pins the transition AND its containment (a
sibling project keeps its untouched full-pass row).

Both predicates run once per WALKED folder (the filter sees the whole volume, not just the touched subset), so they
inherit the walk's no-per-folder-allocation discipline above: `is_in_changed_subtree` does `strip_prefix` plus a
separator check rather than building a `{changed}/` needle, and the touched set is a `HashSet<String>` probed by `&str`.
❌ Don't reintroduce a `format!` in either — at ~161 k folders every 60 s it was a top allocation site
(`docs/notes/memory-runaway-rust-heap-2026-07-25.md`). The separator check is load-bearing for correctness too, not only
cost: a bare prefix test matches `/a/bc` against changed `/a/b` and drags a sibling's whole subtree into every rescore
(`changed_subtree_matches_on_separator_boundaries`).

### Transition semantics

Clearing each changed subtree and then re-inserting handles every floor transition in one
model: a folder RENAMED AWAY or DELETED has its old-path row cleared and is never re-inserted (it's not in the current
walk); a folder that BECAME floored (renamed to `node_modules`, say) and its now-under-floored descendants are cleared
then skipped on re-insert, so no stale positive-score row survives under a fresh `node_modules`; a folder that STOPPED
being floored and its descendants are cleared (they had no row anyway) then inserted because they now score. Both floor
directions are TDD'd (`incremental_deletes_rows_that_become_floored`,
`incremental_scores_rows_that_stop_being_floored`) — the likeliest bug site. A full pass replaces the whole table
instead, which purges any folder that floored or vanished since the last pass. The clear's range math is a property of
the folded PK: `../store/DETAILS.md` § The folded-key primary key.

### Generation semantics on the incremental path

An incremental pass writes its rows at the CURRENT generation and does NOT bump it, so every
untouched folder keeps its as-of marker and the volume doesn't turn wholesale-stale after a one-file change. Only a full
pass advances the generation.

### The incremental never escalates on `/`

`/` reaches a batch whenever something changes directly in the root directory (on macOS that is routine churn), and it
is not a signal that the whole volume changed. `sanitize_incremental_batch` drops `/` (and empty strings) at the
incremental boundary before `touched_folder_set` / `write_weights_incremental` see it; a batch that was only `/` is a
no-op. Full recomputes are `ScanCompleted`-driven only — the incremental path never calls `run_pass_blocking`.
**Gotcha/Why:** treating `/` as a full-refresh sentinel
(escalate to a whole-volume rewrite) meant that because the root volume live-watches `/`, where macOS FSEvent churn is
near-continuous, `/` arrived in almost every batch and full recomputes ran back-to-back forever — pegging a core and
starving the index-DB WAL checkpoint (its `wal_checkpoint(TRUNCATE)` kept losing to importance's long read), which
surfaced as `stall_probe::sqlite_busy` WARN bursts. ❌ Don't reintroduce a `/`→full-pass escalation.

### Throttle (leading plus trailing)

Each incremental still walks the whole index (O(dirs)) before rescoping to the
touched subset; that walk dominates each incremental's cost, because the targeted write is index-served against the
BINARY `path_folded` PK (sub-millisecond even on a 166k-row store). So `spawn_incremental` throttles per volume: the
first pass of a burst runs immediately (leading edge), and under sustained change it runs at most once per
`INCREMENTAL_THROTTLE_WINDOW` (60 s — a throttle, NOT a debounce that never fires under constant change). Coalesced
requests accumulate during the wait and the next drain folds them all in. Importance is a background signal, so the lag
is invisible to consumers. **Ideal follow-up (deferred):** a targeted walk reading only the changed subtree's directory
plus child rows would make each incremental ~O(touched) and remove the need to throttle; it's deferred because computing
`has_marker_below` / `under_floored_ancestor` correctly across the subtree boundary (an ancestor outside the subtree can
floor it) is a real correctness surface.

### The dir-changed `watch` can drop a batch under bursts (accepted)

The incremental trigger rides the per-volume `dir-changed` `watch` channel. A `watch` is last-value-wins: if two
`publish_dirs_changed` batches land between the scheduler's `borrow_and_update` reads, the consumer sees only the later
batch's paths, and the earlier batch's paths can be dropped. This is **acceptable and by design**: importance is
advisory, disposable derived data, and the next full recompute (on the next `ScanCompleted`) heals any folder a dropped
incremental batch missed. We don't add an unbounded queue to make incremental lossless; the full pass is the backstop.

## Multi-volume, kind-aware scoring

The scheduler scores **any** background-scored volume, not just the local `root`. The typed volume kind
(`indexing::IndexVolumeKind`, retained on the registry instance) decides the policy at a single seam
(`ScoringPolicy::for_kind`), never by inspecting the volume-id string (`no-string-matching`):

- **Local** — background-scored; both optional signals available (visits plus Spotlight where the OS has it).
- **SMB** — background-scored, but **Spotlight is unavailable** (no `kMDItemLastUsedDate` over a share), so
  `last_used`'s weight redistributes onto the listing signals (the scorer's redistribution makes this honest: a missing
  signal spreads, never fabricates). Visits still apply — they come from Cmdr navigation, not the mount.
- **MTP** — an explicit **exclusion**, not an accident of gating: a phone or camera is on-demand only, never
  background-scored. The scheduler skips it at every entry point (sweep, registration, bus subscription), and
  `record_visit` skips it too.

`signal_availability(kind)` single-sources the kind→availability mask so a read consumer's `explain` redistributes
exactly as the recompute that wrote the weights did; it returns `None` for a kind that isn't background-scored, and
`is_background_scored(kind)` is the same policy as a bool for `record_visit`.

**Network-mount discipline.** The scheduler never issues a filesystem syscall against an SMB/MTP mount — it reads only
the local index DB. Spotlight sampling is gated on the mask (`last_used_available`), so it never runs for SMB (which
would have meant `MDItem` queries against the mount).

## Testing

The scheduler tests run over synthetic indexes with no FFI and no registry, split by concern:

- `coalescing_tests.rs` — `PassCoordinator`'s one-pass-plus-one-re-run contract and the throttle spacing.
- `multi_volume_tests.rs` — `ScoringPolicy` scores Local/SMB and excludes MTP; SMB's recompute degrades Spotlight and
  redistributes (never fabricates); the initial-full-pass probe's write-path ordering; the offline read returns stored
  weights at the right as-of generation after the index DB is deleted; a multi-volume recompute scores each volume into
  its own store; both floor transitions; the derive-on-read floor invariant over every walked folder.
- `recompute_tests.rs` — scoring and writing over a synthetic walk; the O(dirs) walk's `ChildAggregate` against a
  whole-tree oracle.
- `incremental_tests.rs` — rescoping, the ancestor cap, and the `/` sanitization.
- `walk_memory_tests.rs` — the walk's per-folder byte and allocation ceilings.
- `test_support.rs` — the shared synthetic-index builders.

The registration bus's late-volume delivery is covered in `indexing/lifecycle/lifecycle_bus.rs`.
