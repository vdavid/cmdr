# Importance scheduler

Fills each volume's store: `mod.rs` (the `ImportanceScheduler` handle, `ScoringPolicy`, the `PassCoordinator`, bus
wiring, the incremental throttle), `walk.rs` (the O(dirs) index walk), `recompute.rs` (signals + score + write, full and
incremental). The volume-kind policy and the floor doctrine are in `../CLAUDE.md`.

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
  list on their own, the two must stay on the SAME `changed_paths` slice or the clear deletes rows nothing re-adds.
  `sanitize_incremental_batch` drops the bare `/` — ❌ don't reintroduce a `/` ⇒ full-pass escalation, which pegged a
  core with back-to-back recomputes. Throttled to ≤1 walk per `INCREMENTAL_THROTTLE_WINDOW` (60 s), leading edge first.
  Ancestor rescoping is capped at `ANCESTOR_WALK_CAP` (32).
- **The batch's cost is set by what `dir-changed` carries: ORIGIN dirs, never their ancestors** (contract in
  `../../indexing/lifecycle/CLAUDE.md`). One ancestor in a batch rescores its whole subtree, which is how a two-folder
  change once rewrote ~90 k rows a minute. A floor transition reaches a renamed folder through its PARENT origin, so the
  downward expansion stays load-bearing.
- **Spotlight sampling runs ONLY when the mask says `last_used_available`**, and on a dedicated 8 MB-stack OS thread
  with an autoreleasepool — ❌ never rayon, ❌ never against an SMB mount.

Trigger wiring, the initial-full-pass ordering trap, the walk's measurements, incremental rescoping and its accepted
lossiness, and the kind-aware policy: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
