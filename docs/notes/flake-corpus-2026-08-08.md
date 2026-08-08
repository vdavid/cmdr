# Flake corpus, 2026-08-08

Every test observed failing without a product defect behind it, with the evidence for each and a cause hypothesis at a
stated confidence. Written as the input to a de-flaking pass whose goal is that each of these becomes structurally
unable to flake, so it says WHERE the evidence came from rather than asking anyone to re-measure.

**This is a living ledger.** The de-flaking pass ran on 2026-08-08 and marked every entry below. Legend:

- ✅ **Fixed** — a named defect with a landed remedy, plus how it was proved.
- 🟡 **Explained** — not open flake: the failures all predate a fix that has since landed. Verified against dates, not
  assumed.
- ⚪ **Accepted** — left alone on purpose, with the one-line defense.
- ❌ **Attempted, reverted** — a remedy was tried and MEASURED WORSE than the status quo. Read these before retrying:
  they're the expensive ones.

## What the de-flaking pass changed (2026-08-08)

Landed:

1. **A click that never happened fails at the click.** New `clickEntryInPane` replaces the `if (entry) entry.click()`
   idiom in `ensureAppReady` and four specs, and `app.spec`'s "moves cursor when clicking a file entry" stopped
   `return`ing early on a short pane (a silent pass of a test that never clicked).
2. **`selectItemsByName` is race-free and verifies its own selection**: one-snapshot index read, then an assertion that
   the selection is exactly what was asked for.
3. **`search-open-in-pane` resets the dialog (⌘N) before typing**, so a leftover run's rows can't satisfy its wait;
   `typeAndRunSearch` and `setSearchInputValue` stopped swallowing a missing input.
4. **`flushFileWatcher` after the external writes** in `archive-browsing` (×2) and `conflict-edge-cases`.

Not landed, and the most important thing in this document:

5. ❌ **The shared-fixture leak is diagnosed and reproducible, and five attempts to fix it all measured worse.** See §
   The shared-fixture leak below. It is the single biggest source of E2E flake found in this pass, it is NOT fixed, and
   the next person should start there — with the measurements, not from scratch.

Detail and the evidence for each sit in the commits; the per-entry verdicts are inline below.

## The shared-fixture leak — diagnosed, reproduced, NOT fixed

**The defect.** The fixture tree under `left/` and `right/` is shared and mutated by every conflict, file-op, archive,
and compress spec. `recreateFixtures` in your own `beforeEach` protects YOU, not whoever runs after your last test, and
roughly half the specs never call it at all. So a spec landing behind a mutating one meets that spec's tree and dies
inside `ensureAppReady` with "expected files not found" — a failure that names the VICTIM rather than the culprit and
whose membership shifts with shard order, which is exactly what reads as saturation flake.

**Reproduction (deterministic).** Run `conflict-overwrite-conditional.spec.ts` then `search-open-in-pane.spec.ts`
against one app instance. `search-open-in-pane:272` fails every run with the left pane showing `equal.txt`, `newer.txt`,
`older.txt`.

**Why it matters more than any ranked entry.** It doesn't concentrate on one test, so it never surfaced as a ranked
line: it surfaces as whichever spec happened to run next. Sixteen of the corpus's twenty-two "long tail" entries are
`file-operations` / `conflict-*` / `archive-browsing` failures from ONE shard-run (`nonmtp1/1786017110`), which is
exactly this signature: one spec dirties the tree, everything behind it fails inside `ensureAppReady`.

**What was tried, and what each cost.** Measured on an M3 Max, 2026-08-08, three to four runs per variant against one
app instance, with `archive-browsing.spec.ts` as the regression canary (baseline 3/4 green):

1. Global `beforeEach` doing `recreateFixtures` + `flushFileWatcher`: leak fixed, **`archive-browsing` 0/4**. The blind
   delete-and-rewrite gave every file a new inode each test, cutting the archive volume's watch of `sample.zip` out from
   under the specs that mutate the zip and wait for its refresh.
2. Same, but `recreateFixtures` made non-destructive (compare before writing, so an already-correct tree is untouched):
   `archive-browsing` 2/4. Better, still a regression.
3. Non-destructive restore, flush moved into `ensureAppReady` (after the panes are back on local dirs, so no archive
   listing is active): `archive-browsing` **4/4**, but **the leak came back** — the pane still served the stale listing.
4. Flush repeated between rounds of the readiness poll, in case the fire-and-forget `mcp-nav-to-path` was undoing a
   single flush: leak still present, 0/4.
5. Nav-bounce (navigate the pane to the fixture root and back, forcing a genuine directory read) plus the flush: leak
   still present, 0/3.

**The finding that blocks it.** Restoring the tree on disk is easy; making the app SEE the restore is the hard part, and
`flush_file_watcher` does not do it for this case. Across variants 3-5 the left pane kept serving the previous spec's
entries for the full 10 s budget through five flushes AND a navigate-away-and-back. So the gap is in how a listing gets
replaced, not in the test helper, and closing it means touching production refresh behavior, which is beyond what a
de-flaking pass should change on measurement this thin.

**Recommended next step.** Establish first, in isolation, what actually makes a pane re-read a directory whose contents
changed externally while it was displayed (an MCP `refresh`, a volume re-select, something else), then build the
`beforeEach` on that primitive. Don't re-attempt the five variants above.

**Scope note.** "Flake" here means the test failed while the code under it was correct. Three failures found this night
were NOT flake and are already fixed (`§ Fixed this night`); they're kept because they are the model for the level of
specificity the rest of the corpus should be driven to.

## Evidence sources, and what each can and can't tell you

- **48 E2E shard-runs, 2026-08-04 to 2026-08-08** (`/tmp/cmdr-e2e-playwright-{nonmtp1,nonmtp2,mtp}-<epoch>.log`,
  `/tmp/cmdr-e2e-linux-<epoch>.log`, 102 files). Per-test granularity. This is the richest source. ⚠️ It's `/tmp`, so it
  ages out; the counts below were taken 2026-08-08 and can't be re-derived later.
- **Three whole-suite collapses excluded** from the counts (`cmdr-e2e-linux-1785826042` 240 specs, `nonmtp1-1785971362`
  32, `nonmtp2-1786017110` 50). A run where most of the shard fails is one broken build or one dead app, not per-test
  flake, and folding it in would swamp the ranking.
- **`~/cmdr-check-log.csv`, 2026-07-19 to 2026-08-08, 81 105 rows.** Lane-level only: its `message` column carries a
  summary ("rust tests failed"), never test names, so it gives red-lane FREQUENCY and nothing per-test. Useful as a base
  rate: `rust-tests` 274 fail rows, `svelte-tests` 157, `rust-integration-tests` 80 (some "docker not running"),
  `desktop-e2e-playwright` 78 (several are `tauri build failed`, not test failures), `rust-tests-linux` 64,
  `desktop-e2e-linux` 33, `rustdoc` 17, `website-e2e` 11, `go-tests` 8, `test-sleep` 9.
- **`rust-test-contention`'s own re-run-alone verdicts**, captured in this night's ten check runs. This is the highest-
  quality Rust evidence in the repo: it re-runs each failing test ALONE at the unchanged deadline, so "contention" is
  measured, not assumed.
- **`.config/nextest.toml`** — the repo's existing register of tests that block on real OS event delivery, split into a
  self-healing set (no retries) and a retry-carrying set. Its comments name the exit condition for the second group.
- **`scripts/check/checks/e2e-duration-allowlist.json`** — not a flake list, but a strong correlate: nearly every
  repeatedly-flaking E2E spec below is also on it or near its 2.0 s budget.
- **`scripts/check/checks/e2e-flaky.go`** — reads Playwright's JSON report for `stats.flaky`, so a retry-rescued run
  downgrades to a warn instead of passing silently. It only sees lanes that HAVE retries
  (`retries: process.env.CI ? 1 : 0`), i.e. the Linux lane and CI, never a local macOS run.

## The asymmetry that shapes all of this

`playwright.config.ts` sets `retries: process.env.CI ? 1 : 0`. The Linux Docker lane runs with `CI=true`, the local
macOS lane does not. So the same flake presents as a **red local run** and as a **green-with-a-warn CI run**, and the
Linux logs' `(retry #1)` lines are the only place a rescued flake is visible at all.

## E2E, ranked by distinct shard-runs failed (08-04 → 08-08, 48 shard-runs)

Counts are shard-runs in which the spec failed at least once. A spec passing in the other ~45 runs is the point: none of
these fail consistently.

Each entry: **count — spec** then the hypothesis and confidence.

- **4 — `search-walk-handoff.spec.ts:67` keeps filling the pane, and says so in a toast**: waits on a background walk
  that outlives its dialog, then on a toast; both timing-open. Also the slowest test on the Linux lane (7.5 s against a
  2.0 s budget). Confidence medium.
  - 🟡 **Explained.** All four failures are 2026-08-05 23:01-23:48; `c3eea06eb` (2026-08-06 02:49) says in its own
    message that this spec "failed outright — there was no walk to outlive the dialog", and `0fc5d250c` fixed the
    index-restore behind it. The spec has not failed in any of the nine shard-runs since. Not open flake.
- **4 — `search-walk-handoff.spec.ts:122` reopening the dialog shows the running search**: same walk lifecycle, and it
  asserts on a run still in flight, so its window is inherently narrow. Confidence medium.
  - 🟡 **Explained.** Same four shard-runs, same two commits, same clean record since. See the sibling above.
- **4 — `dialog-inset.spec.ts:214` every dialog's first body section lines up with its title**: not flake. Two real
  defects, both fixed; see § Fixed this night.
- **4 — `search-open-in-pane.spec.ts:280` ⌘[ leaves the snapshot view, ⌘] returns to it**: fails at its
  `pollRightPaneVolumeId` PRE-condition (line 290), before the nav-back logic it exists to test. Its sibling at :247
  does the same setup and rarely fails, so suspicion is on `typeAndRunSearch` returning before results are ready,
  leaving "Open in pane" to act on an incomplete set. ⚠️ Its wait was ALREADY load-scaled 3 s → 10 s once; raising it
  again is the anti-pattern `docs/testing.md` forbids. Confidence medium.
  - ✅ **Fixed, rung 1.** The suspicion was close but not the mechanism. The dialog carries its query across close +
    reopen and RE-RUNS it on its own (this is exactly why the live-walk specs call `resetSearchDialog`), so
    `[aria-label="Show all in main window"]:not([disabled])` could be satisfied by the LEFTOVER run's rows. The click
    then landed while the run under test was still arriving, the button flickered back to `[disabled]` as `resultCount`
    reset, no snapshot was created, and the spec timed out at `pollRightPaneVolumeId` — which reads as "nav history is
    broken". `typeAndRunSearch` now does ⌘N first, so the only run that can enable the button is its own. The wait was
    NOT raised. `typeAndRunSearch` and the shared `setSearchInputValue` also stopped swallowing a missing input, which
    silently ran an earlier spec's query to a green pass.
- **3 — `mtp-conflicts.spec.ts:99` MTP-to-local move with overwrite**: virtual-MTP protocol overhead; on the duration
  allowlist for both platforms. Confidence low.
- **3 — `mtp-cancel-volume-settled.spec.ts:108` first cancel clears via settle, then immediately F8**: waits on the
  backend settle gate quieting down, a real async quiesce with no explicit readiness signal. Confidence medium.
  - ⚪ **Accepted.** Virtual-MTP protocol overhead on a spec already on the duration allowlist; no missing readiness
    signal was found that a wait could replace, and inventing one would mean changing production code to suit a test.
- **3 — `file-operations.spec.ts:291` MCP rename non-autoConfirm**: two of three occurrences are the `dialog-inset`
  sheet leak (fixed); the third predates it. Mostly resolved.
- **3 — `compress-basic.spec.ts:151` cancelling a compress**: real defect, fixed (missing `flushFileWatcher` after a 24
  MB external write). Resolved.
- **2 — `focus-trap.spec.ts:85` command palette Escape**: both occurrences are the `dialog-inset` sheet leak. Resolved.
- **2 — `accessibility.spec.ts:200` light mode main explorer view**: axe audit over a live view; on the duration
  allowlist as inherently heavy. Confidence low.
  - ⚪ **Accepted.** An axe audit over a live view is inherently heavy and has no readiness signal to wait on; both
    failures are the Linux lane under full-suite load.
- **2 — `file-watching.spec.ts:198` handles deletion of the watched directory**: real FSEvents delivery, the same
  drop/coalesce class as the fsevent fix. A watcher test whose subject is a DELETED watch root has no mutation it can
  safely redo, so the usual remedy doesn't transfer. Confidence medium.
  - ⚪ **Accepted.** The corpus's own reasoning holds: there is no mutation to redo once the watch root is gone, and the
    spec already calls `flushFileWatcher` at every other step (12 of its 13 external writes). Left alone rather than
    weakened.
- **2 — `conflict-edge-cases.spec.ts:296` Copy with Overwrite All handles directory-over-file**: conflict spec with
  `recreateFixtures` in `beforeEach`; likely the `selectItemsByName` one-shot read described below. Confidence medium.
  - ✅ **Fixed, rungs 1 + 4.** `selectItemsByName` was indeed the one-shot read; it now waits for the whole set in one
    snapshot and asserts the resulting selection matches. This spec is also exposed to the shared-fixture leak, which
    stays open.
- **2 — `archive-browsing.spec.ts:459` cancelling a paste into the archive**: writes 24 MB externally then relies on the
  pane seeing it, as `compress-basic:151` did, but reaches it via `navigatePaneTo`, which re-reads, so the mechanism
  differs. Confidence low.
  - ✅ **Fixed, rung 1.** The mechanism did NOT differ: `navigatePaneTo(left, …)` targets the path the pane is already
    on, so it serves the cached listing rather than re-reading, and the 24 MB external create reached the pane only via
    FSEvents. `flushFileWatcher` now follows the write, as it does in `compress-basic`. The sibling at `:516` had the
    same gap and got the same fix.
- **2 — `app.spec.ts:285` switches pane focus when clicking other pane**: silent no-op click; see § The two this run's
  board is red for. Confidence high.
  - ✅ **Fixed, rung 4.** See § The two this run's board is red for.
- **2 each — `file-operations.spec.ts:154`, `:214`, `:249` Rename round-trip**: all occurrences are the `dialog-inset`
  sheet leak. Resolved.
- **2 — `mtp-copy-preflight-uses-cache.spec.ts:128` F5 pre-flight scan from cache**: one of two is the sheet leak; the
  other is virtual-MTP overhead, and it's on the duration allowlist. Confidence low.
  - ⚪ **Accepted** for the virtual-MTP half; the sheet-leak half is 🟡 resolved by `4a4840d91`.
- **1 each — 22 further specs**: long tail, mostly `archive-browsing`, `search-*`, `conflict-*`, `mtp-*`. Not
  individually diagnosed.
  - 🟡 **Mostly explained by the shared-fixture leak.** Sixteen of the 22 are `file-operations` / `conflict-*` /
    `archive-browsing` entries from ONE shard-run (`nonmtp1/1786017110`), the signature of a single spec dirtying the
    tree and everything behind it failing inside `ensureAppReady` — the shared-fixture leak, diagnosed but NOT fixed
    (see its section above). `search-live:41` and `search-recent:31` are the 🟡 pre-`0fc5d250c` walk-ground cluster
    (`0fc5d250c` names `search-recent` explicitly). The rest were not individually diagnosed and stay unclaimed rather
    than assumed fixed.

`conflict-dialog-matrix.spec.ts:147` does not appear above because it first failed in the final run of this night; it is
diagnosed below and is **not** a low-confidence entry.

## The two this run's board is red for

Both are named defects with named remedies, not "probably flake".

### `conflict-dialog-matrix.spec.ts:147` — `selectItemsByName: 'doc.txt' not found in focused pane`

**High confidence, and the fix already exists elsewhere in the file tree.** `conflict-helpers.ts::selectItemsByName`
(line ~195) does a **one-shot** `findFileIndex` and throws on a miss. `helpers/cursor.ts::moveCursorToFile` used to do
exactly that and was changed to poll, with a comment stating the precise reason:

> A file-op spec's `recreateFixtures` (beforeEach) deletes then recreates `left/` on disk; the file watcher's debounced
> remove/create diffs can drain just AFTER `ensureAppReady`'s files-present poll, briefly emptying the pane. A one-shot
> read caught that reload window and returned false.

Every conflict spec has `recreateFixtures` in its `beforeEach`, so they sit in exactly that window. `selectItemsByName`
never got the treatment its sibling did. **Remedy**: poll `findFileIndex` the way `moveCursorToFile` does, so a
genuinely absent file still fails after the deadline and a transient empty pane doesn't.

### `app.spec.ts:285` — pane focus after a click

**High confidence.** The test clicks through `if (entry) entry.click()` — a guard that SWALLOWS a missing row — and then
polls 3 s for the focus class. If the right pane hasn't rendered its rows yet, no click happens and the poll waits out
its budget on an effect of an action that never occurred. The failure surfaces as an opaque focus timeout, pointing at
focus handling rather than at the click. **Remedy**: wait for (or assert) the entry before clicking, so a missing row
fails at the click with a message that says so. This is the same family as the repo's existing `bare-poll` rule — an
unchecked boolean guard silently converting "didn't happen" into "didn't work".

## Rust: `rust-tests` contention set

Every name below was re-run **alone at the unchanged deadline by `rust-test-contention` and passed**, in every run it
appeared. That is measured evidence of starvation, not a guess. Counts are runs-appeared out of the six `rust-tests`
runs this night that reported a set.

- 5×
  `indexing::lifecycle::cover::cold_drive_tests::a_change_inside_a_walked_branch_reaches_the_index_and_one_beside_it_does_not`
- 4× `downloads::commands::tests::go_to_latest_returns_download_from_scan_fallback_when_ring_is_empty`
- 4× `downloads::runtime::tests::note_pending_write_for_cmdr_outside_downloads_silently_noops`
- 4× `downloads::runtime::tests::note_pending_write_for_cmdr_suppresses_watcher_event_end_to_end`
- 4× `downloads::commands::tests::go_to_latest_returns_empty_when_ring_and_scan_both_turn_up_nothing`
- 3× `downloads::watcher::tests::dropping_a_file_emits_one_event`
- 3× `indexing::lifecycle::cover::cold_drive_tests::turning_indexing_on_after_a_walk_still_scans_the_drive` /
  `…::an_index_that_predates_the_exclusion_policy_is_dropped_before_the_next_walk`
- 2× `downloads::watcher::tests::latest_download_returns_ring_value_after_event`,
  `downloads::watcher::tests::note_pending_write_suppresses_matching_event`,
  `file_viewer::session_test::test_session_close_stops_watcher`,
  `importance::scheduler::walk_memory_tests::the_walk_does_not_allocate_per_folder_or_per_file`,
  `importance::scheduler::walk_memory_tests::the_walk_holds_a_small_fixed_record_per_folder`
- 1× `file_viewer::session_test::test_session_rotation_reopens_backend`,
  `file_viewer::session_test::test_session_emits_file_changed_on_append`,
  `error_reporter::tests::streaming_tests::streaming_stops_at_cap`,
  `indexing::lifecycle::cover::cold_drive_tests::a_branch_comes_back_when_the_volume_does`,
  `file_system::listing::caching_reaper_test::reaper_evicts_stale_listing_and_its_watcher_together`,
  `file_system::listing::caching_reaper_test::reaper_keeps_recently_touched_listing_even_if_created_long_ago`,
  `file_system::volume::backends::archive::watch_integration_test::lru_eviction_releases_the_archive_and_its_watch`,
  `file_system::volume::backends::local_posix_test::test_listing_is_watched_flips_with_watcher_lifecycle`

Two clusters dominate, and they suggest where structural work pays:

1. **`downloads::watcher` / `file_viewer::session` / `caching_reaper` / `local_posix` watcher tests** — real FSEvents
   delivery. `.config/nextest.toml` already sorts this family into self-healing (redo the mutation until delivered) and
   retry-carrying, and names dropping the retries as the goal.
2. **`walk_memory_tests` and `go_to_latest_*`** — pure compute and pure logic, which cannot race. They fail only because
   the global 8 s nextest cap is wall-clock, so a saturated machine starves them. Nothing about the TEST is wrong;
   `rust-test-contention.go`'s header documents that loosening the cap globally is the wrong fix because it costs every
   idle run its hang detector.
   - ⚪ **Accepted.** `rust-test-contention` already re-runs each of these alone at the unchanged deadline and reports
     the verdict, which is the right handling for wall-clock starvation of a test that cannot race. Nothing in the test
     is wrong, so there is nothing in the test to fix.

### ❌ `file_viewer::watcher_test::` — redo-until-delivered was tried and measured WORSE

The corpus's own "what would make the board deterministic" list asked for this by name, and `.config/nextest.toml` names
dropping these tests' `retries = 2` as the goal. **Don't retry it without reading this.** Two shapes were implemented
and benchmarked against the unchanged baseline on the same machine under the same synthetic saturation (2026-08-08, M3
Max, 32 spinning workers plus a concurrent `pnpm check`), six `cargo nextest run` loops each:

- **Baseline (unchanged):** 0/6 runs red, 0/6 runs needed a retry.
- **Redo the mutation** (the `downloads::watcher::observe_mutation` shape): worse.
- **Redo + per-attempt prime** (append until the watch proves live, then mutate): **3/6 runs RED**, 6/6 needed retries.

Why it doesn't transfer, despite working for `downloads::watcher` and `cmdr-archive`:

- Those watch a **directory** and redo on a **fresh name**, so every attempt is an independent create the debouncer must
  report. This watcher watches **one file** and `classify_and_emit` classifies by comparing against a REMEMBERED size
  and inode, updating them only when it runs. Redoing therefore mutates the very state the classification depends on: a
  second truncate of an already-empty file is `MetadataOnly`, not `Shrunk`, and a re-grow against a stale larger size
  reports `Shrunk` for what was actually a grow. Priming fixes the bookkeeping but adds continuous churn that measurably
  degrades delivery rather than healing it.
- The dominant failure here is **not** coalescing, which is what redo defeats. It's the FSEvents stream never arming:
  the observed signature is a first attempt seeing NOTHING for 15 s while the retry, in a fresh process, delivers in
  under two. There is no mutation to redo when nothing is listening yet, and priming is bounded by the same budget.

So the retries stay, and they stay honest: `rust-tests` parses nextest's `FLAKY n/m` lines and downgrades a
retry-rescued run to a warn naming each test. Anyone revisiting this should target the arming window (a readiness signal
out of the watcher manager that a test can wait on), not the mutation.

## Fixed this night — the model for specificity

Kept because each shows what a diagnosis looks like when it's finished.

1. **`dialog-inset.spec.ts` never called `recreateFixtures()`.** The whole dialog-layout sweep had been dead in every
   full E2E run since `66e82d3a7` took it out of capture-only builds: identical `ensureAppReady` failure on macOS,
   Linux, and Linux's retry, with the pane showing the conflict specs' fixtures.
2. **The same sweep then left the onboarding SOFT SHEET open.** It matches none of `dismissOverlay`'s five selectors,
   the `afterEach` leak guard probes those same five, and the wizard swallows Escape by design — so it leaked past both
   safety nets and ate the keystrokes of every later spec on the shard. That is the entire 11-failure Linux set of
   `cmdr-e2e-linux-1786169269` and the 4 macOS rename failures three specs downstream, all reading as saturation flake.
   Linux went from 11 failures to 0 once it was closed.
3. **`cmdr-fsevent-stream`'s `must_receive_fs_events`** performed ONE create/delete pair and asserted on whatever
   arrived 1 s later. macOS drops the mutation landing in a just-armed watch's window and coalesces a lone create+delete
   into one event; neither is recoverable by waiting, so it lost on an idle machine too.
4. **Two `cold_drive_tests` read a meta row the async `IndexWriter` hadn't written yet** (`cover` waits for coverage,
   not for the write). Immediate assertions, so headroom couldn't help; only the slower Docker Linux host lost.

## Structural levers the repo already has

Reach for these before inventing anything.

- **`cmdr_fs::testing::wait_until` / `wait_until_async`** — panic on timeout with the caller's description and (sync)
  file/line. Replaces every hand-rolled poll loop and fixed sleep. Pick the timeout as a backstop, never as a guess, and
  ❌ never enlarge one to fix a flake.
- **Redo the mutation until delivered** — the shape `.config/nextest.toml` calls self-healing, used by
  `downloads/watcher.rs::observe_mutation`, `cmdr-archive`'s `drive_until_refreshed`, and (as of this night) the fsevent
  fork's producer thread. The only known answer to an OS event source that can drop or coalesce, and the stated exit
  condition for the tests still carrying retries.
- **`FaultyVolume::fault_fired(op)`** (added this night) — assert the test actually REACHED its own subject. A fault the
  code routes around leaves a green test covering the opposite case, which is precisely how
  `smb_integration_unknown_source_type_never_clears_a_share_folder` read as a data-safety violation while measuring
  ordinary behavior. Generalizable well past `FaultyVolume`.
- **`flushFileWatcher(tauriPage)`** — re-reads every active listing through the Volume trait rather than draining the
  debouncer, so it works even when FSEvents hasn't delivered at all. The right tool after any external write in an E2E
  spec.
- **`recreateFixtures(getFixtureRoot())` in `beforeEach`** — mandatory for any spec running after a mutating one; the
  fixture tree is shared.
- **`ensureAppReady()`** — resets route, volume, and directories in that order. Does NOT recreate fixtures and does NOT
  close a soft sheet.
- **`expect.poll(...).toBeTruthy()`** over bare `pollUntil` (enforced by `bare-poll`), and by extension: never let an
  `if (x)` guard swallow a missing element before an action.
- **`TestDir`, `TestOperationGuard`, `operation_log::TestJournalGuard`** — scoped fixtures that clean up on unwind, so
  no test inherits another's leftovers.
- **`virtual_device_test_lock()` and the `real-notify` / `disk-image` / `smb-stress` nextest groups** — serialize tests
  that share one physical resource, instead of multiplying each other's starvation.
- **`rust-test-contention`'s two-stage re-run** — before treating any Rust failure as real, let it re-run alone. It has
  been right every time this night.

## What would make the board deterministic

The original list, with what the 2026-08-08 pass found. Kept scored rather than rewritten, because two of the four
predictions were wrong in instructive ways.

1. ✅ Poll in `selectItemsByName`, and assert-before-click in `app.spec:285`. **Both done**, and both grew a second
   safeguard the diagnosis hadn't asked for: `selectItemsByName` now asserts the selection it produced, and the
   swallowed click was a whole idiom (`clickEntryInPane` replaced it in `ensureAppReady` too, which is where it did the
   most damage).
2. ✅ Give `search-*` specs an explicit readiness signal. **Done for the one live spec**, but not by adding a signal:
   the ⌘N reset that already existed was simply missing. The two `search-walk-handoff` entries turned out to be 🟡, so
   the "highest-count cluster" was really one spec.
3. ❌ Apply redo-until-delivered to `file_viewer::watcher_test::`. **Tried twice, measured worse, reverted.** See the
   section above before repeating it.
4. ⚪ Decide what the local macOS lane should do about `retries: 0`. **Leave it.** Both lanes preserve signal today, in
   different ways: the Linux lane's retry-rescued pass is downgraded to a warn naming the spec (`e2e-flaky.go`), and the
   local lane's hard red is maximum visibility. Dropping the Linux retry would turn genuine shared-VM environment flake
   red, which is the thing the carve-out exists to prevent; adding a local retry would remove the fastest signal we have
   that a real race is back. The asymmetry isn't a gap, it's two lanes with different jobs.

### The biggest finding wasn't on the list

Half the E2E specs never restore the shared fixture tree, so any of them landing behind a mutating spec fails inside
`ensureAppReady` against the WRONG spec's fixtures. It never showed up as a ranked entry because it doesn't concentrate
on one test: it shows up as whichever spec happened to run next, which reads as saturation flake and is why the corpus
sorted sixteen such entries into an undiagnosed long tail. Reproduced deterministically; **not fixed** — five attempts
and their measurements are in § The shared-fixture leak, and that section is where the next attempt should start.

## Appendix: the full ledger, every spec and every shard-run it failed in

The union, not a sample: 40 specs over 48 shard-runs. `<lane>/<epoch>` identifies the shard-run, so a spec that failed
twice in one wall-clock run (two shards) shows both. Taken 2026-08-08; the source logs age out of `/tmp`.

- **[4]** `dialog-inset.spec.ts:214:3 › Dialog body inset › every dialog’s first body section lines up with its title`
  - nonmtp1/1786161532, nonmtp1/1786162024, nonmtp1/1786166838, linux/1786166836
- **[4]**
  `search-open-in-pane.spec.ts:280:3 › Search dialog: Open in pane › ⌘[ leaves the snapshot view, ⌘] returns to it`
  - nonmtp2/1786161532, nonmtp2/1786169270, nonmtp2/1786170974, nonmtp2/1786171558
- **[4]**
  `search-walk-handoff.spec.ts:122:3 › Search dialog: a walk that outlives its dialog › reopening the dialog shows the running search rather than its leftovers`
  - nonmtp2/1785970887, nonmtp2/1785971362, nonmtp2/1785972011, linux/1785973703
- **[4]**
  `search-walk-handoff.spec.ts:67:3 › Search dialog: a walk that outlives its dialog › keeps filling the pane, and says so in a toast`
  - nonmtp2/1785970887, nonmtp2/1785971362, nonmtp2/1785972011, linux/1785973703
- **[3]**
  `compress-basic.spec.ts:151:3 › Compress (⌥F5) › cancelling a compress leaves at worst a valid empty archive, never a torn file`
  - nonmtp1/1786043582, nonmtp1/1786162024, nonmtp1/1786166838
- **[3]**
  `file-operations.spec.ts:291:3 › MCP rename › non-autoConfirm opens the inline editor prefilled with newName, then Enter commits`
  - nonmtp1/1786017110, nonmtp1/1786169270, linux/1786169269
- **[3]**
  `mtp-cancel-volume-settled.spec.ts:108:3 › MTP cancel: settle gate keeps "Cancelling…" until BE quiets down › first cancel clears via settle, then immediately F8 again dispatches successfully`
  - mtp/1786017110, mtp/1786161532, linux/1786169269
- **[3]**
  `mtp-conflicts.spec.ts:99:3 › MTP cross-volume move conflicts › MTP-to-local move with overwrite replaces dest and removes source`
  - mtp/1785971362, mtp/1786043582, mtp/1786161532
- **[2]** `accessibility.spec.ts:200:5 › light mode › main explorer view`
  - linux/1785897736, linux/1785980952
- **[2]** `app.spec.ts:285:3 › Mouse interactions › switches pane focus when clicking other pane`
  - nonmtp1/1786162024, nonmtp1/1786173293
- **[2]**
  `archive-browsing.spec.ts:459:3 › Archive browsing › cancelling a paste into the archive leaves the zip contents intact`
  - nonmtp1/1786017110, nonmtp1/1786169270
- **[2]**
  `conflict-edge-cases.spec.ts:296:3 › Type mismatch conflicts › Copy with Overwrite All handles directory-over-file`
  - nonmtp1/1786017110, nonmtp1/1786170385
- **[2]** `file-operations.spec.ts:154:3 › Rename round-trip › renames file-a.txt to renamed-file.txt via F2`
  - nonmtp1/1786169270, linux/1786169269
- **[2]**
  `file-operations.spec.ts:214:3 › Rename round-trip › clicking inside the editor moves the caret instead of ending the rename`
  - nonmtp1/1786169270, linux/1786169269
- **[2]** `file-operations.spec.ts:249:3 › Rename round-trip › clicking another row saves the typed name`
  - nonmtp1/1786169270, linux/1786169269
- **[2]** `file-watching.spec.ts:198:3 › File watching › handles deletion of the watched directory`
  - nonmtp2/1785971362, nonmtp2/1786043582
- **[2]**
  `focus-trap.spec.ts:85:3 › Dialog focus trapping › command palette: Escape still closes when focus has escaped`
  - nonmtp2/1785884233, linux/1786169269
- **[2]**
  `mtp-copy-preflight-uses-cache.spec.ts:128:3 › MTP copy pre-flight reuses watcher-backed listing › F5 pre-flight scan completes from cache and reports the right file count`
  - linux/1786169269, mtp/1786171558
- **[1]**
  `archive-browsing.spec.ts:173:3 › Archive browsing › pressing Enter on a zip lists its inner entries with a transparent path`
  - nonmtp1/1786162024
- **[1]**
  `archive-browsing.spec.ts:212:3 › Archive browsing › a real directory named like a zip enters as a plain folder`
  - nonmtp1/1786170974
- **[1]**
  `archive-browsing.spec.ts:245:3 › Archive browsing › pressing Enter on a text file inside the archive opens the viewer (not a dead-end)`
  - nonmtp1/1785883303
- **[1]**
  `archive-browsing.spec.ts:397:3 › Archive browsing › deleting a file inside the archive is permanent (no Trash) and removes it`
  - nonmtp1/1786017110
- **[1]** `archive-browsing.spec.ts:426:3 › Archive browsing › pasting a file into the archive lands it inside the zip`
  - nonmtp1/1785893388
- **[1]**
  `archive-browsing.spec.ts:516:3 › Archive browsing › moving a file OUT of the archive removes it from the zip and lands it locally`
  - nonmtp1/1786169270
- **[1]**
  `archive-browsing.spec.ts:606:3 › Archive Enter-behavior menu › Enter on a zip set to Ask shows the menu; Browse steps inside`
  - nonmtp1/1786043582
- **[1]**
  `archive-browsing.spec.ts:622:3 › Archive Enter-behavior menu › Enter then Down then Enter picks Open, launching the zip in the default app`
  - nonmtp1/1786017110
- **[1]**
  `archive-browsing.spec.ts:650:3 › Archive Enter-behavior menu › a zip set to Browse skips the menu and enters directly`
  - linux/1785798734
- **[1]**
  `conflict-dialog-matrix.spec.ts:147:3 › Single clash: baseline file/folder smoke › file→file Skip keeps dest bytes`
  - nonmtp1/1786173293
- **[1]**
  `conflict-dialog-matrix.spec.ts:402:3 › Bucket spread: mixed independent buckets › normal and file→folder buckets latch independently`
  - nonmtp1/1786043582
- **[1]** `conflict-edge-cases.spec.ts:150:3 › Edge cases › Sequential copy triggers conflict on second attempt`
  - nonmtp1/1785826047
- **[1]** `conflict-move.spec.ts:116:3 › Move rollback › Move rollback button is available and cancels operation`
  - linux/1786173272
- **[1]**
  `conflict-move.spec.ts:76:3 › Move multi-item merge (Layout B) › Move multi-item with Skip preserves source of skipped files`
  - nonmtp1/1786017110
- **[1]**
  `conflict-overwrite-conditional.spec.ts:123:3 › Conditional conflict policies (upfront radios) › Overwrite all older: only strictly-older dest is replaced`
  - nonmtp1/1786017110
- **[1]**
  `conflict-overwrite-conditional.spec.ts:150:3 › Conditional conflict policies (per-file dialog buttons) › "Overwrite all older" button in per-file dialog applies to all remaining conflicts`
  - nonmtp1/1786017110
- **[1]**
  `conflict-overwrite-conditional.spec.ts:98:3 › Conditional conflict policies (upfront radios) › Overwrite all smaller: only strictly-smaller dest is replaced`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:322:3 › MCP rename › autoConfirm renames directly, no editor`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:348:3 › MCP delete mode › mode: delete presets the confirmation dialog to permanent`
  - nonmtp1/1786017110
- **[1]**
  `file-operations.spec.ts:380:3 › MCP named create › mkdir autoConfirm creates in the freshly-navigated dir, not a stale one`
  - nonmtp1/1786017110
- **[1]**
  `file-operations.spec.ts:397:3 › MCP named create › mkfile autoConfirm creates an empty file, honest conflict on a duplicate`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:415:3 › Create folder round-trip › creates a new folder via F7 and verifies on disk`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:443:3 › Create folder round-trip › cursor lands on the newly created folder`
  - nonmtp1/1786017110
- **[1]**
  `file-operations.spec.ts:482:3 › New file round-trip › creates a new file via the file.newFile command and lands the cursor on it`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:540:3 › View mode toggle › switches between Brief and Full view modes`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:563:3 › Hidden files toggle › toggles hidden file visibility`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:605:3 › Command palette › opens, shows results, and closes with Escape`
  - nonmtp1/1786017110
- **[1]** `file-operations.spec.ts:655:3 › Empty directory › shows empty right pane gracefully without crash`
  - nonmtp1/1786017110
- **[1]**
  `file-watching.spec.ts:312:3 › File watching › respects hidden file visibility for externally created dotfiles`
  - nonmtp2/1785971362
- **[1]**
  `focus-trap.spec.ts:61:3 › Dialog focus trapping › command palette: programmatically leaked focus is pulled back`
  - nonmtp2/1785971362
- **[1]**
  `media-index-slider.spec.ts:119:3 › Image-index importance slider › persists a new level and updates its preview label`
  - nonmtp2/1786170385
- **[1]**
  `mtp-conflicts.spec.ts:185:3 › MTP cross-volume move conflicts › local-to-MTP move with overwrite replaces MTP file`
  - mtp/1785971362
- **[1]**
  `mtp-conflicts.spec.ts:242:3 › MTP same-volume move conflicts › same-volume MTP move with overwrite replaces dest`
  - mtp/1785971362
- **[1]**
  `mtp-delete-no-double-scan.spec.ts:112:3 › MTP delete reuses scan preview (no double scan) › F8 progresses Scanning -> Deleting exactly once and counts never go backwards`
  - linux/1786169269
- **[1]** `mtp.spec.ts:502:3 › MTP file operations › deletes multiple selected files on MTP`
  - linux/1786169269
- **[1]** `mtp.spec.ts:641:3 › MTP rename › renames file on MTP via keyboard`
  - linux/1786169269
- **[1]** `mtp.spec.ts:677:3 › MTP rename › rename to existing name is rejected on MTP`
  - linux/1786169269
- **[1]** `mtp.spec.ts:804:3 › MTP clipboard rejection › Cmd+C on MTP file shows rejection toast`
  - mtp/1786017110
- **[1]**
  `onboarding.spec.ts:110:3 › Onboarding wizard re-entry › re-entry is idempotent (re-dispatch while open is a no-op)`
  - nonmtp2/1785883303
- **[1]**
  `search-live.spec.ts:41:3 › Search dialog: a live search over unindexed ground › walks the folder, streams what it finds, and says the run covered it`
  - linux/1785973703
- **[1]**
  `search-open-in-pane.spec.ts:236:3 › Search dialog: Open in pane › Open in pane lands the right pane on a search-results snapshot`
  - nonmtp2/1785826047
- **[1]**
  `search-open-in-pane.spec.ts:269:3 › Search dialog: Open in pane › ⌘[ leaves the snapshot view, ⌘] returns to it`
  - nonmtp2/1785826047
- **[1]**
  `search-recent.spec.ts:31:3 › Search dialog: recent searches › Open-in-pane persists the query to the backend recent-search store`
  - linux/1785973703
