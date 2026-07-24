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

## Decision table: what tool for what test

- **Pure function with edge cases**: `proptest` (Rust unit). State a property, fuzz inputs.
- **Pure function with a few specific inputs**: Plain example tests in `mod tests`
- **Behavior coverage of an existing tested function**: `cargo mutants` survivor triage: every survived mutant is a
  behavior-level gap
- **State machine transition**: Rust unit test, **drive via the public interface**, not by setting the atomic directly
- **Wait for background work in a Rust test**: `crate::test_support::wait_until` (sync) / `wait_until_async`
  (`#[tokio::test]`), see § "Waiting for background work (Rust)". **Never** a hand-rolled poll loop or a fixed sleep
- **`#[tauri::command]` boundary**: vitest IPC contract test using `installIpcMock()` from
  `apps/desktop/src/lib/ipc/test-helpers.ts`
- **Frontend component logic**: vitest + svelte-testing-library in `*.test.ts`
- **Component-level a11y (ARIA, labels, focus order)**: tier-3 a11y test in `*.a11y.test.ts`
- **Keyboard shortcut opens a dialog**: E2E spec, use `dispatchMenuCommand(tauriPage, 'file.copy')`. **Never** synthetic
  F-key press unless the test exists to verify the keyboard pathway
- **Wait for UI state to change in E2E**: `expect.poll(async () => …, { timeout }).toBeTruthy()` (preferred — wait fuses
  with assertion); `expect(await pollUntil(...)).toBe(true)` for the few non-Playwright contexts. **Never** bare
  `await pollUntil(...)` (silent timeout) or `await sleep(N)` (flaky)
- **Cross-component flow (return-focus, dialog stack, navigation)**: E2E (Playwright)
- **Storage volume operation (MTP, SMB)**: Integration test against a virtual fixture (virtual-mtp feature, Docker SMB
  containers)

## Waiting for background work (Rust)

`crate::test_support` is the sanctioned way for a Rust test to wait, and the only place in Rust test code that sleeps.
Two flavors, same shape:

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
- ❌ Don't call the sync one from an async test: `std::thread::sleep` blocks the runtime worker and deadlocks a
  current-thread scheduler. `wait_until_async` measures on tokio's clock, so a `start_paused` runtime auto-advances
  through the wait.
- ❌ Don't hand-roll a poll loop, and ❌ don't sleep a fixed span hoping the work landed. A test that genuinely needs a
  fixed wall-clock wait (fake latency in a stub, a negative assertion over a window, a test whose subject IS a debounce)
  carries an `// allowed-test-sleep: <reason>` comment.
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
`apps/desktop/test/e2e-playwright/DETAILS.md` § "Polling helpers" and `scripts/check/checks/DETAILS.md` § `bare-poll`.

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

## Hot spots: modules with the strictest testing bar

These modules have invested test infrastructure. New code here must keep that bar:

- **`apps/desktop/src-tauri/src/file_system/write_operations/`**: state.rs has 30+ tests pinning every state-machine
  transition. Pattern: `cancel_write_operation` through the public interface, never via direct atomic mutation. See
  state.rs `mod tests`.
- **`apps/desktop/src-tauri/src/indexing/`**: `IndexPhase` lifecycle tests in indexing/mod.rs require a real
  `IndexStore` (use `tempdir`-backed) and a dedicated test mutex (INDEXING is global).
- **`apps/desktop/src-tauri/src/file_viewer/`**: `SearchStatus` transitions through `search_cancel` are subtle (the
  thread writes `Cancelled`, the caller must not null `session.search` first). See `session.rs::tests`.
- **`apps/desktop/src-tauri/src/file_system/index/store.rs`**: `platform_case_compare` has proptests for comparator
  algebra and NFC≡NFD equivalence. Don't regress these.

## E2E env-var hooks

E2E test hooks split along two axes:

- **Hard hooks** (binary shape) live behind Cargo features:
  - `playwright-e2e`: feature-gated Tauri commands (`inject_listing_error`, `set_test_throttle`, `flush_file_watcher`)
    and the tauri-plugin-playwright socket bridge.
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
- **`CMDR_E2E_SHARD_KIND`**: "mtp" / "non-mtp" / "all": selects spec subset for parallel sharding.
- **`CMDR_E2E_JSON_REPORT`**: Per-shard Playwright JSON report path.
- **`CMDR_E2E_OUTPUT_DIR`**: Per-shard Playwright artifact dir.
- **`CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP=1`**: Non-MTP shards opt out of wiping the shared MTP backing dir.
- **`CMDR_E2E_SKIP_MTP_FIXTURES=1`**: Non-MTP shards skip `globalSetup`'s MTP fixture reset.
- **`CMDR_VIRTUAL_MTP=1` (or `=<dir>`)**: Dev opt-in: `pnpm dev` registers the virtual MTP device. See
  `tooling/virtual-mtp.md`.
- **`CMDR_E2E_COPY_THROTTLE_MS`**: Per-file sleep inside the copy loop. Lets tests stage Cancel/Rollback.
- **`CMDR_PLAYWRIGHT_SOCKET`**: Override the plugin's Unix socket path (one socket per shard).

**Existing soft hooks** (IPC-driven, feature-gated to `playwright-e2e`):

- **`set_test_throttle(ms)`**: Mid-run override of `CMDR_E2E_COPY_THROTTLE_MS`; clears with `null`.
- **`flush_file_watcher()`**: Synchronously re-reads every active watch, bypassing debouncer + FSEvents latency.
- **`inject_listing_error()`**: Inject an IoError into a volume's next list_directory for retry coverage.

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
