# Cover-walk internals details

Standing a walk up (`bootstrap.rs`) and keeping two walks off the same ground (`live.rs`). Read this before any
non-trivial work here: editing, planning, reorganizing, or advising. The walk itself, its outcome type, and the registry
it activates through are `../CLAUDE.md` and `../DETAILS.md`.

## One writer per database, and one walk per patch of ground

The hazard is the one this file states for the registry generally: two writers on one database own separate id counters
and separate `AccumulatorMaps`, so they produce primary-key collisions and inflated `dir_stats`. A walk answers it at
three levels, each closing a case the one above it can't see.

1. **Reuse the volume's writer** (`state::cover_context_for`). A `Running` volume hands its own writer over; the walk
   never stands a second one up.
2. **Don't walk a volume that's being scanned.** `cover_context_for` answers `None` while `mgr.scanning` is set, and
   `context_for_walk` turns that plus `Initializing` into `NoCoverContext::ScanInProgress`. The scan already covers
   everything a search would have walked, and running beside it isn't merely redundant: both allocate fresh ids for the
   same names, `insert_entries_v2_batch` is `INSERT OR IGNORE`, and the row that loses takes its subtree with it. With
   no index at all, the lock-first reservation inside `start_indexing_for` decides who builds one.
3. **Claim the frontier roots** (`live.rs`). One writer isn't enough on its own, because two walks THROUGH that one
   writer over the same directories hit the same `INSERT OR IGNORE` collision. Decision 11 makes this routine: a refined
   query re-asks `coverage` while the first query's walk is still running, and that first walk keeps going. So
   `cover::start` claims each root on the caller's thread, skips any that overlaps a live one in either direction
   (component-aware, so `/a/bc` is not inside `/a/b`), and reports the skipped ones as
   `CoverWalk::covered_by_another_walk`. The claim is owned by the walk thread, so the ground frees up on the completion
   path, the cancel path, and a panic alike. `ground_being_walked` answers the same question without taking anything,
   which is what `Index::coverage` reports as `CoverageMap::being_walked`: a caller can then tell that a walk would get
   it nothing BEFORE committing to one, and wait for the walk that holds the ground instead of answering empty.

**The claim is also what keeps a rescan off a live walk.** `start_scan` asks `ground_being_walked` over the whole volume
and refuses while anything answers, because a search walk sets no `scanning` flag and a truncate under one blanks rows
it is still writing. That rule is canonical in `../DETAILS.md` § "The two single-flight questions a scan has to ask";
what matters here is that the claim, not a flag, is the thing being read.

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

A root the chain had to CREATE is also emitted to the walk's consumer, once, ahead of its listing
(`cover.rs::emit_root`, counted in `entries_found` / `dirs_found` so what a consumer saw and what the walk added stay
one number). Why: a walk reports a directory's CONTENTS, and a reader of the index answers for rows the index already
held, so a row this walk invented is one nobody else will ever report — which made a search scoped to a folder answer
with that folder over an indexed drive and not over an unindexed one. ❌ The ancestors above it are NOT emitted: the
frontier is cut inside whoever's scope asked for the walk, so anything above the root is outside it.

## Where a cover test goes

Four test files, split by the harness a test needs rather than by what it asserts; a test that reaches for the wrong one
pays for a whole fixture it doesn't use.

- `tests.rs` — the temp-tree `Fixture`: an index that already exists, over a real directory the LOCAL walker reads off
  the disk. Frontier materialization, the non-virgin repair, claims, and cancellation live here.
- `cold_drive_tests.rs` — the `ColdDrive` harness: a drive with NO index, driven through the public `Index` handle so
  the walk runs the real activation. Bootstrap, freshness, branches, and the two switches are here, because those are
  only observable from outside.
- `network_tests.rs` — the `Volume`-trait half, over an `InMemoryVolume` and the hand-rolled backends in
  `test_support.rs`. Touches no disk at all.
- `bench.rs` — the `#[ignore]`d parallel-vs-serial primitive measurement
  (`docs/notes/cover-walk-primitive-2026-08-05.md`).

`test_support.rs` holds what more than one of them needs: `drain`, and (from `network_tests.rs`) the `Share` harness
plus the instrumented `Volume` doubles. A fixture only one file uses stays in that file.
