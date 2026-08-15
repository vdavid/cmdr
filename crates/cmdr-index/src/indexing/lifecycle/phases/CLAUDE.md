# Covering a volume in phases

The volume's first index, in the order its owner cares about: the host's folders, then `$HOME`, then the drive. Every
walk is add-only and resumable, so a quit keeps what it bought. There is no first full scan.

`mod.rs` the driver (its own `Utility` thread) + `PhaseHandle`; `stitch.rs`; `queue.rs` the ranked order; `grouping.rs`
how many roots one walk takes; `visits.rs` where the user is looking; `completion.rs` the two stamps and what completing
owes. The start itself is `../manager/phased.rs`.

## Must-knows

- **The stitch is what makes phases compose.** ❌ Never skip it: an ancestor scope's frontier never shrinks on its own,
  so the next phase re-walks everything and each root takes the SERIAL repair. It marks ONE directory at a time, files
  included, flushes between the upserts and the mark, stamps the CURRENT epoch, and ❌ never marks a directory it
  couldn't read.
- **A phased start prepares its database through writer MESSAGES**, ❌ never a second write connection. No
  `EXCLUSION_POLICY_KEY` stamp ⇒ every coverage query answers "walk everything" and nothing EVER converges, silently.
- **Completion is DERIVED**: "the frontier under this root is empty". ❌ Never remembered, and ❌ never a "didn't shrink
  twice" rule. Abandoned ground leaves the frontier, so one wedged directory can't hold it open.
- **The completion ORDER is enforced by a flush**, stamp before collapse: collapse first and one shallow anchor in that
  window truncates the index that just finished. ❌ `AggregationComplete` never moves ahead of that flush either — the
  ledger heal streams progress THROUGH it (18.8 s over a real `/`), and a terminal before those ticks leaves every
  hourglass lit until the next launch.
- **`working` (a phase queued or running) is what scan entries refuse against; `walking` (reading the disk now) is only
  the verifier's.** ❌ Never `mgr.scanning`: `cover_context_for` returns `None` under it, so our own walks would fail.
- **One `cover()` per GROUP of frontier roots, joined**, sized from what the last group cost (`grouping.rs`), draining
  per phase and taking stock per DRAIN, ❌ never per root. ❌ Never a whole frontier in one call (the cancel check isn't
  a queue check point), ❌ never a fixed size (big roots ⇒ minutes deaf to the user), and per root the stock-take
  re-asks a whole-volume descent about a database the unflushed walk hasn't moved: 75% of a resume.
- **The phase is a typed `CoveragePhase` on ONE event AND on the status response** (transition-only leaves a reloaded
  window blank for the rest of the run). ❌ Never off the branch events (frontier roots, one level down, debounced), ❌
  never derived host-side (firmlinks). ❌ Not one-shot: an interlude announces itself, so the outer phase re-announces
  (the coverage event alone, ❌ never a second `set_phase_for`) or the header names the folder the user opened for the
  rest of it.
- **Every walk is bracketed by `CoverageBranchStarted` / `CoverageBranchEnded`**, and the end fires on EVERY exit path,
  cancels included: a listing marks rows in flux on the start and has nothing else to take that back. A whole-volume
  scan reports the same way, naming the volume root, so ❌ nothing downstream branches on the kind of run;
  `covered_in_phases` answers only which checklist steps this run produces. ❌ No debounce here — how long the UI waits
  is the app's call (`events/index_mapping/walk_announcer.rs`).
- **`home_covered_at` drives exactly ONE subscriber** (the media + importance early kick), ❌ not freshness, the badge,
  or rescan routing. `~/Library` goes last in its phase and the signal doesn't wait for it.
- **Ask the host through the seams it already has**: `priority_roots` (an ORDER, never a scope) and `open_listings` on
  the reporter's tick. ❌ Not `verify_directory`, which fires for the opposite pane and every refresh.
- **A rescan before full coverage RESTARTS the phases**, ❌ never truncates and ❌ never errors. Every door goes through
  `cover_or_scan`; the deferred-rescan mechanism answers for COMPLETED volumes only, so the two never overlap.

- **The escape hatch is `defaults write com.veszelovszki.cmdr PhasedFirstIndex -bool false`** + relaunch. Off, a phased
  partial takes today's TRUNCATING rebuild; that's the intended row.

Depth on every bullet, plus the measurements behind the flush batching and the `~/Library` decision: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
