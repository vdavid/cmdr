/**
 * Playwright global teardown: cleans up the fixture directory.
 */

import { cleanupFixtures } from '../e2e-shared/fixtures.js'

export default function globalTeardown(): void {
    const fixtureRoot = process.env.CMDR_E2E_START_PATH
    if (fixtureRoot) {
        cleanupFixtures(fixtureRoot)
    }
}
