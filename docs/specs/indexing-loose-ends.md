# The coverage machine works; these are the threads left hanging

**Problem**: phased indexing and claim-based ground ownership both shipped, and both left a named tail nobody scheduled.
Neither remaining item is urgent, both are small, and both are engineering calls rather than product ones: nothing here
has a setting, a string, or a permission prompt attached.

**Size**: a day and a half if both are taken, and they are independent.

**Read first**: `crates/cmdr-index/src/indexing/lifecycle/phases/DETAILS.md` (the phase machine and why the walk's order
changed but not its extent), and `crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § "Two ownership designs that
were considered and rejected" before proposing any change to who owns a walk.

## The work

1. **The verifier MARK, with the abandoned-ground visit trigger.** A day, and these ship together. A folder the user
   browses is fully read by the verifier and still needs a walk afterwards. Today the verifier BAILS on
   `listed_epoch == 0` (`reconcile/verifier.rs`, `is_the_walks_to_cover`), which is the correct and safe half; marking
   is strictly more useful and strictly harder. ⚠️ The two are mutually exclusive: a verifier that both bails and marks
   is incoherent, so this is a replacement.

   **What it buys, stated honestly.** ❌ Not throughput. The payoff is that browsed ground becomes searchable ground:
   today a folder the user is looking at can be absent from search until a walk reaches it, and for ground marked
   `Abandoned` nothing the user does fixes it at all (`writer/abandoned_retry.rs` says so twice: "navigating into it
   doesn't"). Secondarily it reuses a directory read already paid for, over the handful of folders somebody actually
   visits, which is real but small. Its companion is the abandoned-ground visit trigger, and landing both lets
   `abandoned_retry`'s backoff drop its five-minute first step and become one curve instead of two policies. ⚠️ That
   collapse is a simplification, ❌ not a saving: the first step is documented as costing "~nothing" because clearing a
   cause does no disk work by itself.

   ⚠️ **The risk is the reason this is a day and not an afternoon.** `verifier.rs` spells out what a wrong mark does:
   writing children under a directory nothing marked is "precisely the non-virgin node that sends a later cover walk
   down the serial repair path, and running a full recursive `scan_subtree` per new subdirectory to get there". The
   failure mode is not a wrong answer, it is a walk that silently degrades.

2. **Feed first-run priority from Spotlight recency.** Half a day once the three questions below are answered.
   `importance/last_used.rs` already samples `kMDItemLastUsedDate`, but from inside the crate and only once an index
   exists. An app-side Spotlight query at launch would feed `priority_roots` on a true first run, which is exactly when
   last session's tab paths are empty and the machine has least to go on. Needs Full Disk Access, which the phase
   machine already waits for.

   ⚠️ **The baseline is not nothing.** On a true first run `priority/roots.rs` already answers with
   `STANDARD_HOME_FOLDERS` (Downloads, Documents, Desktop, Pictures, Movies, Music, each taken only when it exists and
   holds something) plus the cloud roots. So the win is re-ranking those six and surfacing the folder that isn't among
   them, ❌ not filling an empty list.

   **Three questions decide the shape, and an implementer who isn't given them will invent answers:**

   - **Files to folders.** Spotlight returns files. Nothing says whether a hit promotes its parent, its nearest common
     ancestor with other hits, or a bounded ancestor under `$HOME`, nor whether the ranking is by hit count or by
     recency.
   - **The window.** A bounded query (`kMDItemLastUsedDate > $time.today(-N)`) needs an `N`, and the result set needs a
     cap, because Spotlight does not sort.
   - **Which API.** `last_used.rs` uses per-item `MDItemCopyAttribute`, which cannot express a query. This needs
     `MDQuery` or a shelled-out `mdfind`, so it is a new Spotlight pattern rather than an extension of that one.

   ⚠️ The seam is contractually cheap: `HostPolicy::priority_roots` is asked at every phase boundary with "❌ no I/O on
   a contended path and no blocking lock". So the query runs once at launch, off-thread, and feeds the existing cache in
   `priority/roots.rs` behind its `CACHE_TTL`.

## Deliberately not doing

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
