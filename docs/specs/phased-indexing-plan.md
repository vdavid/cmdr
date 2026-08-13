# Phased, priority-driven drive indexing

Status: plan, not started (M1 onward). Branch: `worktree-phased-indexing`. Revised 2026-08-14 after a review round that
took the home-only default seriously; the decisions list records what changed and why.

Turn first-run drive indexing from "one full scan of `/`, all or nothing, a few minutes before anything is useful" into
"the folders this user actually cares about are indexed within seconds, and the rest of their home fills in behind
them". Nothing is truncated on the way, so every second spent walking stays bought.

**The default is home-only, and that is a product change as much as a performance one.** By default the index holds
`$HOME`, the priority roots, and whatever the user opens. The whole drive is one setting away, and existing installs
that already scanned it keep it (decision 11). Everything below is written for the default: a volume that is
permanently, honestly partial. The single most common way to get this plan wrong is to reason about the whole-drive end
state and ship a mechanism that only fires there.

**Be precise about which half of the product this costs, because the obvious answer is wrong.** Search is
coverage-gated, and a search over uncovered ground already walks it: a drive converges toward instant through use, and
search keeps finding files either way, only slower the first time. What loses ground is **the size column**, which has
no equivalent fallback — outside the promised scope it renders `<dir>` and stays that way. So the affordance work in M5
belongs in the pane first and the search dialog second, which is the reverse of where it looks like it belongs.

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
4. Home finishes, and that's "done" by default: the badge goes green, and photo search and folder importance start. The
   rest of the drive follows only if they turn on "Index the whole drive".
5. At every moment the sizes shown are honest: exact where covered, `<dir>` or `≥` where not, an hourglass on ground
   being walked right now. Where the answer is permanently `<dir>` because the folder is outside what they asked us to
   index, the app says so and offers to fix it, rather than looking broken.

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

### The two scopes and the two markers

**Read this before anything else below.** The default is home-only (decision 6), so on a normal Mac this index will
_never_ hold the whole volume. Every existing mechanism that asks "is this index complete?" was written when the only
two answers were "a full walk finished" and "no full walk finished", and handing it a permanently-partial volume breaks
it in a different way each time. Four definitions have to stop being one, and they need names that can't be confused
with each other:

1. **The watched scope** — what this index currently answers for, moment to moment. It is the volume's `WatchScope`. ❌
   **Not "whatever is in the branch set"**: `start_scan` calls `branches::clear` before a whole-volume walk
   (`manager/start.rs:441`) and `finish_branch_coverage` uses `AfterWalk::Forget` while `branch_watched` is false
   (`:275-283`), so a legacy scanned volume is `WatchScope::WholeVolume(<empty set>)` (`scan_completion.rs:164`). Read
   literally, "the branch set is the scope" would make a legacy volume answer for **nothing**: rescan walks nothing, the
   sweep walks nothing, and completion fires instantly. **So the accessor branches on the VARIANT**:
   `WholeVolume ⇒ [volume_root]`, `Branches(set) ⇒ set`. One function — call it `covered_roots(volume_id)` — and every
   consumer below goes through it. **Test-first**, because getting it wrong is silent and total.
2. **The promised scope** — what we told the user we'd hold, and therefore what completion measures. `$HOME` plus the
   priority roots on the boot volume, plus the volume root when the whole-drive toggle is on. It is **persisted
   alongside its marker**, because it moves: the priority roots come from editable favorites and last session's tabs, so
   adding a favorite outside home grows the promise. The generic rule is **"the promised scope changed ⇒ un-stamp and
   re-derive the frontier"**, and the toggle-on case falls out of it instead of being a hand-written special case.
3. **`promised_scope_covered_at`** (new meta key) — the promised scope is covered. This is what freshness, the
   media/importance kick, the per-drive badge, the completion UI, and "the phases are done" read.
4. **`scan_completed_at`** — keeps **today's exact meaning**: a full walk finished and the whole tree has rows. Only the
   volume-root phase completing stamps it. With the default setting it is never stamped, and that is correct.

**Any volume that isn't the boot volume promises its whole root.** An external disk holds no `$HOME` and usually no
priority root, so a promised scope of "home plus favorites" would be **empty** — which stamps
`promised_scope_covered_at` immediately, turns the badge green, and reports `Fresh` over zero rows, on a drive that
today gets scanned when you enable it. Turning indexing on for a drive IS the request, so its promised scope is its
root, and `index_whole_drive` governs the boot volume only. ❌ Don't leave this to fall out of the generic rule; state
it and test it.

**The machine stays resident and idle after the promise is covered.** A folder the user opens gets covered wherever it
lives (decision 10), and that promise is worth nothing if the machine has exited by the time they open it. So scope
completion stops the _phases_, ❌ not the machine: it stays subscribed to `open_listings`, and a visit to uncovered
ground queues a rank-1 walk. A rank-1 walk ❌ never re-opens `promised_scope_covered_at` (it was never promised) and ❌
never un-`Fresh`es the volume. Without this, "anything you open gets indexed" is true for the first few minutes of the
app's life and false forever after, which is worse than not promising it at all.

**Why not one marker.** The first draft stamped `scan_completed_at` at scope coverage, which reaches three consumers
that all mean "the whole tree has rows":

- **`local_rescan_reconciles`** (`lifecycle/manager.rs:193`, literally `entry_count > 1 && prior_scan_completed`) flips
  to true, so "Rescan now" runs the serial per-directory reconcile with an add-everything delta over the ~70% of `/` we
  never covered. Its own doc comment says reconcile is right "only when the index is substantially complete", because a
  4%-complete partial made the app look hung for ~15 minutes. That failure would arrive **by design** instead of by
  accident.
- **`should_replay_journal`** (`manager.rs:435-440`) gates whole-volume FSEvents replay on it, and the phased answer
  sits _below_ that branch. So after the first home completion, every later launch takes whole-volume replay and the
  phase machine never runs again: **flipping the scope toggle on after a relaunch would silently do nothing.**
- **`get_status.scan_completed_at`** feeds the per-drive footer and MCP `cmdr://indexing`, whose entire job is answering
  "can I trust search on this volume?". "Fully scanned" while `/opt` has no rows is exactly the dishonesty this plan
  refuses everywhere else.

**The rule that falls out: the watched scope is the rescan scope.** Wherever today's code asks "is the whole volume
complete?" to decide how to re-walk, the phased world asks `covered_roots(volume_id)` and re-walks those. Three places
consult it — rescan routing (M3.3), sweep routing (the watching section), and completion (M2.8) — and because the
variant-based accessor answers `["/"]` for a legacy `WholeVolume` volume, none of them changes behavior on an index
built before this plan.

⚠️ **The branch set's persistence is currently coupled to `branch_watched`**, so if `DriveWatcher::start_branches` fails
(non-fatal, logged, `manager/start.rs:206-211`) the set silently becomes empty for the rest of the session and persists
nothing. Under this plan that is the covered scope evaporating. Decouple persisting the set from whether a watcher came
up, and treat a failed watcher as "covered but unwatched" (the epoch bump already makes that honest).

**A phased volume never journal-replays through `should_replay_journal`**, since its `scan_completed_at` is absent. Its
covered ground is replayed by `resume_branch_watch` instead, which replays from the volume's last event id and bumps the
epoch when it can't. ⚠️ Confirm during M3 that the branch resume covers what `start_replay` does for the ledger heal
(`heal_pending` is threaded into `start_replay`); if it doesn't, the phased answer owes that call itself.

**⚠️ That also loses journal-gap recovery, and the result is a drive that looks permanently stale and never heals.**
Today a gap wider than the threshold routes to `start_scan("stale index: journal gap too large")`
(`manager.rs:448-462`). A phased volume never reaches that branch; its resume is `ensure_branch_watch(true)`, which on
`since_event_id == 0` only sends `BumpCurrentEpoch` (`manager/start.rs:183-191`). After the bump every covered directory
has `min_subtree_epoch < current_epoch` (so it renders stale / `≥`), the frontier is empty (everything is listed), and
`promised_scope_covered_at` is stamped — **so nothing re-walks and nothing re-stamps, forever.** The phased answer needs
its own arm: **a gap too wide ⇒ reconcile the watched scope in place** (the same M3.3 arm), which re-stamps epochs as it
goes. Come back from a two-week absence and the drive heals instead of showing `≥` on every folder.

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
directory's own `listed_epoch` and its children's (`writer/repair.rs:47`, `:98-135` → `store/dir_stats.rs:217-246`). No
coverage is claimed that wasn't earned, and sizes stay honest.

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

**Decided: the bail, ❌ not the mark.** The alternative was to have the verifier MARK a directory listed when it
genuinely read all of it, turning it from a producer of `NotVirgin` nodes into a producer of legitimately covered ones.
That is more useful (browsed folders would stop needing a walk at all) and strictly more work to get right, and the two
are **mutually exclusive** — a verifier that both bails and marks is incoherent. Build the bail. The mark is an M6
candidate, ❌ never a thing to slip into M2 because it looked like an easy win. (An earlier draft of this plan said
"bail" here and "mark" in its own risk table; if you find both again, the bail wins.)

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

**⚠️ `phase_active` alone does NOT close the two-writer hazard, because a search-driven cover walk sets neither it nor
`mgr.scanning`.** A coalesced shallow anchor can `TruncateData` under a live search walk today; this plan just makes it
routine. Gate `start_scan` on **`cover::ground_being_walked(volume_id)` being non-empty as well as `phase_active`**.
That is a pre-existing bug worth fixing on its own merits, and it makes the guard correct for both walk kinds instead of
only ours.

**⚠️ `begin_branch_coverage` / `finish_branch_coverage` silently no-op unless the manager is `Running`.** Both go
through `with_running_manager` (`lifecycle/state.rs:275-294`), and both `force_scan` and `perform_registry_rescan`
`mem::replace` the phase with `ShuttingDown` for the whole duration of `start_scan` (`state/scan_control.rs:82-94`,
`manager.rs:252-274`). A cover walk that _ends_ inside that window never decrements `walks`, so its branch stays
`walks > 0` **forever**: `may_walk` is false for that ground permanently, every event for it buffers and is never
promoted, and it is never absorbed. Latent today because walks are rare; routine under this plan. Fix by making finish
idempotent and independent of the registry phase, ❌ not by hoping the window is short. Test it.

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

**⚠️ But the enable-button door must NOT be keyed on that marker alone.** Stop the phases from the badge menu before the
promise is covered, and neither the marker nor `promised_scope_covered_at` holds — so `awaits_its_first_scan` answers
"never walked", `start_volume` force-scans, and everything the phases built is truncated. **Key the door on "this volume
has covered ground": `covered_roots(volume_id)` is non-empty.** That is true after the first walk, stays true across a
user stop and a relaunch, and is false on the search-walked drive the predicate was written to serve only if no walk
ever ran, which is exactly right.

**And a second marker is needed, distinct from this one: a durable "this index was built by the phases" stamp**, set at
the first stitch and **never cleared**. M3.6 truncates a legacy partial and preserves a phased partial, and both shapes
have rows and no `scan_completed_at`, so without a format stamp a cleanly stopped phased index looks legacy and gets
truncated on every launch. ❌ Don't reuse the in-progress marker for this; one says "running", the other says "built
this way".

### Interleaving without preemption

Because the stitch turns `$HOME` and `/` into a list of independent frontier roots, the machine walks them **one root at
a time and checks the priority queue between roots** — one `cover()` call per root. A folder the user opens waits for
one subtree, not for the drive. **This is the mechanism; there is no second candidate.**

❌ Don't "save the per-root meta write" by handing one `cover()` call a phase's whole frontier. It looks cheaper (a
single `cover()` already checks its cancel token between frontier roots, `lifecycle/cover.rs:326-330`), but that check
is _inside_ `cover`, so the machine gets no point at which to consult the priority queue. It buys a persisted write per
root and gives up the one property the plan exists for. If the benchmark shows `finish_branch_coverage`'s write is
material, batch roots into small groups — ❌ never into one.

Preemption (cancel the running walk, run the visited root, resume) is **out of scope**. It is expensive and subtle, and
one-root-at-a-time already bounds the wait. If a real measurement later says a single big root starves the queue, these
are the traps to reopen it with:

- the `Claim` is released by the walk thread on exit (`lifecycle/cover/live.rs:104-117`), so cancel-then-immediately-
  start makes the new walk defer the same ground and cover **nothing** while reporting `roots_covered: 0`. The machine
  MUST `CoverWalk::finish()` (join) before starting the next walk, and MUST treat a non-empty
  `covered_by_another_walk()` as "this phase did not run";
- cancel latency is a watchdog tick plus up to `LOCAL_LIST_TIMEOUT` (15 s) on a parked read, so any debounce must be at
  least the join, not the 1 s the UI uses.

The join rule is **not** optional even without preemption: the machine starts a walk only after the previous
`CoverWalk::finish()` returns, and treats a non-empty `covered_by_another_walk()` as "this root did not run".

**Two honest caveats on "one walk at a time" and "waits for one subtree, not for the drive".**

- **Search walks are not ours to serialize.** Live search calls `Index::cover` on the user's behalf
  (`search/execute/live_run.rs`), deliberately carved out of both indexing switches, and only _overlapping_ ground is
  deferred by the `Claim` (`cover/live.rs:62-91`). Disjoint ground runs a second parallel walker against the same disk
  and writer. That is correct — a search somebody typed outranks background phasing — but the plan may not claim a
  single-walker invariant. Two consequences: the machine must tolerate a concurrent walk (it already must, via
  `covered_by_another_walk()`), and the benchmark's browsing arm must include a search.
- **A frontier root can be huge.** The wait is bounded by the largest child of the phase root, not by a small subtree;
  `~/Library` or the 1.14M-file Google Drive temp dir is one root. State the bound honestly rather than promising
  seconds.

### What a full scan does that cover walks don't (and what we owe each one)

Audited end to end against `manager/start.rs::start_scan` + `lifecycle/scan_completion.rs`:

- **`scan_completed_at`** — the phase machine stamps it only when the **volume root** is fully covered, which with the
  default setting never happens. `promised_scope_covered_at` is the marker that fires at the end of the normal first
  run. See "The covered scope" above; getting these two the wrong way round is the single most damaging mistake
  available here.
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

**⚠️ NOT already handled, and this one is fatal on its own: the database is never prepared for a walk.**
`prepare_database_for_a_walk` — the ROOT sentinel, epoch seeding, `volume_path` meta, and the `EXCLUSION_POLICY_KEY`
stamp (`state/walk_database.rs:99-121`) — runs **only** when `activation == Activation::WriterOnly`
(`state/startup.rs:165-167`), and this plan mandates `IndexTheVolume`. The only other writer of the stamp for a local
volume is `start_scan`'s non-reconcile branch (`manager/start.rs:429-430`), which the phase machine never calls.

An absent stamp makes `index_predates_exclusion_policy` answer **true** (`scanner/exclusions.rs:350-353`, "an absent
stamp answers yes"), and `walk_coverage` then short-circuits every query to `Frontier` over the whole scope
(`read/coverage.rs:314-317`). On a fresh phased install that means: the frontier never shrinks, so nothing ever
converges, `promised_scope_covered_at` never fires, and after the first walk every root is non-virgin and takes the
serial `repair_non_virgin`. It reproduces, exactly, the failure the stitch exists to prevent — and it would look like
the stitch not working. `volume_path` meta going unwritten also breaks offline external reads.

**So the phased start owes `prepare_database_for_a_walk`'s work itself.** Two constraints: the stamp is legal only on a
provably empty DB or right after a `TruncateData` (`exclusion_policy_stamp_message`'s own rule), which is satisfied by
the M3.6 upgrade truncate and by a first run; and `SYSTEM_DIR_EXCLUDES` plus the exclusion policy apply identically to
every walk. **Pin it with a test that a fresh phased volume's frontier actually shrinks after one walk** — without it,
every other test in M2 can pass while the product never converges.

One real behavioral difference between the walk kinds: `ScanRoot::Virgin` pins the walk root's **device** while
`ScanRoot::Volume` bounds by path prefix, so the `/` phase cuts at mounted filesystems rather than at `/Volumes/`. A
device cut writes no row, so it can't leave a permanent frontier node, and firmlinked system paths share one device
(`/`, `/System/Volumes/Data`, `/Users`, `/Applications`, `/System` all report dev=16777231; verified on macOS 26.5.2
build 25F84 via `stat -f %d`, 2026-08-14). Acceptable, but it means the `/` phase indexes a slightly different set than
today's scan.

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

So a volume that covers everything it promised can simply keep `WatchScope::Branches`, and the branch→whole-volume
handover never has to be written. On Linux a `/` branch is watched recursively, which is the same cost as whole-volume
watching. **Prefer this**, with one required change and one bonus:

- **Required: the sweep has to become scope-aware, because with the default setting the branch set is never `["/"]`.** A
  `Branches` scope never takes the visible-scanner route for a `MustScanSubDirs` anchor, whatever its depth
  (`reconcile/reconciler.rs:369-374`, `:517-527`). So a branch-confined volume **never sweeps again**: every coalesced
  root-scale anchor (macOS saying "a lot changed under here" and losing the detail) goes to the throttled
  `reconcile_subtree` drain on a shallow anchor, which is exactly the "holds the per-dir hourglass for the better part
  of a full scan" case the depth split exists to avoid, and the sweep-window bookkeeping accumulates with no sweep.

  ⚠️ **The obvious one-liner is wrong under the default setting.** Making `is_branch_confined()` false when the branch
  set covers the volume root (`WatchScope::branches().covers(volume_root)`, `branches.rs:481-485`) only fires on a
  volume that covered all of `/`, which by default never happens. It would leave the sweep permanently dead on exactly
  the configuration every user runs.

  **The fix is the scope rule: the visible-scanner (sweep) route walks the covered scope, not the volume.** A
  shallow/root-scale anchor takes the visible-scanner route as it does today; what changes is where that route lands.
  Today it is `route_shallow_to_scanner` → `perform_registry_rescan` → a whole-volume `start_scan`
  (`reconciler/rescan.rs:119-128`). It becomes **the same branch-anchored rescan arm M3.3 defines** — one
  implementation, not two, and `/` for a legacy `WholeVolume` volume, so its behavior is byte-identical.
  `is_branch_confined()` then stops being a routing question at all and the special case disappears. Two things this
  must not lose: the sweep may only run once `promised_scope_covered_at` is stamped (a sweep over a scope still being
  covered fights the phase machine), and it must **reconcile in place**, ❌ never truncate.

  The sweep window's seed, `max(shallow_sweep_at, scan_completed_at)` (`manager.rs:409-419`), loses its second term on a
  default install. That is why M2.8 step 5 writes `SHALLOW_SWEEP_AT_KEY` at scope completion: without it the window
  reads permanently expired and every launch hands the next shallow anchor a free sweep.

  _Fallback if scoping the sweep route turns out to be genuinely hard_: leave `is_branch_confined()` alone and accept no
  sweep under `Branches`, but then the shallow bookkeeping (`SHALLOW_SWEEP_AT_KEY`, `SHALLOW_COALESCED_KEY`) must stop
  accumulating, or the counters grow forever against a sweep that can't happen. Say which one you built.

- **Bonus, while the volume IS branch-confined**: truncate door (d) in M3 is closed by construction, and so is a door
  the stitch would otherwise open — the stitch creates depth-1 and depth-2 branches, and `SHALLOW_RESCAN_MAX_DEPTH = 2`
  would send those to `perform_registry_rescan` → a truncating `start_scan` if the scope were `WholeVolume`.
- Verified for the single-`/`-branch shape (the toggle-on end state): `may_walk` (`covers("/")` is true), `admit` (a `/`
  branch is `deepest_containing` for every path ⇒ `Process`), and the re-anchoring arm is unreachable with nothing above
  `/`.

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
finishes). Every collapse below is then just this one rule firing, with no special case for full coverage.

**The branch set needs an explicit collapse.** `begin_covering` pushes one `Branch` per path, so N frontier roots means
N branches (this is inherent to the stitch, not to interleaving: it's the same count whether one `cover()` takes N roots
or N calls take one each). Every event then pays an O(branches) scan in `deepest_containing` on the live hot path, and
`finish_covering` only absorbs the path being finished into an existing ancestor branch — siblings never absorb each
other, so the set never collapses on its own. Expect roughly 50–150 entries during the phases (children of `/`, of
`$HOME`, and the priority roots).

**Collapse at the end of each phase, to that phase's root** — that is the general rule, and it works whatever the scope
setting is. When the home phase finishes, everything under `$HOME` absorbs into a single `$HOME` branch; the steady
state on a default install is `$HOME` plus any outside-home priority or visited roots, so well under 30 entries on the
`deepest_containing` hot path. ❌ Don't make the collapse conditional on full-volume coverage: with the default setting
that moment never comes, and the set would stay at 50–150 forever. The `["/"]` end state is then just what the same rule
produces when the volume-root phase finishes with the toggle on — but ❌ **not via `branches::clear` plus a begin/finish
pair.** `clear` calls `forget`, which drops the map entry, while the live loop and its reconciler each hold their own
`Arc<BranchWatch>` captured at `ensure_branch_watch`; `live_for` would then mint a **brand-new** `BranchWatch` that
nothing is reading. The persisted meta would say the collapsed set while the running loop kept filtering against the
stale N-entry set for the rest of the session — and every scope question above (sweep routing, rescan anchoring) would
read that same stale Arc and answer for the wrong ground until the next launch. Silent, and hard to notice. (The
existing `clear` call in `start_scan` is safe only because the loop is torn down and replaced in the same breath.)
Instead add a crate-internal `collapse_to(root)` that mutates the **shared** `BranchWatch` in place — replace the
covered descendants with a single `Branch` at `root`, leave any `walks > 0` entry alone, then `persist()`. Measure
`deepest_containing` under a churn burst to confirm the mid-phase set is not itself a problem.

### Freshness, and the two subsystems that depend on it

**This is the decision with the largest product blast radius, and the first draft of this plan missed it entirely.**

Folder importance and the whole media index (OCR, Vision tags, CLIP embeddings — that is photo search) start their
passes off `Freshness::Fresh` plus a `ScanCompleted` publish on the lifecycle bus
(`state/queries.rs::ready_volumes_with_kind` filters on `Fresh`; the bus publish fires only on
`FreshnessEvent::ScanCompleted`, `state/freshness_bridge.rs:95-98`, which today only `scan_completion.rs:351-358`
fires). If nothing fires until the final `/` phase ends, **photo search and importance scoring are dead for the entire
phased period, and forever on a machine that never finishes `/`.**

**Decision: `Fresh` means "fully covered for the scope the user chose", so it stays honest without stalling.**
Concretely: with the whole-drive toggle **off** (the default), the volume goes `Fresh` when the promised scope (`$HOME`
plus the priority roots) is fully covered; with it **on**, only when the whole drive is. Freshness then never claims
more than the user asked us to hold, the badge is truthful in both modes, and on the default setting importance and
photo search come alive minutes into the first run rather than never.

Freshness fires off **`promised_scope_covered_at`**, ❌ never `scan_completed_at` — see "The covered scope". Scope
completion and whole-volume completion are one concept only on a toggle-on machine that finished; everywhere else they
must stay apart.

Audited, and it is safe: **search never reads freshness at all** (it goes through `coverage()` / `cover()`, so it is
coverage-gated by construction); `Index::is_fresh` has exactly one app caller
(`file_system/write_operations/journal_search.rs:102`), which applies its own coverage gate
(`min_subtree_epoch > 0 && == current_epoch`) and downgrades to `index_stale` otherwise — **pin that with a test, since
this plan now leans on it**; and the `ready_volumes_with_kind` consumers work over whatever the index holds, with the
later full-coverage `ScanCompleted` retriggering a full recompute.

Three consequences of that decision:

- **With the toggle ON, the drive stays un-`Fresh` for a long time**, and importance plus photo search wait it out. That
  is the user having asked for the bigger job; the badge and the copy should say so.
- **With the toggle OFF (the default), the media index starts when home completes**, which is the right moment: the
  walker is finished with everything the user asked for, so OCR / Vision / CLIP enrichment is not competing with it.
  This is what makes the scope decision and the freshness decision one decision rather than two.
- `enqueue_initial_full_pass_if_unscored` only scores a volume whose importance store has no generation yet. Once the
  early pass stamps one, a later launch with coverage still incomplete re-scores nothing and no `ScanCompleted` fires
  until scope coverage, so the importance ranking sits frozen at the priority-phase snapshot across launches (softened,
  not fixed, by the incremental `record_visit` / `publish_dirs_changed` paths). On the default setting that window is
  small, because scope coverage arrives when home completes.

**Flipping the toggle ON un-`Fresh`es a volume that was `Fresh`**, because the promised scope just grew. That is correct
and it must be explicit: the badge goes back to incomplete, and photo search plus importance keep serving what they
already computed (they work over whatever the index holds) rather than being torn down. ❌ Don't clear the media index
or the importance generation on a scope change; the later `ScanCompleted` retriggers a full recompute.

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

1. **Scope** (the whole-drive toggle) is a field on `IndexConfig` — `index_whole_drive: bool`, applied through the
   existing `set_config`, exactly as the media policy is. It is a stored setting, which is what that struct is for, and
   it re-applies **live** when the user flips it: turning it on adds frontier (coverage is add-only) and un-`Fresh`es
   the volume, turning it off simply stops maintaining rows we already hold. A plain `bool` rather than a `DriveScope`
   enum because a new public type would breach the ceilings below; if it ever grows a third value, that is the moment to
   argue for the enum.

   **What the toggle governs is BACKGROUND coverage of ground nobody asked for, ❌ not "never index outside home".** A
   folder the user actually opens gets indexed either way (rank 1, below): opening a folder is the strongest possible
   statement that they want it, and a file manager whose size column stays `<dir>` in a folder you are standing in reads
   as broken, not as respectful. So the setting's name must not promise otherwise — hence `index_whole_drive`, ❌ not
   `index_outside_home`. Draft copy in M5.0.

   **Visited roots don't gate scope completion.** The promised scope is `$HOME` plus the priority roots (plus the volume
   root when the toggle is on); a root the user wandered into is covered opportunistically and added to the branch set,
   but it never holds `promised_scope_covered_at` open. Otherwise a user who opens `/usr` during the first run defers
   freshness, photo search, and importance indefinitely.

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
TYPE on an event or a new `pub fn` on a public type would each breach immediately. New `IndexEvent` variants need doc
comments (`#![deny(missing_docs)]`) and a regenerated `bindings.ts`; `UnreadableCause` isn't re-exported from `lib.rs`
and doesn't cross the bindings, so `Abandoned` needs the doc comment only.

⚠️ **Re-measure before accepting "an enum is impossible".** `pub mod indexing` is private with
`pub use indexing::{host, store}`, so the walker may not descend into `host/`, which would make an enum in
`host/config.rs` free — and `MediaConfig` already carries an `IndexScope` enum as precedent. The measurement is one
command (`pnpm check index-isolation -v`), so settle it by adding the enum and running it, ❌ not by argument. The
`bool` is fine for two values; it will read badly the day per-folder scopes land (already M6.2).

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

M1 touches nothing M2 depends on, so it can run alongside M2's benchmark if an agent is idle. Everything after M2 is
strictly sequential. One ordering constraint that bites: **M4's unit tests stand alone, but its end-to-end assertion
can't run until M3 lands**, because the surfaces it fixes only misbehave once the phase machine is real.

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

Then the machine appends the volume root as the final phase, **only when `index_whole_drive` is on** (see the scope
decision below).

These five plus the volume root are the **promised scope**. Roots the user visits while the machine runs are covered too
(rank 1 in M2.3), but they are not part of it: they never hold `promised_scope_covered_at` open, and they are not
recomputed here.

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
`DETAILS.md`.

**Record time-to-value, not only time-to-full — the gate is meaningless without it.** Full coverage is the cost side of
this trade; the benefit is that `~/Downloads` is usable in seconds. A benchmark that reports only wall clock to full
coverage could pass at 1.4× having never measured the thing the plan is for, or fail at 1.6× while hiding a 4-minutes-
to-3-seconds win nobody would give up. So capture, per arm:

- **a coverage timestamp per priority root**, and for `$HOME` (arm (a) reaches all of them only at the end, which is the
  point);
- **wall clock to full coverage, and peak RSS**;
- **a third arm: (b) under browsing**, driving `open_listings` through a handful of folders mid-run. Interleaving is the
  mechanism, and its cost is invisible in a quiet benchmark. This is also where the M1 high-churn risk shows up.

Also measure **`~/Library`'s share of home-coverage wall clock**. It sits inside the default promised scope, it holds
DerivedData, container images, and the 1.14M-file Google Drive temp directory (risk 8), and `promised_scope_covered_at`
— which gates photo search, importance, the green badge, and `PayLedgerIfUnpaid` — waits for all of it. If it dominates,
the answer is to drop `~/Library` out of the promised scope (still reachable by rank-1 visits and by search walks), ❌
not to weaken the completion rule.

**Gate: if (b) is more than roughly 1.5× (a) to full coverage, stop and re-decide with David** — with the
time-to-first-root numbers in hand, because they are what the decision is actually about.

**The gate runs on a throwaway harness, ❌ not on M2's deliverable.** Arm (b) needs only the stitch, a hardcoded root
list, and a loop: no queue, no completion rule, no status plumbing, no `Abandoned` cause. Otherwise the milestone is
gated on itself, and "M1 can run alongside M2's benchmark" is false because the benchmark would need M1's roots. Keep
the harness disposable and say so, so nobody grows it into the machine.

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

   **Rank 1 is NOT gated by `index_whole_drive`.** A folder the user opened gets covered wherever it lives; the toggle
   governs background coverage of ground nobody asked for. Rank 3 is the only rank the toggle gates. Ranks 0, 2, and 3
   are the promised scope that `promised_scope_covered_at` answers for; rank 1 is covered opportunistically and ❌ never
   holds completion open (M1, "Where the app's answers enter the crate").

4. **Each phase step**: `coverage(volume_id, root, Listing)` for the frontier; empty ⇒ skip; otherwise walk its roots
   one at a time. The walk marks, aggregates, and claims its own ground.
5. **Visits enter through `HostPolicy::open_listings()`**, with a small recently-seen set so a folder the user opened
   and left is still queued. ❌ Not through `Index::verify_directory` (too loose: the opposite pane, MCP listings, and
   refreshes all fire it) and ❌ not by widening any handle method.

   **Rate-limit the poll to ≥500 ms, independent of root boundaries.** The seam's contract is explicit: it allocates and
   "it's asked on the scan-progress reporter's 500 ms tick. ❌ Not from anything faster" (`host/policy.rs:93-100`).
   "Between frontier roots" is not automatically within that — the stitch deliberately produces 50–150 roots and many
   finish in milliseconds. Poll on a timer, consult the cached answer at root boundaries.

6. **One root, one `cover()` call, join before the next.** Preemption is out of scope (see "Interleaving without
   preemption"); the join rule still applies.
7. **Completion is derived, not remembered — but "empty frontier" alone is not a terminating rule.** `abandoned_ground`
   is per-walk and in-memory, so it can't answer "was anything abandoned in a previous session?"; the durable signal is
   that an abandoned directory is never marked listed, so it re-enters the frontier.

   **Two completions, same rule, different scope.** `promised_scope_covered_at` when the frontier of the **promised
   scope** is empty (the union of the M1 roots and `$HOME`, plus the volume root when the toggle is on);
   `scan_completed_at` when the frontier of the **volume root** is empty. On a default install the second never fires,
   and no step below may assume it does. Evaluate both after every root finishes, ❌ not only at the end of a phase:
   with the toggle off, the last root of the home phase is the moment the product has been waiting for.

   **The trap**: a directory the walker timed out on gets **no `unreadable_cause`** — deliberately, "since mounts heal"
   (only denied ids carry a marker, `scanner/mod.rs:881`). So it stays `Frontier` forever, "the frontier is empty except
   for `permission_denied` / `declined`" can never become true, and _everything_ hanging off completion never happens:
   the stamps, `PayLedgerIfUnpaid`, the sweep keys, the branch collapse, the media kick, freshness. Every launch
   re-walks it, times out again at 15 s a directory, and stalls in the same place. One wedged directory anywhere under
   `$HOME` is enough to kill the whole first-run payoff, so this rule is load-bearing on the default setting, not an
   edge case for exotic drives.

   **The fix is a third `UnreadableCause`, not a pass counter.** Give the walk `UnreadableCause::Abandoned` for a
   directory it gave up on, and completion goes back to being a pure function of the database — "frontier empty, only
   unreadable causes left" — durable across relaunch, immune to churn, with no in-session bookkeeping. The machinery
   already fits:
   - `UnreadableCause` is `Denied = 1` / `Declined = 2` and `from_stored` falls back to `Denied` for anything unknown
     (`store/errors.rs:19-49`), so `Abandoned = 3` is additive and an older build reading a newer DB degrades without
     crashing. ⚠️ It degrades to "permission denied", so a downgraded build would tell the user to grant Full Disk
     Access for a timed-out mount. Acceptable for a disposable cache (no migration), worth knowing before someone calls
     it truthful.
   - `MarkDirsUnreadable { ids, cause }` already exists, and **`MarkDirsListed` clears the cause**
     (`writer/mod.rs:332-342`), so it self-heals on the next successful listing with no rebuild — the same contract
     `Denied` already relies on.
   - The verdict match (`read/coverage.rs:277-282`) is exhaustive over the two variants, so a third one is a **compile
     error** at exactly the place that must grow a bucket. The decision can't be silently skipped.
   - Free under the surface ceilings: neither a new enum variant nor a new `CoverageMap` field is counted.

   **⚠️ The timeout is only half of it. The consecutive-failure budget prunes queued siblings UNREAD**, and a pruned
   task never reaches the visitor, so it gets no id, no mark, and stays `Frontier` forever: `run_worker`'s pre-read
   check is `if scheduled.budget.is_given_up() { self.complete_one(); continue; }` (`scanner/walker/engine.rs:341-347`).
   That is precisely the dead-mount case the rule exists for, so completion would still never fire. **Mark the pruned
   tasks `Abandoned` too** — the id is on `scheduled.task` — or the rule is incomplete in exactly its motivating
   scenario. The watchdog-timeout half does reach `visit_read_error(.., TimedOut)` with the id in hand
   (`scanner/insert_visitor.rs:405-427`), so that half is straightforward.

   **The tradeoff, stated rather than hidden:** marking a timeout `Abandoned` takes it out of the frontier, so nothing
   re-attempts it, and with the verifier now bailing on `listed_epoch == 0` its heal path is narrower than `Denied`'s.
   For an external disk that was merely spinning up, that is pessimistic.

   **How it heals, decided rather than left as two half-rules.** ❌ Not "frontier-eligible again in a new session": that
   is the same 15 s per wedged directory per launch the pass-counter rule was rejected for, and it would put process
   state inside `walk_coverage`, a pure DB descent that live search shares — so coverage answers would become
   session-dependent. Instead: **`Abandoned` rows are retried by an explicit, bounded re-attempt** — a launch-time write
   that clears the `Abandoned` cause on a bounded number of rows (leaving `Denied` / `Declined` alone), plus the same
   clear on the maintenance timer and on a user visit to that folder. Coverage stays a pure function of the database,
   the retry is a write like every other heal, and a long-running app (which this is) heals without a relaunch.

   ❌ **Don't use a "frontier didn't shrink across two passes" rule instead.** It has to compare sets rather than counts
   (a pass can legitimately grow the frontier by listing a root and exposing the abandoned directories inside it), it
   never terminates on a continuously-written drive (a build or a sync client produces new unlisted rows every pass —
   see risk 8), and being session-scoped it re-pays a full re-walk plus 15 s per wedged directory on every launch.

   ⚠️ This is still the newest part of the plan. Keep the test
   `a_permanently_timing_out_directory_still_lets_completion_happen`, and nail the details down in M2.

8. **On scope completion, in this ORDER — and the order is enforced by a FLUSH, not by the numbering.** Steps 1–6 are
   writer _messages_; step 7 is in-process state. The read the whole ordering protects (the rescan routing's
   `get_index_status()` inside `start_scan`) goes through a read connection, so it sees a stamp only once the writer has
   committed it — and step 3 can run a full `ComputeAllAggregates` over a large index, which is minutes of writer-thread
   work sitting between the stamp being queued and being visible. **Flush after step 1 and before step 7**, or the
   collapse lands inside exactly the window the order exists to close. Use the shape that matches the context:
   `writer.flush().await` from async (as `scan_completion.rs:228` does), or
   `tokio::task::block_in_place(|| writer.flush_blocking())` from a sync path in an async context (as
   `manager/start.rs:432` does). ❌ A bare `flush_blocking()` blocks a runtime worker; it is only safe on a plain
   `std::thread`, which is why the cover walk's own call is fine.
   1. stamp `promised_scope_covered_at` — **and `scan_completed_at` too, but only if the promised scope was the volume
      root**;
   2. write the calibration meta;
   3. `PayLedgerIfUnpaid` (nothing else ever pays the armed `dir_stats` ledger heal);
   4. `BackfillMissingDirStats`;
   5. `reconciler::record_sweep_completed` plus the `SHALLOW_SWEEP_AT_KEY` / `SHALLOW_COALESCED_KEY` writes — without
      these the in-memory `SweepRecord` stays `None` for the session (it is seeded from meta only at launch), so the
      very first shallow anchor after completion triggers a full sweep nobody asked for;
   6. publish freshness (`FreshnessEvent::ScanCompleted`, which is what wakes photo search and importance) and fire the
      terminal events;
   7. **only then** collapse the branch set, per the absorption rule: to the phase root in every case, which is `["/"]`
      exactly when the volume-root phase is what finished. Collapse before the stamp and there is a window where the
      volume answers for wider ground than it is marked as having covered, and one shallow anchor in it re-walks or
      truncates the index the phases just built.

   ⚠️ **Step 3 is expensive and now fires on the common path.** With the toggle off, scope completion arrives minutes
   into the first run, so `PayLedgerIfUnpaid`'s full `ComputeAllAggregates` lands while the user is actively browsing
   rather than at the end of a long quiet scan. Measure it in the M2 benchmark's browsing arm; if it stalls the writer
   visibly, the answer is to defer it to the maintenance timer, ❌ not to skip it (an unpaid latch re-arms every launch
   and the heal never happens).

9. **Feed the live status shape** (`ScanCalibration`-equivalent counters) throughout, or the per-drive row, progress
   bar, and ETA stay dead for the whole first index. Drive `get_status`'s `scanning` field from the **`phase_active`
   flag**, ❌ never by setting `mgr.scanning` (that would make the machine's own `cover()` calls fail).

   **Decide the progress shape here rather than discovering it in M4.** A phased walk has no knowable total, and the
   design principles forbid a progress bar parked at 100% and require a distinct state when the quantifiable part ends.
   So: **phase label + live entry counter + elapsed, and ❌ no percentage until the volume-root phase**, which is the
   only one with a calibrated total. Say what `writer.set_expected_total_entries` gets meanwhile (the calibration
   estimate for the current root, or nothing) — it is the writer's flushing-progress denominator, so "nothing" is a real
   answer with a real consequence.

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
- **`a_fresh_phased_volume_s_frontier_shrinks_after_one_walk`** — the exclusion-policy stamp. Without it every other
  test here can pass while the product never converges. **Test-first.**
- **`covered_roots_answers_the_volume_root_for_a_legacy_whole_volume_index`** — the variant-based accessor. Silent and
  total if wrong. **Test-first.**
- **`a_walk_that_finishes_while_the_manager_is_shutting_down_still_releases_its_branch`** (the permanent `walks > 0`
  hazard).
- **`a_truncating_rescan_refuses_while_a_search_cover_walk_is_live`** — the two-writer hazard that `phase_active` alone
  does not close.
- **`the_branch_collapse_is_visible_to_the_running_live_loop`** (not just to the persisted meta).
- **`a_relaunch_with_no_replayable_journal_bumps_the_epoch`** — the resume-honesty property.
- **`completion_pays_the_ledger_and_seeds_the_sweep_keys`**.
- **`a_permanently_timing_out_directory_still_lets_completion_happen`**, and its sibling
  **`a_subtree_pruned_by_the_failure_budget_still_lets_completion_happen`** — the bounded-progress rule, whose failure
  mode is wide (it gates the stamps, the media kick, the collapse, and the sweep keys). **Test-first**, both.
- **`enabling_indexing_for_a_search_walked_drive_still_scans_it`** — the shipped behavior `awaits_its_first_scan`
  protects, which the truncate-door work must not regress.
- **`home_coverage_stamps_scope_but_not_scan_completed`** — the marker split, on the default setting. Its inverse,
  **`a_home_scoped_volume_never_journal_replays_the_whole_volume`**, is what keeps the toggle working after a relaunch.
  Both **test-first**: they are the load-bearing half of this revision.
- **`turning_the_whole_drive_toggle_on_queues_the_volume_root_phase`** after a relaunch, on a volume that already
  stamped `promised_scope_covered_at`. This is the one that silently did nothing under the single-marker design.
  **Test-first.**
- **`a_visited_root_outside_home_is_covered_with_the_toggle_off`**, **`it_does_not_hold_scope_completion_open`**, and
  **`a_visit_after_scope_completion_still_gets_walked`** (the resident-machine rule: without it the promise in the
  settings copy is false for the entire rest of the app's life).
- **`growing_the_priority_roots_un_stamps_the_promise`** — the persisted promised scope.
- `phases_run_in_order`, and a covered root is skipped without a walk.
- `a_visited_root_is_taken_between_frontier_roots` without cancelling anything.
- `rows_survive_a_stopped_and_restarted_machine` (row count only grows), and the restart joins before starting.
- `scope_coverage_stamps_completion_once`; abandoned ground prevents it.
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
3. **Rescan routing: the branch set is the rescan scope.** Today the local rescan has two arms and
   `local_rescan_reconciles` (`entry_count > 1 && prior_scan_completed`) picks between them: reconcile the whole volume
   in place, or truncate and bulk-rebuild. A phased volume fits neither — truncating throws away everything the phases
   bought, and reconciling `/` runs the serial per-directory walk with an add-everything delta over ground we never
   covered, which is the documented ~15-minute hang.

   **The third arm is the whole fix: reconcile in place, anchored at each covered branch.** By construction those
   subtrees are the ground we actually hold, so the delta is small and the "only when substantially complete" condition
   the predicate was protecting is satisfied per anchor rather than per volume. Rules:

   - a legacy volume whose scope is `WatchScope::WholeVolume` anchors at `/` — byte-identical to today, which is what
     keeps this from being a behavior change for existing installs;
   - skip any branch with `walks > 0` (it is being covered right now) and let the machine finish it instead;
   - **during the phased period, "Rescan now" means "restart the phases"**, ❌ never an error and ❌ never a truncate;
   - `local_rescan_reconciles` keeps its exact meaning and its exact inputs. ❌ Do not re-key it on
     `promised_scope_covered_at`: it answers "does this volume hold the whole tree", and the phased arm is chosen
     _before_ it is consulted.

4. **Close every truncate door.** A cover-built index has `entry_count > 1` and no `scan_completed_at`, so
   `local_rescan_reconciles` is false and `start_scan` sends `TruncateData`. Two of them close once
   `awaits_its_first_scan` learns about covered ground (the FDA Deny path and the per-drive enable button). **Door (b),
   master off→on, needs explicit work**, and so does **door (d), the coalesced shallow anchor: it is closed today only
   by `is_branch_confined()`, which the watching section deliberately removes** — so door (d) stays open until the sweep
   route is rewritten onto the item-3 arm, and the two are one change, not two. Verify each with a test rather than
   trusting the reasoning. The ways in today:
   - **FDA Deny** ⇒ `start_indexing_after_fda_decision` → `start_volume(root)` → `awaits_its_first_scan` true ⇒
     `force_scan` ⇒ truncating full scan (`commands/indexing.rs:221`, `handle/mod.rs:177-187`). Note this fires on the
     Deny path even though the panes stay on `~`, so the decision to keep both panes home does NOT make this door go
     away. (It only reaches `force_scan` when `state::is_active(volume_id)` already holds, i.e. a search walk stood a
     writer up first — which is precisely the volume that has covered ground worth not truncating.)
   - **Master switch off→on** ⇒ `drives_to_resume()` always includes root ⇒ `start_volume` ⇒ `state::start_indexing()` ⇒
     `resume_or_scan` ⇒ `start_scan("incomplete previous scan")`.
   - **"Rescan now"** ⇒ routed by item 3 above; the door is the truncate arm it must no longer reach.
   - **A coalesced shallow `MustScanSubDirs`** ⇒ `perform_registry_rescan` → `start_scan`
     (`reconcile/reconciler/rescan.rs:122-126`).
   - **`awaits_its_first_scan`** will report "never walked" forever on a phased volume, because it reads
     `scan_completed_at`, which the default setting never stamps. So the per-drive "Turn on indexing for this drive"
     button force-scans too. The predicate needs a phased-aware answer: `promised_scope_covered_at` present, or the
     phased marker set, means "this drive has been walked". ❌ Still not `entry_count > 1` (see the verifier section for
     why that regresses shipped behavior).
5. **Freshness during phases** per the decision above. `StaleDriveDialog` already returns early for `root`, so the
   exposed surface is the per-drive **badge**, not the dialog: a volume that has never reached scope coverage is
   _incomplete_, not _stale_, and those are different sentences to the user.
6. **Existing installs.** The plan is written for a first run, but every beta user upgrades into it with an index
   already on disk. Two cases, and neither may silently narrow what they have:
   - **A volume with `scan_completed_at` set** (a full scan finished under today's code) already holds the whole drive
     and its scope is `WholeVolume`. **Default `index_whole_drive` to ON for it**, once, at the first launch after the
     upgrade, so nobody silently loses drive-wide search they have had for months. New installs get OFF. Implement it
     app-side as a one-time settings backfill keyed on the existing marker, ❌ not as index-DB migration.
   - **A volume with no `scan_completed_at`** (an interrupted or never-finished scan) has partial rows written by a
     `ScanRoot::Volume` walk, whose `listed_epoch` pattern the coverage descent was never designed to read. Per
     "Rebuild, don't migrate" (`indexing/CLAUDE.md`), **truncate it once and let the phases build it fresh**. It is a
     disposable cache, the user was going to pay for a rescan anyway, and reasoning about a foreign partial is a bug
     farm. Say so out loud in the code comment, or someone will "optimize" it later. The truncate is also what makes the
     `EXCLUSION_POLICY_KEY` stamp legal on this path.

     **⚠️ This rule needs a discriminator that doesn't exist yet.** A phased partial and a legacy interrupted partial
     both have rows and no `scan_completed_at`, so "no marker ⇒ truncate" would wipe a cleanly stopped phased index on
     every launch — the exact data loss this milestone exists to prevent. That is what the **durable "built by the
     phases" stamp** (see the verifier section) is for: written at the first stitch, never cleared, and distinct from
     the in-progress marker. ❌ Don't infer it from the branch set or the in-progress flag; both are absent in the
     stopped case.

7. **An external volume promises its own root**, per "The two scopes and the two markers". Test that enabling indexing
   on a drive with no `$HOME` still walks it and does ❌ not stamp completion over an empty promise.

**Tests:** integration tests for launch over an index in each state (nothing, partially covered by phases, partially
covered by a legacy interrupted scan, scope-covered, fully covered, fully covered but stale), asserting which of
{phases, replay, scan, truncate-then-phases} runs; plus one test per truncate door asserting no `TruncateData`. All
**test-first**, because a wrong cell means a wasted full rescan, a silently stale index, or a user quietly losing
coverage they already had. Named individually because each has been reasoned about and could pass for the wrong reason:

- **`an_upgraded_fully_scanned_volume_keeps_indexing_the_whole_drive`** (risk 9: the failure most likely to reach a real
  person, since every beta user hits the upgrade path and nobody hits a first run twice);
- **`a_stopped_phased_index_is_not_truncated_on_the_next_launch`** (the discriminator);
- **`a_legacy_interrupted_partial_is_truncated_once_and_rebuilt`**;
- **`an_external_volume_promises_its_own_root`**;
- **`a_wide_journal_gap_reconciles_the_watched_scope_instead_of_stranding_it`**.

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
     scanning, never scanned" while indexing runs — and then, on the default setting, "never scanned" **forever**. It
     has to report the covered scope, ❌ not a boolean: what we cover, and whether that scope is complete. An agent that
     searches `/opt` and gets nothing deserves to know why.
7. **Write down what a first-run user sees while phases run** (corner hourglass with a phase label, sizes appearing
   folder by folder, search saying it is still building) and check it against the running app. That is the whole "wow
   moment" claim; it deserves an explicit acceptance pass.

   **Include the steady state, not only the busy one.** On the default setting the permanent shape is: home has exact
   sizes, everything outside it renders `<dir>` with no hourglass and no explanation, and nothing is walking. That is
   honest, but "honest" and "doesn't look broken" are different bars, and it is what most users will see most of the
   time. The acceptance pass covers both, and if the steady state reads as broken, the fix is in-context copy (item 3 in
   M5), ❌ not a quiet default flip.

**Tests:** unit tests for the debounce publisher and the bidirectional predicate (both genuinely test-first); a
component test that a row inside _and_ a row above a walking branch show the hourglass while an unrelated row doesn't; a
measurer test that reserved width matches the renderer; and an E2E that the corner appears during a phase — **a post-hoc
pin, not a red→green step**, gated on M3.

**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `file-explorer/views/DETAILS.md` (size state and the measurer contract).

---

## M5 — Surfaces, copy, kill switch

**⚠️ Every user-facing string here is a DRAFT for David** (principle 4: anything meeting human eyes is human-reviewed).
They go through the message catalog with `@key` descriptions, ❌ never hardcoded, and **11 locales ship** — budget the
translation pass rather than discovering it at the end of the milestone.

0. **The whole-drive setting**: default OFF, in the drive-indexing section, written into
   `IndexConfig.index_whole_drive`. Existing installs with a completed full scan get ON once, at upgrade (M3.6).

   ⚠️ **`set_config` is not sufficient on its own.** The media policy works through it because the media gate _reads_
   the value on demand (`host/config.rs:91-103` is a whole-value replace into a static plus atomics). Flipping this one
   has to _do_ something: queue the volume-root phase, un-`Fresh` the volume, and (per the resident-machine rule) wake a
   machine that has already stopped. Name that wake-up path in the implementation, ❌ don't assume the existing call
   covers it.

   Draft copy: label **"Index the whole drive"**; help text **"Cmdr indexes your home folder and the folders you open.
   Turn this on to cover system and app folders too, so search and folder sizes reach everywhere. It runs in the
   background and takes a few extra minutes."** The label has to carry the rule that folders you open are always
   indexed, or the setting reads as a promise the product doesn't keep.

1. **⚠️ The app and the website currently promise a whole-drive index, in the copy that justifies asking for Full Disk
   Access.** This is a deliverable, not a follow-up: shipping default-off without it makes the FDA screen false.
   - App: `onboarding.stepOptional.indexing.benefit1` ("Instant search of your whole drive. Think Spotlight, but even
     faster.") and `benefit2` ("Real-time folder sizes for your whole drive."), plus the `benefit2` key description, in
     `apps/desktop/src/lib/intl/messages/en/onboarding.json` (× 11 locales). Also
     `settings.indexing.enabled.description` ("Index your drive in the background for instant directory sizes."), and
     the three run labels that all claim a full scan: `indexing.run.firstScan`, `indexing.scan.label`,
     `indexing.step.findFilesFirstScan`.
   - Website: `Hero.astro:40`, `Features.astro:13` ("Indexes _your entire drive_ in 4 minutes. Once."),
     `Layout.astro:13`, `index.astro:17`, `features.astro:39`, `llms.txt.ts:35`, `llms-full.txt.ts:51`.
   - **David decides the new claim.** The honest version is stronger, not weaker: your folders are searchable in seconds
     rather than the whole disk in four minutes. Draft it, ❌ don't ship it unreviewed.
2. **The phase labels**, in `IndexingDriveRow` / `IndexingStatusBody` — ❌ not
   `settings/sections/DriveIndexingSection.svelte`, which has three switches and no per-drive rows. Draft: **"Indexing
   the folders you use most"** → **"Indexing the rest of your home folder"** → **"Indexing the rest of the drive"**. ❌
   Not "Indexing your folders" → "Indexing your home folder", which reads as the scope widening and then narrowing,
   since the first is a subset of the second.
3. The drive-index settings section explains the model in a sentence or two.
4. **Three coverage messages, not one, and the third one is the whole discoverability story.** Search's note has to
   distinguish "we haven't got there yet" from "we were refused" and from **"this is outside what you asked us to
   index"**.

   ⚠️ **Get the mechanism right before writing the copy: search self-heals, folder sizes don't.** A search over
   uncovered ground already cover-walks it (carved out of both switches), so search converges toward instant through
   use. What regresses permanently and silently on the default setting is **the size column**, which has no equivalent
   fallback. So the priority is inverted from the obvious one: **the in-pane size affordance comes first**, the search
   note second. Reuse the shipped "index this" affordance rather than inventing a second one, and state plainly that a
   user-requested folder is a rank-1 walk that ❌ never re-opens `promised_scope_covered_at`.

5. **Folders we couldn't read.** Scope completion can be stamped with `Denied` / `Declined` / `Abandoned` directories
   inside it, so "done" can mean "done, with holes". Surface the count where the badge and the coverage note already
   live, ❌ never silently, with a disclosure listing the paths and the cause (principle 3, radical transparency).
   Thousands separators on the count.
6. **The scope-covered badge state needs its own copy.** Green-and-done is the moment a user concludes the app has
   finished, and on the default setting it means "your home folder is indexed, the rest of the drive isn't". Draft that
   sentence with an action attached, ❌ don't let the badge imply more than it holds.
7. **`stop` and `forget` against a phase queue** (`driveIndexMenuActions('scanning')` offers both). **Decided**: `stop`
   cancels the running walk and clears the queue, leaving covered ground covered and watched, and leaves the durable
   "built by the phases" stamp in place so the next launch resumes instead of truncating; `forget` keeps today's
   meaning. ❌ Not a proposal — this is a shipping menu action and an implementer needs the answer.
8. **A kill switch.** This is a big behavioral change to ship into an open beta: one flag that restores the bulk-build
   path, so a bad week is a restart rather than a rollback. ❌ **An env var is not enough**: a beta user launching from
   the Dock never sees one, so it would only ever help David locally. Use a `defaults write` key or a hidden setting
   read at startup, and say who is expected to flip it and how they'd be told to.
9. **Measure it.** Anonymous analytics are live, and this change's justification is a user-experience claim, so make it
   falsifiable: time to `promised_scope_covered_at`, whole-drive toggle adoption, and how often a pane or a search hits
   the "outside your scope" state.

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
4. **Photo search and importance silently dead** ⇒ freshness meaning "fully covered for the chosen scope", fired off
   `promised_scope_covered_at`, which on the default setting arrives when home completes.
5. **A truncate door left open** ⇒ M3.4 enumerates all five; one test each. And the door's twin, the ~15-minute
   reconcile hang over ground we never covered ⇒ M3.3, rescan anchored at the branch set.
6. **The verifier as a second, unthrottled indexer** ⇒ the `phase_active` flag plus the verifier **bailing** on
   `listed_epoch == 0` (❌ not the mark; the two are mutually exclusive and the bail is what this plan builds). Both are
   M2 deliverables, ❌ not "consider it later": with the stitch giving every frontier root a row, the verifier's
   recursive `scan_subtree` fires for every folder the user opens ahead of the walker, which is the central user
   behavior this plan is built around.
7. **TCC popups from a background walk** ⇒ the FDA gate as today, plus the Deny-path decision (panes stay on `~`, and
   background phases skip TCC-restricted roots, so a prompt only ever follows the user's own navigation).
8. **High-churn directories** (the 1.14M-empty-file Google Drive temp dir in `docs/specs/later/sealed-subtrees-plan.md`)
   land in the home phase now instead of the whole-drive scan, so they hit sooner. Watch for it during the benchmark's
   browsing arm.
9. **An existing user silently losing drive-wide coverage** on upgrade ⇒ M3.6: a volume that already completed a full
   scan gets the whole-drive setting ON, once. This is the failure most likely to reach a real person, because every
   beta user hits the upgrade path and nobody hits a first run twice.
10. **The default configuration going untested** because the plan and its tests reason about the toggle-on end state ⇒
    every routing, sweep, collapse, and completion test above runs with the toggle **off** unless it is specifically
    about the whole-drive phase. Default-off is what ships to everyone.
11. **The now-minority whole-drive path rotting**, which is the inverse risk and the one nobody will notice: after this
    change `should_replay_journal`, `local_rescan_reconciles`'s reconcile arm, the whole-volume sweep, and the
    truncate-rebuild path only run on legacy and toggle-on volumes — and per decision 11 that is every existing beta
    user's index. ⇒ Keep a named toggle-on / legacy suite running, and record in `lifecycle/DETAILS.md` that those are
    the minority path now so nobody deletes them as dead.
12. **The app promising a whole-drive index while shipping a home-only one** ⇒ M5.1 enumerates the onboarding, settings,
    run-label, and website strings. This is the one that makes the Full Disk Access ask untrue, so it is a deliverable
    rather than a copy-polish follow-up.

## Decisions (David, 2026-08-13, revised 2026-08-14)

Recorded with the reasoning, because the reasoning is what an implementer needs when reality disagrees with a detail.
Decisions 8–11 came out of the review round that took the home-only default seriously and found that the rest of the
plan had been written for a whole-drive end state that, by default, never arrives.

1. **The app's answers arrive through host seams, not through widened handle calls.** Scope is an `IndexConfig` field,
   priority roots are a `HostPolicy` method, and "where is the user" reuses the `open_listings` seam that already
   exists. `start_root_at_launch` and `verify_directory` keep their exact signatures and meanings. The earlier "add a
   parameter, widen a method" shape was a forced fit around the surface ceilings; this one is what the crate's own seam
   rules already prescribe, and it happens to cost nothing. Full reasoning in "Where the app's answers enter the crate".
2. **On FDA Deny, both panes stay on `~`**, and background phases skip TCC-restricted roots. The permission dialog fires
   when the user navigates somewhere protected, which is the only moment it has a cause they can see. Moving the right
   pane to `~/Downloads` would buy the same prompt a few seconds earlier and re-shuffle the panes behind the onboarding
   sheet.
3. **`Fresh` means "fully covered for the scope the user chose"**: home plus the priority roots with the toggle off (the
   default), the whole drive with it on. Honest in both modes, and on the default it comes alive minutes into the first
   run. It fires off `promised_scope_covered_at` (decision 8).
4. **Branch watching stays on when drive indexing is off, on macOS only**, and branches absorb their descendants. The
   branch set collapses to the phase root at the end of every phase, ❌ not only at full-volume coverage.
5. **"Rescan now" re-walks in place, anchored at the branch set**, keeping sizes visible; during the phased period it
   restarts the phases. ❌ Never anchored at `/` on a volume that only covers part of it: that is the documented
   ~15-minute serial hang, and the single-marker design walked straight into it.
6. **New setting: "Index the whole drive", default OFF.** `index_whole_drive` on `IndexConfig`. Flipping it on adds
   frontier and un-`Fresh`es the volume; flipping it off leaves rows we stop maintaining. Both fall out of coverage
   being add-only, so neither needs a migration. `~/Library` is in scope but never a priority root (see M1).
7. **One worktree, one effort, for the indexing work.** The milestones below are an execution order, ❌ not shippable
   slices, so ordering still matters but "is this milestone independently releasable" doesn't. The first-run startup
   state was the exception: it turned out to be self-contained, so it shipped on its own ahead of the indexing work.
8. **Two markers, not one.** `promised_scope_covered_at` (new) means the promised scope is covered and drives freshness,
   the media and importance kick, the badge, and the completion UI. `scan_completed_at` keeps today's exact meaning, the
   whole tree has rows, and with the default setting is never stamped. Stamping one marker for both would route "Rescan
   now" into a serial reconcile over uncovered ground, make whole-volume journal replay pre-empt the phase machine
   forever (so turning the toggle on after a relaunch would silently do nothing), and have MCP tell an agent a
   third-covered drive was fully scanned. Full reasoning in "The covered scope".
9. **The watched scope is the rescan scope**, derived from the `WatchScope` VARIANT (`WholeVolume ⇒ [volume_root]`,
   `Branches(set) ⇒ set`), and it is what rescan routing, sweep routing, and completion all consult. ❌ Not the branch
   set's contents: a legacy scanned volume carries `WholeVolume(<empty set>)`, so reading the contents would make it
   answer for nothing.
10. **The toggle governs background coverage, ❌ not "never index outside home".** A folder the user opens is covered
    wherever it lives, and never holds scope completion open. The setting's name and help text have to say so (M5.0), or
    it promises something the product deliberately doesn't do. **The machine therefore stays resident and idle after the
    promise is covered** — completion stops the phases, not the machine — or the promise is true for a few minutes and
    false forever after.
11. **Any volume that isn't the boot volume promises its whole root.** Enabling indexing on a drive IS the request, and
    a promise of "home plus favorites" would be empty on an external disk, stamping completion and going `Fresh` over
    zero rows. `index_whole_drive` governs the boot volume only.
12. **Where a user-facing claim and the shipped behavior disagree, the claim changes and David writes it.** The
    onboarding benefit copy, the settings description, the run labels, and seven website strings all promise a
    whole-drive index today (M5.1). Shipping default-off without changing them makes the Full Disk Access ask false.
13. **An existing install that already completed a full scan gets the toggle ON, once, at upgrade** (M3.6). Everyone in
    the beta upgrades into this change; nobody may quietly lose drive-wide search they have been using for months. An
    index with a partial, never-completed scan is truncated once and rebuilt by the phases, per "rebuild, don't
    migrate".

Remaining assumption to confirm during execution, ❌ not a blocker: `~/Library` in scope but de-prioritized (M1).
