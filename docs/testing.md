# Testing playbook

How we test Cmdr. Decision rules, anti-patterns, and a per-feature checklist. If you're adding tests, read this first.

The companion file `tooling/testing.md` is the tools inventory (one paragraph per tool).

## Test pyramid

We prefer broad-shallow unit coverage, narrow-deep integration coverage, and a small number of end-to-end flows. Each
layer catches different bugs:

| Layer        | Catches                                               | Cost per test | Where                                              |
| ------------ | ----------------------------------------------------- | ------------- | -------------------------------------------------- |
| Unit (Rust)  | Algorithmic bugs, state-transition bugs               | ms            | `mod tests` in the same file                       |
| Unit (TS)    | Component logic, store behavior, pure-fn correctness  | ms            | `*.test.ts` next to the source                     |
| Integration  | Cross-module flows that need real fixtures (DB, fs)   | seconds       | `apps/desktop/src-tauri/tests/`                    |
| IPC contract | Serde-shape drift, command-rename drift, side effects | seconds       | `apps/desktop/src/lib/ipc/*.test.ts` via `mockIPC` |
| E2E          | Cross-component flows (focus, keyboard, dialog stack) | minutes       | `apps/desktop/test/e2e-playwright/*.spec.ts`       |
| Tier-3 a11y  | Component-level ARIA, labels, focus order             | ms            | `apps/desktop/src/**/*.a11y.test.ts`               |

Default to the lowest layer that can express the property you want to check. E2E is the most expensive lane; don't push
work into it that a unit test would cover.

## What a test actually costs

Read this before proposing to delete tests to make a lane faster. In both unit lanes the cost sits in **per-file and
per-lane fixed work**, not in the number of tests, so deleting assertions buys close to nothing.

- **`svelte-tests`**: one test FILE costs about as much as **91 test bodies**. Of a full run's worker CPU, 11% is test
  bodies (103.9 s) and 89% is per-file fixed work: module import 603.8 s, environment 167.1 s, transform 57.9 s, setup
  files 20.7 s, over 826 files and 9,209 tests. A slice of 97 one-and-two-test files measured the same shape: 9.3 s of
  the lane's 66 s wall clock to run 1.6% of the suite. (Measured 2026-08-19, M-series laptop, `pnpm exec vitest run`
  reporter totals.)
- **`rust-tests`**: all 6,166 tests EXECUTE in 23.3 s wall clock; the check's ~54 s is mostly cargo's freshness and link
  work, which no test change touches. The slowest single test (5.5 s) sets a floor nothing below it can lower. (Measured
  2026-08-19, `cargo nextest run --workspace`.)

Consequences for anyone tuning a lane:

- **Deleting N average tests saves ~N × 11 ms of CPU** in `svelte-tests` and ~N × 4 ms in `rust-tests`, divided again by
  the lane's parallelism. A hundred of them is under a second of wall clock. "Replace 100 cheap tests with 1" is not a
  speed lever here.
- **The levers that do move a lane** are: fewer test FILES (frontend), and the handful of tests that wait on real time.
  Rank the latter with `~/cmdr-test-log.csv`; a test that appears there on nearly every run is consistently slow, one
  that appears a few times only goes slow under load.
- **A slow test that waits out a production constant is not waste.** `busy_db_is_retried_not_deleted` (5.5 s) waits out
  SQLite's real 5 s `busy_timeout` and `a_slow_first_attempt_spends_the_retry_budget` (2.1 s) waits out the real
  `CONNECT_RETRY_BUDGET`; the duration IS the assertion. Only a sleep the TEST invented (a fake slow closure) is fair
  game, and then keep a loud margin over the timeout it has to outlast.

## Decision table: what tool for what test

- **Pure function with edge cases**: `proptest` (Rust unit). State a property, fuzz inputs.
- **Pure function with a few specific inputs**: Plain example tests in `mod tests`
- **Behavior coverage of an existing tested function**: `cargo mutants` survivor triage: every survived mutant is a
  behavior-level gap
- **State machine transition**: Rust unit test, **drive via the public interface**, not by setting the atomic directly
- **A scratch directory in a Rust test**: `crate::test_support::TestDir` (`cmdr_fs::testing::TestDir` from another
  crate), see § "Scratch directories (Rust)". **Never** `std::env::temp_dir().join("cmdr_something")`
- **Wait for background work in a Rust test**: `crate::test_support::wait_until` (sync) / `wait_until_async`
  (`#[tokio::test]`), see § "Waiting for background work (Rust)". **Never** a hand-rolled poll loop or a fixed sleep
- **`#[tauri::command]` boundary**: vitest IPC contract test using `installIpcMock()` from
  `apps/desktop/src/lib/ipc/test-helpers.ts`
- **Frontend component logic**: vitest + svelte-testing-library in `*.test.ts`
- **A component that sizes itself from its container** (either file-list view, `ShareBrowser`, `NetworkBrowser`):
  `installLayoutMock()` from `$lib/test-layout`, and for `FullList` the ready-made `mountFullList()`. **Never** assert
  on rows without one — see § "A component that measures itself, rendering nothing"
- **Component-level a11y (ARIA, labels, focus order)**: tier-3 a11y test in `*.a11y.test.ts`
- **Keyboard shortcut opens a dialog**: E2E spec, use `dispatchMenuCommand(tauriPage, 'file.copy')`. **Never** synthetic
  F-key press unless the test exists to verify the keyboard pathway
- **Wait for UI state to change in E2E**: `expect.poll(async () => …, { timeout }).toBeTruthy()` (preferred — wait fuses
  with assertion); `expect(await pollUntil(...)).toBe(true)` for the few non-Playwright contexts. **Never** bare
  `await pollUntil(...)` (silent timeout) or `await sleep(N)` (flaky)
- **UI that reacts to a backend event** (indexing phases, walked branches, freshness): E2E spec driving
  `emitBackendEvent(tauriPage, '<event>', payload)`. **Never** a timing-based assertion against the real work — emit the
  terminal event to clean up, and use an id nothing real can claim
- **Cross-component flow (return-focus, dialog stack, navigation)**: E2E (Playwright)
- **Storage volume operation (MTP, SMB)**: Integration test against a virtual fixture (virtual-mtp feature, Docker SMB
  containers)

## Scratch directories (Rust)

`TestDir` is the only sanctioned way for a Rust test to get a directory to write in. It lives in `cmdr_fs::testing`
beside the wait helpers, and the app re-exports it, so app tests use the short path and another crate's tests dev-depend
on `cmdr-fs` with `features = ["testing"]`:

```rust
use crate::test_support::TestDir;

let dir = TestDir::new("listing_sort");
fs::write(dir.join("a.txt"), b"x").unwrap();
let volume = LocalPosixVolume::new("Test", &dir);
```

- **The handle owns the directory.** It's removed when the handle drops, unwind included, so a failing test cleans up
  after itself. Keep it bound for as long as you need the files (`let dir = …`, never `let _ = …`): a `_` binding drops
  immediately and takes the directory with it. A helper that builds a file inside one has to hand the `TestDir` back
  alongside the path (`error_reporter/tail_walker.rs::write_tmp` returns `(TestDir, PathBuf)`), or a struct fixture has
  to hold it in a `_dir` field (`archive/volume_test.rs::TestArchive`).
- **`label` is cosmetic**: it names the directory readably in `/tmp` while it lives. Don't add a PID, a UUID, or a
  thread id to it; the random suffix already covers uniqueness, and a hand-rolled one only makes the name longer.
- It derefs to `Path` **and** implements `AsRef<Path>`, so `dir.join(…)`, `dir.to_string_lossy()`, and a generic
  `impl AsRef<Path>` parameter all work. Both impls are load-bearing; the type's doc comment says why.

❌ **Never build a fixture path from a compile-time constant** (`std::env::temp_dir().join("cmdr_list_test")`). That
path is shared by every process on the machine, and it costs three ways:

1. **Cross-process collision.** Two suite runs at once (parallel worktrees, or CI beside a local run) get the same
   directory, and whichever calls `remove_dir_all` first deletes the other's live fixture mid-test. nextest's
   process-per-test does NOT isolate this: processes share the filesystem.
2. **Cross-run contamination.** A run that doesn't clean up leaves the next one a pre-populated directory, so "the
   listing has three entries" can pass on leftovers and go red later for reasons nobody can reproduce. It also hides
   weak assertions: `listing/operations_test.rs` asserted `entries.len() >= 3` because an exact count was never safe.
3. **No cleanup on panic.** Teardown written as a `remove_dir_all` after the assertions never runs when an assertion
   fails, which is exactly when the debris matters most.

Enforced by `fixed-temp-dir`, which scans test code only: a dedicated test file, a `*test_support*` / `*test_fixtures*`
helper module, or the body of a `#[cfg(test)] mod` inside a production file. Opt a deliberate site out with
`// allowed-fixed-temp-dir: <reason>` on the line above or as a trailing comment; a directive that stops matching
anything is reported as orphaned, so the opt-outs can't quietly outlive their reason.

Two exceptions stay on a raw OS-temp path deliberately, and both name their reason in a comment:
`updater/installer.rs`'s `staging_dir_sits_under_temp_dir` asserts on the path itself, and
`git/test_fixtures.rs::temp_dir` is already process-and-run unique (PID plus a nanosecond stamp) and keeps its directory
on panic on purpose, for post-mortem inspection. Production code that stages into the OS temp dir (the updater, the icon
sample files, `smb_smbclient`'s auth file, `write_operations/scratch_dir.rs`) is correct as-is and is not test
scaffolding.

## Waiting for background work (Rust)

`cmdr_fs::testing` is the sanctioned way for a Rust test to wait, and the only place in Rust test code that sleeps. The
app re-exports both helpers as `crate::test_support`, so app tests keep the shorter path; another crate's tests
dev-depend on `cmdr-fs` with `features = ["testing"]` and import `cmdr_fs::testing::…`. Two flavors, same shape:

```rust
use crate::test_support::{wait_until, wait_until_async};

// Sync `#[test]`:
wait_until(Duration::from_secs(2), "the upgrade to LineIndex to finish", || {
    upgraded_to_line_index(&sid)
});

// `#[tokio::test]`:
wait_until_async(Duration::from_secs(5), "recovery to reopen the device", || {
    connection_manager().is_connected(&device.id)
}).await;
```

- Both **panic** on timeout with the caller's description and (for the sync one, via `#[track_caller]`) the caller's
  file and line. Neither returns anything, so a wait can't silently pass the way a bare `bool` helper can.
- Phrase `description` as a noun phrase: it completes "timed out after 2.0s waiting for …".
- Pick the timeout as a **backstop**, not a guess: far above the real work, so a trip means a regression rather than a
  loaded machine. Never enlarge one to fix a flake; find the missing condition instead.
- **A `wait_until` longer than the test's nextest cap is dead code.** The global cap is 8 s (`.config/nextest.toml`),
  and nextest SIGKILLs at it, so a 30 s in-test wait under the default cap can never fail with its own message: the test
  just dies saying nothing. Writing a generous wait means also giving the test an override whose period exceeds that
  wait, so the in-test deadline stays the authoritative one and the cap stays a hang backstop.
- ❌ Don't call the sync one from an async test: `std::thread::sleep` blocks the runtime worker and deadlocks a
  current-thread scheduler. `wait_until_async` measures on tokio's clock, so a `start_paused` runtime auto-advances
  through the wait.
- ❌ Don't hand-roll a poll loop, and ❌ don't sleep a fixed span hoping the work landed. A test that genuinely needs a
  fixed wall-clock wait (fake latency in a stub, a negative assertion over a window, a test whose subject IS a debounce)
  carries an `// allowed-test-sleep: <reason>` comment on the line directly above (or trailing) the sleep. The
  `test-sleep` check enforces this: an undirectived sleep in test code fails, and an `// allowed-test-sleep:` that
  excuses nothing fails as an orphan.
- **If the subject takes its clock as an argument, pass a future instant; ❌ never shrink a window to sleep less.** An
  `allowed-test-sleep` is only earned once no clock seam exists. Shortening a production window so the test outlives it
  quickly makes the SETUP race too: the live throttle test ran a 150 ms window and 50 rewrites, and on a loaded CI
  runner the window elapsed mid-loop, one rewrite re-applied as a fresh leading edge, and a correct throttle read as a
  broken one. Keep the production-length window and move the clock instead (`reconciler::sweep_throttle(&writer, now)`),
  so both halves are independent of how long the setup took.
- **The predicate must be a pure, cheap READ** — the helper runs it every 5 ms. A predicate that takes write locks can
  sabotage the very work it waits for: `media_index/scheduler/kick_tests.rs::has_enriched_row` used to re-open a full
  `MediaStore` (a write connection: WAL conversion + `CREATE TABLE IF NOT EXISTS`) per poll, and SQLite's lock-upgrade
  deadlock check — which BYPASSES `busy_timeout` — made the enrichment writer's upsert fail with SQLITE_BUSY, silently
  dropping the row the wait was polling for. Probe SQLite state through a read-only connection (`open_read_connection`),
  never by re-opening the store.

## Anti-patterns

These are paid for in lost hours. Don't recreate them.

### ❌ `await sleep(N)` in E2E specs

E2E tests routinely re-find that 80% of wall-clock can be fixed sleeps. Every `sleep()` is a margin that's either too
tight (flake) or too loose (slow). Always replace with a condition:

```ts
// ❌ Don't:
await tauriPage.keyboard.press('F5')
await sleep(2000)
expect(await tauriPage.isVisible('[data-dialog-id="transfer-confirmation"]')).toBe(true)

// ✅ Do:
await tauriPage.keyboard.press('F5')
await tauriPage.waitForSelector('[data-dialog-id="transfer-confirmation"]', 5000)
```

For "wait until X is true" where X isn't a selector, use Playwright's `expect.poll`:

```ts
await expect
  .poll(async () => tauriPage.evaluate<number>(`document.querySelector(…)?.offsetHeight ?? 0`), { timeout: 5000 })
  .toBeGreaterThan(0)
```

The `cmdr/no-arbitrary-sleep-in-e2e` ESLint rule flags `await sleep(N)`. Opt out per-line with
`// eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- <reason>` only when there's a genuine fixed-duration wait
(e.g., watcher debounce settling), and even then, prefer a poll if any state changes.

The per-test wall-clock budget is 2 s, defended automatically: after every E2E run (`desktop-e2e-playwright` and
`desktop-e2e-linux`), the check runner flags any test over it (warn-only, per platform) against
`scripts/check/checks/e2e-duration-allowlist.json`. If your new test trips it, speed the test up; an allowlist entry
needs a reason and David's OK. See `scripts/check/checks/DETAILS.md` § "E2E test duration flagger".

### ❌ Bare `await pollUntil(...)` in E2E specs

The legacy `pollUntil` helper (and its wrappers `pollFs`, `pollUntilValue`, `pollActiveMode`, `pollOverlayGone`,
`pollFocusedPane`) returns `false` on timeout instead of throwing. A bare expression statement discards the return — if
the condition never holds, the test passes green so long as no later `expect` happens to catch it. We discovered 187
sites of this pattern across 20 specs; several tests had **zero** `expect()` calls and literally could not fail. One
viewer test wasted 5 seconds polling for a toast that never appeared in its window (no `ToastContainer` mounted there)
and still passed because the next `expect` was happy.

```ts
// ❌ Don't: timeout returns false, no one checks it, test stays green
await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, dirName), 2000)

// ✅ Do (preferred — wait fuses with the assertion, fails loudly on timeout):
await expect.poll(async () => fileExistsInFocusedPane(tauriPage, dirName), { timeout: 2000 }).toBeTruthy()

// ✅ Also fine (keeps the helper for non-Playwright contexts):
expect(await pollUntil(tauriPage, async () => fileExistsInFocusedPane(tauriPage, dirName), 2000)).toBe(true)

// ✅ Also fine (when you want to act on the false branch instead of failing):
if (!(await pollUntil(tauriPage, async () => isReady(tauriPage), 3000))) {
  throw new Error('listing did not refresh within 3 s')
}
```

Enforced by the `bare-poll` Go check (fast lane, ~9 ms warm; scans `apps/desktop/test/`). Opt out for genuine
best-effort cleanups (dismissing an overlay that might or might not be there) with `// allowed-bare-poll: <reason>` on
the line above or as a trailing comment on the same line. The full design rationale is in
`apps/desktop/test/e2e-playwright/DETAILS.md` § "`pollUntil` is silent on timeout" and `scripts/check/checks/DETAILS.md`
§ `bare-poll`.

### ❌ Synthesized F-key dispatches for tests that care about the resulting dialog

Synthetic `KeyboardEvent`s race against handler attachment under parallel-shard load. If your test asserts on the
_dialog that opens_, not on the keyboard pathway itself, use `dispatchMenuCommand`:

```ts
// ❌ Don't (unless you're testing the keyboard pathway):
await tauriPage.keyboard.press('F5')
await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)

// ✅ Do (when the test is about the Copy dialog, not F5):
await dispatchMenuCommand(tauriPage, 'file.copy')
await tauriPage.waitForSelector(TRANSFER_DIALOG, 5000)
```

Keep one or two dedicated tests on the keyboard pathway (`app.spec.ts` has these, with names like "opens copy dialog
with F5"). The rest should use `dispatchMenuCommand`.

### ❌ Direct atomic / store mutation in state-machine tests

A state-machine test that does `state.intent.store(OperationIntent::RollingBack)` is testing nothing: it bypasses the
validation guard the public function performs. Drive through the public interface:

```rust
// ❌ Don't:
state.intent.store(OperationIntent::RollingBack as u8, Ordering::SeqCst);
assert!(can_transition_to_stopped(&state));

// ✅ Do:
cancel_write_operation(&app, op_id, CancelMode::Rollback).await?;
let intent = state.intent.load(Ordering::SeqCst);
assert_eq!(intent, OperationIntent::RollingBack as u8);
```

If the public function takes `AppHandle` that you can't fixture-up cheaply, extract a pure inner helper and test that
through the public-via-helper path. Don't reach past the guard.

### ❌ Calling a walk-everything global mutator from a test

**The rule:** a unique per-test key isolates a test that touches ONE entry of a process-global registry. It does nothing
for a function that walks the WHOLE registry (`cancel_all_write_operations`, a `clear()`, a "stop everything" teardown
hook). Under plain `cargo test` the crate's tests share one process, so such a call reaches into whatever the tests
running beside it own.

**Why it's expensive:** the failure lands in the VICTIM, not the culprit, and its membership shifts with co-scheduling,
so it reads as environment flake. Fix it by giving the walk a scope the test owns (make the registry a struct, keep the
global as one instance of it, and let the test build its own), never by serializing the suites behind a mutex,
`#[ignore]`ing the victim, or loosening the victim's assertion. Worked example, including how the global wiring stays
covered: `apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "Test isolation for
`WRITE_OPERATION_STATE`".

### ❌ `retries: 1` to mask a race

Retries hide bugs. If a test flakes, find the race and fix it (Rust IPC race, missing await, watcher debounce, etc.).
Drop retries when the cause is gone.

**Carve-out — CI-only, for load-induced environment flake on the shared Docker VM.** The Playwright config sets
`retries: process.env.CI ? 1 : 0`. This is allowed because the Linux Docker lane runs every spec sequentially on a host
that also builds the app, so a busy host can stretch a `waitForSelector` / nav wait past its budget independently of any
app-level race. Local dev stays at zero retries, so a real race still surfaces immediately rather than being papered
over. Playwright marks a retried-pass as `flaky` in its `list` reporter, so the retry stays a tracked, visible event,
not a silenced one. The anti-pattern above still stands for masking a real race in app/IPC code — the carve-out is
narrow: CI-only, environment flake, signal preserved.

**Carve-out, Rust: named real-FSEvents tests only.** `.config/nextest.toml` grants `retries` to a filtered set of tests
that block on a SINGLE real OS watch delivery, where a coalesced or dropped event is unrecoverable within the run (the
override comment names them and states the exit condition: restructure them to redo the mutation, then drop the
retries). Same three conditions as above: narrowly filtered, environment flake rather than an app race, signal
preserved. Signal preservation is the part that's easy to lose, because nextest exits 0 on a retry-rescued run, so a
green suite can hide the exact flake the retries exist to tolerate. `rust-tests` therefore parses nextest's `FLAKY n/m`
lines and downgrades such a run from pass to **warn**, naming each test and the attempt that rescued it. Adding retries
without that reporting is the anti-pattern, not retries themselves.

### Reading a red `rust-tests` run

Two different things produce a red run and they need opposite fixes, so `rust-tests` prints a **diagnosis** above the
raw output that sorts every failing test into:

- **Killed at the nextest cap**: nextest terminated the process at `slow-timeout`, so the test never finished and left
  no panic. Look for a genuine hang, or starvation under load.
- **Blew its own in-test `wait_until` deadline**: the test's own deadline expired below the cap, and the diagnosis
  quotes the wait's description. Raising the nextest cap does nothing here; raise or load-scale the wait instead.
- **Leak**: assertions passed, but a handle or process outlived the test.
- **Ordinary assertion or panic.**

Reach for the cap only when the first class shows up. Guessing between these is how a deflaking pass ends up tuning the
knob that wasn't binding. The parsing lives in `scripts/check/checks/rust-test-diagnostics.go`.

### A red run re-runs its failures alone before believing them

On a saturated machine the 8 s cap kills CPU-bound tests that pass easily on an idle one. Measured 2026-07-29 on an M3
Max at load ~198, a full run produced 13 failures, nine of them cap kills of pure-compute tests
(`find_newlines_utf8_matches_memchr`, `walk_memory_tests::*`, `tar_each_codec_round_trips_a_file`). Nothing was wrong
with those tests; they could not get 8 s of wall-clock while 200 threads fought over 16 cores.

❗ **That result is the opposite of the usual intuition, so measure before restructuring anything.** The natural guess
is that every offender is a watcher, debounce, or lock test, and under saturation it is false: the dominant failures
were pure compute (a memchr comparison, allocation-counting walks, codec round-trips), which no test restructuring can
help — a test needing 0.1 s of CPU cannot finish in 8 s of wall-clock when 200 threads share 16 cores. A deflaking pass
that starts from a hand-picked list of "obviously timing-shaped" tests fixes tests that were not the problem.

Loosening the cap globally would cost every idle run its hang detector, so `rust-tests` instead re-runs **only** the
failing tests, alone, and lets the outcome speak:

- **Passes alone at the unchanged deadline** → the suite was starving it. Contention, not a defect.
- **Fails alone, passes with headroom, machine quiet** → real slowness. Tweak the test or give it an explicit per-test
  override. This one fails the run.
- **Fails alone, passes with headroom, machine still busy** → inconclusive. Starvation and slowness can't be told apart,
  so neither is claimed; re-run on a quiet machine to settle it.
- **Fails alone even with headroom** → a genuine failure. Always red, whatever the load.

Only the first and third soften the result, and only to a **warn**, never a pass. Load is never the gate: the isolated
re-run is. Load enters at exactly one point, demoting the "needed headroom" verdict to inconclusive, because that is the
only verdict whose meaning depends on a quiet machine.

The re-run is capped at 15 tests. Past that the machine was too loaded for any of it to mean anything, and the output
says so rather than quietly examining a subset. Mechanics and the two nextest profiles it drives:
`scripts/check/checks/rust-test-contention.go`.

All three Rust lanes get this: `rust-tests`, `rust-integration-tests`, and `rust-tests-linux`. The Docker lane re-runs
inside the same container the failing run used, at the same deadlines. It's the lane most exposed to starvation (its
cores are a slice of a host that may also be running both E2E lanes and a second container), and the deadlines stay
identical on purpose: a container-only cap bump would hide the Linux-only slowness the lane exists to catch.

**The E2E suites deliberately get no contention re-run.** Playwright runs `workers: 1` with `fullyParallel: false`, so
there is no intra-suite parallelism for a serialized probe to remove: the probe stage would be indistinguishable from
the original run, and every verdict it produced would be noise dressed as a finding. The Rust mechanism works precisely
because that suite is massively parallel. E2E gets the retry-pass warn below instead.

### Playwright retry-passes warn too

Both E2E lanes apply the same rule as the Rust suite: a spec rescued by its retry is a flake, not a pass, so the run is
downgraded to a **warn** naming every rescued spec. This matters most on the Linux lane, which runs with `CI=true` and
therefore inherits `retries: 1`. The verdict reads Playwright's structured JSON report (`stats.flaky` and each test's
`expected`/`unexpected`/`flaky`/`skipped` status), not the `list` reporter's text. A genuinely failing spec is
`unexpected`, not `flaky`, so it stays a failure and isn't double-counted. Mechanics:
`scripts/check/checks/e2e-flaky.go`.

### Every red or slow test lands in a log you can rank

Both verdicts above are per-run: they tell you this run went red, not which tests go red most weeks.
`~/cmdr-test-log.csv` answers that. All three Rust lanes, `svelte-tests`, and both E2E lanes append one row per
individual test, on the red path as well as the green one, carrying its status (including `flaky` and `timeout`),
duration, and attempt. Fast clean passes are dropped so the file stays small, so absence means "fast, or never ran",
never "passed". Schema, covered lanes, and ready-made ranking queries: `scripts/check/DETAILS.md` § "The per-test log".

### Caps are not runtimes

A per-test `slow-timeout` in `.config/nextest.toml` is a hang backstop, typically 20-50x the real runtime. Don't quote
one as what a test takes. Measured 2026-07-29 on an idle M3 Max: the SMB test carrying the largest cap (130 s) runs in
**2.8 s**, and the whole 53-test integration suite finishes in **5.3 s** wall-clock.

❗ The inference doesn't run the other way either, and reading a cap as a comfortable margin is what produced a wrong
deflaking analysis once. On the same machine, `find_newlines_utf8_matches_memchr` takes **3.3 s alone on an idle
machine** against the default 8 s cap: a 2.4x margin, not the 10x the cap suggests, which is why it is one of the first
tests starvation kills. And the headline offender of that analysis, `dropping_a_file_emits_one_event`, was already on a
20 s cap with `real-notify` serialization, so its 17.75 s burn was against 20 s rather than the assumed 8 s. **Measure
the test; never derive either direction from the cap.**

### ❌ Raw `tauri::invoke('command_name', …)` outside the typed bindings

Use `commands.commandName(args)` from `apps/desktop/src/lib/ipc/`. Enforced by `cmdr/no-raw-tauri-invoke` ESLint rule
and the local `bindings-fresh` check (it runs in `pnpm check` on macOS, not in CI — the committed `bindings.ts` is the
macOS command surface, which a Linux runner can't reproduce; see `docs/tooling/ci.md` § the registry ↔ CI contract).

### ❌ Substring-matching error messages or state labels

Use typed enum variants, not `err.message.includes('not found')`. Enforced by `cmdr/no-error-string-match` (TS) and the
`error-string-match` check (Rust).

### ❌ Layering a "skip build if hash matches" wrapper over `pnpm tauri build`

Cargo / Vite / `beforeBuildCommand` already cache. Wrapping risks shipping stale binaries. See AGENTS.md.

### ❌ `requestAnimationFrame` in unfocused windows (readiness markers, deferred closes)

**The rule:** never gate anything a test (or another window) waits on behind `requestAnimationFrame` in a window that
can open without focus. Use `setTimeout(0)` for "defer to the next event-loop tick".

**Why:** macOS WKWebView throttles — and under occlusion fully starves — rAF in windows that aren't focused. E2E
deliberately opens the viewer and settings windows with `focus: false` (so test runs don't steal the developer's
keyboard), which means an rAF-gated signal in those windows fires late or never whenever ANY other window has focus. The
failure looks like environment flake: specs time out only under host load or while a human uses the machine, membership
shifts run to run, Linux (Xvfb, no occlusion) stays green, and reruns "fix" it.

**Recurrences (why this entry exists):**

1. Settings window deferred close — two nested rAFs pushed the close past the E2E budget
   (`routes/settings/+page.svelte`, see `lib/settings/DETAILS.md` § Escape-close gotcha).
2. Viewer window deferred close — same shape (`routes/viewer/+page.svelte::closeWindow`).
3. Viewer `windowReady` / `data-window-ready` marker — an rAF kept the attribute on `"loading"` in unfocused E2E
   windows, timing out every viewer spec whenever the developer was at the keyboard. Cost a full evening of "load flake"
   forensics before the pattern was recognized as this same bug, third time around.

**How to spot the next one:** symptoms are E2E timeouts on `waitForSelector`/window-ready markers that correlate with
human presence at the machine and vanish on idle hosts. Grep the involved window's code for `requestAnimationFrame`
before blaming load. Legitimate rAF uses (animation, paint-coupled measurement like the drag-autoscroll loop) are fine —
those want frames; readiness/lifecycle signals don't.

### ❌ A no-op / empty fixture that passes for the wrong reason

**The rule:** when a test asserts "nothing happened" (zero writes, no diff, no change), also assert that the code path
actually **ran its work** — assert COVERAGE (every item was visited / stamped / considered), not just the absence of
effects. An empty or already-converged fixture has zero effects whether the code did the right thing or nothing at all.

**Why:** the two are indistinguishable from the outside, so a fixture chosen for "no-op" silently certifies a do-nothing
bug. The reconcile-rescan `reconcile_noop_writes_zero_entry_rows` test green-lit a "descends nowhere" bug for two
rounds: an unchanged tree has zero writes AND zero legitimate recursion targets, so a reconcile that stopped at the root
passed it cleanly. The fix was a fixture with a real multi-level tree asserting every directory was re-listed (visited),
not just that the row count held.

**How to spot the next one:** if flipping the production logic to a `return;`/no-op would still pass the test, the test
proves nothing. Pair every "writes nothing" assertion with a "visited everything" one.

### ❌ A component that measures itself, rendering nothing

**The rule:** a component that sizes itself from its container needs a viewport before any assertion about its contents
means anything. Give it one with `installLayoutMock()` (`$lib/test-layout`), and assert the rows you expect are ON
SCREEN before asserting something isn't on them.

**Why:** happy-dom has no layout engine, so `clientHeight` is `0` for every element. `FullList` and `BriefList` feed
that straight to the virtual-window math, which yields a zero-row window — so the list renders NOTHING, with no error
and no warning. Every negative assertion then passes for free: `FullList.ext-in-name-header.test.ts` counted zero
`.col-ext` cells over zero rows for months, and the a11y suite ran axe against an empty listbox while claiming to cover
a populated list. It also reads as an unfixable wall from the inside, because the symptom (an empty DOM) looks the same
as a broken mock, and the component's data path swallows its own throws.

**How to spot the next one:** if the spec's subject is a row, a column, a cell, or an item, assert the count first. Zero
rendered items and a passing test is the signature.

### ❌ The host machine is not a fixture

**The rule:** an automated run must never observe or react to the developer's real hardware, mounts, or network. When a
subsystem's startup path DISCOVERS things rather than being handed them, gate that discovery under `CMDR_E2E_MODE` so
the run only ever sees what it created itself.

**Why:** whatever the machine happens to have becomes a hidden input, and the failure lands on a random innocent spec.
The startup SMB adopter (`file_system::upgrade_existing_smb_mounts`) found David's real NAS at `/Volumes/naspi` in every
shard, tried a direct connection over the real LAN, and raised a genuine "this share is on the slow path" toast — which
the global `afterEach` UI-artifact guard then charged to whichever spec was running. Five runs, five different red
specs, none of them related. It also means a test run was reaching for a Keychain entry and opening a session to a
machine on someone's home network. The gate now lives in `test_mode::may_adopt_preexisting_network_mounts`.

**How to spot the next one:** grep a startup path for enumeration of the world — `/Volumes`, USB, mDNS, Bluetooth,
Keychain — and ask what it finds on a developer's laptop.

**The second instance, and its variant of the gate:** MTP's startup enumeration used to `launchctl disable
com.apple.ptpcamerad` (a real macOS daemon) and toast about it because the run's OWN virtual device made the device list
non-empty. The gate there keys off the DEVICE, not the run
(`mtp/watcher.rs::needs_ptpcamerad_suppression`): a virtual device is filesystem-backed and claims no USB interface, so
it never earns a host workaround, while a real phone plugged in during a run still gets one. Prefer that shape when the
subsystem can tell its own fixtures apart from the real thing — it also covers a `CMDR_VIRTUAL_MTP=1` dev session, which
an `CMDR_E2E_MODE` check would miss. **Known remaining instance:** that same enumeration still auto-connects a real USB
device it finds alongside the virtual one.

## Sanctioned slow-test exceptions

Most "raise the timeout" instincts are wrong (see the `retries: 1` and `sleep(N)` anti-patterns above): a flaky timeout
usually means a real race to fix, not a budget to enlarge. The rare exception is a test whose slow step is **external
and genuinely not optimizable** — then a generous, loudly-commented timeout is correct. Keep this list short; a new
entry needs a real "we can't make this faster" justification, not convenience.

- **`smb_integration_volume_id_is_per_mount_not_per_path_shape`** (`src-tauri/src/network/mount.rs`): uses a **16s**
  NetFS connect timeout (double the usual 8s). It's one of only two SMB tests that do a real macOS NetFS _kernel_ mount
  (`NetFSMountURLSync`); the other ~36 use the userspace `smb2` lib with no OS mount. The kernel mount RTT depends on
  factors we don't control (the OS mount queue, host CPU/lease contention when the full slow suite + both e2e lanes run
  at once), so the default 8s spuriously timed out under load. The mount is pure setup there (the test asserts on the
  resolved volume id, not mount speed), so the larger budget only delays how long a genuinely-hung mount waits before
  nextest's 30s slow-timeout cap fires. Don't copy the 16s to other tests, and don't apply it to
  `smb_integration_mount_guest_no_dialog`, whose 8s budget IS its assertion.
- **`smb_integration_mount_non_ascii_share`** (`src-tauri/src/network/mount.rs`): the third and last real NetFS kernel
  mount, on the same **16s** budget and for the same reason. Only NetFS can answer whether it accepts the escaped URL
  `build_smb_mount_url` produces, which is why a unit test can't replace it. It mounts exactly ONE share (`café`) even
  though the `unicode` fixture serves four: the others assert the same mechanism, their URLs are already pinned byte for
  byte by unit tests, and each extra kernel mount is another one of these budgets in the lane.
- **`smb_integration_concurrent_streaming_writes_no_deadlock`** (~2.8-4.3 s, the integration lane's slowest local test):
  don't shrink it to buy suite time. Its shape (200 files, 60 × 1 MB writes forced through the streaming fallback at
  concurrency 8) is deliberately tuned to the production workload that surfaced the deadlock it guards, and no smaller
  shape can be shown to still catch it without reproducing the original deadlock. It buys ~1 s on a ~5 s suite and
  trades away repro strength on a data-safety regression test.

## When you add X, also add Y

- **New `#[tauri::command]`**: (a) unit test for the underlying `*_core` / `ops_*` helper; (b) IPC contract test in
  `lib/ipc/*.test.ts` IF the command is destructive, cross-window, or has > 2 positional args
- **New state or transition in a state machine**: At least one unit test driving the new transition via the public
  interface
- **New pure parser / transform / collation**: Consider a proptest (round-trip, idempotence, or "output is valid for the
  consumer")
- **New keyboard shortcut**: Spec it via `dispatchMenuCommand` if menu-bound; synthetic keydown only if the test exists
  to verify the keyboard pathway itself
- **New user-visible flow**: One E2E happy-path spec; use `waitForSelector` or `expect.poll(...).toBeTruthy()` for any
  state wait (never bare `await pollUntil(...)`)
- **New write-side operation (copy / move / delete / etc.)**: Unit tests for the core + at least one E2E covering cancel
  and a conflict policy
- **New volume implementation**: Integration tests against the virtual fixture for that volume kind
- **A fixture for a type that crosses a subsystem boundary** (the scan cache, the preflight hints): build it through the
  type's named constructor, ❌ never a struct literal. A hand-written literal reproduces the author's assumptions rather
  than a shape production emits, which is how a suite full of fully-populated `per_path` fixtures certified a data-loss
  bug for three months. `desktop-rust-no-hand-rolled-fixture` enforces it.
- **A `Volume` double that misbehaves**: reach for `FaultyVolume`
  (`write_operations/transfer/volume/faulty_volume_test_support.rs`) or one of `InMemoryVolume`'s named lies, ❌ never a
  fresh 40-method forwarder. The fault the test needs should be the whole diff.

## Hot spots: modules with the strictest testing bar

These modules have invested test infrastructure. New code here must keep that bar:

- **`apps/desktop/src-tauri/src/file_system/write_operations/`**: state.rs has 30+ tests pinning every state-machine
  transition. Pattern: `cancel_write_operation` through the public interface, never via direct atomic mutation. See
  state.rs `mod tests`.
- **`crates/cmdr-index/src/indexing/`**: `IndexPhase` lifecycle tests in indexing/mod.rs require a real `IndexStore`
  (use `tempdir`-backed) and a dedicated test mutex (INDEXING is global).
- **`apps/desktop/src-tauri/src/file_viewer/`**: `SearchStatus` transitions through `search_cancel` are subtle (the
  thread writes `Cancelled`, the caller must not null `session.search` first). See `session.rs::tests`.
- **`crates/cmdr-index/src/indexing/store/`**: `platform_case_compare` (in `store/mod.rs`) has proptests in
  `store/tests/path_resolution.rs` for comparator algebra and NFC≡NFD equivalence. Don't regress these.

## E2E env-var hooks

E2E test hooks split along two axes:

- **Hard hooks** (binary shape) live behind Cargo features:
  - `playwright-e2e`: feature-gated Tauri commands (`inject_listing_error`, `set_test_throttle`,
    `set_test_scan_preview_delay`, `flush_file_watcher`) and the tauri-plugin-playwright socket bridge.
  - `virtual-mtp`: virtual MTP device with deterministic fixtures.
  - `smb-e2e`: virtual SMB hosts injected into mDNS discovery.

  These are compiled out of production binaries entirely. New commands or backends that don't make sense in prod go
  here.

- **Soft hooks** (runtime only) live behind environment variables. They are **strictly additive**: may add a delay, skip
  a non-essential step, or emit extra telemetry. Never replace production logic. With the env var unset, the code path
  is exactly what production runs.

  All soft hooks should be wired through `crate::test_mode` so the list of test hooks is grep-able from one place. New
  env-var-driven hooks land there with a helper function. Don't sprinkle `std::env::var(...)` reads through subsystems.

**Existing soft hooks** (env vars):

- **`CMDR_E2E_MODE=1`**: Canonical "we're under E2E" marker; subsystems can flip behaviors. **Requires
  `CMDR_DATA_DIR`**: the app panics at startup (`guard_e2e_requires_data_dir`) if E2E mode is on with no data dir set,
  since persisted state (favorites, settings, secrets) would otherwise write to the developer's real prod data dir.
  Every harness already sets both; only a bare manual `CMDR_E2E_MODE=1` launch trips it.
- **`CMDR_DATA_DIR`**: Isolated data dir for persisted state. Set by `tauri-wrapper.ts` (dev) and every E2E harness;
  mandatory under E2E mode (see above).
- **`CMDR_E2E_START_PATH`**: Fixture directory; surfaced via `get_e2e_start_path` so FE can pick it up.
- **`CMDR_E2E_SHARD_KIND`**: "mtp" / "non-mtp" / "all" / "i18n-capture" / "marketing-shots": selects spec subset for
  parallel sharding, and the two capture drivers each get their own kind so a normal suite run never takes screenshots.
- **`CMDR_E2E_JSON_REPORT`**: Per-shard Playwright JSON report path.
- **`CMDR_E2E_OUTPUT_DIR`**: Per-shard Playwright artifact dir.
- **`CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP=1`**: Non-MTP shards opt out of wiping the shared MTP backing dir.
- **`CMDR_E2E_SKIP_MTP_FIXTURES=1`**: Non-MTP shards skip `globalSetup`'s MTP fixture reset.
- **`CMDR_VIRTUAL_MTP=1` (or `=<dir>`)**: Dev opt-in: `pnpm dev` registers the virtual MTP device. See
  `tooling/virtual-mtp.md`.
- **`CMDR_E2E_COPY_THROTTLE_MS`**: Per-file sleep inside the copy loop. Lets tests stage Cancel/Rollback.
- **`CMDR_E2E_SCAN_PREVIEW_DELAY_MS`**: Holds every scan-preview worker at its starting line before it walks, so a spec
  can act while a transfer is still counting (`background-while-scanning.spec.ts`). Fixture trees are tiny and
  `data-scan-state` signals "counting done", the opposite of what such a test needs. `set_test_scan_preview_delay`
  overrides it per test, which is what the spec uses; the var is the process-wide fallback. Both are inert outside
  `CMDR_E2E_MODE`.
- **`CMDR_E2E_WALK_THROTTLE_MS`**: Per-directory sleep before a search's COVER walk reads one, so a spec has a window in
  which to watch a live search still running (`search-walk-handoff.spec.ts`). Background scans are never throttled. Read
  in `crates/cmdr-index/src/indexing/scanner/mod.rs` (`cover_walk_throttle`) rather than `crate::test_mode`, because the
  index crate can't reach the app; it's cached in a `LazyLock`, so an unset var costs one deref per walk.
- **`CMDR_PLAYWRIGHT_SOCKET`**: Override the plugin's Unix socket path (one socket per shard).
- **`CMDR_SHOTS_PID` / `CMDR_SHOTS_OUT_DIR` / `CMDR_SHOTS_BROWSE_ROOT`**: read by the marketing capture's spec, never by
  the app, so they stay out of `crate::test_mode`. The orchestrator passes its own app pid (nothing exposes it over the
  socket, and `screencapture -l` needs it to find the window), where masters are written, and which tree the panes
  browse. See `guides/screenshots.md`.

**Existing soft hooks** (IPC-driven, feature-gated to `playwright-e2e`):

- **`set_test_throttle(ms)`**: Mid-run override of `CMDR_E2E_COPY_THROTTLE_MS`; clears with `null`.
- **`flush_file_watcher()`**: Synchronously re-reads every active watch, bypassing debouncer + FSEvents latency.
- **`inject_listing_error()`**: Inject an IoError into a volume's next list_directory for retry coverage.
- **`fail_next_brief_column_widths(count)`**: Fail the next `count` Brief column-width computations, so a spec can watch
  the pane run on provisional widths (`brief-cursor-visibility.spec.ts`). Pass `0` to disarm. A hard hook because
  nothing on the JS side can stand in: Tauri defines `window.__TAURI_INTERNALS__` and its `invoke` non-writable AND
  non-configurable (verified on Tauri 2 / `@tauri-apps/api` 2.11.1, property-descriptor probe in the live webview,
  2026-08-12), so the usual "wrap invoke from the spec" trick silently no-ops.

## Process

- After adding a substantial chunk of new code: run `cargo mutants --file <new_file>` (Rust) or `pnpm exec stryker run`
  (TS) on the file to see if the new tests actually assert anything. Triage survivors.
- After E2E suite changes: run `pnpm check desktop-e2e-playwright` twice back-to-back. The first run warms the cache;
  the second run catches regressions that only fire under quiet load. Both must be green.
- See [maintenance.md § Codebase health](maintenance.md#codebase-health) for the periodic mutation + flake-rate checks.

## Quick links

- Tools inventory: `tooling/testing.md`
- E2E suite docs: `apps/desktop/test/e2e-playwright/CLAUDE.md`
- IPC test helpers: `apps/desktop/src/lib/ipc/CLAUDE.md`
- Notes from the speedup + coverage push: `notes/speed-up-e2e-tests.md`, `notes/extend-e2e-tests.md`
- Pre-release manual verification (native menus, drag-and-drop, real file system): `guides/testing/manual-checklist.md`
