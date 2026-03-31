/**
 * Playwright global teardown: cleans up the fixture directory.
 *
 * Only cleans up fixtures that were created by globalSetup (not pre-existing
 * dirs from an externally launched app).
 */

import { cleanupFixtures } from '../e2e-shared/fixtures.js'

// Set by global-setup.ts when it creates a NEW fixture directory
declare global {
  // eslint-disable-next-line no-var
  var __PLAYWRIGHT_CREATED_FIXTURES: boolean | undefined
}

export default function globalTeardown(): void {
  const fixtureRoot = process.env.CMDR_E2E_START_PATH
  if (fixtureRoot && globalThis.__PLAYWRIGHT_CREATED_FIXTURES) {
    cleanupFixtures(fixtureRoot)
  }
}
