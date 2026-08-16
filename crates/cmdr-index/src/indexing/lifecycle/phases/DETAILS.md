# Covering a volume in phases

Read this before any non-trivial work in `lifecycle/phases/`. Must-know guardrails are in `CLAUDE.md`.

**A volume with no completed scan is covered, ❌ not scanned.** `resume_or_scan`'s third answer, beside journal replay
and `start_scan`, taking only the no-`scan_completed_at` case. There is no first full scan any more: the whole drive is
the LAST phase of the same `coverage`/`cover` mechanism a search-driven walk uses, so a quit keeps every second it
bought instead of truncating and starting over. That is true of every OTHER way a full walk starts too (the buttons, the
automatic rescans), which is what keeps a half-built index from being blanked: `../DETAILS.md` § "What a launch does
with the index it finds" holds the whole routing table.

**The shape of a run.** Ask the host which folders matter to this user (`HostPolicy::priority_roots`, an ORDER and
nothing else), then `$HOME`, then the volume root. Per phase: stitch down to its root, ask for its frontier, walk those
roots in groups with the visit queue checked between them, drain, take stock. `MAX_PASSES_PER_PHASE` re-asks once after
the drain (a root can expose new ground while it is being walked); past that whatever is left is this session's loss and
the next launch asks again. ❌ That is a PASS budget, never a completion rule.

## The stitch, and why phases would silently degrade without it

A cover walk marks only the directories it READS. Bootstrap creates the ancestor chain at `listed_epoch = 0` and claims
nothing (`cover/bootstrap.rs`), and the coverage descent cuts at the first unlisted directory without descending past
it. So after a phase covers `~/Downloads`, `coverage("$HOME")` still answers `["$HOME"]`: **the frontier for an ancestor
scope never shrinks on its own**, and the later phase would re-walk everything the earlier ones covered — over rows that
already exist, which is `ScanError::NotVirgin` and the serial repair behind it, the path documented as making the app
look hung for ~15 minutes over a real `/`.

So each phase is preceded by a shallow stitch (`phases/stitch.rs`): read each ancestor of the phase root, upsert its
children, and mark THAT ONE DIRECTORY listed at the CURRENT epoch. No descent, no recursion, and it is honest — we
really did list those directories. Afterwards the descent walks THROUGH the stitched ancestors and cuts at each
genuinely unlisted child, so a covered subtree is skipped, every frontier root is virgin, and a big phase becomes many
small walks. Measured free: 0.2 s across 1,496 walks, zero `NotVirgin` refusals.

Three things it must keep doing, each of which fails silently:

- **Upsert FILES, not only directories.** `listed_children_on` serves a directory's rows as its FULL contents the moment
  `listed_epoch` is non-zero, and that feeds the agent-facing `list_dir` tool — a directories-only stitch would report a
  folder as holding no files, that same instant.
- **Flush between the upserts and the mark.** `MarkDirsListed` is a PK-keyed `UPDATE`, so marking a row still pending in
  an unflushed batch leaves it at `listed_epoch = 0` forever, and the stitch creates the deeper ancestor rows itself.
- **Never mark a directory it couldn't read.** Recording ground no walk can read is the WALK's job
  (`UnreadableCause::Abandoned`); a false mark is absorbed upward by `min_subtree_epoch` all the way to `/`.

It reuses the reconciler's depth-1 diff core (`read_fs_children` + `diff_dir_against_db`), ❌ not `verify_and_correct`,
which recurses into every new subdirectory with `scan_subtree` and would leave exactly the non-virgin nodes the stitch
exists to prevent.

## The verifier had to be told

Today's verifier no-ops on uncovered ground for an accidental reason: the directory has no row to resolve. **After the
stitch, every frontier root has a row.** Left alone, the first listing of a stitched virgin root would resolve it, find
zero indexed children, treat every name on disk as new, and run a full recursive `scan_subtree` per new subdirectory —
on the verifier task, for every folder the user opens ahead of the walker, which is the central behavior this design is
built around. It would also write the same names as a live cover walk (it consults neither the claim nor
`WatchScope::may_walk`), and two writers of one name orphan each other's subtrees.

The fix is durable rather than a flag: `verify_and_correct` bails when the directory's `listed_epoch == 0` AND the
volume has no `scan_completed_at`. A row outlives any runtime flag, so it covers the windows no flag would — between
launch and the first phase, and while drive indexing is off. ⚠️ The second half of the condition is load-bearing in the
other direction: a directory the reconcile cost budget SKIPPED carries the same `listed_epoch == 0` and no cause, and on
a completed volume the verifier is the only thing that heals it.

## Interleaving without preemption

One `cover()` call per GROUP of frontier roots, joined before the next starts. Measured, the join costs nothing (41 s of
real walking against a whole-volume walk's 38.1 s), and the gaps between calls are what give the queue its check points
— ❌ handing one call a whole phase's frontier looks cheaper but the cancel check inside `cover` is not a point the
machine can consult a queue at. Preemption is out of scope: a root the user opens waits for the running walk, which on a
big folder is tens of seconds, and no stitch depth fixes that (`docs/notes/phased-vs-bulk-index-2026-08-14.md` § depth 1
against depth 2).

**How big a group is, is measured rather than predicted** (`grouping.rs`). A frontier root is virgin ground by
definition, so nothing in the index says how much is under it: the only honest estimate is what the last group cost per
root. So the machine starts at one root, and each group is as many as the previous one's pace says fit inside a
one-second interleaving budget, never growing more than 4× per step and never past 16. Big roots therefore keep it at
one, which is what an uninterrupted run is made of, and the two-entry roots an interrupted one leaves behind let it grow
to the cap. ❌ A fixed group size can't work in either direction: 16 roots of `~/Library`'s size is minutes of deafness
to where the user is looking, and one root apiece is the cost measured below.

## What a resume costs

An interrupted phase leaves its frontier as thousands of DEEP, TINY roots (the stitch descends as it goes, so the deeper
a walk got, the smaller what it left). Nothing is lost, but the per-root costs that vanish into the noise at ~1,500
roots are the whole bill at ten thousand — and they are paid per root, not per entry, so quitting later makes it worse.

Measured by `tests::resume_bench` (release, a 100,170-entry synthetic tree, quit once 60% of its rows were indexed,
2026-08-15): **6.2 s to resume over 10,014 frontier roots, 0.6 ms each, against 2.0 s to cover the same tree
uninterrupted.** ⚠️ The bench's default tree is three times that size; pass `CMDR_RESUME_BENCH_DIRS=50000` to reproduce
these numbers.

It started at **185.0 s, 22.1 ms per root**, of which under 0.2 ms was reading the disk. Three costs made up that bill,
and each is worth knowing because each is a shape a future change can reintroduce:

- **A stock-take per root: 75%.** Completion is a coverage descent over the whole volume, which gets more expensive the
  more of the drive is covered, and it ran after every root. It could not even see what that root did — the walk leaves
  its drain to the caller, so its rows and marks were still in the writer's queue. Now: after a drain, which is where
  `run_phase` already asks.
- **The branch set: most of the remainder**, ~59 s of the run at 10,000 branches. It was a `Vec` scanned per path and
  re-sorted per insert, and a phased run adds one entry per covered frontier root, so the cost grew as the square of the
  width. It is a `BTreeMap` keyed by path now, with both of its questions bounded by the PATH rather than by the set:
  `docs/notes/branch-set-cost-2026-08-15.md`. ⚠️ **Keep it that way.** This also taxes the LIVE event path
  (`deepest_containing` runs per event), where the old shape cost 339 µs an event at 2,500 branches against 0.5 µs now.
- **The `cover()` round trip itself: ~2.4 ms per root** (a claim, a branch bracket, a walk thread, a bootstrap read
  connection), now divided by the group size.

An uninterrupted run was never affected by any of it: its roots are big, so the group stays at one, the stock-take is
dominated by walking, and the branch set stays narrow. It read 2.0 s before every one of these fixes and reads 2.0 s
now.

**Ground another walk holds is left to it.** A live search's walk is not ours to serialize; its rows land in the same
index and the next pass asks again.

⚠️ **A walk can also fail to START.** `force_scan` publishes a transient `ShuttingDown` for the whole of its scan-start
prelude, and `cover_context_for` hands a context out only from a `Running` manager — so a rescan racing the machine
makes that root's walk report "did not run". Bounded, and deliberately not worked around: the pass budget re-asks once,
and the volume-root phase re-offers whatever is still frontier. ❌ Don't build a retry loop around it; a caller that
hammers `force_scan` can starve the machine, and the answer to that is not to hammer it.

**The writer drains once per phase, ❌ not once per root.** A blocking flush at the end of every walk was 37.5 s of the
walker standing still over ~1,500 roots. `CoverContext::flush` carries who owes the drain. ⚠️ A walk that defers it
still flushes when its ground BUFFERED live events: those are replayed the moment the branch is finished, and the loop
resolves their paths through a read connection, so against uncommitted rows every one would look like a change under a
missing parent. Two sequences still need a real flush and ❌ must not be batched away: the stitch's upsert-then-mark,
and the completion sequence's stamp-before-collapse. **What that costs is memory, and it is not free.** Measured over a
real `/` (`docs/notes/phased-vs-bulk-index-2026-08-14.md`): draining per root peaks at **411 MB resident / 254 MB phys
footprint**; draining per phase peaks at **773 MB / 613 MB**, because a 6M-row backlog builds up in the writer queue
during the volume-root phase. That buys 12 s of the 82 s arm (1.79× against 2.10×). It is the same peak today's bulk
build already carries (772 MB / 634 MB, which holds the whole aggregation accumulator instead), so phasing does not
raise the high-water mark the app is already sized for — it just stops being the arm that lowers it. **In the shipped
app the whole process peaks at 927–987 MB** covering a real `/` (three runs, 2026-08-15 evening), which is that backlog
plus the frontend host and everything else a harness process doesn't carry. ⚠️ The 16 GB memory watchdog is nowhere near
any of these numbers; if that ever changes, the knob is the phase boundary, not the flush.

## Completion, derived rather than remembered

"The frontier under this root is empty", read off the database. It survives a relaunch, needs no in-session bookkeeping,
and can't drift from what was actually covered. Ground no walk could read doesn't hold it open: the walk records it as
`UnreadableCause::Abandoned`, which takes it out of the frontier and into a list of its own, with a persisted per-volume
backoff offering it again later. ❌ Don't replace this with "the frontier didn't shrink across two passes": it has to
compare sets rather than counts, never terminates on a drive somebody is writing to, and re-pays a full walk on every
launch.

⚠️ The machine takes stock once more after its phase loop, with nothing left to walk. A phase whose frontier is ALREADY
empty walks nothing and drains nothing, so a run that only had to CONFIRM what a previous session covered would
otherwise never reach a stock-take — and a volume killed between its last walk and its stamp would stay unmarked
forever, re-running the machine every launch to rediscover the same thing.

**What churn under a walked scope costs, and what it doesn't.** A folder created on ground the watcher already covers
gets a row nothing has listed, which is frontier by the descent rule — so the volume is marked done only if some
stock-take catches the frontier empty, and that is a RACE against the watcher's next batch. Measured
(`docs/notes/churn-against-completion-2026-08-15.md`, `tests::churn_bench`): 20 and 60 new folders a second never cost a
completion over six trials, a 2,000-folder burst is absorbed by the pass that follows it, and it takes **~200 new
folders a second sustained** to lose the marker — after which **a resume settles the drive in ~2 s**, writing still
going, because a resume's frontier is a few hundred tiny roots. ⚠️ So churn can cost one PASS its completion and ❌
can't hold a drive open indefinitely, which is the limit the derived rule accepts in exchange for never claiming ground
nobody walked.

**A machine that stops short asks for another go**, on a per-volume backoff of 1 min → 5 min → 15 min
(`../completion_retry.rs`, wired in `../DETAILS.md` § "A first index that stopped short"). Each attempt is that same ~2
s resume, so the wait for a drive somebody is writing to hard is minutes rather than "until the next launch". ⚠️ It
changes WHEN the marker lands and ❌ never what earns it: while the frontier is non-empty the drive genuinely isn't
covered and the badge saying so is right. `Machine::finish` logs the frontier it stopped with either way, ❌ never
leaving the two endings indistinguishable in a support bundle.

⚠️ **A bench over this has to watch the MACHINE, not the marker.** The stock-take stamps before `finish` reports the
machine idle, so one that stopped unmarked will never write one — a marker-only poll can't tell that apart from a slow
run and sits out its whole patience. That cost one `home_bench` run its explanation; both benches watch the machine now.

**What a completed volume owes, in the one order that works.** The order is enforced by a FLUSH, not by the numbering:
the read it protects (`local_rescan_reconciles` asking `get_index_status()` inside `start_scan`) goes through a read
connection, and step 3 is minutes of writer-thread work.

1. stamp `scan_completed_at` — **then flush**;
2. the calibration meta (nothing else writes it, so the ETA tier would degrade permanently);
3. `PayLedgerIfUnpaid` — nothing else ever pays the armed `dir_stats` heal, because cover walks send only
   `ComputeSubtreeAggregates` and the latch is disarmed by a full `ComputeAllAggregates`;
4. `BackfillMissingDirStats`;
5. the shallow-sweep ledger (`record_sweep_completed` + both meta keys), or the first shallow anchor after completion
   triggers a full sweep nobody asked for;
6. `ScanComplete` (the WALK is over) plus freshness ⇒ `Fresh`;
7. **flush again**, which is what step 3's full aggregate runs inside;
8. `AggregationComplete` (aggregation is over) plus `DirsUpdated`, now that the sizes on screen are the final ones;
9. collapse the branch set to the volume root. ⚠️ Collapse before the stamp is visible and there is a window where the
   volume is neither branch-confined nor marked complete, and one coalesced shallow anchor inside it truncates the index
   that just finished.

⚠️ **The two terminal reports sit on either side of that flush, and swapping them is a shipped bug, not a tidy-up.**
Step 3's `ComputeAllAggregates` streams a progress tick every ~1% for as long as it runs — **18.8 s over a real `/`**
(debug build, 603,697 directories, 2026-08-15). A status surface reopens on a tick and only the terminal event closes
it, so `AggregationComplete` fired ahead of them left the corner hourglass, an hourglass on every folder row in both
panes, and the step checklist frozen at "Saving folder sizes… 99%" for the rest of the session; only a relaunch cleared
it. A full scan already orders it this way (`../scan_completion.rs`), and the frontend holds the same line from its end
(`apps/desktop/src/lib/indexing/CLAUDE.md`). Anchored by
`tests::completion::nothing_aggregates_after_the_volume_says_aggregation_is_done`.

The sequence fires on the absent→present transition only. Re-running it would rewrite `SHALLOW_SWEEP_AT_KEY` and push
the 24-hour window forward every time.

## The early home signal

`home_covered_at` exists so photo search and folder importance can start when `$HOME` is covered instead of waiting for
`/`. It drives that and NOTHING else — not freshness, not the badge, not rescan routing, not the sweep, not
`scan_completed_at` — and keeping its blast radius to one subscriber is what makes it cheap. It publishes on its own
`lifecycle_bus` channel, which the media and importance schedulers watch alongside `ScanCompleted`, and
`ready_volumes_with_kind` admits a home-covered volume so a relaunch mid-coverage still wires them.

⚠️ **`~/Library` is walked LAST inside the home phase and the signal doesn't wait for it.** Measured by
`tests::home_bench` on David's real home (2026-08-15 evening, release, 5,154,650 entries): home minus `~/Library`
covered in **37.5 s**, all of home in **76.6 s**. So it is half of home's wall clock, and deferring it moves the early
kick **39 s** earlier. It stays entirely in scope; only the ORDER and the signal change. Linux has no single equivalent
pile, so it has none.

⚠️ **The app sees a smaller `~/Library` than this arm does, and nobody has explained the gap.** Three release-app runs
over the real `/` on the same evening put 19–21 s between `home_covered_at` and the `/` phase starting, against this
arm's 39 s (`docs/notes/phased-vs-bulk-index-2026-08-14.md` § "Re-measured on the shipped machine"). The two differ in
scope root and in whether priority roots are installed, so they are not the same arm. Both say the same thing about the
decision, so ❌ don't read either as wrong; ⚠️ do quote the app's figure for what a user waits.

## What the machine reports, and what refuses against it

- **`working`** — a phase queued or running. Half of what EVERY scan entry refuses against (`start_scan`,
  `awaits_its_first_scan`) and what `get_status` reports as `scanning`. ⚠️ ❌ Never the narrower "a walk is running":
  that goes false between frontier roots, and the stitch produces 50–150 of them per phase, so a truncating rescan timed
  into one of those gaps would blank a half-built index and the search dialog's "building your index" state would
  flicker at root cadence.
- **`IndexManager::pending_phases`** — the other half, and the one nobody would think to write down. There is no handle
  to ask `working` between the launch route handing the volume over and the machine's handle landing on the manager, so
  `phases_have_work` answers from this instead: `Owed` from `register_a_phased_start` until the start is taken,
  `BeingStarted` for as long as `PhaseStart::run` is standing the machine up off the registry lock. That second window
  has a driver thread already walking in it (`manager/phased.rs`).
- **`walking`** — a walk is reading the disk right now. Feeds the verifier's `scanning` argument only; between roots
  there is nothing to race.
- ❌ Never `mgr.scanning`: `cover_context_for` returns `None` while it is true, so the machine's own `cover()` calls
  would fail with `ScanInProgress`.
- **The progress shape**: phase label, live entry counter, elapsed, and ❌ no percentage until the volume-root phase. A
  phased run has no knowable total before then, and the design principles forbid a bar parked at 100%. So the
  `ScanStarted` event and the stashed calibration both carry `volume_used_bytes: None`, and
  `writer.set_expected_total_entries` is left unset (its only consumer is flushing-progress, which degrades to no
  percentage rather than a wrong one).
- **The reporter's lifetime is the MACHINE's**, not a walk's, or the 500 ms tick would die and restart 50–150 times a
  phase, taking the progress stream, mid-scan partial aggregation, and the `open_listings` visit poll with it. A walk
  over one frontier root usually finishes well inside the reporter's first sleep, so a per-walk pump would tick almost
  never. `tests::interleaving::the_progress_pump_outlives_the_walks_it_reports_on` anchors it by holding a between-roots
  gap open from inside the event sink and counting what still arrives.

## Where the app's answers enter

Both through existing seams, ❌ neither as an argument bolted onto a launch call. `HostPolicy::priority_roots` is asked
when the machine needs it, so an edited favorites list or a new session's tabs land without a restart.
`HostPolicy::open_listings` is polled on the reporter's tick (the seam's own rate limit) plus once at machine start, so
the folder somebody has open when indexing begins gets its turn immediately rather than half a second later. ❌ Not
`Index::verify_directory`, which fires for the opposite pane, MCP listings, and every refresh.

## The escape hatch

One flag, read at startup, restoring the bulk-build path. Covering in phases changes how every never-completed volume is
launched and it ships into an open beta, so a bad week has to cost a relaunch rather than a rollback.

- **Flip it**: `defaults write com.veszelovszki.cmdr PhasedFirstIndex -bool false`, then relaunch.
  `defaults delete com.veszelovszki.cmdr PhasedFirstIndex` puts it back. Absent means on.
- **Who flips it**: David, or a beta user David hands that line to. It's a user default rather than a settings key
  precisely so it's one pasteable line that needs no JSON editing and works while the app won't start properly; ❌ an
  env var is not enough, since somebody launching from the Dock never sees one. macOS only, which is where the beta is.
- **How it gets here**: the app reads it once (`index_host::phased_first_index`, cached in a `OnceLock` because every
  media-policy setter re-pushes `IndexConfig`) and hands it over as `IndexConfig::phased_first_index`; `set_config`
  mirrors it into this module's atomic. ❌ Don't live-apply it: a volume half way through being covered has no
  meaningful answer to "what if we had built you the other way".
- **What it changes**: `launch_route` sends every never-completed volume back through `start_scan`, and `cover_or_scan`
  does the same for every rescan door. A phased partial therefore takes today's TRUNCATING rebuild, which is the right
  answer (self-healing, and the behavior that was asked for) and the row of the table nobody would otherwise write down.
  `../DETAILS.md` § "What a launch does with the index it finds".
- It has no UI, and needs none.

## Rescan now, and what it means before the first index finishes

A rescan of a volume the machine is still building RESTARTS the phases: covered ground stays covered, and the queue is
recomputed from the host's current answers plus a coverage query per root, so it picks up folders the user has come to
care about since. ❌ Never an error, and ❌ never a truncate. After full coverage "Rescan now" keeps today's meaning
exactly. The doors this closes, and how it sits beside the deferred-rescan mechanism (they answer for disjoint index
states, so neither supersedes the other), are canonical in `../DETAILS.md` § "Every other way a full walk starts".

## Saying which phase is running

The order is the whole feature, so it has to reach the status surface. `Machine::announce_the_phase` emits
`CoveragePhaseStarted` carrying a typed `CoveragePhase` (the crate's own public value, `events/payload.rs`) plus the
phase root, and writes the same value where `get_status` reads it.

**Two doors, because one of them is transition-only.** The EVENT is what a live frontend follows. The STATUS response
(`IndexStatusResponse::coverage_phase`) is what a window that reloaded mid-run reads, alongside `walked_roots` and
`scan_run_kind`, which recover the same way: the last phase of a first index is the rest of the drive, so a joiner with
only the event would sit with no header until the run ended. It reports `None` once the machine has no work left, ❌
never the phase it finished on. Anchored by
`tests::interleaving::a_window_joining_mid_run_reads_the_running_phase_off_the_status`.

❌ **The phase is not the host's to derive.** An app-side home path can disagree with `IndexPathSpace` about firmlinks,
which works on one machine and mislabels on another, so the crate classifies and the host only chooses words. ❌ Nor can
it be read off `CoverageBranchStarted`: those name frontier roots one level BELOW the phase root, so `~/Library` and
`~/Downloads` are indistinguishable, and they are debounced, so a boundary would lag or be skipped.

`CoveragePhase::VisitedRoot` is a phase of its own rather than a flavour of the priority phase: it is ranked, queued,
and run through `run_phase` like any other. Nothing renders it apart today (the frontend maps it to the priority-folders
header, `apps/desktop/src/lib/indexing/indexing-steps.ts`), which is a wording decision the host can revisit without the
crate changing.

**A phase announces itself again when an interlude ends, ❌ not only when it starts.** Without a re-announcement on the
way back the header names that folder for the rest of the outer phase — observed sitting on "Indexing the folders you
use most" for two minutes while the machine walked `/`. `walk_all` therefore re-announces whenever `take_a_visit`
reports it actually ran one. ⚠️ The re-announcement is the COVERAGE event alone, ❌ never `set_phase_for` as well: the
activity phase is `Scanning` throughout, so a second `PhaseChanged` would carry no news and would inflate the app-wide
debug timeline. Anchored by `tests::completion::the_outer_phase_says_so_again_after_a_visited_root_interrupts_it`.

## Stop and Forget, against a half-covered drive

The drive badge offers both while a drive is scanning (`driveIndexMenuActions('scanning')`), and they sit either side of
the persisted branch set, which is the one fact `launch_route` reads to tell a phased partial from an interrupted bulk
scan.

- **Stop** (`Index::disable_volume`) cancels the running walk and clears the queue through `PhaseHandle::stop`, keeps
  the database, and leaves the branch set on it. `stop_indexing`'s `branches::forget` drops the IN-MEMORY set only. So
  the next launch routes `CoverInPhases` and adds to what this session bought. ❌ Getting this wrong turns a resume into
  a rebuild, and the user's own action is what costs them every folder covered so far.
- **Forget** (`Index::forget_volume`) keeps today's meaning: the database goes, and the branch set inside it goes with
  the coverage it describes, so the next launch is a clean first index.

⚠️ One window is deliberately not resumable: the branch set is written as each walk FINISHES (`finish_branch_coverage`),
so a stop taken inside the very first walk leaves the stitch's rows with nothing recording what ground they cover. The
next launch rebuilds them, by the same rule that throws away an interrupted bulk scan. Tests: `tests/menu_actions.rs`.

## The database is prepared for a walk through writer MESSAGES

`prepare_database_for_a_walk` runs only for an `Activation::WriterOnly` start, on its own write connection, before any
writer exists. A phased start is `IndexTheVolume` and its writer is already live, so a second write connection is
exactly what the single-writer rule forbids: `manager/phased.rs` sends the epoch, `volume_path`, and the
exclusion-policy stamp through the writer instead.

⚠️ **Without the stamp nothing ever converges, silently.** An absent stamp makes `index_predates_exclusion_policy`
answer yes, and `walk_coverage` then short-circuits every query to `Frontier` over the whole scope: the frontier never
shrinks, every root after the first walk is non-virgin, and each takes the serial repair. It looks exactly like the
stitch not working. The stamp's own precondition carries across as `entry_count <= 1 || we-just-truncated` — read as
"only after a truncate" it never stamps a fresh install, read as "always" it blesses rows written under an older policy,
and both misreadings are silent. A STALE fingerprint gets its own arm: truncate once, re-stamp, and drop the completion
markers and the branch set with the rows they describe.
