# Covering a volume in phases

The volume's first index, in the order its owner cares about: the host's folders, then `$HOME`, then the drive. Every
walk is add-only and resumable, so a quit keeps what it bought. There is no first full scan.

`mod.rs` the driver + `PhaseHandle`; `stitch.rs`; `queue.rs` the ranked order; `grouping.rs` how many roots one walk
takes; `visits.rs` where the user is looking; `completion.rs` the two stamps. The start is `../manager/phased.rs`.

## Must-knows

Each of these fails SILENTLY when ignored; `DETAILS.md` has the incident behind every one.

- **The stitch is what makes phases compose; ❌ never skip it** (the next phase re-walks everything, each root taking
  the SERIAL repair). It marks ONE directory at a time, files included, flushes between the upserts and the mark, stamps
  the CURRENT epoch, and ❌ never marks a directory it couldn't read.
- **A phased start prepares its database through writer MESSAGES**, ❌ never a second write connection. No
  `EXCLUSION_POLICY_KEY` stamp ⇒ every coverage query answers "walk everything" and nothing EVER converges.
- **Completion is DERIVED** ("the frontier under this root is empty"), ❌ never remembered, ❌ never a "didn't shrink
  twice" rule. Abandoned ground leaves the frontier, so one wedged directory can't hold it open.
- **The completion ORDER is enforced by a flush, stamp before collapse**, and ❌ `AggregationComplete` never moves ahead
  of that flush: collapsing early lets a shallow anchor truncate the index that just finished, and a terminal ahead of
  the heal's progress ticks leaves every hourglass lit until relaunch.
- **`working` (a phase queued or running) is what scan entries refuse against; `walking` (reading the disk now) is only
  the verifier's.** ❌ Never `mgr.scanning`: `cover_context_for` returns `None` under it, so our own walks fail.
- **One `cover()` per GROUP of frontier roots, joined**, sized from what the last group cost (`grouping.rs`), draining
  per phase and taking stock per DRAIN, ❌ never per root, ❌ never a whole frontier in one call, ❌ never a fixed size.
- **The phase is a typed `CoveragePhase` on ONE event AND on the status response** (transition-only leaves a reloaded
  window blank). ❌ Never off the branch events, ❌ never derived host-side, ❌ not one-shot: an interlude makes the
  outer phase re-announce, via the coverage event alone.
- **Every walk is bracketed by `CoverageBranchStarted` / `CoverageBranchEnded`**, the end on EVERY exit path, cancels
  included. A whole-volume scan reports the same way, so ❌ nothing downstream branches on the kind of run. ❌ No
  debounce here; how long the UI waits is the app's call.
- **`home_covered_at` drives exactly ONE subscriber** (the media + importance early kick), ❌ not freshness, the badge,
  or rescan routing.
- **Ask the host through the seams it already has**: `priority_roots` (an ORDER, never a scope) and `open_listings` on
  the reporter's tick. ❌ Not `verify_directory`, which fires for the opposite pane and every refresh.
- **A rescan before full coverage RESTARTS the phases**, ❌ never truncates, ❌ never errors; the deferred-rescan
  mechanism answers for COMPLETED volumes only.

Depth on every bullet, the escape hatch, and the measurements behind the flush batching and the `~/Library` decision:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
