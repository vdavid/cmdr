# Indexing churn resilience: routing + ingestion fixes

Status: spec for implementation. Owner: David (via Claude). Follows the per-subtree rescan throttle
(`indexing/DETAILS.md` § "Per-subtree rescan throttle").

## Problem

On a high-churn boot disk (dev machine: `~/Library/Caches`, `node_modules`, cargo `target/`, DriveFS
`content_cache`, all indexed), macOS FSEvents drops fine-grained events and sets `MustScanSubDirs` on high paths, up
to `/`. Three coupled defects surface (all verified in code):

1. **Root-scale `MustScanSubDirs` takes the wrong path.** A shallow/`/` anchor goes through the *reconcile* drain
   (`reconciler.rs:378` → `reconcile_subtree`), which is **invisible** (no `Scanning` phase, no freshness update) and
   **holds the per-dir hourglass on the anchor for the whole ~20-min walk** (`rescan.rs:41`, released only at
   completion). Under continuous root churn the anchor stays in `pending_rescans`, so `release_rescan_hold` keeps
   skipping (`rescan.rs:196`) and the local-folder hourglasses never clear. The per-subtree throttle does NOT help here:
   a 60-s cap after a 20-min walk is noise. Meanwhile a *channel* overflow (same "we lost events" meaning) correctly
   routes to the **scanner** (`live.rs:179`, `manager.rs:570`): visible, updates freshness, single-flight, self-clearing.
   The two equivalent signals take divergent paths; that inconsistency is the bug.

2. **Ingestion backpressure cascades into a forced full scan.** The watcher→loop channel is a bounded tokio mpsc of
   `WATCHER_CHANNEL_CAPACITY = 20_000` (`event_loop.rs:64`, `manager.rs:381`). During a long replay the loop drains
   slower than FSEvents produces → the 20K buffer fills → `send().await` blocks the forward task (`watcher.rs:173`) →
   the upstream FSEvents stream's own buffer overflows and sets its flag → `WatcherChannelOverflow` → full-scan fallback
   (`replay.rs:456`). Measured: this fired at a 100M-event replay. So a slow drain, not a real data loss, throws away a
   working replay.

3. **Our replay-unification can route a root anchor to the invisible path.** Replay now hands all coalesced anchors to
   the reconcile drain; a replayed shallow/`/` `MustScanSubDirs` ancestor-collapses to one invisible reconcile-of-`/`
   with a stuck hourglass, where the old >1000 cap took the visible scanner path. Fix 1 subsumes this.

## Fix 1 — Depth-split routing + root-rescan cooldown (correctness; fixes 1 & 3)

Route `MustScanSubDirs` by anchor depth, in BOTH the live path (`process_live_event`) and the post-replay handoff
(`replay.rs`), i.e. wherever `queue_must_scan_sub_dirs` is called:

- **Shallow/root-scale anchor** (depth ≤ a small tunable const, default catches roughly the top 2–3 levels — e.g. `/`,
  `/Users`, `/Users/<me>`): route to the **scanner** (`start_scan`), the same battle-tested path channel-overflow uses.
  It's visible, updates freshness, single-flight (drops redundant → coalesces the storm), and FSEvents replay from the
  last event id catches interim changes. Do NOT take the per-dir reconcile hourglass hold for these.
- **Deep/narrow anchor** (a single `target/`): keep the current throttled `reconcile_subtree` path — that's what it's
  good at.
- **Root-rescan cooldown**: a min-interval between scanner rescans triggered this way (a named const, e.g. 30–60 s),
  so a machine that overflows `/` every few minutes doesn't scanner-rescan continuously. Within the cooldown, coalesce
  (drop) the redundant shallow demand; the single-flight guard + last-event-id replay make dropping safe. This is the
  one genuine staleness knob — bound it, document it.

Depth is a proxy; pick the const with a name + a `DETAILS.md` rationale, and cover the split with tests. Consider a
`RescanRoute::{Scanner, Reconcile}` classifier so the decision is one testable pure function.

## Fix 2 — Unbounded, fast-drained ingestion buffer (resilience; fixes 2)

Replace the bounded-20K channel's OS-overflow-driven fallback with a buffer we control:

- Make the watcher→loop channel **unbounded** (or a bounded channel fronted by an unbounded spill) so the forward task
  **never blocks** and the upstream FSEvents stream is never backpressured into dropping events. Ingestion is decoupled
  from processing.
- **Keep it small by draining fast** — the healthy state is <20K queued. Track queue depth; treat a sustained high
  watermark as a "falling behind" signal (log/metric), NOT a drop.
- **Bounded safety net, our decision not the OS's**: if the queue grows past a HIGH cap (RAM guard — we measured <900 MB
  at 100M distinct paths, so pick a cap well above normal but below OOM), THEN we choose a full-scan fallback. This
  replaces "OS dropped events → forced scan" with "we're hopelessly behind → deliberate scan", at a far higher
  threshold. Keep the existing `WatcherChannelOverflow` reason for the genuine upstream-drop case if it can still occur.
- Watch the memory-watchdog interaction (a global 16 GB watchdog stops indexing; the buffer cap must sit below it).

## Non-goals

- **No multi-core replay processing.** The writer is single-threaded by invariant (parallel writers → `SQLITE_BUSY`
  kills live indexing), and per-event reconcile ordering feeds the `dir_stats` delta ledger. Splitting per-event work
  across cores is error-prone for no real gain. The bottleneck (single-thread per-event CPU: double firmlink
  normalization + ancestor-string building + a per-event `symlink_metadata` stat + a cached SQLite read) is better
  attacked by coalesce-first (stat/normalize once per net path, not per raw event) and by release-build speed.
- **No adaptive large-gap replay yet** — that's the larger follow-up these fixes unblock. Fix 2 (no ingestion overflow)
  plus release speed is its prerequisite; raising `JOURNAL_GAP_THRESHOLD` is a separate, later change gated on release
  timings.

## Testing

- **TDD repros first**: (a) a continuously re-churning `/` anchor leaves the hourglass stuck under today's code (goes
  green when shallow anchors route to the scanner and don't hold); (b) a slow-drain replay no longer forces a full scan
  because the buffer absorbs the backlog without upstream overflow.
- Keep the per-subtree throttle tests green (deep anchors still throttle).
- Full `pnpm check` green before merge.
- **Release-build measurement** (separate agent): rebuild release, re-run the gap-replay bench (12M/30M/50M/100M) to get
  honest timings and confirm the buffer fix removes the 100M overflow → the adaptive-replay ceiling can then be set on
  data.
