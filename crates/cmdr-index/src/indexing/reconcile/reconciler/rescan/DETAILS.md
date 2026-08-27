# Rescan scheduler details

Read this before any non-trivial work in `reconciler/rescan/`: editing, planning, reorganizing, or advising. Must-know
guardrails are in `CLAUDE.md`.

The scheduler decides WHICH `MustScanSubDirs` anchor walks, WHEN, HOW OFTEN, and what the user sees while it does. The
diff engine it calls to do the walking (`reconcile_subtree`, `diff_dir_against_db`) and the event processing that feeds
it anchors are `../../DETAILS.md`. `mod.rs` orchestrates: one walk at a time on a `Utility`-QoS thread, anchors queued
in `pending_rescans`, drained on completion. The five leaves below own one decision each.

## Per-subtree rescan throttle (`throttle.rs`, `mod.rs`)

A `MustScanSubDirs` signal means "re-walk this subtree", and a hard-churning subtree (build output, caches, Cmdr's own
data dir) raises it continuously. The drain caps each anchor to ≤1 reconcile per window, so a folder's size stays
bounded-fresh (≤1 window stale) without re-walking continuously. Leading + trailing, not debounce (mirrors the per-file
`../throttle.rs`): a never-walked anchor reconciles immediately; a sustained one re-walks once per window forever (the
~1 s `throttle_sweep_interval` tick re-kicks via `EventReconciler::sweep_rescan_throttle`, and it re-asks `is_eligible`
each tick, so a longer window is never bypassed). `pick_and_collapse_rescan` picks the shallowest ELIGIBLE anchor;
throttled anchors stay queued in `pending_rescans` until their window elapses. The drain runs on a dedicated
`Utility`-QoS thread (not the tokio blocking pool, which `thread_qos` forbids lowering), so background subtree walks
never outrank the webview for CPU. A single growing file is handled by the per-file live path (incremental `dir_stats`
deltas), never a subtree re-walk, so the throttle needs no significant-change bypass. Tests zero both bounds via
`disable_rescan_throttle_for_test`.

**Each anchor's window is proportional to what its walk COST**:
`clamp(WALK_COST_MULTIPLIER × walk_cost, RESCAN_THROTTLE_WINDOW, RESCAN_THROTTLE_MAX_WINDOW)`, currently `30 ×`, clamped
to 60 s–30 min. So an anchor spends at most ~1/30th of the time re-walking itself, and no single subtree can dominate
the reconcile budget however expensive it is to list. A flat window can't hold that line: a 10 s walk that becomes
eligible again 60 s later is a permanent ~17% duty cycle on one anchor. Measured on David's machine (2026-07-23, a day
of reconciler logs): one anchor (a WebKit cache directory with 144,647 children) averaged 10.5 s per walk, was re-walked
49 times, and burned 516 s, 49% of the day's entire reconcile budget, while 4,559 other anchors finished in under a
second each. Under the cost-scaled window that anchor earns ~5 min and drops to roughly 10 walks a day; every sub-2 s
anchor stays pinned at the 60 s floor, unchanged. The ceiling exists because past half an hour a stale subtree costs the
user more than the CPU the back-off saves.

**Cost is `ReconcileSummary::walk_cost()` (duration MINUS writer wait), never the raw duration.** Time parked on a
saturated writer queue is the writer's, not the anchor's; charging it would let one transient global saturation (an
initial scan, say) inflate every anchor's measured cost at the same moment and back a whole volume off for half an hour.
This is the same attribution `reconcile_report` makes for its log level, from the same `writer_wait` probe.

`gc` measures each record against its OWN window, not a global one. Against a global 60 s an expensive anchor's record
would be evicted the moment the floor elapsed, and the anchor would then be eligible on its leading edge, defeating the
back-off entirely.

## The settle delay for brand-new subtrees (`settle.rs`)

An anchor whose directory was created less than `NEW_SUBTREE_SETTLE_DELAY` (30 s) ago is not walked yet. It stays queued
and becomes eligible once it has settled; nothing is dropped or forgotten.

**Why youth and not repetition.** Measured on David's machine (2026-07-23, a day of reconciler logs): 2,315 of the day's
4,626 subtree reconciles — 422 s, 40% of the total reconcile time, 550,868 row deltas — went to roughly 2,300 UNIQUE
ephemeral paths under `~/Library/Caches/com.inkeep.open-knowledge.ShipIt/update.<random>/OpenKnowledge.app/…`, an app
updater unpacking Electron bundles. That cache directory now holds three entries totalling 36 KB: every one of those
bundles was deleted before we finished indexing it. The per-subtree throttle cannot catch this, and no tuning of it
would: its signal is REPETITION, and every path is unique, so no anchor ever reaches a second strike. The signal that
separates an updater's staging dir from a folder someone made is how long it has existed.

**Birthtime, not mtime.** "Brand new" is a creation-time question. Using mtime would delay a busy but long-established
directory, which is exactly the case the throttle already handles well and this must not touch.

**The stat lives at the enqueue call site, never in the throttle.** `RescanThrottle` is pure and clock-injected (no
filesystem, no logging, no clock of its own), which is why every one of its rules is deterministically unit-testable. So
`settle::note_settle_deadline` does the `symlink_metadata` and passes the resulting DEADLINE into the throttle as data,
the same way `now` and `walk_cost` are passed in. The throttle lock is taken once to read the policy and once to store
the result, never held across the syscall. Cost is one stat on the anchor itself (never a walk), on the same event-loop
thread that already stats once per live create/modify event.

**A re-enqueue can't push the deadline out.** Every enqueue re-derives the deadline from the same immutable birthtime,
so an anchor that keeps raising `MustScanSubDirs` settles on schedule. The deadline moves only when the directory itself
is replaced (delete + recreate gives a new inode with a new birthtime), and that genuinely IS a new subtree, not the
same one being starved.

**Fail open, never closed.** No readable birthtime (a filesystem or platform that doesn't record one, a directory that
already vanished, a wall clock that moved backwards) means no deadline and the anchor walks exactly as before. A missing
birthtime must never stall an anchor.

**It composes as a second eligibility gate.** `RescanThrottle::is_eligible` answers to BOTH the settle deadline and the
cost-proportional window, and whichever says "not yet" wins; neither can starve an anchor, because both are absolute
deadlines that pass. Everything downstream follows for free, because everything downstream asks the same question:
`pick_and_collapse_rescan` leaves a settling anchor queued, and the hourglass hold (below) reads a settling anchor as
neither walking nor queued-and-eligible, so it holds nothing and drags no ancestor into "size updating". The ~1 s sweep
tick re-asks each tick, so the anchor walks within a second of settling. `gc` bounds the settle map exactly as it bounds
completions: drop an elapsed deadline for an anchor nobody has queued (it reads the same as no record at all), keep a
live anchor's.

**Two enqueue sites take the stat, one deliberately doesn't.** `queue_must_scan_sub_dirs` (every live/replay/storm
feeder) and the Leak-B escalation re-queue both stat, the latter because a missing chain is often missing precisely
BECAUSE it was created seconds ago. `requeue_rescan` (the removal-storm drop rule) does not: it fires once per dropped
event, thousands in a storm, and the scope it re-queues is already queued or walking, so its settle verdict is already
recorded.

**The vanished anchor, which is the designed outcome.** Most of these directories are gone by the time they settle.
`reconcile_subtree` on a vanished root that was never indexed resolves neither root nor parent, stats the root, fails,
and returns an empty summary with `escalation: None` at debug level: no rows, no re-queue, no hold left behind (the
completion path releases as usual), and the single-flight drain moves straight to the next anchor. If the root IS in the
DB, `read_fs_children` returns `None` and the walk lists nothing — the rescan drain never deletes rows for a vanished
subtree by design; that is the FSEvents delete path's business (`handle_creation_or_modification`'s stat-failure branch
sends `DeleteSubtreeById`, and the storm drop rule deliberately keeps a scope's OWN removal event on that cheap path).
Worst case an escalation hop re-queues the highest missing dir once; that hop's parent IS in the DB, so it terminates on
the stat-failure branch rather than escalating again.

## The rescan hourglass hold (`hold.rs`)

A rescan root held in `PendingSizes` marks its whole chain pending in BOTH directions
(`../../../read/pending_sizes.rs`), so an anchor at `~/Library/Caches/…/NetworkCache/…/Resource` holding drags
`~/Library`, `~`, and `/` into the "size updating" hourglass with it. The reach is correct while the subtree is being
rewritten and wrong the rest of the time, so the module keeps ONE invariant:

**An anchor holds iff it is walking right now, or it is queued AND eligible to walk now.** The hold means "unprocessed
index writes in flight or imminent" — nothing weaker.

The load-bearing half is what it EXCLUDES. A queued-but-throttled anchor has no writes in flight: its last walk
completed and its final aggregate is consistent; it is only resting out the window that walk earned. Holding through
that rest is what put the hourglass on `~` and `/` for as long as the anchor kept churning — bounded at about a minute
under a flat 60 s window, but up to 30 minutes once the window became cost-proportional, and worst exactly for the
expensive churning anchors the back-off targets. The honest signal is kept for the queued-and-eligible case: an anchor
waiting only on the single-flight active walk still holds, because its walk is imminent.

Four sites maintain it, deliberately overlapping:

- **Enqueue** (`hold_if_eligible`, from `enqueue_rescan` and the Leak-B escalation re-queue): an eligible anchor holds
  as soon as it's queued, so the honest signal doesn't wait up to a second for the sweep tick.
- **Pick** (`adopt_picked_holds`, inside `start_next_rescan`'s pick block): the anchor about to walk holds
  unconditionally, and the descendants ancestor-collapse dropped release theirs (now covered by the picked ancestor's
  hold). Taken UNDER the `pending_rescans` lock. This is what makes "walking ⇒ held" structural rather than inferred,
  and it's why every release path may release freely: a follow-up walk takes its own hold rather than inheriting one.
- **Sweep tick** (`reconcile_with_eligibility`, on the same ~1 s tick as the throttle re-kick): re-derives each QUEUED
  anchor's hold from its current eligibility. This is what turns a throttled anchor quiet and re-arms it when its window
  elapses, one tick before the re-kick walks it.
- **Every rescan exit** (`release_rescan_hold`, `release_and_emit_completion`): releases unless the root is back in
  `pending_rescans` AND eligible. The completion path records the throttle completion FIRST, so a churning re-queue
  reads throttled there and releases; the exits that record nothing (conn-open failure, spawn failure) leave the anchor
  eligible, so a re-queue keeps the hold unbroken for its imminent retry instead of flickering it off and back on.

**Why sweep-time reconciliation rather than pick-time only.** Pick time alone would leave a queued-and-eligible anchor
unheld while it waits behind the active walk, losing the honest signal in exactly the case where the walk IS imminent.
Enqueue alone can't work either: eligibility changes with the clock, and only a tick re-evaluates it. The two ends plus
the tick give each state its own writer, and the pick-time hold is the one that must never be skipped.

**There is no window where a walk is writing while its anchor is unheld.** The active walk is popped out of
`pending_rescans`, so the sweep never iterates it. A storm that re-queues the active path can only put it back while its
throttle record still predates the walk, which reads ELIGIBLE and therefore holds. After the walk records its completion
the anchor is ineligible, but `active_rescan_path` still names it, so the sweep skips it (it's passed in as `active`)
until the task itself releases. The pick-time hold closes the last seam: even if a release and a re-queue interleave,
the follow-up walk holds when it is picked.

**What this does NOT do: a throttled subtree's size is not marked stale.** `recursive_size_stale` is
`complete && min_subtree_epoch < current_epoch` (`../../../read/enrichment.rs`), and `current_epoch` bumps only on a
continuity BREAK (reconnect/rescan, watcher death, overflow, disconnect, launch-loading-Stale) — never per throttle
window. A reconcile STAMPS `listed_epoch` with the current epoch, so a subtree walked this session reads `stale = false`
and renders `'size'`, a confidently-exact value, for the whole back-off. Dropping the hold therefore leaves it looking
fresh rather than muted. Whether that's worth a distinct signal is open.

**A reconcile's log line attributes its writer wait** (`mod.rs` `reconcile_report`, `../../../writer/wait_probe.rs`).
The bounded writer channel means a producer parks once it's full, so `reconcile_subtree`'s own duration silently
included the wait ("reconcile slow for … (21s)" meant "the writer was saturated for 19 of those seconds").
`reconcile_subtree` arms the thread-local writer-wait probe at its start and reports the span as
`ReconcileSummary.writer_wait`. `reconcile_report` is pure and returns `(log::Level, String)`: `debug` under 10 s (see
the churn signal below); past that the wait is named, and when it DOMINATES (over half the duration) the line stays at
`debug` and says "reconcile waited" (writer saturation has its own signal in the writer heartbeat), else it warns
"reconcile slow". The probe mechanism is in `../../../writer/DETAILS.md`.

## The churn signal (`churn.rs`)

Both per-walk lines (`reconcile starting`, `reconcile complete`) are DEBUG, because a normal day produces thousands of
them and most say `+0 -0 ~0`. Measured on David's machine (2026-07-23, a day of reconciler logs): 4,626 starts and 4,596
completes, of which 2,486 changed nothing at all and cost 11.5 s between them. At `info` they buried the two lines that
mattered.

Demoting them alone would be a regression in disguise: the problems this area's fixes address stayed invisible for
months precisely because nobody could see the aggregate. So one INFO line replaces the thousands. `RescanChurnWindow`
rolls every completed reconcile into a 15-minute window (`CHURN_WINDOW`) and emits at most ONE line, only when the
window crossed a budget: more than 60 s of cumulative walk time (`WALK_BUDGET`) or more than 100,000 cumulative row
changes (`ROW_BUDGET`). Under both, the window resets silently, so a quiet machine never sees the line and a quiet
stretch can't accumulate its way to one hours later.

```
Reconciler: heavy churn in the last 15 min: 1,621 subtree reconciles, 507s of walking, 120,190 row changes, 64+ anchors, 37 signals held back, 8,142 signals queued behind a running rescan. Top: /Users/me/Library/Caches/… (18 walks, 96s), …
```

**The top anchors are the point.** "Which folder" is the entire diagnostic value, so the line ranks anchors by
accumulated cost (walks alone would name a cheap chatterbox over the anchor actually burning the CPU) and names three.

**`held_back` is what proves the throttle and the settle delay still work.** It counts `MustScanSubDirs` signals that
arrived for an anchor which may not walk yet, at the `queue_must_scan_sub_dirs` call site only: `requeue_rescan` fires
thousands of times per removal storm for one scope and would drown the number. A window that churns hard while this
reads zero means an eligibility gate stopped engaging, which is the regression that would otherwise be silent. It's
deliberately one number, not one per gate: telling settle from throttle needs a new eligibility-reason API on the pure
throttle, and the top-anchor list already tells you which kind of churn you're looking at.

**`queued behind a running rescan` is what a per-path DEBUG line used to say.** Every signal arriving while the
single-flight drain was already walking used to get its own `MustScanSubDirs for {path} queued (rescan already active)`
line. On a machine running cargo that was ~4,000 lines an hour, a quarter of the entire log, and the existing
consecutive-line dedup never fired because the paths are unique (fingerprint dirs). The rate is the diagnostic, not the
paths, so the count rides this line and the per-path form dropped to TRACE. Counted at the same call site as
`held_back`, and for the same reason. It prints only when non-zero: unlike `held_back`, a zero here isn't a regression
signal.

**The per-walk `unreadable dirs` count is the same trade one level down.** A reconcile that races a compiler emptying
its target dir hits directories that vanish between the event and the read; each one used to log
`reconcile: can't read {path}` at Debug (~750 lines an hour). That's the EXPECTED race, not a diagnosis, so
`reconcile_subtree` counts them into `ReconcileSummary::unreadable_dirs` and the completion line carries the number
(omitted at zero). The per-path lines are TRACE.

**Bounded memory, explicitly.** The machine produces thousands of distinct anchors a day (5,876 across the sampled log,
5,587 of them one-shot), so per-anchor tallies are capped at `MAX_TRACKED_ANCHORS` (64) and nothing survives a window.
Past the cap, a new anchor gets in only by outspending the cheapest one tracked, which then gives way. Refusing every
newcomer instead would be actively broken: one-shot anchors fill the map within minutes, and the expensive anchor that
shows up later (the one the reader needs) would never be named. Totals stay exact whatever the cap drops, and a capped
count prints as `64+ anchors` so it reads as the floor it is.

**Where it lives, and why not the neighbours.** The engine is pure and clock-injected like `throttle.rs` beside it, so
every accumulate/threshold/format rule is unit-tested with no logger, clock, or filesystem; the impure part is three
thin fns owning one process-wide `Mutex`. Process-wide, not per-reconciler, because reconcile churn is a MACHINE
question: two volumes each walking 40 s is 80 s of this machine's CPU, and a per-volume window would report neither. Two
nearby mechanisms were considered and are deliberately separate:

- `DEBUG_STATS` (`../../../events/mod.rs`) counts `MustScanSubDirs` signals and completed rescans app-wide since the
  last reset, for the debug window. No window, no cost, no row deltas, and `reset()` on every scan start, so it cannot
  answer "is this machine reconciling too much right now?". The churn window doesn't replace it; they feed different
  surfaces.
- The churn monitor (`../../../watch/churn_monitor.rs`, `docs/notes/churn-observability-spike.md`) measures FSEvents
  churn per directory rolled up the ancestor chain, off unless `CMDR_CHURN_SPIKE` is set, at Debug, for offline
  analysis. Different input (raw events, not completed reconciles), different sink (a script, not a person reading the
  log), and it can't see walk cost or row deltas at all. This one borrows its discipline (measured window rather than
  assumed, hard cap with the drop counted, pure engine) and nothing else.

**What it should do.** Replayed over the sampled logs (`~/Library/Logs/com.veszelovszki.cmdr/cmdr.log`, 6,286 completed
reconciles, 2026-07-19 to 2026-07-23), the budgets fire in 11 of the 49 windows that saw any reconciling, and every one
of those 11 is a window this area's fixes target: 1,621 reconciles over 1,595 one-shot anchors (the settle delay),
289,531 row changes across 13 walks of 8 anchors (the hardlink diff fix), and repeated ~10 s walks of one cache
directory (the cost-proportional throttle). If it keeps firing on a normal day AFTER those fixes, they didn't finish the
job, and that is exactly what the line is for.

## Depth-split `MustScanSubDirs` routing (`route.rs`)

The per-subtree throttle is the right tool for a DEEP/narrow anchor (a single `target/`), but NOT for a shallow/root-
scale one. Under a high-churn boot disk, macOS drops fine-grained FSEvents and raises `MustScanSubDirs` on ever-higher
paths, up to `/`. Reconciling `/` is a ~20-min walk, and the whole time it runs it legitimately holds the per-dir
hourglass over everything below — an invisible reconcile that makes every local size look unsettled for twenty minutes,
and a 60 s throttle after a 20-min walk is noise. A channel overflow (the SAME "we lost events" meaning) already takes
the VISIBLE scanner path; this makes the two equivalent signals converge. `route_must_scan_sub_dirs` (the single entry
point for the two feeders the fix targets — the live path `process_live_event` and the post-replay handoff
`event_loop::replay`) classifies by anchor depth via `route::classify`:

- **Shallow** (`depth <= SHALLOW_RESCAN_MAX_DEPTH = 2`, i.e. `/`, `/Users`, `/Users/<me>`): `route_to_visible_sweep`
  requests a fresh `start_scan` via `ScanTrigger` and takes NO hourglass hold and NEVER enters `pending_rescans`
  (holding it is the stuck-hourglass bug). In production `ScanTrigger::Registry` spawns
  `manager::perform_registry_rescan` (extract manager → stop watcher + live loop → `start_scan` off the lock → reinsert
  `Running`; shared with the replay full-scan fallback, single-flight). Tests inject `Disabled`/`Recording`.
- **Deep** (`depth >= 3`): unchanged — `queue_must_scan_sub_dirs` keeps the throttled reconcile drain. The removal-storm
  and Leak-B escalation feeders also call `queue_must_scan_sub_dirs` directly, so their behavior is unchanged (only the
  two named feeders route by depth).

Depth is a proxy for "re-listing this is walk-the-world expensive"; 2–3 levels is where a reconcile stops being cheap
and starts holding the hourglass for the better part of a full scan.

## Anchor-cardinality routing (`cardinality.rs`)

**The problem the throttle can't reach.** `throttle.rs` bounds how often a GIVEN anchor re-walks. It contributes nothing
to bounding how many DISTINCT anchors arrive, because `is_eligible` is eligible-on-first-sight by design (a leading
edge, deliberately not a debounce). A machine producing one-shot anchors — a compiler's fingerprint dirs, an updater's
staging dirs — therefore pays one subtree walk per anchor at whatever rate it produces them, so the cost scales with the
user's workload rather than with anything Cmdr controls. `settle.rs` catches the sub-30-second slice of this (an anchor
deleted before it settles) and nothing catches the rest.

**Decision**: past `HIGH_CARDINALITY_ANCHORS` distinct DEEP anchors arriving on one volume within a
`CARDINALITY_WINDOW`, every further deep anchor takes `route_to_visible_sweep` instead of the drain, landing in the same
once-a-day window a root-scale anchor lands in. **Why**: it reuses a shipped mechanism and a shipped user-facing story
(coalesced signals counted into the volume tooltip, badge green), and it's the only bound of the four considered that
fights neither `hold.rs` nor `local_reconcile/cost_budget.rs` — a routed anchor never enters `pending_rescans` at all,
so it takes no hourglass hold and there is nothing for the ~1 s sweep to re-derive. **What it buys**: predictability and
a bounded worst case (one whole-volume sweep a day), paid for with up to 24 hours of whole-volume staleness. ⚠️ It is
NOT a CPU win: "the reconcile drain is the one that moves the CPU number" was wrong answer one in
`docs/notes/idle-cpu-attribution-2026-08-03.md`, refuted by measurement.

**The threshold is a GUESS and it is waiting on data.** No distribution of per-window anchor cardinality has been
collected. The churn line above is what will collect it (an ordinary week on a quiet machine, then a `docs/notes/`
note), and `HIGH_CARDINALITY_ANCHORS` should be re-set from that. What it is positioned against is the only
anchor-cardinality data in the repo, from the same sampled log the sections above use (David's machine, 2026-07-19..23,
running six cargo builds — a heavy case, not a typical one): 5,876 distinct anchors across a sampled day, on the order
of 60 per 15-minute window spread evenly, against 1,595 one-shot anchors in the single worst window. 256 sits several
times above that heavy machine's average window and an order of magnitude below its worst. Corroborating rather than
deriving: the churn window stops tracking per-anchor tallies at 64, so a window past 256 lost the ability to name a
culprit folder long ago.

**Arrivals, never completions, and that is the whole design.** The obvious signal to route from is the churn window
beside it, which already accumulates per-anchor walks and cost. It's the wrong one: it measures completed reconciles,
which is exactly the quantity routing suppresses, so a router reading it would arm, starve its own input, disarm, and
oscillate between the two routes every window. Arrivals are exogenous — a compiler keeps producing anchors whether or
not we walk them — so counting them keeps the verdict stable for as long as the storm lasts and lifts it one to two
windows after the storm stops. ⚠️ This is why every deep arrival must be counted, INCLUDING the ones that end up routed:
skip those and the counter measures the drain's output again, with the oscillation back.

**The previous window counts too.** A window starts from zero distinct anchors, so a machine still churning would read
low for the minutes it takes to re-cross, and every boundary would hand the drain a fresh burst of walks. The verdict is
`crossed this window || crossed the last one`, which makes the bound continuous while anchors keep arriving. More than
one window of silence carries nothing, because the windows in between saw no arrivals at all.

**Boot disk only**, like the sweep window itself, for the two reasons under `EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL` plus
a third that is decisive alone: an external volume's window is 45 s, so routing there would turn high cardinality into a
whole-volume walk every 45 seconds, strictly worse than the drain it replaced. An anchor `may_walk` refuses isn't
counted either — it costs nothing today, so it must not arm the router.

**Bounded memory, explicitly.** The set stops inserting at the threshold: it exists to answer "have we seen this many
DISTINCT anchors yet", and once it has, another path can't change the answer. So one volume holds at most
`HIGH_CARDINALITY_ANCHORS` paths per window, in exactly the case (a storm of unique one-shot anchors) where an unbounded
set would be worst. The ledger is a process-global keyed by volume id, for the same reason the sweep ledger is: a
reconciler is recreated on every scan cycle, and a per-instance window would forget the storm each time.

**What this deliberately does NOT touch.** The two feeders that call `queue_must_scan_sub_dirs` directly — the
removal-storm drop rule and the Leak-B escalation re-queue — are unchanged, exactly as the depth split left them; the
escalation path is a correctness path (a missing parent chain) and must never be coalesced away. ❌ And nothing here
reads a path-shaped exclusion list: cardinality is a rate question, and `scanner/exclusions.rs` gaining a fourth
consumer is an open question for David, not a dependency of this.

## The once-a-day sweep window for shallow anchors

**The measurement** (David's machine, 2026-07-18..20): **14 of 28 scans were triggered by a shallow `MustScanSubDirs`
anchor**, roughly one every 2.5 hours INCLUDING OVERNIGHT while idle (01:17, 03:44, 06:39, 08:46, 11:16). **Thirteen of
those 14 anchors were `/` itself; the fourteenth was `/System`, a sealed read-only volume where nothing writes.** So the
anchor path carries no diagnostic information: macOS isn't reporting where churn happened, it's reporting that it gave
up and coalesced to the watch root. Each trigger runs the SERIAL reconcile walk, measured at 1,309 s on this volume.
That's roughly ten multi-minute-to-multi-hour full walks a day for a signal that says nothing about what changed.

**The policy** (`SHALLOW_RESCAN_MIN_INTERVAL = 24 h`, `decide_shallow_anchor`): a shallow anchor means "this index is
now SUSPECT", not "rescan right now". At most one real sweep per volume per day. Two reasons reach this window and share
it (`SweepReason`): a shallow anchor, and a deep anchor arriving into a high-cardinality storm. They draw on one
resource — whole-volume sweeps per day — and mean the same thing to a reader, so the tooltip line covers both without a
word changing.

- **Boot disk ONLY.** A mount-rooted volume keeps `EXTERNAL_SHALLOW_RESCAN_MIN_INTERVAL` (45 s), selected by
  `min_interval_for(space.is_boot_disk())`. The reason to keep them apart: we measured the storm on `/` and have no
  evidence of one on external volumes, so a longer window there buys nothing while a 24-hour blind window could cost.
  (The per-navigation verifier now covers external volumes too, so an external drive is no longer the one kind with zero
  cover between sweeps — but its cover reaches only directories the user actually opens, which is not an argument for
  stretching the window to a day.) Pinned by `an_external_volume_keeps_the_short_cooldown`.
- **Coalesced anchors are COUNTED, not silently dropped** (`SweepRecord.coalesced_since_sweep`). The count is **since
  the last COMPLETED sweep**, never a lifetime total (a lifetime counter would only measure how long the app has been
  installed). It rides `VolumeIndexStatus.coalesced_signals_since_sweep` alongside `next_sweep_due_at` (computed in
  `queries.rs`), feeding the volume tooltip's "macOS lost track of file system changes N times … next full check in N
  hours" line.
- **The badge deliberately stays GREEN.** Once-a-day sweeping is the DESIGNED operating state, not a fault, so it must
  not raise a fault signal; at the measured rate a Stale badge would sit yellow essentially all day. Yellow stays
  reserved for a sweep that fails to happen when it was due. `StaleDriveDialog.svelte` also excludes `root`.
- **The window is WALL-CLOCK (unix seconds), not `Instant`.** macOS `Instant` is `mach_absolute_time`, which doesn't
  tick while the machine sleeps (an `Instant`-based "day" on a laptop that sleeps 8 hours a night is really 32 hours of
  wall time), and `Instant` can't be restored from disk.
- **It survives relaunch, and an INTERRUPTED sweep can't reopen it.** The ledger is a process-global keyed by volume id
  (NOT a per-reconciler field, so it survives the reconciler recreation on every scan cycle). `resume_or_scan` reseeds
  it from `max(meta.shallow_sweep_at, meta.scan_completed_at)` plus `meta.shallow_coalesced_since_sweep`. Reading BOTH
  timestamps is the fix for a real hazard: `start_scan` DELETES `scan_completed_at` before walking, so keying the window
  off completion alone would make a never-finished sweep look permanently expired and put us back to sweeping every
  launch. A TRIGGERED sweep therefore stamps `shallow_sweep_at` immediately. Pinned by
  `an_interrupted_sweep_does_not_reopen_the_window_on_relaunch`.
- **Every completed full walk restarts the window and clears the count**, not only a shallow-triggered one
  (`scan_completion`): the window means "a full walk happened recently", so the user's own "Rescan now" counts too.
  Seeding takes the `max`, so a stale on-disk timestamp can't undo a sweep this process ran, and a `last` in the future
  (backwards clock jump) counts as elapsed so a bogus record can't wedge sweeps shut for years.

`classify`, `window_elapsed`, `min_interval_for`, and `decide_shallow_anchor_in` are pure/clock-injected and unit-tested
in `route.rs`; the decision and seeding take an EXPLICIT ledger so the tests use a local `HashMap` (clearing a shared
global from parallel tests flaked). `../tests/must_scan_routing.rs` holds the live-path repros.
