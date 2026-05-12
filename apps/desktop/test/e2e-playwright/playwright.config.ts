import { defineConfig } from '@playwright/test'

// Shard kind switches which specs this Playwright process runs:
// - "mtp": only mtp.spec.ts + mtp-conflicts.spec.ts (must run alone — single
//   virtual MTP device backing dir is shared across all Tauri instances).
// - "non-mtp": everything except MTP specs.
// - unset / "all": every spec (single-instance / legacy run).
const shardKind = process.env.CMDR_E2E_SHARD_KIND ?? 'all'

const mtpSpecMatch = /mtp(-conflicts)?\.spec\.ts$/
const testMatch = shardKind === 'mtp' ? mtpSpecMatch : '*.spec.ts'
const testIgnore = shardKind === 'non-mtp' ? mtpSpecMatch : undefined

// Per-shard JSON report path keeps parallel Playwright processes from
// overwriting each other's output. Defaults preserve the legacy filename.
const jsonReport = process.env.CMDR_E2E_JSON_REPORT ?? '/tmp/cmdr-e2e-report.json'

// Per-shard output dir avoids collisions when multiple Playwright processes
// (each with its own Tauri instance) run in parallel. Default keeps the
// legacy single-run path.
const outputDir = process.env.CMDR_E2E_OUTPUT_DIR ?? './test-results'

// MTP specs share a single virtual device whose backing dir is hard-coded.
// Under three parallel Tauri instances the wipe+rescan+resume sequence in
// `beforeEach` is occasionally flaky (the watcher's resume window can race
// with the rescan even in single-instance runs — see Step 4 notes). One retry
// catches genuine flakes without masking a real regression; the MTP shard is
// the only one that needs it.
const retries = shardKind === 'mtp' ? 1 : 0

export default defineConfig({
  testDir: '.',
  testMatch,
  testIgnore,
  outputDir,
  fullyParallel: false, // Tests share app state — run sequentially within a shard
  forbidOnly: !!process.env.CI,
  retries,
  workers: 1, // Single worker per Playwright process — one Tauri app instance
  reporter: [['list'], ['json', { outputFile: jsonReport }]],
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
