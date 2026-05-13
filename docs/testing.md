# Testing playbook

How we test Cmdr. Decision rules, anti-patterns, and a per-feature checklist. If you're adding tests, read this first.

The companion file [docs/tooling/testing.md](tooling/testing.md) is the tools inventory — one paragraph per tool.

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

## Decision table — what tool for what test

| You want to test                                              | Tool / layer                                                                                                                                        |
| ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Pure function with edge cases                                 | `proptest` (Rust unit). State a property, fuzz inputs.                                                                                              |
| Pure function with a few specific inputs                      | Plain example tests in `mod tests`                                                                                                                  |
| Behavior coverage of an existing tested function              | `cargo mutants` survivor triage — every survived mutant is a behavior-level gap                                                                     |
| State machine transition                                      | Rust unit test, **drive via the public interface**, not by setting the atomic directly                                                              |
| `#[tauri::command]` boundary                                  | vitest IPC contract test using `installIpcMock()` from `apps/desktop/src/lib/ipc/test-helpers.ts`                                                   |
| Frontend component logic                                      | vitest + svelte-testing-library in `*.test.ts`                                                                                                      |
| Component-level a11y (ARIA, labels, focus order)              | tier-3 a11y test in `*.a11y.test.ts`                                                                                                                |
| Keyboard shortcut opens a dialog                              | E2E spec, use `dispatchMenuCommand(tauriPage, 'file.copy')` — **never** synthetic F-key press unless the test exists to verify the keyboard pathway |
| Wait for UI state to change in E2E                            | `pollUntil(tauriPage, async () => …, timeout)` from `helpers.ts` — **never** `await sleep(N)`                                                       |
| Cross-component flow (return-focus, dialog stack, navigation) | E2E (Playwright)                                                                                                                                    |
| Storage volume operation (MTP, SMB)                           | Integration test against a virtual fixture (virtual-mtp feature, Docker SMB containers)                                                             |

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

For "wait until X is true" where X isn't a selector, use `pollUntil`:

```ts
await pollUntil(
  tauriPage,
  async () => {
    const size = await tauriPage.evaluate<number>(`document.querySelector(…)?.offsetHeight ?? 0`)
    return size > 0
  },
  5000,
)
```

The `cmdr/no-arbitrary-sleep-in-e2e` ESLint rule flags this. Opt out per-line with
`// eslint-disable-next-line cmdr/no-arbitrary-sleep-in-e2e -- <reason>` only when there's a genuine fixed-duration wait
(e.g., watcher debounce settling) — and even then, prefer a poll if any state changes.

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

Keep one or two dedicated tests on the keyboard pathway (`app.spec.ts` has these — names like "opens copy dialog with
F5"). The rest should use `dispatchMenuCommand`.

### ❌ Direct atomic / store mutation in state-machine tests

A state-machine test that does `state.intent.store(OperationIntent::RollingBack)` is testing nothing — it bypasses the
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
through the public-via-helper path — don't reach past the guard.

### ❌ `retries: 1` to mask a race

Retries hide bugs. If a test flakes, find the race and fix it (Rust IPC race, missing await, watcher debounce, etc.).
Drop retries when the cause is gone.

### ❌ Raw `tauri::invoke('command_name', …)` outside the typed bindings

Use `commands.commandName(args)` from `apps/desktop/src/lib/ipc/`. Enforced by `cmdr/no-raw-tauri-invoke` ESLint rule
and the `bindings-fresh` CI check.

### ❌ Substring-matching error messages or state labels

Use typed enum variants, not `err.message.includes('not found')`. Enforced by `cmdr/no-error-string-match` (TS) and the
`error-string-match` check (Rust).

### ❌ Layering a "skip build if hash matches" wrapper over `pnpm tauri build`

Cargo / Vite / `beforeBuildCommand` already cache. Wrapping risks shipping stale binaries. See AGENTS.md.

## When you add X, also add Y

| New thing                                              | Required tests                                                                                                                                                                   |
| ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| New `#[tauri::command]`                                | (a) unit test for the underlying `*_core` / `ops_*` helper; (b) IPC contract test in `lib/ipc/*.test.ts` IF the command is destructive, cross-window, or has > 2 positional args |
| New state or transition in a state machine             | At least one unit test driving the new transition via the public interface                                                                                                       |
| New pure parser / transform / collation                | Consider a proptest (round-trip, idempotence, or "output is valid for the consumer")                                                                                             |
| New keyboard shortcut                                  | Spec it via `dispatchMenuCommand` if menu-bound; synthetic keydown only if the test exists to verify the keyboard pathway itself                                                 |
| New user-visible flow                                  | One E2E happy-path spec; use `pollUntil` / `waitForSelector` for any state wait                                                                                                  |
| New write-side operation (copy / move / delete / etc.) | Unit tests for the core + at least one E2E covering cancel and a conflict policy                                                                                                 |
| New volume implementation                              | Integration tests against the virtual fixture for that volume kind                                                                                                               |

## Hot spots — modules with the strictest testing bar

These modules have invested test infrastructure. New code here must keep that bar:

- **`apps/desktop/src-tauri/src/file_system/write_operations/`** — state.rs has 30+ tests pinning every state-machine
  transition. Pattern: `cancel_write_operation` through the public interface, never via direct atomic mutation. See
  state.rs `mod tests`.
- **`apps/desktop/src-tauri/src/indexing/`** — `IndexPhase` lifecycle tests in indexing/mod.rs require a real
  `IndexStore` (use `tempdir`-backed) and a dedicated test mutex (INDEXING is global).
- **`apps/desktop/src-tauri/src/file_viewer/`** — `SearchStatus` transitions through `search_cancel` are subtle (the
  thread writes `Cancelled`, the caller must not null `session.search` first). See `session.rs::tests`.
- **`apps/desktop/src-tauri/src/file_system/index/store.rs`** — `platform_case_compare` has proptests for comparator
  algebra and NFC≡NFD equivalence. Don't regress these.

## Process

- After adding a substantial chunk of new code: run `cargo mutants --file <new_file>` (Rust) or `pnpm exec stryker run`
  (TS) on the file to see if the new tests actually assert anything. Triage survivors.
- After E2E suite changes: run `./scripts/check.sh --check desktop-e2e-playwright` twice back-to-back. The first run
  warms the cache; the second run catches regressions that only fire under quiet load. Both must be green.
- See [maintenance.md § Codebase health](maintenance.md#codebase-health) for the periodic mutation + flake-rate checks.

## Quick links

- Tools inventory: [docs/tooling/testing.md](tooling/testing.md)
- E2E suite docs: [apps/desktop/test/e2e-playwright/CLAUDE.md](../apps/desktop/test/e2e-playwright/CLAUDE.md)
- IPC test helpers: [apps/desktop/src/lib/ipc/CLAUDE.md](../apps/desktop/src/lib/ipc/CLAUDE.md)
- Notes from the speedup + coverage push: [docs/notes/speed-up-e2e-tests.md](notes/speed-up-e2e-tests.md),
  [docs/notes/extend-e2e-tests.md](notes/extend-e2e-tests.md)
