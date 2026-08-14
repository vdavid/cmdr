# Phased, priority-driven drive indexing

Status: **`M0`, `M1`, the `Abandoned` cause with its heal, and `M2` (the stitch plus the phase machine) are built**;
`M3` onward remain. Branch: `worktree-david+phased-machine`. What M2 shipped, and where it now lives:
`crates/cmdr-index/src/indexing/lifecycle/phases/CLAUDE.md` + its `DETAILS.md`.

**The gate, and why it passed after reading as a fail.** Measured, the phased shape came in at 4.70× the bulk build,
against a 1.5× bar. The decomposition is what settled it, and it is the number to keep in mind while building:

- **The walking itself is at parity**: 41 s of real walking against the bulk build's whole 38.1 s run, once the
  wasted-effort bug below is gone. Many small walks keep the machine as busy as one whole-volume walk. The plan's join
  rule (one root at a time, check the queue between) costs nothing measurable.
- **69% of the gap was one bug**, not phasing: a directory failing to list with a non-permission errno kept no cause, so
  every later phase re-offered it and re-paid the timeouts (1,497 `ETIMEDOUT` directories inside a wedged MTP mount on
  the measured machine). **Shipped as `UnreadableCause::Abandoned`**, and it was live in the shipped build, making every
  search scoped above `~/Library` re-pay them.
- **The rest is flush cadence.** One `cover()` call per frontier root ends with a blocking writer flush, so with ~1,500
  roots the walker and the writer never overlap: 37.5 s of the arm is the walker standing still. Draining once per phase
  recovers ~30 s of it. **This is a required part of the machine, ❌ not a later tuning pass.**
- **The stitch and the coverage queries cost 0.2 s combined**, 0.1% of the arm, with zero `NotVirgin` refusals across
  every arm. The machinery this plan spends its design effort on is free.

Full evidence: `docs/notes/phased-vs-bulk-index-2026-08-14.md`.

**The product decision, in David's words**: a new user seeing real sizes on the folders they care about within the first
second is worth roughly half a minute more on the total, and image-indexing speed is explicitly not what this is for.
Build it correctly, solidly, and elegantly rather than quickly.

Turn first-run drive indexing from "one full scan of `/`, all or nothing, a few minutes before anything is useful" into
"the folders this user actually cares about are indexed within seconds, and the rest of the drive fills in behind them".
Nothing is truncated on the way, so every second spent walking stays bought.

**The whole drive still gets indexed, and every existing promise stays true.** This changes the ORDER of the walk and
makes it interruptible, ❌ not its extent. `scan_completed_at` keeps its exact meaning, freshness keeps its exact
meaning, the onboarding and website copy stay accurate, and an upgraded install keeps everything it had. The one
addition is a small early signal so photo search and folder importance can start when home is done instead of waiting
for `/`.

Area docs to read before working here: `crates/cmdr-index/src/indexing/CLAUDE.md` (+ its `DETAILS.md`),
`crates/cmdr-index/src/indexing/lifecycle/CLAUDE.md`, `.../lifecycle/cover/CLAUDE.md`, `.../watch/CLAUDE.md`,
`.../scanner/CLAUDE.md`, `.../read/CLAUDE.md`, `crates/cmdr-index/src/indexing/handle/DETAILS.md` (the public-surface
audit), `apps/desktop/src/lib/indexing/CLAUDE.md`, `apps/desktop/src-tauri/src/favorites/CLAUDE.md`. Product framing:
`docs/design-principles.md`.

## Why

Ordered by how much each survives a disappointing benchmark, because that is the order in which they'd be defended.

1. **An interrupted first scan currently loses everything.** Quit mid-scan and the next launch truncates and starts
   over. Coverage walks are add-only, so a phased index survives quits, crashes, and mount hiccups. Independent of
   ordering, and the one reason that holds even if phased coverage turns out slower.
2. **Photo search and folder importance wait for `/` when they only need `$HOME`.** Both start off `Freshness::Fresh`,
   which today means the whole drive. On a first run that is minutes of the most visibly valuable feature sitting idle
   while `/System` is walked. `home_covered_at` is the smallest possible fix and the largest user-visible one.
3. **We index the wrong things first.** `/System`, `/Library`, and Xcode caches get walked with the same priority as
   `~/Downloads`. The user is looking at their own files.
4. **The cost lands at the worst moment.** First launch is also onboarding, the AI model download, and first
   impressions. Spending the machine on ground nobody asked for _first_ violates "respect the user's resources".
5. **The wow moment arrives late.** Real folder sizes take minutes, spread evenly over a drive the user mostly doesn't
   care about. Sized honestly, this is the weakest of the five: the folders it is about total 4,735 entries (under a
   second), and mid-scan partial aggregation already punches the folders a pane is showing through the depth cap every 5
   s (`lifecycle/partial_agg.rs`). What ordering adds is that the punch has something to find, since a folder nobody has
   walked yet aggregates to nothing.

**Why not also narrow the default scope to home**, which an earlier draft proposed: measured on David's machine
(2026-08-14, boot volume, 5,191,189 entries, 768 MB index, full walk 193 s), everything outside `$HOME` is 800,441
entries — **15.4%**, about 30 seconds and 115 MB. Meanwhile `~/Library` alone is 1,437,538 entries (27.7%), so a
home-only default would skip the small pile and keep the biggest pile of machine-generated files on the disk. And the
folders the wow moment is actually about (Desktop, Documents, Downloads, Pictures, Movies, Music) total **4,735
entries** — under a second, free under any design. The ordering change buys the product win; narrowing the extent bought
15% of a background job in exchange for a permanently partial index that every completion, freshness, rescan, sweep, and
upgrade path in the crate would have had to learn about. Full numbers, method, and the conditions that would change the
answer: `docs/notes/index-scope-measurement-2026-08-14.md`.

## What the user gets

1. First launch, before any decision: both panes on `~`, nothing indexed, nothing read that would trip a TCC popup.
   (Ships today.)
2. The moment Full Disk Access is granted, the phased indexer starts on the user's own folders. By the time onboarding
   is done, the folders they'll actually open have real sizes. If they denied FDA, nothing in the background ever raises
   a permission dialog.
3. As they browse, whatever they open gets indexed next, ahead of everything still queued.
4. Home finishes: photo search and folder importance start here rather than at the end of the drive.
5. The rest of the drive fills in behind that, and the volume goes `Fresh` when it is genuinely complete, as today.
6. At every moment the sizes shown are honest: exact where covered, `<dir>` or `≥` where not, an hourglass on ground
   being walked right now.
7. Quit at any point and nothing is lost; the next launch picks up where it stopped.

## The architecture

**Every walk is a coverage walk. There is no first full scan.** Today the first index of a volume is `ScanRoot::Volume`:
truncate, then bulk-build with the parallel guarded walker. Fast, but all-or-nothing, and `local_rescan_reconciles`
(`lifecycle/manager.rs:193`) deliberately routes a populated-but-never-completed index to _truncate and rebuild_
(reconciling a 4%-complete partial once made the app look hung for ~15 minutes). So "cover the important folders first,
then run the normal full scan" would throw away everything the priority phases built.

Instead the whole drive becomes the **last phase of the same mechanism** the priority folders use:
`Index::coverage(volume_id, scope, Listing)` names the frontier, `Index::cover(volume_id, frontier, Listing, cancel)`
walks it — add-only, durable, resumable, cancellable, through the volume's normal writer, with
`ComputeSubtreeAggregates` giving covered folders honest recursive sizes (its handler repairs the ancestor chain upward,
`writer/repair.rs:47`, `:98-135` → `store/dir_stats.rs:217-246`, so a covered subtree's size reaches `~` and `/`
correctly).

**Why this end state, not just the cheap one:** it collapses three mechanisms into one. Search-driven walks, priority
walks, and "index the whole drive" stop being separate things with separate failure modes. It makes the later want
("watch only specific folders, especially on Linux where inotify watches are scarce") the _default_ shape rather than a
retrofit. And it makes indexing interruptible without loss, which is what lets us spend the user's CPU politely.

**The transient state is the whole design problem.** A phased volume is partially covered from the first walk until the
`/` phase finishes. That window is minutes, not forever, but every path that can fire inside it has to behave: launch,
resume, rescan, the shallow sweep, the verifier, the enable button, the master switch. Most of this plan is that window.

### The stitch: what makes phases compose at all

**This is the piece without which the whole model silently degrades, and it is not obvious.**

A cover walk marks only the directories it _reads_. Bootstrap deliberately creates the ancestor chain at
`listed_epoch = 0` and claims nothing (`lifecycle/cover/bootstrap.rs:10-13`), and the coverage descent cuts at the first
`listed_epoch == 0` directory without descending past it (`read/coverage.rs:195-207`). Two consequences:

1. After phase 1 covers `~/Downloads`, `coverage(root, "$HOME")` still answers `["$HOME"]` and `coverage(root, "/")`
   still answers `["/"]`. **The frontier for an ancestor scope never shrinks**, so "skip a root that's already covered"
   and "the later phase resumes with less to do" are both false for exactly the phases that need them.
2. Worse, `cover` over such a root hits `ScanRoot::Virgin`'s refusal — `count_children_capped(root_id) > 0` ⇒
   `ScanError::NotVirgin` (`scanner/mod.rs:776-781`) — and routes to `repair_non_virgin` → `reconcile_subtree`
   (`lifecycle/cover.rs:493`), the **serial** per-directory walk. That is the exact path documented as making the app
   look hung for ~15 minutes over a real `/`.

So each phase is preceded by a **shallow stitch**: for every ancestor of the phase root, from the volume root down, read
that one directory, upsert its children, and `MarkDirsListed` for that directory alone. No descent, no deletion. It is
honest (we really did list those directories) and cheap (a handful of `readdir`s). After the stitch, the coverage
descent walks _through_ the ancestors and cuts at each genuinely unlisted child, so:

- a covered subtree is skipped, correctly;
- every frontier root handed to `cover` is genuinely virgin, so the **parallel** guarded walker takes it;
- the big phases become **many small walks instead of one huge one**, which is what makes priority interleaving cheap.

Confirmed against the descent rule and the epoch rollup: a stitched ancestor reads `Listed` (not `Covered`) while any
child is unlisted, because `repair_dir_stats_upward` recomputes `min_subtree_epoch` as the min over the directory's own
`listed_epoch` and its children's. No coverage is claimed that wasn't earned, and sizes stay honest.

**Five things the stitch must get right:**

1. **Upsert FILES, not just directories.** `listed_children_on` returns `None` while `listed_epoch == 0` and the **full
   child list** the moment it is non-zero (`read/queries.rs:377-386`), and `Index::list_children` feeds the MCP/agent
   `list_dir` tool. So marking a directory listed makes its rows authoritative to a user-visible consumer that same
   instant: a directories-only stitch would report a folder as containing no files. Pin it with a test.
2. **Resolve ids, read the current epoch, and flush before marking.** `MarkDirsListed { ids, epoch }` is a PK-keyed
   `UPDATE`, and marking a row that is still pending in an unflushed batch leaves it `listed_epoch = 0` **forever**
   (`writer/mod.rs:321-330`). The stitch creates the deeper ancestor rows itself, so a flush between the upserts and the
   mark is mandatory, not an optimization. Read `current_epoch` (`IndexStore::read_current_epoch`), the same value a
   walk stamps; ❌ never bump it.
3. **Reuse the diff core, not the whole primitive.** `verify_and_correct` is the right shape (depth-1 readdir diff,
   upserts, stale-row deletes) but must not be used as-is: it recurses into every new subdirectory with
   `scanner::scan_subtree` (`reconcile/verifier.rs:406-419`), which would destroy the point, since new children must
   stay virgin for the parallel walker. `reconcile_subtree` is fully recursive with no depth parameter, so it can't be
   "used at depth 1" either — but it is the model for the mark (it accumulates `listed_dir_ids` and sends one
   `MarkDirsListed` after the walk, `reconciler.rs:953-961`). Extract the depth-1 diff core, have both the verifier and
   the stitch call it, and let the stitch add the mark. ❌ Not `scan_subtree` (`ScanRoot::Rebuild` deletes descendants
   first).
4. **Suppress the verifier while phases run** — next section. A data-safety requirement, not a performance one.
5. **Keep the stitched ancestors maintained.** `/` and `/Users` end up `listed_epoch > 0`, so `listed_children_on`
   serves their child lists as authoritative. Until the `/` phase finishes, no walk re-reads them, so a new top-level
   folder would be invisible in the meantime. Include the stitched chain in the rescan anchor set — a handful of depth-1
   `readdir`s, ❌ not a walk. After full coverage the normal whole-volume mechanisms own it again.

### The verifier has to be told that phases are running

**Without this, the stitch makes things worse rather than better, in two ways.**

Today `verify_and_correct` no-ops on uncovered ground because the directory has no row to resolve
(`reconcile/verifier.rs:171-186`). **After the stitch, every frontier root has a row.** So the first time the user lists
a stitched, virgin frontier root, the verifier resolves it, finds zero indexed children, treats every disk entry as new,
and then runs a full recursive `scan_subtree` **per new subdirectory**, serially, on the verifier task. That:

1. leaves the directory at `listed_epoch = 0` with children, which is exactly the `NotVirgin` node the stitch exists to
   prevent, so its later phase walk takes the serial repair — and it fires for **every folder the user opens ahead of
   the walker**, which is the user behavior this whole plan is designed around;
2. runs a second, unthrottled indexer against the same disk and writer as the phase machine;
3. **can write the same names as a live cover walk.** The verifier consults neither the cover claim nor
   `WatchScope::may_walk` (no references to either in `reconcile/verifier.rs` or `lifecycle/state/scan_control.rs`); it
   sees only the `scanning` flag. Two writers of the same names allocate different ids, and `INSERT OR IGNORE` drops one
   and orphans its subtree (`lifecycle/cover/live.rs:1-23`). This is latent today and routine under this plan, because
   walking-while-browsing IS the plan.

**The fix is durable, not a flag: make the verifier bail when the directory's `listed_epoch == 0`.** A stitched row
outlives any runtime flag — it survives quit and relaunch — so the hazard fires for every listing that happens while the
machine isn't walking: between launch and the first phase, after M5's `stop`, and while the master switch is off. The
one-line epoch gate restores exactly today's semantics on uncovered ground (today the verifier bails because there is no
row; post-stitch it bails because nothing has listed it), and it matches the design rule that the walk owns coverage
growth. The read it needs already exists (`IndexStore::get_listed_epoch_by_id`, used at `read/queries.rs:381`).

⚠️ **Scope the bail to a volume the phases still own.** A directory the cost budget SKIPPED also has a row with
`listed_epoch == 0` and no cause (`reconcile/CLAUDE.md`: "A skipped dir is one we NEVER listed: ❌ never diff it with an
empty listing, ❌ never stamp its `listed_epoch`"). On a phased volume the cover walk reaches it through the frontier,
since unlisted IS frontier. On a fully covered volume no phase is coming, so the per-navigation verifier is the only
thing that heals it. **Bail while the volume has an unfinished frontier; keep today's behavior once `scan_completed_at`
is stamped.**

**Decided: the bail, ❌ not the mark.** The alternative was to have the verifier MARK a directory listed when it
genuinely read all of it, turning it from a producer of `NotVirgin` nodes into a producer of legitimately covered ones.
That is more useful (browsed folders would stop needing a walk at all) and strictly more work to get right, and the two
are **mutually exclusive** — a verifier that both bails and marks is incoherent. Build the bail; the mark is M6.

**Keep `phase_active: Arc<AtomicBool>` on `IndexManager` as well** (crate-internal, no public-surface cost) for the
concurrency half of the problem. It means **a phase or visited-root walk is running right now**, ❌ not "the machine
exists". Fed to:

- the `scanning` argument of `verifier::maybe_verify`, via `trigger_verification` (`state/scan_control.rs:46`);
- **`start_scan`'s single-flight guard (`manager/start.rs:321`)** — the dangerous one. Today that guard is
  `mgr.scanning`, so with a separate flag `start_scan` would no longer refuse while phases run, and a `force_scan`
  through any surviving door would send `TruncateData` + `BumpCurrentEpoch` and start a parallel walker **while a cover
  walk holds a claim and is still writing**. `cover_context_for` only refuses NEW walks; nothing stops one already
  running.
- `awaits_its_first_scan` (`state/queries.rs:93`).

⚠️ **`phase_active` alone does NOT close the two-writer hazard, because a search-driven cover walk sets neither it nor
`mgr.scanning`.** A coalesced shallow anchor can `TruncateData` under a live search walk today; this plan makes it
routine. Gate `start_scan` on **`cover::ground_being_walked(volume_id, &[volume_root])` being non-empty as well as
`phase_active`** (it takes a frontier, and `overlaps` counts an ancestor, `cover/live.rs:126`, `:176-190`). That is a
pre-existing bug worth fixing on its own merits, and it makes the guard correct for both walk kinds.

⚠️ **`begin_branch_coverage` / `finish_branch_coverage` silently no-op unless the manager is `Running`.** Both go
through `with_running_manager` (`lifecycle/state.rs:275-294`), and both `force_scan` and `perform_registry_rescan`
`mem::replace` the phase with `ShuttingDown` for the whole duration of `start_scan` (`state/scan_control.rs:82-94`,
`manager.rs:252-274`). A cover walk that _ends_ inside that window never decrements `walks`, so its branch stays
`walks > 0` **forever**: `may_walk` is false for that ground permanently, every event for it buffers and is never
promoted, and it is never absorbed. Latent today because walks are rare; routine under this plan. Make finish idempotent
and independent of the registry phase, ❌ not "hope the window is short". Test it.

❌ **Do not reuse `mgr.scanning` for any of this.** `cover_context_for` returns `None` while `mgr.scanning` is true
(`lifecycle/state.rs:251-266`), so the phase machine's own `Index::cover` calls would fail with `ScanInProgress`. Leave
that gate exactly as it is.

Checked and clear: `get_writer_and_scanning_for`'s bool reaches only the MTP and SMB watch layers, which stay below the
`is_trait_scanned` early return, and `freshness_bridge.rs:113` ignores it.

### Interleaving without preemption

Because the stitch turns `$HOME` and `/` into a list of independent frontier roots, the machine walks them **one root at
a time and checks the priority queue between roots** — one `cover()` call per root. **This is the mechanism; there is no
second candidate.**

❌ Don't "save the per-root meta write" by handing one `cover()` call a phase's whole frontier. It looks cheaper (a
single `cover()` already checks its cancel token between frontier roots, `lifecycle/cover.rs:326-330`), but that check
is _inside_ `cover`, so the machine gets no point at which to consult the queue. If the benchmark shows
`finish_branch_coverage`'s write is material, batch roots into small groups — ❌ never into one.

Preemption (cancel the running walk, run the visited root, resume) is **out of scope**. The join rule still applies: the
machine starts a walk only after the previous `CoverWalk::finish()` returns, and treats a non-empty
`covered_by_another_walk()` as "this root did not run". The `Claim` is released by the walk thread on exit
(`lifecycle/cover/live.rs:104-117`), so cancel-then-immediately-start would make the new walk defer the same ground and
cover **nothing** while reporting `roots_covered: 0`.

**Two honest caveats on the "waits for one subtree, not for the drive" claim:**

- **Search walks are not ours to serialize.** Live search calls `Index::cover` on the user's behalf
  (`search/execute/live_run.rs`), deliberately carved out of both indexing switches, and only _overlapping_ ground is
  deferred by the `Claim`. Disjoint ground runs a second parallel walker against the same disk and writer. That is
  correct — a search somebody typed outranks background phasing — but the plan may not claim a single-walker invariant,
  and the benchmark's browsing arm must include a search.
- **A frontier root can be huge.** The wait is bounded by the largest child of the phase root, not by a small subtree:
  `~/Library` is 1.44M entries on David's machine, `~/projects-git` 1.58M. So "whatever they open gets indexed next" can
  mean "in 40 seconds", on exactly the machines where it matters most.

  **A deeper stitch was the proposed knob, and the benchmark refuted it.** The idea was that stitching one level deeper
  turns an oversized root into a list of smaller ones — `~/Library/Caches` (423k) as a frontier root instead of
  `~/Library` (1.44M) — cutting the worst-case wait by roughly 3×. Measured, it cut it from 14.0 s to 13.4 s
  (`docs/notes/phased-vs-bulk-index-2026-08-14.md`), because the worst case was never `~/Library`: it is
  `~/projects-git`, and 97% of that is a single child, so a level deeper hands the walker a barely smaller root. **Ship
  depth 1** (Decision 13). The wait is bounded by one user's one big folder, and no stitch depth short of splitting that
  folder changes it. Preemption stays out of scope, so the honest statement is that a visited root can wait tens of
  seconds behind a large sibling.

### What a full scan does that cover walks don't (and what we owe each one)

Audited end to end against `manager/start.rs::start_scan` + `lifecycle/scan_completion.rs`:

- **`scan_completed_at`** — the phase machine stamps it when the volume-root frontier is empty. Same meaning as today.
- **Scan calibration meta** (`scan_duration_ms`, `total_entries`, `total_physical_bytes`, per walk kind) — nothing else
  writes them, so the ETA tier would degrade permanently. The phase machine writes the equivalent from its own totals.
- **`ScanCalibration` capture and the live counters.** `scan_calibration` is set only in `start_scan`, and `get_status`
  derives its counters from it plus a live `ScanHandle`. Without it, `status()` reports `scanning: false` with zero
  counters for the entire first index, so the per-drive row, progress bar, and ETA are dead.
- **⚠️ `ScanProgressReporter`, and everything hanging off its 500 ms tick.** It is spawned only by a scan path
  (`lifecycle/manager.rs`, `network_scan.rs`) and its loop ends when the completion handler sets `scan_done`, so under
  phases it never runs. Three things stop together: the `index-scan-progress` event stream, **mid-scan partial
  aggregation** (`ComputePartialAggregates` with hot paths collected from `open_listings`, `lifecycle/partial_agg.rs`),
  and the only tick the `open_listings` seam permits a caller to use (`host/policy.rs:93-100`). Losing partial
  aggregation hurts precisely where this plan is weakest: a folder INSIDE a frontier root still being walked gets no
  size until that root finishes, and a frontier root can be 1.58M entries. **The phase machine spawns its own reporter**
  (phase-scoped `scan_done`, `AggSource::Sql`, since a cover walk leaves the accumulator maps empty), which also gives
  the visit poll its natural home and satisfies the seam's rate limit by construction.
- **Events**: `ScanStarted`, `ScanComplete`, `AggregationComplete`, `DirsUpdated["/"]`. The frontend's
  `resetAggregation()` handshake depends on their ordering (`scan_completion.rs:208-211`).
- **`writer.set_expected_total_entries`** — the writer's flushing-progress denominator.
- **The one-shot `dir_stats` ledger heal** (`ArmLedgerHealLatch`, armed only in `resume_or_scan`) and **the shallow-
  sweep ledger** (`reconciler::seed_from_meta` + `record_sweep_completed` + `SHALLOW_SWEEP_AT_KEY`). Skipping the second
  reproduces the bug `manager.rs:398-406` warns about: every launch hands the next shallow anchor a free full sweep.
- **`BackfillMissingDirStats`**, and a post-scan `WalCheckpoint` (the latter is a NIT — the 30 s maintenance timer also
  checkpoints).
- **Freshness ⇒ `Fresh`**, at full coverage, exactly as today.
- **`RootUnlistable` detection** is volume-root-scan only, so a cover walk over a vanished drive reports "covered
  nothing" instead of the typed abort that clears the stuck UI row. Handle it in the phase machine.

**⚠️ And one thing that is NOT already handled, which is fatal on its own: the database is never prepared for a walk.**
`prepare_database_for_a_walk` — the ROOT sentinel, epoch seeding, `volume_path` meta, and the `EXCLUSION_POLICY_KEY`
stamp (`state/walk_database.rs:99-121`) — runs **only** when `activation == Activation::WriterOnly`
(`state/startup.rs:165-167`), and this plan mandates `IndexTheVolume`. The only other writer of the stamp for a local
volume is `start_scan`'s non-reconcile branch (`manager/start.rs:429-430`), which the phase machine never calls.

An absent stamp makes `index_predates_exclusion_policy` answer **true** (`scanner/exclusions.rs:350-353`, "an absent
stamp answers yes"), and `walk_coverage` then short-circuits every query to `Frontier` over the whole scope
(`read/coverage.rs:314-317`). On a fresh phased install: the frontier never shrinks, nothing ever converges, and after
the first walk every root is non-virgin and takes the serial repair. It reproduces exactly the failure the stitch exists
to prevent, and it would look like the stitch not working. `volume_path` going unwritten also breaks offline external
reads.

**The phased start owes that work itself — but ❌ NOT by calling that function.** It opens its own write connection and
runs at `state/startup.rs:165`, before `IndexManager::new_for_kind` (`:199`), precisely because no writer thread exists
yet. The phased answer lives in `resume_or_scan` (`:218`), where the writer is live, so calling it there is a second
writer on the same DB. **Do the same work through writer messages** — the epoch (`BumpCurrentEpoch`, which seeds an
absent key; there is no `SeedCurrentEpoch` message today), `volume_path`, and `exclusion_policy_stamp_message()` — as
`start_scan` does (`manager/start.rs:421-431`), sequenced after any truncate.

⚠️ **Carry the stamp's precondition across with it, stated exactly.** `exclusion_policy_stamp_message()`'s own rule is
"❌ Send it ONLY right after a `TruncateData`" (`scanner/exclusions.rs:355-362`), and `prepare_database_for_a_walk`
enforces the legal equivalent, `entry_count <= 1` (`walk_database.rs:105-116`). **The condition is
`entry_count <= 1 || we-just-truncated`.** Read as "only after a truncate" it never stamps a fresh install, reinstating
the fatal bug above; read as "always" it silently blesses rows written under an older policy. Both misreadings are
silent.

**A STALE stamp needs a repair path too.** `evict_an_index_no_walk_can_trust` is `WriterOnly`-only, with the rationale
"❌ Not for an `IndexTheVolume` start: a full scan truncates and re-stamps by itself" (`state/walk_database.rs:28-30`).
Once a phased volume completes, that rationale holds again. But **during the phased window** a fingerprint change would
strand the index, so the phased start needs a "fingerprint changed ⇒ truncate once and re-stamp" arm, clearing
`scan_completed_at`, `home_covered_at`, and the persisted branch set with it.

**Pin it with a test that a fresh phased volume's frontier actually shrinks after one walk.** Without it, every other
test in M2 can pass while the product never converges.

One real behavioral difference between the walk kinds: `ScanRoot::Virgin` pins the walk root's **device** while
`ScanRoot::Volume` bounds by path prefix, so the `/` phase cuts at mounted filesystems rather than at `/Volumes/`. A
device cut writes no row, so it can't leave a permanent frontier node, and firmlinked system paths share one device
(`/`, `/System/Volumes/Data`, `/Users`, `/Applications`, `/System` all report dev=16777231; verified on macOS 26.5.2
build 25F84 via `stat -f %d`, 2026-08-14). Acceptable, but the `/` phase indexes a slightly different set than today's
scan.

### Activation: keep `IndexTheVolume`

❌ **Do not launch the phased volume as `Activation::WriterOnly`.** `journaled` is computed as
`activation == IndexTheVolume && kind.has_event_journal()` (`state/startup.rs:135`), and a `WriterOnly` start never
calls `resume_or_scan`. That would cost, on every launch: no FSEvents journal replay for the boot disk, and (once
`scan_completed_at` exists) a `Stale` load that **bumps the epoch**, rendering every directory size stale forever. The
shallow-sweep seeding (`reconciler::seed_from_meta`) lives in `resume_or_scan` too. The phase machine belongs **inside
`resume_or_scan`'s decision**, as a third answer beside replay and scan.

Three consequences that are easy to miss:

1. **`resume_branch_watch` currently runs only for `Activation::WriterOnly`** (`state/startup.rs:251-253`). Under
   `IndexTheVolume` the persisted branch set is never reloaded, so a partially covered volume would come back with its
   covered ground **unwatched** and no epoch bump to admit it. `startup.rs` needs an explicit phased condition beside
   `WriterOnly`, or this plan's cross-session-resume claim is simply false. **And the ordering matters**:
   `resume_or_scan` runs at `startup.rs:218`, before the registry insert and before `resume_branch_watch` at `:251`. If
   the phase machine starts a walk from inside `resume_or_scan`, that walk's `begin_branch_coverage` starts the watcher
   first, and the later `ensure_branch_watch` returns early because a watcher is already running — so the
   `resuming = true` path never runs and **the epoch bump for an unreplayable gap never fires**, making last session's
   covered rows render as _current_ when nothing verified them.

   **The rule, precisely: the machine's first walk starts only after `resume_branch_watch` has run** (`startup.rs:252`),
   ❌ not merely after the registry insert (`:244`). Concretely: `resume_or_scan`'s phased answer only **registers
   intent**, and `start_indexing_for` starts the machine in its `(true, Ok(()))` arm, after `resume_branch_watch`. ❌
   Moving `branches::resumed_for` earlier is NOT an equivalent fix: it restores the branch set but not the bump.

2. **The `dir_stats` ledger heal is armed but never paid.** `ArmLedgerHealLatch` is disarmed by the next successful
   `ComputeAllAggregates`, and cover walks send only `ComputeSubtreeAggregates` — so the latch stays armed and re-arms
   every launch, and the heal never happens. Fix is one message: send `PayLedgerIfUnpaid` (`writer/mod.rs:415-421`,
   which runs a full `ComputeAllAggregates` iff armed and no-ops otherwise) at full coverage, alongside
   `scan_completed_at`.

3. **Placement inside `resume_or_scan` is constrained.** The phased answer sits _after_ the sweep seed and latch arm
   (`manager.rs:409-430`) and _after_ the `should_replay_journal` branch (`:435-467`), replacing only the final
   `start_scan` fallthrough (`:474-497`). ❌ It must stay below the `is_trait_scanned` early return (`:388`), or SMB and
   MTP volumes get routed into a local phase machine.

   ⚠️ **And it must ❌ NOT swallow the whole fallthrough.** `has_event_journal()` is `Local`-only (`volume.rs:88-90`),
   so a `LocalExternal` drive — and a Linux boot disk, which has no `supports_event_replay` — **always** lands here, and
   today a completed one reconciles in place at every mount. Replace the fallthrough wholesale and a completed external
   volume would be treated as a fresh phased one. **Route `scan_completed_at.is_some()` to today's reconcile arm
   first**, and let the phased answer take only what is left: an index with no completion marker.

### Watching: no handover needed

On macOS `DriveWatcher::start_branches` already watches the **volume root** and filters by `WatchScope::Branches`
(`watch/watcher.rs:204-211`) — exactly the "watch `/`, keep only what we care about" model. On Linux it watches each
branch, deliberately: `notify`'s recursive mode costs one inotify watch per directory against `max_user_watches`.

So a fully covered volume simply keeps `WatchScope::Branches` with `/` as its single branch, and the branch→whole-volume
handover never has to be written. On Linux a `/` branch is watched recursively, the same cost as whole-volume watching.
**Prefer this**, with one required change:

- **Teach `is_branch_confined()` to ask the real question.** A `Branches` scope never takes the visible-scanner route
  for a `MustScanSubDirs` anchor, whatever its depth (`reconcile/reconciler.rs:369-374`, `:517-527`). Keeping `Branches`
  forever would therefore mean a fully covered boot disk **never sweeps again**: every coalesced root-scale anchor goes
  to the throttled `reconcile_subtree` drain on a shallow anchor, which is exactly the "holds the per-dir hourglass for
  the better part of a full scan" case the depth split exists to avoid, and the sweep-window bookkeeping accumulates
  with no sweep. One line fixes it: `is_branch_confined()` is false when the branch set covers the volume root —
  precisely `WatchScope::branches().covers(volume_root)` (`branches.rs:481-485`; `contains` matches `path == self.path`,
  so a `/` branch satisfies it, and the reconciler already holds `self.space` for the root string). That restores the
  shallow sweep at exactly the moment the volume genuinely answers for everything. **It does not reopen the truncate
  door, but only because `scan_completed_at` is stamped by then** (which flips `local_rescan_reconciles` to true, so a
  shallow anchor reconciles in place instead of truncating) — which is why the completion ORDER below is load-bearing.
- **While the volume IS branch-confined**, truncate door (d) in M3 is closed by construction, and so is a door the
  stitch would otherwise open — the stitch creates depth-1 and depth-2 branches, and `SHALLOW_RESCAN_MAX_DEPTH = 2`
  would send those to `perform_registry_rescan` → a truncating `start_scan` if the scope were `WholeVolume`.
- Verified for the single-`/`-branch end state: `may_walk` (`covers("/")` is true), `admit` (a `/` branch is
  `deepest_containing` for every path ⇒ `Process`), and the re-anchoring arm is unreachable with nothing above `/`.

If a handover is written anyway, its traps are: start from `replayable_event_id()` / `BranchWatch::safe_event_id()`,
never 0, or the gap is lost; and `WatchScope` is captured by value into the reconciler and `LiveConfig`, so swapping it
means re-spawning the loop — drain `take_promoted()` first or buffered events die with the old loop.

**Branch watching stays on when drive indexing is off, on macOS only.** Today `branch_watch_allowed` ANDs the master
switch, so walked ground stops being kept current and search can serve rows that are wrong. On macOS the FSEvents stream
is volume-rooted and the filtering is free, so a covered folder stays watched whatever the setting says; stale search
results are a worse failure than a watcher that costs nothing. On Linux the refusal stays: each branch is real inotify
watches against `max_user_watches`, and a user who turned indexing off has asked us not to spend that.

**Branches absorb their descendants.** Adding a branch that is an ancestor of existing ones must retire them: watching
`~/A` should stop `~/A/B` and `~/A/C` being tracked separately. Today `finish_covering` only removes the path being
finished when an ancestor already exists, so siblings accumulate and nothing ever collapses downward. Make absorption a
property of the set itself (on insert, drop every strict descendant; leave any entry with `walks > 0` alone until it
finishes). Every collapse below is then this one rule firing.

**The branch set needs an explicit collapse.** `begin_covering` pushes one `Branch` per path, so N frontier roots means
N branches. Every event then pays an O(branches) scan in `deepest_containing` on the live hot path. Expect roughly
50–150 entries during the phases (children of `/`, of `$HOME`, and the priority roots). **Collapse at the end of each
phase to that phase's root**, which absorption gives for free, and at full coverage the set is `["/"]`. ❌ **Not via
`branches::clear` plus a begin/finish pair**: `clear` calls `forget`, which drops the map entry, while the live loop and
its reconciler each hold their own `Arc<BranchWatch>` captured at `ensure_branch_watch`; `live_for` would then mint a
**brand-new** `BranchWatch` that nothing is reading. The persisted meta would say `["/"]` while the running loop kept
filtering against the stale N-entry set for the rest of the session — and `is_branch_confined` would read that same
stale Arc and stay true, leaving the shallow sweep disabled until the next launch. (The existing `clear` call in
`start_scan` is safe only because the loop is torn down and replaced in the same breath.) Instead add a crate-internal
`collapse_to(root)` that mutates the **shared** `BranchWatch` in place, leaves any `walks > 0` entry alone, then
`persist()`. Measure `deepest_containing` under a churn burst to confirm the mid-phase set isn't itself a problem.

⚠️ **Decouple persisting the branch set from `branch_watched`.** `finish_branch_coverage` uses `AfterWalk::Forget`
(dropping the entry, persisting nothing) whenever `mgr.branch_watched` is false (`manager/start.rs:275-283`), so a
`DriveWatcher::start_branches` failure (non-fatal, logged, `:206-211`) silently loses the record of what we covered —
which M3.3 uses to tell a phased partial from a legacy one. Treat a failed watcher as "covered but unwatched"; the epoch
bump on the next resume already makes that honest.

### Freshness stays as it is, plus one early signal

Folder importance and the whole media index (OCR, Vision tags, CLIP embeddings — that is photo search) start their
passes off `Freshness::Fresh` plus a `ScanCompleted` publish on the lifecycle bus
(`state/queries.rs::ready_volumes_with_kind` filters on `Fresh`; the bus publish fires only on
`FreshnessEvent::ScanCompleted`, `state/freshness_bridge.rs:95-98`).

**Freshness keeps today's exact meaning: `Fresh` when the volume is fully covered.** No new scope concept, no second
completion definition, no change to the badge, the stale dialog, or `Index::is_fresh`. A phased volume is un-`Fresh`
while it is partial, which is true and is what today's first scan does anyway.

**The one addition: a `home_covered_at` marker and a matching lifecycle-bus signal**, whose ONLY job is to let photo
search and importance start when `$HOME` is covered instead of waiting for `/`.

- Stamp `home_covered_at` when the `$HOME` frontier empties. It drives **nothing else** — not freshness, not the badge,
  not rescan routing, not the sweep, not `scan_completed_at`. Keeping its blast radius to one subscriber is the whole
  reason this is cheap.
- Publish a distinct bus signal carrying the volume id, and have the media and importance schedulers subscribe to it
  alongside `ScanCompleted`.
- ⚠️ **The startup sweep needs the same admission.** `ready_volumes_with_kind` gates on `== Freshness::Fresh` and only
  WIRES subscriptions (`state/queries.rs:29-43`; its doc comment says each scheduler pairs it with an explicit startup
  enqueue). So a relaunch mid-coverage would wire nothing for a home-covered-but-not-`Fresh` volume. Admit a volume
  whose `home_covered_at` is stamped, or the early kick works on the first run and never again.
- `enqueue_initial_full_pass_if_unscored` only scores a volume whose importance store has no generation yet, so the
  early pass stamps a generation and a later launch mid-coverage re-scores nothing until `ScanCompleted` fires at full
  coverage. That window is small here (home completes early, `/` minutes later) and the incremental `record_visit` /
  `publish_dirs_changed` paths soften it. Say it rather than discover it.

Audited and safe: **search never reads freshness at all** (it goes through `coverage()` / `cover()`, so it is
coverage-gated by construction); `Index::is_fresh` has exactly one app caller
(`file_system/write_operations/journal_search.rs:102`), which applies its own coverage gate
(`min_subtree_epoch > 0 && == current_epoch`) and downgrades to `index_stale` otherwise — pin that with a test, since a
partially covered volume now exists for minutes at a time.

## What already exists (do not rebuild it)

Confirmed by reading the code, 2026-08-13/14:

- `Index::cover` / `Index::coverage` / `Index::coverage_token`. Reference caller: live search
  (`apps/desktop/src-tauri/src/search/execute/live_run.rs:167`). Note `CoverageMap.frontier` is explicitly **unordered**
  (`read/coverage.rs:134-137`), so walk order can't be read out of a coverage answer.
- `IndexManager::begin_branch_coverage` / `finish_branch_coverage` / `ensure_branch_watch`
  (`lifecycle/manager/start.rs`): register ground before a walk touches it (so live events buffer instead of racing),
  then watch what was covered. `ensure_branch_watch` is conditional: local-scanner kind, no watcher running, non-empty
  branches, and `master::branch_watch_allowed`.
- Cross-session resume: `state/startup.rs::resume_branch_watch` reloads the persisted branch set and replays from the
  volume's last event id, bumping the epoch when it can't, so rows render stale rather than lying.
- `Index::verify_directory(volume_id, path)`, called on every non-archive listing
  (`file_system/listing/operations.rs:108`, `streaming.rs:533`).
- `commands::importance::record_visit` (`commands/importance.rs:40`), the real per-navigation signal.
- Honest sizes: `min_subtree_epoch` absorbs zero upward, so partial coverage renders `<dir>` / `≥` rather than a
  confident wrong number. Partial coverage is already a first-class, honest state.
- The user's favorites: `favorites/store.rs`, our own `favorites.json`, seeded once — **`/Applications`, `~/Desktop`,
  `~/Documents`, `~/Downloads` on macOS; Home, `~/Desktop`, `~/Documents`, `~/Downloads` on Linux**. Not Finder's
  sidebar (explicitly out of scope).
- Unindexed search already walks what you search and writes what it finds, so search converges toward instant through
  use. Nothing here should duplicate that.

## Where the app's answers enter the crate

Two things the index needs are **the app's to answer**: which folders matter to this user, and where the user is looking
right now. `indexing/host/` is the established home for exactly that — "add a seam here, never a new
`crate::<app module>` import", and "vocabulary moves down; questions become seams" (`host/CLAUDE.md`). So neither
arrives as an argument bolted onto a launch call:

1. **Priority roots** are a method on the existing `HostPolicy` trait (`host/policy.rs`), beside the other "what has the
   user's attention" question. Asked when the machine needs them, so an edited favorites list or a new session's tabs
   are picked up without a restart, instead of being frozen at launch.
2. **Where the user is right now needs no new door at all.** `HostPolicy::open_listings()` already reports every
   directory a pane is showing (it exists so mid-scan aggregation can punch the visible folders through the depth cap).
   The phase machine polls it and keeps a small recently-seen set, so a folder the user opened and left still gets
   queued. ❌ **Not `Index::verify_directory`**, which is too loose a signal (it fires for the opposite pane, MCP
   listings, and refreshes).

   ⚠️ **Rate-limit the poll to ≥500 ms, independent of root boundaries.** The seam's contract is explicit: it allocates
   and "it's asked on the scan-progress reporter's 500 ms tick. ❌ Not from anything faster" (`host/policy.rs:93-100`).
   "Between frontier roots" is not automatically within that — the stitch deliberately produces 50–150 roots and many
   finish in milliseconds. Poll on a timer, consult the cached answer at root boundaries.

`Index::start_root_at_launch` keeps its exact signature, and `verify_directory` keeps its exact meaning. **There is no
new setting**, so `IndexConfig` is untouched. The only handle-level change in the whole plan is behavioral, inside the
crate.

**The ceilings this respects:** `scripts/check/checks/index-crate-isolation.go` caps `cmdr-index` at exactly what it
exposes — measured 2026-08-14 via `pnpm check index-isolation -v`:
`50 root promises, 40 handle methods, 17 public modules, 156 items` against ceilings of `50 / 40 / 17 / 156`, zero
headroom in all four buckets, and a raise needs David's explicit say-so. `countModuleItems` matches column-0
`pub struct/enum/fn/const/type` and `pub use` leaves (`index-crate-isolation.go:506-539`), so **struct fields, trait
methods, and enum variants are free** — which is why the shape above costs nothing. ❌ A new payload TYPE on an event or
a new `pub fn` on a public type would breach immediately. New `IndexEvent` variants need doc comments
(`#![deny(missing_docs)]`) and a regenerated `bindings.ts`; `UnreadableCause` isn't re-exported from `lib.rs` and
doesn't cross the bindings, so `Abandoned` needs the doc comment only.

## Milestone map

This ships as **one effort on one worktree**, so the milestones are an execution ORDER, ❌ not shippable slices. Land
them in sequence and keep the tree green at each boundary.

**Already shipped, separately:** the first-run startup state (dotfiles hidden by default, and a fresh install with FDA
opening left `~` / right `~/Downloads` exactly once, never over a layout somebody already has). The rule and its
guardrails live in `apps/desktop/src/lib/file-explorer/pane/first-run-layout.ts`; the persistence trap it depends on is
in `docs/architecture-patterns.md` § Persistence. One piece of its test list was deliberately skipped and is still worth
writing: **a Playwright E2E over a first run with `CMDR_MOCK_FDA`**.

- **M0** — ✅ **shipped**: the pre-existing bugs this plan would otherwise make routine.
- **M1** — ✅ **shipped**: priority-root computation plus the host seam. `HostPolicy::priority_roots(volume_id)` is
  live and has no consumer yet; the phase machine is it.
- **M2** — ✅ **built**: the stitch plus the phase machine. Two things the plan didn't have: a phase whose frontier is
  already empty needs an explicit final stock-take (a run that only CONFIRMS a previous session's coverage would never
  stamp), and the per-root writer flush lives inside `cover.rs` rather than the machine, so batching the drain needed a
  knob on `CoverContext` plus a safety flush when the released ground buffered live events.
- **M3** — launch, resume, and every path that would truncate.
- **M4** — events, status, and the hourglass UI.
- **M5** — surfaces, copy, kill switch.
- **M6** — follow-ups.

M0 and M1 touch nothing M2 depends on. Everything after M2 is strictly sequential. **M4's unit tests stand alone, but
its end-to-end assertion can't run until M3 lands**, because the surfaces it fixes only misbehave once the phase machine
is real.

---

## M0 — The pre-existing bugs, first and on their own

**Intent:** four defects that exist on `main` today, are latent only because cover walks are rare, and become routine
the moment phases ship. Each is correct on its own merits and none needs the phase machine, so they land first: the
risky milestone gets smaller, and a beta build gets safer immediately even if everything after this is re-decided.

1. **A truncating rescan can fire under a live search cover walk.** `start_scan`'s single-flight guard reads
   `mgr.scanning`, which a search-driven `Index::cover` never sets, so a coalesced shallow anchor can send
   `TruncateData` + `BumpCurrentEpoch` while a walk holds a claim and is still writing. Gate `start_scan` on
   `cover::ground_being_walked(volume_id, &[volume_root])` being non-empty as well (`cover/live.rs:126`, `:176-190`;
   `overlaps` counts an ancestor). Test: **`a_truncating_rescan_refuses_while_a_search_cover_walk_is_live`**.
   **Test-first** — a truncate under a live walk is the worst failure in this whole plan's blast radius.
2. **A walk that finishes while the manager is shutting down leaks its branch forever.** `begin_branch_coverage` /
   `finish_branch_coverage` both go through `with_running_manager` (`lifecycle/state.rs:275-294`), and `force_scan` /
   `perform_registry_rescan` `mem::replace` the phase with `ShuttingDown` for the whole of `start_scan`. A walk ending
   inside that window never decrements `walks`, so `may_walk` stays false for that ground permanently, every event for
   it buffers and is never promoted, and it is never absorbed. Make finish idempotent and independent of the registry
   phase. Test: **`a_walk_that_finishes_while_the_manager_is_shutting_down_still_releases_its_branch`**.
3. **A failed watcher silently erases the record of what we covered.** `finish_branch_coverage` uses `AfterWalk::Forget`
   whenever `mgr.branch_watched` is false (`manager/start.rs:275-283`), so a non-fatal `DriveWatcher::start_branches`
   failure drops the persisted branch set. Decouple persisting from `branch_watched`: treat a failed watcher as "covered
   but unwatched", which the epoch bump on the next resume already makes honest. This is also what M3.3's discriminator
   depends on, so it is load-bearing later as well.
4. **Branches never collapse downward.** `finish_covering` only removes the path being finished when an ancestor already
   exists, so siblings accumulate and every event pays an O(branches) `deepest_containing` scan on the live hot path.
   Make absorption a property of the set itself: on insert, drop every strict descendant, leaving any entry with
   `walks > 0` alone until it finishes. Add the crate-internal `collapse_to(root)` that mutates the **shared**
   `BranchWatch` in place and then `persist()`s (❌ never `branches::clear` plus a begin/finish pair, which mints a new
   `BranchWatch` the running live loop isn't reading). Test both, including
   **`the_branch_collapse_is_visible_to_the_running_live_loop`**.

**Not in M0:** the verifier's `listed_epoch == 0` bail. It is a no-op until the stitch gives frontier roots a row, and
its scoping ("bail while the frontier is unfinished, keep today's behavior once `scan_completed_at` is stamped") is only
testable against a phased volume, so it ships with the stitch in M2 as the plan says.

**Docs:** the guardrail lines these four earn in `lifecycle/CLAUDE.md`, with the why in `lifecycle/DETAILS.md`.

---

## M1 — Which folders matter to this user

**Intent:** guess the user's important folders from signals we already have, cheaply, with no new permissions and no
network. Ordered best-signal-first, because the order _is_ the schedule.

A new module **inside the existing `apps/desktop/src-tauri/src/priority/`** (which already holds `AppHostPolicy` in
`host_policy.rs`, beside `foreground.rs` and `transfers.rs`, with its own `C+D.md` pair), exposing one function: the
ordered, deduplicated, existence-checked roots. ❌ Not a new sibling module: "which folders have the user's attention"
is the question `priority/` already exists to answer, and a sibling would split it across two doc pairs. It is called
from `AppHostPolicy::priority_roots`, so the answer is recomputed when asked rather than frozen at launch. Keep it
cheap: the seam is asked at phase boundaries, but the trait's contract is "don't do I/O, don't take a contended lock"
for its other method, so cache the answer behind a short TTL rather than stat-ing a dozen paths per call.

1. **Last session's tab paths**, most recently active first, from `app-status.json`. Empty on a true first run. The
   strongest signal there is: it is literally where the user was.
2. **Cmdr favorites** (`favorites::store::list()`), in the user's order. Platform-dependent seed (macOS vs Linux
   differ), so ❌ don't hardcode the macOS four.
3. **Standard home folders that exist and are non-empty:** `Downloads`, `Documents`, `Desktop`, `Pictures`, `Movies`,
   `Music`.
4. **Cloud roots that exist:** children of `~/Library/CloudStorage/`, `~/Dropbox`,
   `~/Library/Mobile Documents/com~apple~CloudDocs`. After the local ones deliberately: File Provider reads can stall,
   and though the guarded walker survives that, a stall should not delay `~/Downloads`.
5. **`$HOME` itself.**

Then the volume root, as the final phase.

**These set the walk ORDER and nothing else.** There is no scope setting and no promise attached to them, so an edited
favorites list changes what gets indexed first and never what gets indexed.

**`~/Library` is in scope but never a priority root.** It is inside home, so the home phase includes it, and search over
it is occasionally what a user wants. It is also both huge (1.44M entries on David's machine, 27.7% of the whole index:
Caches 423k, Mail 395k, Application Support 210k, CloudStorage 162k) and where the pathological churn lives (the
1.14M-empty-file Google Drive temp directory in `docs/specs/later/sealed-subtrees-plan.md`), so it must never be one of
the roots we walk first. `sealed-subtrees` remains the real fix for that case.

⚠️ **`~/Library`'s size is what `home_covered_at` waits on**, so it delays the early media kick. Measure its share of
home-coverage wall clock in the M2 benchmark; if it dominates, stamp `home_covered_at` when the home frontier minus
`~/Library` is empty. Note `Index::coverage` takes one scope path and descends through everything under it
(`handle/mod.rs:456-465`), so that means "cover the M1 home roots individually", ❌ not a subtraction, which isn't
expressible.

Rules: dedupe; drop any root that is a descendant of an earlier one; cap the list (24 is a reasonable start) so a user
with 200 favorites doesn't turn phase 1 into a drive walk; and existence-check **without tripping TCC while the gate is
pending** by reusing `restricted_paths::tcc_paths::is_potentially_tcc_restricted` (even `Path::exists()` trips a popup;
`volumes::get_favorites` already has this rule — ❌ don't hand-roll a second one).

**Tests:** pure-function unit tests over a synthetic home (ordering, dedupe, descendant-drop, cap, missing paths, empty
first run, both platform seeds). **Test-first**: pure logic, many branches.

**Docs:** extend `priority/CLAUDE.md` + `priority/DETAILS.md` (no new pair, since the module lands inside an existing
documented directory), plus a line in `docs/architecture.md`.

---

## M2 — The stitch and the phase machine

**Intent:** walk the priority roots in order, let the user's navigation jump the queue, then home, then the drive, and
never lose a walk's work to a later one.

Lives in `crates/cmdr-index/src/indexing/lifecycle/` beside `cover.rs`; ❌ nothing below `lifecycle` may import
`lifecycle::state`.

### The gate: measure before committing

Measure on a real `/`: (a) today's truncate-and-bulk-build full scan, and (b) stitch + phased cover walks (M1 roots,
`$HOME`, then the `/` frontier).

**The benchmark must include the stitch**, or arm (b) measures the `NotVirgin` serial repair and looks catastrophic, or
measures a virgin `/` walk the product would never run. Venue: `crates/index-query` or an in-crate `#[cfg(test)]` bench
— ❌ not `crates/cmdr-index/benches/`, which compiles against the crate as EXTERNAL and can only reach the public
surface. Write the numbers to `docs/notes/phased-vs-bulk-index-<date>.md`, linked from the lifecycle `DETAILS.md`.

**Record time-to-value, not only time-to-full.** Full coverage is the cost side; the benefit is that `~/Downloads` is
usable in seconds. A benchmark reporting only wall clock to full coverage could pass at 1.4× having never measured the
thing the plan is for. Capture, per arm:

- **a coverage timestamp per priority root**, for `$HOME`, and for `~/Library` specifically (it gates the early media
  kick);
- **wall clock to full coverage, and peak RSS** (the one number the index database can't answer in advance);
- **a third arm: (b) under browsing**, driving `open_listings` through a handful of folders mid-run **and running a
  search**, since a search walk is a second walker we don't control.

**Gate: if (b) is more than roughly 1.5× (a) to full coverage, stop and re-decide with David** — with the
time-to-first-root numbers in hand, because they are what the decision is about.

**Measured, 2026-08-14: `docs/notes/phased-vs-bulk-index-2026-08-14.md`. The gate FIRES.** Baseline 39.1 s (confirmed by
running the real app, ❌ not the 193 s this plan quoted, which does not reproduce); (b) as written 4.70×. Two fixes take
it to 1.79×, and neither changes the design: record ground a walk could not read so no later phase re-offers it (4.70× →
2.10×), and batch the writer drain (→ 1.79×). Every priority root is covered in under 120 ms against 1.0–26.6 s, and
`home_covered_at` moves the wrong way, 39 s → 88 s.

**✅ The first of those fixes has LANDED** (2026-08-14), as a shipped-build bug fix rather than part of this plan: a
walk now records every directory it couldn't read as `UnreadableCause::Abandoned`, including the `readdir`-errno
producer this plan originally missed (`ETIMEDOUT` from a wedged mount, 1,497 directories on David's machine). Details in
item 7 below. So a re-measurement should start from ~2.10× rather than 4.70×, and the remaining gap is the writer drain.

**What "re-decide" means, decided in advance so the gate is decidable rather than a stop sign.** The honest comparison
isn't wall-clock parity: full coverage is background work already paced by the `clearance` seam, so 1.5× of 193 s is 290
s of politely-throttled walking against a permanent gain in interrupt-survival and a minutes-earlier photo search. The
two prepared answers, in order of preference:

- **Accept the slower full coverage** when time-to-first-root and time-to-`home_covered_at` both land where the plan
  wants them. Reasons 1 and 2 in "Why" don't depend on the ratio at all, and the extra minutes are invisible unless the
  user is watching the badge.
- **Stop and take reasons 1 and 2 only**, which is a real, much smaller product: the phase machine walking `$HOME` and
  the priority roots, with today's bulk build never running because completion still comes from the frontier. ❌ Not
  "priority walks first, then today's truncating `start_scan`": the truncate makes the sizes the user just watched
  appear vanish again, which is worse than never showing them.

❌ Neither answer is "keep going and hope". Write the numbers down, then pick.

**The gate runs on a throwaway harness, ❌ not on M2's deliverable**: the stitch, a hardcoded root list, and a loop. No
queue, no completion rule, no status plumbing, no `Abandoned` cause. Otherwise the milestone is gated on itself.

### The machine

1. **Activation stays `IndexTheVolume`**, and the phase machine is a third answer inside `resume_or_scan`, beside replay
   and scan, taking only the no-completion-marker case.
2. **The stitch runs before each phase**: list each ancestor of the phase root, mark that one directory listed, don't
   descend. Ship it together with the **`phase_active` flag and the verifier changes** — the stitch without them is a
   net regression, so they are one unit of work.
3. **The queue**: rank 0 the M1 roots, rank 1 roots the user visited while running, rank 2 `$HOME`, rank 3 the volume
   root. One walk at a time (`cover` is already internally parallel; a second concurrent walk of ours fights it for the
   disk and the writer). Between frontier roots, re-check the queue.
4. **Each phase step**: `coverage(volume_id, root, Listing)` for the frontier; empty ⇒ skip; otherwise walk its roots
   one at a time. The walk marks, aggregates, and claims its own ground.
5. **Visits enter through `HostPolicy::open_listings()`**, rate-limited per the seam's contract.
6. **One root, one `cover()` call, join before the next.** Preemption is out of scope. Measured: this costs nothing
   (41 s of real walking against a 38.1 s whole-volume walk), so ❌ don't revisit it for speed.

   ⚠️ **But the writer flush must NOT stay per-root.** A blocking flush at the end of every `cover()` call is 37.5 s of
   the walker standing still over ~1,500 roots, and it is the entire remaining gap once the abandoned-ground fix is in.
   **Batch the drain to roughly once per phase** (the plan's own "small groups, ❌ never one" rule applies to the DRAIN,
   while the `cover()` call stays per-root so the queue keeps its check points). Expect a larger writer backlog and say
   what that costs in memory. Two places still need a real flush and ❌ must not be batched away: the stitch's
   upsert-then-`MarkDirsListed` sequence, and the completion sequence's step 1 / step 7 ordering.
7. **Completion is derived, not remembered — but "empty frontier" alone is not a terminating rule.** `abandoned_ground`
   is per-walk and in-memory, so it can't answer "was anything abandoned in a previous session?"; the durable signal is
   that an abandoned directory is never marked listed, so it re-enters the frontier.

   **Two stamps, same rule, different root.** `home_covered_at` when the `$HOME` frontier is empty; `scan_completed_at`
   when the volume-root frontier is empty. Evaluate after every root finishes, ❌ not only at the end of a phase.

   **✅ The `Abandoned` cause and its retry LANDED as a standalone bug fix (2026-08-14), ahead of this milestone**, so
   the completion rule can be written against it as an existing mechanism. What shipped, canonically documented in
   `store/DETAILS.md` § "What coverage needs" and `writer/DETAILS.md` § "Retrying ground a walk gave up on":

   - `UnreadableCause::Abandoned = 3`, written by all three producers a local walk has — a `readdir` errno that isn't
     `EACCES` (the third one this plan originally missed, and 100% of what fires on David's machine), a watchdog
     timeout, and a give-up-pruned task through the new `DirVisitor::visit_pruned` hook.
   - `CoverageMap::abandoned`, a third list beside `permission_denied` and `declined`.
   - `ClearAbandonedIfDue`, fired by the per-volume maintenance timer, clearing only `Abandoned` rows on a persisted 5
     min → 1 h → 4 h → 24 h per-volume window that a mark arms and a retry finding nothing disarms.
   - Search folds the index's abandoned list into `SearchRunCoverage::abandoned_ground`, so a run over a wedged mount
     can't report itself exhaustive now that the frontier no longer offers that ground.

   So **completion can now be a pure function of the database** — "frontier empty, only unreadable causes left" —
   durable across relaunch, immune to churn, with no in-session bookkeeping. Without it a timed-out directory stayed
   `Frontier` forever and _everything_ hanging off completion never happened: the stamps, `PayLedgerIfUnpaid`, the sweep
   keys, the branch collapse, the media kick, freshness, `is_branch_confined` flipping.

   ⚠️ Two things still to know. `from_stored` maps unknown values to `Denied`, so a DOWNGRADED build would tell the user
   to grant Full Disk Access for a timed-out mount (acceptable for a disposable cache, worth not calling truthful). And
   the retry **does not enqueue a walk**: reopened ground is walked by the next search over that scope, or by a rescan.
   Making the clear drive a walk is this machine's job, ❌ not the maintenance timer's.

   **What this milestone still owes the heal**: the **user-visit trigger**, deliberately left out. It belongs on the
   machine's `open_listings` poll — ❌ never on `verify_directory`, since an abandoned directory has `listed_epoch == 0`
   and the verifier bails on it by design.

   Its absence is why the backoff opens at **5 minutes** rather than an hour: with no visit trigger, a ONE-OFF read
   failure puts a folder out of every search answer and NOTHING a user can do brings it back — not navigating in (the
   verifier bails), not re-running the search (the frontier no longer offers it). Five minutes bounds that. Once the
   visit trigger lands, that reason is gone and the first step can grow back toward the wedged-ground curve.

   ❌ **Don't use a "frontier didn't shrink across two passes" rule instead.** It has to compare sets rather than counts
   (a pass can legitimately grow the frontier by listing a root and exposing the abandoned directories inside it), it
   never terminates on a continuously-written drive, and being session-scoped it re-pays a full re-walk plus 15 s per
   wedged directory on every launch.

8. **On full coverage, in this ORDER — and the order is enforced by a FLUSH, not by the numbering.** Steps 1–6 are
   writer _messages_; step 7 is in-process state. The read the whole ordering protects (`local_rescan_reconciles`'s
   `get_index_status()` inside `start_scan`) goes through a read connection, so it sees the stamp only once the writer
   has committed it — and step 3 runs a full `ComputeAllAggregates` over a complete `/` index, minutes of writer-thread
   work sitting between the stamp being queued and being visible. **Flush after step 1 and before step 7**, or the
   collapse lands inside exactly the window the order exists to close. Use `writer.flush().await` from async (as
   `scan_completion.rs:228` does), or `tokio::task::block_in_place(|| writer.flush_blocking())` from a sync path in an
   async context (as `manager/start.rs:432` does). ❌ A bare `flush_blocking()` blocks a runtime worker.
   1. stamp `scan_completed_at`;
   2. write the calibration meta;
   3. `PayLedgerIfUnpaid` (nothing else ever pays the armed `dir_stats` ledger heal);
   4. `BackfillMissingDirStats`;
   5. `reconciler::record_sweep_completed` plus the `SHALLOW_SWEEP_AT_KEY` / `SHALLOW_COALESCED_KEY` writes — without
      these the in-memory `SweepRecord` stays `None` for the session (it is seeded from meta only at launch), so the
      very first shallow anchor after completion triggers a full sweep nobody asked for;
   6. publish freshness (`FreshnessEvent::ScanCompleted`) and fire the terminal events;
   7. **only then** collapse the branch set to `["/"]`. Collapse before the stamp and there is a window where the volume
      is neither branch-confined nor marked complete, and one shallow anchor in it truncates the finished index;
   8. clear the durable in-progress marker.

   **The sequence runs once, on the absent→present transition of `scan_completed_at`.** Re-running it on an
   already-complete volume would rewrite `SHALLOW_SWEEP_AT_KEY` and push the 24-hour sweep window forward every launch,
   the mirror of the bug `manager.rs:398-406` warns about.

   **`home_covered_at` has its own, much smaller sequence**: stamp it, publish the early signal, nothing else. Same
   once-only rule.

9. **Own a `ScanProgressReporter` for the phased run** (see the full-scan audit above) and **feed the live status
   shape** (`ScanCalibration`-equivalent counters) throughout, or the per-drive row, progress bar, ETA, and mid-scan
   partial aggregation stay dead for the whole first index. The reporter's lifetime is the machine's, ❌ not one walk's:
   it must survive the gaps between roots, or the tick dies 50–150 times per phase. Drive `get_status`'s `scanning`
   field from **"the machine has queued work"**, ❌ never by setting `mgr.scanning` (that would make the machine's own
   `cover()` calls fail) and ❌ never from `phase_active` directly: it goes false between roots, and the stitch yields
   50–150 roots many of which finish in milliseconds, so the search dialog's "building your index" state and the
   per-drive row would flicker at root cadence.

   **Decide the progress shape here rather than in M4.** A phased walk has no knowable total until the `/` phase, and
   the design principles forbid a progress bar parked at 100% and require a distinct state when the quantifiable part
   ends. So: **phase label + live entry counter + elapsed, and ❌ no percentage until the volume-root phase.** Say what
   `writer.set_expected_total_entries` gets meanwhile.

10. **Handle `RootUnlistable`** yourself: a cover walk over a vanished drive otherwise reports "covered nothing" instead
    of the typed abort that clears the stuck UI row.
11. **Master switch and per-drive veto** keep outranking everything.

**Tests** (integration, `crates/cmdr-index/src/indexing/tests/`, over the disk-image fixture and `InMemoryVolume`):

- **`a_fresh_phased_volume_s_frontier_shrinks_after_one_walk`** — the exclusion-policy stamp. Without it every other
  test here can pass while the product never converges. **Test-first.** Sibling:
  **`a_changed_exclusion_fingerprint_rebuilds_a_phased_index`**.
- **`frontier_excludes_covered_ground_after_a_stitch`** — and every frontier root it returns is virgin. This is the
  finding that broke the first draft; pin it hard. **Test-first.**
- **`the_verifier_leaves_an_unlisted_directory_alone`** — the data-safety story of the stitch. **Test-first.**
- **`the_verifier_still_heals_a_skipped_dir_on_a_completed_volume`** — the other side of the bail's scoping.
- **`a_stitched_directory_lists_its_files_not_only_its_subdirectories`**.
- **`a_listing_of_ground_a_walk_is_covering_writes_nothing`** (the claim / `may_walk` case). **Test-first.**
- **`start_scan_refuses_while_a_phase_is_active`** and
  **`a_truncating_rescan_refuses_while_a_search_cover_walk_is_live`** — a truncate under a live walk is the worst
  failure this plan can have. **Test-first**, both.
- **`a_walk_that_finishes_while_the_manager_is_shutting_down_still_releases_its_branch`**.
- **`the_branch_collapse_is_visible_to_the_running_live_loop`** (not just to the persisted meta).
- **`a_relaunch_with_no_replayable_journal_bumps_the_epoch`** — the resume-honesty property.
- **`completion_pays_the_ledger_and_seeds_the_sweep_keys`** and
  **`the_completion_sequence_runs_once_across_repeated_launches`**.
- **`a_permanently_timing_out_directory_still_lets_completion_happen`** and
  **`a_subtree_pruned_by_the_failure_budget_still_lets_completion_happen`** — the bounded-progress rule, whose failure
  mode is wide. **Test-first**, both.
- **`home_coverage_fires_the_early_media_signal_without_claiming_fresh`** and
  **`a_relaunch_mid_coverage_still_wires_the_media_subscriptions`** (the `ready_volumes_with_kind` admission).
- **`enabling_indexing_for_a_search_walked_drive_still_indexes_it`** — the shipped behavior `awaits_its_first_scan`
  protects.
- **`partial_aggregation_still_fires_between_frontier_roots`** — the reporter outliving individual walks, which is what
  keeps sizes appearing inside a root still being walked.
- `phases_run_in_order`, and a covered root is skipped without a walk.
- `a_visited_root_is_taken_between_frontier_roots` without cancelling anything.
- `rows_survive_a_stopped_and_restarted_machine` (row count only grows), and the restart joins before starting.
- `master_off_runs_nothing`.

**App-side, not in the crate**: `is_fresh` over partially covered ground still makes `journal_search` downgrade to
`index_stale`. `journal_search` lives in `apps/desktop/src-tauri/src/file_system/write_operations/`, which the crate
can't name; `enumerate_subtree_for_search` already has a `#[cfg(test)] test_hook` seam for exactly this.

**Docs:** `lifecycle/CLAUDE.md` (one must-know per new invariant, terse), `lifecycle/DETAILS.md` (the stitch and why,
the phase model, interleaving, completion, the early media signal), `indexing/DETAILS.md` (the data flow now that there
is no first full scan), the benchmark note.

---

## M3 — Launch, resume, and every path that truncates

**Intent:** a partially covered volume must come back as a partially covered volume, and nothing may quietly truncate
it.

1. **`start_root_at_launch(fda_pending)` is unchanged**; the roots arrive through the host seam. The app side is an
   `AppHostPolicy::priority_roots` implementation.
2. **`resume_or_scan` learns the phased answer** (M2.1), taking only the no-`scan_completed_at` case. The queue needs no
   persistence: it is recomputed from the M1 roots plus a coverage query per root, so a launch naturally skips what is
   done. Prefer that over persisted queue state, which can go stale or disagree with the database.
3. **Telling a phased partial from a legacy interrupted one.** Both have rows and no `scan_completed_at`, and the first
   must resume while the second is rebuilt. **The discriminator is the persisted branch set**: `start_scan` calls
   `branches::clear` before a whole-volume walk (`manager/start.rs:441`), so a legacy interrupted scan has none, while a
   phased (or search-walked) volume does. Non-empty ⇒ resume the phases; empty ⇒ today's rebuild, which under this plan
   means the phase machine builds it fresh. A search-walked volume resuming into phases is correct: it is a volume we
   are about to index anyway. This is why the branch set must persist independently of `branch_watched` (above).
4. **Close every truncate door.** A cover-built index has `entry_count > 1` and no `scan_completed_at`, so
   `local_rescan_reconciles` is false and `start_scan` sends `TruncateData`. Door (d) is closed by construction while
   the volume is branch-confined. The rest need work. Verify each with a test rather than trusting the reasoning:
   - **FDA Deny** ⇒ `start_indexing_after_fda_decision` → `start_volume(root)` → `awaits_its_first_scan` true ⇒
     `force_scan` ⇒ truncating full scan (`commands/indexing.rs:221`, `handle/mod.rs:177-187`). It only reaches
     `force_scan` when `state::is_active(volume_id)` already holds, i.e. a search walk stood a writer up first — which
     is precisely the volume with covered ground worth not truncating.
   - **Master switch off→on** ⇒ `drives_to_resume()` always includes root ⇒ `start_volume` ⇒ `state::start_indexing()` ⇒
     `resume_or_scan` ⇒ `start_scan("incomplete previous scan")`.
   - **"Rescan now"** during the phased window ⇒ **restart the phases**, ❌ never an error and ❌ never a truncate.
     After full coverage it keeps today's meaning exactly.
   - **A coalesced shallow `MustScanSubDirs`** ⇒ `perform_registry_rescan` → `start_scan`
     (`reconcile/reconciler/rescan.rs:122-126`).
   - **The per-drive "Turn on indexing for this drive" button**, because `awaits_its_first_scan` reads
     `scan_completed_at` and answers "never walked" on a phased volume. ❌ **Do not re-key that predicate** — it is
     shared, and it exists for two documented shapes (a search-driven walk that stood up a writer, and a first scan
     someone stopped, `state/queries.rs:79-86`), both of which have rows, so keying on row count would make the button a
     silent no-op on the very volumes it was written to serve. Fix it at the caller: `start_volume` starts the phase
     machine rather than a truncating `force_scan`. That covers the same ground without destroying anything.
5. **A wide journal gap.** Today it routes to `start_scan("stale index: journal gap too large")` (`manager.rs:448-462`).
   During the phased window that would truncate; the phased answer is to let the branch resume's conditional epoch bump
   (`manager/start.rs:183-191`) mark the covered ground stale and let the phases continue, since the frontier is still
   non-empty and the walk re-stamps as it goes. After full coverage, today's path applies unchanged.
6. **Existing installs.** A volume with `scan_completed_at` set replays or reconciles exactly as today and the phase
   machine never runs — nobody loses anything, and there is no settings backfill to get wrong. A volume without it takes
   item 3's discriminator.
7. **The kill switch lands HERE, not in M5.** This is a big behavioral change shipping into an open beta, and the thing
   worth being able to undo is launch routing, which changes in this milestone. A switch that arrives two milestones
   later protects nothing in between. One flag, read at startup, restoring the bulk-build path, so a bad week is a
   restart rather than a rollback. ❌ **An env var is not enough**: a beta user launching from the Dock never sees one.
   Use a `defaults write` key or a hidden setting, and say who is expected to flip it.

   ⚠️ **It needs its own row in item 3's routing table**, because the discriminator reads the persisted branch set and a
   killed build has no phase machine to resume into: a phased partial (rows, no `scan_completed_at`, non-empty branch
   set) must route to today's truncating rebuild when the switch is off. That is the right answer (self-healing, and the
   user asked for the old behavior), but it is an unstated cell in a table whose whole point is that a wrong cell costs
   a wasted rescan or a silently stale index. Test: **`the_kill_switch_routes_a_phased_partial_to_the_legacy_rebuild`**.

**Tests:** integration tests for launch over an index in each state (nothing, partially covered by phases, partially
covered by a legacy interrupted scan, fully covered, fully covered but stale), asserting which of {phases, replay,
reconcile, rebuild-by-phases} runs; plus one test per truncate door asserting no `TruncateData`. **Test-first** for the
routing table: a wrong cell means a wasted full rescan or a silently stale index. Named individually:

- **`a_stopped_phased_index_resumes_instead_of_rebuilding`**;
- **`a_legacy_interrupted_partial_is_rebuilt`**;
- **`an_upgraded_fully_scanned_volume_replays_and_never_phases`**;
- **`a_completed_external_volume_still_reconciles_at_mount`** (the non-journaled routing hole);
- **`a_wide_journal_gap_during_phasing_does_not_truncate`**.

---

## M4 — Events, status, and the hourglass

**Intent:** the hourglasses, corner and per-folder, are visible whenever we are walking that folder, with a 1-second
debounce so work finishing inside a second never flashes anything.

1. **Crate side:** typed `IndexEvent` variants for a coverage phase (started / progress / ended) carrying the volume,
   the roots, and the counters `CoverWalk` already exposes. ❌ Don't overload `ScanStarted` (the checklist branches on
   typed discriminants; an overloaded event makes the run-kind header lie), and ❌ don't introduce a new payload type
   (it breaches the surface ceilings).
2. **Frontend state:** `index-state.svelte.ts` gains the per-volume set of branches being walked (runes ⇒ `.svelte.ts`).
   **Keep walk COUNTERS out of that same reactive map**: a `SvelteMap.set` per progress tick would re-run the membership
   `$derived` for every visible row on every tick.
3. **Corner hourglass:** `isAnyVolumeIndexing()` is `activity.size > 0 || aggregation.size > 0 || phase.size > 0`, and a
   coverage walk populates none of the three (`ComputeSubtreeAggregates` has no progress callback). So the corner stays
   dark through the entire first index unless this milestone fixes it.
4. **Per-folder hourglass:** replace `isDirSizeUpdating`'s "the volume is scanning" input with "this row is affected by
   ground being walked". Two things the naive version gets wrong:
   - **the test is bidirectional** — `ComputeSubtreeAggregates` repairs the ancestor chain upward, so walking
     `~/Downloads/big` changes the size of `~/Downloads` and `~`. Use
     `rowPath.startsWith(walkRoot) || walkRoot.startsWith(rowPath)`;
   - **three consumers must move together**: `views/FullList.svelte` (two call sites), `views/measure-column-widths.ts`,
     and `selection/SelectionInfo.svelte`. The measurer is the dangerous one: the size column reserves width for the
     glyph, so a per-row renderer against a per-volume measurer clips it on exactly the rows that show it.
5. **The 1-second debounce lives in the BACKEND**, in the app's Tauri indexing layer that already relays index events,
   ❌ not in `index-state.svelte.ts` and ❌ never in the rows. It is a rule ("don't announce a walk that finishes inside
   a second"), and rules belong where they are unit-testable in Rust, in line with the house split of smart backend /
   thin frontend. The backend holds one timer per branch and emits the branch-started event only after 1 s of continuous
   walking, emitting the terminal event immediately. The frontend then renders exactly what it is told: no timers, no
   suppression logic, no `destroyIndexState` cleanup to get wrong. ❌ Don't put the timer in `cmdr-index` itself: the
   crate reports what it is doing, and how long the UI waits before believing it is a presentation decision the app
   owns. Test it in Rust (fires after 1 s, suppressed entirely under 1 s, terminal event always delivered).
6. **The surfaces that assume a full scan** — deliverables here, not follow-ups:
   - **Search dialog index-build progress** (`search-lifecycle.svelte.ts` derives from `isVolumeScanning(root)` +
     `getEntriesScanned()`): the "building your index, N files" state never appears during the first index otherwise.
   - **The per-drive freshness badge** (`navigation/drive-index-status.ts`): `freshness == null` renders gray/`disabled`
     whose only action is "Enable indexing".
   - **The step checklist and run-kind header** (`indexing-steps.ts`): `deriveRunLabel` returns `null` without a
     `ScanRunKind`, so the tooltip renders headerless with no steps.
   - **MCP `cmdr://indexing`** (`mcp/resources/indexing.rs`): built from `scanning` / `entries_scanned` /
     `scan_completed_at`; its purpose is answering "can I trust search on this volume?", and it would answer "not
     scanning, never scanned" for the whole first index. It should report what is covered so far, not just a boolean.
7. **Write down what a first-run user sees while phases run** (corner hourglass with a phase label, sizes appearing
   folder by folder, search saying it is still building) and check it against the running app. That is the whole "wow
   moment" claim; it deserves an explicit acceptance pass, including the mixed state where home has exact sizes and
   `/opt` still shows `<dir>`.

**Tests:** Rust unit tests for the debounce (in the backend layer that owns it) and TS unit tests for the bidirectional
predicate (both genuinely **test-first**); a component test that a row inside _and_ a row above a walking branch show
the hourglass while an unrelated row doesn't; a measurer test that reserved width matches the renderer; and an E2E that
the corner appears during a phase — **a post-hoc pin, not a red→green step**, gated on M3.

**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `file-explorer/views/DETAILS.md` (size state and the measurer contract).

---

## M5 — Surfaces, copy, kill switch

**Every user-facing string here is a DRAFT for David** (principle 4). They go through the message catalog with `@key`
descriptions, ❌ never hardcoded, and **10 locales ship** (`de`, `en`, `es`, `fr`, `hu`, `nl`, `pt`, `sv`, `vi`, `zh`;
verified 2026-08-14) — budget the translation pass rather than discovering it at the end.

1. **The phase labels**, in `IndexingDriveRow` / `IndexingStatusBody` — ❌ not
   `settings/sections/DriveIndexingSection.svelte`, which has three switches and no per-drive rows. Draft: **"Indexing
   the folders you use most"** → **"Indexing the rest of your home folder"** → **"Indexing the rest of the drive"**. ❌
   Not "Indexing your folders" → "Indexing your home folder", which reads as the scope widening and then narrowing,
   since the first is a subset of the second.
2. **The run labels that promise one full scan** need to fit a phased run: `indexing.run.firstScan` ("First full scan"),
   `indexing.scan.label` ("Scanning your drive..."), `indexing.step.findFilesFirstScan` ("First scan, so this can take a
   while").
3. **Search's coverage note** should read correctly when the reason is "we haven't got there yet" rather than "we were
   refused". The shipped unindexed-search work already renders coverage notes and an index affordance, so this is an
   edit to a live system, not a greenfield one.
4. **Folders we couldn't read.** Completion can be stamped with `Denied` / `Declined` / `Abandoned` directories inside
   it, so "done" can mean "done, with holes". Surface the count where the badge and the coverage note already live, ❌
   never silently, with a disclosure listing the paths and the cause (principle 3). Thousands separators on the count.
5. **`stop` and `forget` against a phase queue** (`driveIndexMenuActions('scanning')` offers both). **Decided**: `stop`
   cancels the running walk and clears the queue, leaving covered ground covered and watched, and leaves the branch set
   in place so the next launch resumes instead of rebuilding; `forget` keeps today's meaning.
6. **The kill switch shipped in M3.** Only its user-facing surface belongs here: whether it needs any UI at all (it
   doesn't, if it's a `defaults write` key), and the one-line note in the beta feedback channel telling users it exists.
7. **Measure it.** Anonymous analytics are live and this change's justification is a user-experience claim, so make it
   falsifiable: time to `home_covered_at`, time to `scan_completed_at`, how often a first run is interrupted before
   completing (the case the old design lost entirely), and **time from launch to the first honest size on a folder the
   user actually opened**, which is the wow-moment claim itself and the only one of the four nobody can currently
   answer.
8. **Unrelated, found while measuring**: `onboarding.stepOptional.indexing.descCost` promises "a 300 MB index on your
   drive". Actual on David's machine: 768 MB for the boot index, plus 70 MB importance and 31 MB media. Worth
   correcting.

---

## M6 — Follow-ups, not blockers

1. **Recency signal** via Spotlight `kMDItemLastUsedDate` (`importance/last_used.rs` already samples it, but from inside
   the crate and after the index exists; an app-side `mdfind` at launch would work and needs FDA anyway).
2. **The verifier MARK** (a browsed folder stops needing a walk), which the bail deliberately defers.
3. **"Watch only these folders" as a user setting** — the branch-watch mechanism is already the implementation, and this
   plan makes branch-scoped watching the default shape rather than a retrofit.
4. **Finder sidebar favorites**. Deferred.
5. **The SMB/MTP twin of the abandoned-ground bug** (found 2026-08-14 while fixing the local one; ❌ not fixed).
   `network_scanner/cover_scan.rs` leaves a directory whose listing FAILED with no cause — its own comment says "the dir
   stays unlisted, so the frontier offers it again" — so every later search over an ancestor scope re-pays the same
   failing listing, exactly as the local walker did before `UnreadableCause::Abandoned`. On a NAS that went to sleep
   this fires more readily than the local case does.

   ⚠️ **It is NOT a mechanical port of the local fix, and that is the whole reason it's deferred rather than done.** The
   network walk has two failures wearing one shape, and they want opposite answers:

   - **One directory that won't list** while the share is otherwise healthy — the local case, and `Abandoned` is right.
   - **The share itself going away**, which surfaces as `CONSECUTIVE_FAILURE_ABORT` →
     `VolumeScanError::ConsecutiveFailures` after N failures in a row. Marking those `Abandoned` would condemn every
     directory the walk had reached on the way down, potentially thousands, for a disconnect that heals the moment the
     NAS wakes. ❌ Don't.

   So the work is a design question first (where does the boundary sit? does the abort path unwind the marks it made
   before it tripped, or does it never make them?) and a port second, with its own tests over the SMB fixtures. The
   local half's mechanism — the cause, `ClearAbandonedIfDue`, the arming — is already shared and needs nothing new.
   Guardrail and pointers: `network_scanner/DETAILS.md` § "A failed listing leaves no cause (known bug)".

---

## Risks and containment

1. **Cover-over-`/` slower than the bulk build** ⇒ the M2 benchmark gate, with the stitch included, before the machine
   is written.
2. **The frontier not composing** (the finding that broke draft 1) ⇒ the stitch, plus the first M2 test.
3. **Completion never firing** because one wedged directory holds the frontier open forever ⇒ the `Abandoned` cause
   covering both the timeout and the budget-pruned siblings, plus its two tests. It gates the stamps, the media kick,
   the branch collapse, the sweep keys, and `is_branch_confined`, so it fails wide.
4. **The database never prepared for a walk** (no `EXCLUSION_POLICY_KEY` stamp under `IndexTheVolume`) ⇒ the phased
   start doing that work through writer messages, plus the frontier-shrinks test. This one is silent and total.
5. **A truncate door left open** ⇒ M3.4 enumerates all five; one test each.
6. **The verifier as a second, unthrottled indexer** ⇒ the `phase_active` flag plus the verifier bailing on
   `listed_epoch == 0` while the frontier is unfinished. Both are M2 deliverables, ❌ not "consider it later": with the
   stitch giving every frontier root a row, the verifier's recursive `scan_subtree` fires for every folder the user
   opens ahead of the walker, which is the central user behavior this plan is built around.
7. **A truncate under a live search walk** ⇒ gating `start_scan` on `ground_being_walked` as well as `phase_active`. A
   pre-existing bug this plan would otherwise make routine.
8. **TCC popups from a background walk** ⇒ the FDA gate as today, plus background phases skipping TCC-restricted roots,
   so a prompt only ever follows the user's own navigation.
9. **High-churn directories** (the 1.14M-empty-file Google Drive temp dir) land in the home phase now instead of the
   whole-drive scan, so they hit sooner. Watch for it in the benchmark's browsing arm.
10. **The early media kick misfiring** ⇒ `home_covered_at` driving exactly one subscriber and nothing else, plus the
    `ready_volumes_with_kind` admission test so it survives a relaunch mid-coverage.
11. **The 500 ms tick disappearing with `start_scan`** ⇒ the phase machine owning a `ScanProgressReporter` for the
    machine's lifetime. It fails quietly and looks like three unrelated bugs (dead progress events, no mid-scan sizes, a
    visit poll with nowhere legal to run), which is why it is called out in the full-scan audit rather than left to be
    discovered.

## Decisions (David, 2026-08-13, revised 2026-08-14)

Recorded with the reasoning, because the reasoning is what an implementer needs when reality disagrees with a detail.

1. **The app's answers arrive through host seams, not through widened handle calls.** Priority roots are a `HostPolicy`
   method, and "where is the user" reuses the `open_listings` seam that already exists. `start_root_at_launch` and
   `verify_directory` keep their exact signatures and meanings.
2. **On FDA Deny, both panes stay on `~`**, and background phases skip TCC-restricted roots. The permission dialog fires
   when the user navigates somewhere protected, which is the only moment it has a cause they can see.
3. **The whole drive still gets indexed, by default, with no setting.** An earlier draft narrowed the default scope to
   `$HOME`. Measured on David's machine, that saves 15.4% of the entries (~30 s of a 193 s walk, ~115 MB of 768 MB)
   while still walking `~/Library`, which is 27.7% on its own — so it skipped the small pile and kept the big one. In
   exchange every completion, freshness, rescan, sweep, watch, and upgrade path would have had to learn about a
   permanently partial index; three review rounds found nine blockers in that machinery, four of them introduced by the
   previous round's fixes. **This change is about the ORDER of the walk, not its extent.**
4. **`Fresh` keeps today's meaning** (fully covered), and photo search plus importance get an early start from a
   dedicated `home_covered_at` signal instead. One marker, one subscriber, no new scope concept.
5. **Branch watching stays on when drive indexing is off, on macOS only**, and branches absorb their descendants,
   collapsing to the phase root at the end of each phase and to `["/"]` at full coverage.
6. **"Rescan now" restarts the phases during the phased window** and keeps today's meaning once the volume is complete.
7. **`awaits_its_first_scan` is not re-keyed.** The enable button is fixed at the caller, by starting the phase machine
   instead of a truncating `force_scan`. Every re-keying considered would have made the button a silent no-op on the
   search-walked drives the predicate was written to serve.
8. **A phased partial is told from a legacy interrupted partial by the persisted branch set**, which `start_scan` clears
   before a whole-volume walk. No new marker.
9. **The verifier bails on `listed_epoch == 0` while the frontier is unfinished**, ❌ never marks. The mark is M6.
10. **One worktree, one effort, for the indexing work.** The milestones are an execution order, ❌ not shippable slices.
    The first-run startup state was the exception: it was self-contained, so it shipped ahead of the indexing work.

## Decisions (David + lead review, 2026-08-14, second pass)

11. **The pre-existing bugs ship first, as M0.** Four defects that are latent today and routine under phases. They cost
    nothing to separate, they make the risky milestone smaller, and they are worth having in a beta build even if the M2
    gate sends everything after them back to the drawing board.
12. **The phase machine owns a `ScanProgressReporter`.** Without it the 500 ms tick dies with `start_scan`, taking the
    progress event stream, mid-scan partial aggregation, and the only legal home for the `open_listings` poll with it.
13. **Stitch ONE level, and accept that a visited root can wait behind a large sibling.** Revised from the benchmark
    (`docs/notes/phased-vs-bulk-index-2026-08-14.md`): stitching two levels under the `$HOME` and `/` phase roots was
    supposed to cut the worst-case wait ~3× by making `~/Library/Caches` a frontier root instead of `~/Library`. It cut
    it from 14.0 s to 13.4 s, because the worst case is `~/projects-git` and 97% of that is one child. Depth 2 costs
    nothing measurable (190 ms of extra `readdir`s, 1.1% of wall clock, inside noise) — it simply buys nothing, so ❌
    don't carry a depth parameter for it. Preemption stays out of scope; the wait is bounded by the user's largest
    folder and no stitch depth fixes that.
14. **The hourglass debounce lives in the backend**, in the app's Tauri indexing layer: it is a rule, and rules are
    testable in Rust. The frontend renders what it is told. (❌ Not in `cmdr-index`: the crate reports what it is doing;
    how long the UI waits before believing it is the app's presentation decision.)
15. **The kill switch ships with M3**, the milestone that changes launch routing, and it gets its own row in the routing
    table.

Remaining assumption to confirm during execution, ❌ not a blocker: whether `~/Library`'s size makes `home_covered_at`
late enough to want the M1 refinement.
