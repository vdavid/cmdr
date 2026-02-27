# E2E testing guide

This guide explains how to run end-to-end tests for the Cmdr desktop application.

## Overview

Cmdr uses three E2E testing approaches:

1. **Smoke tests** (Playwright): Test basic UI rendering in a browser (Chromium/WebKit). Works on macOS and Linux.
2. **Linux E2E tests** (WebDriverIO + tauri-driver): Test the actual Tauri application with full backend integration.
3. **macOS E2E tests** (WebDriverIO + CrabNebula): Test the actual Tauri application on macOS via CrabNebula's WKWebView WebDriver bridge.

### Why separate test suites?

- **Smoke tests**: Run in a browser, so Tauri IPC is unavailable. Only tests UI structure and basic interactions.
- **Linux E2E tests**: Run against the real Tauri app via tauri-driver, enabling full file operation testing.

### Why separate platforms for Tauri E2E tests?

- **Linux (Docker)**: The workhorse. Uses WebKitGTK with WebKitWebDriver, so standard tauri-driver
  works natively. Runs all platform-independent app logic: dialog flows, keyboard nav, selection,
  view modes, file viewer, settings, command palette, and file operations (copy, move, rename,
  create folder).
- **macOS (CrabNebula)**: Platform integration only. Uses WKWebView, which has no Apple-provided
  WebDriver, so we use CrabNebula's commercial bridge (requires `CN_API_KEY`). Tests real APFS
  file operations, volume detection, and WKWebView rendering. Currently in beta (free), will
  become paid.

### Key dependencies for tauri-driver

tauri-driver requires the full Tauri development prerequisites, not just runtime libraries:

```bash
# From https://tauri.app/start/prerequisites/#linux
sudo apt install libwebkit2gtk-4.1-dev libxdo-dev build-essential \
    curl wget file libssl-dev libayatana-appindicator3-dev librsvg2-dev
```

The `libxdo-dev` package (for X11 automation) is particularly important for WebDriver input simulation.

## Quick start

### Run smoke tests (browser-based, works on macOS)

```bash
cd apps/desktop
pnpm test:e2e:smoke
```

Tests basic UI rendering in Chromium/WebKit browsers. These tests verify that the UI structure
renders correctly but cannot test file operations (which require Tauri backend).

### Run Linux E2E tests (Docker)

The compiled Tauri binary and all build artifacts are cached in Docker volumes. The script skips
the build step when a cached binary exists, so what you need to do depends on what changed:

```bash
cd apps/desktop

# Test-only changes (.ts test files, wdio.conf.ts):
# Source is mounted from host — just re-run, no rebuild needed.
pnpm test:e2e:linux

# Rust or Svelte code changes:
# Remove the target volume (keeps cargo registry cache, so recompilation is fast).
docker volume rm cmdr-target-cache && pnpm test:e2e:linux

# Nuclear option — nuke everything (cargo cache, target, node_modules):
# Use when things are inexplicably broken or after dependency changes.
./scripts/e2e-linux.sh --clean && pnpm test:e2e:linux
```

Other options:
```bash
pnpm test:e2e:linux:build          # force rebuild Docker IMAGE (Dockerfile changes only)
pnpm test:e2e:linux:shell          # interactive shell in container for debugging
```

### Run Linux E2E tests (native Linux)

If you're on Linux with Tauri prerequisites installed:

```bash
cd apps/desktop
pnpm tauri build --no-bundle
pnpm test:e2e:linux:native
```

## Test files

| File                                      | Description                                                         |
|-------------------------------------------|---------------------------------------------------------------------|
| `test/e2e-smoke/smoke.test.ts`            | Playwright tests for basic UI (browser-based)                       |
| `test/e2e-shared/fixtures.ts`             | Shared fixture helper (creates/recreates the test directory tree)   |
| `test/e2e-linux/app.spec.ts`              | WebDriverIO tests: rendering, keyboard nav, dialogs (Linux)         |
| `test/e2e-linux/file-operations.spec.ts`  | WebDriverIO tests: copy, move, rename, mkdir, view modes (Linux)    |
| `test/e2e-linux/settings.spec.ts`         | WebDriverIO tests: settings panel (Linux)                           |
| `test/e2e-linux/viewer.spec.ts`           | WebDriverIO tests: file viewer (Linux)                              |
| `test/e2e-linux/wdio.conf.ts`             | WebDriverIO configuration (Linux)                                   |
| `test/e2e-linux/docker/Dockerfile`         | Docker image for Linux E2E tests                                    |
| `test/e2e-linux/docker/entrypoint.sh`      | Xvfb/dbus setup for headless GUI                                    |
| `test/e2e-macos/app.spec.ts`              | WebDriverIO tests: rendering, keyboard nav, dialogs (macOS)         |
| `test/e2e-macos/file-operations.spec.ts`  | WebDriverIO tests: APFS copy/move, volumes, navigation (macOS)      |
| `test/e2e-macos/wdio.conf.ts`             | WebDriverIO configuration (macOS, CrabNebula)                       |
| `.env.example`                            | Template for CN_API_KEY                                             |
| `scripts/e2e-linux.sh`                    | Main script for Docker-based E2E tests (+ VNC mode)                 |
| `playwright.config.ts`                    | Playwright configuration                                            |

## Fixture system

File operation tests need a known directory tree to work with. The shared helper at
`test/e2e-shared/fixtures.ts` creates a timestamped `/tmp/cmdr-e2e-<timestamp>/` directory
with this layout:

```
left/
  file-a.txt, file-b.txt     (1 KB text files)
  sub-dir/nested-file.txt
  .hidden-file
  bulk/                       (3 x 50 MB + 20 x 1 MB .dat files)
right/                        (empty)
```

Both wdio configs (`e2e-linux/wdio.conf.ts` and `e2e-macos/wdio.conf.ts`) import and call
`createFixtures()` in `onPrepare`, which sets the `CMDR_E2E_START_PATH` env var. The Rust
backend reads this var (always compiled in, no feature flag needed) to open the left and right
panes at `left/` and `right/` on launch.

Fixtures are fully recreated before each test via `recreateFixtures()` in the `beforeTest` hook,
so tests don't depend on each other's side effects. Cleanup happens in `onComplete`.

## Writing tests

### Smoke tests (Playwright)

Smoke tests in `test/e2e-smoke/smoke.test.ts` verify basic UI rendering:

```typescript
import { test, expect } from '@playwright/test'

test('app loads successfully', async ({ page }) => {
    await page.goto('/')
    await expect(page.locator('.dual-pane-explorer')).toBeVisible()
})
```

**Limitations**: Cannot test file operations, navigation, or anything requiring Tauri backend.

### Linux E2E tests (WebDriverIO)

Tests in `test/e2e-linux/app.spec.ts` can test full application functionality:

```typescript
describe('Navigation', () => {
    it('should navigate into directories with Enter', async () => {
        const dirEntry = browser.$('.file-entry:has(.size-dir)')
        await dirEntry.click()
        await browser.keys('Enter')
        // ... verify navigation occurred
    })
})
```

**Capabilities**: Full file operations, keyboard navigation, copy dialog, and more.

## Docker environment

The Docker container (`test/e2e-linux/docker/Dockerfile`) includes:

- Ubuntu 24.04 base
- WebKitGTK runtime libraries + development packages
- X11 libraries for GTK
- Xvfb (virtual framebuffer)
- dbus-x11 (required for WebKitGTK)
- Node.js + pnpm
- Rust toolchain + tauri-driver

### Interactive debugging

To get a shell inside the container:

```bash
pnpm test:e2e:linux:shell
```

Inside the container, you can:
- Run the app manually: `$TAURI_BINARY`
- Check the display: `echo $DISPLAY`
- Inspect the environment

### Watching E2E tests live via VNC

You can watch the tests run in real time by connecting to the container's virtual display via VNC.
This is the best way to debug test failures — you see exactly what the test sees.

1. **Start the interactive shell:**
   ```bash
   cd apps/desktop
   pnpm test:e2e:linux:shell
   ```

2. **Inside the container**, start a VNC server on the Xvfb display:
   ```bash
   x11vnc -display :99 -forever -nopw -rfbport 5900 -passwd "aaaa" &
   ```
   (macOS Screen Sharing requires a non-empty password.)

3. **On your Mac**, open Finder and press **Cmd+K** (or menu: Go → Connect to Server). Type:
   ```
   vnc://localhost:5900
   ```
   Hit Connect, enter the password (`aaaa`), and you'll see the container's virtual display.

4. **Inside the container**, run the tests (all or a specific spec):
   ```bash
   # All file operation tests
   pnpm test:e2e:linux:native -- --spec test/e2e-linux/file-operations.spec.ts

   # All E2E tests
   pnpm test:e2e:linux:native
   ```

You'll see the Tauri app launch and the WebDriverIO test interact with it in real time.

### VNC mode (visual debugging with hot reload)

VNC mode runs `pnpm dev` inside the Docker container with a VNC server, so you can see and
interact with the Cmdr GUI in a browser while editing code on macOS:

Honestly, it's a bit weird, the VM feels better, but this is a small change so good to have this as a quick backup.

```bash
cd apps/desktop
pnpm test:e2e:linux:vnc
```

Once it starts, open the URL printed in the terminal (http://localhost:6090/vnc.html?autoconnect=true).
You can also connect with a native VNC client (macOS Screen Sharing) at `vnc://localhost:5990`.

How it works:
- The container runs Xvfb + x11vnc + noVNC, forwarding the virtual display to your browser
- `pnpm dev` starts Vite + Tauri inside the container
- Source code is mounted from your host, so `.svelte`/`.ts` edits trigger Vite HMR (~1–3s reload)
- Rust changes require restarting (`Ctrl+C` + re-run)
- Terminal streams all Rust/Vite logs

This is useful for debugging issues specific to the Linux/WebKitGTK environment, like keyboard
events or GTK focus behavior that differ from macOS.

### Build caching

The script uses Docker volumes to cache:

| Volume | Contents | Remove to force... |
|---|---|---|
| `cmdr-cargo-cache` | Cargo registry + compiled deps | Full crate re-download |
| `cmdr-target-cache` | Compiled Tauri binary | App recompilation (fast with cargo cache) |
| `cmdr-root-node-modules-cache` | Root `node_modules/` | `pnpm install` |
| `cmdr-desktop-node-modules-cache` | Desktop `node_modules/` | `pnpm install` |

Most common operation: `docker volume rm cmdr-target-cache` after Rust/Svelte changes. This
preserves the cargo registry so recompilation only takes a few minutes instead of 10+.

`./scripts/e2e-linux.sh --clean` removes all four volumes at once.

**Why two node_modules volumes?** The monorepo has node_modules at both the root and `apps/desktop/`.
Both must be Docker volumes to prevent Linux binaries from contaminating the host's node_modules
(which would break macOS smoke tests).

To clear caches: `./scripts/e2e-linux.sh --clean`

## CI integration

The check script (`./scripts/check.sh`) includes:

1. **`rust-tests-linux`**: Runs `cargo test` in Docker (unit tests only, faster)
2. **`desktop-e2e`**: Runs Playwright smoke tests locally

The Docker E2E tests (`pnpm test:e2e:linux`) are not currently in the check script because they're slow.
You can run them manually before releases.

## macOS E2E tests (CrabNebula) — local-only

Uses CrabNebula's WebDriver bridge for WKWebView on macOS. Requires a `CN_API_KEY`.

**These tests are intentionally local-only** (not in CI). GitHub Actions charges macOS runner minutes
at 10x the normal rate — a single ~15-min run costs 150 billing minutes, so you'd get roughly 10 runs
before hitting the free plan's 2,000 minutes/month limit. Linux E2E runs in CI via Docker (free).
macOS E2E is a local pre-release check.

### Setup

1. Copy `apps/desktop/.env.example` to `apps/desktop/.env` and fill in your CrabNebula API key.

2. Build the app with the automation plugin:
   ```bash
   cd apps/desktop
   pnpm test:e2e:macos:build
   ```

3. Run the tests (the config auto-loads `.env`):
   ```bash
   pnpm test:e2e:macos
   ```

### How it works

- `tauri-plugin-automation` (Rust crate, feature-gated behind `automation`) enables WKWebView
  automation in debug builds.
- CrabNebula's `test-runner-backend` is the macOS WebDriver bridge (runs on port 3000).
- CrabNebula's `tauri-driver` fork proxies WebDriver requests (runs on port 4444).
- WebDriverIO connects to tauri-driver like it would any browser driver.

### Notes

- The `automation` feature is only for E2E testing, never for production or normal dev builds.
- macOS tests focus on platform integration (APFS file ops, volume detection). Platform-independent
  app logic lives in the Linux test suite.
- Delete (F8) is intentionally skipped on both platforms since it's not yet implemented.

## Future improvements

### Windows E2E testing

Windows can use tauri-driver with Edge WebDriver. To add:
1. Create `test/e2e-windows/` directory structure
2. Set up CI with Windows runners
3. Configure WebDriverIO for Windows

## Linux stubs

Since Cmdr is macOS-only, the Linux build uses stub implementations for:

- **Volumes**: Returns root `/` and common Linux directories
- **Network**: Returns empty (no Bonjour/SMB support)
- **Permissions**: Always returns "has access"

These stubs are in `src-tauri/src/stubs/` and are compiled only on non-macOS platforms.

## Troubleshooting

### Docker E2E test fails with "App crashed"

1. Get a shell in the container: `pnpm test:e2e:linux:shell`
2. Run the app manually: `$TAURI_BINARY`
3. Check for error messages (GTK warnings, missing libraries)

### Docker build is slow

The first build compiles Rust from scratch. Subsequent builds use cached volumes.
If builds are still slow, ensure Docker has enough resources (4GB+ RAM recommended).

### "Docker not running"

Start Docker Desktop or the Docker daemon:
```bash
# macOS
open -a Docker

# Linux
sudo systemctl start docker
```

### Smoke tests fail with Tauri errors

This is expected - smoke tests run in a browser without Tauri backend. Tests that require
file operations will be skipped automatically. Use `pnpm test:e2e:linux` for full testing.

### "Failed to initialize GTK"

This means the DISPLAY environment variable isn't set. The entrypoint.sh should handle this,
but you can verify:
```bash
pnpm test:e2e:linux:shell
echo $DISPLAY  # Should show :99
```

## Resources

- [Tauri WebDriver docs](https://v2.tauri.app/develop/tests/webdriver/)
- [Playwright docs](https://playwright.dev/docs/intro)
- [WebDriverIO docs](https://webdriver.io/docs/gettingstarted)
- [CrabNebula testing](https://crabnebula.dev/)
