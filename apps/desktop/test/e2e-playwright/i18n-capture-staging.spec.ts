/**
 * The i18n capture's staging, run inside the routine E2E lane with no camera.
 *
 * `i18n-capture.spec.ts` only runs when someone runs `pnpm i18n:shots`, so a UI
 * change that breaks how it reaches a surface would otherwise stay hidden until the
 * next capture. This spec walks the same `MAIN_PASS_STEPS` in stage-only mode
 * (`isStageOnly`): each surface is staged and its ready state proven, then the run
 * moves on. It never enables the key sink, never takes the front position, and
 * never writes a screenshot or a report.
 *
 * One test per step, so a failure names the step and its assertion names the
 * surfaces. A step that can't stage here says why in its `notStagedInLane`. The
 * license and FDA passes aren't steps at all: each needs an app launched with its
 * own mock env, and a lane shard launches its app once.
 */

import { test, expect } from './fixtures.js'
import { drainOperations, getFixtureRoot } from './helpers.js'
import { restoreFixtureTree } from '../e2e-shared/fixture-manifest.js'
import type { TauriPage } from '@srsholmes/tauri-playwright'
import { setStageOnly } from './i18n-capture-config.js'
import { MAIN_PASS_STEPS, type PassLedger } from './i18n-capture-main-pass.js'

test.describe('i18n capture staging', () => {
  // The capture only runs on macOS, and its staging branches on the platform (the
  // FDA onboarding step, the Shift glyph, the Quick Look hint), so a Linux pass would
  // prove a flow `pnpm i18n:shots` never takes.
  test.skip(process.platform !== 'darwin', 'the i18n screenshot capture runs on macOS only')

  test.beforeAll(() => {
    setStageOnly(true)
  })

  test.afterAll(() => {
    setStageOnly(false)
  })

  // Several steps start real operations or copy into `right/`. Drain first, then
  // restore, in ONE hook: a restore under a live operation deletes its source.
  test.afterEach(async ({ tauriPage }) => {
    await drainOperations(tauriPage as TauriPage)
    restoreFixtureTree(getFixtureRoot())
  })

  for (const step of MAIN_PASS_STEPS) {
    if (step.notStagedInLane !== undefined) continue
    test(`${step.name} still stages every surface`, async ({ tauriPage }) => {
      // A healthy step takes well under a second. A BROKEN one waits out each failing
      // surface's own readiness budget (5–20 s) before naming it, so the test gets
      // room to report every surface instead of dying on the default 15 s timeout.
      test.setTimeout(60000)
      const ledger: PassLedger = { report: {}, failed: [], skipped: [] }
      await step.run(tauriPage as TauriPage, ledger)
      expect(ledger.failed, `surfaces that no longer stage: ${ledger.failed.join(', ')}`).toEqual([])
    })
  }
})
