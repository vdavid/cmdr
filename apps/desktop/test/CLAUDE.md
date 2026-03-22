# Desktop app tests

## E2E test suites

Two complementary approaches, each covering what the others can't:

| Suite                        | Tech                       | Runs on         | What it tests                                               | CI?                    |
| ---------------------------- | -------------------------- | --------------- | ----------------------------------------------------------- | ---------------------- |
| **Linux E2E** (`e2e-linux/`) | WebDriverIO + tauri-driver | Docker (Ubuntu) | Full app: dialogs, keyboard nav, file ops, settings, viewer | Yes (slow)             |
| **macOS E2E** (`e2e-macos/`) | WebDriverIO + CrabNebula   | macOS only      | Platform integration: APFS ops, volume detection, WKWebView | No (local pre-release) |

**Why separate?** macOS WKWebView has no Apple-provided WebDriver, so standard tauri-driver only works on Linux
(WebKitGTK has WebKitWebDriver). CrabNebula provides a commercial WKWebView bridge for macOS, but GitHub Actions charges
macOS minutes at 10x — too expensive for CI. So: Linux E2E is the workhorse (all platform-independent logic), macOS E2E
covers platform-specific behavior.

## Shared fixture system

All E2E suites share `e2e-shared/fixtures.ts`, which creates a temp directory at `/tmp/cmdr-e2e-<timestamp>/`:

```
left/                         right/  (empty)
  file-a.txt, file-b.txt
  sub-dir/nested-file.txt
  .hidden-file
  bulk/  (3 x 50 MB + 20 x 1 MB .dat files)
```

`CMDR_E2E_START_PATH` env var tells the app where to open. Fixtures are fully recreated before each test
(`recreateFixtures()` in `beforeTest`) so tests don't affect each other.

## Other test infrastructure

- `smb-servers/` — Docker Compose setup for local SMB share testing
- Unit tests live in `apps/desktop/test/` (Vitest) — separate from E2E

## Detailed docs

Each suite has its own `CLAUDE.md` with running instructions, Docker/VNC setup, WebDriver quirks, and platform-specific
gotchas. See `e2e-linux/CLAUDE.md` and `e2e-macos/CLAUDE.md`.
