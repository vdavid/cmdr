# Desktop app tests

## E2E test suites

Three suites exist. The Playwright suite is the target replacement for the WebDriverIO suites:

| Suite                              | Tech                       | Runs on         | What it tests                                               | Status           |
| ---------------------------------- | -------------------------- | --------------- | ----------------------------------------------------------- | ---------------- |
| **Playwright** (`e2e-playwright/`) | Playwright + tauri-plugin  | macOS and Linux | Full app: dialogs, keyboard nav, file ops, settings, viewer | New (38/38 pass) |
| **Linux E2E** (`e2e-linux/`)       | WebDriverIO + tauri-driver | Docker (Ubuntu) | Same as above, older suite                                  | Legacy, CI       |
| **macOS E2E** (`e2e-macos/`)       | WebDriverIO + CrabNebula   | macOS only      | Platform integration: APFS ops, volume detection            | Legacy, local    |

**Playwright suite** uses `tauri-plugin-playwright` (fork at `vdavid/tauri-playwright`) which injects JS directly into
the Tauri webview via `webview.eval()` and receives results via Tauri IPC. No WebDriver, no CrabNebula dependency. Same
tests work on both macOS and Linux. Gated behind the `playwright-e2e` Cargo feature.

**Legacy suites** remain for now but can be removed once CI is switched to the Playwright suite.

## Shared fixture system

All E2E suites share `e2e-shared/fixtures.ts`, which creates a temp directory at `/tmp/cmdr-e2e-<timestamp>/`:

```
left/                         right/  (empty)
  file-a.txt, file-b.txt
  sub-dir/nested-file.txt
  .hidden-file
  bulk/  (3 x 50 MB + 20 x 1 MB .dat files)
```

`CMDR_E2E_START_PATH` env var tells the app where to open. Fixtures are recreated before tests (`recreateFixtures()`) so
tests don't affect each other.

## Other test infrastructure

- `smb-servers/` -- Docker Compose setup for local SMB share testing
- Unit tests live in `apps/desktop/test/` (Vitest) -- separate from E2E

## Detailed docs

Each suite has its own `CLAUDE.md`: `e2e-playwright/CLAUDE.md`, `e2e-linux/CLAUDE.md`, `e2e-macos/CLAUDE.md`.
