# Importance scheduler

Fills each volume's store: `mod.rs` (the `ImportanceScheduler` handle, `ScoringPolicy`, `PassCoordinator`, the pass),
`wiring.rs` (construction, `wire_volume` + its two callers, throttle, spawns), `walk.rs` (the O(dirs) full pass),
`scoped_walk.rs` (the O(touched) incremental walk + batch plan), `recompute.rs` (signals + score + write),
`differential.rs` (the two-walk harness). Volume-kind policy and floors: `../CLAUDE.md`.

## Must-knows

- **Drive full recompute off the bus `ScanCompleted`, `wire_volume`'s initial-pass probe, and the hourly
  `FULL_REFRESH_INTERVAL` tick, ❌ NEVER phase events** (network volumes never emit them). A volume Fresh at launch
  never re-fires, so `enqueue_full_pass_if_needed` is what scores it. Subscribe to registrations BEFORE the sweep, or a
  share mounted in the gap is never wired.
- **❌ Never move that probe back into the startup sweep alone.** Root's index starts on a spawned task while `start()`
  runs right after it, so the sweep usually sees an EMPTY registry and root arrives on the registration bus — a
  sweep-only probe is unreachable in prod and fails silently (a classifier fix then never reaches existing rows). Keep
  it in `wire_volume`, which both paths share. `wiring_tests.rs`.
- **Coalesce per volume through `PassCoordinator`**: one pass plus one re-run. Incremental has its OWN key, so a rescore
  and a full pass never block each other.
- **The full walk is O(dirs) in a SMALL CONSTANT, every part of that shape measured.** Dirs live in the shared
  `DirTree`; a folder is one `Copy` `IndexFolder`. ❌ No `EntryRow`, no `IndexStore::all_directories`, no stored path,
  no per-folder extension set, and ❌ never drop `for_each_file_child_by_parent`'s `ORDER BY parent_id`.
  `walk_memory_tests.rs` guards it.
- **Incremental writes at the CURRENT generation, doesn't bump it, and NEVER escalates to a full pass.** ❌ Never narrow
  `is_in_changed_subtree` or widen the clear list alone: both must stay on the SAME `changed_paths` slice, or the clear
  deletes rows nothing re-adds. ❌ Don't reintroduce a `/` ⇒ full-pass escalation, which pegged a core.
- **`sanitize_incremental_batch` gates the batch BEFORE the read pool and the walk**, dropping the bare `/`, empties,
  and every FLOORED path. ❌ Don't remove the idle floor.
- **An incremental reads only the CHANGED SUBTREES** (`scoped_walk.rs`). ❌ Don't prune floored subtrees from the
  descent (a marker inside a `node_modules` still raises the folders above it), ❌ don't add a `folders.is_empty()`
  early return (an all-deleted batch is exactly the one whose rows must be CLEARED), ❌ don't raise
  `SCOPED_WALK_MAX_ORIGINS` / `SCOPED_WALK_MAX_DIRS`.
- **An origin past `SCOPED_WALK_MAX_DIRS` is DEMOTED to rescoring itself alone** (`plan_incremental_batch`, keyed on
  `dir_stats.recursive_dir_count`, ❌ never a path shape). A demoted origin is ❌ NEVER in the clear list (or the clear
  wipes a subtree the pass didn't read) and absorbs ❌ NOTHING in `dedupe_nested_origins` (or `$HOME` swallows the real
  change beside it).
- **A pass writes only what MOVED, keyed on the SIGNALS blob and ❌ never the score** (`../writer.rs`): a score drifts
  with `now_secs` on its own. The log carries BOTH `written` and `considered` — ❌ don't drop the latter, it's what
  makes a too-wide batch visible.
- **`dir-changed` carries ORIGIN dirs, never their ancestors**: feed it an ancestor and it rescores that whole subtree.
  The downward expansion is load-bearing (a floor transition reaches a renamed folder through its PARENT origin).
- **Spotlight sampling runs ONLY when the mask says `last_used_available`**, on a dedicated 8 MB-stack OS thread with an
  autoreleasepool — ❌ never rayon, ❌ never against an SMB mount.

Trigger wiring, the initial-full-pass trap, the throttle and ancestor cap, the walk's measurements, rescoping's
lossiness, and the kind policy: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing,
or advising.
