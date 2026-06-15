# Desktop app tests

## Must-knows

- **Never use magic timer waits; probe the actual readiness condition.** A `sleep N` / `setTimeout(N)` as "wait for the
  thing to be ready" is a bug: slow on the happy path (everyone pays N) and flaky on the slow path (load pushes startup
  past N, fails at random with no signal). Active probes exit the instant the condition is true and fail loudly. Applies
  to test code, test-setup scripts (bash, Node, Go), CI helpers, and production code. The only acceptable sub-second
  `sleep` is the poll interval inside a probe loop. Probes by case:
  - "container/daemon listening on a port" → tight loop on `nc -z host port` or bash `/dev/tcp/host/port`, ~100 ms poll,
    ~60 s deadline.
  - "DOM element rendered" → `waitForSelector(...)`.
  - "app state changed" → `expect.poll(() => condition(), { timeout }).toBeTruthy()`. Avoid bare `await pollUntil(...)`:
    it returns `false` on timeout and a discarded return is a silent pass (the `bare-poll` fast-lane check flags it).
  - "Tauri event fired" → `tauriPage.waitForFunction(...)`, or `pollUntil` for Node-side logic.
- **E2E is all Playwright now.** The Playwright suite (`e2e-playwright/`) uses upstream `tauri-plugin-playwright` (Rust,
  crates.io) + `@srsholmes/tauri-playwright` (npm), injecting JS into the Tauri webview via `webview.eval()` and getting
  results over Tauri IPC. The same specs run on macOS (native) and Linux (Docker). Gated behind the `playwright-e2e`
  Cargo feature. `e2e-linux/` holds only the Docker infrastructure (the specs live in `e2e-playwright/` and are shared).
- **All E2E suites share `e2e-shared/fixtures.ts`.** With an instance ID (macOS Playwright, passed by the Go checker)
  fixtures land at `/tmp/cmdr-e2e-fixtures-<instance>-<timestamp>/`; bulk `.dat` files hardlink from a shared cache at
  `/tmp/cmdr-e2e-fixtures-cache/` (built via tmp-dir + atomic-rename so parallel shards don't race). Text files are full
  copies (tests mutate them). Without an instance ID (Linux Docker) it falls back to `/tmp/cmdr-e2e-<timestamp>/` with
  no cache. `CMDR_E2E_START_PATH` tells the app where to open; `recreateFixtures()` runs before tests for isolation.
- **Virtual devices for hardware-free E2E**: MTP via the `virtual-mtp` feature (helpers in `e2e-shared/mcp-client.ts`,
  `mtp-fixtures.ts`); SMB via the `smb-e2e` feature pointing at Docker SMB containers (`e2e-shared/smb-fixtures.ts`).

## Filesystem fixtures layout

```
left/                         right/  (empty)
  file-a.txt, file-b.txt
  sub-dir/nested-file.txt
  .hidden-file
  bulk/  (3 x 50 MB + 20 x 1 MB .dat files)
```

## Other test infrastructure

- Unit tests live in `apps/desktop/test/` (Vitest), separate from E2E.
- `smb-servers/`: SMB test server scripts (containers from smb2's consumer test harness).

## Detailed docs

- `e2e-playwright/CLAUDE.md`: Playwright suite (macOS + Linux).
- `e2e-linux/CLAUDE.md`: Docker infrastructure for Linux E2E.

Full details (the `sleep 3` flake case study): [DETAILS.md](DETAILS.md).
