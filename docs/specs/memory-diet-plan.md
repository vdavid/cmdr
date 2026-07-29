# Memory diet: cut idle RAM from ~2.5 GB

Profiled prod v0.36.2 after ~10 h uptime: `phys_footprint` 2.5–3.0 GB, peak 3.6 GB. It splits in two halves.

## The evidence (measured 2026-07-28, prod PID 3062, macOS 26.5.2)

`footprint -s` breakdown:

- `IOAccelerator` **1386 MB** — the Rust heap (mimalloc arenas, VM tag 100). See `docs/tooling/memory-debugging.md`.
- `MALLOC_LARGE` **730 MB** + `MALLOC_SMALL` **405 MB** — the system C heap, which for us is ~all SQLite.
- Everything else < 50 MB. WebKit/compositor is NOT involved.

Two root causes:

1. **156 live SQLite connections**, each with `PRAGMA cache_size = -16384` (16 MB). `lsof` by DB: 57 ×
   `importance-root.db`, 53 × `index-root.db`, 30 × `index-smb-…naspi.db`, 10 × `importance-smb-…naspi.db`, 6 ×
   `media-root.db`. Connections are **thread-local** (`indexing/read/enrichment.rs` `THREAD_CONN`, and
   `ImportanceIndex`'s own thread-local) and never closed while the thread lives, so the count tracks tokio's
   blocking-thread pool (69 threads at sample time). Ceiling 156 × 16 MB = 2.5 GB; ~1.15 GB was actually resident.

2. **A 60-second rescore treadmill.** Every ~60 s for the whole session:
   `incremental rescore of 'root' updated 90308 folders`, with the count moving by **2 folders in an hour**. Each pass
   does a full O(dirs) `walk_index_folders`, rescopes, writes ~90 k rows, flushes, `wal_checkpoint(TRUNCATE)`, then
   wakes `start_importance_weight_subscriber`, which reloads **all 161 094 weights** while the old map is still live.
   Footprint oscillates 2.6 → 3.0 GB on exactly that cycle. It also burned most of the 122 min of CPU over 10 h.

   It never stops because the boot volume is never quiet (cargo builds under `.claude/worktrees/*/target/`,
   `/private/tmp/claude-*`), so the dir-changed batch is never empty.

## Milestones

### M1 — Bound SQLite page cache (the ~1.15 GB half)

Read-only connections don't need a 16 MB page cache; only the write connection does (it's tied to
`wal_autocheckpoint = 4000` ≈ 16 MiB). Give read opens a much smaller cache across all four stores that set the pragma:
`indexing/store/mod.rs` `apply_pragmas`, `importance/store/connection.rs`, `media_index/store/connection.rs`,
`agent/store/connection.rs`.

- Keep the write path at `-16384`; the autocheckpoint rationale in `apply_pragmas`'s comment still holds.
- Name the two budgets as constants with a comment tying the write one to `wal_autocheckpoint`.
- DONE: read connections open with the small cache; a test pins that read vs write differ; the
  `wal_autocheckpoint`/cache-size coupling comment still reads true.

### M2 — Stop allocating in the rescope inner loop

`is_in_changed_subtree` (`importance/scheduler/recompute.rs`) does `path.starts_with(&format!("{changed}/"))`: one
`String` allocation **per folder per changed path**, in a loop over every walked folder, every 60 s. Replace with
allocation-free path math (`strip_prefix` + separator check). Keep the semantics byte-identical (`path == changed` or
`path` is under `changed/`).

- DONE: no allocation in the predicate; a unit test covers the prefix-but-not-a-child case
  (`/a/bc` must NOT match changed `/a/b`); behavior otherwise unchanged.

### M3 — Make the incremental rescore actually incremental

Today an "incremental" pass walks the WHOLE index before rescoping (the comment in `INCREMENTAL_THROTTLE_WINDOW` admits
it), and the downward subtree expansion pulls ~90 k folders in from a 2-folder change. Two problems: the O(dirs) walk,
and a rescope that is far wider than what changed.

Investigate before choosing a shape; the walk is already memory-tuned (84.2 MB on a 391 k-folder NAS, guarded by
`walk_memory_tests.rs`), so this is about CPU + write volume + the downstream weight reload, not walk bytes.

- The obvious lever: bound the downward expansion. A changed dir high in the tree currently drags its entire subtree in
  via `is_in_changed_subtree`. Check what `lifecycle_bus::subscribe_dirs_changed` actually publishes — if it emits
  ancestors, the subtree expansion multiplies.
- Preserve the correctness the current shape buys: floor transitions must still propagate (a folder renamed to
  `node_modules` floors its whole subtree). Don't regress that to "cheap but wrong".
- DONE: a steady-state idle machine with background churn no longer rewrites ~90 k rows per minute; the floor-transition
  cases in the existing scheduler tests still pass; new tests pin the narrowed scope.

### M4 — An idle floor (only if M3 leaves a treadmill)

If M3 doesn't already settle it, add a cheap "nothing meaningful changed ⇒ no pass" gate so constant background churn
can't drive perpetual rescores. Skip this milestone if M3 makes it moot; say so rather than inventing work.

### M5 — Watchdog: verify, then correct the stale record

The 2026-07-25 note lists watchdog bugs #2 (blind to the heap) and #3 (one-shot) as open. Both were FIXED since
(`f76319997`, `7230e86f3`): `process_memory::query_mimalloc_heap` exists and the loop keeps watching. **Verify that
against the code, then fix the note** so the next investigation doesn't re-derive it.

- Also re-check the note's other open items against current `main` and mark what's landed.
- DONE: `docs/notes/memory-runaway-rust-heap-2026-07-25.md` describes reality; no invented work.

## Invariants (do not break)

- ❌ Never a second writer thread per DB (`importance/CLAUDE.md`).
- Incremental writes at the CURRENT generation and NEVER escalates to a full pass; `sanitize_incremental_batch` drops
  the bare `/`.
- A floored folder gets NO row; floor beats marker; `under_floored_ancestor` floors the whole subtree.
- The walk's memory shape is guarded by `walk_memory_tests.rs` — don't regress it.
- `no-string-matching`: classify by typed enum/errno, never by message substring.

## Test/verify policy for this effort

The machine is under heavy load from other agents. Unrelated E2E/Rust failures are contention flakes — disregard them.
Run `pnpm check -q` scoped to the touched area per milestone; run the slow suite **once, at the very end**.
