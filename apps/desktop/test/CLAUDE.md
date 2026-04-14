# Desktop app tests

## E2E test suites

| Suite                              | Tech                      | Runs on                           | What it tests                                                          |
| ---------------------------------- | ------------------------- | --------------------------------- | ---------------------------------------------------------------------- |
| **Playwright** (`e2e-playwright/`) | Playwright + tauri-plugin | macOS (native) and Linux (Docker) | Full app: dialogs, keyboard nav, file ops, settings, viewer, a11y, MTP |

**Playwright suite** uses `tauri-plugin-playwright` (fork at `vdavid/tauri-playwright`) which injects JS directly into
the Tauri webview via `webview.eval()` and receives results via Tauri IPC. Same tests work on both macOS and Linux.
Gated behind the `playwright-e2e` Cargo feature.

**Linux Docker infrastructure** lives in `e2e-linux/docker/` (Dockerfile + entrypoint). The `e2e-linux.sh` script builds
the Tauri binary with `--features playwright-e2e,virtual-mtp` inside Docker, launches it, and runs the Playwright tests.

## Shared fixture system

All E2E suites share `e2e-shared/fixtures.ts`, which creates a temp directory at `/tmp/cmdr-e2e-<timestamp>/`. MTP E2E
tests use a virtual MTP device (pure Rust, no USB needed) via the `virtual-mtp` feature flag, with helpers in
`e2e-shared/mcp-client.ts` and `e2e-shared/mtp-fixtures.ts`. SMB E2E tests use virtual hosts injected via the `smb-e2e`
feature flag pointing at Docker SMB containers, with helpers in `e2e-shared/smb-fixtures.ts`.

Filesystem fixtures layout:

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

- `smb-servers/` -- SMB test server scripts (containers from smb2's consumer test harness)
- Unit tests live in `apps/desktop/test/` (Vitest) -- separate from E2E

## Detailed docs

- `e2e-playwright/CLAUDE.md` -- Playwright test suite (macOS + Linux)
- `e2e-linux/CLAUDE.md` -- Docker infrastructure for Linux E2E
