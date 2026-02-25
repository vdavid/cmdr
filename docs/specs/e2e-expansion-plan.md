# E2E test expansion plan

## Goal

Expand E2E test coverage to catch regressions in core file manager workflows (copy, move, delete, rename, navigation)
while keeping the test suite fast, cheap, and maintainable. Set up filesystem sandboxing so macOS tests can safely
perform destructive file operations.

## Current state

Three E2E suites exist:

1. **Smoke tests** (Playwright, browser-based, 4 tests): Verify basic rendering and pane focus. No Tauri backend — can't
   test file operations. Run as `desktop-e2e` check.
2. **Linux E2E** (WebDriverIO + tauri-driver, Docker, ~20 tests): Full Tauri integration. Tests rendering, keyboard nav,
   mouse clicks, navigation (Enter/Backspace), dialogs (F5/F6/F7), file viewer + search, settings. Already sandboxed
   inside a Docker container with fixture files created by `entrypoint.sh`.
3. **macOS E2E** (WebDriverIO + CrabNebula, 10 tests): Similar to Linux but fewer tests. Uses `dispatchKey()` workaround
   because CrabNebula's WebDriver doesn't deliver `browser.keys()`. Runs against the real host filesystem — no
   sandboxing. Not in CI yet.

Key gaps: No tests for actual file operations (copy/move/delete/rename round-trips), no macOS filesystem sandboxing, and
the macOS suite is a subset of the Linux suite with significant duplication.

## Design decisions

### Don't duplicate tests across platforms

The Linux suite runs in Docker (free, fast, disposable). CrabNebula (macOS) may become a paid service with unknown
pricing. The two suites should test different things:

- **Linux**: All platform-independent app logic. Dialog flows, keyboard nav, selection mechanics, view modes, file viewer,
  settings, command palette. This is the workhorse.
- **macOS**: Platform integration only. Things the Linux stubs can't exercise: real APFS file operations via `copyfile(3)`,
  volume detection, permissions/Full Disk Access, WKWebView rendering correctness.

The principle: Linux tests verify the logic works. macOS tests verify the platform integration works.

### Don't E2E-test what's well unit-tested

Areas with solid unit test coverage (and no E2E value added):

| Area | Unit tests | Why E2E is unnecessary |
|------|-----------|----------------------|
| File viewer backends (FullLoad, ByteSeek, LineIndex) | 69 tests | Pure logic, no integration boundary |
| Copy/move/delete Rust engine | 67 tests | Engine is covered; but the **UI → dialog → Rust → filesystem → UI refresh** round-trip still needs E2E |
| Alphanumeric sorting | 30 tests | Pure logic |
| Indexing / SQLite | 92 tests | Pure logic |
| Filename validation, fuzzy search, nav history, drag hit-testing | Unit tested | Pure logic |

Where E2E tests add value that unit tests can't:
- **Full round-trips**: F5 → dialog → confirm → file appears. Unit tests cover pieces, E2E verifies wiring.
- **Focus management**: Tab, pane switch, dialog focus traps depend on real DOM.
- **Startup and loading**: App launches, loading screen clears, panes populate.
- **Multi-step workflows**: Navigate → create folder → copy into it → navigate back.

### Filesystem sandboxing via temp directory fixtures

macOS tests currently run against the real host filesystem. For safe file operations:

- **Approach**: Create a temp directory (`/tmp/cmdr-e2e-{timestamp}/`) with fixture files. Pass the path to the app via
  an environment variable (for example, `CMDR_E2E_START_PATH`) so both panes open there.
- **Fresh fixtures per test**: Recreate the full fixture directory structure before each test (in `beforeTest` /
  `beforeEach`), not just once per session. This way each test starts from a known, clean state — a test that deletes or
  moves files doesn't break subsequent tests. Clean up the root temp directory in `onComplete` / `afterSession`. This is
  cleaner than making tests depend on execution order, which is fragile and makes debugging failures harder.
- **Why not `sandbox-exec`**: Deprecated (works on macOS 15 but could break). Writing `.sb` profiles is fiddly. May
  interfere with Tauri's startup (settings store, window state).
- **Why not a dedicated macOS user**: Requires system setup, `sudo` complexity, CI complications.
- **Why not APFS snapshots**: Requires root, slow, overkill for E2E.

The temp directory approach mirrors what the Linux Docker tests already do (see `entrypoint.sh`). Tests can freely
create, copy, move, and delete files without touching anything real. It's not OS-enforced isolation, but E2E tests only
interact with what we tell them to.

### No sleeps — use condition-based waits with tight timeouts

E2E flakiness almost always comes from fixed-duration sleeps (`sleep(2000)`) that are either too short (flaky) or too
long (slow). Avoid them entirely. Instead:

- **Wait on conditions**: Use `browser.waitUntil(() => ...)` (or equivalent) to poll for the expected DOM state — an
  element appearing, disappearing, or changing text. This resolves as soon as the condition is true, so tests run at the
  speed of the app, not the speed of a hardcoded timer.
- **Short timeouts as performance assertions**: Set `waitUntil` timeouts to the maximum acceptable UX latency. For
  example, a dialog opening should complete within 2 seconds, a file copy within 5 seconds. If the app takes longer,
  the test *should* fail — that's a real bug, not flakiness. The app must be snappy; slow responses are defects.
- **Wait for the *right* thing**: After a file operation, wait for the pane listing to update (for example, a new
  filename appearing in the DOM), not for a fixed delay. After closing a dialog, wait for the dialog element to be
  removed from the DOM.
- **No retry loops for intermittent failures**: If a test is flaky, fix the root cause (race condition, missing
  await, stale element reference) instead of wrapping it in a retry. Retries hide real bugs.

Typical timeout values:
- UI reactions (dialog open, focus change, pane refresh): **2 seconds**
- File operations on small files: **3 seconds**
- File operations on large fixtures (see below): **10 seconds**
- App startup and initial pane load: **10 seconds**

### Tests run sequentially

All E2E tests run sequentially (one at a time), not in parallel. Cmdr uses shared resources — the indexing database, the
settings store, window state — that live in a single location per user. Running multiple Cmdr instances simultaneously
would cause conflicts at those shared resources. Sequential execution keeps things simple and deterministic. Combined
with per-test fixture recreation (see above), each test gets a clean slate without worrying about other tests.

## Implementation

### Milestone 1: Temp directory fixture setup

Add a fixture system to both macOS and Linux `wdio.conf.ts` configs that creates a known directory structure before each
test and cleans it up after the session. The macOS config passes the root path to the app so it starts there instead of
the user's home directory.

Fixture structure (recreated before every test):
```
/tmp/cmdr-e2e-{timestamp}/
  left/
    file-a.txt            (small text file, ~1 KB)
    file-b.txt            (small text file, ~1 KB)
    sub-dir/
      nested-file.txt     (~1 KB)
    bulk/                  (for progress UI and realistic load)
      large-1.dat          (50 MB, random/zero-filled)
      large-2.dat          (50 MB)
      large-3.dat          (50 MB)
      medium-01.dat … medium-20.dat   (20 × 1 MB each)
    .hidden-file          (for hidden files toggle test)
  right/
    (empty — target for copy/move operations)
```

Total fixture size: ~170 MB. This is large enough to exercise progress dialogs and buffered I/O during copy/move
(small files complete instantly and skip interesting code paths) while keeping per-test recreation under a few seconds
on an SSD. The `bulk/` directory also gives pane rendering a realistic number of entries to list. Generate the `.dat`
files with `/dev/urandom` or `dd` — contents don't matter, only size.

The app receives the root fixture path via `CMDR_E2E_START_PATH`. When set, the left pane opens `$PATH/left/` and the
right pane opens `$PATH/right/`. Two separate subdirectories are needed because copy/move tests require a source pane
with files and a distinct target pane to receive them — if both panes pointed at the same directory, every test would
need extra navigation steps to set up the source/target split, which adds fragility and noise. The `left/` + `right/`
convention keeps test setup minimal and intention clear.

The env var approach is simplest — read it in the Rust startup code and override the default pane paths. Only active
when the `automation` feature flag is enabled, so it never affects normal builds.

### Milestone 2: Expand Linux suite with file operation round-trips

Add tests to the existing `test/e2e-linux/app.spec.ts` (or a new `file-operations.spec.ts`):

1. **Copy round-trip**: Cursor on `file-a.txt` → F5 → confirm → verify `file-a.txt` exists in right pane listing
2. **Move round-trip**: Cursor on `file-b.txt` → F6 → confirm → verify it's gone from left pane, present in right
3. **Delete round-trip**: Cursor on a file → Delete → confirm → verify it's gone
4. **Rename round-trip**: Cursor on a file → F2 → type new name → Enter → verify name changed in listing
5. **Create folder round-trip**: F7 → type name → OK → verify folder appears in listing

**Important**: All file operation tests (copy, move, delete, rename, create folder) must verify both the UI (pane
listing updates correctly) **and** the actual filesystem (read the temp directory on disk to confirm the operation
happened). The pane could refresh without the operation succeeding, or vice versa — checking only one side misses real
bugs.

Also add:

6. **View mode toggle**: Switch Brief ↔ Full, verify layout changes (different CSS class / column headers appear)
7. **Hidden files toggle**: Toggle hidden files, verify `.hidden-file` appears/disappears
8. **Command palette**: Cmd+P → type a command → verify results appear → Escape to close
9. **Empty directory**: Navigate to an empty folder, verify graceful UI (no crash, shows ".." only)

### Milestone 3: macOS platform integration tests

Add to `test/e2e-macos/app.spec.ts` (or a new `file-operations.spec.ts`):

1. **Copy on APFS**: Copy a fixture file, verify it appears in target pane and on disk (exercises `copyfile(3)`)
2. **Move on APFS**: Move a file, verify source gone + target present (UI and filesystem)
3. **Delete**: Delete a fixture file, verify it's gone (UI and filesystem)
4. **Volume list renders**: Verify that at least one real macOS volume appears (not the Linux stubs). Don't assert a
   specific volume name like "Macintosh HD" — CI machines, external drives, and renamed volumes would make that brittle.

Also port the Linux navigation tests to macOS — Enter (navigate into directory) and Backspace (navigate to parent) are
important for WKWebView confidence. Port them using the `dispatchKey()` helper.

These tests plus the existing 10 rendering/nav tests keep the CrabNebula footprint minimal.

**Note on `dispatchKey()`**: macOS tests rely on the `dispatchKey()` workaround because CrabNebula's WebDriver doesn't
deliver `browser.keys()`. This should work for all interactions including typing filenames in rename/create-folder
dialogs. If it doesn't, escalate to the user — CrabNebula may be able to fix this on their end.

### Milestone 4: CI and docs

- ~~Add the macOS E2E tests to CI~~ — Intentionally staying local-only (GitHub Actions 10x macOS minute multiplier makes this too expensive on the free plan). Linux E2E runs in CI via Docker (free).
- Update `docs/tooling/e2e-testing-guide.md` with the new fixture system and the Linux-vs-macOS test split rationale.
- Update `test/e2e-macos/CLAUDE.md` with the new tests and fixture setup.

## What to skip

- **Drag-and-drop**: CrabNebula doesn't support it, and the logic has unit tests.
- **Licensing flows**: Unit tested, hard to E2E safely.
- **AI features**: Requires model download, unit tested.
- **Network/SMB**: Can't simulate in E2E easily.
- **File viewer backend behavior**: 69 unit tests cover it; the Linux E2E already tests viewer rendering.
- **Sort order correctness**: 30 unit tests cover it.
- **Filename validation edge cases**: Unit tested.

## Task list

### Milestone 1: Temp directory fixture setup
- [x] Add fixture creation helper (shared between macOS and Linux configs): creates the `left/` + `right/` structure, returns root path
- [x] Add `CMDR_E2E_START_PATH` env var support: left pane opens `$PATH/left/`, right pane opens `$PATH/right/` (gated behind `automation` feature)
- [x] Update macOS `wdio.conf.ts`: recreate fixtures in `beforeTest`, pass root path via env var, clean up in `afterSession`
- [x] Update Linux `entrypoint.sh` and/or `wdio.conf.ts`: same fixture structure and per-test recreation
- [x] Verify both suites start with left pane on `left/` and right pane on `right/`

### Milestone 2: Linux file operation tests
- [x] Add `file-operations.spec.ts` with copy round-trip test (verify UI + filesystem)
- [x] Add move round-trip test (verify UI + filesystem)
- [ ] Add delete round-trip test (verify UI + filesystem)
- [x] Add rename round-trip test (verify UI + filesystem)
- [x] Add create folder round-trip test (verify UI + filesystem)
- [x] Add view mode toggle test
- [x] Add hidden files toggle test
- [x] Add command palette open/search/close test
- [x] Add empty directory navigation test
- [x] Run `./scripts/check.sh --check desktop-e2e-linux` and verify all tests pass

### Milestone 3: macOS platform integration tests
- [x] Add copy-on-APFS test (verify UI + filesystem)
- [x] Add move test (verify UI + filesystem)
- [ ] Add delete test (verify UI + filesystem)
- [x] Add volume list rendering test (assert at least one volume, don't check specific names)
- [x] Port "navigates into directories with Enter" to macOS (using `dispatchKey`)
- [x] Port "navigates to parent with Backspace" to macOS
- [ ] Run macOS E2E tests locally and verify all pass

### Milestone 4: CI and docs
- [x] Update `docs/tooling/e2e-testing-guide.md` with fixture system docs and Linux-vs-macOS rationale
- [x] Update `test/e2e-macos/CLAUDE.md` with new tests and fixture setup
- [x] Add `CN_API_KEY` as a GitHub Actions secret (manual step, required for CrabNebula)
- [ ] ~~Add macOS E2E to CI workflow~~ — Intentionally skipped. macOS E2E stays local-only because GitHub Actions free plan charges macOS minutes at 10x, which would burn through the 2,000 free minutes/month quickly. Linux E2E runs in CI via Docker (free).
- [x] Run `./scripts/check.sh` to verify nothing is broken
