# Per-directory "size updating" hourglass plan

## Problem

When the user deletes (or adds) many files at once — for example wiping a handful of Rust `target/` folders, several
hundred thousand files — the recursive folder sizes shown in the file list lag behind reality for seconds to minutes,
with **no indication that they're stale**. A user in the mindset of "freeing up disk space" reads those numbers as
truth, and they're silently wrong.

Evidence from a real session (dev logs, 2026-05-28 ~17:46–17:49): deleting a batch of `target/` dirs produced ~70K
writer operations over ~3 minutes, in two waves separated by a ~90 s lull. Writer `queue_depth` peaked at 167; per-batch
counts hit 18K messages. The writer kept up and propagated `dir_stats` deltas correctly — the data was never corrupt —
but the **displayed** numbers were transient mid-storm snapshots shown with the same confidence as settled values. The
second wave arrived 90 s after things "looked done," shifting totals again with no signal.

## Goal

Show the existing hourglass indicator (`IconHourglass`, already rendered next to indexed folder sizes) **per
directory**, whenever that directory or any descendant has unprocessed indexing work in flight. When the indexer drains,
the hourglass clears and the number standing there is correct.

We deliberately keep showing best-effort, as-fresh-as-possible numbers that ratchet toward truth during the storm — each
marked with the hourglass meaning "still moving, don't trust as final." For someone freeing disk space, a
live-updating-but-flagged number is more honest and reassuring than a frozen one. (This is why we are NOT adding a
trailing-edge debounce that would freeze the number until the end. Decision discussed with David 2026-05-29.)

### Non-goals

- No change to how full-scan / aggregation progress is surfaced (the global `indexing` flag path stays).
- No trailing-edge debounce of the FE refresh.
- No new full-pane refresh (no cursor/scroll/selection reset). We ride the existing in-place size update.
- Brief view (no dir-size column today) is out of scope beyond carrying the new data field for free.

## Key discovery: most of the machinery already exists

The implementing agent should internalize this before writing code — it shrinks the change a lot.

1. **The hourglass already renders per row.** `apps/desktop/src/lib/file-explorer/views/FullList.svelte` lines ~915–925:
   for a directory with a known size, `{#if indexing}` renders
   `<span class="size-stale icon-indicator"><IconHourglass …/></span>` with tooltip "Updating index: size may change."
2. **It's gated on a GLOBAL flag, which is the bug.** `indexing` is `$derived(isScanning() || isAggregating())`
   (`FullList.svelte:200`, from `$lib/indexing/index-state.svelte`). During David's delete storm there was no full
   scan/aggregation running — only live reconciler events — so `indexing` was false and **no hourglass showed**. The fix
   is to drive the hourglass from a **per-directory pending signal** in addition to the global flag.
3. **There's already a `size-stale` display state and tooltip wording.** `full-list-utils.ts::getDirSizeDisplayState`
   returns `'size-stale'` and `buildDirSizeTooltip` appends "Updating index — size may change." Both currently keyed on
   the global `indexing` bool.
4. **The backend already computes the exact set of affected ancestor dirs.** `reconciler.rs:661`
   `let mut affected = collect_ancestor_paths(&normalized);` — `process_fs_event` returns these ancestor paths, which
   flow via `on_dirs_affected` → `WriteMessage::EmitDirUpdated` → `index-dir-updated` event → the FE's throttled
   in-place size refresh. **The paths we must mark as pending are the same `affected` paths already collected**, so the
   pending flag and the refresh target are aligned by construction.
5. **`queue_depth` already exists** as `Arc<AtomicUsize>` on the writer: incremented in `send()` (`writer.rs:270`),
   decremented in the recv loop (`writer.rs:542`). `queue_depth == 0` is a precise, already-maintained "writer fully
   caught up" signal.

So the feature is essentially: **maintain an in-memory set of directory paths with pending work, populated from the live
event loop's existing affected-paths set, cleared wholesale when the writer drains, and read per-dir when building
`DirStats` (the path the live in-place refresh already uses) to flip the existing hourglass.**

## Design

### Why a path set cleared on drain, not per-entry counters

The originally-discussed design used per-`EntryId` counters: increment on enqueue, decrement on commit. That requires
pairing every increment with exactly one decrement — the classic source of "stuck forever" leaks (a dropped message, a
panic, a rename changing the ancestor chain between enqueue and commit). It also needed an ancestor walk per event,
which by `EntryId` means DB reads on the hot path (~1M reads during the observed storm).

Instead:

- **Mark** a `HashSet<String>` of directory paths. The reconciler already hands us the ancestor paths
  (`collect_ancestor_paths`), so marking is pure in-memory string inserts — no DB walk.
- **Clear the whole set** when `queue_depth` reaches 0. By definition, an empty writer queue means there is no
  unprocessed work, so the set is correct to empty. This is **self-healing**: even if marking has a bug, every time the
  writer catches up the state resets to truth. There is no pairing to get wrong, so the entire "leaked → stuck
  hourglass" failure class disappears.

The tradeoff is coarser granularity: during a storm, every touched ancestor stays flagged until the writer fully drains,
then they clear together. For the target scenario (mass delete, "is it settled yet?") this is the _right_ granularity —
it answers exactly the user's question. It also matches "elegance above all": fewer moving parts, a strong invariant, no
bookkeeping I have to prove balances.

### The pending-size tracker

New module `apps/desktop/src-tauri/src/indexing/pending_sizes.rs`:

```rust
/// Tracks directories with unprocessed index writes in flight, so the UI can
/// show a per-directory "size updating" hourglass. Marked from the live event
/// loop (the dirs it's about to notify the UI about), read when building
/// `DirStats`, and cleared wholesale when the writer queue drains to empty.
pub(crate) struct PendingSizes {
    paths: Mutex<HashSet<String>>, // normalized dir paths
}

impl PendingSizes {
    /// Normalize `path` and insert it AND every ancestor dir. Centralizing the
    /// ancestor expansion here means callers can pass whatever set of affected
    /// dirs they have (a full chain or a single parent) and the membership test
    /// is correct for any ancestor row shown in the UI.
    fn mark(&self, path: &str);
    fn is_pending(&self, path: &str) -> bool; // normalizes then membership test
    fn clear(&self);                          // wholesale reset on drain
}
```

The tracker owns the ancestor expansion (split the normalized path on `/`, insert each prefix). This keeps the call
sites trivial and robust: normal live events already hand us full ancestor chains, while the rename path hands us a
single parent — `mark` fills the chain either way.

- Stored as a module-global mirroring the existing `READ_POOL` pattern (`enrichment.rs:69`):
  `static PENDING_SIZES: LazyLock<Mutex<Option<Arc<PendingSizes>>>>` with a `get_pending_sizes()` accessor and a
  `#[cfg(test)]` test mutex like `READ_POOL_TEST_MUTEX`. **Mirror EVERY `READ_POOL` lifecycle site**, not just the
  install — otherwise the tracker outlives the pool on stop/clear/restart and keeps stale flags alive. Production sites:
  install at `state.rs:206`; take/clear at `state.rs:124, 215, 255, 281` and `mod.rs:758`. The `mod.rs:556/582/899`
  sites are `#[cfg(test)]` — mirror them too so the tracker tests stay isolated. Grep `READ_POOL` and add a parallel
  `PENDING_SIZES` line at each.
- `Mutex<HashSet<String>>` is fine: writes are quick inserts at ~5K/s peak; reads are dozens per
  `get_file_range`/`get_dir_stats_batch` every couple seconds. Contention is negligible. (Do not reach for `DashMap` —
  not currently a dependency, and the mutex is simpler. Confirm before adding any new crate.)
- **Normalization**: store and query normalized paths via `firmlinks::normalize_path` on both the mark side (live drain
  points) and the read side (`DirStats` build in `state.rs`), so firmlink aliases match. The `affected` paths in
  `pending_paths` come from `collect_ancestor_paths(&normalized)` where `normalized` is already normalized — `mark`
  should normalize defensively anyway, and the read side must normalize the `get_dir_stats_batch` path args before the
  membership test.

### Marking (live event loop)

Mark at the points where the **live** loop drains `pending_paths` into the UI notification — the same set of dirs we're
about to tell the FE to refresh. This gives the invariant **"flag exactly what we refresh"** and, crucially, keeps
marking off the shared `process_fs_event` (which also runs during replay).

`process_fs_event` is called from BOTH the live loop and the cold-start/replay loops (`reconciler.rs` replay path ~line
195, `event_loop.rs:641`, `event_loop.rs:1167`). **Do NOT mark inside `process_fs_event` / `process_live_event`** — it
would fire during replay too, ballooning the set during startup, contradicting the global-flag-handles-scans split.
During a full scan / replay the global `indexing` flag already drives the hourglass for everything.

There are **two live loops with DIFFERENT emit mechanisms** — mark in both, before draining, regardless of mechanism:

- `run_live_event_loop` (`event_loop.rs:147`) drains via the writer: `writer.send(WriteMessage::EmitDirUpdated(…))` at
  ~**224–226** and ~**272–278**.
- The post-replay live loop inside `run_replay_event_loop` (~line 850+) drains via a **direct**
  `reconciler::emit_dir_updated(&app, …)` at ~**909** and ~**943** — NOT through the writer.

Grep for every `pending_paths.drain()` / `live_pending_paths.drain()` site so none is missed. At each, mark the paths
before they're drained. Mechanism-agnostic shape (adapt the trailing call to whichever the site uses):

```rust
if !pending_paths.is_empty() {
    let paths: Vec<String> = pending_paths.drain().collect();
    if let Some(t) = pending_sizes::get_pending_sizes() {
        for p in &paths { t.mark(p); }
    }
    // loop 1: writer.send(WriteMessage::EmitDirUpdated(paths))
    // loop 2: reconciler::emit_dir_updated(&app, paths)
}
```

Ordering note: in loop 1 the refresh is ordered _after_ the writer applies the size deltas (that's why `EmitDirUpdated`
is a writer message — see the writer CLAUDE.md gotcha). Loop 2 emits directly off the event-loop thread, racing the
writer. **Marking is correct in both** — it's self-healing on drain, so loop 2's race can at worst show a
flagged-but-not-yet-updated size briefly, which the next refresh corrects. No stuck-flag risk either way.

The two drain points cover everything, including moves — **no extra mark site is needed**. Reasoning:

- Normal create/modify/remove events: `pending_paths` already holds the full ancestor chains (`process_fs_event` →
  `collect_ancestor_paths(&normalized)`), and `mark` re-expands ancestors anyway.
- Rename pre-pass (`detect_renames_by_inode`, `event_loop.rs:399`, called only from `process_live_batch` — live-only):
  the **dest** chain reaches `pending_paths` via `new_parent_path` (`event_loop.rs:488`), which `mark` expands to the
  full chain. The **source** chain reaches `pending_paths` too: the OLD-path event stays in the batch and is processed
  by `process_live_event` → `process_fs_event` → `handle_removal`, which computes
  `affected = collect_ancestor_paths(old)` _before_ the removal (`reconciler.rs:661`) and returns it
  (`reconciler.rs:710`) even though `resolve_path` now no-ops. So both chains are in `pending_paths` at the drain.

Do NOT try to mark inside `detect_renames_by_inode`: its only path local (`path`) is the **dest** path (the one that
must exist — `symlink_metadata` bails otherwise at `event_loop.rs:413`), not the source, so a mark there would be a
redundant dest mark, not the source. Edge case: a rename that somehow arrives _without_ a paired old-path event would
leave the source chain unmarked for that batch — acceptable, since clear-on-drain

- the next event settle it (self-healing). If the move integration test (M1) shows the source chain isn't marked,
  revisit; otherwise the two drain points suffice.

### Clearing (writer thread)

In `writer_loop` (`writer.rs`), at the end of each iteration after `process_message` returns, when the queue is empty,
clear the set:

```rust
// end of loop body, after process_message + stats logging:
if queue_depth.load(Ordering::Relaxed) == 0 {
    if let Some(tracker) = pending_sizes::get_pending_sizes() {
        tracker.clear();
    }
}
```

Clear at end-of-iteration (after the message's DB effect is applied), NOT at recv time. At recv the decrement to 0
happens _before_ the delete/propagate runs, so clearing then would briefly show a settled flag against a not-yet-updated
size. End-of-iteration clearing keeps the flag set for the whole duration of the last message's processing (correct:
hourglass stays up while the size is mid-update) and only clears once the writer is genuinely idle. The residual skew
(flag clears microseconds before the next enrich) is on the safe side and self-heals on the final
`EmitDirUpdated`-triggered refresh.

Note `queue_depth` counts ALL messages (scanner inserts, vacuum, checkpoint, flush), so the set only clears when the
writer is fully idle. That's acceptable and honest: if the writer is continuously busy, sizes genuinely are in flux.
From the observed logs the queue hits 0 frequently between bursts.

### Reading — carry the flag on `DirStats` only (not `FileEntry`)

The FE size column updates via two paths, but only one needs the flag:

1. **`get_dir_stats_batch` → `updateIndexSizesInPlace`** (the throttled in-place refresh; the PRIMARY way the column
   updates during a storm — `file-list-utils.ts:223`). This is the one that matters: in the live-watch scenario the user
   is sitting in a folder while `index-dir-updated` events drive repeated `updateIndexSizesInPlace` calls. **Carry the
   flag here.**
2. **`get_file_range` → `enrich_entries_with_index`** (initial fetch / forced refetch). **Deliberately NOT carried.**
   Doing so would require adding the field to the Rust `FileEntry`, which does **not** derive `Default` and has ~30
   exhaustive struct-literal construction sites across backends, MCP, and tests — a large, churny surface for little
   gain. The only thing lost is: a folder freshly navigated-into _during_ a storm shows no hourglass until the first
   `index-dir-updated` → `updateIndexSizesInPlace` tick (sub-2 s, and only while events still flow for it). For the
   steady-watching case there's zero delay. Accepted tradeoff; note it in the indexing CLAUDE.md so a future agent
   doesn't "fix" half of it.

### DTO changes

- `DirStats` (`indexing/store.rs:34`): add `pub recursive_size_pending: bool`. It derives
  `Serialize + Deserialize + specta::Type`; a bool is fine. **Built in `state.rs`, not the command** — the Tauri command
  `commands/indexing.rs::get_dir_stats_batch` is a passthrough; the real construction is at **`state.rs:439` (single
  `get_dir_stats`) and `state.rs:475` (batch)**. Set `recursive_size_pending` from the tracker at BOTH sites (normalize
  the path before the membership test). Two literal sites to update.
- Rust `FileEntry`: **unchanged** (see above).
- Regenerate bindings: `cd apps/desktop && pnpm bindings:regen` (DirStats is exported). CI's `bindings-fresh` check
  enforces freshness.
- FE type: the file list uses the **hand-authored** `lib/file-explorer/types.ts` `FileEntry` interface (line 3), which
  is separate from the generated `bindings.ts` `FileEntry`. Add an optional `recursiveSizePending?: boolean` there. It's
  populated by `updateIndexSizesInPlace` (from `DirStats`) and `createParentEntry`, and left `undefined` (falsy → no
  hourglass) on the initial `get_file_range` render.

### FE rendering

- **The load-bearing edit**: `FullList.svelte` line ~921, change the hourglass guard from `{#if indexing}` to
  `{#if indexing || file.recursiveSizePending}`. The size-column display state is decided INLINE in the template
  (`{#if dirDisplaySize != null}` / `{:else if indexing}`), not via `getDirSizeDisplayState`. Keeps current full-scan
  behavior; adds the live-edit case.
- `buildDirSizeTooltip` (`full-list-utils.ts`) IS used in render (`FullList.svelte:900, 927`, currently passed the
  global `indexing`). Thread the per-entry pending in (e.g. pass `indexing || file.recursiveSizePending` as its
  `scanning` arg) so the "Updating index — size may change" line reflects per-dir pending.
- `getDirSizeDisplayState` (`full-list-utils.ts:332`) is **test-only** — not imported by `FullList.svelte`. Update its
  signature + `dir-size-display.test.ts` for fidelity, but know it does NOT affect rendering.
- `updateIndexSizesInPlace` (`file-list-utils.ts:251`): set
  `entry.recursiveSizePending = stat?.recursiveSizePending ?? false`. Set it **even when `stat` is null/falsy** (the
  current loop only writes fields inside `if (stat)`), so a dir that has drained gets its flag cleared back to `false`
  on the next refresh rather than staying stuck-on from a prior tick.
- `createParentEntry` (`file-list-utils.ts:36`): set `recursiveSizePending: stats?.recursiveSizePending` so the `..` row
  — which shows the CURRENT folder's size, the exact dir the user is watching drain — lights up on first paint, not only
  after the first in-place refresh tick.
- Column width: `measure-column-widths.ts:230` reserves `SIZE_ICON_WIDTH` when
  `entry.isDirectory && indexing && entry.recursiveSize != null`. Extend the condition to
  `(indexing || entry.recursiveSizePending)` so per-dir hourglasses don't get clipped / cause width jitter.

## Milestones

Sequential is fine (we're not in a hurry). Each milestone leaves the tree green.

### M1 — Backend tracker, fully TDD (no UI yet)

1. Write `pending_sizes.rs` with `PendingSizes` + unit tests FIRST:
   - mark then `is_pending` true; unmarked path false.
   - mark a deep descendant's ancestor chain → each ancestor pending.
   - `clear()` empties everything.
   - normalization: marking `/a/b` makes a firmlink-aliased query match (if a representative alias exists in tests;
     otherwise assert plain normalization equivalence).
2. Wire the global + init/teardown alongside `READ_POOL`. Add the `#[cfg(test)]` test mutex.
3. Hook marking at the two live drain points in `event_loop.rs`, clearing in the writer loop. NOT in `process_fs_event`
   (shared with replay).
4. Integration tests:
   - Drive `process_live_batch` (NOT bare `process_fs_event`) with a synthetic delete-subtree sequence against an
     in-memory index; assert the affected ancestors are pending, and that after `flush()` (queue drains) the set is
     empty.
   - **Move test**: model after the existing `event_loop.rs:2066/2131 detect_renames_by_inode_*` tests, but drive the
     full `process_live_batch` (so the OLD-path event is processed alongside the rename pre-pass) and mark at the drain.
     Assert BOTH source and dest ancestor chains end up marked — this is the guardrail for the "moves under-marked"
     concern; if it fails, the source chain isn't reaching `pending_paths` and the rename path needs explicit handling
     after all.

Run: `./scripts/check.sh --check clippy` and the new Rust tests via
`cd apps/desktop/src-tauri && cargo nextest run pending` (and the reconciler/writer test names).

### M2 — Plumb the flag through `DirStats` + FE type

1. Add `recursive_size_pending: bool` to `DirStats` (`store.rs:34`); set it at both build sites `state.rs:439` and
   `state.rs:475` from the tracker (normalize path first). Leave Rust `FileEntry` untouched.
2. `pnpm bindings:regen`; verify `bindings-fresh`.
3. Add optional `recursiveSizePending?: boolean` to the hand-authored `lib/file-explorer/types.ts` `FileEntry` (line 3).
4. Rust test: a `get_dir_stats`/`get_dir_stats_batch`-level test (state.rs) asserting a dir marked pending comes back
   with `recursive_size_pending == true` and an unmarked one `false`.

### M3 — Frontend wiring + tests

1. `FullList.svelte` hourglass guard (`indexing || file.recursiveSizePending`), the two `buildDirSizeTooltip` call sites
   (thread per-entry pending), and `createParentEntry` + `updateIndexSizesInPlace` copy (incl. the null-stat
   clear-to-false).
2. `measure-column-widths.ts` width condition (`indexing || entry.recursiveSizePending`).
3. `getDirSizeDisplayState(recursiveSize, indexing)` signature (`full-list-utils.ts:332`) — test-only; update it +
   `dir-size-display.test.ts` for fidelity (does not affect render).
4. Vitest unit tests (deterministic, no timing):
   - `dir-size-display.test.ts`: a dir with `recursiveSizePending: true` and a size → `'size-stale'` regardless of the
     global flag; hourglass-present logic.
   - `measure-column-widths.test.ts`: width reserves the icon slot when per-dir pending even if not globally indexing.
   - A `FullList`-level test if feasible asserting the hourglass renders for a pending entry with global indexing off
     (extend existing `FullList.a11y.test.ts` patterns, or a focused render test).

Run: `cd apps/desktop && pnpm vitest run -t "dir size"` (and the new test names), plus
`./scripts/check.sh --check oxfmt`.

### M4 — End-to-end confirmation + manual pass

1. Optional Playwright spec (see `apps/desktop/test/e2e-playwright/CLAUDE.md`): in a feature-flagged test dir, create a
   deep tree, delete it, and assert the hourglass appears on the affected dir and clears once indexing settles.
   Timing-sensitive — prefer asserting eventual clear via `expect.poll`, and lean on the deterministic unit tests for
   the "appears" case. If it proves flaky, keep it minimal or drop in favor of the unit coverage (note the gap
   explicitly per AGENTS.md "no silent caps").
2. **Manual verification** (the two things automation can't fully cover):
   - Performance under a real large delete: repeat something like the original `target/` wipe under `pnpm dev`, watch
     `stall_probe::writer` heartbeats and `enrich` timings in the logs, confirm no added latency to indexing throughput
     and the hourglass behaves.
   - Feel: the indicator reads as "hold on" and clears cleanly. Specifically check the lull-flicker case (does a brief
     quiet gap toggle the hourglass off then on jarringly?). If so, apply the clear-debounce fallback from the risks
     table.

### M5 — Docs + full checks

1. Update colocated docs:
   - `apps/desktop/src-tauri/src/indexing/CLAUDE.md` (if present): document the pending-sizes tracker, the drain-clear
     invariant, and the "global flag vs per-dir set" split. Add a Decision/Why entry.
   - `apps/desktop/src/lib/file-explorer/views/CLAUDE.md`: note the per-dir hourglass now also fires on
     `recursiveSizePending`, not just global indexing.
   - Add a Gotcha if any wrong assumption surfaced during implementation.
2. Full suite before declaring done: `./scripts/check.sh` (and `--include-slow` if the Playwright spec landed). Finish
   with `oxfmt`.

## Risks and mitigations

| Risk                                                                                                                                                                         | Severity                    | Mitigation                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Stuck hourglass (state leak)                                                                                                                                                 | would be high with counters | **Eliminated** by clear-on-drain — nothing to leak; self-heals every time the writer idles.                                                                                                                                                                                                                                                                                                                                                                  |
| Hot-path cost on reconciler/writer                                                                                                                                           | low                         | Pure in-memory string inserts from the already-computed `affected` list; ~5K/s peak. Confirm with the manual large-delete pass + heartbeat/enrich timings.                                                                                                                                                                                                                                                                                                   |
| Transient memory during a storm                                                                                                                                              | low                         | Set holds distinct ancestor dir paths until drain (tens of thousands of short strings, a few MB), freed on drain. Cappable if ever needed.                                                                                                                                                                                                                                                                                                                   |
| Skew between in-memory flag and SQLite size                                                                                                                                  | low                         | End-of-iteration clear + the final `EmitDirUpdated` refresh re-reads both → self-heals. Tests assert eventual state, not mid-storm exactness.                                                                                                                                                                                                                                                                                                                |
| Move ops touch two ancestor chains                                                                                                                                           | low                         | `collect_ancestor_paths` runs for both source and dest events; integration test covers it.                                                                                                                                                                                                                                                                                                                                                                   |
| `queue_depth` never reaching 0 under steady background load → lingering flags                                                                                                | low                         | Honest behavior (sizes genuinely in flux); logs show frequent drains in practice.                                                                                                                                                                                                                                                                                                                                                                            |
| **Lull flicker**: a quiet gap mid-operation (the observed ~90 s lull between waves) drives `queue_depth` to 0, clears the set, hourglasses vanish, then wave 2 re-flags them | medium (perception)         | This is _honest_ — at the lull the writer genuinely caught up and the sizes WERE momentarily settled. But it reads as "done!" then "wait, not done." Accept it as correct behavior; do NOT claim "no flicker." If the manual pass finds it jarring, the cheap fix is to debounce the **clear** (not the FE refresh) — keep flags for ~2–3 s after `queue_depth` hits 0 before clearing, so brief lulls don't toggle the indicator. Implement only if needed. |
| Forgot `bindings:regen`                                                                                                                                                      | low                         | `bindings-fresh` CI check fails loudly.                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Brief view shows dir sizes but no hourglass                                                                                                                                  | low                         | Verify Brief has no dir-size column; if it does, decide scope explicitly rather than silently skipping.                                                                                                                                                                                                                                                                                                                                                      |

### Can automated testing mitigate everything?

Correctness, yes: marking, clearing, eventual consistency after drain, move double-marking, and a hot-path overhead
guard are all unit/integration testable, and the drain-clear backstop is both tested and a runtime safety net for
anything tests miss. Two things still need one manual pass and cannot be fully automated: real-world performance under
an actual massive delete (FSEvents timing under load differs from the deterministic harness) and the subjective feel of
the indicator. That residual is small and expected.

## Parallelization

Mostly sequential by dependency (M2 needs M1; M3 needs M2's bindings). Within M3, the three FE edit sites (FullList
render, utils functions, width measurement) are independent and safe to do together. No worktrees needed — the
milestones are small.

## Resolved during review

- **Brief view**: confirmed FullList-only (the `size-stale`/dir-size column lives only in `FullList.svelte`). Brief
  carries the optional FE field for free; no hourglass there. No action.
- **Search-results view**: feeds static entries with no IPC enrichment, so `recursiveSizePending` stays falsy there by
  construction. Correct, no action.
- **`collect_ancestor_paths` returns parents only** (excludes the event path itself). That's fine: the dir whose
  recursive size changed IS an ancestor of the changed file, and `mark` re-expands ancestors anyway.

## Open questions

1. Should the global `indexing` flag eventually be retired in favor of pure per-dir pending (driven also by the
   scanner)? Out of scope here; the OR keeps both. Worth a follow-up note if the per-dir signal proves sufficient.
2. Firmlink normalization edge: confirm the paths in `pending_paths` and the `paths` arg to `get_dir_stats_batch`
   normalize to the same key via `firmlinks::normalize_path` (the tracker normalizes on both mark and read, so this
   should hold — but add a targeted test, since a mismatch silently misses).
