# Desktop app tests

## Testing principles

### ❌ Never use magic timer waits: always probe the actual readiness condition

A `sleep N` or `setTimeout(N)` call as a "wait for the thing to be ready" is a bug. It's **slow on the happy path**
(everyone pays N every time, even when the thing was ready in 50 ms) and **flaky on the slow path** (when load pushes
startup past N, the test fails at random with no signal about what wasn't ready). Active probes are faster AND steadier
They exit the instant the condition becomes true and fail loudly with a clear "X didn't become ready in 60 s" message.

Always write the probe instead:

- "container/daemon listening on a port" → tight loop on `nc -z host port` or bash `/dev/tcp/host/port` with ~100 ms
  poll and ~60 s deadline.
- "DOM element rendered" → `waitForSelector(...)`. Never bare `sleep`.
- "app state changed" → `expect.poll(() => condition(), { timeout }).toBeTruthy()` (Playwright's built-in: the wait IS
  the assertion). Avoid bare `await pollUntil(...)` — it returns `false` on timeout and a discarded return = silent test
  pass; the `bare-poll` check (fast lane) flags it. See `docs/testing.md` § "Bare `await pollUntil(...)` in E2E specs".
- "Tauri event fired" → `tauriPage.waitForFunction(...)` for simple JS, `pollUntil` for anything needing Node-side
  logic.

This applies to test code, test-setup scripts (bash, Node, Go), CI helpers, and production code. The only acceptable
sub-second `sleep` is the poll interval **inside** a probe loop (`while ! ready; do sleep 0.1; done`).

If you catch yourself writing `sleep N`, stop and ask "what am I actually waiting for?", then probe that.

**Case study (2026-05-14):** `smb-servers/start.sh` had `sleep 3` after `docker compose up -d`. The `guest` container
bound port 445 fast enough; `auth`, `50shares`, `unicode` legitimately needed >3 s under load to finish user creation /
share materialisation. E2E runs flaked with `Cannot reach smb-consumer-X` because smbd hadn't bound the port yet when
tests started connecting. Replaced with per-service TCP probes on the published `445` port. Now exits in ~100 ms on a
warm machine, in 5-10 s on a cold one, and gives a deterministic `did not accept TCP within 60s` error if a container is
genuinely broken.

## E2E test suites

| Suite                              | Tech                      | Runs on                           | What it tests                                                          |
| ---------------------------------- | ------------------------- | --------------------------------- | ---------------------------------------------------------------------- |
| **Playwright** (`e2e-playwright/`) | Playwright + tauri-plugin | macOS (native) and Linux (Docker) | Full app: dialogs, keyboard nav, file ops, settings, viewer, a11y, MTP |

**Playwright suite** uses upstream `tauri-plugin-playwright` (Rust, crates.io) and `@srsholmes/tauri-playwright` (npm),
which inject JS directly into the Tauri webview via `webview.eval()` and receive results via Tauri IPC. Same tests work
on both macOS and Linux. Gated behind the `playwright-e2e` Cargo feature.

**Linux Docker infrastructure** lives in `e2e-linux/docker/` (Dockerfile + entrypoint). The `e2e-linux.sh` script builds
the Tauri binary with `--features playwright-e2e,virtual-mtp` inside Docker, launches it, and runs the Playwright tests.

## Shared fixture system

All E2E suites share `e2e-shared/fixtures.ts`. When `createFixtures(instanceId)` is called with an instance ID (macOS
Playwright path, passed by the Go checker), fixtures land at `/tmp/cmdr-e2e-fixtures-<instance>-<timestamp>/`. Bulk
`.dat` files are hardlinked from a shared cache at `/tmp/cmdr-e2e-fixtures-cache/`; the cache is built on first use via
a tmp-dir + atomic-rename protocol so parallel shards never race. Text files are full copies because tests mutate them.
When `createFixtures()` is called without an instance ID (Linux Docker path), it falls back to the legacy
`/tmp/cmdr-e2e-<timestamp>/` root with no cache (single shard, low benefit).

MTP E2E tests use a virtual MTP device (pure Rust, no USB needed) via the `virtual-mtp` feature flag, with helpers in
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
