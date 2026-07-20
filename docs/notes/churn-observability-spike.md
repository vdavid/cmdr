# Churn observability spike (Spike B)

Read-only instrumentation on the live FSEvents loop that records per-subtree churn rolled up the ancestor chain, plus
the offline analysis that turns a collection window into the three answers Spike B owes
[`docs/specs/sealed-subtrees-plan.md`](../specs/sealed-subtrees-plan.md):

1. How fast a hard-churning subtree (DriveFS `fetch_temp`, a `target/` during a build) separates from ordinary
   background filesystem noise.
2. Whether a real ancestor chain shows a ratio-drop boundary where a uniformly-churny subtree meets a directory holding
   things worth keeping (the plan's seal-root rule currently rests on an invented example).
3. What seal-fast / unseal-slow hysteresis windows the real data suggests.

The instrumentation writes no index state, sends no writer messages, and changes no behaviour. It is off unless
`CMDR_CHURN_SPIKE` is set, so a normal run pays nothing.

## Collect

Run the app with the spike on, from this worktree:

```sh
CMDR_CHURN_SPIKE=1 pnpm dev --worktree sealed-subtrees
```

Then leave it alone for the window (~4 h). Do normal work meanwhile: a `cargo build` or two make the `target/` case show
up, and DriveFS churns on its own.

Knobs (all optional):

- `CMDR_CHURN_SPIKE_PERIOD_S` — rollup period in seconds, default `30`.
- `CMDR_CHURN_SPIKE_TOP_N` — directories emitted per period, default `40`.

**Collection only starts when the live event loop does**, which is after the initial scan plus reconciliation. Confirm
with:

```sh
grep -c churn_period ~/Library/Application\ Support/com.veszelovszki.cmdr-dev-sealed-subtrees/logs/cmdr.log
```

A count that grows every period means it's running. Zero means the app is still scanning (look for
`Live event processing: started` in the same file). A `churn_spike_enabled` line confirms the env gate took.

The `dev-sealed-subtrees` data dir already holds a completed scan (a verification run on 2026-07-20 built it), so a
relaunch reaches live mode in a couple of minutes rather than re-scanning `/`. Don't delete it before the collection.

### If no `churn_period` lines appear

Churn only flows while a **live event loop** is running. There is no live loop during a scan or rescan, so the spike is
silent then — that is the instrument being honest, not broken. Three checks tell you which state you're in, over
`L=~/Library/Application\ Support/com.veszelovszki.cmdr-dev-sealed-subtrees/logs/cmdr.log`:

- `Live event processing: started` or `Replay: switching to live mode` — a live loop was entered.
- `routing shallow anchor` and `local scan:` — a scan superseded it.
- `Replay event loop: stopped` — the post-replay live loop exited.

The trap, seen for real on 2026-07-20: a cold start with a journal gap can replay a `MustScanSubDirs` event whose anchor
is shallow (`/`, `/System`). `reconciler/rescan.rs` routes a shallow anchor to the **visible scanner**, which stops the
replay watcher, ends the post-replay live loop within seconds, and starts a full reconcile rescan of the volume. That
rescan runs for many minutes, and no churn is recorded until it completes and live mode restarts. The log says so
plainly (`routing shallow anchor /System to the visible scanner` → `Replay event loop: stopped` →
`local scan: reconcile rescan`), but only if you look for it.

**So don't restart to "fix" a silent spike.** Two reasons: a restart replays the journal again and can draw the same
shallow anchor, and killing the app _during_ a scan makes the next launch do a full fresh scan from scratch
(`local scan: fresh scan (truncate) … (incomplete previous scan)`). Check the three signals above first; if a scan is
running, wait it out.

The log dir is per instance: `~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>/logs/`. Files rotate at 50
MB (`cmdr.log`, `cmdr.log.1`, …), so pass the glob to the analyser, not just the live file.

Volume of output: two line kinds, `1 + top_n` lines per period per volume. At the defaults that's ~82 lines/minute, a
few MB over four hours.

## What is measured

Every flush tick (1 s) the live loop hands the churn monitor the batch of paths it just deduplicated, plus the count of
raw pre-dedup events. For each path, the monitor credits its containing directory and **every ancestor up to `/`**:

- `events` — rolled-up: every change at or under this directory. An ancestor's count is always ≥ its child's.
- `direct` — changes whose containing directory is exactly this one.
- `children` — how many distinct direct children churned, capped at 128 exact (`capped=1` means "≥128"; use `direct` for
  magnitude past that). This is what separates one log file rewritten 500× from 500 distinct temp files.

Per period the monitor emits the top `top_n` directories ranked by `events` desc, then path asc. Ranking by a rolled-up
count means a hot directory's **entire ancestor chain always ranks at or above it**, so a top-N cut never truncates a
chain in the middle — that's what makes question 2 answerable from the emitted subset alone.

Bounds and their failure modes, all reported per period so you can tell when the instrument (rather than the machine) is
the limiting factor:

- **Memory**: nothing survives a period. The map is cleared and shrunk at each rollup, so the ceiling is one period's
  distinct directories, hard-capped at 10,000 (`nodes_dropped` counts insertions refused past the cap; the walk is
  root-first, so what survives is the shallow end of every chain and the rolled-up totals stay honest).
- **Depth**: a chain is walked at most 40 levels (`deep_truncated` counts paths cut).
- **Cost**: aggregation runs once per flush tick, never per event. No locks, no per-event allocation, no per-event
  logging.

Two caveats worth remembering when reading the numbers:

- The input is the loop's **deduplicated** batch, so one file touched 40× in a second counts once. `raw_events` on the
  period line gives the dedup ratio.
- A period closes on the next live batch after the configured length elapses, and under a heavy drain the event loop can
  starve its own flush tick for tens of seconds. So periods stretch (`period_ms=43762` was observed right after a replay
  handoff). That's why `period_ms` is measured rather than assumed, and why the analyser derives its timings from it:
  rates stay honest, the time base just gets coarser when the machine is busiest.

## Log format

Target `indexing::churn`, level Debug, so both lines always reach the log file. `path` is always the **last** field on a
node line: paths contain spaces and `=`, so a parser takes the rest of the line verbatim. Node lines join to their
period line by `(vol, t_ms)`, which survives log rotation, interleaved volumes, and app restarts (`seq` restarts at 0
per live loop, so don't key on it).

```
<ISO ts> DEBUG indexing::churn  churn_period seq=3 t_ms=1784541600000 vol=root period_ms=30001 raw_events=4123 batch_paths=1402 nodes=317 nodes_dropped=0 deep_truncated=0 emitted=40
<ISO ts> DEBUG indexing::churn  churn_node seq=3 t_ms=1784541600000 vol=root events=900 direct=900 children=128 capped=1 path=/Users/me/Library/Containers/com.google.drivefs.fpext/Data/tmp/domain-temp-gdrive-0/fetch_temp
```

Fields:

- `seq`: period number within one live-loop run (restarts at 0).
- `t_ms`: wall-clock milliseconds, identical on a period line and its node lines.
- `vol`: volume id (`root`, or an external drive's id).
- `period_ms`: measured period length, not the configured one.
- `raw_events` / `batch_paths`: events before / after per-path dedup.
- `nodes` / `nodes_dropped`: directories tracked / refused past the 10,000 cap.
- `deep_truncated`: paths cut at 40 levels.
- `emitted`: node lines that follow.
- `events` / `direct`: rolled-up / own-directory change count.
- `children` / `capped`: distinct churny direct children; `capped=1` means the count is a floor.

## Analyse

```sh
cd scripts/churn-analysis
go run . -csv /tmp/churn.csv ~/Library/Application\ Support/com.veszelovszki.cmdr-dev-sealed-subtrees/logs/cmdr.log*
```

That prints all three answers per volume and (with `-csv`) writes the full per-node time series for plotting.

Flags:

- `-factor` (default 10) — a directory is "hot" in a period when its rolled-up `events` reach this multiple of the
  period's **average per-directory churn** (`batch_paths / nodes`), the baseline "what an ordinary background directory
  looks like right now".
- `-min-peak` (default 20) — ignore directories whose best period never reached this many events.
- `-sustained` (default 2) — consecutive hot periods before a subtree counts as separated (question 1).
- `-chains` (default 5) — how many hot chains to print for question 2.
- `-vol` — restrict to one volume id.

### Reading the output

- **Q1 section** — one row per hot directory: total, peak and median events per period, how many periods it was hot, and
  `SEPARATED`, the time from its first appearance to a sustained hot run. **That column is the answer to question 1.**
  Resolution is one period, so a `30s` reading means "within the first period", not "in exactly 30 seconds".
- **Q2 section** — for each hot leaf, its chain from `/` down with `SHARE` = this directory's churn as a fraction of its
  parent's. A share near 1.0 means the parent's churn comes entirely from this child; a low share is the **ratio drop**,
  the boundary where a churny subtree hangs off a directory that also holds quiet content. The steepest drop is called
  out per chain. **If real chains show no drop above the root, the plan's seal-root rule needs rethinking** — that is
  the finding to look for, positive or negative.
- **Q3 section** — hot-run and quiet-gap length distributions per directory, then across all of them. Seal-fast should
  fire well inside the p50 hot run; unseal-slow has to exceed the p90 quiet gap, or a genuinely churny subtree unseals
  and re-seals on its own idle pauses. **Those two numbers are the hysteresis constants.**

The CSV is one row per (period, directory):
`t_ms,iso,vol,seq,period_ms,raw_events,batch_paths,nodes,events,direct,children,capped,path`, with `path` last so a
comma in a path can't shift columns.

## Where the code lives

- `apps/desktop/src-tauri/src/indexing/churn_monitor.rs` — the aggregator, pure and clock-injected (same shape as
  `reconciler/rescan_throttle.rs`), with its unit tests in `churn_monitor/tests.rs`.
- `apps/desktop/src-tauri/src/indexing/event_loop/live.rs` — the single call site, inside `process_live_batch`, before
  the batch drains. It lives there rather than at a loop's flush tick because **there are two live loops**: `live.rs`'s
  `run_live_event_loop` (post-scan) and `replay.rs` Phase 3 (post-journal-replay, the cold-start route). Both funnel
  through `process_live_batch`, which takes a `ChurnObserver` by `&mut`, so the hook is compiler-enforced at every live
  batch. `churn_monitor/tests.rs::every_live_loop_owns_a_real_churn_observer` catches a third loop appearing.
- `scripts/churn-analysis/` — the offline analyser.

The aggregator is designed to be promoted into the sealed-subtrees churn accounting rather than deleted: only the sink
changes (a decision instead of a log line), and the ancestor rollup plus the distinct-children signal are exactly what
`pick_seal_root` needs.

## Results (2026-07-20)

Collected on David's machine, 11:30–12:12, 31 periods spanning 42 minutes: 71,562 raw events, 42,207 deduplicated paths
(1.7× dedup), peak 1,683 directories tracked in a period. No instrument cap engaged (`nodes_dropped=0`,
`deep_truncated=0`), so the numbers are the machine's behaviour, not the tool's limits.

### Q1. Separation is fast, so the seed list is genuinely unnecessary

A node counts as hot at 10× the period's average per-directory churn. Time from first appearance to two consecutive hot
periods:

- `…/domain-temp-gdrive-…/fetch_temp`: **10 s** (one period).
- `…/worktrees/sealed-subtrees/target` and `target/debug`: **31 s**.
- `~/Library/Caches/cmdr/WebKit/NetworkCache/…`: 1 m 18 s.

Every motivating case separates within a minute. That settles the open question behind Decision 4: a churn classifier
converges fast enough to stand alone, and provisional-seal-on-size only has to cover the first scan, not a long learning
period.

### Q2. The boundary is real, but the ratio-drop rule as specified picks the wrong node

The good news: uniform churn along a chain is clearly visible. Below `com.google.drivefs.fpext` the share is exactly
1.000 at every level down to `fetch_temp` (0.888), and below `Library/Caches/cmdr` it is 0.999–1.000 down to `Resource`.
Sealable subtrees really do announce themselves as a run of ~1.0 shares.

The bad news, and the main finding of this spike: **"climb while uniformly churny, stop at the first ratio drop"
over-climbs.** Applied to the real chains it selects:

- `~/Library/Containers` for `fetch_temp` (share 0.369 entering it, 0.971 from `fpext` into it)
- `~/Library/Caches` for the WebKit cache (share 0.408)

Both are far too high. Sealing `~/Library/Containers` would seal every app's container; `~/Library/Caches`, every app's
cache. The rule fails because `fpext`→`Containers` reads 0.971 — indistinguishable from "uniformly churny" — only
because the other ~40 containers happened to be quiet during the window. Churn share alone cannot separate "this parent
is entirely churny" from "this parent's churn is dominated by one child right now".

**What the rule needs.** The plan already says the ratio should be measured "by descendant count and by bytes"; only the
churn dimension was implemented here, and the miss is exactly the omitted dimension. A parent should block the climb
when it holds substantial _quiet content_, not merely when its churn share drops. `~/Library/Containers` holds dozens of
other apps' data that never churned; `fpext/Data/tmp` holds essentially nothing else. Phase B must combine churn share
with a content ratio (entries and/or bytes below the candidate versus below its parent), and the hard-stop list should
include `~/Library/Containers` and `~/Library/Caches` as belt-and-braces.

Without this correction, Phase B would have shipped a rule that seals a user's entire container or cache tree the first
time one app inside it churns.

### Q3. Under-answered, and the reason is itself a finding

The intended 4-hour window produced 42 minutes of data. A shallow `MustScanSubDirs` anchor at `/System` at 12:12:53
superseded live mode with a full reconcile rescan that was still running 85 minutes later, and **the churn monitor only
observes live mode**. Restarting to recover makes it worse: each cold start replays the journal and can draw the same
shallow anchor.

So the hysteresis constants are not yet grounded in data, and Phase C should still treat them as unmeasured.

The finding underneath: on a real dev machine, live mode is a minority of the app's life. Across the available logs,
**14 of 28 recorded scans were triggered by `shallow MustScanSubDirs`**, roughly one every two hours including
overnight. Any future design that assumes "the live event loop is generally running" should check that assumption first.
Raw trigger list preserved during this spike; the rescan-frequency question is being tracked separately.
