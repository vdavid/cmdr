# The rescan scheduler (which anchor walks, and when)

A `MustScanSubDirs` anchor says "re-walk this subtree". This decides which one walks, when, how often, and what the user
sees while it does; the diff engine it calls to do the walking is `../../CLAUDE.md`.

`mod.rs` the drain (one walk at a time, `Utility`-QoS, anchors queued in `pending_rescans`); `route.rs` the depth split;
`throttle.rs` the per-subtree window; `settle.rs` the delay a brand-new subtree gets; `hold.rs` the "size updating"
hourglass; `churn.rs` the 15-minute observability line; `cardinality.rs` the arrival-rate bound.

## Must-knows

- **Per-subtree rescan throttle is COST-PROPORTIONAL** (window scaled off `walk_cost`, clamped), and cost is duration
  MINUS writer wait, else one saturated writer over-throttles every anchor. `gc` measures each record against its OWN
  window.
- **A brand-new anchor SETTLES before it walks** (`settle.rs`), from BIRTHTIME, reading INELIGIBLE while queued and
  holding nothing. ❌ No stat inside the pure throttle (the call site passes a deadline in), ❌ no mtime; a missing
  birthtime FAILS OPEN.
- **A rescan anchor holds the hourglass ONLY while walking or queued-AND-eligible** (`hold.rs`). ❌ No unconditional
  hold at enqueue: a resting anchor puts "size updating" on `~` and `/`. ❌ Don't drop the pick-time hold either; it
  leaves no unheld-write window.
- **Depth-split `MustScanSubDirs`** (`route.rs`): SHALLOW (`depth ≤ 2`) → visible scanner, NO hourglass hold, never
  `pending_rescans`; DEEP (`≥ 3`) → throttled drain. A shallow anchor sweeps at most ONCE A DAY, boot disk only;
  coalesced anchors are counted, the badge stays GREEN, and the window is wall-clock and persisted.
- **Cardinality routing reads ARRIVALS, never completions** (`cardinality.rs`): past `HIGH_CARDINALITY_ANCHORS` distinct
  DEEP anchors in a window, a boot disk's deep anchors join that same once-a-day sweep. ❌ Don't count only the anchors
  that reach the drain, and ❌ don't route off `churn.rs` instead: both make the counter measure the drain's own output,
  so it arms, starves its input, disarms, and oscillates every window. The threshold is an UNMEASURED guess awaiting a
  week of churn lines.
- **`RescanDrain` and `ScanTrigger` stay in `../../reconciler.rs` on purpose**: `EventReconciler`'s own method returns
  the drain, so naming a `rescan` type in the parent's signatures would close a cycle through `lifecycle::manager`. ❌
  Don't "tidy" them down here.

Every window, its measurement, and the incident behind it: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
