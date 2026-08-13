# Phased, priority-driven drive indexing

Status: plan, not started. Branch: `worktree-phased-indexing`.

Turn first-run drive indexing from "one full scan of `/`, all or nothing, a few minutes before anything is useful" into
"the folders this user actually cares about are indexed within seconds, and the rest of the drive fills in behind them".
Nothing is truncated on the way, so every second spent walking stays bought.

Area docs to read before working here: `crates/cmdr-index/src/indexing/CLAUDE.md` (+ its `DETAILS.md`),
`crates/cmdr-index/src/indexing/lifecycle/CLAUDE.md`, `.../lifecycle/cover/CLAUDE.md`, `.../watch/CLAUDE.md`,
`.../scanner/CLAUDE.md`, `.../read/CLAUDE.md`, `crates/cmdr-index/src/indexing/handle/DETAILS.md` (the public-surface
audit), `apps/desktop/src/lib/indexing/CLAUDE.md`, `apps/desktop/src/lib/onboarding/CLAUDE.md`,
`apps/desktop/src-tauri/src/favorites/CLAUDE.md`. Product framing: `docs/design-principles.md`.

## Why

1. **The wow moment is missing.** A new user opens Cmdr, and the one thing that makes it feel unlike Finder (real folder
   sizes, instant search) takes minutes to arrive, spread evenly over a drive they mostly don't care about.
2. **We index the wrong things first.** `/System`, `/Library`, and Xcode caches get walked with the same priority as
   `~/Downloads`. The user is looking at their own files.
3. **The cost lands at the worst moment.** First launch is also onboarding, the AI model download, and first
   impressions. Spending the machine on ground nobody asked for violates "respect the user's resources".

## What the user gets

1. First launch, before any decision: both panes on `~`. Nothing is indexed, nothing is read that would trip a TCC
   popup. (Ships today.)
2. The moment Full Disk Access is granted (today: the relaunch after step 1 of onboarding), the phased indexer starts on
   the user's own folders. By the time onboarding is done, the folders they'll actually open are indexed. If they denied
   FDA instead, nothing in the background ever raises a permission dialog. The startup state that goes with this moment
   already ships: left `~`, right `~/Downloads` on a fresh install, hidden files hidden, and both panes staying on `~`
   on Deny.
3. As they browse, whatever they open gets indexed next, ahead of everything still queued.
4. Home finishes, and that's "done" by default. The rest of the drive follows only if they turn on "Index stuff outside
   my home folder".
5. At every moment the sizes shown are honest: exact where covered, `<dir>` or `≥` where not, an hourglass on ground
   being walked right now.

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
`writer/aggregation.rs:271-304`, so a covered subtree's size reaches `~` and `/` correctly).

**Why this end state, not just the cheap one:** it collapses three mechanisms into one. Search-driven walks, priority
walks, and "index the whole drive" stop being separate things with separate failure modes. It makes the later want
("watch only specific folders, especially on Linux where inotify watches are scarce") the _default_ shape rather than a
retrofit. And it makes indexing interruptible without loss, which is what lets us spend the user's CPU politely.

### The stitch: what makes phases compose at all

**This is the piece without which the whole model silently degrades, and it is not obvious.**

A cover walk marks only the directories it _reads_. Bootstrap deliberately creates the ancestor chain at
`listed_epoch = 0` and claims nothing (`lifecycle/cover/bootstrap.rs:10-13`), and the coverage descent cuts at the first
`listed_epoch == 0` directory without descending past it (`read/coverage.rs:195-207`). Two consequences:

1. After phase 1 covers `~/Downloads`, `coverage(root, "$HOME")` still answers `["$HOME"]` and `coverage(root, "/")`
   still answers `["/"]`. **The frontier for an ancestor scope never shrinks**, so "skip a root that's already covered"
   and "the preempted phase resumes with less to do" are both false for exactly the phases that need them.
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
- the big phases become **many small walks instead of one huge one**, which is what makes priority interleaving cheap
  (below).

Reviewed and confirmed against the descent rule and the epoch rollup: a stitched ancestor reads `Listed` (not `Covered`)
while any child is unlisted, because `repair_dir_stats_upward` recomputes `min_subtree_epoch` as the min over the
directory's own `listed_epoch` and its children's (`writer/aggregation.rs:285-298`). No coverage is claimed that wasn't
earned, and sizes stay honest.

**Four things the stitch must get right:**

1. **Upsert FILES, not just directories.** `listed_children_on` returns `None` while `listed_epoch == 0` and the **full
   child list** the moment it is non-zero (`read/queries.rs:377-386`), and `Index::list_children` feeds the MCP/agent
   `list_dir` tool. So marking a directory listed makes its rows authoritative to a user-visible consumer that same
   instant: a directories-only stitch would report a folder as containing no files. Pin it with a test.
2. **Resolve ids, read the current epoch, and flush before marking.** `MarkDirsListed { ids, epoch }` is a PK-keyed
   `UPDATE`, and marking a row that is still pending in an unflushed batch leaves it `listed_epoch = 0` **forever**
   (`writer/mod.rs:321-330`). The stitch creates the deeper ancestor rows itself, so a `flush_blocking` between the
   upserts and the mark is mandatory, not an optimization. Read `current_epoch` (`IndexStore::read_current_epoch`), the
   same value a walk stamps; ❌ never bump it.
3. **Reuse the diff core, not the whole primitive.** `verify_and_correct` is the right shape (depth-1 readdir diff,
   upserts, stale-row deletes) but must not be used as-is: it recurses into every new subdirectory with
   `scanner::scan_subtree` (`reconcile/verifier.rs:406-419`), which would destroy the point, since new children must
   stay virgin for the parallel walker. `reconcile_subtree` is fully recursive with no depth parameter, so it can't be
   "used at depth 1" either — but it is the model for the mark (it accumulates `listed_dir_ids` and sends one
   `MarkDirsListed` after the walk, `reconciler.rs:953-961`). Extract the depth-1 diff core, have both the verifier and
   the stitch call it, and let the stitch add the mark. ❌ Not `scan_subtree` (`ScanRoot::Rebuild` deletes descendants
   first).
4. **Suppress the verifier while phases run** — see the next section. This one is a data-safety requirement, not a
   performance one.

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
machine isn't walking: between launch and the first phase, after M5's `stop`, while the master switch is off, and
permanently if the user never lets the phases finish. The one-line epoch gate restores exactly today's semantics on
uncovered ground (today the verifier bails because there is no row; post-stitch it bails because nothing has listed it),
and it matches the design rule that the walk owns coverage growth. The read it needs already exists
(`IndexStore::get_listed_epoch_by_id`, used at `read/queries.rs:381`).

**The alternative design is to have the verifier MARK a directory listed when it genuinely read all of it**, turning it
from a producer of `NotVirgin` nodes into a producer of legitimately covered ones. The two are **mutually exclusive**;
pick one deliberately rather than drifting into both. The bail is smaller and safer; the mark is more useful (browsed
folders stop needing a walk at all) and strictly more work to get right.

**Keep `phase_active: Arc<AtomicBool>` on `IndexManager` as well** (crate-internal, no public-surface cost) for the
concurrency half of the problem, fed to **four** places:

- the `scanning` argument of `verifier::maybe_verify`, via `trigger_verification` (`state/scan_control.rs:46`);
- **`start_scan`'s single-flight guard (`manager/start.rs:321`)** — the dangerous one. Today that guard is
  `mgr.scanning`, so with a separate flag `start_scan` would no longer refuse while phases run, and a `force_scan`
  through any surviving door would send `TruncateData` + `BumpCurrentEpoch` and start a parallel walker **while a cover
  walk holds a claim and is still writing**. `cover_context_for` only refuses NEW walks; nothing stops one already
  running. `start_scan` must refuse while `phase_active`, or stop the machine and join its walk first. **Make it a
  handled outcome, not the existing `Err("Scan already running")`**: that error propagates through `force_scan` and the
  IPC, so a naive refusal ships a "Rescan now" that shows a failure for the entire first index. Per the rescan decision
  below, the handled outcome is "restart the phases".
- `awaits_its_first_scan` (`state/queries.rs:93`);
- `get_status`'s `scanning` field (`manager.rs:569`), which M2.9 needs anyway.

❌ **Do not reuse `mgr.scanning` for this.** `cover_context_for` returns `None` while `mgr.scanning` is true
(`lifecycle/state.rs:251-266`), so the phase machine's own `Index::cover` calls would fail with `ScanInProgress`. Leave
that gate exactly as it is.

Checked and clear: `get_writer_and_scanning_for`'s bool reaches only the MTP and SMB watch layers, which stay below the
`is_trait_scanned` early return, and `freshness_bridge.rs:113` ignores it.

**The door `awaits_its_first_scan` guards needs a durable answer too**, because a flag-only one reopens the moment
`phase_active` goes false (phases idle, or stopped from the badge menu) while `scan_completed_at` is still absent.

❌ **But do not re-key the predicate on `entry_count > 1`.** It exists for exactly two shapes — a search-driven walk
that stood up a writer and nothing else, and a first scan someone stopped (`state/queries.rs:79-86`) — and **both have
rows**. Keying on row count would make "Turn on indexing for this drive" a silent no-op on the very volumes the
predicate was written to serve, external drives included. That would be a regression on shipped behavior, introduced by
a change meant to close a truncate door.

Gate the force-scan at the caller instead: `start_volume`'s branch (`handle/mod.rs:183-185`) becomes
`awaits_its_first_scan(vid) && master_enabled && !phased_in_progress(vid)`. A search-walked drive has no marker, so its
enable still force-scans, and the documented case survives.

**The marker's only real job is the crash window** — `phase_active` already covers every in-process case — so: set it
when the machine starts, clear it whenever the machine stops for any reason (completion as part of step 8, M5's `stop`,
and the master-off teardown). Say that plainly, or an implementer will build something more elaborate than it needs.
Note the coupling: if completion never fires, the marker never clears and the per-drive enable button stays a silent
no-op forever, which is a second reason the completion rule has to actually terminate.

### Interleaving without preemption

Because the stitch turns `$HOME` and `/` into a list of independent frontier roots, the machine walks them **one root at
a time and checks the priority queue between roots**. A folder the user opens waits for one subtree, not for the drive.

Cheaper variant worth measuring first: `cover()` already checks its cancel token **between** frontier roots
(`lifecycle/cover.rs:326-330`), so a single `cover()` over a phase's whole frontier is already interruptible at each
root boundary, without paying `finish_branch_coverage`'s persisted meta write per root.

Preemption (cancel the running walk, run the visited root, resume) stays available as a fallback, but it is not the
primary mechanism, because it is expensive and subtle:

- the `Claim` is released by the walk thread on exit (`lifecycle/cover/live.rs:104-117`), so cancel-then-immediately-
  start makes the new walk defer the same ground and cover **nothing** while reporting `roots_covered: 0`. The machine
  MUST `CoverWalk::finish()` (join) before starting the next walk, and MUST treat a non-empty
  `covered_by_another_walk()` as "this phase did not run";
- cancel latency is a watchdog tick plus up to `LOCAL_LIST_TIMEOUT` (15 s) on a parked read, so any debounce must be at
  least the join, not the 1 s the UI uses.

### What a full scan does that cover walks don't (and what we owe each one)

Audited end to end against `manager/start.rs::start_scan` + `lifecycle/scan_completion.rs`:

- **`scan_completed_at`** — the phase machine stamps it at full coverage (below).
- **Scan calibration meta** (`scan_duration_ms`, `total_entries`, `total_physical_bytes`, per walk kind) — nothing
  writes them, so the ETA tier degrades permanently. The phase machine must write the equivalent from its own totals.
- **`ScanCalibration` capture and the live counters.** `scan_calibration` is set only in `start_scan`, and `get_status`
  derives its counters from it plus a live `ScanHandle`. Without it, `status()` reports `scanning: false` with zero
  counters for the entire first index, so the per-drive row, progress bar, and ETA are dead. The phase machine must feed
  the same shape.
- **Events**: `ScanStarted`, `ScanComplete`, `AggregationComplete`, `DirsUpdated["/"]`. The frontend's
  `resetAggregation()` handshake depends on their ordering (`scan_completion.rs:208-211`).
- **`writer.set_expected_total_entries`** — the writer's flushing-progress denominator.
- **The one-shot `dir_stats` ledger heal** (`ArmLedgerHealLatch`, armed only in `resume_or_scan`) and **the shallow-
  sweep ledger** (`reconciler::seed_from_meta` + `record_sweep_completed` + `SHALLOW_SWEEP_AT_KEY`). Skipping the second
  reproduces the bug `manager.rs:398-406` warns about: every launch hands the next shallow anchor a free full sweep.
  Both are reasons to keep `Activation::IndexTheVolume` (below).
- **`BackfillMissingDirStats`**, and a post-scan `WalCheckpoint` (the latter is a NIT — the 30 s maintenance timer also
  checkpoints).
- **Freshness ⇒ `Fresh`** — see the freshness decision below.
- **`RootUnlistable` detection** is volume-root-scan only, so a cover walk over a vanished drive reports "covered
  nothing" instead of the typed abort that clears the stuck UI row. Handle it in the phase machine.

Already handled, do not re-solve: the ROOT sentinel and epoch seeding, `volume_path` meta, and the exclusion-policy
stamp all happen in `prepare_database_for_a_walk` (`state/walk_database.rs:99-121`). **State the stamp explicitly in the
code comments**: a stale or absent `EXCLUSION_POLICY_KEY` sends every scope to the walk wholesale
(`read/coverage.rs:314-317`), which would silently destroy convergence. `SYSTEM_DIR_EXCLUDES` and the exclusion policy
apply identically to every walk. One real behavioral difference: `ScanRoot::Virgin` pins the walk root's **device**
while `ScanRoot::Volume` bounds by path prefix, so the `/` phase cuts at mounted filesystems rather than at `/Volumes/`.
A device cut writes no row, so it can't leave a permanent frontier node, and firmlinked system paths share one device
(`/`, `/System/Volumes/Data`, `/Users`, `/Applications` all report dev=16777231; verified on macOS 25.5 via
`stat -f %d`, 2026-08-13). Acceptable, but it means the `/` phase indexes a slightly different set than today's scan.

### Activation: keep `IndexTheVolume`

❌ **Do not launch the phased volume as `Activation::WriterOnly`.** `journaled` is computed as
`activation == IndexTheVolume && kind.has_event_journal()` (`state/startup.rs:135`), and a `WriterOnly` start never
calls `resume_or_scan`. That would cost, on every launch: no FSEvents journal replay for the boot disk, and (once
`scan_completed_at` exists) a `Stale` load that **bumps the epoch**, rendering every directory size stale forever. The
shallow-sweep seeding (`reconciler::seed_from_meta`) lives in `resume_or_scan` too. `WriterOnly` is designed for a
volume no scan is ever coming for; a launch-time phased index is not that. The phase machine belongs **inside
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
   covered rows render as _current_ when nothing verified them. That is exactly the honesty property the branch-watch
   resume exists to protect.

   **The rule, precisely: the machine's first walk starts only after `resume_branch_watch` has run** (`startup.rs:252`),
   ❌ not merely after the registry insert (`:244`) — the hazard lives in the few lines between them. Concretely:
   `resume_or_scan`'s phased answer only **registers intent**, and `start_indexing_for` starts the machine in its
   `(true, Ok(()))` arm, after `resume_branch_watch`. Spawning the walk from inside `resume_or_scan` and hoping is racy.
   ❌ Moving `branches::resumed_for` earlier is NOT an equivalent fix: it restores the branch set but not the bump,
   because `ensure_branch_watch` returns at its first line once a watcher is running.

2. **The `dir_stats` ledger heal is armed but never paid.** `ArmLedgerHealLatch` is disarmed by the next successful
   `ComputeAllAggregates`, and cover walks send only `ComputeSubtreeAggregates` — so the latch stays armed and re-arms
   every launch, and the heal never happens. Fix is one message: send `PayLedgerIfUnpaid` (`writer/mod.rs:415-421`,
   which runs a full `ComputeAllAggregates` iff armed and no-ops otherwise) at full coverage, alongside
   `scan_completed_at`.
3. **Placement inside `resume_or_scan` is constrained.** The phased answer sits _after_ the sweep seed and latch arm
   (`manager.rs:409-430`) and _after_ the `should_replay_journal` branch (`:435-467`), replacing only the final
   `start_scan` fallthrough (`:474-497`). ❌ It must stay below the `is_trait_scanned` early return (`:388`), or SMB and
   MTP volumes get routed into a local phase machine.

### Watching: probably no handover at all

On macOS `DriveWatcher::start_branches` already watches the **volume root** and filters by `WatchScope::Branches`
(`watch/watcher.rs:204-211`) — which is exactly the "watch `/`, keep only what we care about" model. On Linux it watches
each branch, deliberately: `notify`'s recursive mode costs one inotify watch per directory against `max_user_watches`.

So a fully covered volume can simply keep `WatchScope::Branches` with `/` as its single branch, and the
branch→whole-volume handover never has to be written. On Linux a `/` branch is watched recursively, which is the same
cost as whole-volume watching. **Prefer this**, with one required change and one bonus:

- **Required: teach `is_branch_confined()` to ask the real question.** A `Branches` scope never takes the
  visible-scanner route for a `MustScanSubDirs` anchor, whatever its depth (`reconcile/reconciler.rs:369-374`,
  `:517-527`). Keeping `Branches` forever therefore means a fully covered boot disk **never sweeps again**: every
  coalesced root-scale anchor (macOS saying "a lot changed under here" and losing the detail) goes to the throttled
  `reconcile_subtree` drain on a shallow anchor, which is exactly the "holds the per-dir hourglass for the better part
  of a full scan" case the depth split exists to avoid, and the sweep-window bookkeeping accumulates with no sweep. One
  line fixes it: `is_branch_confined()` is false when the branch set covers the volume root — precisely
  `WatchScope::branches().covers(volume_root)` (`branches.rs:481-485`; `contains` matches `path == self.path`, so a `/`
  branch satisfies it, and the reconciler already holds `self.space` for the root string). That restores the shallow
  sweep at exactly the moment the volume genuinely answers for everything. **It does not reopen the truncate door, but
  only because `scan_completed_at` is stamped by then** (which flips `local_rescan_reconciles` to true, so a shallow
  anchor reconciles in place instead of truncating) — which is why the completion ORDER below is load-bearing.
- **Bonus, while the volume IS branch-confined**: truncate door (d) in M3 is closed by construction, and so is a door
  the stitch would otherwise open — the stitch creates depth-1 and depth-2 branches, and `SHALLOW_RESCAN_MAX_DEPTH = 2`
  would send those to `perform_registry_rescan` → a truncating `start_scan` if the scope were `WholeVolume`.
- Verified for the single-`/`-branch shape: `may_walk` (`covers("/")` is true), `admit` (a `/` branch is
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
finishes), which is the general rule that the full-coverage collapse to `["/"]` then falls out of as a special case.

**The branch set needs an explicit collapse.** `begin_covering` pushes one `Branch` per path, so N frontier roots means
N branches (this is inherent to the stitch, not to interleaving: it's the same count whether one `cover()` takes N roots
or N calls take one each). Every event then pays an O(branches) scan in `deepest_containing` on the live hot path, and
`finish_covering` only absorbs the path being finished into an existing ancestor branch — siblings never absorb each
other, so the set never collapses on its own. Expect roughly 50–150 entries during the phases (children of `/`, of
`$HOME`, and the priority roots). At full coverage, replace the set with `["/"]` — but ❌ **not via `branches::clear`
plus a begin/finish pair.** `clear` calls `forget`, which drops the map entry, while the live loop and its reconciler
each hold their own `Arc<BranchWatch>` captured at `ensure_branch_watch`; `live_for` would then mint a **brand-new**
`BranchWatch` that nothing is reading. The persisted meta would say `["/"]` while the running loop kept filtering
against the stale N-entry set for the rest of the session — and the `is_branch_confined` test above would read that same
stale Arc and stay true, leaving the shallow sweep disabled until the next launch. Silent, and hard to notice. (The
existing `clear` call in `start_scan` is safe only because the loop is torn down and replaced in the same breath.)
Instead add a crate-internal `collapse_to(root)` that mutates the **shared** `BranchWatch` in place — replace
`state.branches` with a single root `Branch`, leave any `walks > 0` entry alone, then `persist()`. During the phases,
either accept the N-entry set or collapse a phase's children into its root on completion; measure `deepest_containing`
under a churn burst before deciding.

### Freshness, and the two subsystems that depend on it

**This is the decision with the largest product blast radius, and the first draft of this plan missed it entirely.**

Folder importance and the whole media index (OCR, Vision tags, CLIP embeddings — that is photo search) start their
passes off `Freshness::Fresh` plus a `ScanCompleted` publish on the lifecycle bus
(`state/queries.rs::ready_volumes_with_kind` filters on `Fresh`; the bus publish fires only on
`FreshnessEvent::ScanCompleted`, `state/freshness_bridge.rs:95-98`, which today only `scan_completion.rs:351-358`
fires). If nothing fires until the final `/` phase ends, **photo search and importance scoring are dead for the entire
phased period, and forever on a machine that never finishes `/`.**

**Decision: `Fresh` means "fully covered for the scope the user chose", so it stays honest without stalling.**
Concretely: with `index_outside_home` **off** (the default), the volume goes `Fresh` when `$HOME` is fully covered; with
it **on**, only when the whole drive is. Freshness then never claims more than the user asked us to hold, the badge is
truthful in both modes, and on the default setting importance and photo search come alive minutes into the first run
rather than never. Full coverage of the chosen scope is also what stamps `scan_completed_at`, so completion and
freshness stay one concept rather than two.

Audited, and it is safe: **search never reads freshness at all** (it goes through `coverage()` / `cover()`, so it is
coverage-gated by construction); `Index::is_fresh` has exactly one app caller
(`file_system/write_operations/journal_search.rs:102`), which applies its own coverage gate
(`min_subtree_epoch > 0 && == current_epoch`) and downgrades to `index_stale` otherwise — **pin that with a test, since
this plan now leans on it**; and the `ready_volumes_with_kind` consumers work over whatever the index holds, with the
later full-coverage `ScanCompleted` retriggering a full recompute.

Two consequences of that decision:

- **With the toggle ON, the drive stays un-`Fresh` for a long time**, and importance plus photo search wait it out. That
  is the user having asked for the bigger job; the badge and the copy should say so.
- **With the toggle OFF (the default), the media index starts when home completes**, which is the right moment: the
  walker is finished with everything the user asked for, so OCR / Vision / CLIP enrichment is not competing with it.
  This is what makes the scope decision and the freshness decision one decision rather than two.
- `enqueue_initial_full_pass_if_unscored` only scores a volume whose importance store has no generation yet, so a later
  launch mid-coverage re-scores nothing. Under this decision that window is small (home completes early), but say it
  rather than discover it.
- `enqueue_initial_full_pass_if_unscored` only scores a volume whose importance store has no generation yet. Once the
  early pass stamps one, a later launch with coverage still incomplete re-scores nothing and no `ScanCompleted` fires
  until full coverage, so the importance ranking sits frozen at the priority-phase snapshot across launches (softened,
  not fixed, by the incremental `record_visit` / `publish_dirs_changed` paths).

## What already exists (do not rebuild it)

Confirmed by reading the code, 2026-08-13:

- `Index::cover` / `Index::coverage` / `Index::coverage_token`. Reference caller: live search
  (`apps/desktop/src-tauri/src/search/execute/live_run.rs:167`). Note `CoverageMap.frontier` is explicitly **unordered**
  (`read/coverage.rs:134-137`), so walk order can't be read out of a coverage answer.
- `IndexManager::begin_branch_coverage` / `finish_branch_coverage` / `ensure_branch_watch`
  (`lifecycle/manager/start.rs`): register ground before a walk touches it (so live events buffer instead of racing),
  then watch what was covered. `ensure_branch_watch` is conditional: local-scanner kind, no watcher running, non-empty
  branches, and `master::branch_watch_allowed` (which ANDs the master switch).
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

## Where the app's answers enter the crate

Three things the index needs are **the app's to answer**: which folders matter to this user, how far indexing should go,
and where the user is looking right now. `indexing/host/` is the established home for exactly that — "add a seam here,
never a new `crate::<app module>` import", and "vocabulary moves down; questions become seams" (`host/CLAUDE.md`). So
none of this arrives as an argument bolted onto a launch call:

1. **Scope** (the "Index stuff outside my home folder" toggle) is a field on `IndexConfig` — `index_outside_home: bool`,
   applied through the existing `set_config`, exactly as the media policy is. It is a stored setting, which is what that
   struct is for, and it re-applies **live** when the user flips it: turning it on adds frontier (coverage is add-only),
   turning it off simply stops maintaining rows we already hold. A plain `bool` rather than a `DriveScope` enum because
   a new public type would breach the ceilings below; if it ever grows a third value, that is the moment to argue for
   the enum.
2. **Priority roots** are a method on the existing `HostPolicy` trait (`host/policy.rs`), beside the other "what has the
   user's attention" question. Asked when the machine needs them, so an edited favorites list or a new session's tabs
   are picked up without a restart, instead of being frozen at launch.
3. **Where the user is right now needs no new door at all.** `HostPolicy::open_listings()` already reports every
   directory a pane is showing (it exists so mid-scan aggregation can punch the visible folders through the depth cap).
   The phase machine polls it between frontier roots and keeps a small recently-seen set, so a folder the user opened
   and left still gets queued. ❌ **This replaces the earlier idea of widening `Index::verify_directory`**, which was
   both a forced fit and too loose a signal (it fires for the opposite pane, MCP listings, and refreshes).

`Index::start_root_at_launch` therefore keeps its exact signature, and `verify_directory` keeps its exact meaning. The
only handle-level change in the whole plan is behavioral, inside the crate.

**The ceilings this respects:** `scripts/check/checks/index-crate-isolation.go` caps `cmdr-index` at exactly what it
exposes — measured 2026-08-13, `50 root promises, 40 handle methods, 17 public modules, 156 items` against ceilings of
`50 / 40 / 17 / 156`, zero headroom in all four buckets, and a raise needs David's explicit say-so. `countModuleItems`
matches column-0 `pub struct/enum/fn/const/type` and `pub use` leaves (`index-crate-isolation.go:506-539`), so **struct
fields, trait methods, and enum variants are all free** — which is why the shape above costs nothing. ❌ A new payload
TYPE on an event, a new `pub fn` on a public type, or a `DriveScope` enum would each breach immediately. New
`IndexEvent` variants and `UnreadableCause::Abandoned` need doc comments (`#![deny(missing_docs)]`) and a regenerated
`bindings.ts`.

## Milestone map

What remains ships as **one effort on one worktree**, so the milestones are an execution ORDER, ❌ not shippable slices.
Land them in sequence and keep the tree green at each boundary.

**Already shipped, separately from this plan:** the first-run startup state. Dotfiles are hidden by default, and a fresh
install with Full Disk Access opens left `~` / right `~/Downloads` exactly once, never over a layout somebody already
has. The rule and its guardrails live in `apps/desktop/src/lib/file-explorer/pane/first-run-layout.ts`; the persistence
trap it depends on is in `docs/architecture-patterns.md` § Persistence. One piece of its test list was deliberately
skipped and is still worth writing: **a Playwright E2E over a first run with `CMDR_MOCK_FDA`**. Everything below assumes
that startup behavior is in place.

- **M1** — priority-root computation plus the two host seams.
- **M2** — the stitch plus the phase machine, gated on a benchmark.
- **M3** — launch, resume, and every path that would truncate.
- **M4** — events, status, and the hourglass UI.
- **M5** — settings (including the scope toggle), surfaces, kill switch.
- **M6** — optional signals and follow-ups.

M1 touches nothing M2 depends on, so it can run alongside M2's benchmark if an agent is idle. Everything after
M2 is strictly sequential. One ordering constraint that bites: **M4's unit tests stand alone, but its end-to-end
assertion can't run until M3 lands**, because the surfaces it fixes only misbehave once the phase machine is real.

---

## M1 — Which folders matter to this user

**Intent:** guess the user's important folders from signals we already have, cheaply, with no new permissions and no
network. Ordered best-signal-first, because the order _is_ the schedule.

A new app-side module (`apps/desktop/src-tauri/src/indexing_priority/`) exposing one function: the ordered,
deduplicated, existence-checked roots. It is called from `AppHostPolicy::priority_roots` (`priority::host_policy`), so
the answer is recomputed when asked rather than frozen at launch. Keep it cheap: the seam is asked at phase boundaries,
but the trait's contract is "don't do I/O, don't take a contended lock" for its other method, so cache the answer behind
a short TTL rather than stat-ing a dozen paths per call.

1. **Last session's tab paths**, most recently active first, from `app-status.json`. Empty on a true first run. The
   strongest signal there is: it is literally where the user was.
2. **Cmdr favorites** (`favorites::store::list()`), in the user's order. Platform-dependent seed (macOS vs Linux
   differ), so ❌ don't hardcode the macOS four.
3. **Standard home folders that exist and are non-empty:** `Downloads`, `Documents`, `Desktop`, `Pictures`, `Movies`,
   `Music`. ❌ Never `~/Library`.
4. **Cloud roots that exist:** children of `~/Library/CloudStorage/`, `~/Dropbox`,
   `~/Library/Mobile Documents/com~apple~CloudDocs`. After the local ones deliberately: File Provider reads can stall,
   and though the guarded walker survives that, a stall should not delay `~/Downloads`.
5. **`$HOME` itself**, last.

Then the machine appends the volume root as the final phase, **only when `index_outside_home` is on** (see the scope
decision below).

**`~/Library` is in scope but never a priority root.** It is inside home, so home coverage includes it, and search over
it is occasionally what a user wants. It is also where the pathological churn lives (the 1.14M-empty-file Google Drive
temp directory in `docs/specs/later/sealed-subtrees-plan.md`), so it must never be one of the roots we walk first, and
`sealed-subtrees` remains the real fix for that case rather than anything invented here. **Assumption, flag it if
wrong.**

Rules: dedupe; drop any root that is a descendant of an earlier one; cap the list (24 is a reasonable start) so a user
with 200 favorites doesn't turn phase 1 into a drive walk; and existence-check **without tripping TCC while the gate is
pending** by reusing `restricted_paths::tcc_paths::is_potentially_tcc_restricted` (even `Path::exists()` trips a popup;
`volumes::get_favorites` already has this rule — ❌ don't hand-roll a second one).

**Tests:** pure-function unit tests over a synthetic home (ordering, dedupe, descendant-drop, cap, missing paths, empty
first run, both platform seeds). Test-first: pure logic, many branches.

**Docs:** a `CLAUDE.md` + `DETAILS.md` pair for the new module (the checker enforces pairs), plus a line in
`docs/architecture.md`.

---

## M2 — The stitch and the phase machine

**Intent:** walk the priority roots in order, let the user's navigation jump the queue, then home, then the drive, and
never lose a walk's work to a later one.

Lives in `crates/cmdr-index/src/indexing/lifecycle/` beside `cover.rs`; ❌ nothing below `lifecycle` may import
`lifecycle::state`.

### The gate: measure before committing

Measure on a real `/`: (a) today's truncate-and-bulk-build full scan, and (b) stitch + phased cover walks (M1 roots,
`$HOME`, then the `/` frontier). Wall clock to full coverage, plus peak RSS.

**The benchmark must include the stitch**, or arm (b) measures the `NotVirgin` serial repair and looks catastrophic, or
measures a virgin `/` walk that the product would never actually run. Venue: `crates/index-query` or an in-crate
`#[cfg(test)]` bench — ❌ not `crates/cmdr-index/benches/`, which compiles against the crate as EXTERNAL and can only
reach the public surface. Write the numbers to `docs/notes/phased-vs-bulk-index-<date>.md`, link it from the lifecycle
`DETAILS.md`. **If (b) is more than roughly 1.5× (a) to full coverage, stop and re-decide with David.**

### The machine

1. **Activation stays `IndexTheVolume`**, and the phase machine is a third answer inside `resume_or_scan`, beside replay
   and scan. (Rationale above: journaling, launch freshness, and the shallow-sweep seeding hang off this; the
   `dir_stats` ledger heal additionally needs `PayLedgerIfUnpaid` at completion, or it is armed and never paid.)
2. **The stitch runs before each phase** (described above): list each ancestor of the phase root, mark that one
   directory listed, don't descend. Ship it together with the **`phase_active` flag and the verifier changes** — the
   stitch without them is a net regression, so they are one unit of work, not two.
3. **The queue**: rank 0 the M1 roots, rank 1 roots the user visited while running, rank 2 `$HOME`, rank 3 the volume
   root. One walk at a time (`cover` is already internally parallel; a second concurrent walk fights it for the disk and
   the writer). Between frontier roots, re-check the queue — that is what makes interleaving cheap.
4. **Each phase step**: `coverage(volume_id, root, Listing)` for the frontier; empty ⇒ skip; otherwise walk its roots
   one at a time. The walk marks, aggregates, and claims its own ground.
5. **Visits enter through `HostPolicy::open_listings()`**, polled between frontier roots, with a small recently-seen set
   so a folder the user opened and left is still queued. ❌ Not through `Index::verify_directory` (too loose: the
   opposite pane, MCP listings, and refreshes all fire it) and ❌ not by widening any handle method. Respect the seam's
   own rule: `open_listings` allocates and is documented as tick-rate, ❌ never per-entry — between frontier roots is
   well within that.
6. **Preemption is the fallback, not the mechanism** — with the join-before-restart and debounce rules above.
7. **Completion is derived, not remembered — but "empty frontier" alone is not a terminating rule.** `abandoned_ground`
   is per-walk and in-memory, so it can't answer "was anything abandoned in a previous session?"; the durable signal is
   that an abandoned directory is never marked listed, so it re-enters the frontier.

   **The trap**: a directory the walker timed out on gets **no `unreadable_cause`** — deliberately, "since mounts heal"
   (only denied ids carry a marker, `scanner/mod.rs:881`). So it stays `Frontier` forever, "the frontier is empty except
   for `permission_denied` / `declined`" can never become true, and _everything_ hanging off completion never happens:
   the stamp, `PayLedgerIfUnpaid`, the sweep keys, the branch collapse, the media kick, `is_branch_confined` flipping.
   Every launch re-walks it, times out again at 15 s a directory, and stalls in the same place.

   **The fix is a third `UnreadableCause`, not a pass counter.** Give the walk `UnreadableCause::Abandoned` for a
   directory it gave up on, and completion goes back to being a pure function of the database — "frontier empty, only
   unreadable causes left" — durable across relaunch, immune to churn, with no in-session bookkeeping. The machinery
   already fits:
   - `UnreadableCause` is `Denied = 1` / `Declined = 2` and `from_stored` falls back to `Denied` for anything unknown
     (`store/errors.rs:19-49`), so `Abandoned = 3` is additive and an older build reading a newer DB degrades
     truthfully. The index is a disposable cache, so no migration.
   - `MarkDirsUnreadable { ids, cause }` already exists, and **`MarkDirsListed` clears the cause**
     (`writer/mod.rs:332-342`), so it self-heals on the next successful listing with no rebuild — the same contract
     `Denied` already relies on.
   - The verdict match (`read/coverage.rs:277-282`) is exhaustive over the two variants, so a third one is a **compile
     error** at exactly the place that must grow a bucket. The decision can't be silently skipped.
   - Free under the surface ceilings: neither a new enum variant nor a new `CoverageMap` field is counted.

   **The tradeoff, stated rather than hidden:** marking a timeout `Abandoned` takes it out of the frontier, so nothing
   re-attempts it, and with the verifier now bailing on `listed_epoch == 0` its heal path is narrower than `Denied`'s.
   For an external disk that was merely spinning up, that is pessimistic. Refinement: treat `Abandoned` — ❌ not
   `Denied` or `Declined` — as frontier-eligible again in a **new session**. One attempt per launch, terminates within a
   session, heals across launches.

   ❌ **Don't use a "frontier didn't shrink across two passes" rule instead.** It has to compare sets rather than counts
   (a pass can legitimately grow the frontier by listing a root and exposing the abandoned directories inside it), it
   never terminates on a continuously-written drive (a build or a sync client produces new unlisted rows every pass —
   see risk 8), and being session-scoped it re-pays a full re-walk plus 15 s per wedged directory on every launch.

   ⚠️ This is still the newest part of the plan. Keep the test
   `a_permanently_timing_out_directory_still_lets_completion_happen`, and nail the details down in M2.

8. **On completion, in this ORDER — and the order is enforced by a FLUSH, not by the numbering.** Steps 1–6 are writer
   _messages_; step 7 is in-process state. The read the whole ordering protects (`local_rescan_reconciles`'s
   `get_index_status()` inside `start_scan`) goes through a read connection, so it sees the stamp only once the writer
   has committed it — and step 3 runs a full `ComputeAllAggregates` over a complete `/` index, which is minutes of
   writer-thread work sitting between the stamp being queued and being visible. **Flush after step 1 and before step
   7**, or the collapse lands inside exactly the window the order exists to close. Use the shape that matches the
   context: `writer.flush().await` from async (as `scan_completion.rs:228` does), or
   `tokio::task::block_in_place(|| writer.flush_blocking())` from a sync path in an async context (as
   `manager/start.rs:432` does). ❌ A bare `flush_blocking()` blocks a runtime worker; it is only safe on a plain
   `std::thread`, which is why the cover walk's own call is fine.
   1. stamp `scan_completed_at`;
   2. write the calibration meta;
   3. `PayLedgerIfUnpaid` (nothing else ever pays the armed `dir_stats` ledger heal);
   4. `BackfillMissingDirStats`;
   5. `reconciler::record_sweep_completed` plus the `SHALLOW_SWEEP_AT_KEY` / `SHALLOW_COALESCED_KEY` writes — without
      these the in-memory `SweepRecord` stays `None` for the session (it is seeded from meta only at launch), so the
      very first shallow anchor after completion triggers a full sweep nobody asked for;
   6. publish freshness and fire the terminal events;
   7. **only then** collapse the branch set to `["/"]`. Collapse before the stamp and there is a window where the volume
      is neither branch-confined nor marked complete, and one shallow anchor in it truncates the finished index.
9. **Feed the live status shape** (`ScanCalibration`-equivalent counters) throughout, or the per-drive row, progress
   bar, and ETA stay dead for the whole first index. Drive `get_status`'s `scanning` field from the **`phase_active`
   flag**, ❌ never by setting `mgr.scanning` (that would make the machine's own `cover()` calls fail).
10. **Handle `RootUnlistable`** yourself: a cover walk over a vanished drive otherwise reports "covered nothing" instead
    of the typed abort that clears the stuck UI row.
11. **Master switch and per-drive veto** keep outranking everything.

**Tests** (integration, `crates/cmdr-index/src/indexing/tests/`, over the disk-image fixture and `InMemoryVolume`):

- **`frontier_excludes_covered_ground_after_a_stitch`** — and every frontier root it returns is virgin. This is the
  finding that broke the first draft; pin it hard. **Test-first.**
- **`the_verifier_leaves_an_unlisted_directory_alone`** — the whole data-safety story of the stitch. **Test-first.**
- **`a_stitched_directory_lists_its_files_not_only_its_subdirectories`**.
- **`a_listing_of_ground_a_walk_is_covering_writes_nothing`** (the claim / `may_walk` case). **Test-first.**
- **`start_scan_refuses_while_a_phase_is_active`** — a truncate under a live walk is the worst failure this plan can
  have. **Test-first.**
- **`the_branch_collapse_is_visible_to_the_running_live_loop`** (not just to the persisted meta).
- **`a_relaunch_with_no_replayable_journal_bumps_the_epoch`** — the resume-honesty property.
- **`completion_pays_the_ledger_and_seeds_the_sweep_keys`**.
- **`a_permanently_timing_out_directory_still_lets_completion_happen`** (the bounded-progress rule).
- **`enabling_indexing_for_a_search_walked_drive_still_scans_it`** — the shipped behavior `awaits_its_first_scan`
  protects, which the truncate-door work must not regress.
- `phases_run_in_order`, and a covered root is skipped without a walk.
- `a_visited_root_is_taken_between_frontier_roots` without cancelling anything.
- `a_preempted_walks_rows_survive` (row count only grows), and the restart joins before starting.
- `full_coverage_stamps_completion_once`; abandoned ground prevents it.
- `master_off_runs_nothing`.

**App-side, not in the crate**: `is_fresh` over partially covered ground still makes `journal_search` downgrade to
`index_stale`. `journal_search` lives in `apps/desktop/src-tauri/src/file_system/write_operations/`, which the crate
can't name; `enumerate_subtree_for_search` already has a `#[cfg(test)] test_hook` seam for exactly this.

**Docs:** `lifecycle/CLAUDE.md` (one must-know per new invariant, terse), `lifecycle/DETAILS.md` (the stitch and why,
the phase model, interleaving, completion), `indexing/DETAILS.md` (the data flow now that there is no first full scan),
the benchmark note.

---

## M3 — Launch, resume, and every path that truncates

**Intent:** a partially covered volume must come back as a partially covered volume, and nothing may quietly truncate
it.

1. **`start_root_at_launch(fda_pending)` is unchanged**; the roots and the scope arrive through the host seams instead
   (see "Where the app's answers enter the crate"). The app side is an `AppHostPolicy::priority_roots` implementation
   plus the new `IndexConfig` field.
2. **`resume_or_scan` learns the phased answer** (see M2.1). The queue itself needs no persistence: it is recomputed
   from the M1 roots plus a coverage query per root, so a launch naturally skips what is done. Prefer that over
   persisted queue state, which can go stale or disagree with the database.
3. **Close every truncate door.** A cover-built index has `entry_count > 1` and no `scan_completed_at`, so
   `local_rescan_reconciles` is false and `start_scan` sends `TruncateData`. Two of these close for free once
   `phase_active` feeds `awaits_its_first_scan` (the FDA Deny path and the per-drive enable button), and one is already
   closed while the volume is branch-confined (the shallow anchor). **Door (b), master off→on, is untouched by any of
   that and is the one that needs explicit work.** Verify each with a test rather than trusting the reasoning. The ways
   in today:
   - **FDA Deny** ⇒ `start_indexing_after_fda_decision` → `start_volume(root)` → `awaits_its_first_scan` true ⇒
     `force_scan` ⇒ truncating full scan (`commands/indexing.rs:221`, `handle/mod.rs:177-187`). Note this fires on the
     Deny path even though the panes stay on `~`, so the decision to keep both panes home does NOT make this door go
     away.
   - **Master switch off→on** ⇒ `drives_to_resume()` always includes root ⇒ `start_volume` ⇒ `state::start_indexing()` ⇒
     `resume_or_scan` ⇒ `start_scan("incomplete previous scan")`.
   - **"Rescan now"**: it keeps today's meaning for a completed index (re-walk **in place**, diff each directory and
     write only changes, so sizes stay visible and marked stale throughout — already what `local_rescan_reconciles` does
     for a populated, previously-completed index). The work is making a phased volume qualify rather than falling into
     truncate-and-rebuild. **During the phased period it means "restart the phases"**, ❌ never an error.
   - **A coalesced shallow `MustScanSubDirs`** ⇒ `perform_registry_rescan` → `start_scan`
     (`reconcile/reconciler/rescan.rs:122-126`).
   - **`awaits_its_first_scan`** will report "never walked" forever on a phased volume, so the per-drive "Turn on
     indexing for this drive" button force-scans too. The predicate needs a phased-aware answer.
4. **Freshness during phases** per the decision above. `StaleDriveDialog` already returns early for `root`, so the
   exposed surface is the per-drive **badge**, not the dialog: a volume that has never reached full coverage is
   _incomplete_, not _stale_, and those are different sentences to the user.

**Tests:** integration tests for launch over an index in each state (nothing, partially covered, fully covered, fully
covered but stale), asserting which of {phases, replay, scan} runs; plus one test per truncate door asserting no
`TruncateData`. Test-first for the routing table: a wrong cell means either a wasted full rescan or a silently stale
index.

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
5. **The 1-second debounce lives in the publisher**, not the rows: `index-state.svelte.ts` exposes a branch only after
   it has been walking 1 s continuously, and drops it immediately on the terminal event. One timer per branch, owned by
   the module, cleared in `destroyIndexState`. ❌ No timers in rows (a `$derived` can't hold one; a per-row interval is
   a per-row leak).
6. **The surfaces that assume a full scan** — deliverables here, not follow-ups:
   - **Search dialog index-build progress** (`search-lifecycle.svelte.ts` derives from `isVolumeScanning(root)` +
     `getEntriesScanned()`): the "building your index, N files" state never appears during the first index otherwise.
   - **The per-drive freshness badge** (`navigation/drive-index-status.ts`): `freshness == null` renders gray/`disabled`
     whose only action is "Enable indexing".
   - **The step checklist and run-kind header** (`indexing-steps.ts`): `deriveRunLabel` returns `null` without a
     `ScanRunKind`, so the tooltip renders headerless with no steps.
   - **MCP `cmdr://indexing`** (`mcp/resources/indexing.rs`): built from `scanning` / `entries_scanned` /
     `scan_completed_at`; its purpose is answering "can I trust search on this volume?", and it would answer "not
     scanning, never scanned" while indexing runs.
7. **Write down what a first-run user sees while phases run** (corner hourglass with a phase label, sizes appearing
   folder by folder, search saying it is still building) and check it against the running app. That is the whole "wow
   moment" claim; it deserves an explicit acceptance pass.

**Tests:** unit tests for the debounce publisher and the bidirectional predicate (both genuinely test-first); a
component test that a row inside _and_ a row above a walking branch show the hourglass while an unrelated row doesn't; a
measurer test that reserved width matches the renderer; and an E2E that the corner appears during a phase — **a post-hoc
pin, not a red→green step**, gated on M3.

**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `file-explorer/views/DETAILS.md` (size state and the measurer contract).

---

## M5 — Surfaces, copy, kill switch

0. **The scope setting itself**: "Index stuff outside my home folder", default OFF, in the drive-indexing section,
   written into `IndexConfig.index_outside_home` through the existing `set_config` path so flipping it takes effect
   without a restart. Turning it ON queues the volume-root phase; turning it OFF stops after the current root. Copy is a
   draft for David.
1. **The phase label in the user's terms** ("Indexing your folders", "Indexing your home folder", "Indexing the rest of
   this drive"), in `IndexingDriveRow` / `IndexingStatusBody` — ❌ not `settings/sections/DriveIndexingSection.svelte`,
   which has three switches and no per-drive rows. Copy is a draft for David; all strings go through the catalog with
   `@key` descriptions, ❌ never hardcoded.
2. The drive-index settings section explains the model in a sentence or two.
3. Search's coverage note should read correctly when the reason is "we haven't got there yet" rather than "we were
   refused".
4. **`stop` and `forget` need a defined meaning against a phase queue** (`driveIndexMenuActions('scanning')` offers
   both). Proposal: `stop` cancels the running walk and clears the queue, leaving covered ground covered and watched;
   `forget` keeps today's meaning.
5. **A kill switch.** This is a big behavioral change to ship into an open beta: one flag (env var is enough) that
   restores the bulk-build path, so a bad week is a restart rather than a rollback.

---

## M6 — Follow-ups, not blockers

1. **Recency signal** via Spotlight `kMDItemLastUsedDate` (`importance/last_used.rs` already samples it, but from inside
   the crate and after the index exists; an app-side `mdfind` at launch would work and needs FDA anyway).
2. **"Watch only these folders" as a user setting** — the branch-watch mechanism is already the implementation.
3. **Finder sidebar favorites**. Deferred.

---

## Risks and containment

1. **Cover-over-`/` slower than the bulk build** ⇒ the M2 benchmark gate, with the stitch included, before the machine
   is written.
2. **The frontier not composing** (the finding that broke draft 1) ⇒ the stitch, plus the first M2 test. 2b.
   **Completion never firing** because one wedged directory holds the frontier open forever ⇒ the bounded-progress rule,
   plus its test. Note this also gates the media kick and the branch collapse, so it fails wide.
3. **A partially covered volume claiming completeness** ⇒ completion derived from a durable empty frontier, not from
   in-memory `abandoned_ground`.
4. **Photo search and importance silently dead** ⇒ freshness meaning "fully covered for the chosen scope", which on the
   default setting arrives when home completes.
5. **A truncate door left open** ⇒ M3.3 enumerates all five; one test each.
6. **The verifier as a second, unthrottled indexer** ⇒ the `phase_active` flag plus the verifier marking a directory
   listed when it genuinely read all of it. Both are M2 deliverables, ❌ not "consider it later": with the stitch giving
   every frontier root a row, the verifier's recursive `scan_subtree` fires for every folder the user opens ahead of the
   walker, which is the central user behavior this plan is built around.
7. **TCC popups from a background walk** ⇒ the FDA gate as today, plus the Deny-path decision (panes stay on `~`, and
   background phases skip TCC-restricted roots, so a prompt only ever follows the user's own navigation).
8. **High-churn directories** (the 1.14M-empty-file Google Drive temp dir in `docs/specs/later/sealed-subtrees-plan.md`)
   land in the home phase now instead of the whole-drive scan, so they hit sooner. Watch for it during the benchmark.

## Decisions (David, 2026-08-13)

All seven open questions are answered. Recorded here with the reasoning, because the reasoning is what an implementer
needs when reality disagrees with a detail.

1. **The app's answers arrive through host seams, not through widened handle calls.** Scope is an `IndexConfig` field,
   priority roots are a `HostPolicy` method, and "where is the user" reuses the `open_listings` seam that already
   exists. `start_root_at_launch` and `verify_directory` keep their exact signatures and meanings. The earlier "add a
   parameter, widen a method" shape was a forced fit around the surface ceilings; this one is what the crate's own seam
   rules already prescribe, and it happens to cost nothing. Full reasoning in "Where the app's answers enter the crate".
2. **On FDA Deny, both panes stay on `~`**, and background phases skip TCC-restricted roots. The permission dialog fires
   when the user navigates somewhere protected, which is the only moment it has a cause they can see. Moving the right
   pane to `~/Downloads` would buy the same prompt a few seconds earlier and re-shuffle the panes behind the onboarding
   sheet.
3. **`Fresh` means "fully covered for the scope the user chose"**: home with the scope toggle off (the default), the
   whole drive with it on. Honest in both modes, and on the default it comes alive minutes into the first run.
4. **Branch watching stays on when drive indexing is off, on macOS only**, and branches absorb their descendants.
5. **"Rescan now" re-walks in place**, keeping sizes visible; during the phased period it restarts the phases.
6. **New setting: "Index stuff outside my home folder", default OFF.** `index_outside_home` on `IndexConfig`. "Fully
   covered" becomes "fully covered for the chosen scope", which is also what stamps completion and freshness. Flipping
   it on adds frontier; flipping it off leaves rows we stop maintaining. Both fall out of coverage being add-only, so
   neither needs a migration. `~/Library` is in scope but never a priority root (see M1).
7. **One worktree, one effort, for the indexing work.** The milestones below are an execution order, ❌ not shippable
   slices, so ordering still matters but "is this milestone independently releasable" doesn't. The first-run startup
   state was the exception: it turned out to be self-contained, so it shipped on its own ahead of the indexing work.

Remaining assumption to confirm during execution, ❌ not a blocker: `~/Library` in scope but de-prioritized (M1).
