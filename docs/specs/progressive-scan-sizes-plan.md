# Progressive folder sizes during full scan plan

## Problem

During a full drive scan (first onboarding, and every forced rescan), folder sizes don't exist until the very end:
`start_scan()` truncates `dir_stats`, the scanner only inserts entries, and `ComputeAllAggregates` runs once after the
walk completes. Every listing shows size placeholders for the whole scan (~2.5 min for a 5M-entry volume, longer on
spinning disks), and then all sizes pop in at once. That's exactly the moment a new user is evaluating the app's
headline feature, and the app looks like it's doing nothing.

David explicitly wants partial, growing sums shown during the scan — with the existing hourglass next to them, a partial
number beats a placeholder. (A separate FE-only change, merged before this work starts, converts the "Scanning…" cell
text to `<dir>` during full scan and reworks the top-right indicator. See "Integration with the concurrent FE change"
below.)

## Goal

While a full scan runs, periodically compute partial recursive sizes from what's been scanned so far and write them to
`dir_stats`, so visible listings refresh every few seconds with growing numbers + the hourglass. Sizes tick upward
("Library: 2 GB… 14 GB… 89 GB") until the final aggregation lands the exact totals, exactly as today.

Stability is the top requirement. This feature must be provably unable to corrupt final sizes — the design keeps all new
work inside the writer thread's serialized message stream, and the test plan's centerpiece is a differential test
proving the final DB state is byte-identical with and without partial passes.

### Non-goals

- No change to the final `ComputeAllAggregates` pass, replay, live mode, subtree scans, or verification. The feature is
  purely additive during the full-scan window.
- No new FE rendering states. The existing hourglass (gated on the global `indexing` flag in `FullList.svelte`) and the
  existing `index-dir-updated` → `refreshIndexSizes` refresh path carry everything.
- No persistence of partial sums beyond what already happens (scan cancellation already leaves partial data; the
  existing `scan_completed_at` mechanism already forces a fresh truncating rescan).
- No ETA / progress-percent changes to the scan overlay.

## Key discovery: most of the machinery already exists

The implementing agent should internalize this before writing code — the feature is mostly wiring, not new machinery.

1. **The writer thread already holds everything needed to compute partial aggregates in memory.**
   `writer.rs::AccumulatorMaps` accumulates `direct_stats` (parent_id → direct child sums) and `child_dirs` (parent_id →
   child dir IDs) as `InsertEntriesV2` batches commit. These are the exact two inputs
   `aggregator::compute_all_aggregates_with_maps` consumes at scan end. A partial pass is "run the same bottom-up
   compute over the maps as they stand right now, write a limited subset of rows."
2. **Maps and DB are consistent at every message boundary.** `handle_insert_entries_v2` accumulates only rows that
   actually landed (post-`INSERT OR IGNORE` filter), and each batch commits before the next message is processed. So
   when a `ComputePartialAggregates` message is dequeued, the in-memory maps describe exactly the committed DB state. No
   snapshotting, no locking, no torn reads — the single-writer serialization we already rely on everywhere.
3. **The dir list and parent relations don't need SQL.** Every scanned directory appears exactly once as a value in
   `child_dirs` (pushed when its own row was inserted under its parent), and the scan root is `ROOT_ID` (1). So the
   partial pass can build `(dir_id, parent_id)` pairs, depth, and the topological order entirely from the maps —
   skipping `load_all_directory_ids` (a full `entries` table scan that gets more expensive as the scan progresses).
4. **The bottom-up compute core is reusable as-is.** `aggregator::compute_bottom_up` + `topological_sort_bottom_up` take
   borrowed maps and return a `HashMap<i64, DirStatsById>`. They don't write; the caller decides what to write.
5. **The FE refresh path needs zero changes.** Emitting `index-dir-updated` with the `/` sentinel makes both panes
   refresh via `hasDescendantUpdate`'s short-circuit (`pane/index-events.ts`), throttled at 2 s per pane. The refresh
   calls `getDirStatsBatch`, which reads whatever `dir_stats` rows exist via `ReadPool` (WAL snapshot isolation — a
   reader never sees a torn write).
6. **The hourglass comes free.** `FullList.svelte` already renders the hourglass next to any directory size whenever the
   global `indexing` flag (`isScanning() || isAggregating()`) is true. Partial sizes mid-scan will automatically render
   as "number + hourglass" — the agreed honest presentation. Note the tail window: `index-scan-complete` sets
   `scanning = false` _before_ the final aggregation finishes, so it's `isAggregating()` (driven by the
   aggregation-progress events) that keeps the hourglass up between scan end and the final sizes landing. That already
   works today; just don't "simplify" the flag to `isScanning()` alone.
7. **A periodic task with the right cadence already exists.** `manager.rs::start_scan` spawns a progress reporter that
   ticks every 500 ms until `scan_done`. The partial-aggregation trigger is one more action on every Nth tick of that
   loop. It dies with the scan, so partial passes can't fire outside the full-scan window by construction.
8. **`queue_depth` already exists** (`Arc<AtomicUsize>` on `IndexWriter`) for backpressure decisions.

## Design

### New writer message: `ComputePartialAggregates`

```rust
/// Mid-scan: compute partial recursive sizes from the accumulator maps as
/// they stand, and write a bounded subset of dir_stats rows so visible
/// listings can show growing sizes during the scan. Borrows the maps
/// read-only; MUST NOT clear or mutate them (the final ComputeAllAggregates
/// depends on them). No SQL fallback when maps are empty: empty maps mid-scan
/// mean "nothing scanned yet", so the correct action is a no-op (unlike
/// ComputeAllAggregates, whose SQL fallback exists for the maps-lost edge case).
ComputePartialAggregates {
    /// Directories whose children should be written regardless of depth,
    /// because a pane is currently showing them ("hot" paths). Already
    /// firmlink-normalized by the sender.
    hot_paths: Vec<String>,
},
```

Handler (`writer.rs::handle_compute_partial_aggregates`, delegating the math to a new
`aggregator::compute_partial_aggregates`):

1. If `accumulator.direct_stats` is empty → return (log at debug). **No SQL fallback** — and this rule is load-bearing,
   not hygiene: the scanner thread sends `ComputeAllAggregates` _before_ the manager's completion handler sets
   `scan_done`, so the 500 ms progress reporter can race one last `ComputePartialAggregates` into the channel _after_
   the final aggregation. Channel ordering does NOT prevent that; what makes it safe is that the final pass clears the
   maps, so the late partial pass sees empty maps and no-ops. A SQL fallback here would overwrite the just-computed
   final `dir_stats` with a depth-capped partial subset. Document this in the handler's comment.
2. Build `dir_entries: Vec<(id, parent_id)>` from `child_dirs` (each child id under its parent key) + `(ROOT_ID, 0)`.
3. Compute each dir's depth from the in-memory parent relation (memoized walk). **`depth(ROOT_ID) = 0` must be the
   memo's explicit base case**, not a walk result — ROOT_ID's parent is the `0` sentinel, which is in no map, so a naive
   walk would assign the root `usize::MAX` and the most visible row (the `/` pane's `..`) would never get a partial
   total. Any other dir whose chain can't reach `ROOT_ID` (shouldn't happen — jwalk inserts parents before children —
   but cheap to guard) gets `depth = usize::MAX` so it's never written mid-scan. Safe: it just stays a placeholder until
   the final pass.
4. `topological_sort_bottom_up` + `compute_bottom_up` over **all** dirs (borrowed maps; this is the cheap part — pure
   in-memory iteration).
5. Select the write set: dirs with `depth <= PARTIAL_AGG_MAX_DEPTH`, **plus** for each hot path: resolve the path to an
   entry id (`store::resolve_path` on the writer's connection — fine here, partial passes run between committed batches
   so there's no open transaction), include that dir itself (feeds the `..` row) and its direct children from
   `child_dirs`. Unresolvable hot paths are skipped silently (the dir may not be scanned yet), as are hot paths
   resolving to non-directory entries (a symlink the scanner stored as a leaf). Note the depth guard and the hot-path
   branch are independent and don't conflict: `compute_bottom_up` gives every dir a correct subtree total regardless of
   its depth-to-root, so a hot-path write is always safe even for a dir the depth walk couldn't reach — the `usize::MAX`
   guard only trims the _depth-cap_ write set.
6. Write the selected rows via the existing `IndexStore::upsert_dir_stats_by_id` in chunks of 1000.
7. Emit `index-dir-updated { paths: ["/"] }` when `app_handle` is `Some` (tests spawn with `None` and assert DB rows
   instead). Use `reconciler::emit_dir_updated` (`pub(super)`, the same helper the writer's `EmitDirUpdated` arm uses;
   the manager's end-of-scan emit constructs the event directly — either is fine, the helper is closer). Emitting from
   inside the handler is correct by the same ordering argument as `EmitDirUpdated`: the writes just committed on this
   thread. `writer_loop` already wraps each message in `objc2::rc::autoreleasepool` on macOS, so the
   ObjC-on-background-thread rule is satisfied.
8. Log one info line: dirs computed, rows written, hot paths resolved, elapsed ms. This is the tuning signal for M4.
   Also add a `ComputePartialAggregates` arm to `WriterStats::record`'s match so passes show up named in the writer's
   periodic summary log instead of bucketing into "other".

Intent notes for the implementer:

- **Why inside the writer thread**: zero new concurrency. The pass sees a consistent prefix of the scan, can't race
  inserts, can't race the final aggregation, and can't run after `TruncateData` cleared the maps (any such ordering is
  resolved by channel order). Every historical index bug came from concurrent writers or forgotten propagation; this
  adds neither.
- **Why borrow, not consume**: `handle_compute_all_aggregates` clears the maps after use because nothing needs them
  afterward. The partial pass runs _before_ the final one, so consuming/mutating the maps would corrupt the final
  totals. This is the invariant the differential test pins.
- **Why depth-limited writes but full compute**: the compute is O(dirs-so-far) pure memory iteration (sub-second even at
  1M dirs). The _write_ is the expensive part (the final pass writes ~1M rows in seconds). Depth ≤ 3 on a typical macOS
  system is a few thousand rows; this keeps each pass's write cost trivial. And because the compute already produced
  every dir's total, the hot-path additions cost only their row writes.
- **Why hot paths ride the message instead of the writer reading pane state**: the writer must stay a pure
  message-processor (testability, no hidden global reads). The sender snapshots pane-visible paths at send time, which
  is also the correct semantics ("the panes as of this pass").
- **Memory**: `compute_bottom_up` allocates the computed map (~100 B/dir, tens of MB transient at 1M dirs) — the same
  allocation the final pass makes today — plus the `dir_entries: Vec<(i64, i64)>` and depth map built from `child_dirs`
  (another ~24 B/dir transient). All freed at end of pass; no new steady-state memory. Include these in M4's per-pass
  measurement. (The memory watchdog also covers pathological cases.)

### Sender: the existing scan progress reporter

In `manager.rs::start_scan`, the 500 ms progress loop gets a tick counter and a clone of `self.writer`. Every
`PARTIAL_AGG_TICK_INTERVAL`-th tick (10 ticks = 5 s), it:

1. Skips if `writer.queue_depth() > PARTIAL_AGG_MAX_QUEUE_DEPTH` (writer is busy catching up on insert backlog; partial
   sizes are a luxury, the scan is the job).
2. Collects hot paths: `file_system::listing::caching::snapshot_listings()` →
   `collect_hot_paths(&listings, scanned_volume_id)`. `ListingSummary` carries
   `{ listing_id, volume_id, path, entry_count, age_ms }` — there is **no volume root**, so the filter works off
   `volume_id`, not a path prefix: keep only entries whose `volume_id` **equals the volume being scanned** (the
   `IndexManager` knows its own `volume_id`; this excludes virtual/MTP/SMB listings AND other local volumes like
   `/Volumes/OtherDisk`, whose absolute-looking paths would otherwise be resolved against the wrong per-volume DB) and
   whose `path` is absolute (`PathBuf::is_absolute()`, belt-and-braces); then map through `firmlinks::normalize_path` so
   paths match the index's canonical form (`/tmp` → `/private/tmp` etc.), and dedup. Take the `snapshot_listings()`
   result first and drop the cache's read lock before normalizing — the snapshot is a cheap clone; don't hold a
   cross-subsystem lock through path work. The whole per-tick block (snapshot included) sits behind the
   `should_send_partial_agg` gate so ticks that skip do zero extra work — which also makes M4's feature-off A/B
   measurement honest (one call site to disable).
3. Sends via a new **`IndexWriter::try_send`** (wraps `SyncSender::try_send`). Non-blocking: if the channel is full, the
   pass is silently dropped — exactly the right behavior, and it guarantees this async task can never park on a full
   channel (the existing blocking `send` would, which is fine for the scanner's dedicated thread but not for a tokio
   task). **Trap to avoid**: `send` bumps `queue_depth` _before_ sending and undoes the bump on failure
   (`writer.rs::send`); `try_send` must do the same undo on `TrySendError::Full`, or `queue_depth` drifts upward
   permanently — which would break both the `PARTIAL_AGG_MAX_QUEUE_DEPTH` skip and the `queue_depth == 0` pending-sizes
   wholesale clear in `writer_loop`.

The loop already exits when `scan_done` is set (success, failure, or cancellation), so partial passes are structurally
scoped to the full-scan window. No partial messages during replay, subtree scans, or live mode — and the decision logic
("is it time, is the queue shallow") lives in a pure helper (`should_send_partial_agg(tick, queue_depth)`) so it's
unit-testable while the timer loop itself stays dumb, matching the existing testing bar in `manager.rs`.

New small accessors on `IndexWriter`: `queue_depth()` (read the existing atomic) and `try_send(msg)`.

### Constants (single block in `writer.rs` or `event_loop.rs`-style location, with rationale comments)

| Constant                      | Initial value | Rationale                                                                                                                              |
| ----------------------------- | ------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| `PARTIAL_AGG_TICK_INTERVAL`   | 10 (= 5 s)    | Matches FE's 2 s/pane refresh throttle (no wasted emits); ~30 reveals over a 2.5 min scan feels live without measurable scan slowdown. |
| `PARTIAL_AGG_MAX_DEPTH`       | 3             | Depth from scan root: `/Users` =1, `/Users/david` =2, `~/Downloads` =3. Covers onboarding browsing; thousands of rows, not 100K+.      |
| `PARTIAL_AGG_MAX_QUEUE_DEPTH` | 4 000         | ~20% of channel capacity. Deep backlog means the writer is the bottleneck; don't add work. Tuned with M4 measurements.                 |

These are starting points; M4 (manual verification on a real volume) measures per-pass cost from the handler's log line
and adjusts. Capture final numbers + measurements in the constants' comments.

### Correctness invariants (the "super stable" contract)

1. **Partial passes never change the final state.** With identical inserts, final `dir_stats` after
   `ComputeAllAggregates` is byte-identical whether 0 or N partial passes ran in between. Pinned by the differential
   test (M1). The mechanism: the handler takes `&AccumulatorMaps`, and the final pass overwrites every row the partial
   passes could have touched (it writes ALL dirs).
2. **Partial sums never overcount.** They derive from the same post-commit accumulator the final pass uses (which
   already filters `INSERT OR IGNORE` losers). They're undercounts by construction (subtrees still being walked).
3. **No partial pass outside a full scan.** Sender lives in the scan progress loop only. (The handler is additionally
   harmless if ever misdelivered: empty maps → no-op. That no-op is also what makes the end-of-scan race safe — see
   handler step 1 — since one last partial message can legitimately land _after_ `ComputeAllAggregates`.) During the
   scan window the only other writer traffic is `InsertEntriesV2` (scanner), `Flush` (manager), and the 30 s maintenance
   timer's `IncrementalVacuum` + `WalCheckpoint` (`state.rs`) — none touch the accumulator; the reconciler buffers
   FSEvents until after the scan and the verifier only runs when `scanning` is false.
4. **Cancellation safety unchanged.** A cancelled scan leaves partial `dir_stats` rows + no `scan_completed_at` — the
   same state the existing "incomplete previous scan → fresh truncating rescan" path already handles. Note: this was
   already true before this feature (the final aggregation also leaves rows if cancellation lands between aggregation
   and meta-write); we add no new variant of it.
5. **The sender can never block.** `try_send` only; a full channel drops the pass.
6. **Readers never see torn rows.** WAL snapshot isolation per `upsert_dir_stats_by_id` chunk transaction; a reader sees
   row sets from committed chunks only — and "some dirs updated, some not yet" is the normal, accepted state of this
   feature anyway.

### Integration with the concurrent FE change

Another agent is (FE-only) changing the mid-scan size cell from "Scanning…" to `<dir>` and reworking the top-right scan
indicator; it merges to `main` before this work starts. The pre-change code is already correct for us:
`full-list-utils.ts::getDirSizeDisplayState` prefers a non-null `recursiveSize` (rendering size + hourglass,
`'size-stale'`) over the mid-scan placeholder, which only shows when the size is null. After rebasing onto their change,
verify that property survived: **the placeholder must show only when `recursive_size` is null, not whenever
`isScanning()`**. If their change regressed the gate (unlikely but cheap to check), fix it in this branch (a one-line
condition) and add a case pinning "non-null recursive size renders during scan" to the existing
`views/dir-size-display.test.ts`.

No other FE work is expected. Manual verification (M4) confirms the full loop.

## TDD milestones

Strict red-green: every behavior lands as a failing test first, then the implementation. Rust message-enum changes need
a compile-able stub to get a _red_ (not non-compiling) state: add the variant + a no-op handler in the same commit as
the first failing test, then implement.

### M1 — Writer-side partial aggregation (the core, fully TDD)

All tests in `writer.rs::tests` / `aggregator.rs::tests` / `mod.rs::tests` style: real SQLite in tempdirs, real
`IndexWriter`, synthetic `EntryRow` batches (reuse
`stress_test_helpers::{setup_writer, build_synthetic_tree, check_db_consistency}` where they fit; note `make_file_entry`
builds a `FileEntry` for enrichment tests — M3.3's territory — not an `EntryRow` for writer batches).

Test sequence (each red → green):

1. **No-op on empty maps**: fresh writer, send `ComputePartialAggregates { hot_paths: vec![] }` + flush → `dir_stats`
   stays empty, `mutation_count()` unchanged (partial passes are not "mutations" for search-staleness purposes — they
   don't change what entries exist; decide and pin this here). Assert the counter as a **before/after delta on this one
   writer** with nothing else sending to it — never as an absolute value, and never via the global `WRITER_GENERATION`
   (other tests' writers run as threads in the same process; see the CLAUDE.md gotcha).
2. **Partial sums at shallow depth**: insert a 3-level synthetic tree in two batches, send partial-agg after batch 1 +
   flush → `dir_stats` rows exist for depth ≤ MAX_DEPTH dirs with sums covering batch-1 contents only; deeper dirs have
   no rows. Send after batch 2 → sums grow accordingly.
3. **Depth limiting**: dirs at depth MAX_DEPTH+1 get no rows from partial passes (and DO get rows from the final pass).
4. **Hot paths punch through the depth limit**: a deep dir listed in `hot_paths` gets its own row + its direct
   children's rows; an unresolvable hot path is skipped without error.
5. **The differential test (centerpiece)**: same synthetic tree, several thousand entries. Note `build_synthetic_tree`
   produces only plain dirs/files (`is_symlink: false`, `inode: None`) — extend it (or add a sibling helper) to inject
   symlink rows and hardlink pairs (second link inserted with `logical_size: None`, matching the scanner's
   dedup-at-insert convention, which keeps the size oracle valid). Two writers/DBs: (a) inserts →
   `ComputeAllAggregates`; (b) same inserts with `ComputePartialAggregates` interleaved every batch (with hot paths) →
   `ComputeAllAggregates`. Both assertions run **after the final aggregation + a `flush`**, comparing value columns (not
   rowids). Two oracles, in priority order:
   - **Primary: `check_db_consistency` on the partial-pass arm (b).** It recomputes every dir's stats independently from
     the `entries` table, bottom-up — a ground-truth oracle that doesn't share state with the code under test. This is
     what actually catches the nightmare bug class: if the partial handler corrupts the shared `AccumulatorMaps`, the
     final pass in _both_ arms uses corrupted maps, so an (a)==(b) comparison alone would pass green while both DBs are
     identically wrong. Caveat: today's `check_db_consistency` validates sizes/counts but not `recursive_has_symlinks`,
     and models no hardlink dedup (fine, since dedup is encoded at insert via `logical_size: None`) — add the
     symlink-flag assertion as a **separate helper** (for example `check_recursive_has_symlinks`) called by this test,
     rather than editing `check_db_consistency` in place: that helper is shared by every stress test and perturbing it
     has a wide blast radius. Alternatively accept that flag divergence is covered only by the secondary oracle and say
     so in the test comment.
   - **Secondary: full `dir_stats` of (a) == (b) row-for-row** (catches subtler divergences like leftover rows the final
     pass didn't cover, and the `recursive_has_symlinks` column if the primary oracle wasn't extended). To verify the
     test has teeth before trusting green: locally make the handler deliberately wrong (mutate the maps) and watch the
     _primary_ oracle fail — don't commit that.
6. **Idempotence**: two consecutive partial passes with no inserts between → identical rows after each (compare table
   snapshots).
7. **Ordering vs TruncateData**: TruncateData → partial-agg → no rows (maps cleared); inserts → TruncateData →
   partial-agg → no rows. Guards the rescan-start window.

Implementation:
`aggregator::compute_partial_aggregates(conn, direct_stats, child_dirs, hot_paths, max_depth) -> Result<u64>` (rows
written) + thin `writer.rs` handler (match arm + emit + log). Reuse `compute_bottom_up` / `topological_sort_bottom_up`;
the new code is dir-list derivation from maps, depth computation, write-set selection.

Checks: `pnpm check --fast` per green; full `cargo nextest run indexing` at milestone end.

### M2 — `try_send`, `queue_depth()`, and the send-decision helper

1. Test: `try_send` returns a distinguishable "dropped" outcome without blocking (pick the shape that reads cleanest
   against `send`'s `Result<(), IndexStoreError>` — likely `Result<bool>` or a tiny enum), and `queue_depth()` reflects
   sends/recvs (existing atomic, just exposed). Feasibility note: `IndexWriter::spawn` always starts a draining writer
   thread, so deterministically filling the 20K channel is awkward. Either test the Full path on a raw `sync_channel`
   via a small extracted helper (the bump/undo logic around `SyncSender::try_send` is the part worth pinning, including
   the `queue_depth` decrement on `Full`), or downgrade this test to the success path + depth accounting and let M2.2's
   pure truth table carry the decision coverage. Don't build channel-stalling machinery just for this.
2. Test (pure): `should_send_partial_agg(tick, queue_depth)` truth table — fires on the interval, skips under backlog,
   never on tick 0.
3. Implement both on `IndexWriter` + the pure helper (in `manager.rs` or a small `partial_agg.rs` next to it).

### M3 — Manager wiring + hot-path collection

1. Test (pure): hot-path collection helper
   `collect_hot_paths(listings: &[ListingSummary], scanned_volume_id: &str) -> Vec<String>`: keeps entries whose
   `volume_id` equals the scanned volume's (which by construction drops `network` / `search-results` / `mtp-*` listings,
   SMB shares, and other local volumes) and whose `path` is absolute, applies `firmlinks::normalize_path`, dedups. Feed
   it synthetic `ListingSummary` values, including a same-path-different-volume case. (Mind the types:
   `ListingSummary.path` is a `PathBuf` — use `path.is_absolute()` for the filter, then `to_string_lossy()` to feed
   `normalize_path(&str)`.)
2. Wire the progress loop in `manager.rs::start_scan`: tick counter, `should_send_partial_agg`, `collect_hot_paths`
   (from `snapshot_listings()`), `try_send`. The loop body stays dumb — all logic is in the two tested helpers. (The
   timer loop itself stays under integration/manual coverage, matching the existing bar for this loop.)
3. Integration test (`mod.rs::tests` level, no Tauri needed since `app_handle` is `Option`): writer over a tempdir tree,
   insert batches and send partial-agg messages from the test thread (deterministic — each `flush` is a barrier; this
   _simulates_ mid-scan rather than racing a live scanner thread, which is the right trade for a non-flaky test), assert
   enrichment (`enrich_entries_with_index`) returns growing non-null `recursive_size` for a top-level dir before
   `ComputeAllAggregates` lands. This proves the read path sees partial rows mid-scan. Mechanics:
   `enrich_entries_with_index` reads the process-global `READ_POOL`, so the test must install it and serialize on
   `READ_POOL_TEST_MUTEX` — copy the existing pattern in `mod.rs::tests` (without the install, enrichment silently
   no-ops and the test asserts nothing).

Checks: full `pnpm check` (clippy, Rust tests, the works).

### M4 — Real-volume verification + tuning + FE integration check

Not skippable; this is where the constants earn their values. On the dev instance (`pnpm dev` in this worktree with
`--worktree progressive-scan-sizes`):

1. Rebase onto main (the other agent's FE change must be in). Do the `getDirSizeDisplayState` integration check from
   "Integration with the concurrent FE change"; add the pinning Vitest case if the gate needed fixing.
2. Force a full scan (debug window / `force_scan`) on the real volume. Via the MCP servers (not the browser): watch a
   pane on `/` and one on `~` — assert sizes appear within ~2 FE throttle windows of the first partial pass, grow over
   time, show the hourglass, and settle to exact values at scan end.
3. Read the handler's per-pass log line over the scan: per-pass elapsed ms, rows written. Budget: p95 pass < 500 ms on a
   ~5M-entry volume. If exceeded: lower MAX_DEPTH, raise the tick interval, or (if compute dominates, unlikely)
   reconsider. Also sanity-check total scan duration vs a baseline run with the feature off (toggle: disable the one
   gated per-tick block — collection AND send together, so the A/B isolates the feature's full cost) — budget ≤ +5%.
4. Cancel a scan mid-way → confirm next startup does the incomplete-scan fresh rescan and the UI never shows stale
   partials as settled (hourglass gone only after the _new_ scan's final aggregation).
5. Record measurements in the constants' rationale comments (and `docs/notes/` if the data is rich enough to warrant
   it).

### M5 — Docs + full checks + wrap

1. Update `apps/desktop/src-tauri/src/indexing/CLAUDE.md`: module-structure entries (writer, aggregator, manager),
   data-flow diagram (partial passes during "Full scan" stage), a Key decision entry ("Progressive partial aggregation
   during full scan": the why — onboarding UX; the how — borrow-only, depth-limited writes, try_send, no SQL fallback;
   the don'ts — don't consume the maps, don't make the sender blocking, don't add partial passes outside the scan loop),
   and a Gotcha for the maps-borrow invariant pointing at the differential test.
2. `pnpm check` full suite, then `--include-slow` (e2e-linux runs scans with `CMDR_E2E_START_PATH`; partial passes
   mostly won't fire there — tiny fixture scans finish before tick 10 — but the suite proves no regression). `oxfmt`
   runs as part of the full suite; confirm regardless.
3. Commit(s) on the worktree branch per git-conventions; FF-merge to local `main` only when David says so.

## Risks and mitigations

| Risk                                                                        | Mitigation                                                                                                                                                                                                                                                                                                                          |
| --------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Partial pass corrupts final totals (the nightmare)                          | Borrow-only maps + differential test (M1.5) + idempotence test. Verified to have teeth before trusting green.                                                                                                                                                                                                                       |
| Per-pass cost slows the scan                                                | Full compute is in-memory; writes depth-limited; queue-depth skip; M4 measures real cost with a ≤ +5% scan-duration budget.                                                                                                                                                                                                         |
| Sender blocks a tokio worker on a full channel                              | `try_send` only — structurally impossible.                                                                                                                                                                                                                                                                                          |
| Growing numbers read as wrong data                                          | Hourglass already renders for all dir sizes while `indexing` is true; this was David's explicit call ("partial + hourglass beats placeholder").                                                                                                                                                                                     |
| Concurrent FE change hides partial sizes behind the new `<dir>` placeholder | Explicit rebase-time integration check + pinning Vitest case (M4.1).                                                                                                                                                                                                                                                                |
| Hot paths point at unscanned/excluded dirs                                  | `resolve_path` miss → skip silently; next pass retries. No error states.                                                                                                                                                                                                                                                            |
| Emit storm                                                                  | One emit per pass (≥ 5 s apart) + FE 2 s/pane throttle.                                                                                                                                                                                                                                                                             |
| E2E-restricted scans (`CMDR_E2E_START_PATH`) get no partial rows            | In restricted scans the scan root is still `/` (ROOT_ID), so fixture dirs sit at their true depth from `/` — typically deeper than MAX_DEPTH. Acceptable: fixture scans rarely reach tick 10 anyway, hot paths still punch through, and no E2E test asserts partial rows. Covered by the slow-lane suite run proving no regression. |

## Parallelization

Sequential is fine (and preferred — M2 builds on M1's message shape, M3 on M2's helpers). No parallel agents, no nested
worktrees. The only external dependency is the other agent's FE change merging before M4.

## Resolved during review

- **R1 (review round 1, blocker)**: `collect_hot_paths` originally took a `volume_root` parameter, but `ListingSummary`
  carries no volume root. Reframed to filter on `volume_id` (drop virtual/MTP ids) + absolute path.
- **R1 (blocker)**: the differential test's primary oracle is now `check_db_consistency`'s independent
  recompute-from-`entries` on the partial-pass arm, not the (a)==(b) comparison — the latter alone passes green if a
  maps-corruption bug poisons both arms identically. (a)==(b) demoted to secondary oracle.
- **R1 (important)**: E2E-restricted scans keep `/` as the scan root, so fixture dirs sit at true depth from `/` — the
  risk table no longer claims the depth cap covers them; hot paths are the mechanism there.
- **R1 (important)**: `try_send` must undo the `queue_depth` bump on `TrySendError::Full` (mirroring `send`'s error-path
  undo), or the depth metric drifts and breaks both the skip logic and the pending-sizes clear.
- **R1 (minor)**: M1.1's `mutation_count` assertion specified as a per-writer before/after delta; FE integration section
  corrected — the current `getDirSizeDisplayState` already prefers non-null sizes mid-scan, so M4.1 is a regression
  check, not a fix; `WriterStats::record` gets a named arm for the new message.
- **R2 (review round 2, important)**: the "no partial pass after the final aggregation" claim was wrong — the scanner
  sends `ComputeAllAggregates` before `scan_done` is set, so one last partial message CAN land after it. Safety comes
  from the final pass clearing the maps + the handler's empty-maps no-op; the "no SQL fallback" rule is load-bearing for
  exactly this interleaving and the plan now says so.
- **R2 (important)**: `build_synthetic_tree` emits no symlinks/hardlinks and `check_db_consistency` doesn't validate
  `recursive_has_symlinks` — M1.5 now spells out extending the helper (hardlink second-link = `logical_size: None`) and
  either extending the oracle or relying on the secondary (a)==(b) comparison for the symlink flag.
- **R3 (review round 3, important)**: `depth(ROOT_ID) = 0` must be the memo base case (its parent is the `0` sentinel,
  in no map — a naive walk would exclude the `/` row, the most visible number). And `collect_hot_paths` regained a
  parameter: it must match listings' `volume_id` against the **scanned** volume's id — "absolute path" alone admits
  other local volumes (`/Volumes/OtherDisk`) and SMB mounts whose paths would resolve against the wrong per-volume DB.
- **R3 (minor)**: symlink-flag oracle goes in a separate helper, not into the shared `check_db_consistency`;
  `isAggregating()` is what bridges the hourglass between `index-scan-complete` and final sizes (credit corrected); the
  per-tick hot-path snapshot drops the listing-cache lock before normalizing and the whole block sits behind the send
  gate (also makes M4's A/B honest); search staleness confirmed safe without bumping `WRITER_GENERATION` (partial passes
  change no `entries` rows; search reload checks the generation only at next dialog open).
- **R2 (minor)**: `ListingSummary.path` is a `PathBuf` (`is_absolute()`, not a string prefix test); `pickSizeDisplay` →
  `getDirSizeDisplayState` (the former is the TCC override, the latter is the mid-scan gate); M3.3 must install the
  global `READ_POOL` + serialize on `READ_POOL_TEST_MUTEX` and is a simulated (flush-barrier) mid-scan, not a live
  scanner race; the 30 s `IncrementalVacuum`/`WalCheckpoint` maintenance timer added to the scan-window interleaving
  inventory (benign — doesn't touch the accumulator).

- **R4 (review round 4, minor)**: M2.1's full-channel `try_send` test gets a feasibility note (no test-only constructor
  exists to wedge the drain; pin the bump/undo logic on a raw `sync_channel` or downgrade to the success path);
  `make_file_entry` builds a `FileEntry` (M3.3 enrichment), not an `EntryRow` (M1 batches). Round 4 verified and
  cleared: single global `IndexManager` (one writer, one volume, no concurrent-scan story needed), dup-free
  `dir_entries` derivation, `topological_sort_bottom_up`'s tolerance of detached parents (proptest-pinned), the depth ≤
  3 row-count estimate (low-thousands to ~10K — fine), no open transaction across partial passes (savepoints release per
  batch; explicit txns are replay-only), and first-reveal latency (FE throttle fires immediately on the first event; the
  2 s cooldown only gates repeats).

## Open questions (resolve during implementation, defaults stated)

- `mutation_count()` semantics for partial passes: default **don't bump** (search staleness cares about entry existence,
  and dir_stats writes during scan are transient) — pinned by M1.1 either way.
- `try_send` return shape: pick whatever reads cleanest against `send`'s `Result<(), IndexStoreError>`; the test pins
  behavior (non-blocking, distinguishable "dropped" outcome), not the exact type.
- Whether hot paths also include the _parents_ of pane paths (for the pane showing the hot dir's parent listing): not
  needed — the hot dir's own row is written, and the parent pane's listing reads the hot dir's row, which is included.
