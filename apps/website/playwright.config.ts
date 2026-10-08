import { defineConfig, devices } from '@playwright/test'

export default defineConfig({
  testDir: './e2e',
  // VISUAL_FULL=1 (see e2e/visual.spec.ts) captures throwaway full-page shots for diffing an Astro
  // major upgrade. They go to their own gitignored directory rather than a `full/` subpath in the
  // snapshot name: Playwright flattens a `/` in the name into a `-`, so `full/home.png` would land
  // beside the committed baselines as `full-home-...png` and get committed with them.
  ...(process.env.VISUAL_FULL
    ? { snapshotPathTemplate: '{testDir}/visual-full-snapshots/{arg}{-projectName}{-snapshotSuffix}{ext}' }
    : {}),
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? 'github' : 'html',
  use: {
    baseURL: 'http://localhost:18473',
    trace: 'on-first-retry',
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    // ❗ The bin shim, never `pnpm exec serve`: pnpm 11.27 starts an exec'd child in its own process
    // group, so Playwright's stop signal misses `serve`, which keeps the output pipe open and hangs
    // the run after the last test until CI's timeout (verified on pnpm 11.27.1, macOS and the CI
    // container, 2026-09-24). The shim `exec`s node, so `serve` stays the process Playwright started.
    command: './node_modules/.bin/serve dist -l 18473',
    url: 'http://localhost:18473',
    // ❗ Never reuse a server already on the port: anything listening there (a stray `astro dev`, a
    // desktop E2E app) would get tested in place of `dist`, and the run fails on a dozen unrelated
    // assertions. With this off, a taken port stops the run at startup and names the port. `serve`
    // starts in under a second, so reuse saves nothing.
    reuseExistingServer: false,
    timeout: 120000,
  },
})
