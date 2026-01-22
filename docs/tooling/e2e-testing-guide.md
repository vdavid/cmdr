# E2E testing guide

This guide explains how to run end-to-end tests for the Cmdr desktop application.

## Overview

Cmdr uses two E2E testing approaches:

1. **Smoke tests** (Playwright): Test basic UI rendering in a browser (Chromium/WebKit). Works on macOS and Linux.
2. **Linux E2E tests** (WebDriverIO + tauri-driver): Test the actual Tauri application with full backend integration.

### Why separate test suites?

- **Smoke tests**: Run in a browser, so Tauri IPC is unavailable. Only tests UI structure and basic interactions.
- **Linux E2E tests**: Run against the real Tauri app via tauri-driver, enabling full file operation testing.

### Why Linux only for Tauri E2E tests?

macOS uses WKWebView which has **no WebDriver implementation**, so we can't run WebDriver-based E2E tests there.
Linux uses WebKitGTK which has WebKitWebDriver, making it the only platform where tauri-driver works.

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

### Run Linux E2E tests (Docker - recommended)

```bash
cd apps/desktop
pnpm test:e2e:linux
```

This will:
1. Build the Docker image if needed (first run only)
2. Build the Tauri app for Linux inside Docker (cached between runs)
3. Run WebDriverIO tests against the actual Tauri app using tauri-driver

Options:
```bash
# Force rebuild the Docker image
pnpm test:e2e:linux:build

# Get an interactive shell in the container for debugging
pnpm test:e2e:linux:shell

# Clean the Linux build cache (forces full rebuild)
./scripts/e2e-linux.sh --clean
```

### Troubleshooting

When adding a new feature (+tests) and the new tests start oddly failing, clean the build cache!
`./scripts/e2e-linux.sh --clean` is your friend!

### Run Linux E2E tests (native Linux)

If you're on Linux with Tauri prerequisites installed:

```bash
cd apps/desktop
pnpm tauri build --no-bundle
pnpm test:e2e:linux:native
```

## Test files

| File                                  | Description                                    |
|---------------------------------------|------------------------------------------------|
| `test/e2e-smoke/smoke.test.ts`        | Playwright tests for basic UI (browser-based)  |
| `test/e2e-linux/app.spec.ts`          | WebDriverIO tests for Tauri app (Linux)        |
| `test/e2e-linux/wdio.conf.ts`         | WebDriverIO configuration                      |
| `test/e2e-linux/docker/Dockerfile`    | Docker image for Linux E2E tests               |
| `test/e2e-linux/docker/entrypoint.sh` | Xvfb/dbus setup for headless GUI               |
| `scripts/e2e-linux.sh`                | Main script for Docker-based E2E tests         |
| `playwright.config.ts`                | Playwright configuration                       |

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
        const dirEntry = await browser.$('.file-entry:has(.size-dir)')
        await dirEntry.click()
        await browser.keys('Enter')
        // ... verify navigation occurred
    })
})
```

**Capabilities**: Full file operations, keyboard navigation, copy dialog, etc.

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

### Build caching

The script uses Docker volumes to cache:
- Cargo registry (`cmdr-cargo-cache`)
- Target directory (`cmdr-target-cache`)
- Root node modules (`cmdr-root-node-modules-cache`)
- Desktop node modules (`cmdr-desktop-node-modules-cache`)

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

## Future improvements

### macOS E2E testing

macOS doesn't have a WebDriver for WKWebView. Options to explore:

1. **CrabNebula Cloud** - Paid service with their own testing infrastructure
2. **Tauri's native testing** - `tauri test` is experimental but might mature
3. **Custom IPC-based testing** - Use Tauri commands to expose test hooks

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
3. Check for error messages (GTK warnings, missing libraries, etc.)

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
