/**
 * Playwright global setup: creates the shared fixture directory tree.
 *
 * Runs once before all tests. Sets `CMDR_E2E_START_PATH` so the app
 * and test helpers know where to find the fixture files.
 */

import { createFixtures } from '../e2e-shared/fixtures.js'

export default function globalSetup(): void {
    const fixtureRoot = createFixtures()
    process.env.CMDR_E2E_START_PATH = fixtureRoot
}
