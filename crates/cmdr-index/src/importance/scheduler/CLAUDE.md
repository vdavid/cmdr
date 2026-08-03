# Importance scheduler

Fills each volume's store: `mod.rs` (the `ImportanceScheduler` handle, `ScoringPolicy`, `PassCoordinator`, running a
pass), `wiring.rs` (construction, bus subscriptions + sweep, throttle, spawns), `walk.rs` (the O(dirs) full-pass walk),
`scoped_walk.rs` (the O(touched) incremental walk), `recompute.rs` (signals + score + write), `differential.rs` (the
two-walk harness). Volume-kind policy and floor doctrine: `../CLAUDE.md`.

## Must-knows

- **Drive full recompute off the bus `ScanCompleted` plus the startup sweep, NEVER phase events** (network volumes never
  emit them). A volume Fresh at launch never re-fires it, so the sweep ALSO runs
  `enqueue_initial_full_pass_if_unscored`, gated on `store::needs_initial_full_pass`. Subscribe to registrations BEFORE
  the sweep, or a share mounted in the gap is never wired.
- **Coalesce per volume through `PassCoordinator`**: one pass at a time plus one re-run. Incremental has its OWN key, so
  a rescore and a full pass never block each other.
- **The walk is O(dirs) in a SMALL CONSTANT, and every part of that shape was measured.** Dirs live in the shared
  `DirTree` (arena + 24 B/dir); a folder is one `Copy` `IndexFolder`. ❌ No `EntryRow`, ❌ no
  `IndexStore::all_directories`, ❌ no stored path (`WalkedFolders::for_each` reconstructs into ONE reused buffer), ❌
  no per-folder extension set (file rows stream GROUPED by parent, so ❌ don't drop `for_each_file_child_by_parent`'s
  `ORDER BY parent_id`). `walk_memory_tests.rs` guards this.
- **Incremental writes at the CURRENT generation, does NOT bump it, and NEVER escalates to a full pass.** ❌ Never
  narrow `is_in_changed_subtree` or widen the clear list on their own: both must stay on the SAME (de-duplicated)
  `changed_paths` slice, or the clear deletes rows nothing re-adds. `sanitize_incremental_batch` gates the batch BEFORE
  the read pool and the walk, dropping the bare `/`, empties, and every FLOORED path (the idle floor — ❌ don't remove
  it). ❌ Don't reintroduce a `/` ⇒ full-pass escalation, which pegged a core. Throttled to ≤1 pass per
  `INCREMENTAL_THROTTLE_WINDOW` (60 s), leading edge first; ancestor rescoping capped at `ANCESTOR_WALK_CAP` (32).
- **An incremental reads only the CHANGED SUBTREES** (`scoped_walk.rs`): ~100–165 µs per origin, not the full walk's
  seconds. Exact, not approximate (reasoning in `DETAILS.md`). ❌ Don't prune floored subtrees from the descent (a
  marker inside a `node_modules` still raises the folders above it), and ❌ don't add a `folders.is_empty()` early
  return to `run_incremental_blocking` — an all-deleted batch walks to nothing and is exactly the batch whose rows must
  be CLEARED. Bounded by `SCOPED_WALK_MAX_ORIGINS` / `SCOPED_WALK_MAX_DIRS`; ❌ don't raise either, measured worse. The
  full walk stays the fallback AND the differential oracle.
- **A pass writes only what MOVED, keyed on the SIGNALS blob and ❌ never the score** (`../writer.rs`'s
  `fate_of_stored_row`): a score also moves with `now_secs`, so it drifts every pass on its own (99.88% of a real pass's
  rows matched on signals, 0.03% on score). A kept row keeps its old `now_secs`. The log carries BOTH `written` and
  `considered` — ❌ don't drop the latter, it's what makes a too-wide batch visible.
- **The batch's cost is set by what `dir-changed` carries: ORIGIN dirs, never their ancestors** (contract in
  `../../indexing/lifecycle/CLAUDE.md`). One ancestor rescores its whole subtree: that's how a two-folder change once
  rewrote ~90 k rows a minute, and how a dotfile in `~` still drags in 83% of the volume. A floor transition reaches a
  renamed folder through its PARENT origin, so the downward expansion stays load-bearing.
- **Spotlight sampling runs ONLY when the mask says `last_used_available`**, and on a dedicated 8 MB-stack OS thread
  with an autoreleasepool — ❌ never rayon, ❌ never against an SMB mount.

Trigger wiring, the initial-full-pass ordering trap, the walk's measurements, incremental rescoping and its lossiness,
and the kind-aware policy: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
