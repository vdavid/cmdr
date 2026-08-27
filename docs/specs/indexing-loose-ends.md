# The coverage machine works; these are the threads left hanging

**Problem**: phased indexing and claim-based ground ownership both shipped, and both left a named tail nobody scheduled.
What is left is one unexplained measurement; every item that was work has landed, moved to GitHub, or been declined on
the record.

**Read first**: `crates/cmdr-index/src/indexing/lifecycle/phases/DETAILS.md` (the phase machine and why the walk's order
changed but not its extent), and `crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § "Two ownership designs that
were considered and rejected" before proposing any change to who owns a walk.

## Deliberately not doing

- **The verifier MARK, with the abandoned-ground visit trigger.** Replacing the verifier's `listed_epoch == 0` bail
  (`reconcile/verifier.rs`, `is_the_walks_to_cover`) with a mark would let a browsed folder become searchable without
  waiting for a walk, and would fix the one case where ground marked `Abandoned` is unreachable by anything the user
  does. ⚠️ **It is not a throughput win, which is why it lost.** `writer/abandoned_retry.rs` documents its five-minute
  first step as costing "~nothing", so collapsing that backoff is a simplification and not a saving; the reuse of a
  directory read already paid for is real but covers only the folders somebody actually visits. Against that,
  `verifier.rs` spells out the risk: writing children under a directory nothing marked is "precisely the non-virgin node
  that sends a later cover walk down the serial repair path, and running a full recursive `scan_subtree` per new
  subdirectory to get there" — a walk that degrades silently rather than an answer that is wrong. **Recommendation:
  leave the bail alone.** Revisit only if abandoned ground shows up in real reports.
- **Pinning the first-run layout end to end.** A Playwright run over a first launch asserting left `~` and right
  `~/Downloads` cannot pass, because `first-run-layout.ts` opens with `if (ctx.isAutomatedRun) return 'leaveAlone'` on
  purpose, so an E2E run never lays out. And `onboarding.spec.ts` records that per-spec env control is out of scope:
  every spec shares one Tauri instance per shard, so there is no first launch for a spec to observe. Making it real
  means a dedicated fresh-data-dir launch plus an escape hatch through the guard that protects every other spec, for one
  assertion over a pure function whose whole matrix is already unit-covered. **Recommendation: leave it to the unit
  tests.**
- **Migrating the reconciler's rescan scheduler onto claims.** It is a second queue-and-lease scheduler unaware of the
  other ten actors, and migrating the ownership half is a day or two. It is also the one milestone in that plan with no
  user-visible payoff, and the module has since grown its own `CLAUDE.md` / `DETAILS.md` pair, so the coupling it was
  meant to cure is at least documented now. **Recommendation: defer indefinitely.** David was asked once and never
  answered; this records the non-answer as a decision rather than leaving it open.
- **A replay-loop test harness.** There is none in the repo, and building one means a real `DriveWatcher` over a temp
  tree, so it is a day of harness for one assertion. The risk it would guard, a truncate under a live replay, is
  prevented by the `Exclusive` claim.

## Shipped

- **The `mgr.scanning` rename**, which closed the claim-based ownership plan. It is `ground_in_flux` now, because a
  replay sets it too and its most important reader is a guard rather than a report.
- **Spotlight recency feeding the first-run walk order.** `src/spotlight.rs` asks Spotlight which folders hold this
  user's recently-opened files, coupled to nothing; `priority/roots/recency.rs` decides when to ask and what is worth
  walking early. Ranked by direct file count over a 30-day window, below tabs and favorites and above the static home
  folders. Details and the three filters: `apps/desktop/src-tauri/src/priority/DETAILS.md` § "The recency signal".

## Split out to GitHub

Both were product work wearing an engineering estimate, so they left this spec rather than sitting in it:

- **"Watch only these folders" as a user setting**: `vdavid/cmdr#56`. The branch-watch mechanism is already the
  implementation; what it needs is four product decisions and copy in 10 locales.
- **Finder sidebar favorites as priority roots**: `vdavid/cmdr#57`. There is no supported API, so it means parsing an
  undocumented `sfl2` plist. Survivable only as strictly best-effort, since `priority_roots` promises order and nothing
  else.

## One measurement nobody has explained

The app sees 19 to 21 seconds between `home_covered_at` and the `/` phase starting, against the in-crate arm's 39
seconds. Both support the same decision, so neither is wrong, but the gap is unaccounted for and is flagged in
`phases/DETAILS.md`. Worth resolving before anyone tunes phase boundaries on those numbers.
