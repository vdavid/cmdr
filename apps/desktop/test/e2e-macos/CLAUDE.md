# macOS E2E tests (CrabNebula)

WebDriverIO E2E tests for Cmdr on macOS, using CrabNebula's WebDriver bridge for WKWebView.

## Architecture

```
WebDriverIO ──HTTP:4444──> tauri-driver (CN fork) ──HTTP:3000──> test-runner-backend ──HTTP:{dynamic}──> tauri-plugin-automation (in-app)
```

- `tauri-plugin-automation` (Rust crate, `0.1.x`): Starts an HTTP server inside the app on a random port. Feature-gated
  behind `automation` in `Cargo.toml` — never compiled into normal dev or release builds.
- `@crabnebula/test-runner-backend` (`0.2.x`): CrabNebula's macOS WebDriver bridge. Connects to the automation plugin.
- `@crabnebula/tauri-driver` (`2.0.x`): Fork of the official tauri-driver that proxies WebDriver requests through the
  test-runner-backend.
- Requires `CN_API_KEY` env var (CrabNebula account, currently in beta/free).

## Running

Put your `CN_API_KEY` in `apps/desktop/.env` (gitignored, see `.env.example` for the template). Then:

```bash
cd apps/desktop
pnpm test:e2e:macos:build                                  # one-time: builds with --features automation
export $(grep -v '^#' .env | xargs) && pnpm test:e2e:macos # runs tests
```

## CrabNebula WebDriver quirks

These are critical for writing tests. Without these workarounds, tests will silently fail.

### 1. `browser.keys()` doesn't deliver key events

Standard WebDriver key input doesn't reach the app. The W3C Actions API (`browser.action('key')`) also fails. **Use
JavaScript `dispatchEvent` instead:**

```typescript
async function dispatchKey(key: string): Promise<void> {
    await browser.execute((k: string) => {
        const target = document.querySelector('.dual-pane-explorer') ?? document.activeElement ?? document.body
        target.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }))
        target.dispatchEvent(new KeyboardEvent('keyup', { key: k, bubbles: true, cancelable: true }))
    }, key)
    await browser.pause(300)
}
```

### 2. Element references in `browser.execute()` args don't serialize

Passing WebDriverIO elements as args to `browser.execute()` results in `undefined` inside the callback. **Use
`document.querySelector()` inside the callback instead:**

```typescript
// BAD — el is undefined inside execute
await browser.execute((el) => el.click(), someElement)

// GOOD — query inside execute
await browser.execute(() => {
    const el = document.querySelector('.my-selector')
    el?.click()
})
```

### 3. Binary path differs with `--target`

When building with `--target aarch64-apple-darwin` (which the build script does to avoid the tauri-wrapper injecting
`--target universal-apple-darwin`), the output goes to `target/<arch>/debug/Cmdr` at the workspace root, NOT
`src-tauri/target/debug/Cmdr`. The `wdio.conf.ts` detects the native arch via `rustc -vV` and resolves the path
automatically.

## Known issues

- **JS dispatch vs native keys**: The `dispatchEvent` workaround means we're not testing the real OS keyboard input
  path. If CrabNebula fixes native key delivery, switch back to `browser.keys()`.
- **Click with offset untested**: `element.click({x: 10, y: 10})` was broken in earlier versions (actions API error). We
  haven't verified whether it's fixed — test before relying on offset clicks.
- **Intentionally local-only (not in CI)**: GitHub Actions charges macOS minutes at 10x, which would
  eat through the free plan's 2,000 minutes/month quickly. Linux E2E runs in CI via Docker (free).
  macOS E2E is a local pre-release check. Requires `CN_API_KEY` env var.

## Fixture system

Tests use a shared fixture helper (`../e2e-shared/fixtures.ts`) that creates a temp directory tree at
`/tmp/cmdr-e2e-<timestamp>/` with `left/` (text files, sub-dir, hidden file, bulk .dat files) and `right/` (empty).

The `CMDR_E2E_START_PATH` env var tells the app where to open. Fixtures are fully recreated before each test via
`recreateFixtures()` in the `beforeTest` hook so tests don't affect each other.

## Files

| File                       | Purpose                                                                             |
| -------------------------- | ----------------------------------------------------------------------------------- |
| `wdio.conf.ts`             | WebDriverIO config: spawns test-runner-backend + tauri-driver, validates CN_API_KEY |
| `app.spec.ts`              | 10 tests: rendering, keyboard nav, mouse interaction, dialogs                       |
| `file-operations.spec.ts`  | 5 tests: APFS copy/move, volume list, navigate into dir, navigate to parent         |
| `tsconfig.json`            | TypeScript config for WDIO types                                                    |
| `../../.env.example`       | Template for `CN_API_KEY`                                                           |

## Related

- Shared fixture helper: `test/e2e-shared/fixtures.ts`
- Rust plugin registration: `src-tauri/src/lib.rs` (search for `automation`)
- Cargo feature: `src-tauri/Cargo.toml` `[features]` section
- Full guide: `docs/tooling/e2e-testing-guide.md`
- Linux E2E tests: `test/e2e-linux/` (the workhorse — all platform-independent app logic)
