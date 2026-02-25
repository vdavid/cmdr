# Linux E2E tests (Docker + tauri-driver)

WebDriverIO E2E tests for Cmdr on Linux, using tauri-driver with WebKitGTK's WebKitWebDriver.

This is the workhorse test suite. All platform-independent app logic lives here: dialog flows, keyboard nav, selection,
view modes, file viewer, settings, command palette, and file operations. macOS tests only cover platform integration
(APFS ops, volume detection).

## Architecture

```
WebDriverIO ──HTTP:4444──> tauri-driver ──> WebKitWebDriver ──> WebKitGTK (in-app)
```

Runs inside a Docker container (Ubuntu 24.04) with Xvfb for headless GUI. The host's source code is mounted in, so the
Tauri app is built inside the container.

## Running

```bash
cd apps/desktop
pnpm test:e2e:linux                # Docker (recommended, works from macOS)
pnpm test:e2e:linux:build          # force rebuild Docker image
pnpm test:e2e:linux:shell          # interactive shell in container for debugging
pnpm test:e2e:linux:native         # native Linux only (requires Tauri prereqs)
./scripts/e2e-linux.sh --clean     # nuke build cache (try this when tests fail oddly)
```

## Fixture system

Tests use a shared fixture helper (`../e2e-shared/fixtures.ts`) that creates a temp directory tree at
`/tmp/cmdr-e2e-<timestamp>/` with `left/` (text files, sub-dir, hidden file, bulk .dat files) and `right/` (empty).

The `CMDR_E2E_START_PATH` env var tells the app where to open. Fixtures are fully recreated before each test via
`recreateFixtures()` in the `beforeTest` hook so tests don't affect each other.

## WebKitGTK WebDriver quirks

These are critical for writing tests. Without these workarounds, tests will silently fail.

### 1. Native clicks fail on non-form elements

WebKitGTK's WebDriver rejects clicks on non-interactive container elements. **Use `jsClick()` (JS `el.click()`)
instead:**

```typescript
async function jsClick(element: WebdriverIO.Element): Promise<void> {
    await browser.execute((el: HTMLElement) => el.click(), element as unknown as HTMLElement)
}
```

### 2. `browser.keys(' ')` doesn't deliver Space

The Space character hits a CharKey/VirtualKey ambiguity in WebKitWebDriver. **Use the W3C Actions API instead:**

```typescript
await browser.action('key').down(' ').pause(50).up(' ').perform()
await browser.releaseActions()
```

## Files

| File                      | Purpose                                                         |
| ------------------------- | --------------------------------------------------------------- |
| `wdio.conf.ts`            | WebDriverIO config: spawns tauri-driver, manages fixtures       |
| `app.spec.ts`             | 14 tests: rendering, keyboard nav, mouse interaction, dialogs   |
| `file-operations.spec.ts` | 8 tests: copy, move, rename, mkdir, view modes, hidden, palette |
| `settings.spec.ts`        | 5 tests: settings panel                                         |
| `viewer.spec.ts`          | 10 tests: file viewer                                           |
| `docker/Dockerfile`       | Ubuntu 24.04 image with Tauri prereqs, Xvfb, Node, Rust         |
| `docker/entrypoint.sh`    | Xvfb/dbus setup for headless GUI                                |
| `tsconfig.json`           | TypeScript config for WDIO types                                |

## Related

- Shared fixture helper: `test/e2e-shared/fixtures.ts`
- Full guide: `docs/tooling/e2e-testing-guide.md`
- macOS E2E tests: `test/e2e-macos/` (platform integration only — APFS ops, volume detection)
- Linux stubs: `src-tauri/src/stubs/` (volumes, network, permissions use stubs on Linux)
