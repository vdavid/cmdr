# Linux E2E Docker infrastructure

Docker setup for running the Playwright E2E tests (`../e2e-playwright/`) on Linux. The test files themselves live in
`e2e-playwright/` and are shared between macOS and Linux -- see `e2e-playwright/CLAUDE.md` for test documentation.

## Architecture

```
e2e-linux.sh
├─ Build Tauri binary in Docker (--features playwright-e2e,virtual-mtp)
├─ Launch Docker container
│   ├─ entrypoint.sh: Xvfb + dbus + optional VNC
│   ├─ Create fixtures, start Tauri app
│   ├─ Wait for /tmp/tauri-playwright.sock
│   └─ Run: npx playwright test --config test/e2e-playwright/playwright.config.ts
└─ Report results
```

## Running

```bash
cd apps/desktop

pnpm test:e2e:linux                    # Full run: build (if needed) + test in Docker
pnpm test:e2e:linux:build              # Force rebuild Docker image (Dockerfile changes only)
pnpm test:e2e:linux:shell              # Interactive shell in container
pnpm test:e2e:linux:vnc                # VNC mode with hot reload (pnpm dev)
```

## Build caching

| Volume                            | Contents                       | Remove to force...                        |
| --------------------------------- | ------------------------------ | ----------------------------------------- |
| `cmdr-cargo-cache`                | Cargo registry + compiled deps | Full crate re-download                    |
| `cmdr-target-cache`               | Compiled Tauri binary          | App recompilation (fast with cargo cache) |
| `cmdr-root-node-modules-cache`    | Root `node_modules/`           | `pnpm install`                            |
| `cmdr-desktop-node-modules-cache` | Desktop `node_modules/`        | `pnpm install`                            |

Most common operation: `docker volume rm cmdr-target-cache` after Rust/Svelte changes or feature flag changes.

## Files

| File                   | Purpose                                                 |
| ---------------------- | ------------------------------------------------------- |
| `docker/Dockerfile`    | Ubuntu 24.04 image with Tauri prereqs, Xvfb, Rust, Node |
| `docker/entrypoint.sh` | Xvfb/dbus/VNC setup for headless GUI                    |

## CI integration

| Check nickname        | What it runs                             | Included by default? |
| --------------------- | ---------------------------------------- | -------------------- |
| `desktop-e2e-linux`   | Playwright E2E in Docker                 | No (slow)            |
| `e2e-linux-typecheck` | TypeScript check on e2e-playwright files | Yes                  |
