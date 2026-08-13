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
   popup.
2. The moment Full Disk Access is granted (today: the relaunch after step 1 of onboarding), the layout becomes left `~`,
   right `~/Downloads`, hidden files hidden, and the phased indexer starts on the user's own folders. By the time
   onboarding is done, the folders they'll actually open are indexed.
3. As they browse, whatever they open gets indexed next, ahead of everything still queued.
4. Home finishes, then the rest of the drive, quietly, in the background.
5. At every moment the sizes shown are honest: exact where covered, `<dir>` or `≥` where not, an hourglass on ground
   being walked right now.

## The architecture

**Every walk is a coverage walk. There is no first full scan.** Today the first index of a volume is
`ScanRoot::Volume`: truncate, then bulk-build with the parallel guarded walker. Fast, but all-or-nothing, and
`local_rescan_reconciles` (`lifecycle/manager.rs:193`) deliberately routes a populated-but-never-completed index to
*truncate and rebuild* (reconciling a 4%-complete partial once made the app look hung for ~15 minutes). So "cover the
important folders first, then run the normal full scan" would throw away everything the priority phases built.

Instead the whole drive becomes the **last phase of the same mechanism** the priority folders use:
`Index::coverage(volume_id, scope, Listing)` names the frontier, `Index::cover(volume_id, frontier, Listing, cancel)`
walks it — add-only, durable, resumable, cancellable, through the volume's normal writer, with
`ComputeSubtreeAggregates` giving covered folders honest recursive sizes (its handler repairs the ancestor chain
upward, `writer/aggregation.rs:271-304`, so a covered subtree's size reaches `~` and `/` correctly).

**Why this end state, not just the cheap one:** it collapses three mechanisms into one. Search-driven walks, priority
walks, and "index the whole drive" stop being separate things with separate failure modes. It makes the later want
("watch only specific folders, especially on Linux where inotify watches are scarce") the *default* shape rather than a
retrofit. And it makes indexing interruptible without loss, which is what lets us spend the user's CPU politely.

### The stitch: what makes phases compose at all

**This is the piece without which the whole model silently degrades, and it is not obvious.**

A cover walk marks only the directories it *reads*. Bootstrap deliberately creates the ancestor chain at
`listed_epoch = 0` and claims nothing (`lifecycle/cover/bootstrap.rs:10-13`), and the coverage descent cuts at the
first `listed_epoch == 0` directory without descending past it (`read/coverage.rs:195-207`). Two consequences:

1. After phase 1 covers `~/Downloads`, `coverage(root, "$HOME")` still answers `["$HOME"]` and `coverage(root, "/")`
   still answers `["/"]`. **The frontier for an ancestor scope never shrinks**, so "skip a root that's already covered"
   and "the preempted phase resumes with less to do" are both false for exactly the phases that need them.
2. Worse, `cover` over such a root hits `ScanRoot::Virgin`'s refusal — `count_children_capped(root_id) > 0` ⇒
   `ScanError::NotVirgin` (`scanner/mod.rs:776-781`) — and routes to `repair_non_virgin` → `reconcile_subtree`
   (`lifecycle/cover.rs:493`), the **serial** per-directory walk. That is the exact path documented as making the app
   look hung for ~15 minutes over a real `/`.

So each phase is preceded by a **shallow stitch**: for every ancestor of the phase root, from the volume root down,
read that one directory, upsert its children, and `MarkDirsListed` for that directory alone. No descent, no deletion.
It is honest (we really did list those directories) and cheap (a handful of `readdir`s). After the stitch, the coverage
descent walks *through* the ancestors and cuts at each genuinely unlisted child, so:

- a covered subtree is skipped, correctly;
- every frontier root handed to `cover` is genuinely virgin, so the **parallel** guarded walker takes it;
- the big phases become **many small walks instead of one huge one**, which is what makes priority interleaving cheap
  (below).

Check whether a depth-1 listing primitive can be assembled from `reconcile/verifier.rs`'s readdir-diff rather than
written from scratch; ❌ do not use `scan_subtree` (`ScanRoot::Rebuild` deletes descendants first, which would destroy
covered ground).

### Interleaving without preemption

Because the stitch turns `$HOME` and `/` into a list of independent frontier roots, the machine walks them **one root
at a time and checks the priority queue between roots**. A folder the user opens waits for one subtree, not for the
drive.

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
- **`ScanCalibration` capture and the live counters.** `scan_calibration` is set only in `start_scan`, and
  `get_status` derives its counters from it plus a live `ScanHandle`. Without it, `status()` reports
  `scanning: false` with zero counters for the entire first index, so the per-drive row, progress bar, and ETA are dead.
  The phase machine must feed the same shape.
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
stamp all happen in `prepare_database_for_a_walk` (`state/walk_database.rs:99-121`). **State the stamp explicitly in
the code comments**: a stale or absent `EXCLUSION_POLICY_KEY` sends every scope to the walk wholesale
(`read/coverage.rs:314-317`), which would silently destroy convergence. `SYSTEM_DIR_EXCLUDES` and the exclusion policy
apply identically to every walk. One real behavioral difference: `ScanRoot::Virgin` pins the walk root's **device**
while `ScanRoot::Volume` bounds by path prefix, so the `/` phase cuts at mounted filesystems rather than at
`/Volumes/`. A device cut writes no row, so it can't leave a permanent frontier node, and firmlinked system paths share
one device (`/`, `/System/Volumes/Data`, `/Users`, `/Applications` all report dev=16777231; verified on macOS 25.5 via
`stat -f %d`, 2026-08-13). Acceptable, but it means the `/` phase indexes a slightly different set than today's scan.

### Activation: keep `IndexTheVolume`

❌ **Do not launch the phased volume as `Activation::WriterOnly`.** `journaled` is computed as
`activation == IndexTheVolume && kind.has_event_journal()` (`state/startup.rs:135`), and a `WriterOnly` start never
calls `resume_or_scan`. That would cost, on every launch: no FSEvents journal replay for the boot disk, and (once
`scan_completed_at` exists) a `Stale` load that **bumps the epoch**, rendering every directory size stale forever. Both
ledger heals above live in `resume_or_scan` too. `WriterOnly` is designed for a volume no scan is ever coming for; a
launch-time phased index is not that. The phase machine belongs **inside `resume_or_scan`'s decision**, as a third
answer beside replay and scan.

### Watching: probably no handover at all

On macOS `DriveWatcher::start_branches` already watches the **volume root** and filters by `WatchScope::Branches`
(`watch/watcher.rs:204-211`) — which is exactly the "watch `/`, keep only what we care about" model. On Linux it
watches each branch, deliberately: `notify`'s recursive mode costs one inotify watch per directory against
`max_user_watches`.

So a fully covered volume can simply keep `WatchScope::Branches` with `/` as its single branch, and the
branch→whole-volume handover never has to be written. On Linux a `/` branch is watched recursively, which is the same
cost as whole-volume watching. **Prefer this.** If a handover is written anyway, its traps are: start from
`replayable_event_id()` / `BranchWatch::safe_event_id()`, never 0, or the gap is lost; and `WatchScope` is captured by
value into the reconciler and `LiveConfig`, so swapping it means re-spawning the loop — drain `take_promoted()` first
or buffered events die with the old loop.

### Freshness, and the two subsystems that depend on it

**This is the decision with the largest product blast radius, and the first draft of this plan missed it entirely.**

Folder importance and the whole media index (OCR, Vision tags, CLIP embeddings — that is photo search) start their
passes off `Freshness::Fresh` plus a `ScanCompleted` publish on the lifecycle bus
(`state/queries.rs::ready_volumes_with_kind` filters on `Fresh`; the bus publish fires only on
`FreshnessEvent::ScanCompleted`, `state/freshness_bridge.rs:95-98`, which today only `scan_completion.rs:351-358`
fires). If nothing fires until the final `/` phase ends, **photo search and importance scoring are dead for the entire
phased period, and forever on a machine that never finishes `/`.**

Recommendation: treat the two axes as what they are. **Freshness = "are the rows we hold current?" Coverage = "how much
do we hold?"** They are already orthogonal in this codebase. So publish `Fresh` (and the bus completion) once the
volume is watched and the priority phases are done, not at full drive coverage, and let importance and media work over
covered ground while later phases fill in. See Question 3.

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

## The public surface constraint

`scripts/check/checks/index-crate-isolation.go` caps `cmdr-index` at exactly what it exposes: measured 2026-08-13,
`50 root promises, 40 handle methods, 17 public modules, 156 items` against ceilings of `50 / 40 / 17 / 156`. **Zero
headroom in all four buckets**, and raising one needs David's explicit say-so.

The plan therefore adds no new `Index` method and no new public type. Two existing doors carry everything:

1. `Index::start_root_at_launch(fda_pending)` gains a parameter: the ordered priority roots as `&[String]`.
2. `Index::verify_directory(volume_id, path)` gains "and if this ground isn't covered, queue it next".

New `IndexEvent` variants are free: `countModuleItems` counts only line-prefixed `pub struct/enum/fn/const/type` and
`pub use` leaves (`index-crate-isolation.go:506-539`), not enum variants. **Caveat: any new payload TYPE those variants
carry, or any new `pub fn` on a public type, breaches immediately.** Each variant needs a doc comment
(`#![deny(missing_docs)]`) and regenerated `bindings.ts`.

## Milestone map

- **M0** — first-run startup state (frontend only). Independently shippable.
- **M1** — priority-root computation, app-side, pure and tested.
- **M2** — the stitch plus the phase machine, gated on a benchmark.
- **M3** — launch, resume, and every path that would truncate.
- **M4** — events, status, and the hourglass UI.
- **M5** — settings, surfaces, kill switch.
- **M6** — optional signals and follow-ups.

M0 and M1 are independent of each other and of M2; either can run in parallel with M2's benchmark. Everything after M2
is sequential, which is fine. One honest caveat: **M4's unit tests stand alone, but its end-to-end assertion doesn't**,
because the bug it fixes only exists once M2/M3 land. Land M4's pure logic whenever; pin its E2E after M3.

---

## M0 — First-run startup state

**Intent:** open on the two folders that matter, without tripping a TCC popup before the user has decided anything, and
without ever overriding a returning user's own layout.

1. **Hidden files off by default.** `listing.showHiddenFiles` defaults to `true` in **five** places, all of which move
   together:
   - `settings/definitions/appearance.ts:410` (the registry default, resolved at read time);
   - `settings/reactive-settings.svelte.ts:34` (the `$state` seed);
   - `src-tauri/src/settings/loader.rs:29` (`#[serde(default = "default_show_hidden")]`);
   - `src-tauri/src/settings/loader.rs:235` — `parse_settings` is hand-rolled and hardcodes `.unwrap_or(true)`, which is
     what actually runs; the serde attribute is dead for the real path;
   - `src-tauri/src/settings/loader.rs:172` — `impl Default for Settings`, returned when `settings.json` is missing or
     unparseable. **That is the first-run path**, so missing it defeats the item.

   **Existing users are handled by the store's own design; say so in the plan so nobody "fixes" it with a migration:**
   `settings-store.ts` is sparse and resolves defaults at read time, and `setSetting` records an explicit choice even
   when the value equals the default. Never-touched ⇒ picks up `false`; deliberately turned on (Settings switch or View
   menu, both via `setSetting`) ⇒ keeps `true`. David's product call accepts the flip for the first group.

   **Tests:** `parse_settings("{}")` ⇒ `show_hidden_files == false`, `Settings::default().show_hidden_files == false`,
   plus a registry assertion on the frontend. No defaults-parity check exists in `scripts/check/`; these are the cheap
   substitute for one.
2. **First run opens `~` on both panes** — already true (`DEFAULT_PATH = '~'`, `app-status-store.ts:12`); pin it.
3. **The one-shot layout.** FDA granted + this install never had a layout ⇒ left `~`, right `~/Downloads`, once, ever.

   **⚠️ The naive guard fires for every existing beta user on the first boot of the new build** (FDA granted, no marker),
   clobbering a real layout. And because pane paths persist like any navigation, an applied layout *becomes* their
   layout: a wrong fire is unrecoverable. So **backfill the marker for pre-existing installs before the rule is
   evaluated**: if `app-status.json` has any pane state (`leftTabs` or the legacy `leftPath` key present — key presence,
   ❌ not tab content), set `firstRunLayoutApplied: true` and skip. Backfill and rule land in one commit.

   ❌ Don't gate on `onboarding.completed`: `startup-gates.ts` flips it to `true` in the same boot for a fresh install on
   a Mac that already has FDA, which is exactly when the layout SHOULD apply.
4. **Where it fires:** inside `loadPersistedState` (`pane/initialization.ts`), **before** the `if (e2eStartPath)` block,
   so the E2E fixture override still wins. Boot is the site because granting FDA requires a relaunch (the gate is set
   once at boot; clearing it at runtime raced 5–10 stacked TCC popups once).
5. **Suppress under automation:** gate on `isE2eRun()` from `$lib/app-mode`, ❌ never `getAppMode() === 'e2e'` (capture
   is a refinement of e2e). Every shard gets a fresh data dir and boots with FDA mocked granted; the marketing-capture
   shard runs over real folders with `CMDR_E2E_START_PATH` unset, so without this it would move the right pane to the
   real `~/Downloads` and change the masters.
6. **`~/Downloads` may not exist.** Fall back to `~` and still set the marker.
7. **Wiring the marker:** a field on `AppStatus`, a read in `loadAppStatus`, and a write branch in `doSaveAppStatus` —
   that function persists only the fields it enumerates, so an unlisted key silently never saves. Don't let the marker
   write ride the 200 ms debounce if anything that could quit follows it.

**Tests:** unit tests for the decision as a pure function (granted × marker × backfill × path-exists × `isE2eRun`), a
store round-trip test, and a Playwright E2E over first run with `CMDR_MOCK_FDA`. The decision function is test-first: a
wrong fire destroys a real user's layout.

**Docs:** `file-explorer/pane/DETAILS.md`, `onboarding/DETAILS.md`, `settings/DETAILS.md`.

**Checks:** `pnpm check --fast`, then `pnpm check svelte desktop`, and **`pnpm check --include-slow` is an exit
criterion for this milestone**: it changes the startup layout and dotfile visibility, which is what the Playwright specs
are most sensitive to. Expect i18n screenshot masters to shift; regenerate them here.

---

## M1 — Which folders matter to this user

**Intent:** guess the user's important folders from signals we already have, cheaply, with no new permissions and no
network. Ordered best-signal-first, because the order *is* the schedule.

A new app-side module (`apps/desktop/src-tauri/src/indexing_priority/`) exposing one function: the ordered,
deduplicated, existence-checked roots.

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

Then the machine appends the volume root as the final phase.

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

**The benchmark must include the stitch**, or arm (b) measures the `NotVirgin` serial repair and looks catastrophic,
or measures a virgin `/` walk that the product would never actually run. Venue: `crates/index-query` or an in-crate
`#[cfg(test)]` bench — ❌ not `crates/cmdr-index/benches/`, which compiles against the crate as EXTERNAL and can only
reach the public surface. Write the numbers to `docs/notes/phased-vs-bulk-index-<date>.md`, link it from the lifecycle
`DETAILS.md`. **If (b) is more than roughly 1.5× (a) to full coverage, stop and re-decide with David.**

### The machine

1. **Activation stays `IndexTheVolume`**, and the phase machine is a third answer inside `resume_or_scan`, beside
   replay and scan. (Rationale above: journaling, launch freshness, and both ledger heals hang off this.)
2. **The stitch runs before each phase** (described above): list each ancestor of the phase root, mark that one
   directory listed, don't descend.
3. **The queue**: rank 0 the M1 roots, rank 1 roots the user visited while running, rank 2 `$HOME`, rank 3 the volume
   root. One walk at a time (`cover` is already internally parallel; a second concurrent walk fights it for the disk and
   the writer). Between frontier roots, re-check the queue — that is what makes interleaving cheap.
4. **Each phase step**: `coverage(volume_id, root, Listing)` for the frontier; empty ⇒ skip; otherwise walk its roots
   one at a time. The walk marks, aggregates, and claims its own ground.
5. **Visits enter through `Index::verify_directory`**, and the enqueue must sit **above `maybe_verify`**: that function
   early-returns while scanning and applies a 30 s debounce, a 2-slot cap, and in-flight dedup that would silently
   swallow enqueues. Note it is a *listing* hook, so it also fires for the opposite pane, MCP listings, and refreshes;
   `record_visit` is the tighter navigation signal if that proves too loose.
6. **Preemption is the fallback, not the mechanism** — with the join-before-restart and debounce rules above.
7. **Completion is derived, not remembered.** `abandoned_ground` is per-walk and in-memory, so it can't answer "was
   anything abandoned in a previous session?". The durable signal already exists: an abandoned directory is never
   marked listed, so it re-enters the frontier. **Derive completion from a final `coverage()` answer whose frontier is
   empty, with only `permission_denied` / `declined` left over**, and keep `abandoned_ground` as an in-session guard.
8. **On completion**: stamp `scan_completed_at`, write the calibration meta, publish freshness, fire the terminal
   events, and (unless the handover is written) leave the volume branch-watched with `/` as its single branch.
9. **Feed the live status shape** (`ScanCalibration`-equivalent counters) throughout, or the per-drive row, progress
   bar, and ETA stay dead for the whole first index.
10. **Handle `RootUnlistable`** yourself: a cover walk over a vanished drive otherwise reports "covered nothing" instead
    of the typed abort that clears the stuck UI row.
11. **Master switch and per-drive veto** keep outranking everything.

**Tests** (integration, `crates/cmdr-index/src/indexing/tests/`, over the disk-image fixture and `InMemoryVolume`):

- after a stitch, an ancestor scope's frontier EXCLUDES already-covered ground, and every frontier root is virgin
  (this is the finding that broke the first draft; pin it hard);
- phases run in order; a covered root is skipped without a walk;
- a visited root is taken between frontier roots without cancelling anything;
- a preempted walk's rows survive (row count only grows), and the restart joins before starting;
- full coverage stamps completion exactly once; abandoned ground prevents it;
- master-off runs nothing.

The first, second, fourth, and fifth are test-first, real red before green.

**Docs:** `lifecycle/CLAUDE.md` (one must-know per new invariant, terse), `lifecycle/DETAILS.md` (the stitch and why,
the phase model, interleaving, completion), `indexing/DETAILS.md` (the data flow now that there is no first full scan),
the benchmark note.

---

## M3 — Launch, resume, and every path that truncates

**Intent:** a partially covered volume must come back as a partially covered volume, and nothing may quietly truncate it.

1. **`start_root_at_launch(fda_pending, priority_roots)`**; call site `apps/desktop/src-tauri/src/lib.rs:847`.
2. **`resume_or_scan` learns the phased answer** (see M2.1). The queue itself needs no persistence: it is recomputed
   from the M1 roots plus a coverage query per root, so a launch naturally skips what is done. Prefer that over
   persisted queue state, which can go stale or disagree with the database.
3. **Close every truncate door.** A cover-built index has `entry_count > 1` and no `scan_completed_at`, so
   `local_rescan_reconciles` is false and `start_scan` sends `TruncateData`. The ways in today:
   - **FDA Deny** ⇒ `start_indexing_after_fda_decision` → `start_volume(root)` → `awaits_its_first_scan` true ⇒
     `force_scan` ⇒ truncating full scan (`commands/indexing.rs:221`, `handle/mod.rs:177-187`). This makes Question 2
     sharper than it looks.
   - **Master switch off→on** ⇒ `drives_to_resume()` always includes root ⇒ `start_volume` ⇒ `state::start_indexing()`
     ⇒ `resume_or_scan` ⇒ `start_scan("incomplete previous scan")`.
   - **"Rescan now"** (Question 5).
   - **A coalesced shallow `MustScanSubDirs`** ⇒ `perform_registry_rescan` → `start_scan`
     (`reconcile/reconciler/rescan.rs:122-126`).
   - **`awaits_its_first_scan`** will report "never walked" forever on a phased volume, so the per-drive "Turn on
     indexing for this drive" button force-scans too. The predicate needs a phased-aware answer.
4. **Freshness during phases** per the decision above. `StaleDriveDialog` already returns early for `root`, so the
   exposed surface is the per-drive **badge**, not the dialog: a volume that has never reached full coverage is
   *incomplete*, not *stale*, and those are different sentences to the user.

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
   **Keep walk COUNTERS out of that same reactive map**: a `SvelteMap.set` per progress tick would re-run the
   membership `$derived` for every visible row on every tick.
3. **Corner hourglass:** `isAnyVolumeIndexing()` is `activity.size > 0 || aggregation.size > 0 || phase.size > 0`, and a
   coverage walk populates none of the three (`ComputeSubtreeAggregates` has no progress callback). So the corner stays
   dark through the entire first index unless this milestone fixes it.
4. **Per-folder hourglass:** replace `isDirSizeUpdating`'s "the volume is scanning" input with "this row is affected by
   ground being walked". Two things the naive version gets wrong:
   - **the test is bidirectional** — `ComputeSubtreeAggregates` repairs the ancestor chain upward, so walking
     `~/Downloads/big` changes the size of `~/Downloads` and `~`. Use
     `rowPath.startsWith(walkRoot) || walkRoot.startsWith(rowPath)`;
   - **three consumers must move together**: `views/FullList.svelte` (two call sites),
     `views/measure-column-widths.ts`, and `selection/SelectionInfo.svelte`. The measurer is the dangerous one: the size
     column reserves width for the glyph, so a per-row renderer against a per-volume measurer clips it on exactly the
     rows that show it.
5. **The 1-second debounce lives in the publisher**, not the rows: `index-state.svelte.ts` exposes a branch only after
   it has been walking 1 s continuously, and drops it immediately on the terminal event. One timer per branch, owned by
   the module, cleared in `destroyIndexState`. ❌ No timers in rows (a `$derived` can't hold one; a per-row interval is a
   per-row leak).
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
component test that a row inside *and* a row above a walking branch show the hourglass while an unrelated row doesn't; a
measurer test that reserved width matches the renderer; and an E2E that the corner appears during a phase — **a
post-hoc pin, not a red→green step**, gated on M3.

**Docs:** `indexing/CLAUDE.md` + `DETAILS.md`, `file-explorer/views/DETAILS.md` (size state and the measurer contract).

---

## M5 — Surfaces, copy, kill switch

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
2. **The frontier not composing** (the finding that broke draft 1) ⇒ the stitch, plus the first M2 test.
3. **A partially covered volume claiming completeness** ⇒ completion derived from a durable empty frontier, not from
   in-memory `abandoned_ground`.
4. **Photo search and importance silently dead** ⇒ the freshness decision, made explicitly (Question 3).
5. **A truncate door left open** ⇒ M3.3 enumerates all five; one test each.
6. **The verifier manufacturing non-virgin nodes.** `verify_and_correct` upserts children into a directory without
   marking it listed, which is exactly what `ScanRoot::Virgin` refuses. Today that is rare because the volume is fully
   scanned early; under this plan the user browses uncovered ground for minutes, so browsed-but-uncovered folders become
   non-virgin nodes whose later phase walk takes the serial repair. Bounded (those subtrees are small), but measure it,
   and consider having the stitch mark a verified directory listed when the verifier genuinely read all of it.
7. **TCC popups from a background walk** ⇒ the FDA gate as today, plus Question 2.
8. **High-churn directories** (the 1.14M-empty-file Google Drive temp dir in `docs/specs/later/sealed-subtrees-plan.md`)
   land in the home phase now instead of the whole-drive scan, so they hit sooner. Watch for it during the benchmark.

## Open questions

Answers wanted before M2 starts; M0 and M1 can proceed regardless (except where noted).

1. **Public-surface consent.** No new `Index` method, no new public type: `start_root_at_launch` gains a parameter and
   `verify_directory` gains behavior. Enum variants on `IndexEvent` are free per the checker, as long as they carry no
   new type. Confirm that's the deal.
2. **The Deny path.** If the user denies FDA, indexing starts in-session and covering `~/Downloads` / `~/Documents`
   raises a per-folder TCC prompt from a background walk. Also: today Deny routes through `start_volume` →
   `force_scan`, which is a truncating full scan, so this path needs M3.3 either way. And the wizard renders the live
   app behind its backdrop (`onboarding/DETAILS.md` leans on "first launch lands on `~`, so what peeks through is
   friendly"), so applying the layout in-session makes the panes visibly re-shuffle behind the sheet. Options: (a) apply
   the layout and run the phases anyway; (b) apply the layout but skip TCC-restricted roots until the user visits them;
   (c) stay on `~`/`~` and index only unrestricted ground; (d) defer the layout to the next launch. Blocks the last part
   of M0.
3. **Freshness during phases** (the big one). Publish `Fresh` + the bus completion once the volume is watched and the
   priority phases are done, so folder importance and the media index (photo search) work over covered ground — or hold
   both until full drive coverage and accept they're dead until then? My recommendation is the first: freshness answers
   "are these rows current?", coverage answers "how much do we hold?", and they're already orthogonal.
4. **Branch watch while drive indexing is off.** `branch_watch_allowed` ANDs the master switch, so a search-walked
   folder stays covered but stops being kept current, and search can serve stale rows from it. Your argument says watch
   it anyway. My read: agree on macOS (the FSEvents stream is volume-rooted and nearly free; stale search results are
   worse than a cheap watcher), keep the refusal on Linux (each branch costs real inotify watches). Confirm the platform
   split, or pick one rule for both.
5. **What "Rescan now" means** once there is no full scan: (a) truncate and re-walk everything (today's meaning), or
   (b) re-walk covered ground in place, keeping sizes visible throughout.
6. **Scope of the final phase.** Everything under `/` as today, or stop at `$HOME` plus mounted volumes and leave
   `/System`, `/Library`, and friends to on-demand search walks? The second is faster to "done" and indexes less that
   nobody browses, but changes what a `/`-scoped search can answer without walking.
7. **Do you want M0 shipped on its own** (it is independent and delivers the startup-state half immediately), or held
   until the whole effort lands as one story?
