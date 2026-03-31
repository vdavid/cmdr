import { defineConfig } from '@playwright/test'

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.ts',
  fullyParallel: false, // Tests share app state — run sequentially
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: 1, // Single worker — one Tauri app instance
  reporter: [['html', { open: 'never' }], ['list']],
  timeout: 30000,

  globalSetup: './global-setup.ts',
  globalTeardown: './global-teardown.ts',

  projects: [
    {
      name: 'tauri',
      use: {
        // @ts-expect-error — custom fixture option from tauri-playwright
        mode: 'tauri',
        // Traces and screenshots are useless in Tauri mode — they capture
        // the blank Playwright browser page, not the real Tauri webview.
        // Native screenshots are captured via CoreGraphics on test failure.
        trace: 'off',
        screenshot: 'off',
      },
    },
  ],
})
