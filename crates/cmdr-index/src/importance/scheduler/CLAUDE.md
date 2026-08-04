# Importance scheduler

Fills each volume's store: `mod.rs` (the `ImportanceScheduler` handle, `ScoringPolicy`, `PassCoordinator`, the pass),
`wiring.rs` (construction, bus subscriptions + sweep, throttle, spawns), `walk.rs` (the O(dirs) full-pass walk),
`scoped_walk.rs` (the O(touched) incremental walk + batch plan), `recompute.rs` (signals + score + write),
`differential.rs` (the two-walk harness). Volume-kind policy and floors: `../CLAUDE.md`.

## Must-knows

- **Drive full recompute off the bus `ScanCompleted`, the startup sweep, and the hourly `FULL_REFRESH_INTERVAL` tick,
  NEVER phase events** (network volumes never emit them). A volume Fresh at launch never re-fires it, so the sweep ALSO
  runs `enqueue_initial_full_pass_if_unscored`, gated on `store::needs_initial_full_pass`. Subscribe to registrations
  BEFORE the sweep, or a share mounted in the gap is never wired. The hourly tick bounds the write-skip and demotion
  stalenesses; `periodic_refresh_tests` stops anyone shortening it back into the treadmill.
- **Coalesce per volume through `PassCoordinator`**: one pass at a time plus one re-run. Incremental has its OWN key, so
  a rescore and a full pass never block each other.

- **The walk is O(dirs) in a SMALL CONSTANT, every part of that shape measured.** Dirs live in the shared `DirTree` (24
  B/dir); a folder is one `Copy` `IndexFolder`. ❌ No `EntryRow`, no `IndexStore::all_directories`, no stored path
  (`for_each` reconstructs into ONE reused buffer), no per-folder extension set, and ❌ never drop
  `for_each_file_child_by_parent`'s `ORDER BY parent_id`. `walk_memory_tests.rs` guards it.
- **Incremental writes at the CURRENT generation, does NOT bump it, and NEVER escalates to a full pass.** ❌ Never
  narrow `is_in_changed_subtree` or widen the clear list on their own: both must stay on the SAME `changed_paths` slice,
  or the clear deletes rows nothing re-adds. `sanitize_incremental_batch` gates the batch BEFORE the read pool and the
  walk, dropping the bare `/`, empties, and every FLOORED path (the idle floor — ❌ don't remove it). ❌ Don't
  reintroduce a `/` ⇒ full-pass escalation, which pegged a core. Throttled to ≤1 pass per `INCREMENTAL_THROTTLE_WINDOW`
  (60 s), leading edge first; ancestors capped at `ANCESTOR_WALK_CAP` (32).
- **An incremental reads only the CHANGED SUBTREES** (`scoped_walk.rs`): ~100–165 µs per origin, exact. ❌ Don't prune
  floored subtrees from the descent (a marker inside a `node_modules` still raises the folders above it), ❌ don't add a
  `folders.is_empty()` early return to `run_incremental_blocking` (an all-deleted batch is exactly the one whose rows
  must be CLEARED), and ❌ don't raise `SCOPED_WALK_MAX_ORIGINS` / `SCOPED_WALK_MAX_DIRS`. The full walk stays the
  fallback AND the differential oracle.
- **An origin past `SCOPED_WALK_MAX_DIRS` is DEMOTED to rescoring itself alone** (`plan_incremental_batch`, keyed on
  `dir_stats.recursive_dir_count` — one PK lookup, never a path shape). Two ❌s carry it: a demoted origin is NEVER in
  the clear list (or the clear wipes a subtree the pass didn't read), and it absorbs NOTHING in `dedupe_nested_origins`
  (or `$HOME` swallows the real change beside it). Its `has_marker_below` comes from the stored row, one-directionally;
  a marker APPEARING still escalates. `DETAILS.md` § An over-budget origin is demoted.
- **A pass writes only what MOVED, keyed on the SIGNALS blob and ❌ never the score** (`../writer.rs`'s
  `fate_of_stored_row`): a score drifts with `now_secs` every pass on its own. A kept row keeps its old `now_secs`. The
  log carries BOTH `written` and `considered` — ❌ don't drop the latter, it's what makes a too-wide batch visible.
- **`dir-changed` carries ORIGIN dirs, never their ancestors** (`../../indexing/lifecycle/CLAUDE.md`): feed it an
  ancestor and it rescores that whole subtree. A floor transition reaches a renamed folder through its PARENT origin, so
  the downward expansion stays load-bearing.
- **Spotlight sampling runs ONLY when the mask says `last_used_available`**, on a dedicated 8 MB-stack OS thread with an
  autoreleasepool — ❌ never rayon, ❌ never against an SMB mount.

Trigger wiring, the initial-full-pass trap, the walk's measurements, rescoping's lossiness, the demotion's marker
reasoning, and the kind policy: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing,
or advising.
