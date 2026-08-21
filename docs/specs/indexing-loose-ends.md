# The coverage machine works; these are the threads left hanging

**Problem**: phased indexing and claim-based ground ownership both shipped, and both left a named tail nobody scheduled.
None of it is urgent, all of it is small, and one item is the last thing standing between a finished plan and a closed
one. Left alone, these become the kind of half-memory that gets re-planned from scratch in six months.

**Size**: four days total if every item is taken, but they are genuinely independent, so any one can be picked up alone.

**Read first**: `crates/cmdr-index/src/indexing/lifecycle/phases/DETAILS.md` (the phase machine and why the walk's order
changed but not its extent), and `crates/cmdr-index/src/indexing/lifecycle/DETAILS.md` § "Two ownership designs that
were considered and rejected" before proposing any change to who owns a walk.

## The work

1. **Rename `mgr.scanning` to say what it now is.** Two to three hours, mechanical, and it closes the ownership plan.
   After the claim work landed, the flag is a reporting-and-buffering signal plus exactly one guard:
   `cover_context_for`, which is how a replaying or scanning volume refuses a new cover walk. The name no longer says
   that. About 27 sites across ten files, all crate-internal, field at `lifecycle/manager.rs:78`. ⚠️ A name saying
   "reporting only" would be a lie: pick one that covers the `cover_context_for` reader. ❌ Not a split and ❌ not a
   deletion. All seven readers stay. Dropping the store would break SMB and MTP buffering across the truncate, dark the
   hourglass, empty `walked_roots`, un-suppress the verifier, and make `awaits_its_first_scan` lie.

2. **The verifier MARK, with the abandoned-ground visit trigger.** A day, and these ship together. A folder the user
   browses is fully read by the verifier and still needs a walk afterwards. Today the verifier BAILS on
   `listed_epoch == 0`, which is the correct and safe half; marking is strictly more useful and strictly harder. ⚠️ The
   two are mutually exclusive: a verifier that both bails and marks is incoherent, so this is a replacement. Its
   companion is the abandoned-ground visit trigger, which the phase machine deliberately left out and which is the
   reason `abandoned_retry`'s backoff opens at five minutes instead of an hour (`writer/DETAILS.md` says so outright:
   "if something ever walks reopened ground on its own, the first step's whole reason goes away"). Landing both lets the
   backoff's first step grow back toward the wedged-ground curve.

3. **Feed first-run priority from Spotlight recency.** Half a day. `importance/last_used.rs` already samples
   `kMDItemLastUsedDate`, but from inside the crate and only once an index exists. An app-side `mdfind` at launch would
   feed `priority_roots` on a true first run, which is exactly when last session's tab paths are empty and the machine
   has least to go on. Needs Full Disk Access, which the phase machine already waits for.

4. **"Watch only these folders" as a user setting.** A day or two, mostly UI and copy. The branch-watch mechanism IS the
   implementation; phased indexing made branch-scoped watching the default shape rather than a retrofit, so this is
   exposure rather than construction. Highest value on Linux, where inotify watches are scarce against
   `max_user_watches` (`docs/notes/linux-gaps-2026-08-10.md`). ⚠️ It is a new setting, so it is user-facing copy in ten
   locales and needs David's review before it ships.

5. **Finder sidebar favorites as priority roots.** Half a day. Deferred twice already. `priority/roots/` reads Cmdr's
   own `favorites.json`; Finder's sidebar is explicitly out of scope today, and a user who has curated that sidebar has
   told the OS exactly which folders matter to them.

6. **Pin the first-run layout end to end.** Two hours. The one test deliberately skipped from the shipped first-run
   work: a Playwright run over a first launch with `CMDR_MOCK_FDA`, asserting left `~` and right `~/Downloads` exactly
   once, and never over a layout somebody already has. `CMDR_MOCK_FDA` is already wired and `onboarding.spec.ts` uses
   it; unit coverage exists, so this is the end-to-end pin only.

## Deliberately not doing

- **Migrating the reconciler's rescan scheduler onto claims.** It is a second queue-and-lease scheduler unaware of the
  other ten actors, and migrating the ownership half is a day or two. It is also the one milestone in that plan with no
  user-visible payoff, and the module has since grown its own `CLAUDE.md` / `DETAILS.md` pair, so the coupling it was
  meant to cure is at least documented now. **Recommendation: defer indefinitely.** David was asked once and never
  answered; this records the non-answer as a decision rather than leaving it open.
- **A replay-loop test harness.** There is none in the repo, and building one means a real `DriveWatcher` over a temp
  tree, so it is a day of harness for one assertion. The risk it would guard, a truncate under a live replay, is
  prevented by the `Exclusive` claim.

## One measurement nobody has explained

The app sees 19 to 21 seconds between `home_covered_at` and the `/` phase starting, against the in-crate arm's 39
seconds. Both support the same decision, so neither is wrong, but the gap is unaccounted for and is flagged in
`phases/DETAILS.md`. Worth resolving before anyone tunes phase boundaries on those numbers.
