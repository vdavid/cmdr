# Cover-walk details

The walk itself (`mod.rs`), which primitive reads a root (`ground.rs`), standing one up (`bootstrap.rs`), and keeping
two walks off the same ground (`live.rs`). Read this before any non-trivial work here: editing, planning, reorganizing,
or advising. What the registry and the phase machine owe a walk, and what a walk owes them back, is `../DETAILS.md`.

## What the walk does

`../../read/coverage.rs` says which folders under a scope nothing has listed; this walks them. Every row goes into the
volume's real index through its ONE writer (Decision 2 of `docs/specs/unindexed-search-plan.md`), so the work outlives
the search that paid for it and the next search over the same ground walks less.

**The walk is deliberately caller-agnostic**, and search is the first caller rather than the only one. Getting a
folder's recursive size on demand (pressing Space on it) is the same question over the same frontier, so it wants a
trigger and a consumer for the batches, ❌ never a second scoped-walk primitive beside this one. That is why the API is
"give me a frontier, take batches and an outcome" rather than anything search-shaped, and why nothing here knows what a
query is.

**Which ground, and which walk reads it** (`Ground`). Every volume kind falls in one of two halves, and that is the ONE
per-kind branch in the whole coverage concept: a local filesystem is read by the guarded walker, and everything else — a
share, a phone, whatever backend comes next — through its `Volume` (`../../network_scanner/DETAILS.md` § "The scoped
cover walk"). Downstream of a discovered entry the two are identical: same writer, same epochs, same `dir_stats`, same
frontier query, same descent rule. `Ground::under` resolves the half from the kind on the registry instance the writer
came from, and answers `None` when a trait-scanned volume has been ejected since the coverage answer.

**Two primitives on the local half, and which one runs.** A frontier node is virgin ground by definition, so this is a
bulk add and the parallel guarded walker wins outright — 3.2–5.8x over the serial reconcile with identical row counts on
four real trees (`docs/notes/cover-walk-primitive-2026-08-05.md`). The serial reconcile is kept as the REPAIR path, for
the one case the parallel walker can't take: a frontier node the index already holds rows under (`ScanError::NotVirgin`,
see `../../scanner/DETAILS.md` § "Three scan roots"). It compares by name and writes only differences, which is exactly
that case's shape. The trait half needs no such split — it is add-only per directory, so it simply takes the case. ❌ No
path ever deletes: covering is add-only work.

**The repair path REPORTS like every other primitive, and that is load-bearing.** A live search answers with the index's
covered half plus what the walk hands back. The covered half is an unpruned arena scan — it DOES serve rows under a
frontier root, which is how the pre-existing ones still show up — but that arena was read before the walk started, so a
row the walk creates reaches the search through the walk or through nothing. So the repair hands `reconcile_subtree` the
same two things the parallel walker gets (`LiveWalk`: the `EntrySender`, and the `WalkHeartbeat` it pulses once per
directory read) and returns a `ScanSummary` built from `ReconcileSummary::added` / `added_dirs`. Created rows only: an
updated row was already in the arena, and sending it would double it in the results.

The pulse is a separate obligation from the rows, and skipping it costs two different things. `foldersFound` and the
dialog's "N folders scanned" are read off `WalkHeartbeat::dirs_scanned`, never off the entries, so a pulseless repair
fills a list while claiming it walked nothing. And a SECOND run waiting on this ground judges the walk by that same
number (`search/execute/live_run.rs`'s `OTHER_WALK_STALL`, 30 s), so it reads the unmoving zero as a stall and answers
with a lower bound rather than waiting for a walk that is working.

Handing back `(None, Covered)` instead is a silent wrong answer, not a missing nicety: search a folder, then search its
parent, and the parent's frontier root is exactly this case — the second search returned only the first one's rows,
reported `foldersFound: 0`, and stamped `coverage.complete: true` over it. Regression-locked by
`a_repaired_frontier_node_reports_the_rows_it_wrote`, `a_repair_reports_the_directories_it_read`, and
`a_repair_whose_consumer_left_still_covers_the_ground` for the bounded channel that reporting now parks on.

⚠️ What the repair still does NOT report is `WalkHeartbeat::abandoned`. Its `unreadable_dirs` are dominated by ordinary
races (a directory deleted between the listing and the read, ~750 an hour on a build machine), so feeding them in would
put "this list is a lower bound" on nearly every repaired search. The cost of leaving it: a directory the repair
genuinely couldn't read is never marked listed, so it stays frontier and the NEXT search offers it, but THIS search says
`Completed` without naming it.

⚠️ One row shape still reaches nobody: a child that was a FILE and is now a DIRECTORY counts as an update, so it goes
out through neither half — the arena holds the stale file row and the walk doesn't emit the new dir row. Narrow enough
to accept (it needs a type change under an unlisted frontier root between two searches), and the next search over that
ground, by then covered, serves it from the arena.

**What it costs to cover a whole volume this way**, measured over a real 6.06M-entry `/` against today's
truncate-and-bulk-build: `docs/notes/phased-vs-bulk-index-2026-08-14.md`. Two findings a caller planning many walks over
one volume needs. First, the shallow stitch and the frontier query are free (0.2 s and 5 ms across 1,496 walks), so the
cost is never in the bookkeeping. Second, and load-bearing: a directory a walk couldn't read must be RECORDED, or every
later walk over an ancestor scope offers it again and pays the failing read again — 101 s of 147 s of walk time on a
machine with 76 such directories. The walk does that for itself now (`UnreadableCause::Abandoned`,
`../../scanner/DETAILS.md` § "Ground the walk couldn't read"), so a caller planning many walks needs no bookkeeping of
its own. ❌ Don't reach for `WalkHeartbeat::abandoned_count` if you ever need the signal in-process: it counts stall
timeouts and consecutive-failure pruning, and read zero for every walk in every arm of that measurement — the failing
`readdir` case, which was 100% of what fired, never touched it.

**The backend's scan session brackets the WHOLE frontier**, not each root: over SMB that's a pool of extra connections
(`begin_scan_session` / `end_scan_session`), and opening one per frontier root would pay the setup repeatedly inside one
walk. ❌ Nothing between the two calls may return early — `walk_frontier` keeps the loop in a helper for exactly that
reason, so a cancelled walk can't leave the pool standing.

**Terminal states.** `CoverOutcome.cancelled` separates "the index answers for this scope now" from "somebody stopped
us", which are different phases in the UI. Neither is a failure: a cancelled walk still left every directory it read
marked, so the next walk resumes rather than restarts. One frontier root that can't be walked doesn't stop the others —
it stays frontier, and the next `coverage` call names it again. The one thing that DOES stop them is the volume itself
going away, below.

**A dead volume is concluded once, not per root** (`RootOutcome::VolumeGone`). Re-discovering that a share has stopped
answering costs whatever the frontier size multiplies it by: over a share a listing that can't be answered runs to
`LIST_TIMEOUT` (120 s), a cold NAS hands the walk a frontier of thousands of roots, and browsing that share drops the
in-flight budget to one (`network_scanner/scan_pace.rs`), so those roots serialize. So the loop stops at the first root
whose walk ends `VolumeScanError::is_terminal_disconnect()`.

- **The trigger is exactly that predicate**, which the whole-volume scan's completion handler already acts on: a typed
  `DeviceDisconnected`, or the consecutive-failure backstop reaching the same verdict about a reset that arrived
  untyped. ❌ Never widen it to "the root failed". A `Timeout` is one wedged directory on a share that is otherwise
  answering, and an `EmptyRoot` is not a health claim at all — reading either as a disconnect would strand every root
  behind an ordinary bad folder.
- **Skipping is sound because one walk is one volume.** Every root in a frontier belongs to the same `volume_id`, so it
  resolves to one `Arc<dyn Volume>` and, over SMB, one session (connection health belongs to the SHARE, not to a mount
  root — `crates/cmdr-smb/src/volume/state.rs`). A second export that is still up is a different volume id and a
  different walk, so partial reachability can't be hit here.
- **Skipped is not condemned, and that is what makes it safe.** A root the loop never reaches is walked by nothing, so
  it is marked by nothing and stays frontier — indistinguishable from a root that failed. The walk declines to pay for a
  listing that was going to fail; it gives up no recoverable ground. The marking rule this rests on is single-sourced in
  `network_scanner/DETAILS.md` § "A failed listing is held until the share answers again".
- **The same conclusion is drawn elsewhere in this crate**, which is the precedent to copy rather than a new idea:
  `media_index/network/enrich.rs` pauses a whole enrichment pass on the first typed disconnect and resumes off the
  registration bus, for the reason a retry would only re-hit the dead transport.
- **What it does NOT catch**, so nobody reads the bound as tighter than it is: a root whose OWN listing fails is its own
  branch (`network_scanner/cover_scan.rs`, the `dir_path == root` arm), and an untyped `IoError` or a `LIST_TIMEOUT`
  there is not a health claim, so a share that is dead at every root listing still pays one round trip per root. The
  same holds one step earlier, in `bootstrap::ensure_walkable`: a root with no `entries` row is stat'd before it is
  walked, and that stat is per root too.

**Ownership and cancellation.** `cover_context_for` matches `IndexPhase::Running` only, so a walk reuses the volume's
existing writer and never stands a second one up (two writers on one DB race the id counter and the accumulator maps).
Cancel through the `CancellationToken` the caller passed to `Index::cover`, never through the handle: the handle owns a
`Receiver` and so can't be shared with the thread that decides to stop it (a closing dialog, a quitting app), which is
why `CoverWalk` has no `cancel` of its own. DROPPING the handle does not stop the walk either (Decision 11 — walking is
coverage work, matching is query work, so a superseded query keeps its walk). `finish` drops the batch channel BEFORE
joining, so a caller that stopped reading can't deadlock against a walk parked on a full one.

**The channel is bounded at eight batches.** A consumer that falls behind slows the walk rather than growing a queue to
the size of the subtree; each batch already carries up to 2 000 entries.

## One writer per database, and one walk per patch of ground

The hazard is the one this file states for the registry generally: two writers on one database own separate id counters
and separate `AccumulatorMaps`, so they produce primary-key collisions and inflated `dir_stats`. A walk answers it at
three levels, each closing a case the one above it can't see.

1. **Reuse the volume's writer** (`state::cover_context_for`). A `Running` volume hands its own writer over; the walk
   never stands a second one up.
2. **Don't walk a volume that's being scanned.** `cover_context_for` answers `None` while `mgr.ground_in_flux` is set,
   and `context_for_walk` turns that plus `Initializing` into `NoCoverContext::ScanInProgress`. The scan already covers
   everything a search would have walked, and running beside it isn't merely redundant: both allocate fresh ids for the
   same names, `insert_entries_v2_batch` is `INSERT OR IGNORE`, and the row that loses takes its subtree with it. With
   no index at all, the lock-first reservation inside `start_indexing_for` decides who builds one.
3. **Claim the frontier roots.** One writer isn't enough on its own, because two walks THROUGH that one writer over the
   same directories hit the same `INSERT OR IGNORE` collision. Decision 11 makes this routine: a refined query re-asks
   `coverage` while the first query's walk is still running, and that first walk keeps going. So `cover::start` claims
   on the CALLER's thread, before it spawns anything, and reports what it didn't get as
   `CoverWalk::covered_by_another_walk`; the claim then travels into the walk thread, so the ground frees up on the
   completion path, the cancel path, and a panic alike. Everything the table itself does — the overlap rule, the modes,
   the handover, `ground_being_walked` — is `live/DETAILS.md`.

**The claim is also what keeps a rescan off a live walk, and it is the scan entries' own single-flight answer.** Both
take the volume root `Exclusive`ly (`IndexManager::claim_the_volume`) instead of reading a flag: a search walk sets no
flag at all, and a truncate under one blanks rows it is still writing. That rule is canonical in `../DETAILS.md` § "The
two single-flight questions a scan has to ask"; what matters here is that one table answers for every holder, and that a
whole-volume claim outlives the call that takes it.

The deferred caller loses nothing durable: the other walk's rows land in the same index, and Decision 12 makes them
visible to the very next query — which is exactly how Decision 11 already says a superseded query recovers its
predecessor's ground, from the index rather than from a replay. ❌ Don't replace this with a shared-subscriber fan-out
to get live batches for the shared ground; it needs per-subscriber filtering and per-subscriber completion (one root is
done while the walk moves on to the next), and there is no second consumer today to shape either against.

### A claim that takes nothing makes no walk

When the claim leaves EVERY requested root to another walk, `cover::start` hands back a `CoverWalk` with no thread
behind it (`CoverWalk::took_no_ground`): the batch channel is closed on arrival, so `next_batch` is `None` at once, and
`finish` answers with a zero `CoverOutcome` that is NOT `cancelled` — nothing ran, and nothing stopped it.

That is a correctness-of-reporting fix, not a saved thread. A walk thread runs `walk_frontier` whatever its frontier
holds, and `walk_frontier`'s tail commits the writer, because a search-driven walk takes
`FlushOnFinish::BeforeReporting` (the marks matter more than the rows). The writer is one thread behind one bounded
queue per database, so that commit parks behind everything already queued — during a drive's first index, behind the
first index. Measured in the app: `Cover: 0 entries over 0 frontier roots in 5.8s (5.8s of it waiting on the writer)`,
with `Index::cover` itself returning in 33-104 µs and `CoverWalk::finish` eating the whole wait. The search that asked
showed "0 matches so far" for all of it, and only afterwards reached the phase that says whose walk it is queued behind.
Full measurement: `docs/notes/cover-no-ground-block-2026-08-15.md`.

So the rule is a shape, not a special case: **everything a walk owes on the way out is owed for the ground it took**.
Nothing is skipped by taking this path — an empty claim frees no ground, so there is no branch set to finish and no
`rescan_request::run_if_owed` to fire (the walk that holds the ground runs it when IT lets go). ❌ Don't add work to
`start` or `walk_frontier` that a no-ground request would still owe; put it where the ground is.

## What has to exist before a walk can run (`bootstrap.rs`)

A walk needs a database with a writer behind it, an epoch to stamp listed directories with, and an `entries` row to
resolve its root against. A volume nobody ever indexed has none of them, and a volume indexed yesterday can still be
missing the last one — a folder created since its parent was listed has no row either, so this is not only a cold-drive
concern. ❌ Nothing in here lists a directory or claims coverage: it creates rows at `listed_epoch = 0` and the walk
earns the rest.

**Standing an index up for a walk** (`context_for_walk`). A volume that's already `Running` hands its writer over
untouched. Otherwise the bootstrap starts one with `Activation::WriterOnly` — the same `start_indexing_for` every enable
funnels through, minus `resume_or_scan`, so the lock-first reservation, read-handle install, failure supervisor, and
maintenance timer can't drift between the two. What that start does differently:

- **Seeds `current_epoch` and stamps `EXCLUSION_POLICY_KEY`, the latter ONLY while the database holds nothing past the
  `ROOT` sentinel** (`prepare_database_for_a_walk`). An empty database satisfies any exclusion policy trivially, which
  is the same argument that licenses the stamp right after a `TruncateData` — the only other moment that holds
  (`../../store/CLAUDE.md`). Load-bearing: without the stamp `coverage` trusts nothing the walk writes, so every later
  search re-walks the same ground and a cold drive never converges.
- **Never inherits the Fresh a journal replay earns.** It doesn't replay, so a persisted index it didn't verify loads
  Stale exactly like a non-journaled one (and bumps the epoch on the way, as any launch-as-Stale does).
- **Evicts a database whose coverage claims this build refuses** (`evict_an_index_no_walk_can_trust`), before the store
  is opened. An index that predates the current exclusion policy counts as covering nothing, and only a full scan can
  re-stamp it — which a writer-only start is by definition not getting. The rationale, the cost, and the two cases that
  already evicted: `../../resources/DETAILS.md` § "Rebuilt-from-scratch coverage is EVICTED, not refilled".
- **Classifies the volume by how its ground is read**, so the only thing it refuses is a volume nothing has mounted.
  Same typed facts and the same predicates the enable command uses (MTP's id vocabulary first — `mtp://…` is not a path
  a `statfs` can answer for — then `routes_to_local_external` over a live smb2 session and the network-filesystem flag),
  with the `statfs` probe bounded on a thread of its own: the async timeout `local_external::classify` uses needs a
  runtime this path doesn't have, and a probe that won't answer IS the answer (`MountFacts::UNPROBEABLE` reads as
  network, which routes to the trait walk — the right walk for a mount whose `statfs` won't return). ⚠️ The kind names
  the SCAN PATH, not the protocol: an NFS or WebDAV mount is classified `Smb` because that is what every trait-scanned,
  mount-rooted, journal-less volume needs. Refusing it instead would make a search of it silently wrong, and calling it
  local would point the guarded walker at syscalls that block for minutes.
- **Runs with drive indexing turned off** (Decision 13). Neither the master switch nor the sticky per-drive
  `user_disabled` veto gates it: both stop work the app does uninvited, and a search is a read the user just asked for.
  The carve-out is one condition in `start_indexing_for` (`activation == IndexTheVolume`), so `WriterOnly` is the only
  start that passes a closed gate — and it starts nothing autonomous, which is what makes that safe.

**Active is not indexed.** A writer-only instance makes a volume `is_active` while nothing has ever scanned it, so
`Index::start_volume` asks `awaits_its_first_scan` (Running, not scanning, no `scan_completed_at`) and force-scans
instead of reporting `Started` at a volume that would never index. A first scan someone stopped leaves the same shape
and had the same problem. Two consequences deliberately left standing: `VolumeIndexStatus.enabled` reads true (the
frontend renders `freshness: null` gray, so the badge is honest), and the first-connect "index this drive?" toast
suppresses itself on a drive a search already walked.

**Materializing a path's chain** (`ensure_walkable`). The common case is one lookup — a frontier node a coverage answer
found by descending into its parent's listing already has a row. Otherwise the chain from the volume root down is
created through the writer (`UpsertEntryV2`, resolved by `(parent_id, name)`, so a row arriving meanwhile is updated
rather than duplicated past `idx_parent_name_folded`), one flush per created component, each carrying the real
directory's metadata. Where that metadata comes from is `Ground`'s to answer: an `lstat` on the local half, and a
deadline-bounded `stat_one_directory` round trip on the trait half (whose timeout races the task's JOIN handle for the
same reason listings do — dropping a `get_metadata` future mid-round-trip wedges a phone). It declines rather than
guesses in three cases: a chain running through a FILE row (the stale file→dir type change `reconcile_subtree` escalates
on — parenting under a file id orphans everything below), a path that isn't a directory any more, and a symlink (stored,
never descended into, so a walk rooted below one would attribute another directory's contents to it).

A root the chain had to CREATE is also emitted to the walk's consumer, once, ahead of its listing (`mod.rs::emit_root`,
counted in `entries_found` / `dirs_found` so what a consumer saw and what the walk added stay one number). Why: a walk
reports a directory's CONTENTS, and a reader of the index answers for rows the index already held, so a row this walk
invented is one nobody else will ever report — which made a search scoped to a folder answer with that folder over an
indexed drive and not over an unindexed one. ❌ The ancestors above it are NOT emitted: the frontier is cut inside
whoever's scope asked for the walk, so anything above the root is outside it.

## Where a cover test goes

Five test files, split by the harness a test needs rather than by what it asserts; a test that reaches for the wrong one
pays for a whole fixture it doesn't use.

- `tests.rs` — the temp-tree `Fixture`: an index that already exists, over a real directory the LOCAL walker reads off
  the disk. Frontier materialization, claims, and cancellation live here.
- `repair_tests.rs` — the same fixture, over the one case the parallel walker refuses: a frontier node the index already
  holds rows under. What the repair keeps, what it reports, and what a departed consumer does to it.
- `cold_drive_tests.rs` — the `ColdDrive` harness: a drive with NO index, driven through the public `Index` handle so
  the walk runs the real activation. Bootstrap, freshness, branches, and the two switches are here, because those are
  only observable from outside. The harness stays in the parent and the tests sit in `cold_drive_tests/`, by subject:
  `activation.rs` (the index a walk stands up, and what a later enable does to it), `switches.rs` (both indexing
  switches govern background work only), `intent.rs` (only a user's ask writes per-drive intent), `branches.rs` (what
  the walk leaves watched and every path that releases it), `walkable.rs` (which drives can be walked, and by which
  walker), `rescans.rs` (the rescan a live walk defers and later fires). A new test goes in the subject it asserts
  about; a fixture more than one subject needs goes up into the parent.
- `network_tests.rs` — the `Volume`-trait half, over an `InMemoryVolume` and the hand-rolled backends in
  `test_support.rs`. Touches no disk at all.
- `bench.rs` — the `#[ignore]`d parallel-vs-serial primitive measurement
  (`docs/notes/cover-walk-primitive-2026-08-05.md`).

`test_support.rs` holds what more than one of them needs: `drain`, the temp-tree `Fixture` (in
`test_support/fixture.rs`, shared by `tests.rs` and `repair_tests.rs`), and the `Share` harness plus the instrumented
`Volume` doubles `network_tests.rs` runs on. A fixture only one file uses stays in that file.
