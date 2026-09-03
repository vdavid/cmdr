# Importance scheduler — details

What recomputes a volume's folder weights, when, and how cheaply. Read this before any non-trivial work here: editing,
planning, reorganizing, or advising.

## What drives a recompute

Two triggers, unified through one coalescing core:

- **The lifecycle bus** (`indexing/lifecycle/lifecycle_bus.rs`, mechanism documented in `../../indexing/DETAILS.md`,
  single-source): the scheduler subscribes per volume; a `ScanCompleted` publish ⇒ recompute.
- **The startup registry sweep** (`indexing::ready_volumes_with_kind`): a volume already Fresh at launch never re-fires
  `ScanCompleted` (its retained bus value stays `Pending`), so wiring its subscription alone never recomputes it — the
  common restart case. The sweep wires each ready volume WITH its typed kind (so MTP is excluded and SMB degrades
  correctly), then runs `enqueue_full_pass_if_needed` per volume to actually score a fresh or recreated store.
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
not the generation (`ImportanceIndex::is_scored`).

## The initial full pass (the fresh/recreated-store trigger)

**A fresh, recreated, or policy-superseded store must get a full pass — the invariant.** Because a Fresh-at-launch
volume never fires `ScanCompleted`, the bus subscription alone never scores it. The sweep therefore runs
`enqueue_full_pass_if_needed` per ready volume: it enqueues a full recompute IFF the store carries no generation, or its
`SCORING_POLICY_KEY` doesn't match this build's classifiers. That second reason is the ONLY thing that ever re-scores
rows a scored volume already holds; see `../store/DETAILS.md` § The scoring-policy stamp. Gating on those two conditions
rather than kicking unconditionally is deliberate — importance is expensive, so an unconditional kick would rescore
every volume on every launch; media's kick is unconditional because a redundant enrichment pass is a cheap staleness
no-op. The policies differ on purpose.

**The recreate-ordering trap, and why the decision binds to the write-path open.** The schema delete-and-recreate
happens LAZILY, only inside `ImportanceStore::open` on a WRITE-path open (`open_write_connection`); the read path never
recreates. So on a schema-upgrade launch, the DB is still on the OLD schema at sweep time WITH its old stamped
generation. A naive sweep-time generation READ would read that non-zero generation, decide "already scored", skip the
full pass — and THEN the recreate fires on the first incremental write, wiping the generation and leaving the volume
stuck at "never scored" forever. `store::needs_full_pass` avoids this by opening the store on the WRITE path
FIRST (forcing the recreate), then reading the generation, so the decision reflects the current schema. ❌ Never probe
the generation via the read path before the write-path open.

`should_enqueue_full_pass` is the combined kind + store-state decision, extracted so it's testable without
spawning a recompute (which needs a read pool). The probe itself is a DB open, so it runs on a blocking task, and when a
pass is due it hands off to the normal coordinated `spawn_recompute`, so a concurrent `ScanCompleted` coalesces
correctly.

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
tree raises its ancestors); `under_floored_ancestor` is its downward twin, a second pass over the same tree that floors
every folder below a self-flooring one (`../DETAILS.md` § The floor propagates to descendants). The floor seed set is a
flag per TREE row, so an ancestor check is a binary search plus a byte, never a hash lookup.

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

## A pass can't be stopped

Every other long walk in the crate runs under a `CancellationToken` rooted at the volume
(`../../indexing/host/DETAILS.md` § Cancellation). An importance pass runs under nothing: no token reaches
`run_pass_blocking` or the walk below it, and the scheduler registers no
`indexing::resources::subsystem_stop::register_subsystem_stop_hook`. Two consequences worth knowing before you assume
otherwise:

- **`stop_all_indexing` doesn't reach it.** That's both the memory watchdog's emergency stop and the shutdown path, so a
  recompute that's running when either fires walks the whole index to the end anyway.
- **Nothing observes a stop request**, so there's no `Cancelled` outcome to handle and no partial-pass state to reason
  about. A pass either completes and stamps its generation, or fails.

**Why it hasn't hurt.** The full walk is O(dirs) in a small constant: 5.5–6.4 s over real 391k / 611k-folder indexes
(measured 2026-07-29, § "The scoped walk"), and an incremental is microseconds. Seconds of unstoppable work inside a 16
GB emergency stop is survivable, where a scan's minutes wouldn't be.

**Closing it** (the `TODO(importance)` sits on `recompute_folders`, the loop that would poll): thread a child of the
volume's token in from whoever starts the pass (it is handed down, never looked up by id — `indexing/host/DETAILS.md` §
Cancellation), and register a stop hook. ❌ Don't introduce a second primitive (an `AtomicBool`, a `Notify`): the
one-token tree is what makes stopping a volume stop everything under it at once. The hook must be cheap and
non-blocking; it runs INLINE in the stop path.

## The measurement entry point

`recompute::recompute_index_to_db` walks a real index read-only, scores, and writes an `importance.db` through a fresh
writer — the full-pass core without the registry, read-pool registry, or async driver. The `importance-measure` dev bin
(`crates/index-query`) wraps it and reports the row count, store size, the phase wall-clock split (walk+score vs
write+flush), and the pass's memory growth. The live full pass logs that same split at info (`run_pass_blocking`,
`target: "importance"`), so a regression in a real recompute's cost shows up in the logs.

## Incremental recompute

A full-volume recompute on `ScanCompleted` stays the default. On top of it, live listing changes drive an **incremental
rescore** of only the touched folders, so a single file edit doesn't re-walk-and-rescore the whole volume.

### The event source (documented choice)

There is no clean in-process per-directory hook in `indexing/`: the reconciler reports directory changes only via
`IndexEvent::DirsUpdated` to the frontend, and the writer/aggregation `emit_dir_updated` sites aren't uniformly
volume-aware. So, exactly as the full pass uses `publish_scan_completed` alongside the frontend `.emit`, there is a
per-volume `dir-changed` channel on `indexing/lifecycle/lifecycle_bus.rs` (`publish_dirs_changed`), published from the
**live-change sites where `volume_id` is in scope**: the live event loop (FSEvents batches, under
`indexing/watch/event_loop/`) and the per-navigation verifier. The scan-completion `/`-refresh emits stay on the
full-recompute path (already covered by `ScanCompleted`), so incremental captures exactly the "listing changed while
running" signal. The scheduler subscribes via `subscribe_dirs_changed` and coalesces bursts per volume (accumulating
paths into a pending set, one pass plus at most one re-run — a distinct coordinator key from the full pass so the two
don't block each other).

### Rescoping and the ancestor cap

For each changed path the touched set is the folder itself plus its ancestor chain (`touched_folder_set`, because a
project marker or size/mtime change can raise parents) UNION each changed path's whole descendant subtree
(`is_in_changed_subtree`, because a floor transition flips the whole subtree). The ancestor walk is capped at
`ANCESTOR_WALK_CAP` (32) levels per changed path: a project marker appearing deep in a tree could otherwise raise every
ancestor to the root and rescope half the volume (plan open-question); the downward side is bounded by the subtree that
actually changed. The pass walks the index once, filters to the touched subset, clears each changed subtree, and
re-inserts only its non-floored folders. Spotlight is sampled only for the touched subset (bounded work).

**The downward expansion is only as bounded as the batch is.** The batch arrives on `dir-changed`, which carries the
ORIGIN dirs — those whose OWN listings changed — and NOT their ancestor closure; that contract is canonical in
`indexing/lifecycle/DETAILS.md` § The lifecycle bus, and it is what keeps `is_in_changed_subtree` proportional. Feed the
same code an ancestor and it rescores that ancestor's entire subtree:
`incremental_scope_follows_the_changed_dir_not_its_ancestors` pins both sides on one synthetic volume (5 rows from the
origin, all 423 from the closure).

**Clear and insert must agree, and they do because both read the SAME `changed_paths` slice** — the de-duplicated one,
since `dedupe_nested_origins` runs before either. `write_weights_incremental` clears each entry's subtree; the row set
is `in_changed_subtree` (plus, on the full-walk path, `touched`) over the same entries, so the cleared region is always
a SUBSET of what gets re-inserted (the touched ancestors add rows outside the cleared region, which is harmless — the
insert is an upsert on the folded PK). ❌ Never narrow `is_in_changed_subtree` (or widen the clear list) independently:
a clear wider than the insert deletes rows nothing re-adds, and the weights vanish silently until the next full pass.
De-duplication is safe here precisely because it changes neither side: `subtree(P/x) ⊆ subtree(P)`.

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

Clearing each changed subtree and then re-inserting handles every floor transition in one model: a folder RENAMED AWAY
or DELETED has its old-path row cleared and is never re-inserted (it's not in the current walk); a folder that BECAME
floored (renamed to `node_modules`, say) and its now-under-floored descendants are cleared then skipped on re-insert, so
no stale positive-score row survives under a fresh `node_modules`; a folder that STOPPED being floored and its
descendants are cleared (they had no row anyway) then inserted because they now score. Both floor directions are TDD'd
(`incremental_deletes_rows_that_become_floored`, `incremental_scores_rows_that_stop_being_floored`) — the likeliest bug
site. A full pass replaces the whole table instead, which purges any folder that floored or vanished since the last
pass. The clear's range math is a property of the folded PK: `../store/DETAILS.md` § The folded-key primary key.

### Only what moved is written

A pass RESCORES its subset and then writes only the rows that actually changed. `apply_incremental` (`../writer.rs`)
reads each rescored subtree over the same folded-PK range the writes key on, gives every stored row exactly one
`StoredRowFate`, and acts on that: `Keep` (leave it entirely alone), `Rewrite` (the insert overwrites it), `Remove` (the
pass no longer scores it — deleted, renamed away, or newly floored). Rows OUTSIDE every rescored subtree — a full-walk
pass's capped ancestor chain — get a PK probe each, bounded by `ANCESTOR_WALK_CAP` × the origin count. ❌ A row there is
never REMOVED: only a rescored subtree is cleared.

**Decision/Why the equality key is the SIGNALS blob, never the score.** A score is a function of the signals AND
`now_secs`, and `scorer::recency` decays continuously, so every score moves a little every pass even when nothing about
the folder changed. Measured 2026-08-04 on the real 160,719-row root store, comparing what the app had written against a
fresh recompute over the same index snapshot: of the 51,081 rows a `$HOME`-origin pass rewrote, **99.88% carried a
byte-identical signals blob and 0.03% an identical score**. A score diff would have skipped 17 rows in 51,081 and left
the treadmill running. `FolderSignals` carries no clock (raw `mtime_secs`, counts, flags), which is what makes it a
sound identity. Full evidence: `docs/notes/importance-treadmill-2026-08-04.md`.

**Decision/Why keeping a row keeps its old score.** A `Keep` row stays at the `now_secs` it was last written at. That is
the SAME bounded staleness `RescoreScope::ChangedSubtreesOnly` already accepts for an origin's ancestors (above), and it
makes the store MORE uniform: every folder now ages between full passes instead of only the churny ones being
re-decayed. It also means a `Weights` change reaches a `Keep` row only at the next full pass — which was already true
for the 99.9% of folders no incremental touches, so it widens nothing new.

Measured on that same real store, over the 51,081-row `$HOME` subtree: the range READ costs **10 ms** where the old
subtree-DELETE-plus-reinsert cost **550–620 ms**.

**Both numbers are logged, on purpose.** `IncrementalReport` carries `considered` (folders rescored — the batch's true
cost, set by how wide the changed subtrees are) and `written`. `written` alone would read as "this pass was free" while
the batch still drags in most of the volume, which is the cost that remains. ❌ Don't drop `considered` from the log
line; it is what names a too-wide batch.

The ONE pass that doesn't get its own line is `considered == 0`: its whole batch was filtered out by
`sanitize_incremental_batch` before the read pool, so it walked nothing and wrote nothing. On a machine running cargo
that's nearly every pass (measured 2026-08-04: 908 of 972 rescore lines in half an hour read
`updated 0 folders (of 0 rescored)`, 38% of the entire log file). Those roll up through `EMPTY_RESCORES` to one line a
minute per volume. Any pass with `considered > 0` still logs both numbers every time, so the too-wide-batch signal is
untouched. Policy: `docs/tooling/logging.md` § "Keeping the file readable".

### Generation semantics on the incremental path

An incremental pass writes its rows at the CURRENT generation and does NOT bump it, so every untouched folder keeps its
as-of marker and the volume doesn't turn wholesale-stale after a one-file change. Only a full pass advances the
generation.

### The batch gate (the idle floor)

`sanitize_incremental_batch` runs FIRST in `run_incremental_blocking`, before the read pool opens and before the walk,
and an empty result returns `Ok(0)` immediately. It drops three kinds of path:

- the bare root `/` and empty strings (below);
- anything **floored by path** — `target`, `node_modules`, `.git`, `Library/Caches`, any dot-directory. A floored folder
  gets NO row and floors its whole subtree, so a batch of only these would pay a full O(dirs) walk to write zero rows
  and clear subtrees that hold none. Skipping it is exactly a no-op.

That second rule is the idle floor. A boot volume is never silent (builds, caches, agent scratch dirs write
continuously), so without it every 60-second window found a non-empty batch and ran a full walk plus — through
`notify_recompute_completed` — a reload of every weight in `search::volumes` while the old map was still live. With it,
a machine whose only activity is machine output does no importance work at all.

The gate calls `classify::floors_by_path`, the SAME predicate the writer applies when deciding to skip a row, so the two
can't disagree about what scores. **Decision/Why the accepted lossiness:** filtering a floored path also drops the
unfloored ancestors `touched_folder_set` would have pulled in from it, and the one signal that can move for such an
ancestor is `has_marker_below` (a project marker appearing inside a floored subtree — `propagate_marker_to_ancestors`
doesn't stop at a floor). We accept it: a marker buried in machine output is the weakest reason to raise a folder,
importance is advisory, and the next full pass heals it. The rationale and the test
(`a_batch_of_only_floored_churn_is_dropped_whole`) live with the function.

### The incremental never escalates on `/`

`/` reaches a batch whenever something changes directly in the root directory (on macOS that is routine churn), and it
is not a signal that the whole volume changed. `sanitize_incremental_batch` drops `/` (and empty strings) at the
incremental boundary before `touched_folder_set` / `write_weights_incremental` see it; a batch that was only `/` is a
no-op. Full recomputes are `ScanCompleted`-driven only — the incremental path never calls `run_pass_blocking`.
**Gotcha/Why:** treating `/` as a full-refresh sentinel (escalate to a whole-volume rewrite) meant that because the root
volume live-watches `/`, where macOS FSEvent churn is near-continuous, `/` arrived in almost every batch and full
recomputes ran back-to-back forever — pegging a core and starving the index-DB WAL checkpoint (its
`wal_checkpoint(TRUNCATE)` kept losing to importance's long read), which surfaced as `stall_probe::sqlite_busy` WARN
bursts. ❌ Don't reintroduce a `/`→full-pass escalation.

### The scoped walk (`scoped_walk.rs`)

An incremental reads only the CHANGED SUBTREES out of the index, not the whole volume. `walk_for_incremental` is the one
seam: it tries `try_scoped_walk` and falls back to `walk_index_folders` when the scoped one can't stand in, so the full
walk stays both the fallback path and the differential oracle. Measured 2026-07-29 with `importance-diff` over real
indexes: **median 98 µs per origin** on a 391,563-folder NAS index and **165 µs** on a 611,699-folder root index,
against 6.4 s / 5.5 s for a full walk.

**Why scoping is exact, not approximate.** The two whole-tree propagations decompose:

- **`under_floored_ancestor` needs no walk.** A folder's ancestors are exactly the prefixes of its absolute path, and
  each one's name is that prefix's last component, so `classify::under_floored_ancestor` (pure path math, shared with
  `floors_by_path`) sees a flooring ancestor far above the subtree root. It matches `propagate_floor_to_descendants` by
  construction: same seeds, same root boundary, same names.
- **`has_marker_below` is exact inside a subtree**, which is downward-closed, so `propagate_marker_to_ancestors` runs
  unchanged over the scoped tree. Ancestor-chain rows are in the tree (paths reconstruct through them) but are NOT
  folders, so the climb simply stops being recorded once it leaves the subtree.
- **The cross-boundary case has an exact test.** For a strict ancestor `A` of origin `C`,
  `has_marker_below(A) = markerOutside(A) OR M(C)` where `M(C) = has_direct_marker(C) OR has_marker_below(C)` — and
  `M(C)` is precisely the `has_project_marker` the store persists for `C`. `markerOutside` can't move from inside the
  subtree, so `A` can only move when `M(C)` does. `load_previous_markers` reads the stored side, the scoped walk
  produces the fresh side, and a flip (or an origin with no stored row to compare) takes the full walk this pass. One
  carve-out: an origin with NEITHER a stored row nor an index row never scored and still doesn't, so it rules its own
  ancestors out without a fallback. `only_a_marker_transition_costs_the_full_walk` pins both halves.

Nothing else can move for such an ancestor: a deep write changes neither its mtime nor its direct-child aggregate (both
are direct-listing facts, and a listing change makes the folder an origin itself), and a rename above it reaches it
through that rename's parent origin, which puts it INSIDE a subtree rather than above one.

**Decision/Why the accepted lossiness.** On the scoped path, `RescoreScope::ChangedSubtreesOnly` means strict ancestors
of an origin no longer get a rewritten row every pass. Two things go with that, both accepted:

- Their **recency term** stops being recomputed against a fresh `now_secs`. Every other folder already ages that way
  between full passes, so this makes the store MORE uniform: previously an ancestor of a churny directory had its score
  decayed every 60 s while its siblings kept an older, higher `now_secs`.
- A **visit** recorded since the row was written folds in at the next pass that covers the folder. Visits already lag a
  pass.

`has_project_marker`, the one signal that genuinely propagates upward, is covered by the guard above, so it is never
silently wrong. This extends the batch gate's floor-filter decision (below) rather than contradicting it: both accept a
bounded staleness in a derived, advisory signal that the next full pass heals.

**Reading the subtree.** Each surviving origin is read SEPARATELY, into one shared scoped `DirTree`. ❌ Don't collapse a
batch into a single walk over the origins' common ancestor: unrelated origins share only `/`, so that re-walks the
volume. Per origin: resolve the path to an entry id by descending `resolve_component` from the root (indexed point
queries, O(depth)); read the ancestor chain upward so paths reconstruct from the index's OWN names (an origin can be
spelled in another case — see the known gap below); then descend level by level with
`IndexStore::for_each_child_directory_of` and fold the file children with `for_each_child_file_of`, which keeps
`for_each_file_child_by_parent`'s `ORDER BY parent_id` group contract so ONE reusable extension accumulator serves the
whole scan. Floored subtrees ARE descended: a marker inside a `node_modules` still raises the folders above it, so
pruning them would make the two walks disagree.

**❌ Don't add a `folders.is_empty()` early return to `run_incremental_blocking`.** A batch whose origins were all
deleted between the publish and the pass produces an EMPTY scoped walk, and that batch is exactly the one whose rows
must be CLEARED. An early return there leaves every deleted folder's weight behind until the next full pass
(`an_origin_deleted_between_publish_and_pass_loses_its_rows`). For the same reason a pass is announced
(`notify_recompute_completed`) whether or not it wrote a row: it cleared either way.

**Bounded, by named constants rather than optimism.** `SCOPED_WALK_MAX_ORIGINS` (64) caps the batch width;
`SCOPED_WALK_MAX_DIRS` (20,000) caps one origin's subtree and is checked twice — against the index's own
`recursive_dir_count` before anything is read (`plan_incremental_batch`), and again as a running count during the
descent, which catches a missing or stale `dir_stats` row. Origins nested under a DESCENDED origin are dropped
(`dedupe_nested_origins`), which also spares the common brand-new-folder case from the marker fallback: creating a
folder changes its PARENT's listing, so the parent is an origin too and it has a stored row.

### An over-budget origin is DEMOTED, not descended

**Decision/Why an origin bigger than the budget is rescored alone.** `origin_dir` is the PARENT of the changed file, so
any file written directly in `~` makes `$HOME` an origin — and `$HOME` covers 574,006 of the root volume's 694,963
directories (83%, read straight off `dir_stats`, 2026-08-04). A change to a file sitting directly inside an origin can
move that origin's own signals and propagate UP its ancestors, but it cannot move any DESCENDANT's, so the old descent
read 574,006 rows to discover that 574,005 of them were unchanged. An over-budget origin is now rescored ALONE:
`read_origin_alone` reads the directory itself plus one level of child directories (a marker is often a directory), and
nothing below.

**The load-bearing half is that a demoted origin is NOT in the clear list.** `BatchPlan::lists_for` hands back two
slices: `cleared` (subtrees the writer clears and re-inserts) and `demoted` (origins rescored alone, added to the
rescore subset through `rescore_subset`'s `touched` set). ❌ Never fold them together — a demoted origin in the clear
list deletes its whole subtree's weights and re-inserts one row. Its own row is still written, through
`apply_incremental`'s outside-the-subtree PK probe, which never REMOVES.

**Decision/Why a demoted origin's `has_marker_below` comes from its stored row.** Nothing below it was read, so the
propagation can't produce the flag, and `carry_marker_below_for_demoted` seeds the stored `has_project_marker` instead.
Three cases, and how each resolves:

- **A direct child that IS a marker appears.** `read_origin_alone` reads the direct children, so `has_direct_marker`
  moves, the origin's marker presence flips against its stored value, and the guard takes the FULL walk — exactly what
  an unbounded origin does. Pinned by `a_marker_appearing_in_a_demoted_origin_takes_the_full_walk`.
- **A direct child DIRECTORY's own marker-below changes.** That child is its own origin, in this batch or a later one
  (the `dir-changed` contract carries the dirs whose listings changed), where it is descended or demoted in its own
  right. De-duplication is what makes this hold, which is why a demoted origin absorbs nothing (below).
- **The origin has no stored row yet.** `FullWalkReason::MarkerPresenceUnknown` still fires: the guard runs over every
  planned origin, demoted or not. Pinned by `a_demoted_origin_with_no_stored_row_takes_the_full_walk`.

**The accepted lossiness, and it is one-directional.** The seed can only ADD marker presence, so the last marker
disappearing from deep inside a demoted origin's subtree leaves the origin (and its ancestors) reading project-adjacent
until the next full pass. That is the same bounded staleness the batch gate already accepts for a marker inside a
floored subtree, in the same advisory, derived signal — and the marker-APPEARS direction, the one that changes a
ranking, stays exact.

**A demoted origin absorbs nothing during de-duplication.** `dedupe_nested_origins` takes the demoted set and only lets
a DESCENDED origin swallow the origins nested under it. Without that, `$HOME` (an origin on essentially every batch)
would absorb every real change the same batch carried and the pass would silently drop it —
`a_change_under_a_demoted_origin_still_rescores_its_own_subtree` fails loudly if the demoted set is ignored.

**Measured 2026-08-04, real `index-root.db` (694,963 dirs) against a copy of the real `importance-root.db`,** one
`$HOME`-origin pass end to end:

- **Full walk plus `WithAncestors`** (what this origin used to take): walk 4.31 s, whole pass 5.25 s, 51,082 folders
  rescored, 61 rows written, a 61-upsert delta.
- **Demoted**: plan-plus-walk 0.76–1.03 ms, whole pass 1.6–2.1 ms, 1 folder rescored, 1 row written, a 1-upsert delta.

The differential agrees: over the five widest real origins, four demote and all five produce byte-identical rows to the
full walk's (`importance-diff`, 8 rows each side, zero disagreements); over 385 sampled origins none demotes, none falls
back, and none disagrees. Before the bound, those same four cost an abandoned descent probe of median 30 ms (max 305 ms)
and then a full walk each.

**The differential.** `differential.rs` runs both walks over one real index and compares the rows each WOULD write for
the same subtree — path, score, and signal blob — at a fixed `now_secs` (the recency signal moves scores with the wall
clock). The `importance-diff` dev bin (`crates/index-query`) drives it and reports counts and timings only, never a
folder name. Verified 2026-07-29: zero disagreements over 964 sampled origins (764 of the 611,699-folder root index, 200
of the 391,563-folder NAS index; 9,384 rows compared). Every transition scenario in `incremental_transition_tests.rs`
runs twice, once per walk, and asserts the two stores come out identical.

**Known gap, pre-existing:** the subtree clear folds the path (`path_folded` is the PK) while `is_in_changed_subtree` /
`touched_folder_set` compare bytes, so an origin spelled in a different case than the index holds it clears rows that
nothing re-adds. BOTH walks lose the row identically, so it is pinned as a differential-only scenario
(`an_origin_spelled_in_another_case_behaves_the_same_under_both_walks`) rather than asserted as desirable.

The fix, when someone takes it: canonicalize every origin against the index BEFORE either walk (resolve it, then rebuild
the path from the index's own names), which is what `collect_ancestor_chain` already does internally for the rows it
writes, at one resolve per origin. It has to be its own change, because it moves the FULL walk's behaviour too and the
full walk is the differential's oracle.

### Throttle (leading plus trailing)

`spawn_incremental` throttles per volume: the first pass of a burst runs immediately (leading edge), and under sustained
change it runs at most once per `INCREMENTAL_THROTTLE_WINDOW` (60 s — a throttle, NOT a debounce that never fires under
constant change). Coalesced requests accumulate during the wait and the next drain folds them all in. Importance is a
background signal, so the lag is invisible to consumers.

The window originally paid for the full walk. Neither of the two costs it used to pace is large any more: the scoped
walk made a typical pass microseconds, and the weight reload it triggers through `notify_recompute_completed` is now a
DELTA rather than an O(all weights) rebuild (`../read/DETAILS.md` § The reload contract). Measured 2026-08-03 over the
real `importance-root.db` (160,302 scored folders) with `search::bench::bench_weight_reload`, release build, warm page
cache: the full reload costs **72–74 ms**, while patching a typical 8-upsert / 1-removal delta costs **333 ns** with no
search in flight and **72 µs** while one holds the map (`Arc::make_mut` clones it there). Three to five orders of
magnitude.

**Recommendation, David's call, not taken here:** the reason to keep the window at 60 s is gone, so it can come down.
What's left to pace is the store write itself plus its `wal_checkpoint(TRUNCATE)`, which is real but far smaller than a
walk. ❌ Don't lower it as a side effect of an unrelated change; it's a behavior change to make deliberately, with the
FSEvent-firehose case (a boot volume is never truly idle) in mind.

### The dir-changed `watch` can drop a batch under bursts (accepted)

The incremental trigger rides the per-volume `dir-changed` `watch` channel. A `watch` is last-value-wins: if two
`publish_dirs_changed` batches land between the scheduler's `borrow_and_update` reads, the consumer sees only the later
batch's paths, and the earlier batch's paths can be dropped. This is **acceptable and by design**: importance is
advisory, disposable derived data, and the next full recompute (on the next `ScanCompleted`) heals any folder a dropped
incremental batch missed. We don't add an unbounded queue to make incremental lossless; the full pass is the backstop.

**Don't "fix" this one by analogy with the OUTPUT channel.** The recompute-completed channel a pass publishes on IS
lossless-or-loudly-lossy (a `broadcast`, `../read/DETAILS.md` § The reload contract), and the two decisions are opposite
on purpose. A dropped `dir-changed` batch costs a folder a rescore, which the next full pass heals and nothing depends
on. A dropped weight DELTA would leave a consumer's cached map disagreeing with the store with nothing able to detect
it. Losing an input is a staleness; losing an output is a corruption.

## The hourly full refresh

`FULL_REFRESH_INTERVAL` (1 hour, `wiring.rs`) runs a full recompute per volume on a plain timer, on top of
`ScanCompleted` and the startup sweep. It exists because the incremental path accepts two bounded stalenesses on
purpose, and nothing else corrects them:

- A row whose SIGNALS didn't move isn't rewritten (`../writer.rs`, `fate_of_stored_row`), so its score keeps the
  `now_secs` it was last written at and its recency decay pauses.
- A demoted origin seeds `has_marker_below` from its stored row, which can only ADD marker presence, so the last marker
  LEAVING a big subtree reads stale until a full pass (see § "An over-budget origin is DEMOTED, not descended").

Both are ranking nuances rather than correctness bugs, which is why an hour is enough and why nothing more elaborate (a
dirty set, a watermark) is warranted.

**It is affordable, measured rather than assumed.** Against the real indexes on 2026-08-04 (release build,
`index-query --bin importance-measure`):

- boot volume, 694,963 dirs / 7.09 M entries: **8.29 s wall, 5.8 s CPU**, +166 MB transient `phys_footprint`, 62 MB out
- NAS (`naspi`), 71,365 dirs / 2.65 M entries: **2.39 s wall, 1.6 s CPU**, +81 MB transient, 19 MB out

So hourly across both is ~7.4 s CPU per hour, about **0.2% of one core**. The walk reads the per-volume INDEX DB, never
the volume itself, so a network volume costs no SMB traffic and does not need to be reachable.

⚠️ **The cost that scales with the cadence is the allocation, not the CPU.** The boot-volume walk grows the footprint by
~166 MB while it runs, so halving the interval doubles how often that spike happens. That is the reason not to make it
"fresher" on instinct.

The tick fires on the interval rather than immediately: the scan-completion subscription already covers startup.
`spawn_recompute` coalesces on the full-pass key, so a tick landing inside a running pass is absorbed, not queued.
`periodic_refresh_tests` pins the interval far above `INCREMENTAL_THROTTLE_WINDOW`, because nothing in the types stops
someone rebuilding the treadmill by tuning one constant.

## Multi-volume, kind-aware scoring

The scheduler scores **any** background-scored volume, not just the local `root`. The typed volume kind
(`indexing::IndexVolumeKind`, retained on the registry instance) decides the policy at a single seam
(`ScoringPolicy::for_kind`), never by inspecting the volume-id string:

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
- `incremental_tests.rs` — rescoping, the ancestor cap, and the `/` sanitization, over synthetic walks.
- `incremental_transition_tests.rs` — the whole pass over a REAL mutable index DB, every scenario run under BOTH walks
  and differenced: marker created / deleted, renamed to and away from `node_modules`, a change under a floored ancestor,
  a change at the volume root, a batch spanning unrelated subtrees, an origin deleted between publish and pass, a
  case-variant origin, and nested-origin de-duplication.
- `walk_memory_tests.rs` — the walk's per-folder byte and allocation ceilings.
- `test_support.rs` — the shared synthetic-index builders.

The registration bus's late-volume delivery is covered in `indexing/lifecycle/lifecycle_bus.rs`.
