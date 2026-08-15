# Testing tools

Inventory of testing tools available in Cmdr. One paragraph per tool: what it is, where it lives, how to invoke it, when
to reach for it.

Decision rules for which tool to use are in `../testing.md`. This file answers "is there a tool for X?". To create large
fixture trees, see `../guides/generating-test-files.md`.

## Rust unit + integration

### `cargo nextest` (test runner)

Standard. Faster than `cargo test`. Run a single test by name: `cd apps/desktop/src-tauri && cargo nextest run <name>`.
Run all: through the checker: `pnpm check rust-tests`. Don't run raw `cargo test` (see AGENTS.md).

### `crate::test_support::TestDir` (scratch directory)

In `crates/cmdr-fs/src/testing.rs` beside the wait helpers, behind the `testing` feature, re-exported as
`crate::test_support` in the app. `TestDir::new("label")` gives a process-unique directory that removes itself when the
handle drops (unwind included); it derefs to `Path` and implements `AsRef<Path>`, so a converted test body reads like
the `PathBuf` it replaced. It's the only sanctioned way to get a directory to write in — a fixed
`std::env::temp_dir().join("cmdr_foo")` is shared by every process on the machine. Rules and the three failure modes:
`../testing.md` § "Scratch directories (Rust)".

### `crate::test_support::wait_until` / `wait_until_async` (waiting for background work)

In `crates/cmdr-fs/src/testing.rs`, behind the `testing` feature, re-exported as `crate::test_support` in the app.
`wait_until` for sync `#[test]`s, `wait_until_async` for `#[tokio::test]`s; both take a timeout, a description, and a
condition closure, and panic on timeout. The only sanctioned sleep in Rust test code lives inside them. Rules and
examples: `../testing.md` § "Waiting for background work (Rust)".

### `indexing::test_support::count_allocations` / `heap_bytes_held` (memory shape)

Under `cfg(test)` this module installs a pass-through global allocator that counts allocations and live bytes per THREAD
(parallel tests can't pollute each other). `count_allocations(|| …)` reports how many allocations a closure made;
`heap_bytes_held(|| …)` reports the requested bytes its result still holds. Use them to pin a hot path's SHAPE against a
generous bound ("this walk doesn't allocate per row"), never an exact number: the numbers move with buffer growth and
allocator internals, the shape is the invariant. Worked examples: `media_index/scheduler/enrich_memory_tests.rs`.

It lives in the indexing tree, not next to `wait_until`, because a `#[global_allocator]` is per BINARY: it has to sit in
the crate whose test binary is measuring, and a shared crate would give the shipped app a second one. Rust memory
numbers taken in tests are therefore measured under THIS allocator, not mimalloc — don't compare them with production
figures.

### `proptest` (property-based testing)

Dev-dependency on `cmdr-lib`. Use for pure functions where the input space is large enough that example tests miss edge
cases: comparators, parsers, transforms, generators. State a property (round-trip, idempotence, "output is valid for the
consumer"), let proptest fuzz inputs. Patterns to copy: `indexing/aggregator/tests.rs` (topological sort),
`search/query.rs` (glob_to_regex + scope parsing), `indexing/store/tests/path_resolution.rs` (platform_case_compare
comparator laws). Keep properties **tight**: "function doesn't panic" is too weak.

### `cargo-mutants` (mutation testing)

Not in Cargo.toml: install with `cargo install --locked cargo-mutants`. Use ad-hoc on hot-spot modules to find behavior
coverage gaps (tests that pass against the production code AND against deliberately-corrupted variants are not actually
asserting). Run on one file: `cd apps/desktop/src-tauri && cargo mutants --file src/<path> --timeout 60`. Cargo-mutants
copies the workspace and rebuilds per mutant (~10-15 minutes per file on this hardware). Use `--list` first (instant) to
preview the mutant set, then triage manually if a full run is too slow. Aim for ~80-90% mutation score per module; 100%
chases equivalent mutants and isn't worth it.

### `criterion` (benchmarks)

Two bench targets: `apps/desktop/src-tauri/benches/icon_benchmarks.rs` (icon fetching) and
`crates/cmdr-index/benches/index_benchmarks.rs` (index enrichment, the IPC dir-stats read, and the dir-stats roll-up,
over a synthetic index DB built through the public `store` API). Run one with `cargo bench -p <package> --bench <name>`;
add `-- --save-baseline <name>` to record a run and `-- --baseline <name>` to diff the next one against it. Reports land
in `target/criterion/`. No check runs them: they're for answering "did this get slower", not for gating. Recorded index
numbers, before and after the crate extraction, plus the method: `docs/notes/index-extraction-baseline.md`.

A bench compiles against its crate as an EXTERNAL one, so it sees neither `#[cfg(test)]` items nor `pub(crate)` ones.
The `testing` Cargo feature widens the few scaffolding items a bench needs (today: the root-read-pool installers in
`indexing/read/enrichment.rs`, and `FileEntry` at the crate root). ❌ Don't reach for `required-features` to enable it:
`cargo clippy --all-targets` silently SKIPS targets whose required features are off, and an unlinted, never-compiled
benchmark rots. Instead the package dev-depends on itself (`cmdr = { path = ".", features = ["testing"] }`), which turns
the feature on for every dev target and leaves it off for the lib and the shipped `Cmdr` binary. That self-dependency is
load-bearing, and it's why `lib.rs` carries a `#[cfg(test)] use cmdr_lib as _;` marker.

## Frontend + Svelte

### `vitest` (test runner)

For TS, Svelte, and IPC contract tests. Run all: `pnpm check svelte-tests`. Run by name:
`cd apps/desktop && pnpm vitest run -t "<name>"`. Existing patterns: component tests in `*.test.ts` next to the source,
tier-3 a11y tests in `*.a11y.test.ts`.

### `installIpcMock()`: IPC contract test harness

In `apps/desktop/src/lib/ipc/test-helpers.ts`. Thin wrapper around Tauri's `@tauri-apps/api/mocks::mockIPC`. Returns a
recorder with `calls: ReadonlyArray<{command, payload}>`, `mock(command, responder)`, `lastCall(command)`, and
`callCount(command)`. Use to pin the wire shape of `#[tauri::command]` boundaries: payload keys, positional-arg order,
typed-error variant discrimination. **Doesn't** simulate the Tauri permission gate (it patches
`__TAURI_INTERNALS__.invoke` upstream of the gate), so it can't catch permission-config drift. Use for destructive /
cross-window / multi-positional-arg commands; skip for thin getters.

### `installLayoutMock()`: a viewport for components that measure themselves

In `apps/desktop/src/lib/test-layout.ts`. happy-dom has no layout engine, so every `clientHeight` / `clientWidth` /
`offsetWidth` / `offsetHeight` reads back `0`. `installLayoutMock({ '<selector>': { clientHeight: 400 } })` hands those
four metrics a number for the elements a test names, leaves every other element reading the environment's own `0`, and
restores itself when the test finishes. The returned handle also drives change: `resize(selector, box)` updates the box
AND notifies the `ResizeObserver`s watching matching elements (which is how Svelte's `bind:clientHeight` family
re-reads), and `scroll(selector, top)` sets `scrollTop` before dispatching the `scroll` event the component listens for.

Reach for it whenever a component sizes itself from its container: both file-list views, `ShareBrowser`,
`NetworkBrowser`. Without it a virtualized list computes a zero-row window and renders NOTHING, with no error — so a
spec asserting "no Ext cell" or "no hourglass" passes against an empty DOM. See `../testing.md` § "A component that
measures itself, rendering nothing".

It supplies measurements, it doesn't compute them: `getBoundingClientRect`, `scrollHeight`, and everything else stay at
the environment's zeros, so pixel geometry (drag auto-scroll, hit-testing) still belongs in Playwright. What it does
give is faithful for the window math, because that math is unchanged — a 100 px surface over 20 px rows renders the same
five rows the app renders, and scrolling to 200 px lands on the same row through the same gutter correction.

The global `ResizeObserver` stub (`src/test-setup.ts`) is the same class, and stays silent for every spec that doesn't
call `resize()`: nothing measures anything here, so a resize can only come from a test.

### `mountFullList()` + the file-list mocks

In `apps/desktop/src/lib/file-explorer/views/test-full-list.ts` (mounting, entry fixtures) and `test-file-list-mocks.ts`
(the module stand-ins). `mountFullList({ entries })` gives a real `FullList` a measured surface, a `getFileRange` that
serves the `(start, count)` range it's asked for, and the twelve required props, then hands back `rowNames()` /
`hourglassRowNames()` / `layout` / `settle()`. The mock factories are spread into the spec's own `vi.mock` calls (the
only form Vitest hoists) and cover every export the listing path touches.

Both halves exist because `FullList`'s data path fails silently: `fetchVisibleRange` swallows every throw, so one
missing mock export or one absent numeric setting (`NaN` fetch range) empties the list with no complaint. ⚠️ The mocks
file must not import a component — a `vi.mock` factory reaching back into a module the component imports deadlocks the
run.

### `stryker-mutator` (mutation testing for TS)

Not in package.json: install ad-hoc: `pnpm add -D -w @stryker-mutator/core @stryker-mutator/typescript-checker`. Fast on
a single file (~12 s on a 600-line module) but choppy on the full Svelte/Tauri project. Sharp config edges. Use for
numeric / pure-TS modules only; **don't** attempt on `.svelte` files. Pattern to copy: how it ran on
`apps/desktop/src/lib/.../scan-throughput.ts` during the Step 7 push.

## End-to-end

### Playwright (E2E suite)

`apps/desktop/test/e2e-playwright/`. Runs against the real Tauri binary built with the `playwright-e2e` feature. Three
sharded workers on macOS (one MTP-only + two non-MTP). Run: `pnpm check desktop-e2e-playwright`. See
`apps/desktop/test/e2e-playwright/CLAUDE.md` for the full docs.

### `pollUntil`: condition-based wait

In `apps/desktop/test/e2e-playwright/helpers.ts`. The condition-based wait for the E2E suite. Polls a condition every
50ms (default) until it returns true or times out. The rules for how to wait (never `await sleep(N)`, never a bare
`await pollUntil(...)` whose timeout is silently discarded) live in `../testing.md` § Anti-patterns; the
`cmdr/no-arbitrary-sleep-in-e2e` and `bare-poll` checks enforce them.

```ts
await pollUntil(tauriPage, async () => tauriPage.isVisible('.error-pane'), 5000)
```

### `emitBackendEvent`: drive UI that reacts to a backend event

In `apps/desktop/test/e2e-playwright/helpers/core.ts`. Emits a Tauri event into the running app in the shape the Rust
side emits it: `emitBackendEvent(tauriPage, 'index-phase-changed', { volumeId, phase: 'scanning' })`. Tauri's event
plugin broadcasts an emit to every listener including the webview it came from, so the frontend's `listen()` receives it
exactly as it receives the backend's own. `dispatchMenuCommand` is one line on top of it.

Reach for it whenever the UI under test reacts to something the backend announces (indexing phases, walked branches,
freshness, aggregation progress) and reproducing that for real would mean racing the work. The event name and payload
come from `src/lib/ipc/bindings.ts`, both generated from Rust, so a renamed event or a changed field surfaces as the
spec going red instead of silent drift — which is most of what such a spec is for, since the state machine behind the UI
is already unit-tested.

Two rules, both in the helper's doc comment: only for events **nothing in Rust listens to** (these are one-way
announcements; emitting one the backend consumes would drive real work from a test), and **undo it before the test
ends** — the app is shared by every spec in the shard, so emit the terminal event and prefer a synthetic id nothing real
can claim. Worked example: `indexing-status-corner.spec.ts`.

### `dispatchMenuCommand`: bypass keyboard simulation

In `apps/desktop/test/e2e-playwright/helpers.ts`. Triggers a registry command directly via the `execute-command` Tauri
event, mimicking what the OS native menu accelerator does in production. Use for menu-bound shortcuts (F2/F5/F6/F7/F8,
⌘C/X/V) when the test cares about the resulting dialog, not the keyboard pathway. Synthetic `keyboard.press('F5')` races
against handler attachment under parallel-shard load; this path doesn't.

### Virtual MTP device

Feature flag `virtual-mtp`. Pure-Rust MTP device backed by `/tmp/cmdr-mtp-e2e-fixtures-<pid>/` under the checker (the
bare path is the manual-dev default; see `virtual-mtp.md` § "Custom backing dir"). Lets MTP tests run without real
hardware. Helpers in `apps/desktop/test/e2e-shared/mtp-fixtures.ts` and `mcp-client.ts`. In `beforeEach`, pause the
watcher (`pause_virtual_mtp_watcher`), recreate fixtures, then sync the object tree with `rescan_virtual_mtp`. The
watcher stays PAUSED for the test body so late FSEvents from the wipe+recreate can't remove freshly rescanned handles;
only the one test that verifies the live-watch pipeline resumes it. See `src-tauri/src/mtp/DETAILS.md` § "Virtual device
watcher in E2E".

The same device is available in a normal dev session via `CMDR_VIRTUAL_MTP=1 pnpm dev` — see `virtual-mtp.md` for the
dev workflow.

### Docker SMB containers

14 Samba containers for SMB integration tests. Start with `apps/desktop/test/smb-servers/start.sh`. macOS skips SMB E2E
entirely (mount requires permissions a headless run can't grant); Linux uses GVFS mounts. The 50-share and unicode share
tests have a known GVFS race in Docker (the `UDisks2VolumeMonitor` warning, see `gio mount` failures); they flake
~10-20% of the time. Treated as a pre-existing environmental issue, not the test's fault.

**The stack is shared machine-wide.** Concurrent SMB-touching runs across git worktrees (two `check.sh` invocations, or
a `check.sh` plus a manual `start.sh`) now coexist: every bring-up and teardown routes through a Go lease helper
(`scripts/check/smblease`) that refcounts holders and downs the stack only when the last one leaves. So a sibling
worktree's teardown no longer kills your live suite. If a leaked lease keeps the stack up after everything's idle, check
state with `(cd scripts/check && go run ./smb-lease status)` and force it down with
`rm -rf /tmp/cmdr-smb-leases && apps/desktop/test/smb-servers/stop.sh`. See `apps/desktop/test/smb-servers/README.md` §
"Shared stack across worktrees" for the full model.

### MCP servers (for ad-hoc exploration during test writing)

When the dev server is running (`pnpm dev` at repo root):

- **cmdr** MCP server: high-level: navigation, file ops, search, dialogs, state inspection
- **tauri** MCP bridge: low-level: screenshots, DOM inspection, JS execution, IPC calls

Both bind `127.0.0.1` only on ephemeral ports per instance. External clients read the actual port from
`<CMDR_DATA_DIR>/mcp.port` and `<CMDR_DATA_DIR>/tauri-mcp.port`. See `docs/tooling/mcp.md` and
`docs/tooling/instance-isolation.md`. Use this to verify expected behavior empirically before writing a test. Don't
leave the dev server running after; stop it when done.

## Linters / static checks

### `cmdr/no-arbitrary-sleep-in-e2e` (ESLint)

In `apps/desktop/eslint-plugins/no-arbitrary-sleep-in-e2e.js`. Flags `await sleep(N)` in `*.spec.ts` files. Opt out with
`// eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- <reason>` only when a genuine fixed wait is needed (e.g.,
file-watcher debounce settling). Mirrors the `pollUntil`-first rule from `../testing.md`.

### `cmdr/no-raw-tauri-invoke` (ESLint)

Bans `invoke('command_name', …)` outside `src/lib/ipc/`. Use the typed `commands.commandName(args)` instead.

### `cmdr/no-error-string-match` (ESLint) + `error-string-match` (Rust check)

Ban substring-matching against error/state semantics. Use typed enum variants. See AGENTS.md "No string-matching error
or state classification".

### `custom/no-isolated-tests` (ESLint)

Ensures test files actually exercise the source they sit next to (not just isolated assertions on inlined logic).

### `bindings-fresh` (Rust check)

Verifies the committed `bindings.ts` matches what `pnpm bindings:regen` would produce. Catches forgotten regenerations
after `#[tauri::command]` surface changes.

## Test data

### E2E fixtures

`apps/desktop/test/e2e-shared/fixtures.ts` creates a deterministic directory tree at `/tmp/cmdr-e2e-<timestamp>/` with
small text files, hidden files, a sub-directory, and ~170 MB of bulk `.dat` files for transfer tests. Each shard gets
its own timestamped path (auto-collision-safe). `recreateFixtures()` does a lightweight per-test reset that preserves
the bulk `.dat` files.

### MTP fixtures

`apps/desktop/test/e2e-shared/mtp-fixtures.ts` populates the virtual MTP device's backing dir. Use
`recreateMtpFixtures()` for cleanup, bracketed by `pause_virtual_mtp_watcher` before and `rescan_virtual_mtp` after so
the watcher stays paused and can't race the reset (see the "Virtual MTP device" section above).

## Process tooling

### `pnpm check` (the checker)

The single entry point for all linters, formatters, type checkers, and test runners. It delegates to `scripts/check.sh`;
use `pnpm check --help` for the full option list. Always use it instead of raw `cargo`, `pnpm vitest`, `eslint`, etc.
Its output is concise and CI-aligned. Per-check: `pnpm check <name>`. By group: `pnpm check rust` / `svelte`. Fast
pre-commit lane (~7 s, curated): `--fast`. Slow checks (E2E, Docker): `--only-slow`. See AGENTS.md "Testing and
checking" for the three-cadence guidance.
