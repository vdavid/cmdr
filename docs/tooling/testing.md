# Testing tools

Inventory of testing tools available in Cmdr. One paragraph per tool: what it is, where it lives, how to invoke it, when
to reach for it.

Decision rules for which tool to use are in [docs/testing.md](../testing.md). This file answers "is there a tool for
X?". To create large fixture trees, see [generating-test-files.md](../guides/generating-test-files.md).

## Rust unit + integration

### `cargo nextest` (test runner)

Standard. Faster than `cargo test`. Run a single test by name: `cd apps/desktop/src-tauri && cargo nextest run <name>`.
Run all: through the checker: `pnpm check rust-tests`. Don't run raw `cargo test` (see AGENTS.md).

### `proptest` (property-based testing)

Dev-dependency on `cmdr-lib`. Use for pure functions where the input space is large enough that example tests miss edge
cases: comparators, parsers, transforms, generators. State a property (round-trip, idempotence, "output is valid for the
consumer"), let proptest fuzz inputs. Patterns to copy: `indexing/aggregator.rs` (topological sort), `search/query.rs`
(glob_to_regex + scope parsing), `indexing/store.rs` (platform_case_compare comparator laws). Keep properties **tight**:
"function doesn't panic" is too weak.

### `cargo-mutants` (mutation testing)

Not in Cargo.toml: install with `cargo install --locked cargo-mutants`. Use ad-hoc on hot-spot modules to find behavior
coverage gaps (tests that pass against the production code AND against deliberately-corrupted variants are not actually
asserting). Run on one file: `cd apps/desktop/src-tauri && cargo mutants --file src/<path> --timeout 60`. Cargo-mutants
copies the workspace and rebuilds per mutant (~10-15 minutes per file on this hardware). Use `--list` first (instant) to
preview the mutant set, then triage manually if a full run is too slow. Aim for ~80-90% mutation score per module; 100%
chases equivalent mutants and isn't worth it.

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

In `apps/desktop/test/e2e-playwright/helpers.ts`. **The canonical way to wait** in E2E. Polls a condition every 50ms
(default) until it returns true or times out. Never use `await sleep(N)` in spec files; the
`cmdr/no-arbitrary-sleep-in-e2e` ESLint rule will flag it.

```ts
await pollUntil(tauriPage, async () => tauriPage.isVisible('.error-pane'), 5000)
```

### `dispatchMenuCommand`: bypass keyboard simulation

In `apps/desktop/test/e2e-playwright/helpers.ts`. Triggers a registry command directly via the `execute-command` Tauri
event, mimicking what the OS native menu accelerator does in production. Use for menu-bound shortcuts (F2/F5/F6/F7/F8,
⌘C/X/V) when the test cares about the resulting dialog, not the keyboard pathway. Synthetic `keyboard.press('F5')` races
against handler attachment under parallel-shard load; this path doesn't.

### Virtual MTP device

Feature flag `virtual-mtp`. Pure-Rust MTP device backed by `/tmp/cmdr-mtp-e2e-fixtures/`. Lets MTP tests run without
real hardware. Helpers in `apps/desktop/test/e2e-shared/mtp-fixtures.ts` and `mcp-client.ts`. The
`resync_virtual_mtp_after_disk_change` IPC command atomically pauses the watcher, recreates fixtures, drains pending
FSEvents, rescans, and resumes. Use it from `beforeEach`, not the four-step manual sequence (race-prone).

The same device is available in a normal dev session via `CMDR_VIRTUAL_MTP=1 pnpm dev` — see
[virtual-mtp.md](virtual-mtp.md) for the dev workflow.

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
file-watcher debounce settling). Mirrors the `pollUntil`-first rule from [docs/testing.md](../testing.md).

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
`recreateMtpFixtures()` for cleanup (preferably wrapped by `resync_virtual_mtp_after_disk_change` so the watcher doesn't
race).

## Process tooling

### `pnpm check` (the checker)

The single entry point for all linters, formatters, type checkers, and test runners. It delegates to `scripts/check.sh`;
use `pnpm check --help` for the full option list. Always use it instead of raw `cargo`, `pnpm vitest`, `eslint`, etc.
Its output is concise and CI-aligned. Per-check: `pnpm check <name>`. By group: `pnpm check rust` / `svelte`. Fast
pre-commit lane (~7 s, curated): `--fast`. Slow checks (E2E, Docker): `--only-slow`. See AGENTS.md "Testing and
checking" for the three-cadence guidance.
