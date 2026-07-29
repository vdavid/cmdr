# Importance scheduler

Fills each volume's store: `mod.rs` (the `ImportanceScheduler` handle, `ScoringPolicy`, the `PassCoordinator`, bus
wiring, the incremental throttle), `walk.rs` (the O(dirs) full-pass index walk), `scoped_walk.rs` (the O(touched)
incremental walk), `recompute.rs` (signals + score + write, full and incremental), `differential.rs` (the two-walk
correctness harness). The volume-kind policy and the floor doctrine are in `../CLAUDE.md`.

## Must-knows

- **Drive full recompute off the bus `ScanCompleted` plus the startup sweep, NEVER phase events** (network volumes
  never emit them). A volume Fresh at launch never re-fires `ScanCompleted`, so the sweep ALSO runs
  `enqueue_initial_full_pass_if_unscored`, gated on `store::needs_initial_full_pass` (which forces the WRITE-path open
  first). Subscribe to registrations BEFORE the sweep, or a share mounted in the gap is never wired.
- **Coalesce per volume through `PassCoordinator`**: one pass at a time plus at most one re-run. Incremental uses its
  OWN coordinator key, so a rescore and a full pass never block each other.
- **The walk is O(dirs) in a SMALL CONSTANT, and every part of that shape was measured.** Dirs live in the shared
  `DirTree` (arena + 24 B/dir); each folder is one `Copy` `IndexFolder` (tree-row index, mtime, `ChildAggregate`, two
  flags). ❌ No `EntryRow`, ❌ no `IndexStore::all_directories`, ❌ no stored path (`WalkedFolders::for_each`
  reconstructs into ONE reused buffer), ❌ no per-folder extension set (file rows stream GROUPED by parent, so ❌ don't
  drop `for_each_file_child_by_parent`'s `ORDER BY parent_id`). 84.2 MB not 256.4 MB on a 391,563-folder NAS;
  `walk_memory_tests.rs` guards it.
- **Incremental writes at the CURRENT generation, does NOT bump it, and NEVER escalates to a full pass.** It clears each
  changed subtree, then re-inserts only non-floored folders — ❌ never narrow `is_in_changed_subtree` or widen the clear
  list on their own, the two must stay on the SAME (de-duplicated) `changed_paths` slice or the clear deletes rows
  nothing re-adds. `sanitize_incremental_batch` gates the batch BEFORE the read pool and the walk, dropping the bare
  `/`, empties, and every FLOORED path (that last one is the idle floor: build output and caches can't score, so a batch
  of only churn costs nothing — ❌ don't remove it, it's what stops a pass a minute forever). ❌ Don't reintroduce a `/`
  ⇒ full-pass escalation, which pegged a core with back-to-back recomputes. Throttled to ≤1 pass per
  `INCREMENTAL_THROTTLE_WINDOW` (60 s), leading edge first. Ancestor rescoping is capped at `ANCESTOR_WALK_CAP` (32).
- **An incremental reads only the CHANGED SUBTREES** (`scoped_walk.rs`): median ~100–165 µs per origin against a full
  walk's seconds. It's exact, not approximate — `under_floored_ancestor` is pure path math over a folder's own prefixes,
  `has_marker_below` is exact inside a downward-closed subtree, and the one cross-boundary case (a subtree's marker
  presence raising ancestors above it) is caught by comparing the origin's STORED `has_project_marker` against the fresh
  one, which takes the full walk when it flips. ❌ Don't prune floored subtrees from the descent (a marker inside a
  `node_modules` still raises the folders above it) and ❌ don't add a `folders.is_empty()` early return to
  `run_incremental_blocking` — an all-deleted batch walks to nothing and is exactly the batch whose rows must be
  CLEARED. Bounded by `SCOPED_WALK_MAX_ORIGINS` / `SCOPED_WALK_MAX_DIRS`, the latter checked mid-descent. The full walk
  stays the fallback AND the oracle: `differential.rs` + `importance-diff` difference the two over a real index, and
  `incremental_transition_tests.rs` runs every scenario under both.
- **The batch's cost is set by what `dir-changed` carries: ORIGIN dirs, never their ancestors** (contract in
  `../../indexing/lifecycle/CLAUDE.md`). One ancestor in a batch rescores its whole subtree, which is how a two-folder
  change once rewrote ~90 k rows a minute. A floor transition reaches a renamed folder through its PARENT origin, so the
  downward expansion stays load-bearing.
- **Spotlight sampling runs ONLY when the mask says `last_used_available`**, and on a dedicated 8 MB-stack OS thread
  with an autoreleasepool — ❌ never rayon, ❌ never against an SMB mount.

Trigger wiring, the initial-full-pass ordering trap, the walk's measurements, incremental rescoping and its accepted
lossiness, and the kind-aware policy: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
