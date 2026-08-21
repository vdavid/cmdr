# Media-index scheduler

The coalesced enrichment passes: bus wiring, the walk, the enrich core, the parallel pool, and the reclaim prune.
Subsystem-wide invariants (deletion doctrine, the scope model, `should_stop`) are in `media_index/CLAUDE.md`.

## Module map

- `mod.rs` — the `MediaScheduler` struct + the pass bodies. `coordinator.rs` — the pure `PassCoordinator` (one pass per
  volume, coalesced re-run). `lifecycle.rs` — the wiring/kick layer (`start`, `wire_volume`, `pass_coverage`,
  `spawn_pass`, `local_should_enrich`).
- `enrich.rs` — the index walk + the shared enrich/GC core. `pool.rs` — the parallel workers. `live.rs` — the live
  follow-the-index tick. `reclaim.rs` — the user-explicit prune.

## Must-knows

- **Consume the lifecycle bus by EDGE**: `borrow_and_update` / `has_changed`, ❌ never a `borrow()` poll, which would GC
  live rows mid-truncate (the why is `../DETAILS.md` § The GC safety argument). ❌ NEVER persist the bus `generation`
  (it's a transient in-memory wake counter).
- **GC scope is a PARAMETER, and getting it wrong wipes the store.** `GcScope::WholeStore` is for the full pass / Fresh
  sweep only; a live tick MUST use `GcScope::TouchedDirs`, which collects only rows under the dirs it re-walked.
  Whole-store `gc_targets` on a scoped walk deletes every row OUTSIDE the touched dirs.
- **A tick coverage-filters its touched dirs BEFORE walking**, then the walk hands back a `WalkedDirs` token that
  `GcScope::TouchedDirs` and `coverage::patch_touched_dirs` are the only consumers of — so a GC scope wider than the
  walk (which deletes every row in the difference) is unrepresentable rather than forbidden. `local_dir_may_be_covered`
  is a proven superset of `local_should_enrich`; keep it that way (a proptest holds it).
- **The live tick coalesces on its own `#live` coordinator key**, ❌ never the full-pass key, or a `ScanCompleted` full
  pass would silently downgrade into a scoped tick. It skips entirely while a full pass runs for that volume.
- **`folder_scores` is a wrapper over `coverage::importance_scores`** (the subsystem-wide "never `above_threshold`
  direct" rule): a tick asks once a minute per volume, and the direct read was 45.8 ms at 90,308 folders against 2.8 µs.
- **`folder_scores` `None` ⇒ override-only.** ❌ Never fall back to enrich-all: importance takes seconds to land, the
  slider is forward-only, and an enrich-all pass over-indexes the volume permanently. A pass that deferred marks the
  volume (`mark_deferred_for_importance`); `wire_volume` subscribes to importance SYNCHRONOUSLY, before the first pass,
  or the unscored → scored bridge has a hole. ❌ A live tick never marks deferred.
- **Parallelism is N INDEPENDENT backends** (`pool.rs`): ❌ never feed one backend concurrently (CF confinement,
  `../backend/CLAUDE.md`), ❌ never fan out the single writer. Workers pull off ONE shared atomic cursor, so no
  double-enrichment. Tests inject `FakeVisionBackend` via `MediaScheduler::new`, never `start`.
- **The walk holds folders compactly.** `for_each_qualifying_image` streams file rows and hands each COMPLETE per-dir
  group to `qualify_dir` (the sibling-aware rules need the whole name set); the folder side rides
  `indexing/store/dir_tree.rs`'s `DirTree`. ❌ Don't reach for `IndexStore::all_directories` here (~3× the RAM).
- **A pass refills the coverage cache from its own walk** (`replace_from_entries` right after the walk, a tick's
  `patch_touched_dirs`); ❌ don't just invalidate, or the next slider preview re-pays a whole-index walk.

Pass anatomy, the parallel-enrichment measurements, importance ordering, defer-until-scored, live-tick guardrails, and
reclaim: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
