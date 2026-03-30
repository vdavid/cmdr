/**
 * Playwright global setup: creates the shared fixture directory tree.
 *
 * Runs once before all tests. Sets `CMDR_E2E_START_PATH` so the app
 * and test helpers know where to find the fixture files.
 *
 * If CMDR_E2E_START_PATH is already set (app started externally with
 * its own fixture dir), we recreate the text files in that existing
 * dir to ensure a clean state but don't create a new directory.
 */

import fs from 'fs'
import { createFixtures, recreateFixtures } from '../e2e-shared/fixtures.js'

export default function globalSetup(): void {
    const existingRoot = process.env.CMDR_E2E_START_PATH
    if (existingRoot && fs.existsSync(existingRoot)) {
        // App already running with this fixture dir — refresh text files
        recreateFixtures(existingRoot)
        return
    }

    const fixtureRoot = createFixtures()
    process.env.CMDR_E2E_START_PATH = fixtureRoot
    globalThis.__PLAYWRIGHT_CREATED_FIXTURES = true
}
