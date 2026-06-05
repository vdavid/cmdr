import { defineConfig } from '@playwright/test'

// Shard kind switches which specs this Playwright process runs:
// - "mtp": only mtp.spec.ts + mtp-conflicts.spec.ts (must run alone: single
//   virtual MTP device backing dir is shared across all Tauri instances).
// - "non-mtp": everything except MTP specs.
// - unset / "all": every spec (single-instance / legacy run).
const shardKind = process.env.CMDR_E2E_SHARD_KIND ?? 'all'

// Match every MTP spec: `mtp.spec.ts`, `mtp-conflicts.spec.ts`, plus the
// fresh-listing-reuse specs (`mtp-copy-preflight-uses-cache.spec.ts`,
// `mtp-delete-no-double-scan.spec.ts`). Every spec that touches the virtual
// MTP backing dir must live on the dedicated MTP shard — running two MTP
// specs in parallel corrupts the shared fixture root.
const mtpSpecMatch = /mtp(-[a-z-]+)?\.spec\.ts$/
const testMatch = shardKind === 'mtp' ? mtpSpecMatch : '*.spec.ts'
const testIgnore = shardKind === 'non-mtp' ? mtpSpecMatch : undefined

// Per-shard JSON report path keeps parallel Playwright processes from
// overwriting each other's output. Defaults preserve the legacy filename.
const jsonReport = process.env.CMDR_E2E_JSON_REPORT ?? '/tmp/cmdr-e2e-report.json'

// Per-shard output dir avoids collisions when multiple Playwright processes
// (each with its own Tauri instance) run in parallel. Default keeps the
// legacy single-run path.
const outputDir = process.env.CMDR_E2E_OUTPUT_DIR ?? './test-results'

// Step 6b: the MTP shard used to need `retries: 1` because the watcher's
// resume window raced with delayed FSEvents from `recreateMtpFixtures`.
// `resync_virtual_mtp_after_disk_change` (commands/mtp.rs) now drains those
// events while the watcher is still paused, so no shard needs a retry for an
// app-level race.
//
// CI-only retry for load-induced environment flake on the shared Docker VM:
// the Linux lane sets `CI=true` and runs this exact config, so it inherits one
// retry, while local dev stays at zero so a real race surfaces immediately
// instead of being papered over. A retried pass shows as `flaky` in the `list`
// reporter, keeping the signal visible. See docs/testing.md § retries carve-out.
const retries = process.env.CI ? 1 : 0

export default defineConfig({
  testDir: '.',
  testMatch,
  testIgnore,
  outputDir,
  fullyParallel: false, // Tests share app state, run sequentially within a shard
  forbidOnly: !!process.env.CI,
  retries,
  workers: 1, // Single worker per Playwright process, one Tauri app instance
  reporter: [['list'], ['json', { outputFile: jsonReport }]],
  // 15 s per test absorbs load-induced UI-wait jitter on the shared Docker VM and
  // on CI, where a busy host stretches `waitForSelector` / nav waits past the old
  // 8 s budget. Tests that legitimately need longer (axe audits, MTP virtual-device
  // protocol overhead, SMB+Docker latency, drive-index convergence) still call
  // `test.setTimeout` with a reason in the comment. Anything without a justified
  // override should fit comfortably in 15 s.
  timeout: 15000,

  globalSetup: './global-setup.ts',
  globalTeardown: './global-teardown.ts',

  projects: [
    {
      name: 'tauri',
      use: {
        // @ts-expect-error -- custom fixture option from tauri-playwright
        mode: 'tauri',
        // Traces and screenshots are useless in Tauri mode. They capture
        // the blank Playwright browser page, not the real Tauri webview.
        // Native screenshots are captured via CoreGraphics on test failure.
        trace: 'off',
        screenshot: 'off',
      },
    },
  ],
})
