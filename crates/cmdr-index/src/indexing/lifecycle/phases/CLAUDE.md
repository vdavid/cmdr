# Covering a volume in phases

The volume's first index, in the order its owner cares about: the host's folders, then `$HOME`, then the drive. Every
walk is add-only and resumable, so a quit keeps what it bought. There is no first full scan.

`mod.rs` the driver + `PhaseHandle`; `stitch.rs`; `queue.rs` the ranked order; `grouping.rs` how many roots one walk
takes; `visits.rs` where the user is looking; `completion.rs` the two stamps. The start is `../manager/phased.rs`.

## Must-knows

Each of these fails SILENTLY when ignored.

- **The stitch is what makes phases compose; ❌ never skip it**, or the next phase re-walks everything and each root
  takes the SERIAL repair. It marks ONE readable directory at a time, files included, flushes between the upserts and
  the mark, and stamps the CURRENT epoch.
- **A phased start prepares its database through writer MESSAGES**, ❌ never a second write connection. Miss the
  `EXCLUSION_POLICY_KEY` stamp and every coverage query answers "walk everything", so nothing EVER converges.
- **Completion is DERIVED** ("the frontier under this root is empty"), ❌ never remembered. Abandoned ground leaves the
  frontier, so one wedged directory can't hold it open, and a run that stops short asks for another pass
  (`../completion_retry.rs`), which moves WHEN the marker lands, not what earns it.
- **The completion ORDER is a flush, then the stamp, then the collapse.** Collapse early and a shallow anchor can
  truncate the index that just finished; a terminal ahead of the heal's progress ticks leaves every hourglass lit until
  relaunch.
- **`working` (a phase queued or running) is what scan entries refuse against; `walking` (reading the disk now) is only
  the verifier's.** ❌ Never `mgr.ground_in_flux`: `cover_context_for` returns `None` under it, so our own walks fail.
- **One `cover()` per GROUP of frontier roots**, joined, sized from what the last group cost (`grouping.rs`), draining
  per phase and taking stock per DRAIN. ⚠️ A group also STOPS for a folder somebody opened, through a PEEK over every
  remembered folder; that earns another PASS and ❌ never sizes the next group.
- **The phase is a typed `CoveragePhase`, on ONE event AND on the status response**, since a transition-only signal
  leaves a reloaded window blank. ❌ Never derived host-side, and an interlude makes the outer phase re-announce.
- **Every walk is bracketed by `CoverageBranchStarted` / `CoverageBranchEnded`**, the end on EVERY exit path, cancels
  included. A whole-volume scan reports the same way, so nothing downstream branches on the kind of run, and how long
  the UI waits is the app's call.
- **`home_covered_at` drives exactly ONE subscriber** (the media + importance early kick), ❌ not freshness, the badge,
  or rescan routing.
- **Ask the host through the seams it already has**: `priority_roots` (an ORDER, never a scope) and `open_listings` on
  the reporter's tick, ❌ not `verify_directory`, which fires for the opposite pane and every refresh.
- **A rescan before full coverage RESTARTS the phases**, never truncating and never erroring; the deferred-rescan
  mechanism answers for COMPLETED volumes only.

Depth on every bullet (with the incident behind each), the escape hatch, and the measurements behind the flush batching
and the `~/Library` decision: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing,
or advising.
