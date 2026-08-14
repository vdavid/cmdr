# Covering a volume in phases

Read this before any non-trivial work in `lifecycle/phases/`. Must-know guardrails are in `CLAUDE.md`.

**A volume with no completed scan is covered, ❌ not scanned.** `resume_or_scan`'s third answer, beside journal replay
and `start_scan`, taking only the no-`scan_completed_at` case. There is no first full scan any more: the whole drive is
the LAST phase of the same `coverage`/`cover` mechanism a search-driven walk uses, so a quit keeps every second it
bought instead of truncating and starting over.

**The shape of a run.** Ask the host which folders matter to this user (`HostPolicy::priority_roots`, an ORDER and
nothing else), then `$HOME`, then the volume root. Per phase: stitch down to its root, ask for its frontier, walk those
roots one at a time with the visit queue checked between them, drain, take stock. `MAX_PASSES_PER_PHASE` re-asks once
after the drain (a root can expose new ground while it is being walked); past that whatever is left is this session's
loss and the next launch asks again. ❌ That is a PASS budget, never a completion rule.

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

One `cover()` call per frontier root, joined before the next starts. Measured, the join costs nothing (41 s of real
walking against a whole-volume walk's 38.1 s), and it is what gives the queue its check points — ❌ handing one call a
whole phase's frontier looks cheaper but the cancel check inside `cover` is not a point the machine can consult a queue
at. Preemption is out of scope: a root the user opens waits for the running walk, which on a big folder is tens of
seconds, and no stitch depth fixes that (`docs/notes/phased-vs-bulk-index-2026-08-14.md` § depth 1 against depth 2).

**Ground another walk holds is left to it.** A live search's walk is not ours to serialize; its rows land in the same
index and the next pass asks again.

**The writer drains once per phase, ❌ not once per root.** A blocking flush at the end of every walk was 37.5 s of the
walker standing still over ~1,500 roots. `CoverContext::flush` carries who owes the drain. ⚠️ A walk that defers it
still flushes when its ground BUFFERED live events: those are replayed the moment the branch is finished, and the loop
resolves their paths through a read connection, so against uncommitted rows every one would look like a change under a
missing parent. Two sequences still need a real flush and ❌ must not be batched away: the stitch's upsert-then-mark,
and the completion sequence's stamp-before-collapse. The cost is a larger writer backlog — bounded by the same
`InsertEntriesV2` batching a full scan already runs at, and measured at no change in peak RSS (408 MB either way).

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
6. freshness ⇒ `Fresh` plus the terminal events, in the order the frontend's `resetAggregation()` handshake expects;
7. **flush again**, then collapse the branch set to the volume root. ⚠️ Collapse before the stamp is visible and there
   is a window where the volume is neither branch-confined nor marked complete, and one coalesced shallow anchor inside
   it truncates the index that just finished.

The sequence fires on the absent→present transition only. Re-running it would rewrite `SHALLOW_SWEEP_AT_KEY` and push
the 24-hour window forward every time.

## The early home signal

`home_covered_at` exists so photo search and folder importance can start when `$HOME` is covered instead of waiting for
`/`. It drives that and NOTHING else — not freshness, not the badge, not rescan routing, not the sweep, not
`scan_completed_at` — and keeping its blast radius to one subscriber is what makes it cheap. It publishes on its own
`lifecycle_bus` channel, which the media and importance schedulers watch alongside `ScanCompleted`, and
`ready_volumes_with_kind` admits a home-covered volume so a relaunch mid-coverage still wires them.

⚠️ **`~/Library` is walked LAST inside the home phase and the signal doesn't wait for it.** Measured on David's real
home (2026-08-15, release, 5,230,809 entries): home minus `~/Library` covered in **43.1 s**, all of home in **82.5 s**.
So it is 48% of home's wall clock, and deferring it moves the early kick 39 s earlier. It stays entirely in scope; only
the ORDER and the signal change. Linux has no single equivalent pile, so it has none.

## What the machine reports, and what refuses against it

- **`working`** — a phase queued or running. What EVERY scan entry refuses against (`start_scan`,
  `awaits_its_first_scan`) and what `get_status` reports as `scanning`. ⚠️ ❌ Never the narrower "a walk is running":
  that goes false between frontier roots, and the stitch produces 50–150 of them per phase, so a truncating rescan timed
  into one of those gaps would blank a half-built index and the search dialog's "building your index" state would
  flicker at root cadence.
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
  phase, taking the progress stream, mid-scan partial aggregation, and the `open_listings` visit poll with it.

## Where the app's answers enter

Both through existing seams, ❌ neither as an argument bolted onto a launch call. `HostPolicy::priority_roots` is asked
when the machine needs it, so an edited favorites list or a new session's tabs land without a restart.
`HostPolicy::open_listings` is polled on the reporter's tick (the seam's own rate limit) plus once at machine start, so
the folder somebody has open when indexing begins gets its turn immediately rather than half a second later. ❌ Not
`Index::verify_directory`, which fires for the opposite pane, MCP listings, and every refresh.

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
