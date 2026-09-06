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
    command: 'pnpm exec serve dist -l 18473',
    url: 'http://localhost:18473',
    reuseExistingServer: !process.env.CI,
    timeout: 120000,
  },
})
